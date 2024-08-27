use std::fmt::Display;
use std::fs::File;
use std::io::{Error, ErrorKind, Result};
use std::path::Path;
use std::str::FromStr;

use elf::endian::AnyEndian;
use elf::to_str::e_machine_to_string;
use elf::ElfStream;

use crate::input::app::AppInfo;
use crate::input::data::ComponentInfo;
use crate::input::dir_size;
use crate::input::service::ServiceInfo;

#[derive(Eq, Hash, PartialEq, Debug, Clone)]
pub enum PackageArch {
    ARM,
    X86(String),
    ALL,
}

pub struct ValidationInfo {
    pub arch: Option<PackageArch>,
    pub size: u64,
}

pub trait Validation {
    fn validate(&self) -> Result<ValidationInfo>;
}

impl Validation for ComponentInfo<AppInfo> {
    fn validate(&self) -> Result<ValidationInfo> {
        let size = dir_size(&self.path, self.excludes.as_ref())?;
        let mut arch: Option<PackageArch> = None;
        if self.info.r#type == "native" {
            arch = infer_arch(self.path.join(&self.info.main))?;
        }
        Ok(ValidationInfo { arch, size })
    }
}

impl Validation for ComponentInfo<ServiceInfo> {
    fn validate(&self) -> Result<ValidationInfo> {
        let size = dir_size(&self.path, self.excludes.as_ref())?;
        let mut arch: Option<PackageArch> = None;
        if let (Some(engine), Some(executable)) = (&self.info.engine, &self.info.executable) {
            if engine == "native" {
                arch = infer_arch(self.path.join(executable))?;
            }
        }
        Ok(ValidationInfo { arch, size })
    }
}

impl Display for PackageArch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            PackageArch::ARM => String::from("arm"),
            PackageArch::ALL => String::from("all"),
            PackageArch::X86(s) => s.clone(),
        };
        write!(f, "{}", str)
    }
}

impl FromStr for PackageArch {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "arm" => Ok(PackageArch::ARM),
            "all" => Ok(PackageArch::ALL),
            "i386" | "i486" | "i586" | "i686" | "x86" => Ok(PackageArch::X86(String::from(s))),
            _ => Err(format!("Invalid architecture {s}")),
        }
    }
}

fn infer_arch<P: AsRef<Path>>(path: P) -> Result<Option<PackageArch>> {
    let elf = ElfStream::<AnyEndian, _>::open_stream(File::open(path.as_ref())?)
        .map_err(|e| Error::new(ErrorKind::InvalidData, format!("Bad binary: {e:?}")))?;
    match elf.ehdr.e_machine {
        elf::abi::EM_ARM => Ok(Some(PackageArch::ARM)),
        elf::abi::EM_386 => Ok(Some(PackageArch::X86(String::from("x86")))),
        e => Err(Error::new(
            ErrorKind::InvalidData,
            format!("Unsupported binary machine type {}", e_machine_to_string(e)),
        )),
    }
}
