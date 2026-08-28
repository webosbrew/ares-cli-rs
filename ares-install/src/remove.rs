use ares_connection_lib::luna::Luna;
use ares_connection_lib::session::DeviceSession;
use serde::Serialize;

use crate::install::{InstallError, REMOVED, map_installer_message};

pub(crate) trait RemoveApp {
    fn remove_app(&self, package_id: &str) -> Result<String, InstallError>;
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct RemovePayload {
    id: String,
    subscribe: bool,
}

impl RemoveApp for DeviceSession {
    fn remove_app(&self, package_id: &str) -> Result<String, InstallError> {
        let result = match self.subscribe(
            "luna://com.webos.appInstallService/dev/remove",
            RemovePayload {
                id: String::from(package_id),
                subscribe: true,
            },
            true,
        ) {
            Ok(subscription) => subscription
                .filter_map(|item| {
                    map_installer_message(item, &REMOVED, |progress| {
                        println!("{}", progress);
                    })
                })
                .next(),
            Err(e) => Some(Err(e.into())),
        };

        // A stream that ends having said neither "removed" nor a failure used
        // to panic here. It is the same silence an install can meet, and it
        // deserves the same answer.
        result.unwrap_or(Err(InstallError::NoVerdict))
    }
}
