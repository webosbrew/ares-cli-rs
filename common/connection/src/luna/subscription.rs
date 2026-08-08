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
            if let Some(item) = take_line(&mut self.buffer) {
                return Some(item);
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

/// Take the first complete line out of `buffer` and parse it as one message.
/// Returns `None` while no line has arrived yet, so the caller reads more.
fn take_line(buffer: &mut Vec<u8>) -> Option<std::io::Result<Message>> {
    let idx = buffer.iter().position(|&r| r == b'\n')?;
    let item = serde_json::from_slice(&buffer[..idx]);
    buffer.drain(..idx + 1);
    Some(
        item.map_err(|e| Error::new(ErrorKind::InvalidData, format!("Bad JSON response: {e:?}")))
            .map(|value| Message { value }),
    )
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

#[cfg(test)]
mod tests {
    use super::take_line;

    fn buffer(text: &str) -> Vec<u8> {
        text.as_bytes().to_vec()
    }

    #[test]
    fn no_line_yet_reads_more() {
        let mut buf = buffer(r#"{"returnValue":true"#);
        assert!(take_line(&mut buf).is_none());
        // Nothing was consumed, so the rest of the line can still arrive.
        assert_eq!(buf, buffer(r#"{"returnValue":true"#));
    }

    #[test]
    fn one_read_holding_many_lines_drains_one_at_a_time() {
        // appinstalld streams several status lines, and a single read can catch
        // more than one of them.
        let mut buf = buffer("{\"a\":1}\n{\"a\":2}\n{\"a\":3}\n");
        for expected in 1..=3 {
            let message = take_line(&mut buf).expect("a line").expect("valid JSON");
            let value: serde_json::Value = message.deserialize().unwrap();
            assert_eq!(value["a"], expected);
        }
        assert!(take_line(&mut buf).is_none());
        assert!(buf.is_empty());
    }

    #[test]
    fn a_partial_trailing_line_is_kept() {
        let mut buf = buffer("{\"a\":1}\n{\"a\":2");
        assert!(take_line(&mut buf).is_some());
        assert!(take_line(&mut buf).is_none());
        assert_eq!(buf, buffer("{\"a\":2"));
    }

    #[test]
    fn bad_json_is_an_error_and_still_consumes_the_line() {
        let mut buf = buffer("not json\n{\"a\":1}\n");
        let error = take_line(&mut buf).expect("a line").expect_err("bad JSON");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        // The bad line must not block the good one behind it.
        let message = take_line(&mut buf).expect("a line").expect("valid JSON");
        let value: serde_json::Value = message.deserialize().unwrap();
        assert_eq!(value["a"], 1);
    }

    #[test]
    fn carriage_returns_do_not_break_parsing() {
        let mut buf = buffer("{\"a\":1}\r\n");
        let message = take_line(&mut buf).expect("a line").expect("valid JSON");
        let value: serde_json::Value = message.deserialize().unwrap();
        assert_eq!(value["a"], 1);
    }
}
