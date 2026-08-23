use std::fmt::{Display, Formatter};
use std::io::Error as IoError;
use std::time::Duration;

use libssh_rs::Channel;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::session::SessionError;

mod luna;
mod message;
mod subscription;

/// How to make a call, for the things that are not the URI or the payload.
///
/// [`Luna::call`] is this with defaults, kept because most callers only ever
/// need to pick a bus.
#[derive(Clone, Copy, Debug)]
pub struct LunaOptions {
    /// `true` uses `luna-send-pub`, `false` uses `luna-send`. Private-bus
    /// methods generally need root.
    pub public: bool,
    /// Give up after this long. `None` waits forever, which is rarely what
    /// anyone wants — a method that subscribes instead of replying never
    /// returns, and neither does the caller.
    pub timeout: Option<Duration>,
}

/// Long enough for a slow capture on a 4K panel, short enough that an
/// unattended run notices a wedged service rather than waiting for a person.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

impl Default for LunaOptions {
    fn default() -> Self {
        LunaOptions {
            public: true,
            timeout: Some(DEFAULT_TIMEOUT),
        }
    }
}

impl LunaOptions {
    /// Defaults: public bus, [`DEFAULT_TIMEOUT`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The private bus, `luna-send`.
    #[must_use]
    pub fn private() -> Self {
        LunaOptions {
            public: false,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn public(mut self, public: bool) -> Self {
        self.public = public;
        self
    }

    /// `None` waits forever.
    #[must_use]
    pub fn timeout(mut self, timeout: Option<Duration>) -> Self {
        self.timeout = timeout;
        self
    }

    fn program(self) -> &'static str {
        if self.public {
            "luna-send-pub"
        } else {
            "luna-send"
        }
    }
}

pub trait Luna {
    /// One call, on `public`'s bus, with the default timeout.
    ///
    /// # Errors
    ///
    /// See [`Luna::call_with`].
    fn call<P, R>(&self, uri: &str, payload: P, public: bool) -> Result<R, LunaError>
    where
        P: Sized + Serialize,
        R: DeserializeOwned,
    {
        self.call_with(uri, payload, LunaOptions::new().public(public))
    }

    /// One call, spelled out.
    ///
    /// # Errors
    ///
    /// [`LunaError::Timeout`] when it does not finish in time,
    /// [`LunaError::Command`] when `luna-send` exits non-zero,
    /// [`LunaError::Reply`] when what came back is not a reply, and
    /// [`LunaError::Session`] when the channel fails. A service answering
    /// `returnValue: false` is *not* an error here — that is a reply, and what
    /// it means is the caller's business.
    fn call_with<P, R>(&self, uri: &str, payload: P, options: LunaOptions) -> Result<R, LunaError>
    where
        P: Sized + Serialize,
        R: DeserializeOwned;

    /// Subscribe, on `public`'s bus.
    ///
    /// # Errors
    ///
    /// See [`Luna::subscribe_with`].
    fn subscribe<P>(&self, uri: &str, payload: P, public: bool) -> Result<Subscription, LunaError>
    where
        P: Sized + Serialize,
    {
        self.subscribe_with(uri, payload, LunaOptions::new().public(public))
    }

    /// Subscribe, spelled out.
    ///
    /// [`LunaOptions::timeout`] is not applied: a subscription is meant to
    /// stay open, and each message is read when the caller asks for it.
    ///
    /// # Errors
    ///
    /// [`LunaError::Session`] when the channel cannot be opened.
    fn subscribe_with<P>(
        &self,
        uri: &str,
        payload: P,
        options: LunaOptions,
    ) -> Result<Subscription, LunaError>
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
    /// `luna-send` ran, but what it printed was not a reply.
    Reply {
        said: String,
        why: String,
    },
    /// It never finished. Usually a method that subscribes rather than
    /// replying, or a service that has wedged.
    Timeout {
        uri: String,
        after: Duration,
    },
}

impl Display for LunaError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            LunaError::Session(e) => write!(f, "{e}"),
            LunaError::Io(e) => write!(f, "{e}"),
            LunaError::NotAvailable => write!(f, "luna service is not available"),
            LunaError::Reply { said, why } => {
                write!(f, "not a luna reply ({why}): {said}")
            }
            LunaError::Timeout { uri, after } => {
                write!(f, "{uri} did not answer within {after:?}")
            }
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{DEFAULT_TIMEOUT, LunaOptions};

    #[test]
    fn the_default_is_the_public_bus_and_a_bounded_wait() {
        let o = LunaOptions::new();
        assert!(o.public);
        assert_eq!(o.timeout, Some(DEFAULT_TIMEOUT));
    }

    #[test]
    fn private_flips_the_bus_and_keeps_the_bound() {
        let o = LunaOptions::private();
        assert!(!o.public);
        assert_eq!(o.timeout, Some(DEFAULT_TIMEOUT));
        assert_eq!(o.program(), "luna-send");
        assert_eq!(LunaOptions::new().program(), "luna-send-pub");
    }

    #[test]
    fn the_builders_compose_and_none_means_forever() {
        let o = LunaOptions::new().public(false).timeout(None);
        assert!(!o.public);
        assert_eq!(o.timeout, None);
        let o = LunaOptions::private().timeout(Some(Duration::from_secs(1)));
        assert_eq!(o.timeout, Some(Duration::from_secs(1)));
    }
}
