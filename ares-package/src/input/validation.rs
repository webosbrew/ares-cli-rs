use std::fs::File;
use std::io::{Error, ErrorKind, Result};
use std::path::Path;

use elf::endian::AnyEndian;
use elf::to_str::e_machine_to_string;
use elf::ElfStream;
use fs_extra::error::ErrorKind as ExtraErrorKind;

use crate::input::app::AppInfo;
use crate::input::data::ComponentInfo;
use crate::input::service::ServiceInfo;

pub struct ValidationInfo {
    pub arch: Option<String>,
    pub size: usize,
}

pub trait Validation {
    fn validate(&self) -> Result<ValidationInfo>;
}

impl Validation for ComponentInfo<AppInfo> {
    fn validate(&self) -> Result<ValidationInfo> {
        let size = fs_extra::dir::get_size(&self.path).map_err(|e| to_io_error(e))? as usize;
        let mut arch: Option<String> = None;
        if self.info.r#type == "native" {
            arch = infer_arch(self.path.join(&self.info.main))?;
        }
        return Ok(ValidationInfo { arch, size });
    }
}

impl Validation for ComponentInfo<ServiceInfo> {
    fn validate(&self) -> Result<ValidationInfo> {
        let size = fs_extra::dir::get_size(&self.path).map_err(|e| to_io_error(e))? as usize;
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

fn to_io_error(e: fs_extra::error::Error) -> Error {
    match e.kind {
        ExtraErrorKind::NotFound => Error::new(ErrorKind::NotFound, e.to_string()),
        ExtraErrorKind::PermissionDenied => Error::new(ErrorKind::PermissionDenied, e.to_string()),
        ExtraErrorKind::AlreadyExists => Error::new(ErrorKind::AlreadyExists, e.to_string()),
        ExtraErrorKind::Interrupted => Error::new(ErrorKind::Interrupted, e.to_string()),
        ExtraErrorKind::InvalidFolder => Error::new(ErrorKind::InvalidInput, e.to_string()),
        ExtraErrorKind::InvalidFile => Error::new(ErrorKind::InvalidInput, e.to_string()),
        ExtraErrorKind::InvalidFileName => Error::new(ErrorKind::InvalidInput, e.to_string()),
        ExtraErrorKind::InvalidPath => Error::new(ErrorKind::InvalidInput, e.to_string()),
        ExtraErrorKind::Io(e) => e,
        _ => Error::new(ErrorKind::Other, e.to_string()),
    }
}
