use std::fs::File;
use std::io::{Error as IoError, ErrorKind};
use std::path::Path;

use libssh_rs::Session;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Error as JsonError;

use common::luna::{Luna, LunaError};
use common::session::SessionError;
use common::transfer::FileTransfer;

pub(crate) trait InstallApp {
    fn install_app<P: AsRef<Path>>(&self, package: P) -> Result<String, InstallError>;
}

#[derive(Debug)]
pub enum InstallError {
    Response { error_code: i32, reason: String },
    Luna(LunaError),
    Io(IoError),
    Session(SessionError),
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

impl InstallApp for Session {
    fn install_app<P: AsRef<Path>>(&self, package: P) -> Result<String, InstallError> {
        let mut file = File::open(&package)?;
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
        let ipk_path = format!("/media/developer/temp/ares_install_{checksum}.ipk");
        self.put(&mut file, &ipk_path)?;

        let payload = InstallPayload {
            id: String::from("com.ares.defaultName"),
            ipk_url: ipk_path.clone(),
            subscribe: true,
        };
        let subscription = self.subscribe(
            "luna://com.webos.appInstallService/dev/install",
            payload,
            true,
        )?;
        let result = subscription
            .filter_map(|item| {
                return match item {
                    Ok(message) => match message.deserialize::<InstallResponse>() {
                        Ok(resp) => {
                            if let Some(details) = resp.details {
                                if let Some(state) = details.state {
                                    if Regex::new(r"(?i)FAILED").unwrap().is_match(&state) {
                                        return Some(Err(InstallError::Response {
                                            error_code: details.error_code.unwrap_or(0),
                                            reason: details
                                                .reason
                                                .unwrap_or(String::from("unknown error")),
                                        }));
                                    } else if Regex::new(r"(?i)^SUCCESS").unwrap().is_match(&state)
                                        || Regex::new(r"(?i)installed").unwrap().is_match(&state)
                                    {
                                        return Some(Ok(details
                                            .package_id
                                            .unwrap_or(String::from(""))));
                                    }
                                }
                            }
                            None
                        }
                        Err(e) => Some(Err(e.into())),
                    },
                    Err(e) => Some(Err(InstallError::Io(e))),
                };
            })
            .next();

        self.rm(&ipk_path)?;

        return result.unwrap();
    }
}

impl From<SessionError> for InstallError {
    fn from(value: SessionError) -> Self {
        Self::Session(value)
    }
}

impl From<LunaError> for InstallError {
    fn from(value: LunaError) -> Self {
        Self::Luna(value)
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
