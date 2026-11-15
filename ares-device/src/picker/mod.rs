use std::io::Error;
use std::str::FromStr;

use ares_device_lib::{Device, DeviceManager};
cfg_if::cfg_if! {
    if #[cfg(target_os="windows")] {
        mod windows;
    } else if #[cfg(target_os = "macos")] {
    } else if #[cfg(feature = "gtk4")] {
        mod gtk4;
    } else if #[cfg(feature = "gtk3")] {
        mod gtk3;
    }
}

pub trait PickDevice {
    fn pick(&self, selection: Option<&DeviceSelection>) -> Result<Option<Device>, Error>;
}

#[derive(Clone, Debug)]
pub enum DeviceSelection {
    Name(String),
    Pick,
}

// Unused where there is no picker: macOS, or a build with neither GTK feature.
#[cfg_attr(
    not(any(
        target_os = "windows",
        all(not(target_os = "macos"), any(feature = "gtk4", feature = "gtk3"))
    )),
    allow(dead_code)
)]
trait PickPrompt: Default {
    fn pick<D: AsRef<Device>>(&self, devices: Vec<D>) -> Option<Device>;
}

impl PickDevice for DeviceManager {
    fn pick(&self, selection: Option<&DeviceSelection>) -> Result<Option<Device>, Error> {
        let devices = self.list()?;
        let device = match selection {
            Some(DeviceSelection::Name(s)) => devices.iter().find(|d| &d.name == s).cloned(),
            Some(DeviceSelection::Pick) => {
                cfg_if::cfg_if! {
                    if #[cfg(target_os="windows")] {
                        windows::PickPromptWindows::default().pick(devices)
                    } else if #[cfg(target_os = "macos")] {
                        None
                    } else if #[cfg(feature = "gtk4")] {
                        gtk4::PickPromptGtk4::default().pick(devices)
                    } else if #[cfg(feature = "gtk3")] {
                        gtk3::PickPromptGtk3::default().pick(devices)
                    } else {
                        None
                    }
                }
            }
            None => devices.iter().find(|d| d.default.unwrap_or(false)).cloned(),
        };
        Ok(device)
    }
}

impl FromStr for DeviceSelection {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            Ok(Self::Pick)
        } else {
            Ok(Self::Name(s.to_string()))
        }
    }
}
