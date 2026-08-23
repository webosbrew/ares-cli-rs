//! One place that knows how a luna reply is shaped.
//!
//! Every service answers with `returnValue`, and adds `errorText`/`errorCode`
//! when it is false. [`Luna::call`] only tells us the transport worked, so
//! turning "the call happened" into "the call did what was asked" happens here.

use std::fmt::{Display, Formatter};
use std::time::Duration;

use libssh_rs::Session;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::error::DoError;
use crate::exec;
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

/// Call a private-bus method and insist it succeeded.
///
/// Everything `ares-do` drives is on the private bus, so `public` is never a
/// parameter here — passing `true` by accident would silently switch to
/// `luna-send-pub` and fail differently.
///
/// # Errors
///
/// [`DoError::LunaUnavailable`] when `luna-send` itself failed, and
/// [`DoError::LunaFailed`] when the service answered with `returnValue: false`.
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

/// Call a private-bus method and hand back the whole reply.
///
/// # Errors
///
/// [`DoError::LunaUnavailable`] when `luna-send` itself failed. A negative
/// reply is *not* an error here — the caller decides what it means.
pub(crate) fn raw_pub<P: Serialize, R: DeserializeOwned>(
    session: &Session,
    uri: &str,
    payload: &P,
    timeout: Option<Duration>,
    reporter: &Reporter,
) -> Result<R, DoError> {
    send(session, "luna-send-pub", uri, payload, timeout, reporter)
}

/// Private bus, which is where everything `ares-do` drives lives.
///
/// # Errors
///
/// See [`send`].
pub(crate) fn raw<P: Serialize, R: DeserializeOwned>(
    session: &Session,
    uri: &str,
    payload: &P,
    timeout: Option<Duration>,
    reporter: &Reporter,
) -> Result<R, DoError> {
    send(session, "luna-send", uri, payload, timeout, reporter)
}

/// Run one `luna-send` and parse what comes back.
///
/// # Errors
///
/// [`DoError::LunaCommand`] when `luna-send` exits non-zero,
/// [`DoError::LunaReply`] when what it printed is not a reply, and
/// [`DoError::Timeout`] when it never finished.
fn send<P: Serialize, R: DeserializeOwned>(
    session: &Session,
    program: &str,
    uri: &str,
    payload: &P,
    timeout: Option<Duration>,
    reporter: &Reporter,
) -> Result<R, DoError> {
    let rendered = serde_json::to_string(payload)
        .map_err(|e| DoError::Usage(format!("payload could not be serialised: {e}")))?;
    // Escaped exactly as ares-connection-lib does, so the two build the same
    // command line for the same call.
    let command = format!(
        "{program} -n 1 {} {}",
        snailquote::escape(uri),
        snailquote::escape(&rendered)
    );
    reporter.trace(&command);

    let output = exec::run(session, &command, timeout)?;
    if output.code != 0 {
        return Err(DoError::LunaCommand {
            uri: uri.to_string(),
            code: output.code,
            said: output.complaint().to_string(),
        });
    }
    // Some builds print a warning before the reply, so take the last line that
    // looks like one rather than all of stdout.
    let reply = output
        .stdout
        .lines()
        .map(str::trim)
        .rfind(|l| l.starts_with('{'))
        .unwrap_or("");
    serde_json::from_str(reply).map_err(|e| DoError::LunaReply {
        uri: uri.to_string(),
        said: if reply.is_empty() {
            output.complaint().to_string()
        } else {
            reply.to_string()
        },
        why: e.to_string(),
    })
}

/// True when a failure means "try the other service", rather than "give up".
///
/// Deliberately broad: whether a missing service shows up as a non-zero exit,
/// an unparseable reply or a plain `returnValue: false` varies by build, so
/// the screenshot fallback retries on any of them rather than matching prose.
/// A timeout is excluded — waiting again for something that already hung is
/// just waiting twice.
pub(crate) fn is_worth_retrying_elsewhere(error: &DoError) -> bool {
    matches!(
        error,
        DoError::LunaFailed { .. } | DoError::LunaCommand { .. } | DoError::LunaReply { .. }
    )
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
