use crate::Device;

impl AsRef<Device> for Device {
    fn as_ref(&self) -> &Device {
        return self;
    }
}

impl Device {
    pub fn valid_passphrase(&self) -> Option<String> {
        return self.passphrase.clone().filter(|s| !s.is_empty());
    }
}
