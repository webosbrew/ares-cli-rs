use std::fmt::{Display, Formatter};
use std::io::{Error as IoError, Read, Write};
use std::path::Path;

use libssh_rs::{Error as SshError, FileType, OpenFlags, Sftp};
use path_slash::PathExt;

use crate::session::SshConnection;

pub trait FileTransfer {
    fn maybe_sftp(&self) -> Result<Sftp, libssh_rs::Error>;
    fn mkdir<P: AsRef<Path>>(&self, dir: &mut P, mode: u32) -> Result<(), TransferError>;
    fn put<P: AsRef<Path>, R: Read, F: Fn(usize)>(
        &self,
        source: &mut R,
        target: P,
        progress: F,
    ) -> Result<(), TransferError>;
    fn get<P: AsRef<Path>, W: Write>(&self, source: P, target: &mut W)
    -> Result<(), TransferError>;

    fn rm<P: AsRef<Path>>(&self, path: P) -> Result<(), TransferError>;

    /// Read the sha256 of a file on the device, as lowercase hex.
    /// Returns `None` if the device has no usable `sha256sum` command.
    fn sha256sum<P: AsRef<Path>>(&self, path: P) -> Result<Option<String>, TransferError>;
}

#[derive(Debug)]
pub enum TransferError {
    ExitCode { code: i32, reason: String },
    Ssh(SshError),
    Io(IoError),
}

impl Display for TransferError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TransferError::ExitCode { reason, .. } => write!(f, "{reason}"),
            TransferError::Ssh(e) => write!(f, "{e}"),
            TransferError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for TransferError {}

impl<T: SshConnection> FileTransfer for T {
    fn maybe_sftp(&self) -> Result<Sftp, libssh_rs::Error> {
        if !self.supports_sftp() {
            return Err(libssh_rs::Error::RequestDenied(
                "SFTP is not supported".to_string(),
            ));
        }
        self.session().sftp()
    }
    fn mkdir<P: AsRef<Path>>(&self, dir: &mut P, mode: u32) -> Result<(), TransferError> {
        if let Ok(sftp) = self.maybe_sftp() {
            if let Ok(Some(file_type)) = sftp
                .metadata(dir.as_ref().to_slash_lossy().as_ref())
                .map(|m| m.file_type())
            {
                if file_type == FileType::Directory {
                    return Ok(());
                }
                return Err(TransferError::ExitCode {
                    code: 1,
                    reason: format!(
                        "File {} exists and is not a directory",
                        dir.as_ref().to_slash_lossy()
                    ),
                });
            }
            sftp.create_dir(dir.as_ref().to_slash_lossy().as_ref(), mode)?;
        } else {
            let command =
                mkdir_command(dir.as_ref().to_slash_lossy().as_ref(), mode, self.is_root());
            let ch = self.session().new_channel()?;
            ch.open_session()?;
            ch.request_exec(&command)?;
            ch.send_eof()?;
            let result_code = ch.get_exit_status().unwrap_or(0) as i32;
            ch.close()?;
            if result_code != 0 {
                return Err(TransferError::ExitCode {
                    code: result_code,
                    reason: format!("mkdir command exited with status {result_code}"),
                });
            }
        }
        Ok(())
    }

    fn put<P: AsRef<Path>, R: Read, F: Fn(usize)>(
        &self,
        source: &mut R,
        target: P,
        progress: F,
    ) -> Result<(), TransferError> {
        if let Ok(sftp) = self.maybe_sftp() {
            let mut file = sftp.open(
                target.as_ref().to_slash_lossy().as_ref(),
                OpenFlags::WRITE_ONLY | OpenFlags::CREATE | OpenFlags::TRUNCATE,
                0o644,
            )?;
            copy_with_progress(source, &mut file, progress)?;
        } else {
            let ch = self.session().new_channel()?;
            ch.open_session()?;
            ch.request_exec(&format!(
                "cat > {}",
                snailquote::escape(target.as_ref().to_slash_lossy().as_ref())
            ))?;
            copy_with_progress(source, &mut ch.stdin(), progress)?;
            ch.send_eof()?;
            let result_code = ch.get_exit_status().unwrap_or(0) as i32;
            ch.close()?;
            if result_code != 0 {
                return Err(TransferError::ExitCode {
                    code: result_code,
                    reason: format!("cat command exited with status {result_code}"),
                });
            }
        }
        Ok(())
    }

    fn get<P: AsRef<Path>, W: Write>(
        &self,
        source: P,
        target: &mut W,
    ) -> Result<(), TransferError> {
        if let Ok(sftp) = self.maybe_sftp() {
            let mut file = sftp.open(
                source.as_ref().to_slash_lossy().as_ref(),
                OpenFlags::READ_ONLY,
                0,
            )?;
            std::io::copy(&mut file, target)?;
        } else {
            let ch = self.session().new_channel()?;
            ch.open_session()?;
            ch.request_exec(&format!(
                "cat {}",
                snailquote::escape(source.as_ref().to_slash_lossy().as_ref())
            ))?;
            std::io::copy(&mut ch.stdout(), target)?;
            let result_code = ch.get_exit_status().unwrap_or(0) as i32;
            ch.close()?;
            if result_code != 0 {
                return Err(TransferError::ExitCode {
                    code: result_code,
                    reason: format!("cat command exited with status {result_code}"),
                });
            }
        }
        Ok(())
    }

    fn rm<P: AsRef<Path>>(&self, path: P) -> Result<(), TransferError> {
        if let Ok(sftp) = self.maybe_sftp() {
            sftp.remove_file(path.as_ref().to_slash_lossy().as_ref())?;
        } else {
            let ch = self.session().new_channel()?;
            ch.open_session()?;
            ch.request_exec(&format!(
                "rm -rf {}",
                snailquote::escape(path.as_ref().to_slash_lossy().as_ref())
            ))?;
            ch.send_eof()?;
            let result_code = ch.get_exit_status().unwrap_or(0) as i32;
            ch.close()?;
            if result_code != 0 {
                return Err(TransferError::ExitCode {
                    code: result_code,
                    reason: format!("rm command exited with status {result_code}"),
                });
            }
        }
        Ok(())
    }

    fn sha256sum<P: AsRef<Path>>(&self, path: P) -> Result<Option<String>, TransferError> {
        let ch = self.session().new_channel()?;
        ch.open_session()?;
        ch.request_exec(&format!(
            "sha256sum {}",
            snailquote::escape(path.as_ref().to_slash_lossy().as_ref())
        ))?;
        let mut buf = String::new();
        ch.stdout().read_to_string(&mut buf)?;
        let result_code = ch.get_exit_status().unwrap_or(0) as i32;
        ch.close()?;
        if result_code != 0 {
            // Some devices have no sha256sum. Report "can't tell" instead of an error.
            return Ok(None);
        }
        Ok(parse_sha256sum(&buf))
    }
}

impl From<IoError> for TransferError {
    fn from(value: IoError) -> Self {
        Self::Io(value)
    }
}
impl From<SshError> for TransferError {
    fn from(value: SshError) -> Self {
        Self::Ssh(value)
    }
}

fn copy_with_progress<R: Read, W: Write, F: Fn(usize)>(
    source: &mut R,
    target: &mut W,
    progress: F,
) -> Result<(), TransferError> {
    let mut buffer = [0u8; 1024 * 8];
    let mut total = 0usize;
    loop {
        let read = source.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        target.write_all(&buffer[..read])?;
        total += read;
        progress(total);
    }
    Ok(())
}

/// Build the shell command that makes `dir`.
///
/// Only root gets the chmod. A non-root user can't change the mode of a directory
/// somebody else owns, and that directory is usually already usable, so trying
/// would fail the whole transfer for nothing.
fn mkdir_command(dir: &str, mode: u32, is_root: bool) -> String {
    let path = snailquote::escape(dir).to_string();
    if is_root {
        format!("(test -d {path} || mkdir -p {path}) && chmod {mode:o} {path}")
    } else {
        format!("test -d {path} || mkdir -p {path}")
    }
}

/// Read the digest out of `sha256sum` output, which looks like "<hex>  <path>".
/// Returns `None` when the output is not a sha256, so a device without the
/// command doesn't fail the transfer.
fn parse_sha256sum(stdout: &str) -> Option<String> {
    stdout
        .split_whitespace()
        .next()
        .filter(|s| s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()))
        .map(str::to_lowercase)
}

#[cfg(test)]
mod tests {
    use libssh_rs::Session;

    use super::{FileTransfer, TransferError, mkdir_command, parse_sha256sum};
    use crate::session::SshConnection;

    /// A connection handed out by a pool, as dev-manager-desktop has. It is not a
    /// [`crate::session::DeviceSession`], so it proves `FileTransfer` is reusable.
    struct PooledConnection {
        session: Session,
        uid: u32,
    }

    impl SshConnection for PooledConnection {
        fn session(&self) -> &Session {
            &self.session
        }

        fn supports_sftp(&self) -> bool {
            true
        }

        // A pool can know the real uid, which beats guessing from the user name.
        fn is_root(&self) -> bool {
            self.uid == 0
        }
    }

    fn assert_file_transfer<T: FileTransfer>() {}

    #[test]
    fn pooled_connection_can_transfer_files() {
        assert_file_transfer::<PooledConnection>();
    }

    #[test]
    fn root_gets_a_chmod() {
        assert_eq!(
            mkdir_command("/media/developer/temp", 0o777, true),
            concat!(
                "(test -d /media/developer/temp || mkdir -p /media/developer/temp)",
                " && chmod 777 /media/developer/temp"
            )
        );
    }

    #[test]
    fn a_normal_user_does_not_chmod() {
        // The directory is often left over from a root install, and chmod on
        // somebody else's directory fails and would abort the whole transfer.
        assert_eq!(
            mkdir_command("/media/developer/temp", 0o777, false),
            "test -d /media/developer/temp || mkdir -p /media/developer/temp"
        );
    }

    #[test]
    fn a_path_with_spaces_is_quoted() {
        let command = mkdir_command("/media/developer/my temp", 0o755, true);
        assert!(command.contains("'/media/developer/my temp'"), "{command}");
        assert!(command.contains("chmod 755"), "{command}");
    }

    #[test]
    fn sha256sum_output_yields_the_digest() {
        let digest = "a".repeat(64);
        assert_eq!(
            parse_sha256sum(&format!(
                "{digest}  /media/developer/temp/app.ipk
"
            )),
            Some(digest.clone())
        );
        // Some builds print uppercase hex.
        assert_eq!(
            parse_sha256sum(&format!("{}  file", digest.to_uppercase())),
            Some(digest)
        );
    }

    #[test]
    fn output_that_is_not_a_digest_is_ignored() {
        // A device with no sha256sum must skip the check, not fail the install.
        for output in [
            "",
            "
",
            "sh: sha256sum: not found
",
            "abc123  file
",
            &format!("{}  file", "z".repeat(64)),
        ] {
            assert_eq!(parse_sha256sum(output), None, "{output:?}");
        }
    }

    #[test]
    fn errors_read_as_sentences() {
        assert_eq!(
            TransferError::ExitCode {
                code: 1,
                reason: String::from("mkdir command exited with status 1"),
            }
            .to_string(),
            "mkdir command exited with status 1"
        );
    }
}
