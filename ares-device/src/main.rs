use std::process::exit;

use clap::Parser;

mod picker;

use ares_device_lib::cli::unwrap_or_exit;
use ares_device_lib::{Device, DeviceManager};
use picker::{DeviceSelection, PickDevice};

#[derive(Parser, Debug)]
#[command(about)]
struct Cli {
    #[arg(
        short,
        long,
        default_missing_value = "",
        num_args = 0..2,
        value_name = "DEVICE",
        env = "ARES_DEVICE",
        help = "Specify DEVICE to use, show picker if no value specified"
    )]
    device: Option<DeviceSelection>,
    #[arg(short = 'D', long = "device-list", help = "List the available devices")]
    device_list: bool,
}

fn main() {
    let cli = Cli::parse();
    let manager = DeviceManager::default();

    if cli.device_list {
        list_devices(&manager);
        return;
    }

    let Some(device) = unwrap_or_exit(manager.pick(cli.device.as_ref()), "find device") else {
        eprintln!("Device not found");
        exit(1);
    };
    println!("{}", device.name);
}

fn list_devices(manager: &DeviceManager) {
    let devices = unwrap_or_exit(manager.list(), "list devices");

    let headers = ["name", "deviceinfo", "connection", "profile", "passphrase"];
    let rows: Vec<[String; 5]> = devices.iter().map(device_row).collect();
    print_table(&headers, &rows);
}

fn device_row(device: &Device) -> [String; 5] {
    let name = if device.default == Some(true) {
        format!("{} (default)", device.name)
    } else {
        device.name.clone()
    };
    [
        name,
        format!("{}@{}:{}", device.username, device.host, device.port),
        String::from("ssh"),
        device.profile.clone(),
        device.passphrase.clone().unwrap_or_default(),
    ]
}

/// Prints a left-aligned, space-padded table with a dashed header underline,
/// mirroring the reference CLI's device list output.
fn print_table<const N: usize>(headers: &[&str; N], rows: &[[String; N]]) {
    let mut widths = headers.map(str::len);
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.len());
        }
    }

    let print_row = |cells: &[String; N]| {
        let line: Vec<String> = cells
            .iter()
            .enumerate()
            .map(|(i, cell)| format!("{cell:<width$}", width = widths[i]))
            .collect();
        println!("{}", line.join("  ").trim_end());
    };

    let header_row: [String; N] = std::array::from_fn(|i| headers[i].to_string());
    let separator_row: [String; N] = std::array::from_fn(|i| "-".repeat(widths[i]));
    print_row(&header_row);
    print_row(&separator_row);
    for row in rows {
        print_row(row);
    }
}
