//! Tab completion driven by the device.
//!
//! There is no local copy of the remote file system, so every Tab asks the
//! device: a hidden command expands a glob and the answer comes back through
//! the same fence the prompt uses (see [`crate::marker`]). One round trip per
//! Tab.
//!
//! This completes paths and command names. It cannot do what `bash-completion`
//! does — git subcommands, per-command flags — because those scripts are not on
//! the device, and a non-interactive shell would not load them anyway.

use std::io::Write;
use std::thread;
use std::time::{Duration, Instant};

use libssh_rs::Channel;
use libssh_rs::Error::TryAgain;

use crate::marker::{REPORT_CMD, Scanner};

/// How long to wait for the device to answer a Tab before giving up on it.
const QUERY_TIMEOUT: Duration = Duration::from_secs(2);
/// How long to wait between reads while an answer comes in.
const POLL_INTERVAL: Duration = Duration::from_millis(5);
/// Longest answer we keep. A Tab on `/` in a huge directory is not worth more.
const MAX_ANSWER: usize = 64 * 1024;
/// Most candidates we hand back.
const MAX_CANDIDATES: usize = 500;
/// Characters that end the word Tab completes.
const SEPARATORS: &[char] = &[' ', '\t', ';', '|', '&', '<', '>', '(', ')', '='];

/// Sends one hidden command and collects what it prints.
///
/// Returns `None` if the device did not answer in time. The caller must then
/// expect a stray record later, because the answer may still turn up.
pub(crate) fn query(ch: &Channel, scanner: &mut Scanner, command: &str) -> Option<String> {
    {
        let mut stdin = ch.stdin();
        stdin.write_all(command.as_bytes()).ok()?;
        stdin.write_all(b"\n").ok()?;
        stdin.write_all(REPORT_CMD.as_bytes()).ok()?;
        stdin.flush().ok()?;
    }

    let deadline = Instant::now() + QUERY_TIMEOUT;
    let mut buf = [0u8; 8192];
    let mut out = Vec::new();
    let mut chunk = Vec::new();
    loop {
        let mut done = false;
        loop {
            match ch.read_nonblocking(&mut buf, false) {
                Ok(0) | Err(TryAgain) => break,
                Ok(size) => {
                    chunk.clear();
                    if !scanner.push(&buf[..size], &mut chunk).is_empty() {
                        done = true;
                    }
                    if out.len() < MAX_ANSWER {
                        out.extend_from_slice(&chunk);
                    }
                }
                Err(_) => return None,
            }
        }
        // Glob errors land on stderr and are none of the user's business here.
        while matches!(ch.read_nonblocking(&mut buf, true), Ok(n) if n > 0) {}

        if done {
            return Some(String::from_utf8_lossy(&out).into_owned());
        }
        if ch.is_eof() || ch.is_closed() || Instant::now() >= deadline {
            return None;
        }
        thread::sleep(POLL_INTERVAL);
    }
}

/// Where the word Tab completes starts.
pub(crate) fn word_start(line: &str, pos: usize) -> usize {
    let head = &line[..pos];
    let mut start = 0;
    let mut escaped = false;
    for (i, c) in head.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
        } else if SEPARATORS.contains(&c) {
            start = i + c.len_utf8();
        }
    }
    start
}

/// True when the word starts a command, so command names belong in the list.
pub(crate) fn is_command_position(line: &str, start: usize) -> bool {
    let head = line[..start].trim_end();
    head.is_empty()
        || head.ends_with([';', '|', '&', '('])
        || head.ends_with("do")
        || head.ends_with("then")
}

/// Builds the command that lists paths starting with `prefix`.
pub(crate) fn path_query(prefix: &str) -> String {
    format!(
        "__p={}; for f in \"$__p\"*; do \
         if [ -d \"$f\" ]; then printf '%s/\\n' \"$f\"; \
         elif [ -e \"$f\" ]; then printf '%s\\n' \"$f\"; fi; done",
        quote(prefix)
    )
}

/// Builds the command that lists programs on `$PATH` starting with `prefix`.
///
/// The loop runs in a subshell so the changed `IFS` does not outlive it.
pub(crate) fn command_query(prefix: &str) -> String {
    format!(
        "__p={}; ( IFS=:; for d in $PATH; do for f in \"$d/$__p\"*; do \
         if [ -f \"$f\" ] && [ -x \"$f\" ]; then printf '%s\\n' \"${{f##*/}}\"; fi; \
         done; done )",
        quote(prefix)
    )
}

/// Wraps text in single quotes so the remote shell reads it literally.
pub(crate) fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('\'');
    for c in text.chars() {
        if c == '\'' {
            // Close the quote, add an escaped one, open again.
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

/// Turns the lines the device printed into candidates.
pub(crate) fn parse(answer: &str) -> Vec<String> {
    let mut out: Vec<String> = answer
        .lines()
        .map(|line| line.trim_end_matches('\r'))
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    out.sort_unstable();
    out.dedup();
    out.truncate(MAX_CANDIDATES);
    out
}

/// Drops the backslashes the user typed to protect a character.
pub(crate) fn unescape(word: &str) -> String {
    let mut out = String::with_capacity(word.len());
    let mut escaped = false;
    for c in word.chars() {
        if escaped {
            out.push(c);
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else {
            out.push(c);
        }
    }
    out
}

/// Puts those backslashes back, so the completed word survives the shell.
pub(crate) fn escape(word: &str) -> String {
    let mut out = String::with_capacity(word.len());
    for c in word.chars() {
        if " \t'\"\\$`&;|<>()*?[]{}!#~".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        command_query, escape, is_command_position, parse, path_query, quote, unescape, word_start,
    };

    #[test]
    fn finds_the_word_under_the_cursor() {
        assert_eq!(word_start("ls /media/dev", 13), 3);
        assert_eq!(word_start("ls", 2), 0);
        assert_eq!(word_start("cat a.txt | gre", 15), 12);
        assert_eq!(word_start("ls ", 3), 3);
        // A backslash protects the space, so the word carries on.
        assert_eq!(word_start(r"ls my\ fi", 9), 3);
    }

    #[test]
    fn tells_a_command_from_an_argument() {
        assert!(is_command_position("ls", 0));
        assert!(is_command_position("  ls", 2));
        assert!(is_command_position("cat x | gre", 8));
        assert!(is_command_position("cd /tmp; l", 9));
        assert!(!is_command_position("ls /med", 3));
        assert!(!is_command_position("cat x | grep fo", 13));
    }

    #[test]
    fn quotes_text_the_shell_would_otherwise_read() {
        assert_eq!(quote("plain"), "'plain'");
        assert_eq!(quote("it's"), "'it'\\''s'");
        assert_eq!(quote("$(rm -rf /)"), "'$(rm -rf /)'");
    }

    #[test]
    fn builds_queries_that_keep_the_prefix_literal() {
        let q = path_query("a b");
        assert!(q.contains("__p='a b';"));
        assert!(q.contains("for f in \"$__p\"*"));
        let q = command_query("l");
        assert!(q.contains("IFS=:"));
        assert!(q.contains("\"$d/$__p\"*"));
    }

    #[test]
    fn sorts_and_dedupes_the_answer() {
        assert_eq!(parse("b\r\na\n\nb\n"), vec!["a".to_owned(), "b".to_owned()]);
        assert!(parse("").is_empty());
    }

    #[test]
    fn round_trips_escapes() {
        assert_eq!(unescape(r"my\ file"), "my file");
        assert_eq!(escape("my file"), r"my\ file");
        assert_eq!(unescape(&escape("a'b c")), "a'b c");
    }
}
