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
