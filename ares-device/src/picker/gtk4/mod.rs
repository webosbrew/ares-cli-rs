use std::cell::Cell;
use std::rc::Rc;

use ares_device_lib::Device;
use gtk4::prelude::*;
use gtk4::{
    Align, Application, ApplicationWindow, Button, Label, ListBox, ListBoxRow, Orientation,
    PolicyType, ScrolledWindow, gio,
};

use crate::picker::PickPrompt;

#[derive(Default)]
pub struct PickPromptGtk4 {}

impl PickPrompt for PickPromptGtk4 {
    fn pick<D: AsRef<Device>>(&self, devices: Vec<D>) -> Option<Device> {
        let app = Application::new(
            Some("com.ares.devicePickPrompt"),
            gio::ApplicationFlags::default(),
        );
        let items: Vec<Device> = devices.iter().map(|d| d.as_ref().clone()).collect();
        let result_index: Rc<Cell<i32>> = Rc::new(Cell::new(-1));
        let ui_result = result_index.clone();
        app.connect_activate(move |app| {
            let window = ApplicationWindow::new(app);
            let ui_selected: Rc<Cell<i32>> = Rc::new(Cell::new(-1));

            // GTK 4 has no window positioning or keep-above; the window
            // manager decides where the picker appears and how it stacks.
            window.set_title(Some("Select Device"));
            window.set_default_size(400, 300);
            window.set_resizable(false);

            let content = gtk4::Box::new(Orientation::Vertical, 5);
            // Stands in for GTK 3's container border width.
            content.set_margin_top(10);
            content.set_margin_bottom(10);
            content.set_margin_start(10);
            content.set_margin_end(10);

            let list = ListBox::new();
            let mut selected_row: Option<i32> = None;
            for (index, item) in items.iter().enumerate() {
                let row = ListBoxRow::new();
                let label = Label::new(Some(&item.name));
                label.set_halign(Align::Start);
                row.set_child(Some(&label));
                list.append(&row);
                if item.default.unwrap_or(false) {
                    selected_row = i32::try_from(index).ok();
                }
            }
            list.set_activate_on_single_click(false);
            let index = ui_selected.clone();
            list.connect_row_selected(move |_, selected| {
                index.set(selected.map_or(-1, ListBoxRowExt::index));
            });
            // Selected after the handler is connected, so OK on the
            // preselected default device returns it.
            let default_row = selected_row.and_then(|i| list.row_at_index(i));
            list.select_row(default_row.as_ref());

            {
                let window = window.clone();
                let ui_result = ui_result.clone();
                list.connect_row_activated(move |_, selected| {
                    ui_result.set(selected.index());
                    window.close();
                });
            }

            // Scrolls inside the window's fixed height, however many devices
            // there are.
            let scroller = ScrolledWindow::new();
            scroller.set_policy(PolicyType::Never, PolicyType::Automatic);
            scroller.set_vexpand(true);
            scroller.set_child(Some(&list));
            content.append(&scroller);

            let buttons = gtk4::Box::new(Orientation::Horizontal, 5);
            let ok = Button::with_label("OK");
            let cancel = Button::with_label("Cancel");

            buttons.append(&ok);
            buttons.append(&cancel);
            buttons.set_valign(Align::End);
            buttons.set_halign(Align::End);

            {
                let window = window.clone();
                let ui_selected = ui_selected.clone();
                let ui_result = ui_result.clone();
                ok.connect_clicked(move |_| {
                    ui_result.set(ui_selected.get());
                    window.close();
                });
            }

            {
                let window = window.clone();
                cancel.connect_clicked(move |_| {
                    window.close();
                });
            }

            content.append(&buttons);

            window.set_child(Some(&content));

            window.present();
            // Focusing the default row scrolls it into view.
            if let Some(row) = default_row {
                row.grab_focus();
            }
        });
        app.run_with_args::<&str>(&[]);

        let index = usize::try_from(result_index.get()).ok()?;
        devices.get(index).map(|v| v.as_ref().clone())
    }
}
