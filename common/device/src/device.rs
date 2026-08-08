use crate::Device;

impl AsRef<Device> for Device {
    fn as_ref(&self) -> &Device {
        self
    }
}

impl Device {
    #[must_use]
    pub fn valid_passphrase(&self) -> Option<String> {
        self.passphrase.clone().filter(|s| !s.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use crate::Device;

    fn device_with_passphrase(passphrase: Option<&str>) -> Device {
        Device {
            order: None,
            default: None,
            profile: String::from("ose"),
            name: String::from("tv"),
            description: None,
            host: String::from("192.168.1.2"),
            port: 9922,
            username: String::from("prisoner"),
            new: false,
            private_key: None,
            files: None,
            passphrase: passphrase.map(String::from),
            password: None,
            log_daemon: None,
            no_port_forwarding: None,
            indelible: None,
        }
    }

    #[test]
    fn an_empty_passphrase_counts_as_none() {
        // The config file stores "" for no passphrase, which libssh must not see.
        assert_eq!(device_with_passphrase(Some("")).valid_passphrase(), None);
        assert_eq!(device_with_passphrase(None).valid_passphrase(), None);
        assert_eq!(
            device_with_passphrase(Some("secret")).valid_passphrase(),
            Some(String::from("secret"))
        );
    }
}
