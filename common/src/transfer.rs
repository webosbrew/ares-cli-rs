use std::io::{Error as IoError, Read, Write};
use std::path::Path;

use libssh_rs::Error as SshError;
use libssh_rs::Session;
use path_slash::PathExt;

pub trait FileTransfer {
    fn put<P: AsRef<Path>, R: Read>(&self, source: &mut R, target: P) -> Result<(), TransferError>;
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

impl FileTransfer for Session {
    fn put<P: AsRef<Path>, R: Read>(&self, source: &mut R, target: P) -> Result<(), TransferError> {
        println!("Copying file to {}...", target.as_ref().to_slash_lossy());
        let ch = self.new_channel()?;
        ch.open_session()?;
        ch.request_exec(&format!(
            "cat > {}",
            snailquote::escape(target.as_ref().to_slash_lossy().as_ref())
        ))?;
        std::io::copy(source, &mut ch.stdin())?;
        ch.send_eof()?;
        ch.close()?;
        println!("Copied!");
        return Ok(());
    }

    fn get<P: AsRef<Path>, W: Write>(
        &self,
        source: P,
        target: &mut W,
    ) -> Result<(), TransferError> {
        todo!()
    }

    fn rm<P: AsRef<Path>>(&self, path: P) -> Result<(), TransferError> {
        println!("Removing file {}...", path.as_ref().to_slash_lossy());
        let ch = self.new_channel()?;
        ch.open_session()?;
        ch.request_exec(&format!(
            "rm -rf {}",
            snailquote::escape(path.as_ref().to_slash_lossy().as_ref())
        ))?;
        ch.send_eof()?;
        ch.close()?;
        println!("Removed!");
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
