use std::io::Error;

mod setup;
pub mod session;
pub mod luna;
pub mod transfer;

pub trait DeviceSetupManager {
    fn novacom_getkey(&self, address: &str, passphrase: &str) -> Result<String, Error>;

    fn localkey_verify(&self, name: &str, passphrase: &str) -> Result<(), Error>;
}
