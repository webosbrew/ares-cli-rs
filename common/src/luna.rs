use std::fmt::Debug;
use std::io::{Error as IoError, Error, ErrorKind, Read};
use std::time::Duration;

use libssh_rs::{Channel, Error as SshError, Session};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{Error as JsonError, Value};
use crate::session::SessionError;

pub trait Luna {
    fn call<P, R>(&self, uri: &str, payload: P, public: bool) -> Result<R, LunaError>
    where
        P: Sized + Serialize,
        R: DeserializeOwned;

    fn subscribe<P>(&self, uri: &str, payload: P, public: bool) -> Result<Subscription, LunaError>
    where
        P: Sized + Serialize;
}

#[derive(Debug)]
pub enum LunaError {
    Session(SessionError),
    Io(IoError),
    NotAvailable,
}

pub struct Subscription {
    ch: Channel,
    buffer: Vec<u8>,
}

#[derive(Debug)]
pub struct Message {
    value: Value,
}

#[derive(Serialize, Default)]
pub struct LunaEmptyPayload {}

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

impl Luna for Session {
    fn call<P, R>(&self, uri: &str, payload: P, public: bool) -> Result<R, LunaError>
    where
        P: Sized + Serialize,
        R: DeserializeOwned,
    {
        let ch = self.new_channel()?;
        ch.open_session()?;
        let luna_cmd = if public { "luna-send-pub" } else { "luna-send" };
        let uri = snailquote::escape(uri.into());
        let payload_str = serde_json::to_string(&payload)?;
        ch.request_exec(&format!(
            "{luna_cmd} -n 1 {uri} {}",
            snailquote::escape(&payload_str)
        ))?;
        let mut buf = String::new();
        ch.stdout().read_to_string(&mut buf)?;
        let mut stderr = String::new();
        ch.stderr().read_to_string(&mut stderr)?;
        let exit_code = ch.get_exit_status().unwrap_or(0);
        ch.close()?;
        if exit_code == 0 {
            return Ok(serde_json::from_str(&buf)?);
        }
        return Err(LunaError::NotAvailable);
    }

    fn subscribe<P>(&self, uri: &str, payload: P, public: bool) -> Result<Subscription, LunaError>
    where
        P: Sized + Serialize,
    {
        let ch = self.new_channel()?;
        ch.open_session()?;
        let luna_cmd = if public { "luna-send-pub" } else { "luna-send" };
        let uri = snailquote::escape(uri.into());
        let payload_str = serde_json::to_string(&payload)?;
        ch.request_exec(&format!(
            "{luna_cmd} -i {uri} {}",
            snailquote::escape(&payload_str)
        ))?;
        return Ok(Subscription {
            ch,
            buffer: Vec::new(),
        });
    }
}

impl Message {
    pub fn deserialize<T: DeserializeOwned>(self) -> Result<T, serde_json::Error> {
        return serde_json::from_value(self.value);
    }
}

impl From<SshError> for LunaError {
    fn from(value: SshError) -> Self {
        return Self::Session(value.into());
    }
}

impl From<SessionError> for LunaError {
    fn from(value: SessionError) -> Self {
        return Self::Session(value);
    }
}

impl From<JsonError> for LunaError {
    fn from(value: JsonError) -> Self {
        return Self::Io(IoError::new(
            ErrorKind::InvalidData,
            format!("Invalid JSON: {value:?}"),
        ));
    }
}

impl From<IoError> for LunaError {
    fn from(value: IoError) -> Self {
        return Self::Io(value);
    }
}
