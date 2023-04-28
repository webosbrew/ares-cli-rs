use std::fmt::Debug;
use std::io::{Error as IoError, Read};

use libssh_rs::{Error as SshError, Session};
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;
use serde_json::Error as JsonError;

#[derive(Debug)]
pub enum LunaError {
    NotAvailable,
}


#[derive(Serialize, Default)]
pub struct LunaEmptyPayload {}

pub trait Luna {
    fn call<P, R>(&self, uri: &str, payload: P, public: bool) -> Result<R, LunaError>
        where P: Sized + Serialize, R: DeserializeOwned;
}

impl Luna for Session {
    fn call<P, R>(&self, uri: &str, payload: P, public: bool) -> Result<R, LunaError>
        where P: Sized + Serialize, R: DeserializeOwned {
        let ch = self.new_channel()?;
        ch.open_session()?;
        let luna_cmd = if public { "luna-send-pub" } else { "luna-send" };
        let uri = snailquote::escape(uri.into());
        let payload_str = serde_json::to_string(&payload)?;
        ch.request_exec(&format!("{luna_cmd} -n 1 {uri} {}", snailquote::escape(&payload_str)))?;
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
}

impl From<SshError> for LunaError {
    fn from(value: SshError) -> Self {
        todo!()
    }
}

impl From<JsonError> for LunaError {
    fn from(value: JsonError) -> Self {
        panic!("{value:?}")
    }
}

impl From<IoError> for LunaError {
    fn from(value: IoError) -> Self {
        todo!()
    }
}