use std::fmt::{Display, Formatter};
use std::io::{Error as IoError, ErrorKind, Read, Write};
use std::path::Path;

use libssh_rs::{Error as SshError, FileType, OpenFlags, Session, Sftp};
use path_slash::PathExt;

use crate::session::SshConnection;

/// File transfer against a device, one call at a time.
///
/// Every call opens its own transport, which costs a round trip on SFTP. Use
/// [`Transfer`] instead to copy many files, because it opens the transport once
/// and keeps it.
pub trait FileTransfer {
    /// An SFTP session, or an error when this connection has to stream files
    /// over an exec channel.
    ///
    /// # Errors
    ///
    /// Returns [`libssh_rs::Error::RequestDenied`] when the device is set to
    /// stream, or the libssh error when the SFTP session does not start.
    fn maybe_sftp(&self) -> Result<Sftp, SshError>;

    /// Make `dir` and every missing parent, the way `mkdir -p` does.
    ///
    /// # Errors
    ///
    /// Returns an error when a path in the chain exists and is not a directory,
    /// or when the device turns the request down.
    fn mkdir<P: AsRef<Path>>(&self, dir: P, mode: u32) -> Result<(), TransferError>;

    /// Copy `source` to `target` on the device.
    ///
    /// # Errors
    ///
    /// Returns an error when the read fails, or when the device does not take
    /// the file.
    fn put<P: AsRef<Path>, R: Read, F: Fn(usize)>(
        &self,
        source: &mut R,
        target: P,
        progress: F,
    ) -> Result<(), TransferError>;

    /// Copy `source` from the device into `target`.
    ///
    /// `progress` is called with the running total of bytes read, the same way
    /// [`FileTransfer::put`] reports them. Pass `|_| {}` to ignore it.
    ///
    /// # Errors
    ///
    /// Returns an error when the device does not hand the file over, or when
    /// the write fails.
    fn get<P: AsRef<Path>, W: Write, F: Fn(usize)>(
        &self,
        source: P,
        target: &mut W,
        progress: F,
    ) -> Result<(), TransferError>;

    /// Delete `path` from the device.
    ///
    /// # Errors
    ///
    /// Returns an error when the device turns the request down.
    fn rm<P: AsRef<Path>>(&self, path: P) -> Result<(), TransferError>;

    /// Read the sha256 of a file on the device, as lowercase hex.
    /// Returns `None` if the device has no usable `sha256sum` command.
    ///
    /// # Errors
    ///
    /// Returns an error when the command cannot run at all.
    fn sha256sum<P: AsRef<Path>>(&self, path: P) -> Result<Option<String>, TransferError>;
}

/// What a path is on the device.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathKind {
    Dir,
    File,
    /// Neither of the two: a device node, a socket, or a symlink with no target.
    Other,
    Missing,
}

/// One name directly under a directory, with what it is.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirEntry {
    pub name: String,
    pub kind: PathKind,
}

/// An open file transfer to a device.
///
/// [`Transfer::open`] picks SFTP or a stream over exec channels once, and every
/// call after that uses the same transport. Copying many files this way costs
/// one SFTP handshake instead of one per file.
///
/// A device is set to stream with `"files": "stream"` in the device list, for
/// firmware whose SSH server has no SFTP subsystem. The stream path needs only
/// a shell on the device, so it uses `cat`, `mkdir -p` and `rm -rf`.
pub struct Transfer<'a> {
    session: &'a Session,
    sftp: Option<Sftp>,
    is_root: bool,
}

#[derive(Debug)]
pub enum TransferError {
    ExitCode { code: i32, reason: String },
    Ssh(SshError),
    Io(IoError),
}

impl<'a> Transfer<'a> {
    /// Open a transfer over `connection`.
    ///
    /// Falls back to streaming when the device is set to stream, and also when
    /// the SFTP session does not start.
    pub fn open<T: SshConnection + ?Sized>(connection: &'a T) -> Self {
        let sftp = if connection.supports_sftp() {
            connection.session().sftp().ok()
        } else {
            None
        };
        Self {
            session: connection.session(),
            sftp,
            is_root: connection.is_root(),
        }
    }

    /// `true` when files move over SFTP, `false` when they stream over exec
    /// channels.
    #[must_use]
    pub fn is_sftp(&self) -> bool {
        self.sftp.is_some()
    }

    /// What `path` is on the device. Symlinks are followed, so a link to a
    /// directory reads as [`PathKind::Dir`].
    ///
    /// A path the device will not talk about, for want of permission on a
    /// parent, reads as [`PathKind::Missing`]. The shell tests that ares-cli
    /// uses cannot tell those apart either.
    ///
    /// # Errors
    ///
    /// Returns an error only when the command itself cannot run.
    pub fn stat<P: AsRef<Path>>(&self, path: P) -> Result<PathKind, TransferError> {
        let path = path.as_ref().to_slash_lossy();
        if let Some(sftp) = &self.sftp {
            return Ok(sftp_kind(sftp, path.as_ref()));
        }
        let quoted = snailquote::escape(path.as_ref());
        let (out, _) = self.exec(&format!(
            "if [ -d {quoted} ]; then echo d; \
             elif [ -f {quoted} ]; then echo f; \
             elif [ -e {quoted} ] || [ -L {quoted} ]; then echo o; \
             else echo n; fi"
        ))?;
        Ok(parse_path_kind(&out))
    }

    /// Names directly under `path`, with what each one is. "." and ".." are
    /// left out, and symlinks are followed.
    ///
    /// # Errors
    ///
    /// Returns an error when `path` cannot be listed.
    pub fn read_dir<P: AsRef<Path>>(&self, path: P) -> Result<Vec<DirEntry>, TransferError> {
        let path = path.as_ref().to_slash_lossy();
        if let Some(sftp) = &self.sftp {
            let mut entries = Vec::new();
            for entry in sftp.read_dir(path.as_ref())? {
                let Some(name) = entry.name() else { continue };
                if name == "." || name == ".." {
                    continue;
                }
                let kind = match entry.file_type() {
                    Some(FileType::Directory) => PathKind::Dir,
                    Some(FileType::Regular) => PathKind::File,
                    // readdir reports the link itself, so follow it the way
                    // `[ -d ]` does.
                    _ => sftp_kind(sftp, &join(path.as_ref(), name)),
                };
                entries.push(DirEntry {
                    name: name.to_string(),
                    kind,
                });
            }
            return Ok(entries);
        }
        // One command per directory, and it needs no `find` on the device.
        // `cd` first so a name with a space needs no quoting of its own.
        let (out, status) = self.exec(&format!(
            "cd {} || exit 1; \
             for f in * .*; do \
             [ \"$f\" = . ] || [ \"$f\" = .. ] && continue; \
             if [ -d \"$f\" ]; then echo \"d $f\"; \
             elif [ -f \"$f\" ]; then echo \"f $f\"; \
             elif [ -e \"$f\" ] || [ -L \"$f\" ]; then echo \"o $f\"; fi; \
             done; exit 0",
            snailquote::escape(path.as_ref())
        ))?;
        if status != 0 {
            return Err(TransferError::ExitCode {
                code: status,
                reason: format!("Cannot list {path}"),
            });
        }
        Ok(parse_dir_listing(&out))
    }

    /// Make `dir` and every missing parent, the way `mkdir -p` does.
    ///
    /// # Errors
    ///
    /// Returns an error when a path in the chain exists and is not a directory,
    /// or when the device turns the request down.
    pub fn mkdir<P: AsRef<Path>>(&self, dir: P, mode: u32) -> Result<(), TransferError> {
        let path = dir.as_ref().to_slash_lossy();
        if let Some(sftp) = &self.sftp {
            return mkdir_sftp(sftp, path.as_ref(), mode);
        }
        let (_, status) = self.exec(&mkdir_command(path.as_ref(), mode, self.is_root))?;
        if status != 0 {
            return Err(TransferError::ExitCode {
                code: status,
                reason: format!("mkdir command exited with status {status}"),
            });
        }
        Ok(())
    }

    /// Copy `source` to `target` on the device.
    ///
    /// # Errors
    ///
    /// Returns an error when the read fails, or when the device does not take
    /// the file.
    pub fn put<P: AsRef<Path>, R: Read, F: Fn(usize)>(
        &self,
        source: &mut R,
        target: P,
        progress: F,
    ) -> Result<(), TransferError> {
        let target = target.as_ref().to_slash_lossy();
        if let Some(sftp) = &self.sftp {
            let mut file = sftp.open(
                target.as_ref(),
                OpenFlags::WRITE_ONLY | OpenFlags::CREATE | OpenFlags::TRUNCATE,
                0o644,
            )?;
            copy_with_progress(source, &mut file, progress)?;
            return Ok(());
        }
        let ch = self.session.new_channel()?;
        ch.open_session()?;
        ch.request_exec(&format!("cat > {}", snailquote::escape(target.as_ref())))?;
        copy_with_progress(source, &mut ch.stdin(), progress)?;
        ch.send_eof()?;
        let status = ch.get_exit_status().unwrap_or(0);
        ch.close()?;
        if status != 0 {
            return Err(TransferError::ExitCode {
                code: status,
                reason: format!("cat command exited with status {status}"),
            });
        }
        Ok(())
    }

    /// Copy `source` from the device into `target`.
    ///
    /// `progress` is called with the running total of bytes read. Pass `|_| {}`
    /// to ignore it.
    ///
    /// # Errors
    ///
    /// Returns an error when the device does not hand the file over, or when
    /// the write fails.
    pub fn get<P: AsRef<Path>, W: Write, F: Fn(usize)>(
        &self,
        source: P,
        target: &mut W,
        progress: F,
    ) -> Result<(), TransferError> {
        let source = source.as_ref().to_slash_lossy();
        if let Some(sftp) = &self.sftp {
            let mut file = sftp.open(source.as_ref(), OpenFlags::READ_ONLY, 0)?;
            copy_with_progress(&mut file, target, progress)?;
            return Ok(());
        }
        let ch = self.session.new_channel()?;
        ch.open_session()?;
        ch.request_exec(&format!("cat {}", snailquote::escape(source.as_ref())))?;
        copy_with_progress(&mut ch.stdout(), target, progress)?;
        let status = ch.get_exit_status().unwrap_or(0);
        ch.close()?;
        if status != 0 {
            return Err(TransferError::ExitCode {
                code: status,
                reason: format!("cat command exited with status {status}"),
            });
        }
        Ok(())
    }

    /// Delete `path` from the device.
    ///
    /// # Errors
    ///
    /// Returns an error when the device turns the request down.
    pub fn rm<P: AsRef<Path>>(&self, path: P) -> Result<(), TransferError> {
        let path = path.as_ref().to_slash_lossy();
        if let Some(sftp) = &self.sftp {
            sftp.remove_file(path.as_ref())?;
            return Ok(());
        }
        let (_, status) = self.exec(&format!("rm -rf {}", snailquote::escape(path.as_ref())))?;
        if status != 0 {
            return Err(TransferError::ExitCode {
                code: status,
                reason: format!("rm command exited with status {status}"),
            });
        }
        Ok(())
    }

    /// Read the sha256 of a file on the device, as lowercase hex.
    /// Returns `None` if the device has no usable `sha256sum` command.
    ///
    /// # Errors
    ///
    /// Returns an error when the command cannot run at all.
    pub fn sha256sum<P: AsRef<Path>>(&self, path: P) -> Result<Option<String>, TransferError> {
        let path = path.as_ref().to_slash_lossy();
        let (out, status) =
            self.exec(&format!("sha256sum {}", snailquote::escape(path.as_ref())))?;
        if status != 0 {
            // Some devices have no sha256sum. Report "can't tell" instead of an error.
            return Ok(None);
        }
        Ok(parse_sha256sum(&out))
    }

    /// Run `command` on the device and read its stdout. Returns the output and
    /// the exit status.
    fn exec(&self, command: &str) -> Result<(String, i32), TransferError> {
        let ch = self.session.new_channel()?;
        ch.open_session()?;
        ch.request_exec(command)?;
        ch.send_eof()?;
        let mut out = String::new();
        ch.stdout().read_to_string(&mut out)?;
        let status = ch.get_exit_status().unwrap_or(0);
        ch.close()?;
        Ok((out, status))
    }
}

impl TransferError {
    /// `true` when the device turned the operation down for want of permission.
    #[must_use]
    pub fn is_permission_denied(&self) -> bool {
        match self {
            TransferError::Ssh(e) => sftp_status(e) == Some(3),
            TransferError::Io(e) => e.kind() == ErrorKind::PermissionDenied,
            TransferError::ExitCode { .. } => false,
        }
    }
}

impl Display for TransferError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TransferError::ExitCode { reason, .. } => write!(f, "{reason}"),
            // A bare "Sftp error code 3" says nothing, so name the reason.
            TransferError::Ssh(e) => match sftp_status(e) {
                Some(code) => write!(f, "{}", sftp_reason(code)),
                None => write!(f, "{e}"),
            },
            TransferError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for TransferError {}

impl<T: SshConnection> FileTransfer for T {
    fn maybe_sftp(&self) -> Result<Sftp, SshError> {
        if !self.supports_sftp() {
            return Err(SshError::RequestDenied("SFTP is not supported".to_string()));
        }
        self.session().sftp()
    }

    fn mkdir<P: AsRef<Path>>(&self, dir: P, mode: u32) -> Result<(), TransferError> {
        Transfer::open(self).mkdir(dir, mode)
    }

    fn put<P: AsRef<Path>, R: Read, F: Fn(usize)>(
        &self,
        source: &mut R,
        target: P,
        progress: F,
    ) -> Result<(), TransferError> {
        Transfer::open(self).put(source, target, progress)
    }

    fn get<P: AsRef<Path>, W: Write, F: Fn(usize)>(
        &self,
        source: P,
        target: &mut W,
        progress: F,
    ) -> Result<(), TransferError> {
        Transfer::open(self).get(source, target, progress)
    }

    fn rm<P: AsRef<Path>>(&self, path: P) -> Result<(), TransferError> {
        Transfer::open(self).rm(path)
    }

    fn sha256sum<P: AsRef<Path>>(&self, path: P) -> Result<Option<String>, TransferError> {
        Transfer::open(self).sha256sum(path)
    }
}

impl From<IoError> for TransferError {
    fn from(value: IoError) -> Self {
        Self::Io(value)
    }
}
impl From<SshError> for TransferError {
    fn from(value: SshError) -> Self {
        Self::Ssh(value)
    }
}

fn copy_with_progress<R: Read, W: Write, F: Fn(usize)>(
    source: &mut R,
    target: &mut W,
    progress: F,
) -> Result<(), TransferError> {
    let mut buffer = [0u8; 1024 * 8];
    let mut total = 0usize;
    loop {
        let read = source.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        target.write_all(&buffer[..read])?;
        total += read;
        progress(total);
    }
    Ok(())
}

/// What `path` is, over SFTP. `sftp_stat` follows symlinks, the way `[ -d ]`
/// does.
fn sftp_kind(sftp: &Sftp, path: &str) -> PathKind {
    match sftp.metadata(path).map(|m| m.file_type()) {
        Ok(Some(FileType::Directory)) => PathKind::Dir,
        Ok(Some(FileType::Regular)) => PathKind::File,
        Ok(_) => PathKind::Other,
        Err(_) => PathKind::Missing,
    }
}

fn mkdir_sftp(sftp: &Sftp, path: &str, mode: u32) -> Result<(), TransferError> {
    match sftp_kind(sftp, path) {
        PathKind::Dir => return Ok(()),
        PathKind::Missing => {}
        _ => {
            return Err(TransferError::ExitCode {
                code: 1,
                reason: format!("File {path} exists and is not a directory"),
            });
        }
    }
    if let Some(parent) = parent_of(path) {
        mkdir_sftp(sftp, parent, mode)?;
    }
    // Another writer may win the race, so a failure only counts when the
    // directory still is not there.
    if let Err(e) = sftp.create_dir(path, mode)
        && sftp_kind(sftp, path) != PathKind::Dir
    {
        return Err(TransferError::Ssh(e));
    }
    Ok(())
}

/// Parent of a device path, which always uses "/". Returns None when there is
/// no parent left to make.
fn parent_of(path: &str) -> Option<&str> {
    let head = path.trim_end_matches('/').rsplit_once('/')?.0;
    if head.is_empty() { None } else { Some(head) }
}

/// Join a device path with one name below it.
fn join(dir: &str, name: &str) -> String {
    format!("{}/{name}", dir.trim_end_matches('/'))
}

/// Build the shell command that makes `dir`.
///
/// Only root gets the chmod. A non-root user can't change the mode of a directory
/// somebody else owns, and that directory is usually already usable, so trying
/// would fail the whole transfer for nothing.
fn mkdir_command(dir: &str, mode: u32, is_root: bool) -> String {
    let path = snailquote::escape(dir).to_string();
    if is_root {
        format!("(test -d {path} || mkdir -p {path}) && chmod {mode:o} {path}")
    } else {
        format!("test -d {path} || mkdir -p {path}")
    }
}

/// Read the one-letter answer of the `stat` command.
fn parse_path_kind(stdout: &str) -> PathKind {
    match stdout.trim() {
        "d" => PathKind::Dir,
        "f" => PathKind::File,
        "o" => PathKind::Other,
        _ => PathKind::Missing,
    }
}

/// Read the "<kind> <name>" lines of the `read_dir` command. A name may hold
/// spaces, so it runs to the end of the line.
fn parse_dir_listing(stdout: &str) -> Vec<DirEntry> {
    stdout
        .lines()
        .filter_map(|line| {
            let (kind, name) = line.trim_end_matches('\r').split_once(' ')?;
            if name.is_empty() {
                return None;
            }
            Some(DirEntry {
                name: name.to_string(),
                kind: parse_path_kind(kind),
            })
        })
        .collect()
}

/// Recover the numeric SFTP status code from a libssh error. `SftpError`'s code
/// field is private, so parse it out of the Display text ("Sftp error code N").
/// Returns `None` for non-SFTP errors.
fn sftp_status(e: &SshError) -> Option<u32> {
    if !matches!(e, SshError::Sftp(_)) {
        return None;
    }
    e.to_string().rsplit(' ').next()?.parse().ok()
}

/// Words for an SFTP status code (the `SSH_FX_*` set).
fn sftp_reason(code: u32) -> String {
    match code {
        1 => String::from("end of file"),
        2 => String::from("no such file or directory"),
        3 => String::from("permission denied"),
        4 => String::from("failure"),
        5 => String::from("bad message"),
        6 => String::from("no connection"),
        7 => String::from("connection lost"),
        8 => String::from("operation not supported"),
        other => format!("SFTP error code {other}"),
    }
}

/// Read the digest out of `sha256sum` output, which looks like "<hex>  <path>".
/// Returns `None` when the output is not a sha256, so a device without the
/// command doesn't fail the transfer.
fn parse_sha256sum(stdout: &str) -> Option<String> {
    stdout
        .split_whitespace()
        .next()
        .filter(|s| s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()))
        .map(str::to_lowercase)
}

#[cfg(test)]
mod tests {
    use libssh_rs::Session;

    use super::{
        DirEntry, FileTransfer, PathKind, TransferError, join, mkdir_command, parent_of,
        parse_dir_listing, parse_path_kind, parse_sha256sum, sftp_reason,
    };
    use crate::session::SshConnection;

    /// A connection handed out by a pool, as dev-manager-desktop has. It is not a
    /// [`crate::session::DeviceSession`], so it proves `FileTransfer` is reusable.
    struct PooledConnection {
        session: Session,
        uid: u32,
    }

    impl SshConnection for PooledConnection {
        fn session(&self) -> &Session {
            &self.session
        }

        fn supports_sftp(&self) -> bool {
            true
        }

        // A pool can know the real uid, which beats guessing from the user name.
        fn is_root(&self) -> bool {
            self.uid == 0
        }
    }

    fn assert_file_transfer<T: FileTransfer>() {}

    #[test]
    fn pooled_connection_can_transfer_files() {
        assert_file_transfer::<PooledConnection>();
    }

    #[test]
    fn root_gets_a_chmod() {
        assert_eq!(
            mkdir_command("/media/developer/temp", 0o777, true),
            concat!(
                "(test -d /media/developer/temp || mkdir -p /media/developer/temp)",
                " && chmod 777 /media/developer/temp"
            )
        );
    }

    #[test]
    fn a_normal_user_does_not_chmod() {
        // The directory is often left over from a root install, and chmod on
        // somebody else's directory fails and would abort the whole transfer.
        assert_eq!(
            mkdir_command("/media/developer/temp", 0o777, false),
            "test -d /media/developer/temp || mkdir -p /media/developer/temp"
        );
    }

    #[test]
    fn a_path_with_spaces_is_quoted() {
        let command = mkdir_command("/media/developer/my temp", 0o755, true);
        assert!(command.contains("'/media/developer/my temp'"), "{command}");
        assert!(command.contains("chmod 755"), "{command}");
    }

    #[test]
    fn parents_stop_at_the_root() {
        assert_eq!(parent_of("/media/developer/apps"), Some("/media/developer"));
        assert_eq!(
            parent_of("/media/developer/apps/"),
            Some("/media/developer")
        );
        // "/media" has only the root above it, and the root always exists.
        assert_eq!(parent_of("/media"), None);
        assert_eq!(parent_of("/"), None);
        assert_eq!(parent_of("app.ipk"), None);
    }

    #[test]
    fn names_join_under_a_directory() {
        assert_eq!(join("/var/log", "messages"), "/var/log/messages");
        assert_eq!(join("/var/log/", "messages"), "/var/log/messages");
        assert_eq!(join("/", "var"), "/var");
    }

    #[test]
    fn one_letter_answers_map_to_a_kind() {
        assert_eq!(parse_path_kind("d\n"), PathKind::Dir);
        assert_eq!(parse_path_kind("f\n"), PathKind::File);
        assert_eq!(parse_path_kind("o\n"), PathKind::Other);
        assert_eq!(parse_path_kind("n\n"), PathKind::Missing);
        assert_eq!(parse_path_kind(""), PathKind::Missing);
    }

    #[test]
    fn a_listing_reads_kind_and_name() {
        let out = "d apps\nf messages\nf my log.txt\no socket\n";
        assert_eq!(
            parse_dir_listing(out),
            vec![
                DirEntry {
                    name: String::from("apps"),
                    kind: PathKind::Dir
                },
                DirEntry {
                    name: String::from("messages"),
                    kind: PathKind::File
                },
                // A name with a space runs to the end of the line.
                DirEntry {
                    name: String::from("my log.txt"),
                    kind: PathKind::File
                },
                DirEntry {
                    name: String::from("socket"),
                    kind: PathKind::Other
                },
            ]
        );
    }

    #[test]
    fn a_listing_drops_lines_with_no_name() {
        assert_eq!(parse_dir_listing("\nd\nf \n"), vec![]);
    }

    #[test]
    fn sha256sum_output_yields_the_digest() {
        let digest = "a".repeat(64);
        assert_eq!(
            parse_sha256sum(&format!(
                "{digest}  /media/developer/temp/app.ipk
"
            )),
            Some(digest.clone())
        );
        // Some builds print uppercase hex.
        assert_eq!(
            parse_sha256sum(&format!("{}  file", digest.to_uppercase())),
            Some(digest)
        );
    }

    #[test]
    fn output_that_is_not_a_digest_is_ignored() {
        // A device with no sha256sum must skip the check, not fail the install.
        for output in [
            "",
            "
",
            "sh: sha256sum: not found
",
            "abc123  file
",
            &format!("{}  file", "z".repeat(64)),
        ] {
            assert_eq!(parse_sha256sum(output), None, "{output:?}");
        }
    }

    #[test]
    fn errors_read_as_sentences() {
        assert_eq!(
            TransferError::ExitCode {
                code: 1,
                reason: String::from("mkdir command exited with status 1"),
            }
            .to_string(),
            "mkdir command exited with status 1"
        );
        assert_eq!(sftp_reason(3), "permission denied");
        assert_eq!(sftp_reason(99), "SFTP error code 99");
    }
}
