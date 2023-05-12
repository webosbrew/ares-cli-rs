use std::io::{Error, ErrorKind};

use libssh_rs::SshKey;

use ares_device_lib::DeviceManager;

use crate::DeviceSetupManager;

impl DeviceSetupManager for DeviceManager {
    //noinspection HttpUrlsUsage
    fn novacom_getkey(&self, address: &str, passphrase: &str) -> Result<String, Error> {
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

    fn localkey_verify(&self, name: &str, passphrase: &str) -> Result<(), Error> {
        todo!();
    }
}
