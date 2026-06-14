use std::fs::{File, create_dir_all};
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};
use std::process::exit;

use ares_connection_lib::session::NewSession;
use ares_device_lib::DeviceManager;
use clap::Parser;
use libssh_rs::{FileType, OpenFlags, Sftp};

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
    #[arg(
        short,
        long,
        help = "Continue on errors instead of stopping at the first failure"
    )]
    ignore: bool,
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

fn main() {
    let cli = Cli::parse();
    let manager = DeviceManager::default();
    let Some(device) = manager.find_or_default(cli.device.as_ref()).unwrap() else {
        eprintln!("Device not found");
        exit(1);
    };
    let session = device.new_session().unwrap();
    let sftp = session.sftp().unwrap();

    let target = resolve_target(&cli.source, &cli.destination);
    if let Err(e) = pull(&sftp, &cli.source, &target, cli.ignore) {
        eprintln!("Failed to pull: {e}");
        exit(1);
    }
}

/// Maps the remote `source` onto a local destination path. Mirrors ares-push:
/// a trailing "/" on the destination keeps the source's last path component,
/// otherwise the source maps directly onto the destination.
fn resolve_target(source: &str, destination: &str) -> PathBuf {
    let source = Path::new(source);
    let dest_base = Path::new(destination);
    let source_prefix = if destination.ends_with('/') {
        source.parent().unwrap_or(source)
    } else {
        source
    };
    match source.strip_prefix(source_prefix) {
        Ok(relative) if !relative.as_os_str().is_empty() => dest_base.join(relative),
        _ => dest_base.to_path_buf(),
    }
}

/// Recursively pulls `remote` into the local path `local`.
fn pull(sftp: &Sftp, remote: &str, local: &Path, ignore: bool) -> Result<(), Error> {
    let metadata = sftp.symlink_metadata(remote).map_err(to_io)?;
    match metadata.file_type() {
        Some(FileType::Symlink) => {
            eprintln!("Skipping symlink {remote}");
            Ok(())
        }
        Some(FileType::Directory) => pull_dir(sftp, remote, local, ignore),
        _ => pull_file(sftp, remote, local),
    }
}

fn pull_dir(sftp: &Sftp, remote: &str, local: &Path, ignore: bool) -> Result<(), Error> {
    create_dir_all(local)?;
    println!("{remote} => {}", local.display());
    for entry in sftp.read_dir(remote).map_err(to_io)? {
        let Some(name) = entry.name() else { continue };
        if name == "." || name == ".." {
            continue;
        }
        let child_remote = format!("{}/{name}", remote.trim_end_matches('/'));
        let child_local = local.join(name);
        if let Err(e) = pull(sftp, &child_remote, &child_local, ignore) {
            if ignore {
                eprintln!("Skipping {child_remote}: {e}");
            } else {
                return Err(e);
            }
        }
    }
    Ok(())
}

fn pull_file(sftp: &Sftp, remote: &str, local: &Path) -> Result<(), Error> {
    if let Some(parent) = local.parent() {
        create_dir_all(parent)?;
    }
    println!("{remote} => {}", local.display());
    let mut remote_file = sftp.open(remote, OpenFlags::READ_ONLY, 0).map_err(to_io)?;
    let mut local_file = File::create(local)?;
    std::io::copy(&mut remote_file, &mut local_file)?;
    Ok(())
}

fn to_io(error: libssh_rs::Error) -> Error {
    Error::new(ErrorKind::Other, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::resolve_target;

    fn target(source: &str, destination: &str) -> String {
        resolve_target(source, destination)
            .to_string_lossy()
            .replace('\\', "/")
    }

    #[test]
    fn file_maps_onto_destination() {
        assert_eq!(target("/remote/f.txt", "out.txt"), "out.txt");
    }

    #[test]
    fn trailing_slash_keeps_last_component() {
        assert_eq!(target("/remote/f.txt", "dir/"), "dir/f.txt");
        assert_eq!(target("/remote/dir", "out/"), "out/dir");
    }

    #[test]
    fn directory_contents_map_into_destination() {
        assert_eq!(target("/remote/dir", "out"), "out");
    }
}
