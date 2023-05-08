use std::process::exit;

use clap::Parser;

use common::device_manager::DeviceManager;
use common::device_picker::PickDevice;
use common::session::NewSession;

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
    #[arg(long, group = "device_group", help = "Open device chooser")]
    pick_device: bool,
    #[arg(
        value_name = "SOURCE",
        help = "Path in the host machine, where files exist."
    )]
    source: String,
    #[arg(
        value_name = "DESTINATION",
        help = "Path in the DEVICE, where multiple files can be copied"
    )]
    destination: String,
}

fn main() {
    let cli = Cli::parse();
    let manager = DeviceManager::default();
    let device = manager.pick(cli.device, cli.pick_device).unwrap();
    if device.is_none() {
        eprintln!("Device not found");
        exit(1);
    }
    let device = device.unwrap();
    let session = device.new_session().unwrap();
}
