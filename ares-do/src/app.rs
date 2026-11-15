//! Starting and stopping apps, so a flow does not have to shell out.
//!
//! `ares-launch` already does this. Duplicating it is deliberate: a flow that
//! spawns `ares-launch` pays a second SSH handshake for every step, which on a
//! TV is most of the wall clock. The URIs and payload shape below are the same
//! ones in `ares-launch/src/{launch,close}.rs` — keep the two in step.
//!
//! `applicationManager` is reachable on both buses, but not with the same
//! permissions: on a 49LK5900 the public bus answers `getForegroundAppInfo`
//! with `Denied method call ... for category "/"`, and the private bus answers
//! it properly. Since `ares-do` already requires root for everything else,
//! these use the private bus too.

use std::time::Duration;

use libssh_rs::Session;
use serde::Serialize;
use serde_json::{Value, json};

use crate::error::DoError;
use crate::luna::{self, LunaReply};
use crate::output::{Reporter, Timer};

const LAUNCH_URI: &str = "luna://com.webos.applicationManager/launch";
const CLOSE_URI: &str = "luna://com.webos.applicationManager/dev/closeByAppId";
const FOREGROUND_URI: &str = "luna://com.webos.applicationManager/getForegroundAppInfo";

#[derive(Serialize, Debug)]
struct AppParams {
    id: String,
    subscribe: bool,
    #[serde(skip_serializing_if = "Value::is_null")]
    params: Value,
}

/// An `applicationManager` call, on the private bus for the reason above.
fn call_app(
    session: &Session,
    uri: &str,
    payload: &AppParams,
    timeout: Option<Duration>,
    reporter: &Reporter,
) -> Result<(), DoError> {
    let reply: LunaReply = luna::raw(session, uri, payload, timeout, reporter)?;
    reply.check(uri)
}

pub(crate) trait AppControl {
    /// # Errors
    ///
    /// Whatever `applicationManager` said.
    fn launch_app(
        &self,
        id: &str,
        params: Value,
        timeout: Option<Duration>,
        reporter: &Reporter,
    ) -> Result<(), DoError>;

    /// # Errors
    ///
    /// Whatever `applicationManager` said.
    fn close_app(
        &self,
        id: &str,
        params: Value,
        timeout: Option<Duration>,
        reporter: &Reporter,
    ) -> Result<(), DoError>;

    /// Which app currently owns the screen, for context on a screenshot.
    ///
    /// Best effort: a failure here is never worth failing a capture over, so
    /// it answers `None` rather than erroring.
    fn foreground_app(&self, timeout: Option<Duration>, reporter: &Reporter) -> Option<String>;
}

impl AppControl for Session {
    fn launch_app(
        &self,
        id: &str,
        params: Value,
        timeout: Option<Duration>,
        reporter: &Reporter,
    ) -> Result<(), DoError> {
        let timer = Timer::start();
        call_app(
            self,
            LAUNCH_URI,
            &AppParams {
                id: id.to_string(),
                subscribe: false,
                params: params.clone(),
            },
            timeout,
            reporter,
        )?;
        reporter.info(&format!("Launched {id}"));
        reporter.event(&json!({
            "event": "launch", "ok": true, "appId": id,
            "params": params, "ms": timer.ms(),
        }));
        Ok(())
    }

    fn close_app(
        &self,
        id: &str,
        params: Value,
        timeout: Option<Duration>,
        reporter: &Reporter,
    ) -> Result<(), DoError> {
        let timer = Timer::start();
        call_app(
            self,
            CLOSE_URI,
            &AppParams {
                id: id.to_string(),
                subscribe: false,
                params,
            },
            timeout,
            reporter,
        )?;
        reporter.info(&format!("Closed {id}"));
        reporter.event(&json!({
            "event": "close", "ok": true, "appId": id, "ms": timer.ms(),
        }));
        Ok(())
    }

    fn foreground_app(&self, timeout: Option<Duration>, reporter: &Reporter) -> Option<String> {
        let reply: Value = luna::raw(self, FOREGROUND_URI, &json!({}), timeout, reporter).ok()?;
        reply
            .get("appId")
            .and_then(Value::as_str)
            .map(ToString::to_string)
    }
}
