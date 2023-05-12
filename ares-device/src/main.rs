use std::process::exit;

use clap::Parser;

mod picker;

use ares_common_device::DeviceManager;
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
}

fn main() {
    let cli = Cli::parse();
    let manager = DeviceManager::default();
    let device = if let Some(d) = manager.pick(cli.device.as_ref()).unwrap() {
        d
    } else {
        eprintln!("Device not found");
        exit(1);
    };
    println!("{}", device.name);
}
