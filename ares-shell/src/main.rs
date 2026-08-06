use std::io::{stdin, stdout};
use std::process::exit;

use ares_connection_lib::session::NewSession;
use ares_device_lib::DeviceManager;
use clap::Parser;
use crossterm::terminal;
use crossterm::tty::IsTty;

mod complete;
mod dumb;
mod interactive;
mod io;
mod marker;
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
    #[arg(
        long,
        help = "Disable the local prompt and line editor used without a pseudo-terminal"
    )]
    no_prompt: bool,
}

fn main() {
    let cli = Cli::parse();
    let manager = DeviceManager::default();
    let Some(device) = manager.find_or_default(cli.device.as_ref()).unwrap() else {
        eprintln!("Device not found");
        exit(255);
    };

    let session = device.new_session().unwrap();
    let ch = session.new_channel().unwrap();
    ch.open_session().unwrap();
    let mut has_pty = false;
    if !cli.no_pty && (cli.pty || stdout().is_tty()) {
        let (width, height) = terminal::size().unwrap_or((80, 24));
        let term = std::env::var("TERM").unwrap_or_else(|_| String::from("xterm"));
        if let Err(e) = ch.request_pty(&term, u32::from(width), u32::from(height)) {
            // Only --pty makes this fatal, so only there is the reason worth
            // printing. Otherwise say what happens next instead.
            if cli.pty {
                eprintln!("The device refused a pseudo-terminal: {e:?}");
                exit(255);
            }
            eprintln!("PTY is not available, using dumb shell instead.");
        } else {
            has_pty = true;
        }
    }
    let run_command = cli.run.is_some();
    if let Some(command) = cli.run {
        ch.request_exec(&command).unwrap();
    } else {
        ch.request_shell().unwrap();
    }
    // Without a remote pty the shell has no prompt, no echo and no line editing.
    // Draw them locally instead, as long as there is a user in front of us.
    let local_prompt =
        !cli.no_prompt && !has_pty && !run_command && stdin().is_tty() && stdout().is_tty();
    let result = if has_pty {
        pty::shell(ch)
    } else if local_prompt {
        interactive::shell(ch, &device)
    } else {
        dumb::shell(ch)
    };
    match result {
        Ok(code) => exit(code),
        Err(e) => {
            eprintln!("Error: {:?}", e);
            exit(1);
        }
    }
}
