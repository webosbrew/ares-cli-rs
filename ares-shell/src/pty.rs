use std::io::Write;
use std::thread;
use std::time::Duration;

use crossbeam_channel::{select, tick};
use crossterm::terminal;
use libssh_rs::Channel;
use libssh_rs::Error::TryAgain;

use crate::io::{io_error, spawn_stdin_reader};

/// How long the main loop waits for input before checking the remote for
/// output and the local terminal for resizes. Small enough to feel instant,
/// large enough to keep the process idle when nothing is happening.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Restores the terminal to cooked mode when dropped, even on early return.
struct RawMode;

impl RawMode {
    fn enable() -> std::io::Result<Self> {
        terminal::enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

pub(crate) fn shell(ch: Channel) -> Result<i32, std::io::Error> {
    let _raw = RawMode::enable()?;
    let stdin_rx = spawn_stdin_reader();
    let ticker = tick(POLL_INTERVAL);

    let mut buf = [0u8; 8192];
    let mut last_size = terminal::size().unwrap_or((80, 24));
    let mut input_open = true;

    loop {
        // Forward everything the remote has sent us. In a PTY, stderr is
        // folded into stdout, so reading the stdout stream is enough.
        drain(&ch, &mut buf)?;

        if ch.is_eof() || ch.is_closed() {
            break;
        }

        // Mirror local terminal resizes to the remote PTY.
        match terminal::size() {
            Ok(size) if size != last_size => {
                ch.change_pty_size(u32::from(size.0), u32::from(size.1))
                    .map_err(io_error)?;
                last_size = size;
            }
            _ => {}
        }

        if input_open {
            select! {
                recv(stdin_rx) -> msg => match msg {
                    Ok(bytes) => {
                        let mut stdin = ch.stdin();
                        stdin.write_all(&bytes)?;
                        stdin.flush()?;
                    }
                    // Local stdin reached EOF: let the remote know we are done.
                    Err(_) => {
                        input_open = false;
                        let _ = ch.send_eof();
                    }
                },
                recv(ticker) -> _ => {}
            }
        } else {
            // No more input to forward; just keep draining remote output.
            thread::sleep(POLL_INTERVAL);
        }
    }

    Ok(ch.get_exit_status().unwrap_or(-1))
}

/// Writes all currently-available remote output to stdout without blocking.
fn drain(ch: &Channel, buf: &mut [u8]) -> Result<(), std::io::Error> {
    let mut stdout = std::io::stdout().lock();
    let mut wrote = false;
    loop {
        match ch.read_nonblocking(buf, false) {
            Ok(0) | Err(TryAgain) => break,
            Ok(size) => {
                stdout.write_all(&buf[..size])?;
                wrote = true;
            }
            Err(e) => return Err(io_error(e)),
        }
    }
    if wrote {
        stdout.flush()?;
    }
    Ok(())
}
