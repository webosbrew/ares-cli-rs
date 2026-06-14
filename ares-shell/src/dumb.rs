use std::io::{Error, Write};
use std::thread;
use std::time::Duration;

use crossbeam_channel::{select, tick};
use libssh_rs::Channel;
use libssh_rs::Error::TryAgain;

use crate::io::{io_error, spawn_stdin_reader};

const POLL_INTERVAL: Duration = Duration::from_millis(10);

pub(crate) fn shell(ch: Channel) -> Result<i32, Error> {
    let stdin_rx = spawn_stdin_reader();
    let ticker = tick(POLL_INTERVAL);

    let mut buf = [0u8; 8192];
    let mut input_open = true;
    let mut pending_cr = false;

    loop {
        drain(&ch, &mut buf, false, &mut std::io::stdout().lock())?;
        drain(&ch, &mut buf, true, &mut std::io::stderr().lock())?;

        if ch.is_eof() || ch.is_closed() {
            break;
        }

        if input_open {
            select! {
                recv(stdin_rx) -> msg => match msg {
                    Ok(bytes) => {
                        // Without a remote PTY there is no line discipline to
                        // translate carriage returns, so normalize them here.
                        let bytes = crlf_to_lf(&bytes, &mut pending_cr);
                        let mut stdin = ch.stdin();
                        stdin.write_all(&bytes)?;
                        stdin.flush()?;
                    }
                    Err(_) => {
                        input_open = false;
                        let _ = ch.send_eof();
                    }
                },
                recv(ticker) -> _ => {}
            }
        } else {
            thread::sleep(POLL_INTERVAL);
        }
    }

    Ok(ch.get_exit_status().unwrap_or(-1))
}

/// Converts CR and CRLF line endings to LF.
///
/// In dumb mode there is no remote pseudo-terminal, so nothing on the far end
/// turns a carriage return into a newline. A raw `\r` would otherwise reach the
/// shell as part of the command (e.g. `ls\r`), which fails with
/// "<cmd>: not found". `pending_cr` carries the state of a `\r` seen at the end
/// of the previous chunk so a `\r\n` split across reads still collapses to one
/// `\n`.
fn crlf_to_lf(bytes: &[u8], pending_cr: &mut bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    for &b in bytes {
        match b {
            b'\r' => {
                out.push(b'\n');
                *pending_cr = true;
            }
            // The `\n` of a `\r\n` pair: the newline was already emitted.
            b'\n' if *pending_cr => *pending_cr = false,
            _ => {
                out.push(b);
                *pending_cr = false;
            }
        }
    }
    out
}

/// Writes all currently-available data from one remote stream without blocking.
fn drain<W: Write>(
    ch: &Channel,
    buf: &mut [u8],
    is_stderr: bool,
    out: &mut W,
) -> Result<(), Error> {
    let mut wrote = false;
    loop {
        match ch.read_nonblocking(buf, is_stderr) {
            Ok(0) | Err(TryAgain) => break,
            Ok(size) => {
                out.write_all(&buf[..size])?;
                wrote = true;
            }
            Err(e) => return Err(io_error(e)),
        }
    }
    if wrote {
        out.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::crlf_to_lf;

    fn convert(chunks: &[&[u8]]) -> Vec<u8> {
        let mut pending_cr = false;
        let mut out = Vec::new();
        for chunk in chunks {
            out.extend(crlf_to_lf(chunk, &mut pending_cr));
        }
        out
    }

    #[test]
    fn collapses_crlf_and_bare_cr() {
        assert_eq!(convert(&[b"ls\r\n"]), b"ls\n");
        assert_eq!(convert(&[b"ls\n"]), b"ls\n");
        assert_eq!(convert(&[b"ls\r"]), b"ls\n");
        assert_eq!(convert(&[b"a\r\rb"]), b"a\n\nb");
    }

    #[test]
    fn handles_crlf_split_across_chunks() {
        assert_eq!(convert(&[b"ls\r", b"\npwd\r\n"]), b"ls\npwd\n");
    }
}
