use std::fs::File;
use std::path::Path;

use libssh_rs::Session;
use serde::Serialize;

use common::luna::Luna;
use common::session::SessionError;
use common::transfer::FileTransfer;

pub(crate) trait InstallApp {
    fn install_app<P: AsRef<Path>>(&self, package: P) -> Result<(), SessionError>;
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct InstallPayload {
    id: String,
    ipk_url: String,
    subscribe: bool,
}

impl InstallApp for Session {
    fn install_app<P: AsRef<Path>>(&self, package: P) -> Result<(), SessionError> {
        let mut file = File::open(&package)?;
        let checksum = sha256::try_digest(package.as_ref()).unwrap();
        let ipk_path = format!("/media/developer/temp/ares_install_{checksum}.ipk");
        self.put(&mut file, &ipk_path)?;

        let payload = InstallPayload {
            id: String::from("com.ares.defaultName"),
            ipk_url: ipk_path.clone(),
            subscribe: true,
        };
        let subscription = self.subscribe("luna://com.webos.appInstallService/dev/install",
                                          payload, true).unwrap();
        for item in subscription {
            let message = item.unwrap();
            println!("{:?}", message);
        }
        return Ok(());
    }
}
