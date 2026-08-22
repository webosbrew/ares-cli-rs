//! Running one plain command on the device.
//!
//! `ares-connection-lib` has this, but `Transfer::exec` is private and the
//! `Luna` trait only speaks luna. The root probe needs neither, so it gets
//! this — the same channel dance both of those do.

use std::io::Read;

use libssh_rs::{Error as SshError, Session};

pub(crate) struct Output {
    pub stdout: String,
    pub code: i32,
}

pub(crate) trait Exec {
    /// Run `command`, wait for it, and collect stdout.
    ///
    /// # Errors
    ///
    /// Fails when the channel cannot be opened or the command cannot be sent.
    /// A command that runs and fails is a successful call with a non-zero
    /// [`Output::code`].
    fn exec(&self, command: &str) -> Result<Output, SshError>;
}

impl Exec for Session {
    fn exec(&self, command: &str) -> Result<Output, SshError> {
        let ch = self.new_channel()?;
        ch.open_session()?;
        ch.request_exec(command)?;
        let mut stdout = String::new();
        // A command whose output is not UTF-8 is not one we ask for.
        let _ = ch.stdout().read_to_string(&mut stdout);
        let mut stderr = String::new();
        let _ = ch.stderr().read_to_string(&mut stderr);
        let code = ch.get_exit_status().unwrap_or(0);
        ch.close()?;
        Ok(Output { stdout, code })
    }
}
