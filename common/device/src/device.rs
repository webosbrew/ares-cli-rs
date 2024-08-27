use crate::Device;

impl AsRef<Device> for Device {
    fn as_ref(&self) -> &Device {
        self
    }
}

impl Device {
    pub fn valid_passphrase(&self) -> Option<String> {
        self.passphrase.clone().filter(|s| !s.is_empty())
    }
}
