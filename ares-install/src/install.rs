use std::fmt::Write;
use std::fs::File;
use std::io::{Error as IoError, ErrorKind};
use std::path::Path;
use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Error as JsonError;

use ares_connection_lib::luna::{Luna, LunaError, Message};
use ares_connection_lib::session::DeviceSession;
use ares_connection_lib::transfer::{FileTransfer, TransferError};

pub(crate) trait InstallApp {
    fn install_app<P: AsRef<Path>>(&self, package: P) -> Result<(), InstallError>;
}

#[derive(Debug)]
pub enum InstallError {
    Response { error_code: i32, reason: String },
    Luna(LunaError),
    Transfer(TransferError),
    Io(IoError),
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct InstallPayload {
    id: String,
    ipk_url: String,
    subscribe: bool,
}

#[derive(Deserialize, Debug)]
struct InstallResponse {
    details: Option<InstallResponseDetails>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct InstallResponseDetails {
    package_id: Option<String>,
    state: Option<String>,
    error_code: Option<i32>,
    reason: Option<String>,
}

impl InstallApp for DeviceSession {
    fn install_app<P: AsRef<Path>>(&self, package: P) -> Result<(), InstallError> {
        let mut file = File::open(&package)?;
        let file_size = file.metadata()?.len();
        let checksum = sha256::try_digest(package.as_ref()).map_err(|e| {
            IoError::new(
                ErrorKind::Other,
                format!(
                    "Failed to generate checksum for {}: {:?}",
                    package.as_ref().to_string_lossy(),
                    e
                ),
            )
        })?;
        let ipk_path = format!("/media/developer/temp/ares_install_{}.ipk", &checksum[..10]);

        let package_display_name = package
            .as_ref()
            .file_name()
            .map(|s| s.to_string_lossy())
            .unwrap_or_else(|| package.as_ref().to_string_lossy());

        self.mkdir(&mut Path::new("/media/developer/temp"), 0o777)?;

        let pb = ProgressBar::new(file_size);
        pb.suspend(|| {
            println!(
                "Uploading {} to {}...",
                package_display_name, self.device.name
            )
        });
        pb.enable_steady_tick(Duration::from_millis(50));
        pb.set_prefix("Uploading");
        pb.set_style(ProgressStyle::with_template("{prefix:10.bold.dim} {spinner} {percent:>3}% [{wide_bar}] {bytes}/{total_bytes}  {eta} ETA")
            .unwrap());

        self.put(&mut file, &ipk_path, |transferred| {
            pb.set_position(transferred as u64);
        })?;

        pb.suspend(|| {
            println!(
                "Installing {} on {}...",
                package_display_name, self.device.name
            )
        });
        pb.set_prefix("Installing");

        let spinner_style =
            ProgressStyle::with_template("{prefix:10.bold.dim} {spinner} {wide_msg}").unwrap();
        pb.set_style(spinner_style);

        let result = match self.subscribe(
            "luna://com.webos.appInstallService/dev/install",
            InstallPayload {
                id: String::from("com.ares.defaultName"),
                ipk_url: ipk_path.clone(),
                subscribe: true,
            },
            true,
        ) {
            Ok(subscription) => subscription
                .filter_map(|item| {
                    map_installer_message(
                        item,
                        &Regex::new(r"(?i)installed").unwrap(),
                        |progress| {
                            pb.set_message(
                                progress
                                    .strip_prefix("installing : ")
                                    .unwrap_or(&progress)
                                    .to_string(),
                            );
                        },
                    )
                })
                .next()
                .unwrap_or_else(|| Ok(String::new())),
            Err(e) => Err(e.into()),
        };

        if let Ok(package_id) = &result {
            pb.suspend(|| println!("Installed package {}!", package_id));
        }
        pb.suspend(|| println!("Deleting uploaded package..."));

        pb.set_prefix("Cleanup");
        pb.set_message("Deleting uploaded package");

        if let Err(e) = self.rm(&ipk_path) {
            pb.suspend(|| {
                eprintln!("Failed to delete {}: {:?}", ipk_path, e);
            });
        }
        pb.finish_and_clear();

        result?;
        Ok(())
    }
}

pub(crate) fn map_installer_message<F: Fn(String)>(
    item: std::io::Result<Message>,
    expected: &Regex,
    progress: F,
) -> Option<Result<String, InstallError>> {
    match item {
        Ok(message) => match message.deserialize::<InstallResponse>() {
            Ok(resp) => {
                if let Some(details) = resp.details {
                    if let Some(state) = details.state {
                        if Regex::new(r"(?i)FAILED").unwrap().is_match(&state) {
                            return Some(Err(InstallError::Response {
                                error_code: details.error_code.unwrap_or(0),
                                reason: details.reason.unwrap_or(String::from("unknown error")),
                            }));
                        } else if Regex::new(r"(?i)^SUCCESS").unwrap().is_match(&state)
                            || expected.is_match(&state)
                        {
                            return Some(Ok(details.package_id.unwrap_or(String::from(""))));
                        } else {
                            progress(state);
                        }
                    }
                }
                None
            }
            Err(e) => Some(Err(e.into())),
        },
        Err(e) => Some(Err(InstallError::Io(e))),
    }
}

impl From<LunaError> for InstallError {
    fn from(value: LunaError) -> Self {
        Self::Luna(value)
    }
}

impl From<TransferError> for InstallError {
    fn from(value: TransferError) -> Self {
        Self::Transfer(value)
    }
}

impl From<IoError> for InstallError {
    fn from(value: IoError) -> Self {
        Self::Io(value)
    }
}

impl From<JsonError> for InstallError {
    fn from(value: JsonError) -> Self {
        Self::Io(IoError::new(
            ErrorKind::InvalidData,
            format!("Invalid JSON data: {value:?}"),
        ))
    }
}
