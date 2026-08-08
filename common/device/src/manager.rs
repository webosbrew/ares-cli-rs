use std::fs::{create_dir_all, remove_file};
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};

use crate::io::{conf_dir, ensure_ssh_dir, read_in, write_in};
use crate::{Device, DeviceManager, PrivateKey};

/// The bundled default device list, restored by [`DeviceManager::reset`].
const DEFAULT_DEVICES_JSON: &str = r#"[
    {
        "order": "0",
        "default": true,
        "profile": "ose",
        "name": "emulator",
        "description": "LG webOS Emulator",
        "host": "127.0.0.1",
        "port": 6622,
        "username": "developer",
        "privateKey": { "openSsh": "webos_emul" },
        "files": "sftp",
        "noPortForwarding": false,
        "indelible": true
    }
]"#;

/// Converts an absolute private-key path into a name relative to `ssh_dir`, so
/// it is stored portably in the device list.
///
/// Both [`PrivateKey::Path`] and a [`PrivateKey::Name`] holding a full path are
/// converted. The webOS SDK and older versions of these tools wrote the second
/// form, and the device list is shared with them.
///
/// # Errors
///
/// Returns an error if the path is not under `ssh_dir` and no relative path
/// exists between them.
fn normalize_private_key(device: &mut Device, ssh_dir: &Path) -> Result<(), Error> {
    let full_path = match &device.private_key {
        Some(PrivateKey::Path { path }) => Some(PathBuf::from(path)),
        Some(PrivateKey::Name { name }) if Path::new(name).is_absolute() => {
            Some(PathBuf::from(name))
        }
        _ => return Ok(()),
    };
    let Some(full_path) = full_path else {
        return Ok(());
    };
    let name = String::from(
        pathdiff::diff_paths(&full_path, ssh_dir)
            .ok_or(Error::from(ErrorKind::NotFound))?
            .to_string_lossy(),
    );
    device.private_key = Some(PrivateKey::Name { name });
    Ok(())
}

impl DeviceManager {
    /// A manager that reads the directories the webOS SDK uses.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A manager that reads `conf_dir` for the device list and `ssh_dir` for
    /// the keys, instead of the SDK's own directories.
    #[must_use]
    pub fn with_dirs(conf_dir: PathBuf, ssh_dir: PathBuf) -> Self {
        Self {
            conf_dir: Some(conf_dir),
            ssh_dir: Some(ssh_dir),
        }
    }

    /// The directory holding the device list, created if it is absent.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory is unknown or cannot be created.
    pub fn conf_dir(&self) -> Result<PathBuf, Error> {
        let dir = match &self.conf_dir {
            Some(dir) => dir.clone(),
            None => conf_dir()?,
        };
        if !dir.exists() {
            create_dir_all(&dir)?;
        }
        Ok(dir)
    }

    /// The directory where SSH keys are stored, created if it is absent.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory is unknown or cannot be created.
    pub fn ssh_key_dir(&self) -> Result<PathBuf, Error> {
        match &self.ssh_dir {
            Some(dir) => {
                if !dir.exists() {
                    create_dir_all(dir)?;
                }
                Ok(dir.clone())
            }
            None => ensure_ssh_dir(),
        }
    }

    /// # Errors
    ///
    /// Returns an error if the device list cannot be read.
    pub fn list(&self) -> Result<Vec<Device>, Error> {
        read_in(&self.conf_dir()?)
    }

    /// # Errors
    ///
    /// Returns an error if the device list cannot be read.
    pub fn find_or_default<S: AsRef<str>>(
        &self,
        name: Option<&S>,
    ) -> Result<Option<Device>, Error> {
        let devices = self.list()?;
        Ok(devices
            .iter()
            .find(|d| {
                if let Some(name) = &name {
                    d.name == name.as_ref()
                } else {
                    d.default.unwrap_or(false)
                }
            })
            .cloned())
    }

    /// # Errors
    ///
    /// Returns an error if the device list cannot be read or written.
    pub fn set_default(&self, name: &str) -> Result<Option<Device>, Error> {
        let conf_dir = self.conf_dir()?;
        let mut devices = read_in(&conf_dir)?;
        let mut result: Option<Device> = None;
        for device in &mut devices {
            if device.name == name {
                device.default = Some(true);
                result = Some(device.clone());
            } else {
                device.default = None;
            }
        }
        log::trace!("{devices:?}");
        write_in(&conf_dir, &devices)?;
        Ok(result)
    }

    /// # Errors
    ///
    /// Returns [`ErrorKind::AlreadyExists`] if a device of that name is already
    /// in the list, or an error if the list cannot be read or written.
    pub fn add(&self, device: &Device) -> Result<Device, Error> {
        let mut device = device.clone();
        normalize_private_key(&mut device, &self.ssh_key_dir()?)?;
        let conf_dir = self.conf_dir()?;
        let mut devices = read_in(&conf_dir)?;
        if devices.iter().any(|d| d.name == device.name) {
            return Err(Error::new(
                ErrorKind::AlreadyExists,
                format!("Device {} already exists", device.name),
            ));
        }
        log::info!("Save device {}", device.name);
        devices.push(device.clone());
        write_in(&conf_dir, &devices)?;
        Ok(device)
    }

    /// Replaces an existing device (matched by `name`) with `device`, keeping
    /// its position in the list. The replacement may itself carry a new name.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorKind::NotFound`] if no device carries that name, or an
    /// error if the list cannot be read or written.
    pub fn modify(&self, name: &str, device: &Device) -> Result<Device, Error> {
        let mut device = device.clone();
        normalize_private_key(&mut device, &self.ssh_key_dir()?)?;
        let conf_dir = self.conf_dir()?;
        let mut devices = read_in(&conf_dir)?;
        let index = devices
            .iter()
            .position(|d| d.name == name)
            .ok_or_else(|| Error::new(ErrorKind::NotFound, format!("Device {name} not found")))?;
        log::info!("Modify device {name}");
        devices[index] = device.clone();
        write_in(&conf_dir, &devices)?;
        Ok(device)
    }

    /// Restores the device list to the bundled default emulator entry.
    ///
    /// # Errors
    ///
    /// Returns an error if the list cannot be written.
    pub fn reset(&self) -> Result<(), Error> {
        let devices: Vec<Device> = serde_json::from_str(DEFAULT_DEVICES_JSON)
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        write_in(&self.conf_dir()?, &devices)?;
        Ok(())
    }

    /// Removes the device named `name`.
    ///
    /// A device marked `indelible` is refused, because the webOS SDK ships it
    /// and expects it to stay. Set `force` to remove it anyway. A caller that
    /// already asked the person to confirm has nothing left to protect them
    /// from, so it should pass `true`.
    ///
    /// `remove_key` also deletes the device's key file, but only a key this
    /// tool put in the SSH directory. A path points somewhere else, and inline
    /// data is not a file.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorKind::PermissionDenied`] for an indelible device when
    /// `force` is false, or an error if the list cannot be read or written.
    pub fn remove(&self, name: &str, remove_key: bool, force: bool) -> Result<(), Error> {
        let conf_dir = self.conf_dir()?;
        let devices = read_in(&conf_dir)?;
        if !force
            && devices
                .iter()
                .any(|d| d.name == name && d.indelible.unwrap_or(false))
        {
            return Err(Error::new(
                ErrorKind::PermissionDenied,
                format!("Device {name} can't be removed"),
            ));
        }
        let (will_delete, mut will_keep): (Vec<Device>, Vec<Device>) =
            devices.into_iter().partition(|d| d.name == name);
        let mut need_new_default = false;
        if remove_key {
            for device in will_delete {
                if device.default.unwrap_or(false) {
                    need_new_default = true;
                }
                if let Some(name) = device.private_key.and_then(|k| match k {
                    PrivateKey::Name { name } => Some(name),
                    PrivateKey::Path { .. } | PrivateKey::Data { .. } => None,
                }) {
                    if !name.starts_with("webos_") {
                        continue;
                    }
                    let key_path = self.ssh_key_dir()?.join(name);
                    remove_file(key_path)?;
                }
            }
        }
        if need_new_default && !will_keep.is_empty() {
            will_keep.first_mut().unwrap().default = Some(true);
        }
        write_in(&conf_dir, &will_keep)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{create_dir_all, remove_dir_all, write};
    use std::sync::atomic::{AtomicU32, Ordering};

    fn temp_manager(label: &str) -> (DeviceManager, PathBuf) {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ares-manager-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        create_dir_all(dir.join("ssh")).unwrap();
        (
            DeviceManager::with_dirs(dir.join("conf"), dir.join("ssh")),
            dir,
        )
    }

    fn device(name: &str, indelible: bool) -> Device {
        serde_json::from_str(&format!(
            r#"{{"profile":"ose","name":"{name}","host":"10.0.0.2","port":9922,
                "username":"prisoner","indelible":{indelible}}}"#
        ))
        .unwrap()
    }

    #[test]
    fn an_indelible_device_stays_unless_it_is_forced() {
        let (manager, dir) = temp_manager("indelible");
        manager.add(&device("emulator", true)).unwrap();

        let error = manager
            .remove("emulator", false, false)
            .expect_err("an indelible device is refused");
        assert_eq!(error.kind(), ErrorKind::PermissionDenied);
        assert_eq!(manager.list().unwrap().len(), 1);

        manager.remove("emulator", false, true).unwrap();
        assert!(manager.list().unwrap().is_empty());

        remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_normal_device_needs_no_force() {
        let (manager, dir) = temp_manager("normal");
        manager.add(&device("tv", false)).unwrap();

        manager.remove("tv", false, false).unwrap();
        assert!(manager.list().unwrap().is_empty());

        remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_key_path_under_the_ssh_dir_is_stored_as_a_name() {
        let (manager, dir) = temp_manager("normalize");
        let ssh_dir = manager.ssh_key_dir().unwrap();
        write(ssh_dir.join("webos_tv"), "KEY").unwrap();

        let mut tv = device("tv", false);
        tv.private_key = Some(PrivateKey::Path {
            path: ssh_dir.join("webos_tv").to_string_lossy().to_string(),
        });
        let stored = manager.add(&tv).unwrap();

        assert!(matches!(stored.private_key, Some(PrivateKey::Name { name }) if name == "webos_tv"));
        remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_name_holding_a_full_path_is_normalized_too() {
        let (manager, dir) = temp_manager("legacy-name");
        let ssh_dir = manager.ssh_key_dir().unwrap();

        let mut tv = device("tv", false);
        tv.private_key = Some(PrivateKey::Name {
            name: ssh_dir.join("webos_tv").to_string_lossy().to_string(),
        });
        let stored = manager.add(&tv).unwrap();

        assert!(matches!(stored.private_key, Some(PrivateKey::Name { name }) if name == "webos_tv"));
        remove_dir_all(&dir).ok();
    }

    #[test]
    fn adding_the_same_name_twice_is_refused() {
        let (manager, dir) = temp_manager("duplicate");
        manager.add(&device("tv", false)).unwrap();
        let error = manager
            .add(&device("tv", false))
            .expect_err("the name is taken");
        assert_eq!(error.kind(), ErrorKind::AlreadyExists);
        remove_dir_all(&dir).ok();
    }
}
