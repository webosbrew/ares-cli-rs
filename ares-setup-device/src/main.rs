use std::process::exit;

use ares_device_lib::DeviceManager;
use clap::Parser;

mod info;
mod output;

use info::{build_device, modified_device, parse_info};
use output::{print_list, print_list_full};

#[derive(Parser, Debug)]
#[command(about)]
struct Cli {
    #[arg(short = 'l', long, group = "action", help = "List the devices")]
    list: bool,
    #[arg(
        short = 'F',
        long = "listfull",
        group = "action",
        help = "List the devices with detailed information"
    )]
    list_full: bool,
    #[arg(
        short = 'a',
        long,
        value_name = "NAME",
        group = "action",
        help = "Add a device with NAME (use --info to provide details)"
    )]
    add: Option<String>,
    #[arg(
        short = 'm',
        long,
        value_name = "NAME",
        group = "action",
        help = "Modify the device with NAME (use --info to provide changes)"
    )]
    modify: Option<String>,
    #[arg(
        short = 'r',
        long,
        value_name = "NAME",
        group = "action",
        help = "Remove the device with NAME"
    )]
    remove: Option<String>,
    #[arg(
        short = 'f',
        long,
        value_name = "NAME",
        group = "action",
        help = "Set the device with NAME as default"
    )]
    default: Option<String>,
    #[arg(
        short = 'R',
        long,
        group = "action",
        help = "Reset the device list to the default"
    )]
    reset: bool,
    #[arg(
        short = 'i',
        long,
        value_name = "INFO",
        help = "Device details as JSON or key=value (repeatable) for --add/--modify"
    )]
    info: Vec<String>,
}

fn main() {
    let cli = Cli::parse();
    let manager = DeviceManager::default();

    if cli.list {
        print_devices(&manager, false);
    } else if cli.list_full {
        print_devices(&manager, true);
    } else if let Some(name) = &cli.add {
        run_add(&manager, name, &cli.info);
    } else if let Some(name) = &cli.modify {
        run_modify(&manager, name, &cli.info);
    } else if let Some(name) = &cli.remove {
        unwrap_or_exit(manager.remove(name, true), "remove device");
        print_devices(&manager, false);
    } else if let Some(name) = &cli.default {
        unwrap_or_exit(manager.set_default(name), "set default device");
        print_devices(&manager, false);
    } else if cli.reset {
        unwrap_or_exit(manager.reset(), "reset devices");
        print_devices(&manager, false);
    } else {
        Cli::parse_from(["", "--help"]);
    }
}

fn run_add(manager: &DeviceManager, name: &str, info: &[String]) {
    let info = unwrap_or_exit(parse_info(info).map_err(into_error), "parse --info");
    let device = unwrap_or_exit(
        build_device(name, &info).map_err(into_error),
        "build device",
    );
    unwrap_or_exit(manager.add(&device), "add device");
    print_devices(manager, false);
}

fn run_modify(manager: &DeviceManager, name: &str, info: &[String]) {
    let info = unwrap_or_exit(parse_info(info).map_err(into_error), "parse --info");
    let Some(existing) = unwrap_or_exit(manager.find_or_default(Some(&name)), "find device") else {
        eprintln!("Device {name} not found");
        exit(1);
    };
    let device = unwrap_or_exit(
        modified_device(&existing, &info).map_err(into_error),
        "build device",
    );
    unwrap_or_exit(manager.modify(name, &device), "modify device");
    print_devices(manager, false);
}

fn print_devices(manager: &DeviceManager, full: bool) {
    let devices = unwrap_or_exit(manager.list(), "list devices");
    if full {
        print_list_full(&devices);
    } else {
        print_list(&devices);
    }
}

fn into_error(message: String) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message)
}

fn unwrap_or_exit<T>(result: Result<T, std::io::Error>, action: &str) -> T {
    result.unwrap_or_else(|e| {
        eprintln!("Failed to {action}: {e}");
        exit(1);
    })
}
