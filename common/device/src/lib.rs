use std::sync::Mutex;

use serde::{Deserialize, Serialize};

mod device;
mod io;
mod manager;
mod privkey;

#[derive(Default)]
pub struct DeviceManager {
    devices: Mutex<Vec<Device>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Device {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<bool>,
    pub profile: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(default, skip_serializing)]
    pub(crate) new: bool,
    #[serde(rename = "privateKey", skip_serializing_if = "Option::is_none")]
    pub private_key: Option<PrivateKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<FileTransfer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passphrase: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(rename = "logDaemon", skip_serializing_if = "Option::is_none")]
    pub log_daemon: Option<String>,
    #[serde(
        rename = "noPortForwarding",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub no_port_forwarding: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indelible: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum PrivateKey {
    Name {
        #[serde(rename = "openSsh")]
        name: String,
    },
    Path {
        #[serde(rename = "openSshPath")]
        path: String,
    },
}
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum FileTransfer {
    #[serde(rename = "stream")]
    Stream,
    #[serde(rename = "sftp")]
    Sftp,
}

pub fn add(left: usize, right: usize) -> usize {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
