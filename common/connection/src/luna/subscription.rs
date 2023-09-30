use crate::luna::{Message, Subscription};
use serde_json::Value;
use std::io::{Error, ErrorKind};
use std::time::Duration;

impl Iterator for Subscription {
    type Item = std::io::Result<Message>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.ch.is_closed() || self.ch.is_eof() {
            return None;
        }
        let item: serde_json::Result<Value>;
        loop {
            let mut buffer = [0; 1024];
            match self
                .ch
                .read_timeout(&mut buffer, false, Some(Duration::from_millis(10)))
            {
                Ok(len) => {
                    self.buffer.extend_from_slice(&buffer[..len]);
                }
                Err(e) => {
                    return Some(Err(Error::new(
                        ErrorKind::Other,
                        format!("SSH read error: {e:?}"),
                    )));
                }
            }
            if self.buffer.is_empty() {
                continue;
            }
            if let Some(idx) = self.buffer.iter().position(|&r| r == b'\n') {
                item = serde_json::from_slice(&self.buffer[..idx]);
                self.buffer.drain(..idx + 1);
                break;
            }
        }
        return Some(
            item.map_err(|e| {
                Error::new(ErrorKind::InvalidData, format!("Bad JSON response: {e:?}"))
            })
            .map(|value| Message { value }),
        );
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
        return Ok(status.unwrap_or(-1) as i32);
    }
}
