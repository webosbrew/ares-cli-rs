use std::io::Result;
use std::path::Path;

use path_slash::PathExt;
use regex::Regex;
use walkdir::WalkDir;

pub mod app;
pub mod data;
pub mod service;
pub mod validation;

pub(crate) fn dir_size<P: AsRef<Path>>(path: P, excludes: Option<&Regex>) -> Result<u64> {
    let walker = WalkDir::new(path.as_ref());
    let mut size = 0;
    for entry in walker.into_iter().filter_entry(|entry| {
        if let Some(exclude) = excludes {
            return !exclude.is_match(entry.path().to_slash_lossy().as_ref());
        }
        return true;
    }) {
        let entry = entry?;
        size += entry.metadata()?.len();
    }
    return Ok(size);
}
