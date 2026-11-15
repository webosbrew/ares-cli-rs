use std::sync::{Arc, Mutex};

use ares_device_lib::Device;
use gtk::prelude::*;
use gtk::{
    Adjustment, Align, Application, ApplicationWindow, Button, Label, ListBox, ListBoxRow,
    Orientation, PolicyType, ScrolledWindow,
};

use crate::picker::PickPrompt;

#[derive(Default)]
pub struct PickPromptGtk3 {}

impl PickPrompt for PickPromptGtk3 {
    fn pick<D: AsRef<Device>>(&self, devices: Vec<D>) -> Option<Device> {
        let app = Application::new(Some("com.ares.devicePickPrompt"), Default::default());
        let items: Vec<Device> = devices.iter().map(|d| d.as_ref().clone()).collect();
        let result_index: Arc<Mutex<i32>> = Arc::new(Mutex::new(-1));
        let ui_result = result_index.clone();
        app.connect_activate(move |app| {
            let window = Arc::new(ApplicationWindow::new(app));
            let ui_selected: Arc<Mutex<i32>> = Arc::new(Mutex::new(-1));

            window.set_title("Select Device");
            window.set_border_width(10);
            window.set_position(gtk::WindowPosition::Center);
            window.set_default_size(400, 300);
            window.set_resizable(false);
            window.set_keep_above(true);

            let content = gtk::Box::new(Orientation::Vertical, 5);

            let list = ListBox::new();
            let mut selected_row: i32 = -1;
            for (index, item) in items.iter().enumerate() {
                let row = ListBoxRow::new();
                let label = Label::new(Some(&item.name));
                label.set_halign(Align::Start);
                row.set_child(Some(&label));
                list.insert(&row, -1);
                if item.default.unwrap_or(false) {
                    selected_row = index as i32;
                }
            }
            let default_row = list.row_at_index(selected_row);
            list.select_row(default_row.as_ref());
            list.set_activate_on_single_click(false);
            let index = ui_selected.clone();
            list.connect_row_selected(move |_, selected| {
                *index.lock().unwrap() = selected.map(|row| row.index()).unwrap_or(-1);
            });

            {
                let window = window.clone();
                let ui_result = ui_result.clone();
                list.connect_row_activated(move |_, selected| {
                    *ui_result.lock().unwrap() = selected.index();
                    window.close();
                });
            }

            // Scrolls inside the window's fixed height, however many devices
            // there are.
            let scroller = ScrolledWindow::new(None::<&Adjustment>, None::<&Adjustment>);
            scroller.set_policy(PolicyType::Never, PolicyType::Automatic);
            scroller.set_vexpand(true);
            scroller.add(&list);
            list.set_focus_vadjustment(&scroller.vadjustment());
            content.add(&scroller);

            let buttons = gtk::Box::new(Orientation::Horizontal, 5);
            let ok = Button::with_label("OK");
            let cancel = Button::with_label("Cancel");

            buttons.add(&ok);
            buttons.add(&cancel);
            buttons.set_valign(Align::End);
            buttons.set_halign(Align::End);

            {
                let window = window.clone();
                let ui_selected = ui_selected.clone();
                let ui_result = ui_result.clone();
                ok.connect_clicked(move |_| {
                    *ui_result.lock().unwrap() = *ui_selected.lock().unwrap();
                    window.close();
                });
            }

            {
                let window = window.clone();
                cancel.connect_clicked(move |_| {
                    window.close();
                });
            }

            content.add(&buttons);

            window.add(&content);

            window.show_all();
            // Focusing the default row scrolls it into view.
            if let Some(row) = default_row {
                row.grab_focus();
            }
        });
        app.run_with_args::<&str>(&[]);

        let index = result_index.lock().unwrap().clone();
        if index < 0 {
            return None;
        }
        return devices.get(index as usize).map(|v| v.as_ref().clone());
    }
}
