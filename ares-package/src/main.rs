use std::fs::File;
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
    #[arg(short, long, value_name = "OUTPUT_DIR", help = "Use OUTPUT_DIR as the output directory")]
    outdir: Option<String>,
    #[arg(short = 'e', long, value_name = "PATTERN", help = "Exclude files, given as a PATTERN")]
    app_exclude: Vec<String>,
    #[arg(help = "App directory containing a valid appinfo.json file.")]
    app_dir: String,
    #[arg(help = "Directory containing a valid services.json file")]
    service_dir: Vec<String>,
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

#[derive(Debug, Deserialize)]
pub(crate) struct ServiceInfo {
    pub id: String,
    pub description: Option<String>,
}

fn main() {
    let cli = Cli::parse();
    let app_dir = PathBuf::from(&cli.app_dir);
    let outdir = cli.outdir.map(|outdir| PathBuf::from(outdir))
        .or_else(|| app_dir.parent().map(|p| p.to_path_buf()))
        .expect("Invalid output directory");
    let path = outdir.join("test.ipk");
    println!("Packaging {}...", path.to_string_lossy());

    let service_dirs: Vec<PathBuf> = cli.service_dir.iter().map(|s| PathBuf::from(s))
        .collect();
    let package_info = PackageInfo::from_input(app_dir, &service_dirs).unwrap();

    let ipk_file = File::create(path).unwrap();
    let mut builder = Builder::new(ipk_file);
    let debian_binary = b"2.0\n".to_vec();

    builder.append(&Header::new(b"debian-binary".to_vec(), debian_binary.len() as u64),
                   debian_binary.deref()).unwrap();
    let control = ControlInfo {
        package: format!("org.mariotaku.ihsplay"),
        version: format!("0.9.1"),
        architecture: format!("arm"),
    };
    builder.append_control(&control).unwrap();
    builder.append_data(&package_info).unwrap();
}
