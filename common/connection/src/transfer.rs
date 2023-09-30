use std::io::{Error as IoError, Read, Write};
use std::path::Path;

use libssh_rs::Sftp;
use libssh_rs::{Error as SshError, FileType};
use path_slash::PathExt;

use ares_device_lib::FileTransfer::Stream;

use crate::session::DeviceSession;

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
}

#[derive(Debug)]
pub enum TransferError {
    ExitCode { code: i32, reason: String },
    Ssh(SshError),
    Io(IoError),
}

impl FileTransfer for DeviceSession {
    fn maybe_sftp(&self) -> Result<Sftp, libssh_rs::Error> {
        if Some(Stream) == self.device.files {
            return Err(libssh_rs::Error::RequestDenied(
                "SFTP is not supported".to_string(),
            ));
        }
        return self.sftp();
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
            let ch = self.new_channel()?;
            ch.open_session()?;
            ch.request_exec(&format!(
                "mkdir -p {} && chmod {mode:o} {}",
                snailquote::escape(dir.as_ref().to_slash_lossy().as_ref()),
                snailquote::escape(dir.as_ref().to_slash_lossy().as_ref())
            ))?;
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
        return Ok(());
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
                0o1101, /*O_WRONLY | O_CREAT | O_TRUNC on Linux*/
                0o644,
            )?;
            copy_with_progress(source, &mut file, progress)?;
        } else {
            let ch = self.new_channel()?;
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
        return Ok(());
    }

    fn get<P: AsRef<Path>, W: Write>(
        &self,
        source: P,
        target: &mut W,
    ) -> Result<(), TransferError> {
        if let Ok(sftp) = self.maybe_sftp() {
            let mut file = sftp.open(source.as_ref().to_slash_lossy().as_ref(), 0, 0)?;
            std::io::copy(&mut file, target)?;
        } else {
            let ch = self.new_channel()?;
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
        return Ok(());
    }

    fn rm<P: AsRef<Path>>(&self, path: P) -> Result<(), TransferError> {
        if let Ok(sftp) = self.maybe_sftp() {
            sftp.remove_file(path.as_ref().to_slash_lossy().as_ref())?;
        } else {
            let ch = self.new_channel()?;
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
        return Ok(());
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
    return Ok(());
}
