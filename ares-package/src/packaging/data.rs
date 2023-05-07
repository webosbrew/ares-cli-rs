use std::io::{Cursor, Result, Write as IoWrite, Write};
use std::ops::Deref;
use std::path::Path;
use std::time::SystemTime;

use ar::{Builder as ArBuilder, Header as ArHeader};
use flate2::write::GzEncoder;
use flate2::Compression;
use path_slash::PathExt as _;
use tar::{Builder as TarBuilder, EntryType, Header as TarHeader};

use crate::input::data::DataInfo;

pub trait AppendData {
    fn append_data(&mut self, details: &DataInfo) -> Result<()>;
}

impl<W> AppendData for ArBuilder<W>
where
    W: IoWrite,
{
    fn append_data(&mut self, details: &DataInfo) -> Result<()> {
        let info = &details.package;
        let package_info = serde_json::to_vec(&info).unwrap();

        let mut data_tar_gz = Vec::<u8>::new();
        let gz = GzEncoder::new(&mut data_tar_gz, Compression::default());
        let mut tar = TarBuilder::new(gz);

        let mtime = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        mkdirp(
            &mut tar,
            &format!("usr/palm/applications/"),
            Option::<&Path>::None,
            mtime,
        )?;
        tar.append_dir_all(
            format!("usr/palm/applications/{}", info.app),
            &details.app.path,
        )?;
        for service in &details.services {
            tar.append_dir_all(
                format!("usr/palm/services/{}", &service.info.id),
                &service.path,
            )?;
        }

        mkdirp(
            &mut tar,
            &format!("usr/palm/packages/{}/", info.id),
            Some(Path::new("usr/palm")),
            mtime,
        )?;
        let mut header = TarHeader::new_gnu();
        header.set_path(format!("usr/palm/packages/{}/packageinfo.json", info.id))?;
        header.set_mode(0o644);
        header.set_size(package_info.len() as u64);
        header.set_mtime(mtime);
        header.set_cksum();
        tar.append(&header, package_info.deref())?;

        drop(tar);
        return self.append(
            &ArHeader::new(b"data.tar.gz".to_vec(), data_tar_gz.len() as u64),
            Cursor::new(data_tar_gz),
        );
    }
}

fn mkdirp<W, P>(
    tar: &mut TarBuilder<W>,
    path: P,
    path_stop: Option<&Path>,
    mtime: u64,
) -> Result<()>
where
    W: Write,
    P: AsRef<Path>,
{
    let mut stack = Vec::new();
    let empty = Vec::<u8>::new();
    let mut p = path.as_ref();
    while p != Path::new("") {
        if let Some(s) = path_stop {
            if p == s {
                break;
            }
        }
        stack.insert(0, p);
        if let Some(parent) = p.parent() {
            p = parent;
        }
    }
    for p in stack {
        let mut header = TarHeader::new_gnu();
        header.set_path(format!("{}/", p.to_slash_lossy()))?;
        header.set_entry_type(EntryType::Directory);
        header.set_mode(0o755);
        header.set_size(0);
        header.set_mtime(mtime);
        header.set_cksum();
        tar.append(&header, empty.deref())?;
    }
    return Ok(());
}
