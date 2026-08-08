use std::fs::File;
use std::io::{Error, Read};
use std::path::Path;

use crate::PrivateKey;
use crate::io::ssh_dir;

impl PrivateKey {
    /// Read the key, resolving [`PrivateKey::Name`] against the user's `~/.ssh`.
    pub fn content(&self) -> Result<String, Error> {
        self.content_in(None)
    }

    /// Read the key, resolving [`PrivateKey::Name`] against `ssh_dir`.
    /// `None` means the user's `~/.ssh`.
    pub fn content_in(&self, ssh_dir_override: Option<&Path>) -> Result<String, Error> {
        match self {
            PrivateKey::Name { name } => {
                let dir = match ssh_dir_override {
                    Some(dir) => dir.to_path_buf(),
                    None => ssh_dir()?,
                };
                read_file(dir.join(name))
            }
            PrivateKey::Path { path } => read_file(path),
            PrivateKey::Data { data } => Ok(data.clone()),
        }
    }
}

fn read_file<P: AsRef<Path>>(path: P) -> Result<String, Error> {
    let mut secret_file = File::open(path)?;
    let mut secret = String::new();
    secret_file.read_to_string(&mut secret)?;
    Ok(secret)
}

#[cfg(test)]
mod tests {
    use std::fs::{create_dir_all, remove_dir_all, write};
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    /// A directory of this test's own, so tests can run side by side.
    fn temp_dir(label: &str) -> std::path::PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ares-privkey-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_name_is_read_from_the_given_directory() {
        let dir = temp_dir("name");
        write(dir.join("webos_tv"), "KEY BODY").unwrap();

        let key = PrivateKey::Name {
            name: String::from("webos_tv"),
        };
        assert_eq!(key.content_in(Some(&dir)).unwrap(), "KEY BODY");

        remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_path_ignores_the_ssh_directory() {
        let dir = temp_dir("path");
        let elsewhere = dir.join("id_rsa");
        write(&elsewhere, "PATH BODY").unwrap();

        let key = PrivateKey::Path {
            path: elsewhere.to_string_lossy().to_string(),
        };
        // The directory passed here is wrong on purpose: a full path wins.
        assert_eq!(
            key.content_in(Some(Path::new("/nonexistent"))).unwrap(),
            "PATH BODY"
        );

        remove_dir_all(&dir).ok();
    }

    #[test]
    fn inline_data_needs_no_file() {
        let key = PrivateKey::Data {
            data: String::from("INLINE BODY"),
        };
        assert_eq!(key.content_in(None).unwrap(), "INLINE BODY");
    }

    #[test]
    fn a_missing_key_file_reports_not_found() {
        let dir = temp_dir("missing");
        let key = PrivateKey::Name {
            name: String::from("absent"),
        };
        let error = key.content_in(Some(&dir)).expect_err("no such file");
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);

        remove_dir_all(&dir).ok();
    }
}
