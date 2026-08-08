use std::fmt::Display;
use std::process::exit;

/// Return the value, or print `Failed to <action>: <error>` and exit with code 1.
///
/// Use this in place of `unwrap()` in binaries. A panic gives users a backtrace
/// notice instead of a message they can act on.
pub fn unwrap_or_exit<T, E: Display>(result: Result<T, E>, action: &str) -> T {
    result.unwrap_or_else(|e| {
        eprintln!("Failed to {action}: {e}");
        exit(1);
    })
}
