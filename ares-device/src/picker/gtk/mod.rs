use std::sync::{Arc, Mutex};

use ares_device_lib::Device;
use gtk::prelude::*;
use gtk::{Align, Application, ApplicationWindow, Button, Label, ListBox, ListBoxRow, Orientation};

use crate::picker::PickPrompt;

#[derive(Default)]
pub struct PickPromptGtk {}

impl PickPrompt for PickPromptGtk {
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
            for item in &items {
                let row = ListBoxRow::new();
                let label = Label::new(Some(&item.name));
                label.set_halign(Align::Start);
                row.set_child(Some(&label));
                list.insert(&row, -1);
            }
            list.set_vexpand(true);
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

            content.add(&list);

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
        });
        app.run_with_args::<&str>(&[]);

        let index = result_index.lock().unwrap().clone();
        if index < 0 {
            return None;
        }
        return devices.get(index as usize).map(|v| v.as_ref().clone());
    }
}
