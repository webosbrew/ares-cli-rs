use std::io::{Result, Write as IoWrite};
use std::ops::Deref;

use ar::{Builder as ArBuilder, Header};

pub trait AppendHeader {
    fn append_header(&mut self, mtime:u64) -> Result<()>;
}

impl<W> AppendHeader for ArBuilder<W>
where
    W: IoWrite,
{
    fn append_header(&mut self, mtime:u64) -> Result<()> {
        let debian_binary = b"2.0\n".to_vec();

        let mut header = Header::new(b"debian-binary".to_vec(), debian_binary.len() as u64);
        header.set_mode(0o100644);
        header.set_mtime(mtime);
        self.append(&header, debian_binary.deref())
    }
}
