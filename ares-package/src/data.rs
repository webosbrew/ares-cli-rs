use std::io::{Cursor, Write as IoWrite};
use std::ops::Deref;
use std::time::SystemTime;

use ar::{Builder as ArBuilder, Header as ArHeader};
use flate2::Compression;
use flate2::write::GzEncoder;
use tar::{Builder as TarBuilder, Header as TarHeader};

use crate::PackageInfo;

pub(crate) trait AppendData {
    fn append_data(&mut self, info: &PackageInfo) -> std::io::Result<()>;
}

impl<W> AppendData for ArBuilder<W> where W: IoWrite {
    fn append_data(&mut self, info: &PackageInfo) -> std::io::Result<()> {
        let package_info = serde_json::to_vec(info).unwrap();

        let mut data_tar_gz = Vec::<u8>::new();
        let gz = GzEncoder::new(&mut data_tar_gz, Compression::default());
        let mut builder = TarBuilder::new(gz);

        let mut header = TarHeader::new_gnu();
        header.set_mode(0o644);
        header.set_size(package_info.len() as u64);
        header.set_mtime(SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs());
        header.set_cksum();
        builder.append_data(&mut header, format!("usr/palm/packages/{}/packageinfo.json", info.id),
                            package_info.deref())?;
        drop(builder);
        return self.append(&ArHeader::new(b"data.tar.gz".to_vec(), data_tar_gz.len() as u64),
                           Cursor::new(data_tar_gz));
    }
}
