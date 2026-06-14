use std::path::PathBuf;
use std::process::exit;

use ares_connection_lib::session::NewSession;
use ares_device_lib::DeviceManager;
use clap::Parser;
use install::InstallApp;
use list::ListApps;

use crate::remove::RemoveApp;

mod install;
mod list;
mod remove;

#[derive(Parser, Debug)]
#[command(about)]
struct Cli {
    #[arg(
        short,
        long,
        value_name = "DEVICE",
        env = "ARES_DEVICE",
        help = "Specify DEVICE to use"
    )]
    device: Option<String>,
    #[arg(short, long, group = "action", help = "List the installed apps")]
    list: bool,
    #[arg(
        short = 'F',
        long = "listfull",
        group = "action",
        help = "List the installed apps with detailed information"
    )]
    list_full: bool,
    #[arg(
        short = 't',
        long = "type",
        value_name = "APP_TYPE",
        help = "Filter the listed apps by APP_TYPE"
    )]
    app_type: Option<String>,
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
    let device = manager.find_or_default(cli.device.as_ref()).unwrap();
    if device.is_none() {
        eprintln!("Device not found");
        exit(1);
    }
    let device = device.unwrap();
    let session = device.new_session().unwrap();
    if cli.list || cli.list_full {
        session.list_apps(cli.list_full, cli.app_type.as_deref());
    } else if let Some(id) = cli.remove {
        println!("Removing {id}...");
        match session.remove_app(&id) {
            Ok(_) => println!("{id} removed."),
            Err(e) => {
                eprintln!("Failed to remove {id}: {e:?}");
                exit(1);
            }
        }
    } else if let Some(package) = cli.package {
        match session.install_app(package) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Failed to install: {e:?}");
                exit(1);
            }
        }
    } else {
        Cli::parse_from(vec!["", "--help"]);
    }
}
