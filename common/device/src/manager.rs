use std::fs::remove_file;
use std::io::{Error, ErrorKind};
use std::path::Path;

use crate::io::{ensure_ssh_dir, read, write};
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

/// Converts an absolute private-key path into a name relative to the ssh dir,
/// so it is stored portably in the device list.
fn normalize_private_key(device: &mut Device) -> Result<(), Error> {
    if let Some(PrivateKey::Path { path }) = &device.private_key {
        let path = Path::new(path);
        if path.is_absolute() {
            let name = String::from(
                pathdiff::diff_paths(path, ensure_ssh_dir()?)
                    .ok_or(Error::from(ErrorKind::NotFound))?
                    .to_string_lossy(),
            );
            device.private_key = Some(PrivateKey::Name { name });
        }
    }
    Ok(())
}

impl DeviceManager {
    pub fn list(&self) -> Result<Vec<Device>, Error> {
        read()
    }

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

    pub fn set_default(&self, name: &str) -> Result<Option<Device>, Error> {
        let mut devices = read()?;
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
        write(&devices)?;
        Ok(result)
    }

    pub fn add(&self, device: &Device) -> Result<Device, Error> {
        let mut device = device.clone();
        normalize_private_key(&mut device)?;
        let mut devices = read()?;
        if devices.iter().any(|d| d.name == device.name) {
            return Err(Error::new(
                ErrorKind::AlreadyExists,
                format!("Device {} already exists", device.name),
            ));
        }
        log::info!("Save device {}", device.name);
        devices.push(device.clone());
        write(&devices)?;
        Ok(device)
    }

    /// Replaces an existing device (matched by `name`) with `device`, keeping
    /// its position in the list. The replacement may itself carry a new name.
    pub fn modify(&self, name: &str, device: &Device) -> Result<Device, Error> {
        let mut device = device.clone();
        normalize_private_key(&mut device)?;
        let mut devices = read()?;
        let index = devices
            .iter()
            .position(|d| d.name == name)
            .ok_or_else(|| Error::new(ErrorKind::NotFound, format!("Device {name} not found")))?;
        log::info!("Modify device {name}");
        devices[index] = device.clone();
        write(&devices)?;
        Ok(device)
    }

    /// Restores the device list to the bundled default emulator entry.
    pub fn reset(&self) -> Result<(), Error> {
        let devices: Vec<Device> = serde_json::from_str(DEFAULT_DEVICES_JSON)
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        write(&devices)?;
        Ok(())
    }

    pub fn remove(&self, name: &str, remove_key: bool) -> Result<(), Error> {
        let devices = read()?;
        if devices
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
                    PrivateKey::Path { .. } => None,
                }) {
                    if !name.starts_with("webos_") {
                        continue;
                    }
                    let key_path = ensure_ssh_dir()?.join(name);
                    remove_file(key_path)?;
                }
            }
        }
        if need_new_default && !will_keep.is_empty() {
            will_keep.first_mut().unwrap().default = Some(true);
        }
        write(&will_keep)?;
        Ok(())
    }
}
