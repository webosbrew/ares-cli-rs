use std::io::Write;
use std::io::{stdout, Read};
use std::process::exit;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use clap::Parser;
use crossbeam_channel::internal::SelectHandle;
use crossbeam_channel::unbounded;
use crossterm::event::Event;
use crossterm::tty::IsTty;
use crossterm::{terminal, Command};

use ares_connection_lib::session::NewSession;
use ares_device_lib::DeviceManager;

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
        exit(1);
    };

    let session = device.new_session().unwrap();
    let ch = session.new_channel().unwrap();
    ch.open_session().unwrap();
    let (width, height) = terminal::size().unwrap_or((80, 24));
    let mut has_pty = false;
    if !cli.no_pty && (cli.pty || stdout().is_tty()) {
        if let Err(e) = ch.request_pty("xterm", width as u32, height as u32) {
            eprintln!("Can't request pty: {:?}", e);
            if cli.pty {
                exit(1);
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
    session.set_blocking(false);
    if has_pty {
        terminal::enable_raw_mode().unwrap();
    }
    let (tx, rx) = unbounded::<Event>();
    let terminated = Arc::new(Mutex::new(false));
    let thread_terminated = Arc::downgrade(&terminated);
    let join = thread::spawn(move || loop {
        if let Some(terminated) = thread_terminated.upgrade() {
            if *terminated.lock().unwrap() {
                break;
            }
        } else {
            break;
        }
        let Ok(has_event) = crossterm::event::poll(Duration::from_millis(20)) else {
            break;
        };
        if !has_event {
            continue;
        }
        let Ok(event) = crossterm::event::read() else {
            break;
        };
        if !tx.send(event).is_ok() {
            break;
        }
    });

    let result = pty::shell(&ch, rx, has_pty);
    if has_pty {
        terminal::disable_raw_mode().unwrap();
    }
    *terminated.lock().unwrap() = true;
    join.join().unwrap();
    session.set_blocking(true);
    match result {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Error: {:?}", e);
        }
    }
}
