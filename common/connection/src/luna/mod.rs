use std::fmt::{Display, Formatter};
use std::io::Error as IoError;

use libssh_rs::Channel;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::session::SessionError;

mod luna;
mod message;
mod subscription;

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
#[non_exhaustive]
pub enum LunaError {
    Session(SessionError),
    Io(IoError),
    /// `luna-send` failed and said nothing about why.
    NotAvailable,
    /// `luna-send` exited non-zero. Carries what it said, so a caller can tell
    /// "no such service" from "not allowed on this bus" — previously both
    /// arrived as [`LunaError::NotAvailable`] with the output thrown away.
    Command {
        exit_code: i32,
        stdout: String,
        stderr: String,
    },
}

impl Display for LunaError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            LunaError::Session(e) => write!(f, "{e}"),
            LunaError::Io(e) => write!(f, "{e}"),
            LunaError::NotAvailable => write!(f, "luna service is not available"),
            LunaError::Command {
                exit_code,
                stdout,
                stderr,
            } => {
                let said = [stderr.trim(), stdout.trim()]
                    .into_iter()
                    .find(|s| !s.is_empty())
                    .unwrap_or("no output");
                write!(f, "luna-send exited {exit_code}: {said}")
            }
        }
    }
}

impl std::error::Error for LunaError {}

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
