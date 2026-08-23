use std::collections::HashSet;
use std::fs;
use std::fs::File;
use std::io::{Cursor, Result, Write as IoWrite, Write};
use std::path::{Path, PathBuf};

use ar::{Builder as ArBuilder, Header as ArHeader};
use flate2::Compression;
use flate2::write::GzEncoder;
use path_slash::PathExt as _;
use regex::Regex;
use tar::{Builder as TarBuilder, EntryType, Header as TarHeader};
use walkdir::WalkDir;

use crate::PackageInfo;
use crate::input::data::DataInfo;
use crate::input::filter_by_excludes;

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
        ar_header.set_mode(0o100_644);
        ar_header.set_mtime(mtime);
        self.append(&ar_header, Cursor::new(data_tar_gz))
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
        header.set_entry_type(EntryType::Directory);
        header.set_mode(0o040_777);
        header.set_size(0);
        header.set_uid(0);
        header.set_gid(5000);
        header.set_mtime(mtime);
        header.set_cksum();
        println!("Adding {dir}");
        tar.append_data(&mut header, &dir, &*empty)?;
    }
    Ok(())
}

fn tar_path<S, P>(prefix: S, path: P) -> PathBuf
where
    S: AsRef<str>,
    P: AsRef<Path>,
{
    PathBuf::from(format!(
        "{}{}",
        prefix.as_ref(),
        path.as_ref().to_slash_lossy()
    ))
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
        .filter_entry(|entry| filter_by_excludes(base_path, entry, excludes))
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
            header.set_metadata(&entry_metadata);
            header.set_uid(0);
            header.set_gid(5000);
            header.set_cksum();
            println!("Adding {path}", path = tar_path.to_string_lossy());
            tar.append_data(&mut header, tar_path, &mut File::open(entry_path)?)?;
        }
    }
    Ok(())
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
    header.set_mode(0o100_644);
    header.set_size(details.package_data.len() as u64);
    header.set_mtime(mtime);
    header.set_uid(0);
    header.set_gid(5000);
    header.set_cksum();
    tar.append_data(&mut header, &pkg_info_path, &*details.package_data)?;
    println!("Adding {pkg_info_path}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::sync::atomic::{AtomicU32, Ordering};

    use ar::Archive as ArArchive;
    use flate2::read::GzDecoder;
    use tar::Archive as TarArchive;

    use super::*;
    use crate::input::data::DataInfo;

    const APP_ID: &str = "com.example.modes";

    /// A fresh directory under the system temp dir, removed when it drops.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> TempDir {
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let path = std::env::temp_dir().join(format!(
                "ares-package-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).expect("Failed to create temp dir");
            TempDir(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Package a small app dir and return the members of the resulting
    /// data.tar.gz as (path, mode, entry type).
    fn package() -> Vec<(String, u32, EntryType)> {
        let temp = TempDir::new();
        let app_dir = temp.0.join("app");
        fs::create_dir_all(app_dir.join("assets")).unwrap();
        fs::write(
            app_dir.join("appinfo.json"),
            format!(
                r#"{{"id":"{APP_ID}","version":"1.0.0","type":"web","main":"index.html","title":"Modes"}}"#
            ),
        )
        .unwrap();
        fs::write(app_dir.join("assets").join("a.png"), "x").unwrap();

        let data = DataInfo::from_input(&app_dir, &[] as &[PathBuf], &[] as &[&str])
            .expect("Failed to read input");
        let mut ar = ArBuilder::new(Vec::<u8>::new());
        ar.append_data(&data, 0).expect("Failed to append data");

        let mut ar = ArArchive::new(Cursor::new(ar.into_inner().unwrap()));
        let mut data_tar_gz = Vec::<u8>::new();
        while let Some(entry) = ar.next_entry() {
            let mut entry = entry.unwrap();
            if entry.header().identifier() == b"data.tar.gz" {
                entry.read_to_end(&mut data_tar_gz).unwrap();
                break;
            }
        }
        assert!(!data_tar_gz.is_empty(), "data.tar.gz is missing");

        TarArchive::new(GzDecoder::new(Cursor::new(data_tar_gz)))
            .entries()
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                let header = entry.header();
                // Directory members carry a trailing slash; drop it so lookups
                // can name a path just once.
                let path = header.path().unwrap().to_slash_lossy().to_string();
                (
                    path.trim_end_matches('/').to_string(),
                    header.mode().unwrap(),
                    header.entry_type(),
                )
            })
            .collect()
    }

    fn find<'a>(
        entries: &'a [(String, u32, EntryType)],
        path: &str,
    ) -> Option<&'a (String, u32, EntryType)> {
        entries.iter().find(|(name, _, _)| name == path)
    }

    #[test]
    fn every_directory_is_world_writable() {
        let entries = package();
        for dir in [
            "usr",
            "usr/palm",
            "usr/palm/applications",
            &format!("usr/palm/applications/{APP_ID}"),
            &format!("usr/palm/applications/{APP_ID}/assets"),
            &format!("usr/palm/packages/{APP_ID}"),
        ] {
            let (_, mode, entry_type) = find(&entries, dir).expect("Missing directory");
            assert_eq!(*mode & 0o7777, 0o777, "{dir} should be 0777");
            assert_eq!(*mode, 0o040_777, "{dir} is missing S_IFDIR");
            assert_eq!(*entry_type, EntryType::Directory);
        }
    }

    #[test]
    fn files_are_not_writable() {
        let entries = package();
        for file in [
            format!("usr/palm/applications/{APP_ID}/appinfo.json"),
            format!("usr/palm/applications/{APP_ID}/assets/a.png"),
            format!("usr/palm/packages/{APP_ID}/packageinfo.json"),
        ] {
            let (_, mode, entry_type) = find(&entries, &file).expect("Missing file");
            assert_eq!(
                *mode & 0o022,
                0,
                "{file} should not be group/world writable"
            );
            assert_eq!(*entry_type, EntryType::Regular);
        }
    }
}
