use std::io::{Read, Write};
use std::path::Path;

use libssh_rs::Session;

use crate::session::SessionError;

pub trait FileTransfer {
    fn put<P: AsRef<Path>, R: Read>(&self, source: &mut R, target: P) -> Result<(), SessionError>;
    fn get<P: AsRef<Path>, W: Write>(&self, source: P, target: &mut W) -> Result<(), SessionError>;
}

impl FileTransfer for Session {
    fn put<P: AsRef<Path>, R: Read>(&self, source: &mut R, target: P) -> Result<(), SessionError> {
        let ch = self.new_channel()?;
        ch.open_session()?;
        ch.request_exec(&format!("cat > {}", snailquote::escape(target.as_ref().to_string_lossy().as_ref())))?;
        std::io::copy(source, &mut ch.stdin())?;
        ch.send_eof()?;
        return Ok(());
    }

    fn get<P: AsRef<Path>, W: Write>(&self, source: P, target: &mut W) -> Result<(), SessionError> {
        todo!()
    }
}