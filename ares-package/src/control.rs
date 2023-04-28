use std::fmt::{Display, Formatter};
use std::io::{Cursor, Write as IoWrite};
use std::ops::Deref;

use ar::{Builder as ArBuilder, Header as ArHeader};
use flate2::write::GzEncoder;
use flate2::Compression;
use tar::{Builder as TarBuilder, Header as TarHeader};

pub(crate) struct ControlInfo {
    pub package: String,
    pub version: String,
    pub architecture: String,
}

pub(crate) trait AppendControl {
    fn append_control(&mut self, info: &ControlInfo) -> std::io::Result<()>;
}

impl<W> AppendControl for ArBuilder<W>
where
    W: IoWrite,
{
    fn append_control(&mut self, info: &ControlInfo) -> std::io::Result<()> {
        let control = info.to_string().into_bytes();

        let mut control_tar_gz = Vec::<u8>::new();
        let gz = GzEncoder::new(&mut control_tar_gz, Compression::default());
        let mut builder = TarBuilder::new(gz);

        let mut header = TarHeader::new_gnu();
        header.set_mode(0o644);
        header.set_size(control.len() as u64);
        header.set_cksum();
        builder.append_data(&mut header, "control", control.deref())?;
        drop(builder);
        return self.append(
            &ArHeader::new(b"control.tar.gz".to_vec(), control_tar_gz.len() as u64),
            Cursor::new(control_tar_gz),
        );
    }
}

impl Display for ControlInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("Package: {}\n", self.package))
            .unwrap();
        f.write_fmt(format_args!("Version: {}\n", self.version))
            .unwrap();
        f.write_fmt(format_args!("Section: {}\n", "misc")).unwrap();
        f.write_fmt(format_args!("Priority: {}\n", "optional"))
            .unwrap();
        f.write_fmt(format_args!("Architecture: {}\n", self.architecture))
            .unwrap();
        f.write_fmt(format_args!("Installed-Size: {}\n", 0))
            .unwrap();
        f.write_fmt(format_args!("Maintainer: {}\n", "N/A <nobody@example.com>"))
            .unwrap();
        f.write_fmt(format_args!(
            "Description: {}\n",
            "This is a webOS application."
        ))
        .unwrap();
        f.write_fmt(format_args!("webOS-Package-Format-Version: {}\n", 2))
            .unwrap();
        f.write_fmt(format_args!("webOS-Packager-Version: {}\n", "x.y.x"))
            .unwrap();
        return Ok(());
    }
}
