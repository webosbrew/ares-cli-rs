use std::fs::File;
use std::io::{Error, Read};

use libssh_rs::{PublicKeyHashType, SshKey};

use crate::device_manager::io::ssh_dir;
use crate::device_manager::PrivateKey;

impl PrivateKey {
    pub fn content(&self) -> Result<String, Error> {
        return match self {
            PrivateKey::Path { name } => {
                let mut secret_file = File::open(ssh_dir()?.join(name))?;
                let mut secret = String::new();
                secret_file.read_to_string(&mut secret)?;
                Ok(secret)
            }
            PrivateKey::Data { data } => Ok(data.clone()),
        };
    }

    pub fn name(&self, passphrase: Option<String>) -> Result<String, Error> {
        return match self {
            PrivateKey::Path { name } => Ok(name.clone()),
            PrivateKey::Data { data } => Ok(String::from(
                &hex::encode(
                    SshKey::from_privkey_base64(data, passphrase.as_deref())?
                        .get_public_key_hash(PublicKeyHashType::Sha256)?,
                )[..10],
            )),
        };
    }
}
