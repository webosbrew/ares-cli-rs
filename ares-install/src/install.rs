use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::{Error as IoError, ErrorKind};
use std::path::Path;
use std::sync::LazyLock;
use std::time::Duration;

use ares_connection_lib::luna::{Luna, LunaError, Message};
use ares_connection_lib::session::DeviceSession;
use ares_connection_lib::transfer::{Transfer, TransferError};
use indicatif::{ProgressBar, ProgressStyle};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Error as JsonError;

pub(crate) trait InstallApp {
    fn install_app<P: AsRef<Path>>(&self, package: P) -> Result<(), InstallError>;
}

#[derive(Debug)]
pub enum InstallError {
    Response {
        error_code: i32,
        reason: String,
    },
    /// The install stream ended without ever saying how it went.
    NoVerdict,
    /// The connection went down with the install already under way, so what
    /// the device did with the package is not known.
    Interrupted {
        device: String,
    },
    ChecksumMismatch {
        expected: String,
        actual: String,
    },
    Luna(LunaError),
    Transfer(TransferError),
    Io(IoError),
}

impl Display for InstallError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::Response { error_code, reason } => {
                write!(f, "{reason} (error {error_code})")
            }
            InstallError::NoVerdict => write!(
                f,
                "the device stopped reporting before it said whether the package installed"
            ),
            InstallError::Interrupted { device } => write!(
                f,
                "lost the connection to {device} while the install was running. The device may \
                 have installed the package anyway - `ares-install -d {device} --listfull` \
                 prints the version that is on it. `--list` alone only names the app, which \
                 says nothing about which build landed"
            ),
            InstallError::ChecksumMismatch { expected, actual } => write!(
                f,
                "uploaded package is corrupted: expected sha256 {expected}, device has {actual}"
            ),
            InstallError::Luna(e) => write!(f, "{e}"),
            InstallError::Transfer(e) => write!(f, "{e}"),
            InstallError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for InstallError {}

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
                    "Failed to generate checksum for {}: {e}",
                    package.as_ref().to_string_lossy()
                ),
            )
        })?;
        let ipk_path = format!("/media/developer/temp/ares_install_{}.ipk", &checksum[..10]);

        let package_display_name = package
            .as_ref()
            .file_name()
            .map(|s| s.to_string_lossy())
            .unwrap_or_else(|| package.as_ref().to_string_lossy());

        // One transport for the whole install. Each `Transfer::open` asks the
        // device for the sftp subsystem, which execs an sftp-server there, so
        // opening one per step paid for four where one does - and the one
        // sha256sum opened was never used at all, since that runs over an exec
        // channel. ares-push and ares-pull already work this way.
        let transfer = Transfer::open(self);

        transfer.mkdir(Path::new("/media/developer/temp"), 0o777)?;

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

        transfer.put(&mut file, &ipk_path, |transferred| {
            pb.set_position(transferred as u64);
        })?;

        let spinner_style =
            ProgressStyle::with_template("{prefix:10.bold.dim} {spinner} {wide_msg}").unwrap();
        pb.set_style(spinner_style);

        pb.set_prefix("Verifying");
        pb.set_message("Checking uploaded package");
        let verified = verify_upload(&transfer, &ipk_path, &checksum);
        if let Err(e) = &verified {
            pb.suspend(|| eprintln!("Upload of {package_display_name} is broken: {e}"));
        }

        let result = verified.and_then(|_| {
            pb.suspend(|| {
                println!(
                    "Installing {} on {}...",
                    package_display_name, self.device.name
                )
            });
            pb.set_prefix("Installing");
            pb.set_message("");

            match self.subscribe(
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
                        map_installer_message(item, &INSTALLED, |progress| {
                            pb.set_message(
                                progress
                                    .strip_prefix("installing : ")
                                    .unwrap_or(&progress)
                                    .to_string(),
                            );
                        })
                    })
                    .next()
                    // Reaching the end of the stream having seen neither
                    // "installed" nor a failure is not a success. Reporting one
                    // claims a package is on the device that may well not be.
                    .unwrap_or(Err(InstallError::NoVerdict)),
                Err(e) => Err(e.into()),
            }
        });

        // The install request is sent before the device answers it, so a
        // connection that dies here says nothing about what the device did -
        // and on some sets the install goes through regardless. Calling that a
        // failed install is a guess, and the wrong one often enough to matter.
        let result = result.map_err(|e| match e {
            InstallError::Luna(LunaError::Session(_)) | InstallError::Io(_)
                if !self.is_connected() =>
            {
                InstallError::Interrupted {
                    device: self.device.name.clone(),
                }
            }
            other => other,
        });

        if let Ok(package_id) = &result {
            pb.suspend(|| println!("Installed package {}!", package_id));
        }

        pb.set_prefix("Cleanup");
        pb.set_message("Deleting uploaded package");

        // Cleaning up over a connection that is already gone only produces a
        // second, confusing error - and it lands before the one that explains
        // the run, because this happens on the way out. Say what was left
        // behind instead, and let the real error through.
        if self.is_connected() {
            pb.suspend(|| println!("Deleting uploaded package..."));
            if let Err(e) = transfer.rm(&ipk_path) {
                pb.suspend(|| eprintln!("Failed to delete {ipk_path}: {e}"));
            }
        } else {
            pb.suspend(|| {
                eprintln!(
                    "Lost the connection to {}, so {ipk_path} is still on the device.",
                    self.device.name
                );
            });
        }
        pb.finish_and_clear();

        result?;
        Ok(())
    }
}

/// Compare the uploaded package against the local file. Devices without `sha256sum` skip the check.
fn verify_upload(transfer: &Transfer, ipk_path: &str, expected: &str) -> Result<(), InstallError> {
    let Some(actual) = transfer.sha256sum(ipk_path)? else {
        return Ok(());
    };
    if actual != expected {
        return Err(InstallError::ChecksumMismatch {
            expected: expected.to_string(),
            actual,
        });
    }
    Ok(())
}

/// The device reports progress a line at a time, so these are matched once per
/// line. Building them per line meant compiling three regexes per message, and
/// three `unwrap`s that could panic on a hot path.
static FAILED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)FAILED").unwrap());
static SUCCEEDED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^SUCCESS").unwrap());
pub(crate) static INSTALLED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)installed").unwrap());
pub(crate) static REMOVED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)removed").unwrap());

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
                        if FAILED.is_match(&state) {
                            return Some(Err(InstallError::Response {
                                error_code: details.error_code.unwrap_or(0),
                                reason: details.reason.unwrap_or(String::from("unknown error")),
                            }));
                        } else if SUCCEEDED.is_match(&state) || expected.is_match(&state) {
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
