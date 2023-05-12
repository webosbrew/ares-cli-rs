extern crate native_windows_derive as nwd;
extern crate native_windows_gui as nwg;

use std::fmt::{Display, Formatter};

use nwd::NwgUi;
use nwg::NativeUi;

use ares_device_lib::Device;

use crate::picker::PickPrompt;

#[derive(Default)]
pub struct PickPromptWindows {}

impl PickPrompt for PickPromptWindows {
    fn pick<D: AsRef<Device>>(&self, devices: Vec<D>) -> Option<Device> {
        nwg::init().expect("Failed to init Native Windows GUI");
        nwg::Font::set_global_family("Segoe UI").expect("Failed to set default font");

        let app = PickPromptApp::default();
        let ui = PickPromptApp::build_ui(app).expect("Failed to build UI");
        ui.devices.set_collection(
            devices
                .iter()
                .map(|d| DeviceEntry {
                    device: Some(d.as_ref().clone()),
                })
                .collect(),
        );

        nwg::dispatch_thread_events();
        if let Some(index) = ui.devices.selection() {
            return devices.get(index).map(|d| d.as_ref().clone());
        }
        return None;
    }
}

#[derive(Default, NwgUi)]
pub struct PickPromptApp {
    #[nwg_control(size: (400, 500), center: true, topmost:true, title: "Select Device", flags: "WINDOW|VISIBLE")]
    #[nwg_events( OnWindowClose: [PickPromptApp::on_close] )]
    window: nwg::Window,

    #[nwg_control(size: (380, 420), position: (10, 10))]
    #[nwg_events( OnListBoxSelect: [PickPromptApp::on_selection_change], OnListBoxDoubleClick: [PickPromptApp::on_confirm] )]
    devices: nwg::ListBox<DeviceEntry>,

    #[nwg_control(text: "Select", size: (185, 60), position: (10, 420), enabled: false)]
    #[nwg_events( OnButtonClick: [PickPromptApp::on_confirm] )]
    ok: nwg::Button,

    #[nwg_control(text: "Cancel", size: (185, 60), position: (205, 420))]
    #[nwg_events( OnButtonClick: [PickPromptApp::on_cancel] )]
    cancel: nwg::Button,
}

#[derive(Default)]
struct DeviceEntry {
    device: Option<Device>,
}

impl Display for DeviceEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        return if let Some(device) = &self.device {
            f.write_str(&device.name)
        } else {
            f.write_str("<none>")
        };
    }
}

impl PickPromptApp {
    fn on_close(&self) {
        nwg::stop_thread_dispatch();
    }

    fn on_confirm(&self) {
        self.window.close();
    }

    fn on_cancel(&self) {
        self.window.close();
    }

    fn on_selection_change(&self) {
        self.ok.set_enabled(self.devices.selection().is_some());
    }
}
