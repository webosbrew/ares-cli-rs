use std::fs::File;
use std::io::{Error, ErrorKind, Read};
use std::ops::Deref;
use std::path::PathBuf;

use ar::{Builder, Header};
use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::control::{AppendControl, ControlInfo};
use crate::data::AppendData;

mod control;
mod data;
mod input;

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
    #[arg(help = "App directory containing a valid appinfo.json file.")]
    app_dir: PathBuf,
    #[arg(help = "Directory containing a valid services.json file")]
    service_dir: Vec<PathBuf>,
}

#[derive(Debug, Serialize)]
struct PackageInfo {
    app: String,
    id: String,
    services: Vec<String>,
    version: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AppInfo {
    pub id: String,
    pub version: String,
    pub r#type: String,
    pub title: String,
    pub vendor: Option<String>,
}

impl AppInfo {
    fn read_from<R: Read>(reader: R) -> std::io::Result<AppInfo> {
        return serde_json::from_reader(reader).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Invalid appinfo.json: {e:?}"),
            )
        });
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct ServiceInfo {
    pub id: String,
    pub description: Option<String>,
}

impl ServiceInfo {
    fn read_from<R: Read>(reader: R) -> std::io::Result<ServiceInfo> {
        return serde_json::from_reader(reader).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Invalid services.json: {e:?}"),
            )
        });
    }
}

fn main() {
    let cli = Cli::parse();
    let app_dir = cli.app_dir;
    let outdir = cli
        .outdir
        .or_else(|| app_dir.parent().map(|p| p.to_path_buf()))
        .expect("Invalid output directory");
    let path = outdir.join("test.ipk");
    println!("Packaging {}...", path.to_string_lossy());

    let package_info = PackageInfo::from_input(&app_dir, &cli.service_dir).unwrap();

    let ipk_file = File::create(path).unwrap();
    let mut ar = Builder::new(ipk_file);
    let debian_binary = b"2.0\n".to_vec();

    ar.append(
        &Header::new(b"debian-binary".to_vec(), debian_binary.len() as u64),
        debian_binary.deref(),
    )
    .unwrap();
    let control = ControlInfo {
        package: package_info.id.clone(),
        version: package_info.version.clone(),
        architecture: format!("arm"),
    };
    ar.append_control(&control).unwrap();
    ar.append_data(&package_info, &app_dir, &cli.service_dir)
        .unwrap();
}
