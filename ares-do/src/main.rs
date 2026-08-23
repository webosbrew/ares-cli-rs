//! Drive a webOS device: press buttons, capture the screen, replay a flow.
//!
//! Both services this leans on live on the private luna bus, so everything
//! here needs a root session. See the README for the exit-code table — it is
//! a contract, not an implementation detail.

use std::process::exit;

use ares_connection_lib::session::NewSession;
use ares_device_lib::DeviceManager;
use clap::Parser;

use crate::cli::{Cli, Command};
use crate::error::DoError;
use crate::output::Reporter;
use crate::preflight::Preflight;
use crate::run::Runner;

mod app;
mod cli;
mod error;
mod flow;
mod key;
mod keycode;
mod luna;
mod output;
mod preflight;
mod run;
mod screenshot;
mod text;

fn main() {
    let cli = Cli::parse();
    let reporter = Reporter::new(cli.json, cli.quiet, cli.verbose);

    if let Err(e) = dispatch(&cli, &reporter) {
        reporter.error(&e);
        exit(e.exit_code());
    }
}

fn dispatch(cli: &Cli, reporter: &Reporter) -> Result<(), DoError> {
    // Answers with no TV in the room, which is the point of it.
    if let Command::Keys(args) = &cli.command {
        run::list_keys(args.pattern.as_deref(), args.all, reporter);
        return Ok(());
    }

    // Resolves and prints without connecting — not even a device lookup, so a
    // generated flow can be checked anywhere.
    if cli.dry_run {
        return run::dry_run(cli, reporter);
    }

    if !cli.command.needs_device() {
        return Ok(());
    }

    let manager = DeviceManager::default();
    let device = manager
        .find_or_default(cli.device.as_ref())
        .map_err(DoError::DeviceLookup)?
        .ok_or_else(|| DoError::DeviceNotFound {
            name: cli.device.clone(),
        })?;

    let session = device.new_session().map_err(|source| DoError::Connect {
        device: device.name.clone(),
        source,
    })?;

    // `--timeout 0` means wait forever, which is the only way to run something
    // genuinely long without picking a number for it.
    let timeout = (!cli.timeout.is_zero()).then_some(cli.timeout);

    session.require_root(cli.allow_non_root, timeout, reporter)?;
    run::sweep_leftovers(&session, timeout, reporter);

    Runner::new(&session, timeout, reporter).one(&cli.command)
}
