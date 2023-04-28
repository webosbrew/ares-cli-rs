use std::fs;
use std::fs::{remove_file, File};
use std::io::{Error, ErrorKind, Write};
use std::path::Path;

use libssh_rs::SshKey;

use crate::device_manager::io::{ensure_ssh_dir, read, ssh_dir, write};
use crate::device_manager::{Device, DeviceManager, PrivateKey};

impl DeviceManager {
    pub fn list(&self) -> Result<Vec<Device>, Error> {
        let devices = read()?;
        *self.devices.lock().unwrap() = devices.clone();
        return Ok(devices);
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
        if let Some(key) = &device.private_key {
            match key {
                PrivateKey::Path { name } => {
                    let path = Path::new(name);
                    if path.is_absolute() {
                        let name = String::from(
                            pathdiff::diff_paths(path, ensure_ssh_dir()?)
                                .ok_or(Error::from(ErrorKind::NotFound))?
                                .to_string_lossy(),
                        );
                        device.private_key = Some(PrivateKey::Path { name });
                    }
                }
                PrivateKey::Data { data } => {
                    let name = key.name(device.valid_passphrase())?;
                    let key_path = ensure_ssh_dir()?.join(&name);
                    let mut file = File::create(key_path)?;
                    file.write(data.as_bytes())?;
                    device.private_key = Some(PrivateKey::Path { name });
                }
            }
        }
        log::info!("Save device {}", device.name);
        let mut devices = read()?;
        devices.push(device.clone());
        write(devices.clone())?;
        return Ok(device);
    }

    pub async fn remove(&self, name: &str, remove_key: bool) -> Result<(), Error> {
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
                    PrivateKey::Path { name } => Some(name),
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

    //noinspection HttpUrlsUsage
    pub async fn novacom_getkey(&self, address: &str, passphrase: &str) -> Result<String, Error> {
        let content = reqwest::blocking::get(format!("http://{}:9991/webos_rsa", address))
            .and_then(|res| res.error_for_status())
            .and_then(|res| res.text())
            .map_err(|e| {
                Error::new(
                    ErrorKind::Other,
                    format!("Can't request private key: {e:?}"),
                )
            })?;

        return match SshKey::from_privkey_base64(&content, Some(passphrase)) {
            Ok(_) => Ok(content),
            _ => Err(if passphrase.is_empty() {
                Error::new(ErrorKind::Other, format!("Passphrase is empty"))
            } else {
                Error::new(ErrorKind::Other, format!("Passphrase is incorrect"))
            }),
        };
    }

    pub async fn localkey_verify(&self, name: &str, passphrase: &str) -> Result<(), Error> {
        let name_path = Path::new(name);
        let ssh_key_path = if name_path.is_absolute() {
            name_path.to_path_buf()
        } else {
            fs::canonicalize(ssh_dir()?.join(name))?
        };
        return match SshKey::from_privkey_file(ssh_key_path.to_str().unwrap(), Some(passphrase)) {
            Ok(_) => Ok(()),
            _ => Err(if passphrase.is_empty() {
                Error::new(ErrorKind::Other, format!("Passphrase is empty"))
            } else {
                Error::new(ErrorKind::Other, format!("Passphrase is incorrect"))
            }),
        };
    }
}
