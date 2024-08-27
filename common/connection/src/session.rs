use std::fmt::Debug;
use std::io::Error as IoError;
use std::ops::Deref;
use std::time::Duration;

use libssh_rs::{AuthStatus, Error as SshError, Session, SshKey, SshOption};

use ares_device_lib::Device;

pub trait NewSession {
    fn new_session(&self) -> Result<DeviceSession, SessionError>;
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
