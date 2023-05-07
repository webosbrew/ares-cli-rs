use std::io::Error as IoError;

use libssh_rs::Channel;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use crate::session::SessionError;

mod luna;
mod subscription;
mod message;

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
