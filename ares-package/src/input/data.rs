use std::collections::HashSet;
use std::fs::File;
use std::io::{Error, ErrorKind, Result};
use std::path::{Path, PathBuf};

use crate::{PackageInfo, ParseFrom};
use crate::input::app::AppInfo;
use crate::input::service::ServiceInfo;
use crate::input::validation::{Validation, ValidationInfo};

#[derive(Debug)]
pub struct DataInfo {
    pub package: PackageInfo,
    pub app: ComponentInfo<AppInfo>,
    pub services: Vec<ComponentInfo<ServiceInfo>>,
}

#[derive(Debug)]
pub struct ComponentInfo<T> {
    pub path: PathBuf,
    pub info: T,
    pub size: usize,
}

impl DataInfo {
    pub fn from_input<P1, P2>(app_dir: P1, service_dirs: &[P2]) -> Result<DataInfo>
    where
        P1: AsRef<Path>,
        P2: AsRef<Path>,
    {
        let app_dir = app_dir.as_ref();
        let app_info: AppInfo = AppInfo::parse_from(File::open(app_dir.join("appinfo.json"))?)?;
        let mut services: Vec<ComponentInfo<ServiceInfo>> = Vec::new();
        for service_dir in service_dirs {
            let service_dir = service_dir.as_ref();
            let service_info =
                ServiceInfo::parse_from(File::open(service_dir.join("services.json"))?)?;
            services.push(ComponentInfo {
                path: service_dir.to_path_buf(),
                info: service_info,
                size: fs_extra::dir::get_size(service_dir).unwrap() as usize,
            });
        }
        return Ok(DataInfo {
            package: PackageInfo {
                id: app_info.id.clone(),
                version: app_info.version.clone(),
                app: app_info.version.clone(),
                services: services.iter().map(|info| info.info.id.clone()).collect(),
            },
            app: ComponentInfo {
                path: app_dir.to_path_buf(),
                info: app_info,
                size: fs_extra::dir::get_size(app_dir).unwrap() as usize,
            },
            services,
        });
    }
}

impl Validation for DataInfo {
    fn validate(&self) -> Result<ValidationInfo> {
        let app_validation = self.app.validate()?;
        let mut archs = HashSet::<String>::new();
        let mut size_sum = 0;
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
        return Ok(ValidationInfo {
            arch: archs.iter().next().cloned(),
            size: size_sum,
        });
    }
}
