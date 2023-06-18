use std::collections::HashSet;
use std::fs;
use std::fs::File;
use std::io::{Cursor, Result, Write as IoWrite, Write};
use std::ops::Deref;
use std::path::{Path, PathBuf};

use ar::{Builder as ArBuilder, Header as ArHeader};
use flate2::write::GzEncoder;
use flate2::Compression;
use path_slash::PathExt as _;
use regex::Regex;
use tar::{Builder as TarBuilder, EntryType, Header as TarHeader};
use walkdir::WalkDir;

use crate::input::data::DataInfo;
use crate::input::filter_by_excludes;
use crate::PackageInfo;

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

        let mut dir_entries: HashSet<PathBuf> = HashSet::new();

        append_tree(
            &mut tar,
            format!("usr/palm/applications/{}/", info.app),
            &details.app.path,
            &mut dir_entries,
            details.excludes.as_ref(),
            mtime,
        )?;
        for service in &details.services {
            append_tree(
                &mut tar,
                format!("usr/palm/services/{}/", service.info.id),
                &service.path,
                &mut dir_entries,
                details.excludes.as_ref(),
                mtime,
            )?;
        }
        append_package_info(&mut tar, &mut dir_entries, info, details, mtime)?;
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
    dir_entries: &mut HashSet<PathBuf>,
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
        if dir_entries.contains(p) {
            break;
        }
        stack.insert(0, p);
        dir_entries.insert(p.to_path_buf());
        if let Some(parent) = p.parent() {
            p = parent;
        }
    }
    for p in stack {
        let mut header = TarHeader::new_gnu();
        let mut dir = String::from(p.to_slash_lossy());
        if !dir.ends_with('/') {
            dir.push('/');
        }
        header.set_path(&dir)?;
        header.set_entry_type(EntryType::Directory);
        header.set_mode(0o100775);
        header.set_size(0);
        header.set_uid(0);
        header.set_gid(5000);
        header.set_mtime(mtime);
        header.set_cksum();
        println!("Adding {path}", path = dir);
        tar.append(&header, empty.deref())?;
    }
    return Ok(());
}

fn tar_path<S, P>(prefix: S, path: P) -> PathBuf
where
    S: AsRef<str>,
    P: AsRef<Path>,
{
    return PathBuf::from(format!(
        "{}{}",
        prefix.as_ref(),
        path.as_ref().to_slash_lossy()
    ));
}

fn append_tree<W, S, P>(
    tar: &mut TarBuilder<W>,
    prefix: S,
    path: P,
    dir_entries: &mut HashSet<PathBuf>,
    excludes: Option<&Regex>,
    mtime: u64,
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
    for entry in walker
        .into_iter()
        .filter_entry(|entry| filter_by_excludes(entry, excludes))
    {
        let entry = entry?;
        let entry_type = entry.file_type();
        let entry_metadata = entry.metadata()?;
        let entry_path = entry.path();
        let tar_path = tar_path(&prefix, entry_path.strip_prefix(base_path).unwrap());
        if entry_type.is_dir() {
            append_dirs(tar, &tar_path, dir_entries, mtime)?;
        } else if let Some(parent) = tar_path.parent() {
            append_dirs(tar, parent, dir_entries, mtime)?;
        }
        if entry_type.is_symlink() {
            let link_target = fs::read_link(entry_path)?;
            let mut header = TarHeader::new_gnu();
            header.set_metadata(&entry_metadata);
            header.set_uid(0);
            header.set_gid(5000);
            header.set_cksum();
            println!(
                "Adding {path} -> {target}",
                path = tar_path.to_string_lossy(),
                target = link_target.to_string_lossy()
            );
            tar.append_link(&mut header, tar_path, link_target)?;
        } else if entry_type.is_file() {
            let mut header = TarHeader::new_gnu();
            header.set_path(&tar_path)?;
            header.set_metadata(&entry_metadata);
            header.set_uid(0);
            header.set_gid(5000);
            header.set_cksum();
            println!("Adding {path}", path = tar_path.to_string_lossy());
            tar.append_data(&mut header, tar_path, &mut File::open(entry_path)?)?;
        }
    }
    return Ok(());
}

fn append_package_info<W>(
    tar: &mut TarBuilder<W>,
    dir_entries: &mut HashSet<PathBuf>,
    info: &PackageInfo,
    details: &DataInfo,
    mtime: u64,
) -> Result<()>
where
    W: Write,
{
    let package_dir = format!("usr/palm/packages/{}/", info.id);
    append_dirs(tar, &package_dir, dir_entries, mtime)?;
    let mut header = TarHeader::new_gnu();
    let pkg_info_path = format!("usr/palm/packages/{}/packageinfo.json", info.id);
    header.set_path(&pkg_info_path)?;
    header.set_mode(0o100644);
    header.set_size(details.package_data.len() as u64);
    header.set_mtime(mtime);
    header.set_uid(0);
    header.set_gid(5000);
    header.set_cksum();
    tar.append(&header, details.package_data.deref())?;
    println!("Adding {path}", path = pkg_info_path);
    return Ok(());
}
