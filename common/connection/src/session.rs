use std::fmt::{Debug, Display, Formatter};
use std::io::Error as IoError;
use std::ops::Deref;
use std::time::Duration;

use ares_device_lib::{Device, FileTransfer};
use libssh_rs::{AuthStatus, Error as SshError, Session, SshKey, SshOption};

pub trait NewSession {
    fn new_session(&self) -> Result<DeviceSession, SessionError>;
}

/// A live SSH connection to a device.
///
/// [`DeviceSession`] is the one connection per run that the ares-cli-rs tools use.
/// Implement this trait for your own type to reuse [`crate::transfer::FileTransfer`]
/// with a connection that comes from somewhere else, such as a connection pool.
///
/// The trait asks only for what file transfer needs, so it does not tie you to
/// this crate's [`Device`] type.
pub trait SshConnection {
    fn session(&self) -> &Session;

    /// `false` when files have to stream over an exec channel instead of SFTP.
    fn supports_sftp(&self) -> bool;

    /// `true` when this user may chmod any path. A non-root user can't change the
    /// mode of a directory somebody else owns, so `mkdir` leaves the mode alone.
    fn is_root(&self) -> bool;
}

impl SshConnection for DeviceSession {
    fn session(&self) -> &Session {
        &self.session
    }

    fn supports_sftp(&self) -> bool {
        self.device.files != Some(FileTransfer::Stream)
    }

    fn is_root(&self) -> bool {
        self.device.username == "root"
    }
}

pub struct DeviceSession {
    pub device: Device,
    pub session: Session,
}

#[derive(Debug)]
pub enum SessionError {
    Io(IoError),
    LibSsh(SshError),
    Authorization { message: String },
}

impl Display for SessionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionError::Io(e) => write!(f, "{e}"),
            SessionError::LibSsh(e) => write!(f, "{e}"),
            SessionError::Authorization { message } => write!(f, "not authorized: {message}"),
        }
    }
}

impl std::error::Error for SessionError {}

impl NewSession for Device {
    fn new_session(&self) -> Result<DeviceSession, SessionError> {
        let kex = vec![
            "curve25519-sha256",
            "curve25519-sha256@libssh.org",
            "ecdh-sha2-nistp256",
            "ecdh-sha2-nistp384",
            "ecdh-sha2-nistp521",
            "diffie-hellman-group18-sha512",
            "diffie-hellman-group16-sha512",
            "diffie-hellman-group-exchange-sha256",
            "diffie-hellman-group14-sha256",
            "diffie-hellman-group1-sha1",
            "diffie-hellman-group14-sha1",
        ];
        let hmac = vec![
            "hmac-sha2-256-etm@openssh.com",
            "hmac-sha2-512-etm@openssh.com",
            "hmac-sha2-256",
            "hmac-sha2-512",
            "hmac-sha1-96",
            "hmac-sha1",
            "hmac-md5",
        ];
        let key_types = vec![
            "ssh-ed25519",
            "ecdsa-sha2-nistp521",
            "ecdsa-sha2-nistp384",
            "ecdsa-sha2-nistp256",
            "rsa-sha2-512",
            "rsa-sha2-256",
            "ssh-rsa",
        ];
        let session = Session::new()?;
        session.set_option(SshOption::Timeout(Duration::from_secs(10)))?;
        session.set_option(SshOption::Hostname(self.host.clone()))?;
        session.set_option(SshOption::Port(self.port.clone()))?;
        session.set_option(SshOption::User(Some(self.username.clone())))?;
        session.set_option(SshOption::KeyExchange(kex.join(",")))?;
        session.set_option(SshOption::HmacCS(hmac.join(",")))?;
        session.set_option(SshOption::HmacSC(hmac.join(",")))?;
        session.set_option(SshOption::HostKeys(key_types.join(",")))?;
        session.set_option(SshOption::PublicKeyAcceptedTypes(key_types.join(",")))?;
        session.set_option(SshOption::ProcessConfig(false))?;
        #[cfg(windows)]
        {
            session.set_option(SshOption::KnownHosts(Some("C:\\nul".to_string())))?;
            session.set_option(SshOption::GlobalKnownHosts(Some("C:\\nul".to_string())))?;
        }

        #[cfg(not(windows))]
        {
            session.set_option(SshOption::KnownHosts(Some(format!("/dev/null"))))?;
            session.set_option(SshOption::GlobalKnownHosts(Some(format!("/dev/null"))))?;
        }

        session.connect()?;

        if let Some(private_key) = &self.private_key {
            let passphrase = self.valid_passphrase();
            let priv_key_content = private_key.content()?;
            let priv_key = SshKey::from_privkey_base64(&priv_key_content, passphrase.as_deref())?;

            if session.userauth_publickey(None, &priv_key)? != AuthStatus::Success {
                return Err(SessionError::Authorization {
                    message: "Key authorization failed".to_string(),
                });
            }
        } else if let Some(password) = &self.password {
            if session.userauth_password(None, Some(password))? != AuthStatus::Success {
                return Err(SessionError::Authorization {
                    message: "Bad SSH password".to_string(),
                });
            }
        } else if session.userauth_none(None)? != AuthStatus::Success {
            return Err(SessionError::Authorization {
                message: "Host needs authorization".to_string(),
            });
        }
        Ok(DeviceSession {
            device: self.clone(),
            session,
        })
    }
}

impl Deref for DeviceSession {
    type Target = Session;

    fn deref(&self) -> &Self::Target {
        &self.session
    }
}

impl From<SshError> for SessionError {
    fn from(value: SshError) -> Self {
        SessionError::LibSsh(value)
    }
}

impl From<IoError> for SessionError {
    fn from(value: IoError) -> Self {
        SessionError::Io(value)
    }
}
