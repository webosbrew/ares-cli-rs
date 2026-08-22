//! Pressing buttons.
//!
//! `test/sendKeyCode` reaches `UInputWriter::sendKeyPress`, which writes one
//! press to `/dev/uinput`. There is no press/release pair to hold, and the
//! call returns as soon as the service accepts the code — not when the focused
//! window has done anything with it. Both facts shape everything here.

use std::thread::sleep;
use std::time::Duration;

use libssh_rs::Session;
use serde::Serialize;
use serde_json::json;

use crate::error::DoError;
use crate::keycode::Key;
use crate::luna;
use crate::output::{Reporter, Timer};

pub(crate) const SEND_KEY_URI: &str = "luna://com.webos.service.networkinput/test/sendKeyCode";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SendKeyCode {
    key_code: u16,
}

pub(crate) trait SendKey {
    /// # Errors
    ///
    /// Whatever the luna call failed with.
    fn send_key(&self, key: Key, reporter: &Reporter) -> Result<(), DoError>;

    /// Send each key in turn, waiting `delay` *between* them.
    ///
    /// The wait goes between rather than after, so a flow does not pay for a
    /// delay nobody is waiting on.
    ///
    /// # Errors
    ///
    /// Stops at the first key that fails, and says which one.
    fn send_keys(&self, keys: &[Key], delay: Duration, reporter: &Reporter) -> Result<(), DoError>;
}

impl SendKey for Session {
    fn send_key(&self, key: Key, reporter: &Reporter) -> Result<(), DoError> {
        luna::call(
            self,
            SEND_KEY_URI,
            &SendKeyCode { key_code: key.code },
            reporter,
        )
    }

    fn send_keys(&self, keys: &[Key], delay: Duration, reporter: &Reporter) -> Result<(), DoError> {
        let timer = Timer::start();
        for (i, key) in keys.iter().enumerate() {
            if i > 0 && !delay.is_zero() {
                sleep(delay);
            }
            self.send_key(*key, reporter)?;
        }

        let rendered: Vec<String> = keys.iter().map(ToString::to_string).collect();
        reporter.info(&format!("Sent {}", rendered.join(" ")));
        reporter.event(&json!({
            "event": "key",
            "ok": true,
            "keys": keys.iter().map(|k| json!({"name": k.name, "code": k.code}))
                .collect::<Vec<_>>(),
            "ms": timer.ms(),
        }));
        Ok(())
    }
}
