//! Every way `ares-do` can fail, and the exit code each one earns.
//!
//! The codes are a published contract — see the README table. A caller,
//! especially an unattended one, branches on them, so a failure must never be
//! flattened into a generic 1. That is why nothing here uses
//! [`ares_device_lib::cli::unwrap_or_exit`], which always exits 1.

use std::fmt::{Display, Formatter};
use std::io::Error as IoError;

use ares_connection_lib::luna::LunaError;
use ares_connection_lib::session::SessionError;
use ares_connection_lib::transfer::TransferError;

use crate::flow::FlowError;

#[derive(Debug)]
pub(crate) enum DoError {
    /// The device list could not be read.
    DeviceLookup(IoError),
    /// No device by that name, and no default.
    DeviceNotFound { name: Option<String> },
    /// Reached the device, but could not connect or authenticate.
    Connect {
        device: String,
        source: SessionError,
    },
    /// The private luna bus needs root and this session does not have it.
    NotRoot {
        device: String,
        user: String,
        uid: Option<u32>,
    },
    /// The call itself failed: no such service, no permission, no reply in
    /// time. [`LunaError`] says which.
    Luna { uri: String, source: LunaError },
    /// The service answered, and said no.
    LunaFailed {
        uri: String,
        /// Rendered, not typed: services disagree on whether this is a number.
        code: Option<String>,
        text: String,
    },
    /// The capture ran but produced nothing usable.
    Capture(String),
    /// Moving the file off the device failed.
    Transfer(TransferError),
    /// Something on this machine failed: writing a PNG, reading a flow.
    Io(IoError),
    /// The flow did not parse. Carries every error found, not just the first.
    Flow(Vec<FlowError>),
    /// Waited for something that never happened.
    Timeout(String),
    /// Bad input that clap did not catch, such as untypeable text.
    Usage(String),
    /// `exec` ran a command and it failed. Carries the command's own status.
    RemoteCommand { command: String, code: i32 },
    /// `exec` could not run the command at all, or it never finished.
    Exec(String),
}

impl DoError {
    /// A call that never answered is a timeout first and a luna problem
    /// second, because a caller retries the two differently.
    fn timed_out(&self) -> bool {
        matches!(
            self,
            DoError::Luna {
                source: LunaError::Timeout { .. },
                ..
            }
        )
    }

    /// The process exit code. Documented in the README; keep the two in step.
    pub(crate) fn exit_code(&self) -> i32 {
        match self {
            DoError::Usage(_) | DoError::Flow(_) => 2,
            DoError::DeviceNotFound { .. } | DoError::DeviceLookup(_) => 3,
            DoError::Connect { .. } => 4,
            DoError::NotRoot { .. } => 5,
            DoError::Luna { .. } if self.timed_out() => 9,
            DoError::Luna { .. } => 6,
            DoError::LunaFailed { .. } => 7,
            DoError::Io(_) | DoError::Transfer(_) => 8,
            DoError::Capture(_) | DoError::Timeout(_) | DoError::Exec(_) => 9,
            // Forwarded, not from this table — see the README.
            DoError::RemoteCommand { code, .. } => *code,
        }
    }

    /// A stable tag for the `--json` `error` event, so a caller reading the
    /// stream never has to match on prose.
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            DoError::Usage(_) => "usage",
            DoError::Flow(_) => "flow_syntax",
            DoError::DeviceLookup(_) | DoError::DeviceNotFound { .. } => "device_not_found",
            DoError::Connect { .. } => "connect_failed",
            DoError::NotRoot { .. } => "not_root",
            DoError::Luna { .. } if self.timed_out() => "timeout",
            DoError::Luna { .. } => "luna_unavailable",
            DoError::LunaFailed { .. } => "luna_failed",
            DoError::Transfer(_) | DoError::Io(_) => "io",
            DoError::Capture(_) => "capture_failed",
            DoError::Timeout(_) => "timeout",
            DoError::Exec(_) => "exec_failed",
            DoError::RemoteCommand { .. } => "remote_command",
        }
    }
}

impl Display for DoError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DoError::DeviceLookup(e) => write!(f, "Failed to read the device list: {e}"),
            DoError::DeviceNotFound { name: Some(name) } => write!(
                f,
                "Device \"{name}\" not found. Run `ares-setup-device --device-list` to see \
                 what is configured."
            ),
            DoError::DeviceNotFound { name: None } => write!(
                f,
                "No default device. Pass -d NAME, set ARES_DEVICE, or run `ares-setup-device`."
            ),
            DoError::Connect { device, source } => {
                write!(f, "Failed to connect to {device}: {source}")
            }
            DoError::NotRoot { device, user, uid } => {
                let uid = uid.map_or_else(|| String::from("unknown uid"), |u| format!("uid {u}"));
                write!(
                    f,
                    "ares-do needs a root shell on the device.\n  \
                     {device} is connected as {user} ({uid}).\n  \
                     Key injection and screen capture are on the private luna bus, which only \
                     root may use.\n  \
                     Root the device, or point the entry at a root account:\n      \
                     ares-setup-device -m {device} -i username=root -i port=22\n  \
                     Pass --allow-non-root to try anyway."
                )
            }
            DoError::Luna { uri, source } => {
                write!(f, "Could not call {uri}: {source}")?;
                if matches!(source, LunaError::NotAvailable | LunaError::Command { .. }) {
                    write!(
                        f,
                        "\n  The service may be missing, or this user may not be allowed on \
                         the private bus."
                    )?;
                }
                Ok(())
            }
            DoError::LunaFailed { uri, code, text } => {
                write!(f, "{uri} refused the call: {text}")?;
                if let Some(code) = code {
                    write!(f, " ({code})")?;
                }
                Ok(())
            }
            DoError::Capture(message) => write!(f, "Screen capture failed: {message}"),
            DoError::Transfer(e) => write!(f, "File transfer failed: {e}"),
            DoError::Io(e) => write!(f, "{e}"),
            DoError::Timeout(what) => write!(f, "Timed out waiting for {what}"),
            DoError::Usage(message) | DoError::Exec(message) => write!(f, "{message}"),
            DoError::RemoteCommand { command, code } => {
                write!(f, "`{command}` exited {code} on the device")
            }
            DoError::Flow(errors) => {
                for (i, error) in errors.iter().enumerate() {
                    if i > 0 {
                        writeln!(f)?;
                    }
                    write!(f, "{error}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for DoError {}

impl From<IoError> for DoError {
    fn from(value: IoError) -> Self {
        DoError::Io(value)
    }
}

impl From<TransferError> for DoError {
    fn from(value: TransferError) -> Self {
        DoError::Transfer(value)
    }
}
