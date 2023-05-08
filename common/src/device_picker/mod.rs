use std::io::{Error, ErrorKind};

use crate::device_manager::{Device, DeviceManager};
#[cfg(target_os = "windows")]
use crate::device_picker::windows::PickPromptWindows;

#[cfg(target_os = "windows")]
mod windows;

pub trait PickDevice {
    fn pick<S: AsRef<str>>(&self, name: Option<S>, pick: bool) -> Result<Option<Device>, Error>;
}

impl PickDevice for DeviceManager {
    fn pick<S: AsRef<str>>(&self, name: Option<S>, pick: bool) -> Result<Option<Device>, Error> {
        let devices = self.list()?;
        let device = if let Some(s) = name {
            devices.iter().find(|d| d.name == s.as_ref()).cloned()
        } else if !pick {
            devices.iter().find(|d| d.default.unwrap_or(false)).cloned()
        } else if cfg!(windows) {
            PickPromptWindows::default().pick(devices)
        } else {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "This system doesn't support device picker",
            ));
        };
        return Ok(device);
    }
}

trait PickPrompt: Default {
    fn pick<D: AsRef<Device>>(&self, devices: Vec<D>) -> Option<Device>;
}
