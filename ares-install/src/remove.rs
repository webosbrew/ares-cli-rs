use regex::Regex;
use serde::Serialize;

use ares_connection_lib::luna::Luna;
use ares_connection_lib::session::DeviceSession;

use crate::install::{map_installer_message, InstallError};

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
                    map_installer_message(item, &Regex::new(r"(?i)removed").unwrap(), |progress| {
                        println!("{}", progress);
                    })
                })
                .next(),
            Err(e) => Some(Err(e.into())),
        };

        return result.unwrap();
    }
}
