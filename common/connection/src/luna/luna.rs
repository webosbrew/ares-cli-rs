use std::io::{Error as IoError, ErrorKind};

use libssh_rs::{Error as SshError, Session};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Error as JsonError;

use crate::exec::{Exec, ExecError};
use crate::luna::{Luna, LunaError, LunaOptions, Subscription};
use crate::session::SessionError;

impl Luna for Session {
    fn call_with<P, R>(&self, uri: &str, payload: P, options: LunaOptions) -> Result<R, LunaError>
    where
        P: Sized + Serialize,
        R: DeserializeOwned,
    {
        let payload_str = serde_json::to_string(&payload)?;
        let command = format!(
            "{} -n 1 {} {}",
            options.program(),
            snailquote::escape(uri),
            snailquote::escape(&payload_str)
        );

        let output = self
            .exec_timeout(&command, options.timeout)
            .map_err(|e| match e {
                ExecError::Timeout { after, .. } => LunaError::Timeout {
                    uri: uri.to_string(),
                    after,
                },
                ExecError::Ssh(e) => LunaError::Session(e.into()),
            })?;

        if !output.success() {
            // Keep NotAvailable for the case it was always meant for: it
            // failed and said nothing at all about why.
            if output.complaint() == "no output" {
                return Err(LunaError::NotAvailable);
            }
            return Err(LunaError::Command {
                exit_code: output.exit_code,
                stdout: output.stdout,
                stderr: output.stderr,
            });
        }

        // Some builds print a warning line before the reply, so take the last
        // line that looks like one rather than all of stdout.
        let reply = output
            .stdout
            .lines()
            .map(str::trim)
            .rfind(|l| l.starts_with('{'))
            .unwrap_or("");

        serde_json::from_str(reply).map_err(|e| LunaError::Reply {
            said: if reply.is_empty() {
                output.complaint().to_string()
            } else {
                reply.to_string()
            },
            why: e.to_string(),
        })
    }

    fn subscribe_with<P>(
        &self,
        uri: &str,
        payload: P,
        options: LunaOptions,
    ) -> Result<Subscription, LunaError>
    where
        P: Sized + Serialize,
    {
        let ch = self.new_channel()?;
        ch.open_session()?;
        let payload_str = serde_json::to_string(&payload)?;
        ch.request_exec(&format!(
            "{} -i {} {}",
            options.program(),
            snailquote::escape(uri),
            snailquote::escape(&payload_str)
        ))?;
        Ok(Subscription {
            ch,
            buffer: Vec::new(),
        })
    }
}

impl From<SshError> for LunaError {
    fn from(value: SshError) -> Self {
        Self::Session(value.into())
    }
}

impl From<SessionError> for LunaError {
    fn from(value: SessionError) -> Self {
        Self::Session(value)
    }
}

impl From<JsonError> for LunaError {
    fn from(value: JsonError) -> Self {
        Self::Io(IoError::new(
            ErrorKind::InvalidData,
            format!("Invalid JSON: {value:?}"),
        ))
    }
}

impl From<IoError> for LunaError {
    fn from(value: IoError) -> Self {
        Self::Io(value)
    }
}
