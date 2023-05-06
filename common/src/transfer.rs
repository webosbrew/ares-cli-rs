use std::io::{Read, Write};
use std::path::Path;

use crate::session::SessionError;
use libssh_rs::Session;
use path_slash::PathExt;

pub trait FileTransfer {
    fn put<P: AsRef<Path>, R: Read>(&self, source: &mut R, target: P) -> Result<(), SessionError>;
    fn get<P: AsRef<Path>, W: Write>(&self, source: P, target: &mut W) -> Result<(), SessionError>;

    fn rm<P: AsRef<Path>>(&self, path: P) -> Result<(), SessionError>;
}

impl FileTransfer for Session {
    fn put<P: AsRef<Path>, R: Read>(&self, source: &mut R, target: P) -> Result<(), SessionError> {
        let ch = self.new_channel()?;
        ch.open_session()?;
        ch.request_exec(&format!(
            "cat > {}",
            snailquote::escape(target.as_ref().to_slash_lossy().as_ref())
        ))?;
        std::io::copy(source, &mut ch.stdin())?;
        ch.send_eof()?;
        return Ok(());
    }

    fn get<P: AsRef<Path>, W: Write>(&self, source: P, target: &mut W) -> Result<(), SessionError> {
        todo!()
    }

    fn rm<P: AsRef<Path>>(&self, path: P) -> Result<(), SessionError> {
        let ch = self.new_channel()?;
        ch.open_session()?;
        ch.request_exec(&format!(
            "rm -rf {}",
            snailquote::escape(path.as_ref().to_slash_lossy().as_ref())
        ))?;
        ch.send_eof()?;
        return Ok(());
    }
}
