use std::fs::File;
use std::io;
use std::io::{BufReader, Error, ErrorKind};
use std::path::Path;
use crate::{AppInfo, PackageInfo, ServiceInfo};

impl PackageInfo {
    pub fn from_input<P1, P2>(app_dir: P1, service_dirs: &[P2]) -> io::Result<PackageInfo>
        where P1: AsRef<Path>, P2: AsRef<Path> {
        let reader = BufReader::new(File::open(app_dir.as_ref().join("appinfo.json"))?);
        let app_info: AppInfo = serde_json::from_reader(reader)
            .map_err(|e| Error::new(ErrorKind::InvalidData, format!("Invalid appinfo.json: {e:?}")))?;
        let mut services: Vec<ServiceInfo> = Vec::new();
        for service_dir in service_dirs {
            let reader = BufReader::new(File::open(service_dir.as_ref().join("services.json"))?);
            let service: ServiceInfo = serde_json::from_reader(reader)
                .map_err(|e| Error::new(ErrorKind::InvalidData, format!("Invalid appinfo.json: {e:?}")))?;
            services.push(service);
        }
        return Ok(PackageInfo {
            id: app_info.id.clone(),
            app: app_info.id.clone(),
            services: services.iter().map(|s| s.id.clone()).collect(),
            version: app_info.version.clone(),
        });
    }
}