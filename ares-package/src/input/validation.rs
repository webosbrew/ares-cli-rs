use std::fs::File;
use std::io::{Error, ErrorKind, Result};
use std::path::Path;

use elf::endian::AnyEndian;
use elf::to_str::e_machine_to_string;
use elf::ElfStream;

use crate::input::app::AppInfo;
use crate::input::data::ComponentInfo;
use crate::input::dir_size;
use crate::input::service::ServiceInfo;

pub struct ValidationInfo {
    pub arch: Option<String>,
    pub size: u64,
}

pub trait Validation {
    fn validate(&self) -> Result<ValidationInfo>;
}

impl Validation for ComponentInfo<AppInfo> {
    fn validate(&self) -> Result<ValidationInfo> {
        let size = dir_size(&self.path, self.excludes.as_ref())?;
        let mut arch: Option<String> = None;
        if self.info.r#type == "native" {
            arch = infer_arch(self.path.join(&self.info.main))?;
        }
        return Ok(ValidationInfo { arch, size });
    }
}

impl Validation for ComponentInfo<ServiceInfo> {
    fn validate(&self) -> Result<ValidationInfo> {
        let size = dir_size(&self.path, self.excludes.as_ref())?;
        let mut arch: Option<String> = None;
        if let (Some(engine), Some(executable)) = (&self.info.engine, &self.info.executable) {
            if engine == "native" {
                arch = infer_arch(self.path.join(executable))?;
            }
        }
        return Ok(ValidationInfo { arch, size });
    }
}

fn infer_arch<P: AsRef<Path>>(path: P) -> Result<Option<String>> {
    let elf = ElfStream::<AnyEndian, _>::open_stream(File::open(path.as_ref())?)
        .map_err(|e| Error::new(ErrorKind::InvalidData, format!("Bad binary: {e:?}")))?;
    return match elf.ehdr.e_machine {
        elf::abi::EM_ARM => Ok(Some(String::from("arm"))),
        elf::abi::EM_386 => Ok(Some(String::from("x86"))),
        e => Err(Error::new(
            ErrorKind::InvalidData,
            format!("Unsupported binary machine type {}", e_machine_to_string(e)),
        )),
    };
}
