use serde::{Deserialize, Serialize};

pub mod cli;
mod device;
pub mod io;
mod manager;
mod privkey;

#[derive(Default)]
pub struct DeviceManager {}

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
    // Public so other crates can build a Device, not just read one.
    #[serde(default, skip_serializing)]
    pub new: bool,
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

/// How a device's SSH key is stored. The variants are untagged, so each one is
/// told apart by its JSON key alone.
///
/// Where a [`PrivateKey::Name`] is resolved is up to the reader: ares-cli-rs looks
/// in `~/.ssh`, dev-manager-desktop looks in its own app directory.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum PrivateKey {
    /// A file name inside an SSH directory.
    Name {
        #[serde(rename = "openSsh")]
        name: String,
    },
    /// A full path to a key file.
    Path {
        #[serde(rename = "openSshPath")]
        path: String,
    },
    /// The key itself, in OpenSSH format.
    Data {
        #[serde(rename = "openSshData")]
        data: String,
    },
}
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum FileTransfer {
    #[serde(rename = "stream")]
    Stream,
    #[serde(rename = "sftp")]
    Sftp,
}

#[must_use]
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

#[cfg(test)]
mod device_json_tests {
    use super::*;

    /// The JSON must match what ares-cli, dev-manager-desktop and the webOS SDK
    /// already write into novacom-devices.json.
    #[test]
    fn key_variants_keep_their_json_names() {
        let by_name: PrivateKey = serde_json::from_str(r#"{"openSsh":"webos_key"}"#).unwrap();
        assert!(matches!(by_name, PrivateKey::Name { ref name } if name == "webos_key"));

        let by_path: PrivateKey = serde_json::from_str(r#"{"openSshPath":"/tmp/id_rsa"}"#).unwrap();
        assert!(matches!(by_path, PrivateKey::Path { ref path } if path == "/tmp/id_rsa"));

        let inline: PrivateKey = serde_json::from_str(r#"{"openSshData":"KEY"}"#).unwrap();
        assert!(matches!(inline, PrivateKey::Data { ref data } if data == "KEY"));

        assert_eq!(
            serde_json::to_string(&PrivateKey::Name {
                name: "webos_key".into()
            })
            .unwrap(),
            r#"{"openSsh":"webos_key"}"#
        );
        assert_eq!(
            serde_json::to_string(&PrivateKey::Data { data: "KEY".into() }).unwrap(),
            r#"{"openSshData":"KEY"}"#
        );
    }

    #[test]
    fn device_round_trips_without_losing_fields() {
        let json = r#"{"profile":"ose","name":"tv","host":"192.168.1.2","port":9922,
            "username":"prisoner","privateKey":{"openSsh":"webos_tv"},"files":"stream",
            "passphrase":"pw","logDaemon":"pmlogd","noPortForwarding":true,"indelible":true}"#;
        let device: Device = serde_json::from_str(json).unwrap();
        assert_eq!(device.files, Some(FileTransfer::Stream));
        assert_eq!(device.no_port_forwarding, Some(true));
        assert_eq!(device.valid_passphrase(), Some(String::from("pw")));

        let out = serde_json::to_value(&device).unwrap();
        assert_eq!(out["privateKey"]["openSsh"], "webos_tv");
        assert_eq!(out["files"], "stream");
        // `new` is runtime state, so it must never reach the file.
        assert!(out.get("new").is_none());
    }
}
