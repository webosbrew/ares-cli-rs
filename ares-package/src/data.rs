use std::io::{Cursor, Write as IoWrite, Write};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use ar::{Builder as ArBuilder, Header as ArHeader};
use flate2::Compression;
use flate2::write::GzEncoder;
use path_slash::PathExt as _;
use tar::{Builder as TarBuilder, EntryType, Header as TarHeader};

use crate::PackageInfo;

pub(crate) trait AppendData {
    fn append_data<P1, P2>(
        &mut self,
        info: &PackageInfo,
        app_dir: P1,
        service_dirs: &[P2],
    ) -> std::io::Result<()>
        where
            P1: AsRef<Path>,
            P2: AsRef<Path>;
}

impl<W> AppendData for ArBuilder<W>
    where
        W: IoWrite,
{
    fn append_data<P1, P2>(
        &mut self,
        info: &PackageInfo,
        app_dir: P1,
        service_dirs: &[P2],
    ) -> std::io::Result<()>
        where
            P1: AsRef<Path>,
            P2: AsRef<Path>,
    {
        let package_info = serde_json::to_vec(info).unwrap();

        let mut data_tar_gz = Vec::<u8>::new();
        let gz = GzEncoder::new(&mut data_tar_gz, Compression::default());
        let mut tar = TarBuilder::new(gz);

        let mtime = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        mkdirp(&mut tar, &format!("usr/palm/applications"), Option::<&Path>::None, mtime)?;
        tar.append_dir_all(format!("usr/palm/applications/{}", info.app), app_dir)?;

        mkdirp(&mut tar, &format!("usr/palm/packages/{}/", info.id), Some(Path::new("usr/palm")), mtime)?;
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

fn mkdirp<W, P>(tar: &mut TarBuilder<W>, path: P, path_stop: Option<&Path>,
                mtime: u64) -> std::io::Result<()>
    where W: Write, P: AsRef<Path> {
    let mut stack = Vec::new();
    let mut path = path.as_ref();
    let empty = Vec::<u8>::new();
    while let Some(p) = path.parent() {
        if p == Path::new("") {
            break;
        }
        if let Some(s) = path_stop {
            if p == s {
                break;
            }
        }
        stack.insert(0, p);
        path = p;
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