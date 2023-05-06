use std::fmt::Debug;
use std::io::{BufRead, BufReader, Error as IoError, Read};

use libssh_rs::{Channel, Error as SshError, Session};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{Error as JsonError, Value};

#[derive(Debug)]
pub enum LunaError {
    NotAvailable,
}

#[derive(Serialize, Default)]
pub struct LunaEmptyPayload {}

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
pub struct Message {
    value: Value,
}

pub struct Subscription {
    ch: Channel,
}

impl Iterator for Subscription {
    type Item = std::io::Result<Message>;

    fn next(&mut self) -> Option<Self::Item> {
        println!("iterator begin call");
        if self.ch.is_closed() || self.ch.is_eof() {
            println!("iterator finish");
            return None;
        }
        let stdout = self.ch.stdout();
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        if let Err(e) = reader.read_line(&mut line) {
            return Some(Err(e));
        }
        println!("iterator end call: {line}");
        return Some(Ok(Message { value: serde_json::from_str(line.trim()).unwrap() }));
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
            P: Sized + Serialize
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
        return Ok(Subscription { ch });
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
