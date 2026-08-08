use std::fmt::{Debug, Display, Formatter};
use std::io::Error as IoError;
use std::ops::Deref;
use std::time::Duration;

use ares_device_lib::{Device, FileTransfer, PrivateKey};
use libssh_rs::{AuthStatus, Error as SshError, Session, SshKey, SshOption};

pub trait NewSession {
    fn new_session(&self) -> Result<DeviceSession, SessionError>;
}

/// Authenticate a connected session as `device`.
///
/// `key` is the private key itself, in OpenSSH format. The caller reads it,
/// because where a [`crate::session::NewSession`] implementer looks for a key
/// name is its own business. Pass `None` to fall back to the password, and to
/// no authentication after that.
///
/// # Errors
///
/// Returns [`SessionError::Authorization`] if the device turns the attempt
/// down, or the libssh error if the key does not parse.
pub fn authenticate(
    session: &Session,
    device: &Device,
    key: Option<&str>,
) -> Result<(), SessionError> {
    let (status, refused) = match (key, &device.password) {
        (Some(key), _) => {
            let key = SshKey::from_privkey_base64(key, device.valid_passphrase().as_deref())?;
            (
                session.userauth_publickey(None, &key)?,
                "Key authorization failed",
            )
        }
        (None, Some(password)) => (
            session.userauth_password(None, Some(password))?,
            "Bad SSH password",
        ),
        (None, None) => (session.userauth_none(None)?, "Host needs authorization"),
    };
    if status == AuthStatus::Success {
        return Ok(());
    }
    Err(SessionError::Authorization {
        message: refused.to_string(),
    })
}

/// Set the timeout, the crypto algorithms and the host-key policy that a webOS
/// device needs.
///
/// A device runs an old SSH server, so the lists below keep algorithms that
/// current defaults drop. Known hosts are off: a device has no stable host key.
///
/// Call this on a new [`Session`], before you set the host, port and user. Use
/// it to build a session yourself, when [`NewSession::new_session`] does not fit
/// because the session is pooled or the key comes from elsewhere.
///
/// # Errors
///
/// Returns the libssh error if an option is rejected.
pub fn configure_session(session: &Session) -> Result<(), SshError> {
    let kex = [
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
    let hmac = [
        "hmac-sha2-256-etm@openssh.com",
        "hmac-sha2-512-etm@openssh.com",
        "hmac-sha2-256",
        "hmac-sha2-512",
        "hmac-sha1-96",
        "hmac-sha1",
        "hmac-md5",
    ];
    let key_types = [
        "ssh-ed25519",
        "ecdsa-sha2-nistp521",
        "ecdsa-sha2-nistp384",
        "ecdsa-sha2-nistp256",
        "rsa-sha2-512",
        "rsa-sha2-256",
        "ssh-rsa",
    ];
    session.set_option(SshOption::Timeout(Duration::from_secs(10)))?;
    session.set_option(SshOption::KeyExchange(kex.join(",")))?;
    session.set_option(SshOption::HmacCS(hmac.join(",")))?;
    session.set_option(SshOption::HmacSC(hmac.join(",")))?;
    session.set_option(SshOption::HostKeys(key_types.join(",")))?;
    session.set_option(SshOption::PublicKeyAcceptedTypes(key_types.join(",")))?;
    session.set_option(SshOption::ProcessConfig(false))?;

    #[cfg(windows)]
    let null_device = "C:\\nul";
    #[cfg(not(windows))]
    let null_device = "/dev/null";
    session.set_option(SshOption::KnownHosts(Some(null_device.to_string())))?;
    session.set_option(SshOption::GlobalKnownHosts(Some(null_device.to_string())))?;
    Ok(())
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
        let session = Session::new()?;
        configure_session(&session)?;
        session.set_option(SshOption::Hostname(self.host.clone()))?;
        session.set_option(SshOption::Port(self.port))?;
        session.set_option(SshOption::User(Some(self.username.clone())))?;

        session.connect()?;

        let key = self
            .private_key
            .as_ref()
            .map(PrivateKey::content)
            .transpose()?;
        authenticate(&session, self, key.as_deref())?;
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
