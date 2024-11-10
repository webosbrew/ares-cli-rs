use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::exit;

use clap::Parser;
use path_slash::PathBufExt;
use walkdir::WalkDir;

use ares_connection_lib::session::NewSession;
use ares_device_lib::DeviceManager;
use libssh_rs::OpenFlags;

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

fn main() {
    let cli = Cli::parse();
    let manager = DeviceManager::default();
    let Some(device) = manager.find_or_default(cli.device).unwrap() else {
        eprintln!("Device not found");
        exit(1);
    };
    let session = device.new_session().unwrap();
    let sftp = session.sftp().unwrap();
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
                    if file_type.is_dir() {
                        println!(
                            "{} => {}",
                            entry.path().to_string_lossy(),
                            dest_path.to_slash_lossy()
                        );
                        sftp.create_dir(dest_path.to_slash_lossy().as_ref(), 0o755)
                            .unwrap_or(());
                    } else if file_type.is_file() {
                        println!(
                            "{} => {}",
                            entry.path().to_string_lossy(),
                            dest_path.to_slash_lossy()
                        );
                        let mut file = match sftp.open(
                            dest_path.to_slash_lossy().as_ref(),
                            OpenFlags::WRITE_ONLY | OpenFlags::CREATE | OpenFlags::TRUNCATE,
                            0o644,
                        ) {
                            Ok(file) => file,
                            Err(e) => {
                                eprintln!("Failed to open file: {e:?}");
                                continue;
                            }
                        };
                        let mut loc_file = File::open(entry.path()).unwrap();
                        std::io::copy(&mut loc_file, &mut file).unwrap();
                    } else if file_type.is_symlink() {
                        eprintln!("Skipping symlink {}", entry.path().to_string_lossy());
                    }
                }
                Err(e) => eprintln!("Failed to push file: {e:?}"),
            }
        }
    }
}
