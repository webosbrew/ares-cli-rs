//! A friendlier shell for when the device gives us no pseudo-terminal.
//!
//! The remote side runs non-interactive, so it prints no prompt and echoes
//! nothing. Everything the user sees here is drawn locally: a prompt built from
//! the state report the remote shell sends between commands (see [`crate::marker`]),
//! and a line editor from `rustyline` for arrow keys and history.
//!
//! The two phases never overlap. While the prompt is up, `rustyline` owns the
//! terminal. While a command runs, this module owns it and forwards keys straight
//! to the remote, so `cat` and friends still work.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::fmt::Write as _;
use std::io::{Error, Write, stderr, stdout};
use std::time::{Duration, Instant};

use ares_device_lib::Device;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use libssh_rs::Channel;
use libssh_rs::Error::TryAgain;
use rustyline::completion::Completer;
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{CompletionType, Config, Editor, Helper};

use crate::io::{RawMode, io_error};
use crate::marker::{REPORT_CMD, Report, Scanner};
use crate::{complete, dumb};

/// How long the command phase waits for a key before it looks at the remote
/// again.
const POLL_INTERVAL: Duration = Duration::from_millis(10);
/// How long to wait for the first state report before giving up on the prompt.
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
/// How long to keep draining output after the session ends.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
/// Press Ctrl+C twice inside this window to drop the connection.
const INTERRUPT_WINDOW: Duration = Duration::from_secs(1);

/// Why the command phase stopped.
enum Stop {
    Report(Report),
    /// The remote closed the channel.
    Eof,
    /// No report arrived in time.
    Timeout,
    /// The user pressed Ctrl+C twice.
    Aborted,
}

pub(crate) fn shell(ch: Channel, device: &Device) -> Result<i32, Error> {
    let mut scanner = Scanner::new();
    let to_skip = Cell::new(0u32);

    // Ask once before the first prompt. Anything the server prints ahead of the
    // report, such as a message of the day, goes to the screen as usual.
    send_report_cmd(&ch)?;
    let mut state = match run(&ch, &mut scanner, &to_skip, Some(PROBE_TIMEOUT))? {
        Stop::Report(report) => report,
        Stop::Eof | Stop::Aborted => return Ok(exit_status(&ch)),
        Stop::Timeout => {
            eprintln!("This shell can't report its state, so there is no prompt.");
            return dumb::shell(ch);
        }
    };

    let color = Palette::pick();
    // List on Tab and fill in the common prefix, the way bash does, rather than
    // cycling through candidates one at a time.
    let config = Config::builder()
        .completion_type(CompletionType::List)
        .build();
    let mut editor: Editor<EditorHelper, DefaultHistory> =
        Editor::with_config(config).map_err(readline_error)?;
    editor.set_helper(Some(EditorHelper::new(&ch, &to_skip)));
    let mut command = String::new();
    loop {
        let (plain, colored) = if command.is_empty() {
            (
                prompt(&state, &device.name, &Palette::NONE),
                prompt(&state, &device.name, &color),
            )
        } else {
            (
                continuation_prompt(&Palette::NONE),
                continuation_prompt(&color),
            )
        };
        if let Some(helper) = editor.helper_mut() {
            helper.colored = colored;
            helper.home.clone_from(&state.home);
        }
        match editor.readline(&plain) {
            Ok(line) => {
                command.push_str(&line);
                // An open quote would leave the remote shell waiting for the
                // rest, and it would swallow the reporting command with it. Ask
                // for the rest here instead.
                if is_incomplete(&command) {
                    command.push('\n');
                    continue;
                }
                if command.trim().is_empty() {
                    command.clear();
                    continue;
                }
                let _ = editor.add_history_entry(command.as_str());
                let mut stdin = ch.stdin();
                stdin.write_all(command.as_bytes())?;
                stdin.write_all(b"\n")?;
                stdin.flush()?;
                command.clear();
                send_report_cmd(&ch)?;
            }
            // Ctrl+C at the prompt: throw the line away and start over.
            Err(ReadlineError::Interrupted) => {
                command.clear();
                continue;
            }
            // Ctrl+D on an empty line: end the session like any other shell.
            // The far end drops the socket as it goes, so a read error while
            // draining the last output is the expected ending, not a fault.
            Err(ReadlineError::Eof) => {
                let _ = ch.send_eof();
                let _ = run(&ch, &mut scanner, &to_skip, Some(DRAIN_TIMEOUT));
                break;
            }
            Err(e) => return Err(readline_error(e)),
        }

        match run(&ch, &mut scanner, &to_skip, None)? {
            Stop::Report(report) => state = report,
            Stop::Eof | Stop::Timeout => break,
            Stop::Aborted => return Ok(130),
        }
    }

    Ok(exit_status(&ch))
}

/// Asks the remote shell to report its state.
fn send_report_cmd(ch: &Channel) -> Result<(), Error> {
    let mut stdin = ch.stdin();
    stdin.write_all(REPORT_CMD.as_bytes())?;
    stdin.flush()
}

/// Runs the command phase: remote output to the screen, keys to the remote,
/// until a state report arrives or the session ends.
fn run(
    ch: &Channel,
    scanner: &mut Scanner,
    to_skip: &Cell<u32>,
    timeout: Option<Duration>,
) -> Result<Stop, Error> {
    let _raw = RawMode::enable()?;
    let deadline = timeout.map(|t| Instant::now() + t);
    let mut buf = [0u8; 8192];
    let mut plain = Vec::with_capacity(buf.len());
    let mut last_interrupt: Option<Instant> = None;

    loop {
        // Only stdout can carry a report: the reporting command prints there.
        let mut report = None;
        loop {
            match ch.read_nonblocking(&mut buf, false) {
                Ok(0) | Err(TryAgain) => break,
                Ok(size) => {
                    plain.clear();
                    for found in scanner.push(&buf[..size], &mut plain) {
                        // A Tab the device answered too late. Its record is not
                        // the one this command produced, so let it go by.
                        if to_skip.get() > 0 {
                            to_skip.set(to_skip.get() - 1);
                        } else {
                            report = Some(found);
                        }
                    }
                    write_raw(&mut stdout().lock(), &plain)?;
                }
                // A read fails once the far end drops the connection. That is
                // how a session ends, so only report it if the channel still
                // looks alive.
                Err(_) if ch.is_eof() || ch.is_closed() => return Ok(Stop::Eof),
                Err(e) => return Err(io_error(e)),
            }
        }
        loop {
            match ch.read_nonblocking(&mut buf, true) {
                Ok(0) | Err(TryAgain) => break,
                Ok(size) => write_raw(&mut stderr().lock(), &buf[..size])?,
                Err(_) if ch.is_eof() || ch.is_closed() => return Ok(Stop::Eof),
                Err(e) => return Err(io_error(e)),
            }
        }
        if let Some(report) = report {
            return Ok(Stop::Report(report));
        }
        if ch.is_eof() || ch.is_closed() {
            return Ok(Stop::Eof);
        }
        if deadline.is_some_and(|d| Instant::now() >= d) {
            return Ok(Stop::Timeout);
        }

        if !event::poll(POLL_INTERVAL)? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if is_interrupt(&key) {
            let now = Instant::now();
            if last_interrupt.is_some_and(|t| now.duration_since(t) < INTERRUPT_WINDOW) {
                write_raw(&mut stdout().lock(), b"\r\n")?;
                return Ok(Stop::Aborted);
            }
            last_interrupt = Some(now);
            // Without a pseudo-terminal there is no line discipline to turn this
            // into a signal, and most servers ignore a signal request. Send both
            // and tell the user what to do if neither works.
            let _ = ch.request_send_signal("INT");
            let mut stdin = ch.stdin();
            stdin.write_all(&[0x03])?;
            stdin.flush()?;
            write_raw(
                &mut stdout().lock(),
                b"^C\r\n(press Ctrl+C again to close the connection)\r\n",
            )?;
            continue;
        }
        if let Some(bytes) = key_bytes(&key) {
            // Nothing echoes on the far end, so show the keys here.
            echo(&mut stdout().lock(), &bytes)?;
            let mut stdin = ch.stdin();
            stdin.write_all(&bytes)?;
            stdin.flush()?;
        }
    }
}

/// Draws the prompt, for example `ares root@tv:/media/developer [1]# `.
fn prompt(state: &Report, host: &str, color: &Palette) -> String {
    let user = if state.user.is_empty() {
        "?"
    } else {
        &state.user
    };
    let root = user == "root";
    let mut out = String::new();
    out.push_str(color.dim);
    out.push_str("ares ");
    out.push_str(color.reset);
    out.push_str(color.user);
    out.push_str(user);
    out.push('@');
    out.push_str(host);
    out.push_str(color.reset);
    out.push(':');
    out.push_str(color.cwd);
    out.push_str(&shorten(&state.cwd, &state.home));
    out.push_str(color.reset);
    out.push(' ');
    if state.status != 0 {
        out.push_str(color.error);
        let _ = write!(out, "[{}]", state.status);
        out.push_str(color.reset);
    }
    out.push_str(if root { color.root } else { color.plain });
    out.push(if root { '#' } else { '$' });
    out.push_str(color.reset);
    out.push(' ');
    out
}

/// Draws the prompt that asks for the rest of an unfinished command.
fn continuation_prompt(color: &Palette) -> String {
    format!("{}ares{} > ", color.dim, color.reset)
}

/// True when the line ends inside a quote or on a line continuation, so the
/// remote shell would wait for more.
///
/// This only looks at quoting and escapes, which covers a typo such as a stray
/// `'`. It does not track `for` or `if` blocks: those are rare to type by hand
/// at a debug prompt, and telling them apart needs a real parser.
fn is_incomplete(line: &str) -> bool {
    let mut single = false;
    let mut double = false;
    let mut escaped = false;
    for c in line.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' if !single => escaped = true,
            '\'' if !double => single = !single,
            '"' if !single => double = !double,
            _ => {}
        }
    }
    single || double || escaped
}

/// Replaces the home directory with `~`, the way a shell prompt does.
fn shorten(cwd: &str, home: &str) -> String {
    if home.is_empty() || home == "/" || !cwd.starts_with(home) {
        return cwd.to_owned();
    }
    let rest = &cwd[home.len()..];
    if rest.is_empty() || rest.starts_with('/') {
        format!("~{rest}")
    } else {
        cwd.to_owned()
    }
}

/// What `rustyline` needs from us: the colored prompt, and Tab completion.
///
/// `readline` gets the plain prompt, because `rustyline` counts every byte of
/// the prompt it is given when it works out where the cursor goes. Feed it the
/// escape codes and the cursor lands far to the right. The highlighter is the
/// way in: `rustyline` measures the plain prompt and draws this one.
struct EditorHelper<'a> {
    /// The prompt with color, redrawn by the highlighter.
    colored: String,
    /// The remote home directory, for completing a leading `~/`.
    home: String,
    ch: &'a Channel,
    /// Records the command phase must skip, one per Tab the device never
    /// answered. See [`complete::query`].
    to_skip: &'a Cell<u32>,
    /// Separate from the one the command phase uses. Each phase reads whole
    /// records, so neither ever sees half of the other's.
    scanner: RefCell<Scanner>,
}

impl<'a> EditorHelper<'a> {
    fn new(ch: &'a Channel, to_skip: &'a Cell<u32>) -> Self {
        Self {
            colored: String::new(),
            home: String::new(),
            ch,
            to_skip,
            scanner: RefCell::new(Scanner::new()),
        }
    }
}

impl Completer for EditorHelper<'_> {
    type Candidate = String;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<String>)> {
        let start = complete::word_start(line, pos);
        let typed = complete::unescape(&line[start..pos]);

        // `~` never reaches the device: the glob is quoted, so the shell would
        // take it literally. Swap in the home directory we already know, then
        // fold it back on the way out.
        let tilde = !self.home.is_empty() && (typed == "~" || typed.starts_with("~/"));
        let prefix = if tilde {
            format!("{}{}", self.home, &typed[1..])
        } else {
            typed
        };

        let command = if complete::is_command_position(line, start) && !prefix.contains('/') {
            complete::command_query(&prefix)
        } else {
            complete::path_query(&prefix)
        };

        let Some(answer) = complete::query(self.ch, &mut self.scanner.borrow_mut(), &command)
        else {
            self.to_skip.set(self.to_skip.get() + 1);
            return Ok((start, Vec::new()));
        };

        let home = format!("{}/", self.home);
        let candidates = complete::parse(&answer)
            .into_iter()
            .map(|c| {
                let c = if tilde {
                    c.strip_prefix(&home)
                        .map_or(c.clone(), |r| format!("~/{r}"))
                } else {
                    c
                };
                complete::escape(&c)
            })
            .collect();
        Ok((start, candidates))
    }
}

impl Hinter for EditorHelper<'_> {
    type Hint = String;
}

impl Validator for EditorHelper<'_> {}

impl Highlighter for EditorHelper<'_> {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        _default: bool,
    ) -> Cow<'b, str> {
        if self.colored.is_empty() {
            Cow::Borrowed(prompt)
        } else {
            Cow::Borrowed(&self.colored)
        }
    }
}

impl Helper for EditorHelper<'_> {}

/// ANSI codes for the prompt, or nothing at all when colors are unwanted.
struct Palette {
    dim: &'static str,
    user: &'static str,
    cwd: &'static str,
    error: &'static str,
    root: &'static str,
    plain: &'static str,
    reset: &'static str,
}

impl Palette {
    /// No codes at all. Used for the plain copy of the prompt that `rustyline`
    /// measures, and when the user asked for no color.
    const NONE: Self = Self {
        dim: "",
        user: "",
        cwd: "",
        error: "",
        root: "",
        plain: "",
        reset: "",
    };

    fn pick() -> Self {
        if std::env::var_os("NO_COLOR").is_some() {
            return Self::NONE;
        }
        Self {
            dim: "\x1b[2m",
            user: "\x1b[36m",
            cwd: "\x1b[34m",
            error: "\x1b[31m",
            root: "\x1b[1;31m",
            plain: "\x1b[1;32m",
            reset: "\x1b[0m",
        }
    }
}

/// True for Ctrl+C.
fn is_interrupt(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c')
}

/// Turns a key into the bytes a remote program expects to read.
fn key_bytes(key: &KeyEvent) -> Option<Vec<u8>> {
    match key.code {
        KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let c = c.to_ascii_lowercase();
            c.is_ascii_lowercase()
                .then(|| vec![c as u8 - b'a' + 1])
                .or_else(|| match c {
                    ' ' | '@' => Some(vec![0]),
                    '[' => Some(vec![0x1b]),
                    '\\' => Some(vec![0x1c]),
                    ']' => Some(vec![0x1d]),
                    _ => None,
                })
        }
        KeyCode::Char(c) => {
            let mut buf = [0u8; 4];
            Some(c.encode_utf8(&mut buf).as_bytes().to_vec())
        }
        KeyCode::Enter => Some(vec![b'\n']),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Esc => Some(vec![0x1b]),
        _ => None,
    }
}

/// Shows locally what the user typed, since the remote does not echo.
fn echo<W: Write>(out: &mut W, bytes: &[u8]) -> Result<(), Error> {
    for &b in bytes {
        match b {
            b'\n' => out.write_all(b"\r\n")?,
            0x7f => out.write_all(b"\x08 \x08")?,
            b'\t' => out.write_all(b"\t")?,
            b if b < 0x20 => out.write_all(&[b'^', b + b'@'])?,
            b => out.write_all(&[b])?,
        }
    }
    out.flush()
}

/// Writes remote output while the terminal is in raw mode, where a bare `\n`
/// would drop a line without returning the cursor to column one.
fn write_raw<W: Write>(out: &mut W, bytes: &[u8]) -> Result<(), Error> {
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' && (i == 0 || bytes[i - 1] != b'\r') {
            out.write_all(&bytes[start..i])?;
            out.write_all(b"\r\n")?;
            start = i + 1;
        }
    }
    out.write_all(&bytes[start..])?;
    out.flush()
}

/// The status the remote shell ended with. A lost connection leaves none, and
/// that is not worth failing over in an interactive session.
fn exit_status(ch: &Channel) -> i32 {
    ch.get_exit_status().unwrap_or(0)
}

fn readline_error(e: ReadlineError) -> Error {
    match e {
        ReadlineError::Io(e) => e,
        e => Error::other(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{Palette, Report, is_incomplete, prompt, shorten, write_raw};

    #[test]
    fn spots_an_unfinished_line() {
        assert!(!is_incomplete("echo hi"));
        assert!(!is_incomplete("echo 'hi'"));
        assert!(!is_incomplete(r#"echo "a'b""#));
        assert!(!is_incomplete(r"echo a\'b"));
        assert!(is_incomplete("echo 'hi"));
        assert!(is_incomplete(r#"echo "hi"#));
        assert!(is_incomplete("echo hi \\"));
        assert!(!is_incomplete(r"echo '\'"));
    }

    #[test]
    fn shortens_the_home_directory() {
        assert_eq!(shorten("/home/root", "/home/root"), "~");
        assert_eq!(shorten("/home/root/x", "/home/root"), "~/x");
        assert_eq!(shorten("/home/rooted", "/home/root"), "/home/rooted");
        assert_eq!(shorten("/tmp", "/home/root"), "/tmp");
        assert_eq!(shorten("/tmp", ""), "/tmp");
        assert_eq!(shorten("/tmp", "/"), "/tmp");
    }

    #[test]
    fn draws_the_prompt() {
        let root = Report {
            status: 0,
            user: "root".into(),
            cwd: "/media/developer".into(),
            home: "/home/root".into(),
        };
        assert_eq!(
            prompt(&root, "tv", &Palette::NONE),
            "ares root@tv:/media/developer # "
        );
        let failed = Report {
            status: 1,
            user: "developer".into(),
            cwd: "/home/developer".into(),
            home: "/home/developer".into(),
        };
        assert_eq!(
            prompt(&failed, "tv", &Palette::NONE),
            "ares developer@tv:~ [1]$ "
        );
    }

    #[test]
    fn adds_carriage_returns_for_raw_mode() {
        let mut out = Vec::new();
        write_raw(&mut out, b"a\nb\r\nc\n").unwrap();
        assert_eq!(out, b"a\r\nb\r\nc\r\n");
    }
}
