use std::fs::remove_file;
use std::io::{Error, ErrorKind};
use std::path::Path;

use crate::io::{ensure_ssh_dir, read, write};
use crate::{Device, DeviceManager, PrivateKey};

impl DeviceManager {
    pub fn list(&self) -> Result<Vec<Device>, Error> {
        return read();
    }

    pub fn find_or_default<S: AsRef<str>>(&self, name: Option<S>) -> Result<Option<Device>, Error> {
        let devices = self.list()?;
        return Ok(devices
            .iter()
            .find(|d| {
                if let Some(name) = &name {
                    &d.name == name.as_ref()
                } else {
                    d.default.unwrap_or(false)
                }
            })
            .cloned());
    }

    pub fn set_default(&self, name: &str) -> Result<Option<Device>, Error> {
        let mut devices = read()?;
        let mut result: Option<Device> = None;
        for mut device in &mut devices {
            if device.name == name {
                device.default = Some(true);
                result = Some(device.clone());
            } else {
                device.default = None;
            }
        }
        log::trace!("{:?}", devices);
        write(devices)?;
        return Ok(result);
    }

    pub fn add(&self, device: &Device) -> Result<Device, Error> {
        let mut device = device.clone();
        match &device.private_key {
            Some(PrivateKey::Path { path }) => {
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
            _ => {}
        }
        log::info!("Save device {}", device.name);
        let mut devices = read()?;
        devices.push(device.clone());
        write(devices.clone())?;
        return Ok(device);
    }

    pub fn remove(&self, name: &str, remove_key: bool) -> Result<(), Error> {
        let devices = read()?;
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
                    _ => None,
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
        write(will_keep)?;
        return Ok(());
    }
}
