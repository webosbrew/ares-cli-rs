use std::io::stdout;
use std::process::exit;

use clap::Parser;
use crossterm::terminal;
use crossterm::tty::IsTty;

use ares_connection_lib::session::NewSession;
use ares_device_lib::DeviceManager;

mod dumb;
mod pty;

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
    #[arg(short, long, value_name = "COMMAND", help = "Run COMMAND")]
    run: Option<String>,
    #[arg(long, group = "pty_opt", help = "Force pseudo-terminal allocation")]
    pty: bool,
    #[arg(long, group = "pty_opt", help = "Disable pseudo-terminal allocation")]
    no_pty: bool,
}

fn main() {
    let cli = Cli::parse();
    let manager = DeviceManager::default();
    let Some(device) = manager.find_or_default(cli.device).unwrap() else {
        eprintln!("Device not found");
        exit(255);
    };

    let session = device.new_session().unwrap();
    let ch = session.new_channel().unwrap();
    ch.open_session().unwrap();
    let mut has_pty = false;
    if !cli.no_pty && (cli.pty || stdout().is_tty()) {
        let (width, height) = terminal::size().unwrap_or((80, 24));
        if let Err(e) = ch.request_pty("xterm", width as u32, height as u32) {
            eprintln!("Can't request pty: {:?}", e);
            if cli.pty {
                exit(255);
            }
        } else {
            has_pty = true;
        }
    }
    if let Some(command) = cli.run {
        ch.request_exec(&command).unwrap();
    } else {
        ch.request_shell().unwrap();
    }
    let result = if has_pty {
        pty::shell(ch)
    } else {
        dumb::shell(ch)
    };
    match result {
        Ok(code) => exit(code),
        Err(e) => {
            eprintln!("Error: {:?}", e);
        }
    }
}
