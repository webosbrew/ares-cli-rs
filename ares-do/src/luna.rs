//! One place that knows how a luna reply is shaped.
//!
//! Every service answers with `returnValue`, and adds `errorText`/`errorCode`
//! when it is false. [`Luna::call`] only tells us the transport worked, so
//! turning "the call happened" into "the call did what was asked" happens here.

use ares_connection_lib::luna::{Luna, LunaError};
use libssh_rs::Session;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::error::DoError;
use crate::output::Reporter;

/// The part of a reply every service sends.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LunaReply {
    #[serde(default)]
    pub return_value: bool,
    pub error_code: Option<i32>,
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
            code: self.error_code,
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
    reporter: &Reporter,
) -> Result<(), DoError> {
    let reply: LunaReply = raw(session, uri, payload, reporter)?;
    reply.check(uri)
}

/// Call a private-bus method and hand back the whole reply.
///
/// # Errors
///
/// [`DoError::LunaUnavailable`] when `luna-send` itself failed. A negative
/// reply is *not* an error here — the caller decides what it means.
pub(crate) fn raw<P: Serialize, R: DeserializeOwned>(
    session: &Session,
    uri: &str,
    payload: &P,
    reporter: &Reporter,
) -> Result<R, DoError> {
    if reporter.is_verbose() {
        let rendered = serde_json::to_string(payload).unwrap_or_else(|_| String::from("<payload>"));
        reporter.trace(&format!("luna-send -n 1 {uri} {rendered}"));
    }
    session
        .call(uri, payload, false)
        .map_err(|source| DoError::LunaUnavailable {
            uri: uri.to_string(),
            source,
        })
}

/// True when a failure means "try the other service", rather than "give up".
///
/// [`LunaError`] collapses "no such service", "no permission" and "the channel
/// died" into one value, so this cannot be precise. That is deliberate: the
/// screenshot fallback retries on *any* failure of the first URI rather than
/// matching on prose that may never arrive.
pub(crate) fn is_worth_retrying_elsewhere(error: &DoError) -> bool {
    matches!(
        error,
        DoError::LunaUnavailable { .. } | DoError::LunaFailed { .. }
    )
}

/// What the error itself does not spell out.
///
/// [`LunaError::Command`] carries `luna-send`'s own output, so it needs no
/// help. [`LunaError::NotAvailable`] carries nothing at all, which is exactly
/// when a caller most needs a nudge.
pub(crate) fn unavailable_hint(error: &LunaError) -> Option<&'static str> {
    match error {
        LunaError::Session(_) => Some("the SSH session failed mid-call"),
        LunaError::Io(_) => Some("the reply was not JSON"),
        LunaError::NotAvailable => Some(
            "luna-send exited non-zero without saying why: the service may be missing, or this \
             user may not be allowed on the private bus",
        ),
        _ => None,
    }
}
