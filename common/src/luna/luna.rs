use std::io::Read;
use std::io::{Error as IoError, ErrorKind};

use libssh_rs::{Error as SshError, Session};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Error as JsonError;

use crate::luna::{Luna, LunaError, Subscription};
use crate::session::SessionError;

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
