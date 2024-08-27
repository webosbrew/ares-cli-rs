use std::collections::HashSet;
use std::fs::File;
use std::io::{Error, ErrorKind, Result};
use std::path::{Path, PathBuf};

use regex::Regex;

use crate::input::app::AppInfo;
use crate::input::service::ServiceInfo;
use crate::input::validation::{PackageArch, Validation, ValidationInfo};
use crate::{PackageInfo, ParseFrom};

#[derive(Debug)]
pub struct DataInfo {
    pub package: PackageInfo,
    pub package_data: Vec<u8>,
    pub app: ComponentInfo<AppInfo>,
    pub services: Vec<ComponentInfo<ServiceInfo>>,
    pub excludes: Option<Regex>,
}

#[derive(Debug)]
pub struct ComponentInfo<T> {
    pub path: PathBuf,
    pub info: T,
    pub excludes: Option<Regex>,
}

impl DataInfo {
    pub fn from_input<P1, P2, E>(
        app_dir: P1,
        service_dirs: &[P2],
        excludes: &[E],
    ) -> Result<DataInfo>
    where
        P1: AsRef<Path>,
        P2: AsRef<Path>,
        E: AsRef<str>,
    {
        let app_dir = app_dir.as_ref();
        let app_info: AppInfo = AppInfo::parse_from(File::open(app_dir.join("appinfo.json"))?)?;
        let mut services: Vec<ComponentInfo<ServiceInfo>> = Vec::new();
        let mut exclude_queries = Vec::<String>::new();
        for pattern in excludes {
            let mut pattern = String::from(pattern.as_ref());
            if pattern.starts_with(".") {
                pattern = pattern.replacen(".", "^\\.", 1);
            } else if pattern.starts_with("*") {
                pattern = pattern.replacen("*", "", 1);
            }
            pattern.push('$');
            exclude_queries.push(pattern);
        }
        let mut excludes: Option<Regex> = None;
        if !exclude_queries.is_empty() {
            excludes = Regex::new(&format!("(?i){}", exclude_queries.join("|"))).ok();
        }
        for service_dir in service_dirs {
            let service_dir = service_dir.as_ref();
            let service_info =
                ServiceInfo::parse_from(File::open(service_dir.join("services.json"))?)?;
            services.push(ComponentInfo {
                path: service_dir.to_path_buf(),
                info: service_info,
                excludes: excludes.clone(),
            });
        }
        let package_info = PackageInfo {
            id: app_info.id.clone(),
            version: app_info.version.clone(),
            app: app_info.id.clone(),
            services: services.iter().map(|info| info.info.id.clone()).collect(),
        };
        let mut package_info_data = serde_json::to_vec_pretty(&package_info)?;
        package_info_data.push(b'\n');
        Ok(DataInfo {
            package: package_info,
            package_data: package_info_data,
            app: ComponentInfo {
                path: app_dir.to_path_buf(),
                info: app_info,
                excludes: excludes.clone(),
            },
            services,
            excludes,
        })
    }
}

impl Validation for DataInfo {
    fn validate(&self) -> Result<ValidationInfo> {
        let app_validation = self.app.validate()?;
        let mut archs = HashSet::<PackageArch>::new();
        let mut size_sum = self.package_data.len() as u64;
        if let Some(arch) = &app_validation.arch {
            archs.insert(arch.clone());
        }
        size_sum += app_validation.size;

        for info in &self.services {
            let service_validation = info.validate()?;
            if let Some(arch) = &service_validation.arch {
                archs.insert(arch.clone());
            }
            size_sum += service_validation.size;
        }

        if archs.len() > 1 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Mixed architecture is not allowed",
            ));
        }
        Ok(ValidationInfo {
            arch: archs.iter().next().cloned(),
            size: size_sum,
        })
    }
}
