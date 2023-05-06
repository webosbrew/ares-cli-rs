use std::path::PathBuf;
use std::process::exit;

use clap::Parser;

use crate::install::InstallError;
use common::device_manager::DeviceManager;
use common::session::NewSession;
use install::InstallApp;
use list::ListApps;

mod install;
mod list;

#[derive(Parser, Debug)]
#[command(about)]
struct Cli {
    #[arg(
        short,
        long,
        group = "device_group",
        value_name = "DEVICE",
        help = "Specify DEVICE to use"
    )]
    device: Option<String>,
    #[arg(short, long, group = "device_group", help = "Open device chooser")]
    pick_device: bool,
    #[arg(short, long, group = "action", help = "List the installed apps")]
    list: bool,
    #[arg(
        short,
        long,
        group = "action",
        value_name = "APP_ID",
        help = "Remove app with APP_ID"
    )]
    remove: Option<String>,
    #[arg(
        value_name = "PACKAGE_FILE",
        group = "action",
        help = "webOS package with .ipk extension"
    )]
    package: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();
    let manager = DeviceManager::default();
    let devices = manager.list().unwrap();
    let device = if let Some(s) = cli.device {
        devices.iter().find(|d| d.name == s)
    } else {
        devices.iter().find(|d| d.default.unwrap_or(false))
    };
    if device.is_none() {
        eprintln!("Device not found");
        exit(1);
    }
    let device = device.unwrap();
    let session = device.new_session().unwrap();
    if cli.list {
        session.list_apps();
    } else if let Some(id) = cli.remove {
        println!("Removing {id}...");
    } else if let Some(package) = cli.package {
        println!("Installing {}...", package.to_string_lossy());
        match session.install_app(package) {
            Ok(package_id) => println!("{package_id} installed."),
            Err(e) => {
                eprintln!("{e:?}");
                exit(1);
            }
        }
    } else {
        Cli::parse_from(vec!["", "--help"]);
    }
}
