use crate::device_manager::Device;
use libssh_rs::{AuthStatus, Error as SshError, Session, SshKey, SshOption};
use std::fmt::{Debug, Formatter};
use std::io::Error as IoError;
use std::time::Duration;

pub trait NewSession {
    fn new_session(&self) -> Result<Session, NewSessionError>;
}

#[derive(Debug)]
pub enum NewSessionError {
    Io(IoError),
    LibSsh(SshError),
    Authorization { message: String },
}

impl NewSession for Device {
    fn new_session(&self) -> Result<Session, NewSessionError> {
        let session = Session::new()?;
        session.set_option(SshOption::Timeout(Duration::from_secs(10)))?;
        session.set_option(SshOption::Hostname(self.host.clone()))?;
        session.set_option(SshOption::Port(self.port.clone()))?;
        session.set_option(SshOption::User(Some(self.username.clone())))?;
        session.set_option(SshOption::HostKeys(format!("ssh-ed25519,ecdsa-sha2-nistp521,ecdsa-sha2-nistp384,ecdsa-sha2-nistp256,rsa-sha2-512,rsa-sha2-256,ssh-rsa")))?;
        session.set_option(SshOption::PublicKeyAcceptedTypes(format!("ssh-ed25519,ecdsa-sha2-nistp521,ecdsa-sha2-nistp384,ecdsa-sha2-nistp256,rsa-sha2-512,rsa-sha2-256,ssh-rsa")))?;
        session.set_option(SshOption::ProcessConfig(false))?;
        #[cfg(windows)]
        {
            session.set_option(SshOption::KnownHosts(Some(format!("C:\\nul"))))?;
            session.set_option(SshOption::GlobalKnownHosts(Some(format!("C:\\nul"))))?;
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
                return Err(NewSessionError::Authorization {
                    message: format!("Key authorization failed"),
                });
            }
        } else if let Some(password) = &self.password {
            if session.userauth_password(None, Some(password))? != AuthStatus::Success {
                return Err(NewSessionError::Authorization {
                    message: format!("Bad SSH password"),
                });
            }
        } else if session.userauth_none(None)? != AuthStatus::Success {
            return Err(NewSessionError::Authorization {
                message: format!("Host needs authorization"),
            });
        }
        return Ok(session);
    }
}

impl From<SshError> for NewSessionError {
    fn from(value: SshError) -> Self {
        NewSessionError::LibSsh(value)
    }
}

impl From<IoError> for NewSessionError {
    fn from(value: IoError) -> Self {
        NewSessionError::Io(value)
    }
}
