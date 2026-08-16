use std::fs::{File, create_dir_all};
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};
use std::process::exit;

use ares_connection_lib::session::NewSession;
use ares_device_lib::DeviceManager;
use ares_device_lib::cli::unwrap_or_exit;
use clap::Parser;
use libssh_rs::{Error as SshError, FileType, OpenFlags, Sftp};

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
    let sftp = unwrap_or_exit(session.sftp(), "start SFTP");

    let mut pull = Pull {
        sftp: &sftp,
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
    sftp: &'a Sftp,
    quiet: bool,
    keep_going: bool,
    /// Set when --keep-going swallowed a failure, so the exit code still says so.
    failed: bool,
}

impl Pull<'_> {
    fn run(&mut self, source: &str, destination: &str) -> Result<(), Error> {
        let source_is_dir = is_dir(self.sftp, source)
            .map_err(|e| Error::new(e.kind(), format!("SOURCE {source}: {e}")))?;
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
        self.copy(source, &target, 0)
    }

    fn copy(&mut self, remote: &str, local: &Path, depth: usize) -> Result<(), Error> {
        let file_type = match self.sftp.metadata(remote).map(|m| m.file_type()) {
            Ok(file_type) => file_type,
            Err(e) => {
                // ares-cli walks with `find -follow`, which lists a broken
                // symlink as neither a file nor a directory and skips it.
                if matches!(
                    self.sftp.symlink_metadata(remote).map(|m| m.file_type()),
                    Ok(Some(FileType::Symlink))
                ) {
                    eprintln!("Skipping {remote}: it is a broken symlink");
                    return Ok(());
                }
                return Err(sftp_error(remote, &e));
            }
        };
        if file_type == Some(FileType::Directory) {
            if depth >= MAX_DEPTH {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("{remote} is nested too deep, which usually means a symlink loop"),
                ));
            }
            self.copy_dir(remote, local, depth)
        } else {
            self.copy_file(remote, local)
        }
    }

    fn copy_dir(&mut self, remote: &str, local: &Path, depth: usize) -> Result<(), Error> {
        let sftp = self.sftp;
        create_dir_all(local)?;
        self.report(remote, local);
        for entry in sftp.read_dir(remote).map_err(|e| sftp_error(remote, &e))? {
            let Some(name) = entry.name() else { continue };
            if name == "." || name == ".." {
                continue;
            }
            let child_remote = format!("{}/{name}", remote.trim_end_matches('/'));
            let child_local = local.join(name);
            if let Err(e) = self.copy(&child_remote, &child_local, depth + 1) {
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
        let mut remote_file = self
            .sftp
            .open(remote, OpenFlags::READ_ONLY, 0)
            .map_err(|e| sftp_error(remote, &e))?;
        let mut local_file = File::create(local)?;
        std::io::copy(&mut remote_file, &mut local_file)?;
        Ok(())
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

/// True when `path` is a directory on the device. ares-cli tests with `[ -f ]`
/// and `[ -d ]`, which both follow symlinks, so follow them here too.
fn is_dir(sftp: &Sftp, path: &str) -> Result<bool, Error> {
    let metadata = sftp.metadata(path).map_err(|e| sftp_error(path, &e))?;
    Ok(metadata.file_type() == Some(FileType::Directory))
}

/// Recover the numeric SFTP status code from a libssh error. `SftpError`'s code
/// field is private, so parse it out of the Display text ("Sftp error code N").
/// Returns `None` for non-SFTP errors.
fn sftp_status(e: &SshError) -> Option<u32> {
    if !matches!(e, SshError::Sftp(_)) {
        return None;
    }
    e.to_string().rsplit(' ').next()?.parse().ok()
}

/// Human-readable reason for an SFTP status code (subset of `SSH_FX_*` codes).
fn sftp_reason(code: u32) -> &'static str {
    match code {
        2 => "no such file or directory",
        3 => "permission denied",
        4 => "failure",
        8 => "operation not supported",
        _ => "SFTP error",
    }
}

fn sftp_error(path: &str, e: &SshError) -> Error {
    match sftp_status(e) {
        Some(2) => Error::new(
            ErrorKind::NotFound,
            format!("{path} does not exist on the device"),
        ),
        Some(3) => Error::new(
            ErrorKind::PermissionDenied,
            format!("{path}: permission denied"),
        ),
        Some(code) => Error::other(format!("{path}: {} (SFTP code {code})", sftp_reason(code))),
        None => Error::other(format!("{path}: {e}")),
    }
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
