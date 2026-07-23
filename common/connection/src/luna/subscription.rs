use std::io::{Error, ErrorKind};
use std::time::Duration;

use libssh_rs::Error as SshError;

use crate::luna::{Message, Subscription};

impl Iterator for Subscription {
    type Item = std::io::Result<Message>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Emit any complete line already buffered before reading more, so a
            // single read that returns multiple lines is drained one at a time.
            if let Some(idx) = self.buffer.iter().position(|&r| r == b'\n') {
                let item = serde_json::from_slice(&self.buffer[..idx]);
                self.buffer.drain(..idx + 1);
                return Some(
                    item.map_err(|e| {
                        Error::new(ErrorKind::InvalidData, format!("Bad JSON response: {e:?}"))
                    })
                    .map(|value| Message { value }),
                );
            }
            if self.ch.is_closed() || self.ch.is_eof() {
                return None;
            }
            let mut buffer = [0; 1024];
            match self
                .ch
                .read_timeout(&mut buffer, false, Some(Duration::from_millis(10)))
            {
                Ok(len) => {
                    self.buffer.extend_from_slice(&buffer[..len]);
                }
                // The channel is in non-blocking mode, so a read that finds no
                // data within the timeout window returns TryAgain (SSH_AGAIN).
                // This is not fatal — the streaming install response simply
                // hasn't produced the next line yet, so keep polling.
                Err(SshError::TryAgain) => {}
                Err(e) => {
                    return Some(Err(Error::new(
                        ErrorKind::Other,
                        format!("SSH read error: {e:?}"),
                    )));
                }
            }
        }
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.close().unwrap_or_else(|e| {
            eprintln!("Failed to close subscription: {e:?}");
            return 0;
        });
    }
}

impl Subscription {
    fn close(&mut self) -> Result<i32, Error> {
        self.ch.send_eof()?;
        self.ch.request_send_signal("TERM")?;
        let status = self.ch.get_exit_status();
        self.ch.close()?;
        Ok(status.unwrap_or(-1) as i32)
    }
}
