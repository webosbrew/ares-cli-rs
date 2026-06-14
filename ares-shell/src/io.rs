use std::io::{Read, stdin};
use std::thread;

use crossbeam_channel::{Receiver, unbounded};

/// Reads raw bytes from stdin on a background thread and forwards them
/// verbatim, exactly like `ssh`. Doing the read on its own thread lets the
/// main loop block on either input or remote output without busy-polling.
///
/// In PTY mode the terminal is in raw mode, so control keys, UTF-8 input and
/// escape sequences all arrive already correctly encoded and are passed
/// through untouched. The channel closes when stdin reaches EOF.
pub(crate) fn spawn_stdin_reader() -> Receiver<Vec<u8>> {
    let (tx, rx) = unbounded::<Vec<u8>>();
    thread::spawn(move || {
        let mut stdin = stdin().lock();
        let mut buf = [0u8; 8192];
        loop {
            match stdin.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(size) => {
                    if tx.send(buf[..size].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });
    rx
}

pub(crate) fn io_error(e: libssh_rs::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
}
