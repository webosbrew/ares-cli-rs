use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::exit;

use ares_connection_lib::session::NewSession;
use ares_device_lib::DeviceManager;
use clap::Parser;
use libssh_rs::{Error as SshError, OpenFlags};
use path_slash::PathBufExt;
use walkdir::WalkDir;

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
        help = "Path in the host machine, where files exist.",
        required = true
    )]
    source: Vec<PathBuf>,
    #[arg(
        value_name = "DESTINATION",
        help = "Path in the DEVICE, where multiple files can be copied",
        required = true
    )]
    destination: String,
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

/// Human-readable reason for an SFTP status code (subset of SSH_FX_* codes).
fn sftp_reason(code: u32) -> &'static str {
    match code {
        2 => "no such file or directory",
        3 => "permission denied",
        4 => "failure",
        8 => "operation not supported",
        _ => "SFTP error",
    }
}

fn main() {
    let cli = Cli::parse();
    let manager = DeviceManager::default();
    let Some(device) = manager.find_or_default(cli.device.as_ref()).unwrap() else {
        eprintln!("Device not found");
        exit(1);
    };
    let session = match device.new_session() {
        Ok(session) => session,
        Err(e) => {
            eprintln!("Failed to connect to {}: {e:?}", device.name);
            exit(1);
        }
    };
    let sftp = match session.sftp() {
        Ok(sftp) => sftp,
        Err(e) => {
            eprintln!("Failed to start SFTP session on {}: {e}", device.name);
            exit(1);
        }
    };
    for source in cli.source {
        let walker = WalkDir::new(&source).contents_first(false);
        let dest_base = Path::new(&cli.destination);
        let mut source_prefix: &Path = &source;
        if cli.destination.ends_with("/") {
            if let Some(parent) = source_prefix.parent() {
                source_prefix = parent;
            }
        }
        for entry in walker {
            match entry {
                Ok(entry) => {
                    let file_type = entry.file_type();
                    let dest_path =
                        dest_base.join(entry.path().strip_prefix(source_prefix).unwrap());
                    let dest_display = dest_path.to_slash_lossy();
                    if file_type.is_dir() {
                        println!("{} => {}", entry.path().to_string_lossy(), dest_display);
                        // A directory that already exists reports an error we can
                        // safely ignore; a genuine failure surfaces when we try to
                        // write a file into it below.
                        sftp.create_dir(dest_display.as_ref(), 0o755).unwrap_or(());
                    } else if file_type.is_file() {
                        println!("{} => {}", entry.path().to_string_lossy(), dest_display);
                        let mut file = match sftp.open(
                            dest_display.as_ref(),
                            OpenFlags::WRITE_ONLY | OpenFlags::CREATE | OpenFlags::TRUNCATE,
                            0o644,
                        ) {
                            Ok(file) => file,
                            Err(e) => {
                                match sftp_status(&e) {
                                    Some(code) => {
                                        eprintln!(
                                            "Failed to write {dest_display}: {} (SFTP code {code})",
                                            sftp_reason(code)
                                        );
                                        if code == 3 {
                                            eprintln!(
                                                "  The destination may not be writable on this \
                                                 device; try a different path."
                                            );
                                        }
                                    }
                                    None => eprintln!("Failed to write {dest_display}: {e}"),
                                }
                                if !cli.ignore {
                                    exit(1);
                                }
                                continue;
                            }
                        };
                        let mut loc_file = match File::open(entry.path()) {
                            Ok(loc_file) => loc_file,
                            Err(e) => {
                                eprintln!("Failed to read {}: {e}", entry.path().to_string_lossy());
                                if !cli.ignore {
                                    exit(1);
                                }
                                continue;
                            }
                        };
                        if let Err(e) = std::io::copy(&mut loc_file, &mut file) {
                            eprintln!("Failed to write {dest_display}: {e}");
                            if !cli.ignore {
                                exit(1);
                            }
                        }
                    } else if file_type.is_symlink() {
                        eprintln!("Skipping symlink {}", entry.path().to_string_lossy());
                    }
                }
                Err(e) => {
                    eprintln!("Failed to push file: {e:?}");
                    if !cli.ignore {
                        exit(1);
                    }
                }
            }
        }
    }
}
