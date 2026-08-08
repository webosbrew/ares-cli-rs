//! Reads and writes `novacom-devices.json`, the device list that ares-cli, the
//! webOS SDK and dev-manager-desktop all share.

use std::fs::{File, create_dir_all};
use std::io::{BufReader, BufWriter, Error, ErrorKind};
use std::path::{Path, PathBuf};
use std::{env, fs};

use serde_json::Value;

use crate::Device;

/// The name of the device list inside a configuration directory.
const DEVICES_FILE_NAME: &str = "novacom-devices.json";

/// Read the device list from `conf_dir`. A missing file is an empty list, not
/// an error. An entry that does not parse is skipped, so one bad entry written
/// by another tool does not hide the rest.
///
/// # Errors
///
/// Returns an error if the file exists but cannot be read or is not JSON.
pub fn read_in(conf_dir: &Path) -> Result<Vec<Device>, Error> {
    let path = conf_dir.join(DEVICES_FILE_NAME);
    let file = match File::open(path.as_path()) {
        Ok(file) => file,
        Err(e) => {
            return match e.kind() {
                ErrorKind::NotFound => Ok(Vec::new()),
                _ => Err(e),
            };
        }
    };
    let reader = BufReader::new(file);

    let raw_list: Vec<Value> = serde_json::from_reader(reader)?;
    Ok(raw_list
        .iter()
        .filter_map(|v| serde_json::from_value::<Device>(v.clone()).ok())
        .collect())
}

/// Write the device list to `conf_dir`, creating the directory if it is absent.
/// The webOS SDK leaves the file read-only, so this clears that first.
///
/// # Errors
///
/// Returns an error if the directory cannot be created or the file cannot be
/// written.
pub fn write_in(conf_dir: &Path, devices: &[Device]) -> Result<(), Error> {
    let path = conf_dir.join(DEVICES_FILE_NAME);
    let file = match File::create(path.as_path()) {
        Ok(file) => file,
        Err(e) => {
            match e.kind() {
                ErrorKind::PermissionDenied => {
                    fix_devices_json_perm(path.clone())?;
                }
                ErrorKind::NotFound => {
                    let parent = path.parent().ok_or(Error::from(ErrorKind::NotFound))?;
                    create_dir_all(parent)?;
                }
                _ => return Err(e),
            }
            File::create(path.as_path())?
        }
    };
    log::info!("make the file writable: {}", path.display());
    file.metadata()?.permissions().set_readonly(false);
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, &devices)?;
    Ok(())
}

pub(crate) fn read() -> Result<Vec<Device>, Error> {
    read_in(&conf_dir()?)
}

pub(crate) fn write(devices: &[Device]) -> Result<(), Error> {
    write_in(&conf_dir()?, devices)
}

pub(crate) fn ssh_dir() -> Result<PathBuf, Error> {
    env::home_dir()
        .map(|d| d.join(".ssh"))
        .ok_or(Error::new(ErrorKind::NotFound, "SSH directory not found"))
}

pub(crate) fn ensure_ssh_dir() -> Result<PathBuf, Error> {
    let dir = ssh_dir()?;
    if !dir.exists() {
        create_dir_all(dir.clone())?;
    }
    Ok(dir)
}

/// The directory the webOS SDK keeps the device list in.
///
/// # Errors
///
/// Returns an error if the home directory is unknown.
#[cfg(target_family = "windows")]
pub fn conf_dir() -> Result<PathBuf, Error> {
    let home = env::var("APPDATA")
        .or_else(|_| env::var("USERPROFILE"))
        .map_err(|_| Error::new(ErrorKind::NotFound, "Can't find %AppData% or %UserProfile%"))?;
    Ok(PathBuf::from(home).join(".webos").join("ose"))
}

/// The directory the webOS SDK keeps the device list in.
///
/// # Errors
///
/// Returns an error if the home directory is unknown.
#[cfg(not(target_family = "windows"))]
pub fn conf_dir() -> Result<PathBuf, Error> {
    let home = env::home_dir()
        .ok_or_else(|| Error::new(ErrorKind::NotFound, "Can't find home directory"))?;
    Ok(home.join(".webos").join("ose"))
}

#[cfg(not(unix))]
fn fix_devices_json_perm(path: PathBuf) -> Result<(), Error> {
    let mut perm = fs::metadata(path.clone())?.permissions();
    #[allow(
        clippy::permissions_set_readonly_false,
        reason = "cfg(not(unix)) above"
    )]
    perm.set_readonly(false);
    fs::set_permissions(path, perm)?;
    Ok(())
}

#[cfg(unix)]
fn fix_devices_json_perm(path: PathBuf) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;
    let perm = fs::Permissions::from_mode(0o644);
    fs::set_permissions(path, perm)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{create_dir_all, remove_dir_all, write};
    use std::sync::atomic::{AtomicU32, Ordering};

    fn temp_dir(label: &str) -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ares-device-io-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_missing_file_reads_as_an_empty_list() {
        let dir = temp_dir("missing");
        assert!(read_in(&dir).unwrap().is_empty());
        remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_unreadable_entry_is_skipped() {
        let dir = temp_dir("partial");
        write(
            dir.join(DEVICES_FILE_NAME),
            r#"[{"not":"a device"},
                {"profile":"ose","name":"tv","host":"10.0.0.2","port":9922,"username":"prisoner"}]"#,
        )
        .unwrap();

        let devices = read_in(&dir).unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name, "tv");
        remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_written_list_reads_back() {
        let dir = temp_dir("roundtrip");
        let devices = read_in(Path::new("/nonexistent")).unwrap();
        assert!(devices.is_empty());

        write(
            dir.join(DEVICES_FILE_NAME),
            r#"[{"profile":"ose","name":"tv","host":"10.0.0.2","port":9922,"username":"prisoner"}]"#,
        )
        .unwrap();
        let devices = read_in(&dir).unwrap();

        let out_dir = dir.join("nested").join("deeper");
        write_in(&out_dir, &devices).unwrap();
        assert_eq!(read_in(&out_dir).unwrap()[0].name, "tv");

        remove_dir_all(&dir).ok();
    }
}
