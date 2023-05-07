use std::io::{Result, Write as IoWrite};
use std::ops::Deref;

use ar::{Builder as ArBuilder, Header};

pub trait AppendHeader {
    fn append_header(&mut self) -> Result<()>;
}

impl<W> AppendHeader for ArBuilder<W>
where
    W: IoWrite,
{
    fn append_header(&mut self) -> Result<()> {
        let debian_binary = b"2.0\n".to_vec();

        self.append(
            &Header::new(b"debian-binary".to_vec(), debian_binary.len() as u64),
            debian_binary.deref(),
        )
    }
}
