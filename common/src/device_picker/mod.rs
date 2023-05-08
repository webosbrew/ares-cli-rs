use std::io::Error;

use crate::device_manager::{Device, DeviceManager};
#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
use crate::device_picker::gtk::PickPromptGtk;
#[cfg(target_os = "windows")]
use crate::device_picker::windows::PickPromptWindows;

#[cfg(target_os = "windows")]
mod windows;

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
mod gtk;

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
        } else {
            pick_prompt().pick(devices)
        };
        return Ok(device);
    }
}

trait PickPrompt: Default {
    fn pick<D: AsRef<Device>>(&self, devices: Vec<D>) -> Option<Device>;
}

fn pick_prompt() -> impl PickPrompt {
    #[cfg(target_os = "windows")]
    return PickPromptWindows::default();
    #[cfg(target_os = "macos")]
    todo!("macOS is not yet supported");
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    return PickPromptGtk::default();
}
