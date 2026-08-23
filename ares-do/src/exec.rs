//! Running one command on the device, with a deadline.
//!
//! `ares-connection-lib` has this, but `Transfer::exec` is private and
//! `Luna::call` reads to EOF with no bound — so a command that never finishes,
//! or a luna method that subscribes instead of replying, hangs forever. That
//! is the one failure an unattended run cannot recover from, so every command
//! `ares-do` sends goes through here and every one of them can be given up on.

use std::time::{Duration, Instant};

use libssh_rs::{Error as SshError, PollStatus, Session};

use crate::error::DoError;

/// Read in chunks this size. Big enough that a luna reply arrives in one or
/// two reads, small enough not to hold a wasteful buffer per call.
const CHUNK: usize = 8 * 1024;

pub(crate) struct Output {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

impl Output {
    /// What the command said, preferring stderr, for an error message.
    pub(crate) fn complaint(&self) -> &str {
        [self.stderr.trim(), self.stdout.trim()]
            .into_iter()
            .find(|s| !s.is_empty())
            .unwrap_or("no output")
    }
}

/// Run `command` and collect what it said.
///
/// `timeout` bounds the whole call, not each read. `None` waits forever, which
/// is what `--timeout 0` asks for.
///
/// # Errors
///
/// [`DoError::Timeout`] when the deadline passes with the command still
/// running, and [`DoError::Usage`] when the channel itself fails.
pub(crate) fn run(
    session: &Session,
    command: &str,
    timeout: Option<Duration>,
) -> Result<Output, DoError> {
    let deadline = timeout.map(|t| Instant::now() + t);
    let describe = || {
        let short: String = command.chars().take(60).collect();
        format!("`{short}` to finish")
    };

    let ch = session
        .new_channel()
        .and_then(|ch| {
            ch.open_session()?;
            ch.request_exec(command)?;
            Ok(ch)
        })
        .map_err(|e: SshError| DoError::Usage(format!("could not run `{command}`: {e}")))?;

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut buf = vec![0u8; CHUNK];

    // Both streams have to be drained together: a command that fills the
    // stderr window while we wait on stdout would deadlock otherwise.
    loop {
        let remaining = match deadline {
            Some(end) => match end.checked_duration_since(Instant::now()) {
                Some(left) if !left.is_zero() => Some(left),
                _ => return Err(DoError::Timeout(describe())),
            },
            None => None,
        };

        let mut moved = false;
        let mut open = false;
        for (is_stderr, sink) in [(false, &mut stdout), (true, &mut stderr)] {
            match ch.poll_timeout(is_stderr, Some(Duration::ZERO)) {
                Ok(PollStatus::AvailableBytes(0)) => open = true,
                Ok(PollStatus::AvailableBytes(_)) => {
                    match ch.read_timeout(&mut buf, is_stderr, Some(Duration::ZERO)) {
                        Ok(0) => {}
                        Ok(n) => {
                            sink.extend_from_slice(&buf[..n]);
                            moved = true;
                            open = true;
                        }
                        Err(SshError::TryAgain) => open = true,
                        Err(e) => {
                            return Err(DoError::Usage(format!("reading from `{command}`: {e}")));
                        }
                    }
                }
                Ok(PollStatus::EndOfFile) => {}
                Err(e) => return Err(DoError::Usage(format!("reading from `{command}`: {e}"))),
            }
        }

        if !open {
            break;
        }
        if !moved {
            // Nothing ready. Wait on stdout rather than spinning, but never
            // past the deadline.
            let slice = remaining.map_or(Duration::from_millis(50), |left| {
                left.min(Duration::from_millis(50))
            });
            let _ = ch.poll_timeout(false, Some(slice));
        }
    }

    let code = ch.get_exit_status().unwrap_or(0);
    let _ = ch.close();

    Ok(Output {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        code,
    })
}
