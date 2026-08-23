//! Running one command on the device, with a deadline.
//!
//! Everything that talks to a device eventually runs a command over an exec
//! channel, and the obvious way to read one — `read_to_string` until EOF — has
//! no bound. A command that never exits, or a service that subscribes instead
//! of replying, hangs the caller forever. That is the one failure an
//! unattended run cannot recover from, so reading is bounded here and every
//! caller inherits it.

use std::fmt::{Display, Formatter};
use std::time::{Duration, Instant};

use libssh_rs::{Error as SshError, PollStatus, Session};

/// Read in chunks this size: big enough that a typical reply lands in one or
/// two reads, small enough not to hold a wasteful buffer per call.
const CHUNK: usize = 8 * 1024;

/// How long to wait for something to become readable before checking the
/// deadline again.
const SLICE: Duration = Duration::from_millis(50);

/// What a command said, and how it ended.
#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl CommandOutput {
    #[must_use]
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }

    /// What the command said, preferring stderr — for putting in an error.
    #[must_use]
    pub fn complaint(&self) -> &str {
        [self.stderr.trim(), self.stdout.trim()]
            .into_iter()
            .find(|s| !s.is_empty())
            .unwrap_or("no output")
    }
}

#[derive(Debug)]
pub enum ExecError {
    Ssh(SshError),
    /// The deadline passed with the command still running.
    Timeout {
        command: String,
        after: Duration,
    },
}

impl Display for ExecError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecError::Ssh(e) => write!(f, "{e}"),
            ExecError::Timeout { command, after } => {
                let short: String = command.chars().take(60).collect();
                write!(f, "`{short}` did not finish within {after:?}")
            }
        }
    }
}

impl std::error::Error for ExecError {}

impl From<SshError> for ExecError {
    fn from(value: SshError) -> Self {
        ExecError::Ssh(value)
    }
}

pub trait Exec {
    /// Run `command` and collect what it said, waiting as long as it takes.
    ///
    /// # Errors
    ///
    /// Fails when the channel cannot be opened or the command cannot be sent.
    /// A command that runs and fails is a successful call with a non-zero
    /// [`CommandOutput::exit_code`].
    fn exec(&self, command: &str) -> Result<CommandOutput, ExecError> {
        self.exec_timeout(command, None)
    }

    /// Run `command`, giving up after `timeout`.
    ///
    /// The timeout bounds the whole call, not each read. `None` waits forever.
    ///
    /// # Errors
    ///
    /// [`ExecError::Timeout`] when the deadline passes with the command still
    /// running, and [`ExecError::Ssh`] when the channel fails.
    fn exec_timeout(
        &self,
        command: &str,
        timeout: Option<Duration>,
    ) -> Result<CommandOutput, ExecError>;
}

impl Exec for Session {
    fn exec_timeout(
        &self,
        command: &str,
        timeout: Option<Duration>,
    ) -> Result<CommandOutput, ExecError> {
        let deadline = timeout.map(|t| Instant::now() + t);
        let expired = || ExecError::Timeout {
            command: command.to_string(),
            after: timeout.unwrap_or_default(),
        };

        let ch = self.new_channel()?;
        ch.open_session()?;
        ch.request_exec(command)?;

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut buf = vec![0u8; CHUNK];

        // Both streams are drained together. Waiting on stdout while the
        // command fills the stderr window would deadlock.
        loop {
            if let Some(end) = deadline
                && Instant::now() >= end
            {
                return Err(expired());
            }

            let mut moved = false;
            let mut open = false;
            for (is_stderr, sink) in [(false, &mut stdout), (true, &mut stderr)] {
                match ch.poll_timeout(is_stderr, Some(Duration::ZERO)) {
                    Ok(PollStatus::AvailableBytes(0)) => open = true,
                    Ok(PollStatus::AvailableBytes(_)) => {
                        open = true;
                        match ch.read_timeout(&mut buf, is_stderr, Some(Duration::ZERO)) {
                            // 0 and TryAgain both mean "not yet"; the stream
                            // is still open either way.
                            Ok(0) | Err(SshError::TryAgain) => {}
                            Ok(n) => {
                                sink.extend_from_slice(&buf[..n]);
                                moved = true;
                            }
                            Err(e) => return Err(e.into()),
                        }
                    }
                    Ok(PollStatus::EndOfFile) => {}
                    Err(e) => return Err(e.into()),
                }
            }

            if !open {
                break;
            }
            if !moved {
                // Nothing ready: wait on stdout rather than spinning, but
                // never past the deadline.
                let slice = deadline.map_or(SLICE, |end| {
                    end.saturating_duration_since(Instant::now()).min(SLICE)
                });
                if slice.is_zero() {
                    return Err(expired());
                }
                let _ = ch.poll_timeout(false, Some(slice));
            }
        }

        let exit_code = ch.get_exit_status().unwrap_or(0);
        let _ = ch.close();

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
            exit_code,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::CommandOutput;

    fn out(stdout: &str, stderr: &str, exit_code: i32) -> CommandOutput {
        CommandOutput {
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            exit_code,
        }
    }

    #[test]
    fn success_is_only_a_zero_status() {
        assert!(out("", "", 0).success());
        assert!(!out("", "", 1).success());
    }

    #[test]
    fn the_complaint_prefers_stderr_then_stdout() {
        assert_eq!(out("nope", "bad", 1).complaint(), "bad");
        assert_eq!(out("nope", "  ", 1).complaint(), "nope");
        assert_eq!(out(" ", "\n", 1).complaint(), "no output");
    }
}
