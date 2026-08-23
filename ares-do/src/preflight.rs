//! Checking, once, that this session can do anything at all.
//!
//! Both services `ares-do` drives sit on the private luna bus, which only root
//! may use. Finding that out after three keys have gone nowhere — and seeing
//! only `luna service is not available` — is a bad afternoon, so the check
//! happens before the first action.

use std::time::Duration;

use ares_connection_lib::exec::Exec;
use ares_connection_lib::session::{DeviceSession, SshConnection};

use crate::error::DoError;
use crate::output::Reporter;

pub(crate) trait Preflight {
    /// The uid this session actually has. `None` when the probe itself failed,
    /// which is different from "not root".
    fn remote_uid(&self, timeout: Option<Duration>) -> Option<u32>;

    /// # Errors
    ///
    /// [`DoError::NotRoot`] when the session is not root and `allow` is false.
    fn require_root(
        &self,
        allow: bool,
        timeout: Option<Duration>,
        reporter: &Reporter,
    ) -> Result<(), DoError>;
}

impl Preflight for DeviceSession {
    fn remote_uid(&self, timeout: Option<Duration>) -> Option<u32> {
        let output = self.session.exec_timeout("id -u", timeout).ok()?;
        if output.exit_code != 0 {
            return None;
        }
        output.stdout.trim().parse().ok()
    }

    fn require_root(
        &self,
        allow: bool,
        timeout: Option<Duration>,
        reporter: &Reporter,
    ) -> Result<(), DoError> {
        let uid = self.remote_uid(timeout);
        // The device entry says who we asked to be; the probe says who we are.
        // Trust the probe, and fall back to the entry only if it did not answer.
        let is_root = uid.map_or_else(|| self.is_root(), |uid| uid == 0);
        if is_root {
            return Ok(());
        }

        let error = DoError::NotRoot {
            device: self.device.name.clone(),
            user: self.device.username.clone(),
            uid,
        };
        if allow {
            reporter.warn(&format!(
                "not connected as root ({}); the private luna bus will probably refuse. \
                 Continuing because --allow-non-root was given.",
                self.device.username
            ));
            return Ok(());
        }
        Err(error)
    }
}
