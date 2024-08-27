use std::fmt::Debug;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::time::SystemTime;

use ar::Builder;
use clap::Parser;
use serde::Serialize;

use crate::input::data::DataInfo;
use crate::input::validation::{PackageArch, Validation};
use crate::packaging::control::{AppendControl, ControlInfo};
use crate::packaging::data::AppendData;
use crate::packaging::header::AppendHeader;

mod input;
mod packaging;

#[derive(Parser, Debug)]
#[command(about)]
struct Cli {
    #[arg(
        short,
        long,
        value_name = "OUTPUT_DIR",
        help = "Use OUTPUT_DIR as the output directory"
    )]
    outdir: Option<PathBuf>,
    #[arg(
        short = 'e',
        long,
        value_name = "PATTERN",
        help = "Exclude files, given as a PATTERN"
    )]
    app_exclude: Vec<String>,
    #[arg(
        short = 'A',
        long,
        value_name = "ARCH",
        help = "Explicitly specify the architecture"
    )]
    force_arch: Option<PackageArch>,
    #[arg(help = "App directory containing a valid appinfo.json file.")]
    app_dir: PathBuf,
    #[arg(help = "Directory containing a valid services.json file")]
    service_dir: Vec<PathBuf>,
}

#[derive(Debug, Serialize)]
pub struct PackageInfo {
    id: String,
    version: String,
    app: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    services: Vec<String>,
}

pub trait ParseFrom: Sized {
    fn parse_from<R: Read>(reader: R) -> std::io::Result<Self>;
}

fn main() {
    let cli = Cli::parse();
    let app_dir = cli.app_dir;
    let outdir = cli
        .outdir
        .or_else(|| std::env::current_dir().ok())
        .expect("Invalid output directory");

    let data = DataInfo::from_input(&app_dir, &cli.service_dir, &cli.app_exclude).unwrap();
    let package_info = &data.package;
    let validation = data.validate().unwrap();
    let arch = cli
        .force_arch
        .or_else(|| validation.arch.clone())
        .unwrap_or_else(|| PackageArch::ALL);
    if let Some(validation_arch) = &validation.arch {
        if std::mem::discriminant(&arch) != std::mem::discriminant(validation_arch) {
            eprintln!(
                "Incompatible architecture: {} != {}",
                arch.to_string(),
                validation_arch.to_string()
            );
            return;
        }
    }

    let path = outdir.join(format!(
        "{}_{}_{}.ipk",
        package_info.id, package_info.version, arch.to_string()
    ));
    println!("Packaging {}...", path.to_string_lossy());
    let ipk_file = File::create(path).unwrap();
    let mut ar = Builder::new(ipk_file);

    let mtime = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    ar.append_header(mtime).unwrap();
    let control = ControlInfo {
        package: package_info.id.clone(),
        version: package_info.version.clone(),
        installed_size: validation.size,
        architecture: arch.to_string(),
    };
    ar.append_control(&control, mtime).unwrap();
    ar.append_data(&data, mtime).unwrap();
    println!("Done.");
}
