//! Taking a picture of the screen and bringing it back.
//!
//! The device writes the PNG to its own filesystem and says nothing about when
//! it finished, so the sequence is: ask, wait for the file to appear, copy it,
//! check it is a whole PNG, delete it. Every one of those steps has a way to
//! go wrong quietly, which is why none of them are skipped.

use std::fmt::{Display, Formatter};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{fs, process};

use ares_connection_lib::session::DeviceSession;
use ares_connection_lib::transfer::{PathKind, Transfer};
use clap::ValueEnum;
use serde_json::json;

use crate::error::DoError;
use crate::luna::{self, LunaReply};
use crate::output::Reporter;

/// Tried first. Newer firmware has it.
const CAPTURE_URI: &str = "luna://com.webos.service.capture/executeOneShot";
/// Tried second, for the sets that only have this one.
const TV_CAPTURE_URI: &str = "luna://com.webos.service.tv.capture/executeOneShot";

/// How long to wait for the capture service to actually produce the file.
const APPEAR_TIMEOUT: Duration = Duration::from_secs(5);
const APPEAR_POLL: Duration = Duration::from_millis(50);

const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub(crate) enum CaptureMethod {
    /// Everything on screen, which is almost always what you want.
    #[default]
    Display,
    /// The video plane alone. Comes back black under HDCP.
    Video,
    /// The graphics plane alone: UI without video behind it.
    Graphic,
}

impl CaptureMethod {
    fn as_luna(self) -> &'static str {
        match self {
            CaptureMethod::Display => "DISPLAY",
            CaptureMethod::Video => "VIDEO",
            CaptureMethod::Graphic => "GRAPHIC",
        }
    }
}

impl Display for CaptureMethod {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_luna())
    }
}

/// Where a captured PNG should end up.
pub(crate) enum ShotTarget {
    /// Straight to stdout, as `adb exec-out screencap -p` does.
    Stdout,
    File(PathBuf),
}

/// A file on the device that must not outlive this value.
///
/// `/tmp` is tmpfs on a TV and a 4K PNG is several megabytes, so leaking these
/// across a long flow is a real way to fill the device up.
///
/// Registered *before* the capture is asked for, so no early return can skip
/// it, and armed only once the service has accepted. A capture the service
/// refused left no file behind, and warning that it could not be deleted
/// points the reader at the wrong problem entirely.
struct RemoteTemp<'a> {
    transfer: &'a Transfer<'a>,
    path: String,
    reporter: &'a Reporter,
    armed: bool,
}

impl RemoteTemp<'_> {
    /// The service accepted, so a file may now exist.
    fn arm(&mut self) {
        self.armed = true;
    }
}

impl Drop for RemoteTemp<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if let Err(e) = self.transfer.rm(&self.path) {
            // Cleanup failing is not worth losing the screenshot over, but it
            // is worth knowing about — the next one may fail for want of space.
            self.reporter.warn(&format!(
                "could not delete {} on the device: {e}",
                self.path
            ));
        }
    }
}

/// A unique path on the device for one capture.
///
/// The shared `ares-do-` prefix means anything a killed run leaves behind can
/// be found and swept up later; the pid and clock keep two runs apart.
fn remote_path(dir: &str, seq: u32) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let dir = dir.trim_end_matches('/');
    format!("{dir}/ares-do-{}-{seq}-{nanos:08x}.png", process::id())
}

/// Is this a complete PNG, rather than a truncated one or an error page?
///
/// The capture service reports success before the file is finished, so reading
/// it too early gives a valid-looking prefix. Checking for the end marker is
/// the cheap way to notice.
fn is_whole_png(bytes: &[u8]) -> bool {
    bytes.len() > PNG_MAGIC.len() + 12
        && bytes.starts_with(PNG_MAGIC)
        && bytes.windows(4).rev().take(12).any(|w| w == b"IEND")
}

pub(crate) struct CaptureResult {
    pub bytes: Vec<u8>,
    pub method: CaptureMethod,
    pub service: &'static str,
}

/// Which capture service answered last time, so a flow of twenty shots does
/// not pay for the fallback twenty times.
pub(crate) struct Capturer {
    method: CaptureMethod,
    timeout: Option<Duration>,
    remote_dir: String,
    retries: u8,
    known_uri: Option<&'static str>,
    seq: u32,
}

impl Capturer {
    pub(crate) fn new(
        method: CaptureMethod,
        remote_dir: String,
        retries: u8,
        timeout: Option<Duration>,
    ) -> Self {
        Capturer {
            method,
            timeout,
            remote_dir,
            retries,
            known_uri: None,
            seq: 0,
        }
    }

    /// Ask one service to capture to `path`.
    fn request(
        &self,
        session: &DeviceSession,
        uri: &'static str,
        path: &str,
        reporter: &Reporter,
    ) -> Result<(), DoError> {
        let payload = json!({
            "path": path,
            "method": self.method.as_luna(),
            "format": "PNG",
        });
        let reply: LunaReply = luna::raw(&session.session, uri, &payload, self.timeout, reporter)?;
        reply.check(uri)
    }

    /// Ask whichever service exists, remembering which one that was.
    ///
    /// The retry is on *any* failure of the first URI, not on a particular
    /// error string: `LunaError` collapses "no such service" and "no
    /// permission" into one value, so there is no message to match on.
    fn request_either(
        &mut self,
        session: &DeviceSession,
        path: &str,
        reporter: &Reporter,
    ) -> Result<&'static str, DoError> {
        if let Some(uri) = self.known_uri {
            self.request(session, uri, path, reporter)?;
            return Ok(uri);
        }

        match self.request(session, CAPTURE_URI, path, reporter) {
            Ok(()) => {
                self.known_uri = Some(CAPTURE_URI);
                Ok(CAPTURE_URI)
            }
            Err(first) if luna::is_worth_retrying_elsewhere(&first) => {
                reporter.trace(&format!(
                    "{CAPTURE_URI} failed ({first}); trying {TV_CAPTURE_URI}"
                ));
                self.request(session, TV_CAPTURE_URI, path, reporter)
                    .map_err(|second| {
                        DoError::Capture(format!(
                            "neither capture service worked.\n  {CAPTURE_URI}: {first}\n  \
                             {TV_CAPTURE_URI}: {second}"
                        ))
                    })?;
                self.known_uri = Some(TV_CAPTURE_URI);
                Ok(TV_CAPTURE_URI)
            }
            Err(other) => Err(other),
        }
    }

    /// Wait until the capture service has actually written something.
    fn wait_for_file(transfer: &Transfer<'_>, path: &str) -> Result<(), DoError> {
        let deadline = std::time::Instant::now() + APPEAR_TIMEOUT;
        loop {
            if matches!(transfer.stat(path), Ok(PathKind::File)) {
                return Ok(());
            }
            if std::time::Instant::now() >= deadline {
                // stat says Missing for "not there" and for "cannot see it",
                // so do not claim to know which.
                return Err(DoError::Timeout(format!(
                    "the capture file {path} to appear on the device"
                )));
            }
            sleep(APPEAR_POLL);
        }
    }

    /// Capture once, and hand back the bytes.
    ///
    /// # Errors
    ///
    /// [`DoError::Capture`] when neither service works or the file never
    /// becomes a whole PNG, plus whatever the transfer failed with.
    pub(crate) fn capture(
        &mut self,
        session: &DeviceSession,
        transfer: &Transfer<'_>,
        reporter: &Reporter,
    ) -> Result<CaptureResult, DoError> {
        let mut last: Option<DoError> = None;

        for attempt in 0..=self.retries {
            self.seq += 1;
            let path = remote_path(&self.remote_dir, self.seq);
            // Registered before the call, so a failure anywhere below still
            // takes the file with it.
            let mut temp = RemoteTemp {
                transfer,
                path: path.clone(),
                reporter,
                armed: false,
            };

            let service = self.request_either(session, &path, reporter)?;
            temp.arm();
            Self::wait_for_file(transfer, &path)?;

            let mut bytes = Vec::new();
            transfer.get(&path, &mut bytes, |_| {})?;

            if is_whole_png(&bytes) {
                return Ok(CaptureResult {
                    bytes,
                    method: self.method,
                    service: service
                        .trim_start_matches("luna://")
                        .split('/')
                        .next()
                        .unwrap_or(service),
                });
            }

            let complaint = if bytes.is_empty() {
                String::from("the capture file was empty")
            } else if bytes.starts_with(PNG_MAGIC) {
                format!("the PNG was truncated ({} bytes, no IEND)", bytes.len())
            } else {
                format!("the file was not a PNG ({} bytes)", bytes.len())
            };
            if attempt < self.retries {
                reporter.trace(&format!("{complaint}; retrying"));
            }
            last = Some(DoError::Capture(complaint));
        }

        Err(last.unwrap_or_else(|| DoError::Capture(String::from("no attempt was made"))))
    }
}

/// Put the bytes where they were asked for.
///
/// A file is written beside its destination and renamed, so anything watching
/// the directory never opens a half-written PNG.
///
/// # Errors
///
/// [`DoError::Io`] if the file cannot be written or renamed.
pub(crate) fn write_shot(target: &ShotTarget, bytes: &[u8]) -> Result<(), DoError> {
    match target {
        ShotTarget::Stdout => {
            let mut out = std::io::stdout().lock();
            out.write_all(bytes)?;
            out.flush()?;
            Ok(())
        }
        ShotTarget::File(path) => {
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                fs::create_dir_all(parent)?;
            }
            let part = temp_sibling(path);
            fs::write(&part, bytes)?;
            fs::rename(&part, path)?;
            Ok(())
        }
    }
}

fn temp_sibling(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.part", process::id()));
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{CaptureMethod, PNG_MAGIC, is_whole_png, remote_path, temp_sibling};

    fn png(body: &[u8]) -> Vec<u8> {
        let mut v = PNG_MAGIC.to_vec();
        v.extend_from_slice(body);
        v
    }

    #[test]
    fn a_complete_png_passes() {
        assert!(is_whole_png(&png(b"....IHDR........IEND\xae\x42\x60\x82")));
    }

    #[test]
    fn a_truncated_png_fails() {
        assert!(!is_whole_png(&png(
            b"....IHDR....some pixels but no end marker"
        )));
    }

    #[test]
    fn an_empty_or_foreign_file_fails() {
        assert!(!is_whole_png(b""));
        assert!(!is_whole_png(b"<html>not a png at all, really not</html>"));
        assert!(!is_whole_png(PNG_MAGIC));
    }

    #[test]
    fn remote_paths_are_unique_and_sweepable() {
        let a = remote_path("/tmp", 1);
        let b = remote_path("/tmp", 2);
        assert_ne!(a, b);
        assert!(a.starts_with("/tmp/ares-do-"), "{a}");
        assert!(
            std::path::Path::new(&a)
                .extension()
                .is_some_and(|e| e == "png"),
            "{a}"
        );
    }

    #[test]
    fn a_trailing_slash_does_not_double_up() {
        assert!(!remote_path("/tmp/", 1).contains("//"));
    }

    #[test]
    fn the_part_file_sits_beside_the_target() {
        let part = temp_sibling(Path::new("/out/shot.png"));
        assert_eq!(part.parent(), Path::new("/out/shot.png").parent());
        assert!(part.to_string_lossy().ends_with(".part"), "{part:?}");
    }

    #[test]
    fn methods_render_as_the_service_spells_them() {
        assert_eq!(CaptureMethod::Display.to_string(), "DISPLAY");
        assert_eq!(CaptureMethod::Video.to_string(), "VIDEO");
        assert_eq!(CaptureMethod::Graphic.to_string(), "GRAPHIC");
    }
}
