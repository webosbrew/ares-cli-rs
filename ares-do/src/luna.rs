//! What a luna reply means to `ares-do`.
//!
//! The call itself belongs to `ares-connection-lib`; this is only the layer
//! above it — every service answers with `returnValue`, and turning "the call
//! happened" into "the call did what was asked" happens here.

use std::fmt::{Display, Formatter};
use std::time::Duration;

use ares_connection_lib::luna::{Luna, LunaOptions};
use libssh_rs::Session;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::error::DoError;
use crate::output::Reporter;

/// An `errorCode`, which is not consistently a number.
///
/// `com.webos.service.tv.capture` answers a failed VIDEO capture with
/// `"errorCode":"CAPTURE_ERROR_09"`. Typing this as `i32` turned that perfectly
/// good error reply into "the reply was not JSON", which sent the reader
/// looking in entirely the wrong place.
#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub(crate) enum ErrorCode {
    Number(i64),
    Text(String),
}

impl Display for ErrorCode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorCode::Number(n) => write!(f, "{n}"),
            ErrorCode::Text(s) => write!(f, "{s}"),
        }
    }
}

/// The part of a reply every service sends.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LunaReply {
    #[serde(default)]
    pub return_value: bool,
    pub error_code: Option<ErrorCode>,
    pub error_text: Option<String>,
}

impl LunaReply {
    /// Turn a negative reply into the error it describes.
    ///
    /// # Errors
    ///
    /// [`DoError::LunaFailed`] when `returnValue` is false.
    pub(crate) fn check(&self, uri: &str) -> Result<(), DoError> {
        if self.return_value {
            return Ok(());
        }
        Err(DoError::LunaFailed {
            uri: uri.to_string(),
            code: self.error_code.as_ref().map(ToString::to_string),
            text: self
                .error_text
                .clone()
                .unwrap_or_else(|| String::from("no reason given")),
        })
    }
}

/// Everything `ares-do` drives is on the private bus, so that is the default
/// here rather than a parameter every caller has to remember.
pub(crate) fn options(public: bool, timeout: Option<Duration>) -> LunaOptions {
    LunaOptions::private().public(public).timeout(timeout)
}

/// Call a private-bus method and hand back the whole reply.
///
/// # Errors
///
/// Whatever the call failed with. A negative reply is *not* an error here —
/// the caller decides what it means.
pub(crate) fn raw<P: Serialize, R: DeserializeOwned>(
    session: &Session,
    uri: &str,
    payload: &P,
    timeout: Option<Duration>,
    reporter: &Reporter,
) -> Result<R, DoError> {
    with(session, uri, payload, options(false, timeout), reporter)
}

/// Call with the options spelled out, for the `luna` subcommand.
///
/// # Errors
///
/// Whatever the call failed with.
pub(crate) fn with<P: Serialize, R: DeserializeOwned>(
    session: &Session,
    uri: &str,
    payload: &P,
    options: LunaOptions,
    reporter: &Reporter,
) -> Result<R, DoError> {
    if let Ok(rendered) = serde_json::to_string(payload) {
        let program = if options.public {
            "luna-send-pub"
        } else {
            "luna-send"
        };
        reporter.trace(&format!("{program} -n 1 {uri} {rendered}"));
    }
    session
        .call_with(uri, payload, options)
        .map_err(|source| DoError::Luna {
            uri: uri.to_string(),
            source,
        })
}

/// Call a private-bus method and insist it succeeded.
///
/// # Errors
///
/// Whatever the call failed with, or [`DoError::LunaFailed`] when the service
/// answered `returnValue: false`.
pub(crate) fn call<P: Serialize>(
    session: &Session,
    uri: &str,
    payload: &P,
    timeout: Option<Duration>,
    reporter: &Reporter,
) -> Result<(), DoError> {
    let reply: LunaReply = raw(session, uri, payload, timeout, reporter)?;
    reply.check(uri)
}

/// True when a failure means "try the other service", rather than "give up".
///
/// Deliberately broad: whether a missing service shows up as a non-zero exit,
/// an unparseable reply or a plain `returnValue: false` varies by build, so
/// the screenshot fallback retries on any of them rather than matching prose.
/// A timeout is excluded — waiting again for something that already hung is
/// just waiting twice.
pub(crate) fn is_worth_retrying_elsewhere(error: &DoError) -> bool {
    match error {
        DoError::LunaFailed { .. } => true,
        DoError::Luna { source, .. } => {
            !matches!(source, ares_connection_lib::luna::LunaError::Timeout { .. })
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::LunaReply;

    fn reply(json: &str) -> LunaReply {
        serde_json::from_str(json).expect("should parse")
    }

    #[test]
    fn a_numeric_error_code_parses() {
        let r = reply(r#"{"returnValue":false,"errorCode":-1,"errorText":"nope"}"#);
        let e = r.check("luna://x/y").unwrap_err().to_string();
        assert!(e.contains("nope"), "{e}");
        assert!(e.contains("(-1)"), "{e}");
    }

    #[test]
    fn a_string_error_code_parses_too() {
        // com.webos.service.tv.capture really does this. Typing errorCode as
        // i32 turned a good error reply into "the reply was not JSON".
        let r = reply(
            r#"{"returnValue":false,"errorCode":"CAPTURE_ERROR_09",
                "errorText":"Could not capture in no signal state"}"#,
        );
        let e = r.check("luna://x/y").unwrap_err().to_string();
        assert!(e.contains("no signal state"), "{e}");
        assert!(e.contains("CAPTURE_ERROR_09"), "{e}");
    }

    #[test]
    fn a_positive_reply_is_not_an_error() {
        assert!(reply(r#"{"returnValue":true}"#).check("luna://x/y").is_ok());
    }

    #[test]
    fn a_reply_with_no_return_value_counts_as_failure() {
        let e = reply("{}").check("luna://x/y").unwrap_err().to_string();
        assert!(e.contains("no reason given"), "{e}");
    }
}
