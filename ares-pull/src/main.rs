use std::fs::{File, create_dir_all};
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};
use std::process::exit;

use ares_connection_lib::session::NewSession;
use ares_connection_lib::transfer::{PathKind, Transfer, TransferError};
use ares_device_lib::DeviceManager;
use ares_device_lib::cli::unwrap_or_exit;
use clap::Parser;

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
    #[arg(short, long, help = "Hide the detailed copy messages")]
    ignore: bool,
    #[arg(
        short,
        long,
        help = "Continue on errors instead of stopping at the first failure"
    )]
    keep_going: bool,
    #[arg(
        value_name = "SOURCE",
        help = "Path on the DEVICE, where files exist",
        required = true
    )]
    source: String,
    #[arg(
        value_name = "DESTINATION",
        default_value = ".",
        help = "Path on the host machine, where files are copied to"
    )]
    destination: String,
}

/// Directory nesting we refuse to go past. Symlinks are followed, so a link
/// that points at a parent would otherwise never end.
const MAX_DEPTH: usize = 64;

fn main() {
    let cli = Cli::parse();
    let manager = DeviceManager::default();
    let Some(device) = unwrap_or_exit(manager.find_or_default(cli.device.as_ref()), "find device")
    else {
        eprintln!("Device not found");
        exit(1);
    };
    let session = unwrap_or_exit(device.new_session(), &format!("connect to {}", device.name));
    // Open the transport once. It is SFTP unless the device is set to stream,
    // and a copy of many files would otherwise pay a handshake per file.
    let transfer = Transfer::open(&session);

    let mut pull = Pull {
        transfer: &transfer,
        quiet: cli.ignore,
        keep_going: cli.keep_going,
        failed: false,
    };
    if let Err(e) = pull.run(&cli.source, &cli.destination) {
        eprintln!("Failed to pull: {e}");
        exit(1);
    }
    if pull.failed {
        exit(1);
    }
}

struct Pull<'a> {
    transfer: &'a Transfer<'a>,
    quiet: bool,
    keep_going: bool,
    /// Set when --keep-going swallowed a failure, so the exit code still says so.
    failed: bool,
}

impl Pull<'_> {
    fn run(&mut self, source: &str, destination: &str) -> Result<(), Error> {
        let source_kind = self.stat(source)?;
        if source_kind == PathKind::Missing {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("SOURCE {source} does not exist on the device"),
            ));
        }
        let source_is_dir = source_kind == PathKind::Dir;
        let target = resolve_target(
            source,
            destination,
            source_is_dir,
            Path::new(destination).is_dir(),
        );
        if source_is_dir && target.exists() && !target.is_dir() {
            return Err(Error::new(
                ErrorKind::AlreadyExists,
                format!("{} is not a directory", target.display()),
            ));
        }
        self.copy(source, source_kind, &target, 0)
    }

    fn copy(
        &mut self,
        remote: &str,
        kind: PathKind,
        local: &Path,
        depth: usize,
    ) -> Result<(), Error> {
        match kind {
            PathKind::Dir => {
                if depth >= MAX_DEPTH {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        format!("{remote} is nested too deep, which usually means a symlink loop"),
                    ));
                }
                self.copy_dir(remote, local, depth)
            }
            PathKind::File => self.copy_file(remote, local),
            // ares-cli walks with `find -follow`, which lists a device node or a
            // broken symlink as neither a file nor a directory and skips it.
            PathKind::Other | PathKind::Missing => {
                eprintln!("Skipping {remote}: it is not a file or a directory");
                Ok(())
            }
        }
    }

    fn copy_dir(&mut self, remote: &str, local: &Path, depth: usize) -> Result<(), Error> {
        create_dir_all(local)?;
        self.report(remote, local);
        let entries = self
            .transfer
            .read_dir(remote)
            .map_err(|e| transfer_error(remote, &e))?;
        for entry in entries {
            let child_remote = format!("{}/{}", remote.trim_end_matches('/'), entry.name);
            let child_local = local.join(&entry.name);
            if let Err(e) = self.copy(&child_remote, entry.kind, &child_local, depth + 1) {
                self.item_failed(&child_remote, e)?;
            }
        }
        Ok(())
    }

    fn copy_file(&mut self, remote: &str, local: &Path) -> Result<(), Error> {
        if let Some(parent) = local.parent() {
            create_dir_all(parent)?;
        }
        self.report(remote, local);
        let mut local_file = File::create(local)?;
        self.transfer
            .get(remote, &mut local_file, |_| {})
            .map_err(|e| transfer_error(remote, &e))
    }

    fn stat(&self, remote: &str) -> Result<PathKind, Error> {
        self.transfer
            .stat(remote)
            .map_err(|e| transfer_error(remote, &e))
    }

    fn report(&self, remote: &str, local: &Path) {
        if !self.quiet {
            println!("{remote} => {}", local.display());
        }
    }

    /// Handle a failure on one item. Returns the error to stop the whole copy,
    /// or Ok to go on when --keep-going is set.
    fn item_failed(&mut self, what: &str, e: Error) -> Result<(), Error> {
        if !self.keep_going {
            return Err(e);
        }
        eprintln!("Skipping {what}: {e}");
        self.failed = true;
        Ok(())
    }
}

/// Where SOURCE lands on the host.
///
/// This follows ares-cli: a directory always keeps its own name under
/// DESTINATION, and a file keeps its name only when DESTINATION already is a
/// directory. A trailing "/" changes nothing.
fn resolve_target(
    source: &str,
    destination: &str,
    source_is_dir: bool,
    dest_is_dir: bool,
) -> PathBuf {
    let dest = Path::new(destination);
    if !source_is_dir && !dest_is_dir {
        return dest.to_path_buf();
    }
    match remote_name(source) {
        Some(name) => dest.join(name),
        None => dest.to_path_buf(),
    }
}

/// Last component of a device path, which always uses "/". Returns None for a
/// path with no name of its own ("/", "." and ".."), where the copy goes
/// straight into DESTINATION.
fn remote_name(path: &str) -> Option<&str> {
    let name = path.trim_end_matches('/').rsplit('/').next()?;
    if name.is_empty() || name == "." || name == ".." {
        return None;
    }
    Some(name)
}

/// Name the path a transfer failed on.
fn transfer_error(path: &str, e: &TransferError) -> Error {
    Error::other(format!("{path}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::{remote_name, resolve_target};

    fn target(source: &str, destination: &str, source_is_dir: bool, dest_is_dir: bool) -> String {
        resolve_target(source, destination, source_is_dir, dest_is_dir)
            .to_string_lossy()
            .replace('\\', "/")
    }

    #[test]
    fn a_file_takes_the_destination_name() {
        assert_eq!(target("/remote/f.txt", "out.txt", false, false), "out.txt");
    }

    #[test]
    fn a_file_keeps_its_name_under_a_directory() {
        assert_eq!(target("/remote/f.txt", "out", false, true), "out/f.txt");
    }

    #[test]
    fn a_directory_always_keeps_its_own_name() {
        // The point of the ares-cli rule: "dir" lands as out/dir, whether or not
        // "out" already exists.
        assert_eq!(target("/remote/dir", "out", true, true), "out/dir");
        assert_eq!(target("/remote/dir", "out", true, false), "out/dir");
    }

    #[test]
    fn a_trailing_slash_changes_nothing() {
        assert_eq!(target("/remote/dir/", "out", true, true), "out/dir");
        assert_eq!(target("/remote/dir", "out/", true, true), "out/dir");
        assert_eq!(target("/remote/f.txt", "out/", false, true), "out/f.txt");
    }

    #[test]
    fn a_source_with_no_name_copies_its_contents() {
        assert_eq!(remote_name("/"), None);
        assert_eq!(remote_name("."), None);
        assert_eq!(remote_name(".."), None);
        assert_eq!(target("/", "out", true, true), "out");
    }

    #[test]
    fn names_come_from_the_last_component() {
        assert_eq!(remote_name("/var/log/messages"), Some("messages"));
        assert_eq!(remote_name("/var/log/"), Some("log"));
        assert_eq!(remote_name("messages"), Some("messages"));
    }
}
