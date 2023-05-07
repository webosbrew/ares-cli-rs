use std::fs;
use std::fs::File;
use std::io::{Cursor, Result, Write as IoWrite, Write};
use std::ops::Deref;
use std::path::Path;

use ar::{Builder as ArBuilder, Header as ArHeader};
use flate2::write::GzEncoder;
use flate2::Compression;
use path_slash::{PathBufExt, PathExt as _};
use regex::Regex;
use tar::{Builder as TarBuilder, EntryType, Header as TarHeader};
use walkdir::WalkDir;

use crate::input::data::DataInfo;

pub trait AppendData {
    fn append_data(&mut self, details: &DataInfo, mtime: u64) -> Result<()>;
}

impl<W> AppendData for ArBuilder<W>
where
    W: IoWrite,
{
    fn append_data(&mut self, details: &DataInfo, mtime: u64) -> Result<()> {
        let info = &details.package;

        let mut data_tar_gz = Vec::<u8>::new();
        let gz = GzEncoder::new(&mut data_tar_gz, Compression::default());
        let mut tar = TarBuilder::new(gz);

        append_dirs(
            &mut tar,
            &format!("usr/palm/applications/"),
            Option::<&Path>::None,
            mtime,
        )?;
        append_tree(
            &mut tar,
            format!("usr/palm/applications/{}/", info.app),
            &details.app.path,
            details.excludes.as_ref(),
        )?;
        for service in &details.services {
            append_tree(
                &mut tar,
                format!("usr/palm/services/{}/", service.info.id),
                &service.path,
                details.excludes.as_ref(),
            )?;
        }

        append_dirs(
            &mut tar,
            &format!("usr/palm/packages/{}/", info.id),
            Some(Path::new("usr/palm")),
            mtime,
        )?;
        let mut tar_header = TarHeader::new_gnu();
        tar_header.set_path(format!("usr/palm/packages/{}/packageinfo.json", info.id))?;
        tar_header.set_mode(0o100644);
        tar_header.set_size(details.package_data.len() as u64);
        tar_header.set_mtime(mtime);
        tar_header.set_cksum();
        tar.append(&tar_header, details.package_data.deref())?;
        drop(tar);

        let mut ar_header = ArHeader::new(b"data.tar.gz".to_vec(), data_tar_gz.len() as u64);
        ar_header.set_mode(0o100644);
        ar_header.set_mtime(mtime);
        return self.append(&ar_header, Cursor::new(data_tar_gz));
    }
}

fn append_dirs<W, P>(
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
        header.set_mode(0o100755);
        header.set_size(0);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(mtime);
        header.set_cksum();
        tar.append(&header, empty.deref())?;
    }
    return Ok(());
}

fn append_tree<W, S, P>(
    tar: &mut TarBuilder<W>,
    prefix: S,
    path: P,
    excludes: Option<&Regex>,
) -> Result<()>
where
    W: Write,
    S: AsRef<str>,
    P: AsRef<Path>,
{
    let base_path = path.as_ref();
    let walker = WalkDir::new(base_path)
        .contents_first(false)
        .sort_by_file_name();
    for entry in walker.into_iter().filter_entry(|entry| {
        if let Some(exclude) = excludes {
            return !exclude.is_match(entry.path().to_slash_lossy().as_ref());
        }
        return true;
    }) {
        let entry = entry?;
        let entry_type = entry.file_type();
        let tar_path = format!(
            "{}{}",
            prefix.as_ref(),
            entry
                .path()
                .strip_prefix(base_path)
                .unwrap()
                .to_slash_lossy()
        );
        if entry_type.is_symlink() {
            let link_target = fs::read_link(entry.path())?;
            println!(
                "Adding symlink {tar_path} => {}",
                link_target.to_slash_lossy()
            );
            let mut header = TarHeader::new_gnu();
            header.set_metadata(&entry.metadata()?);
            header.set_cksum();
            tar.append_link(&mut header, tar_path, link_target)?;
        } else if entry_type.is_dir() {
            println!("Adding dir {tar_path}");
            tar.append_dir(tar_path, entry.path())?;
        } else {
            println!("Adding file {tar_path}");
            tar.append_file(tar_path, &mut File::open(entry.path())?)?;
        }
    }
    return Ok(());
}
