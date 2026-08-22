//! Who hears what.
//!
//! One rule, borrowed from `adb exec-out screencap`: **stdout is data, stderr
//! is for people.** stdout carries PNG bytes, NDJSON events and key listings,
//! and nothing else — so `ares-do screenshot > shot.png` and
//! `ares-do --json run - | jq` both work without a flag to quieten chatter.

use std::io::{Write, stderr, stdout};
use std::time::Instant;

use serde_json::{Value, json};

use crate::error::DoError;

/// Wall time for one action, for the `ms` field an agent uses to spot a step
/// that took far longer than the others.
pub(crate) struct Timer(Instant);

impl Timer {
    pub(crate) fn start() -> Self {
        Timer(Instant::now())
    }

    pub(crate) fn ms(&self) -> u64 {
        u64::try_from(self.0.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

pub(crate) struct Reporter {
    json: bool,
    quiet: bool,
    verbose: u8,
}

impl Reporter {
    pub(crate) fn new(json: bool, quiet: bool, verbose: u8) -> Self {
        Reporter {
            json,
            quiet,
            verbose,
        }
    }

    pub(crate) fn is_json(&self) -> bool {
        self.json
    }

    pub(crate) fn is_verbose(&self) -> bool {
        self.verbose > 0
    }

    /// One machine-readable event, one line, flushed.
    ///
    /// Flushing per line is what lets a caller react to step 3 while step 4 is
    /// still running, and what leaves a valid prefix behind if the process
    /// dies mid-flow.
    pub(crate) fn event(&self, value: &Value) {
        if !self.json {
            return;
        }
        let mut out = stdout().lock();
        // Nothing useful to do if stdout is gone; the exit code still lands.
        let _ = writeln!(out, "{value}");
        let _ = out.flush();
    }

    /// Progress for a person. Silent under `--json`, which owns the narration,
    /// and under `--quiet`.
    pub(crate) fn info(&self, message: &str) {
        if self.quiet || self.json {
            return;
        }
        let _ = writeln!(stderr(), "{message}");
    }

    /// Something worth knowing that is not a failure. Survives `--json`,
    /// because it is not part of the event contract, and survives `--quiet`,
    /// because silencing a warning is not what quiet is for.
    ///
    /// Takes `&self` for symmetry with the rest of the reporter, even though
    /// no setting can silence it.
    #[allow(clippy::unused_self)]
    pub(crate) fn warn(&self, message: &str) {
        let _ = writeln!(stderr(), "warning: {message}");
    }

    /// What is about to be sent to the device, under `-v`.
    pub(crate) fn trace(&self, message: &str) {
        if self.verbose == 0 {
            return;
        }
        let _ = writeln!(stderr(), "{message}");
    }

    /// The last thing printed on a failing run.
    ///
    /// Under `--json` the error is also the final event, carrying the same
    /// code the process exits with — so a caller that only parses the last
    /// line still learns everything.
    pub(crate) fn error(&self, error: &DoError) {
        let _ = writeln!(stderr(), "{error}");
        self.event(&json!({
            "event": "error",
            "ok": false,
            "code": error.exit_code(),
            "kind": error.kind(),
            "message": error.to_string(),
        }));
    }
}
