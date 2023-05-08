use crate::device_manager::Device;

impl AsRef<Device> for Device {
    fn as_ref(&self) -> &Device {
        return self;
    }
}

impl Device {
    pub(crate) fn valid_passphrase(&self) -> Option<String> {
        return self.passphrase.clone().filter(|s| !s.is_empty());
    }
}
