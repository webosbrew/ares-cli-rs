use std::collections::HashSet;
use std::ffi::OsString;
use std::fs::File;
use std::io::{Error, ErrorKind};
use std::path::{Component, Path, PathBuf};
use std::process::exit;

use ares_connection_lib::session::NewSession;
use ares_connection_lib::transfer::{PathKind, Transfer, TransferError};
use ares_device_lib::DeviceManager;
use ares_device_lib::cli::unwrap_or_exit;
use clap::Parser;
use path_slash::PathExt;
use walkdir::WalkDir;

#[derive(Parser, Debug)]
#[command(about)]
struct Cli {
    #[arg(
        short,
        long,
        value_name = "DEVICE",
        env = "ARES_DEVICE",
        help = "Specify DEVICE to use"
    )]
    device: Option<String>,
    #[arg(short, long, help = "Hide the detailed copy messages")]
    ignore: bool,
    #[arg(
        short,
        long,
        help = "Continue on errors instead of stopping at the first failure"
    )]
    keep_going: bool,
    #[arg(
        value_name = "SOURCE",
        help = "Path in the host machine, where files exist.",
        required = true
    )]
    source: Vec<PathBuf>,
    #[arg(
        value_name = "DESTINATION",
        help = "Path in the DEVICE, where multiple files can be copied",
        required = true
    )]
    destination: String,
}

fn main() {
    let cli = Cli::parse();
    let manager = DeviceManager::default();
    let Some(device) = unwrap_or_exit(manager.find_or_default(cli.device.as_ref()), "find device")
    else {
        eprintln!("Device not found");
        exit(1);
    };
    let session = unwrap_or_exit(device.new_session(), &format!("connect to {}", device.name));
    // Open the transport once. It is SFTP unless the device is set to stream,
    // and a copy of many files would otherwise pay a handshake per file.
    let transfer = Transfer::open(&session);

    let dest_kind = match transfer.stat(&cli.destination) {
        Ok(kind) => kind,
        Err(e) => {
            eprintln!("Failed to read {}: {e}", cli.destination);
            exit(1);
        }
    };
    let single = cli.source.len() == 1;
    if dest_kind == PathKind::File && !single {
        eprintln!(
            "Failed to push: {} is a file, so it can hold only one SOURCE",
            cli.destination
        );
        exit(1);
    }

    let mut push = Push {
        transfer: &transfer,
        quiet: cli.ignore,
        keep_going: cli.keep_going,
        made_dirs: HashSet::new(),
        failed: false,
    };
    for source in &cli.source {
        if let Err(e) = push.source(source, &cli.destination, dest_kind, single) {
            eprintln!("Failed to push {}: {e}", source.display());
            if !push.keep_going {
                exit(1);
            }
            push.failed = true;
        }
    }
    if push.failed {
        exit(1);
    }
}

struct Push<'a> {
    transfer: &'a Transfer<'a>,
    quiet: bool,
    keep_going: bool,
    /// Device paths we already made, so a run does not stat the same directory
    /// once per file.
    made_dirs: HashSet<String>,
    /// Set when --keep-going swallowed a failure, so the exit code still says so.
    failed: bool,
}

impl Push<'_> {
    /// Copy one SOURCE. `dest` is DESTINATION as typed, `kind` what it already
    /// is on the device, and `single` whether it is the only SOURCE.
    fn source(
        &mut self,
        source: &Path,
        dest: &str,
        kind: PathKind,
        single: bool,
    ) -> Result<(), Error> {
        // Follow a symlinked SOURCE, the same way walkdir follows the root.
        let source_is_dir = std::fs::metadata(source)?.is_dir();
        let root = resolve_dest(dest, kind, source, source_is_dir, single)?;

        for entry in WalkDir::new(source) {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    self.item_failed(&source.to_string_lossy(), Error::from(e))?;
                    continue;
                }
            };
            let Ok(relative) = entry.path().strip_prefix(source) else {
                continue;
            };
            let target = if relative.as_os_str().is_empty() {
                root.clone()
            } else {
                root.join(relative)
            };
            let target = target.to_slash_lossy().to_string();

            let file_type = entry.file_type();
            let result = if file_type.is_dir() {
                self.report(entry.path(), &target);
                self.mkdir(&target)
            } else if file_type.is_symlink() {
                // ares-cli reads through a symlink to a file and copies the
                // content. A symlink to a directory makes it fail, so skip that.
                match std::fs::metadata(entry.path()) {
                    Ok(metadata) if metadata.is_dir() => {
                        eprintln!(
                            "Skipping {}: it is a symlink to a directory",
                            entry.path().display()
                        );
                        Ok(())
                    }
                    Ok(_) => self.put_file(entry.path(), &target),
                    Err(e) => Err(e),
                }
            } else {
                self.put_file(entry.path(), &target)
            };
            if let Err(e) = result {
                self.item_failed(&entry.path().to_string_lossy(), e)?;
            }
        }
        Ok(())
    }

    fn put_file(&mut self, local: &Path, target: &str) -> Result<(), Error> {
        if let Some(parent) = parent_of(target) {
            self.mkdir(parent)?;
        }
        self.report(local, target);
        let mut source = File::open(local)?;
        self.transfer
            .put(&mut source, target, |_| {})
            .map_err(|e| transfer_error(target, &e))
    }

    /// Make `path` on the device, unless this run made it already.
    fn mkdir(&mut self, path: &str) -> Result<(), Error> {
        if self.made_dirs.contains(path) {
            return Ok(());
        }
        self.transfer
            .mkdir(path, 0o755)
            .map_err(|e| transfer_error(path, &e))?;
        self.made_dirs.insert(path.to_string());
        Ok(())
    }

    fn report(&self, local: &Path, target: &str) {
        if !self.quiet {
            println!("{} => {target}", local.display());
        }
    }

    /// Handle a failure on one item. Returns the error to stop the whole copy,
    /// or Ok to go on when --keep-going is set.
    fn item_failed(&mut self, what: &str, e: Error) -> Result<(), Error> {
        if !self.keep_going {
            return Err(e);
        }
        eprintln!("Skipping {what}: {e}");
        self.failed = true;
        Ok(())
    }
}

/// Where SOURCE itself lands on the device.
///
/// This follows ares-cli: a directory always keeps its own name under
/// DESTINATION, and a lone file keeps its name only when DESTINATION already is
/// a directory. A trailing "/" changes nothing.
fn resolve_dest(
    dest: &str,
    kind: PathKind,
    source: &Path,
    source_is_dir: bool,
    single: bool,
) -> Result<PathBuf, Error> {
    let dest_path = Path::new(dest);
    if source_is_dir {
        if kind == PathKind::File {
            return Err(Error::new(
                ErrorKind::AlreadyExists,
                format!("{dest} is a file, and SOURCE is a directory"),
            ));
        }
    } else if single && kind != PathKind::Dir {
        // One file onto a free path, or onto a file to overwrite.
        return Ok(dest_path.to_path_buf());
    }
    Ok(match source_name(source)? {
        Some(name) => dest_path.join(name),
        None => dest_path.to_path_buf(),
    })
}

/// Name that SOURCE takes on the device.
///
/// `Path::file_name` gives None for ".", ".." and "/". "." copies the contents,
/// the way `cp -r . dest` does, so it has no name of its own. The rest resolve
/// to a real directory name.
fn source_name(path: &Path) -> Result<Option<OsString>, Error> {
    if let Some(name) = path.file_name() {
        return Ok(Some(name.to_os_string()));
    }
    if path.components().all(|c| c == Component::CurDir) {
        return Ok(None);
    }
    let resolved = path.canonicalize()?;
    match resolved.file_name() {
        Some(name) => Ok(Some(name.to_os_string())),
        None => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("{} has no name to copy", path.display()),
        )),
    }
}

/// Parent of a device path, which always uses "/". Returns None when the path
/// has no parent to make.
fn parent_of(path: &str) -> Option<&str> {
    let head = path.trim_end_matches('/').rsplit_once('/')?.0;
    if head.is_empty() { None } else { Some(head) }
}

/// Name the path a transfer failed on, and say what to try when the device
/// turned it down.
fn transfer_error(path: &str, e: &TransferError) -> Error {
    if e.is_permission_denied() {
        return Error::new(
            ErrorKind::PermissionDenied,
            format!(
                "{path}: permission denied. The destination may not be writable on this device, \
                 so try a different path."
            ),
        );
    }
    Error::other(format!("{path}: {e}"))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use ares_connection_lib::transfer::PathKind;

    use super::{parent_of, resolve_dest};

    fn dest(source: &str, destination: &str, kind: PathKind, is_dir: bool, single: bool) -> String {
        resolve_dest(destination, kind, Path::new(source), is_dir, single)
            .expect("resolve_dest")
            .to_string_lossy()
            .replace('\\', "/")
    }

    #[test]
    fn a_lone_file_takes_the_destination_name() {
        for kind in [PathKind::Missing, PathKind::File] {
            assert_eq!(dest("a.txt", "/tmp/b.txt", kind, false, true), "/tmp/b.txt");
        }
    }

    #[test]
    fn a_lone_file_keeps_its_name_under_a_directory() {
        assert_eq!(
            dest("a.txt", "/tmp", PathKind::Dir, false, true),
            "/tmp/a.txt"
        );
    }

    #[test]
    fn many_files_go_into_the_destination() {
        for kind in [PathKind::Dir, PathKind::Missing] {
            assert_eq!(dest("a.txt", "/tmp", kind, false, false), "/tmp/a.txt");
        }
    }

    #[test]
    fn a_directory_always_keeps_its_own_name() {
        // The point of the ares-cli rule: "build" lands as /tmp/out/build, even
        // when /tmp/out already is a directory.
        for kind in [PathKind::Dir, PathKind::Missing] {
            assert_eq!(
                dest("build", "/tmp/out", kind, true, true),
                "/tmp/out/build"
            );
        }
    }

    #[test]
    fn a_trailing_slash_changes_nothing() {
        assert_eq!(
            dest("build", "/tmp/out/", PathKind::Dir, true, true),
            "/tmp/out/build"
        );
        assert_eq!(
            dest("build/", "/tmp/out", PathKind::Dir, true, true),
            "/tmp/out/build"
        );
        assert_eq!(
            dest("a.txt", "/tmp/", PathKind::Dir, false, true),
            "/tmp/a.txt"
        );
    }

    #[test]
    fn a_dot_source_copies_its_contents() {
        assert_eq!(dest(".", "/tmp/out", PathKind::Dir, true, true), "/tmp/out");
        assert_eq!(
            dest("./", "/tmp/out", PathKind::Missing, true, true),
            "/tmp/out"
        );
    }

    #[test]
    fn a_directory_onto_a_file_is_an_error() {
        assert!(
            resolve_dest("/tmp/a.txt", PathKind::File, Path::new("build"), true, true).is_err()
        );
    }

    #[test]
    fn parents_stop_at_the_root() {
        assert_eq!(parent_of("/media/developer/apps"), Some("/media/developer"));
        assert_eq!(
            parent_of("/media/developer/apps/"),
            Some("/media/developer")
        );
        assert_eq!(parent_of("/media"), None);
        assert_eq!(parent_of("/"), None);
        assert_eq!(parent_of("app.ipk"), None);
    }
}
