use std::fs::File;
use std::io::{Error, Read};

use crate::io::ssh_dir;
use crate::PrivateKey;

impl PrivateKey {
    pub fn content(&self) -> Result<String, Error> {
        return match self {
            PrivateKey::Name { name } => {
                let mut secret_file = File::open(ssh_dir()?.join(name))?;
                let mut secret = String::new();
                secret_file.read_to_string(&mut secret)?;
                Ok(secret)
            }
            PrivateKey::Path { path } => {
                let mut secret_file = File::open(path)?;
                let mut secret = String::new();
                secret_file.read_to_string(&mut secret)?;
                Ok(secret)
            }
        };
    }
}
