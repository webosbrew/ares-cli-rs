use std::io::Error;

pub mod luna;
pub mod session;
mod setup;
pub mod transfer;

pub trait DeviceSetupManager {
    fn novacom_getkey(&self, address: &str, passphrase: &str) -> Result<String, Error>;

    fn localkey_verify(&self, name: &str, passphrase: &str) -> Result<(), Error>;
}
