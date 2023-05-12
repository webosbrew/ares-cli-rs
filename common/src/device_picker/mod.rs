use std::io::Error;
use std::str::FromStr;

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
    fn pick(&self, selection: Option<&DeviceSelection>) -> Result<Option<Device>, Error>;
}

#[derive(Clone, Debug)]
pub enum DeviceSelection {
    Name(String),
    Pick,
}

trait PickPrompt: Default {
    fn pick<D: AsRef<Device>>(&self, devices: Vec<D>) -> Option<Device>;
}

impl PickDevice for DeviceManager {
    fn pick(&self, selection: Option<&DeviceSelection>) -> Result<Option<Device>, Error> {
        let devices = self.list()?;
        let device = match selection {
            Some(DeviceSelection::Name(s)) => {
                devices.iter().find(|d| &d.name == s).cloned()
            }
            Some(DeviceSelection::Pick) => pick_prompt().pick(devices),
            None => devices.iter().find(|d| d.default.unwrap_or(false)).cloned(),
        };
        return Ok(device);
    }
}

impl FromStr for DeviceSelection {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        return if s.is_empty() {
            Ok(Self::Pick)
        } else {
            Ok(Self::Name(s.to_string()))
        };
    }
}

fn pick_prompt() -> impl PickPrompt {
    #[cfg(target_os = "windows")]
    return PickPromptWindows::default();
    #[cfg(target_os = "macos")]
    todo!("macOS is not yet supported");
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    return PickPromptGtk::default();
}
