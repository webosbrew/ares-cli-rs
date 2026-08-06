//! State reports the remote shell prints between commands.
//!
//! Without a pseudo-terminal the remote shell runs non-interactive: it prints no
//! prompt and tells us nothing about itself. To draw a local prompt we send our
//! own reporting command after every user command. It prints a fenced record that
//! carries the exit status, the current user, the working directory and the home
//! directory. The fence also tells us exactly when the command finished, so the
//! prompt never lands in the middle of the output.

/// Bytes that open a record. `\x01` (SOH) and `\x02` (STX) are control codes no
/// shell prints on purpose, and the `ares` tag keeps stray `\x01` bytes in binary
/// output from being mistaken for a record.
const PREFIX: &[u8] = b"\x01ares\x01";
/// Separates the fields inside a record.
const SEP: u8 = 0x01;
/// Closes a record.
const END: u8 = 0x02;
/// Longest record body we accept. Anything longer is binary output that happened
/// to start with our prefix, so we give up and print it.
const MAX_BODY: usize = 4096;

/// The command that prints one record. Sent on its own line after every user
/// command.
///
/// `$?` goes into a variable first, so the command substitutions that follow
/// cannot overwrite it.
pub(crate) const REPORT_CMD: &str = concat!(
    "__ares_st=$?; ",
    r#"printf '\001ares\001%s\001%s\001%s\001%s\002' "$__ares_st" "$(id -un)" "$PWD" "$HOME""#,
    "\n"
);

/// What the remote shell reported after the last command.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Report {
    pub status: i32,
    pub user: String,
    pub cwd: String,
    pub home: String,
}

impl Report {
    fn parse(body: &[u8]) -> Self {
        let mut fields = body.split(|&b| b == SEP);
        let mut next = || {
            fields
                .next()
                .map(|f| String::from_utf8_lossy(f).into_owned())
                .unwrap_or_default()
        };
        let status = next().trim().parse().unwrap_or(0);
        Self {
            status,
            user: next(),
            cwd: next(),
            home: next(),
        }
    }
}

enum State {
    /// Everything is plain output.
    Normal,
    /// The last bytes matched this many bytes of [`PREFIX`].
    Matching(usize),
    /// Inside a record, collecting the body.
    Body,
}

/// Pulls records out of a byte stream and passes everything else through.
pub(crate) struct Scanner {
    state: State,
    body: Vec<u8>,
}

impl Scanner {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Normal,
            body: Vec::new(),
        }
    }

    /// Feeds one chunk of remote output. Plain output goes to `out`. Returns
    /// every record the chunk completed, in order. Normally none or one, but a
    /// timed-out Tab query can leave a record to arrive beside the next one.
    pub(crate) fn push(&mut self, chunk: &[u8], out: &mut Vec<u8>) -> Vec<Report> {
        let mut found = Vec::new();
        let mut i = 0;
        while i < chunk.len() {
            let b = chunk[i];
            match self.state {
                State::Normal => {
                    if b == PREFIX[0] {
                        self.state = State::Matching(1);
                    } else {
                        out.push(b);
                    }
                    i += 1;
                }
                State::Matching(n) => {
                    if b != PREFIX[n] {
                        // Not a record after all. Print what we held back and
                        // look at this byte again from the start: it may itself
                        // open a record.
                        out.extend_from_slice(&PREFIX[..n]);
                        self.state = State::Normal;
                        continue;
                    }
                    self.state = if n + 1 == PREFIX.len() {
                        self.body.clear();
                        State::Body
                    } else {
                        State::Matching(n + 1)
                    };
                    i += 1;
                }
                State::Body => {
                    if b == END {
                        found.push(Report::parse(&self.body));
                        self.body.clear();
                        self.state = State::Normal;
                    } else if self.body.len() >= MAX_BODY {
                        out.extend_from_slice(PREFIX);
                        out.append(&mut self.body);
                        self.state = State::Normal;
                        continue;
                    } else {
                        self.body.push(b);
                    }
                    i += 1;
                }
            }
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_BODY, Report, Scanner};

    fn feed(chunks: &[&[u8]]) -> (Vec<u8>, Option<Report>) {
        let mut scanner = Scanner::new();
        let mut out = Vec::new();
        let mut found = Vec::new();
        for chunk in chunks {
            found.extend(scanner.push(chunk, &mut out));
        }
        assert!(found.len() <= 1, "these cases hold at most one record");
        (out, found.pop())
    }

    #[test]
    fn reads_two_records_from_one_chunk() {
        let mut scanner = Scanner::new();
        let mut out = Vec::new();
        let found = scanner.push(
            b"\x01ares\x010\x01a\x01/1\x01/h\x02x\x01ares\x017\x01b\x01/2\x01/h\x02",
            &mut out,
        );
        assert_eq!(out, b"x");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].user, "a");
        assert_eq!(found[1].status, 7);
    }

    #[test]
    fn reads_a_whole_record() {
        let (out, report) = feed(&[b"hello\n\x01ares\x010\x01root\x01/tmp\x01/home/root\x02"]);
        assert_eq!(out, b"hello\n");
        assert_eq!(
            report,
            Some(Report {
                status: 0,
                user: "root".into(),
                cwd: "/tmp".into(),
                home: "/home/root".into(),
            })
        );
    }

    #[test]
    fn reads_a_record_split_across_chunks() {
        let (out, report) = feed(&[b"hi\x01a", b"res\x0113\x01dev", b"eloper\x01/x\x01/y\x02!"]);
        assert_eq!(out, b"hi!");
        assert_eq!(
            report,
            Some(Report {
                status: 13,
                user: "developer".into(),
                cwd: "/x".into(),
                home: "/y".into(),
            })
        );
    }

    #[test]
    fn passes_through_a_stray_prefix_byte() {
        let (out, report) = feed(&[b"a\x01b\x01\x01are\x01c"]);
        assert_eq!(out, b"a\x01b\x01\x01are\x01c");
        assert_eq!(report, None);
    }

    #[test]
    fn gives_up_on_an_overlong_body() {
        let body = vec![b'x'; MAX_BODY + 8];
        let mut chunk = b"\x01ares\x01".to_vec();
        chunk.extend_from_slice(&body);
        let (out, report) = feed(&[&chunk]);
        assert_eq!(out.len(), chunk.len());
        assert_eq!(report, None);
    }

    #[test]
    fn fills_in_missing_fields() {
        let (_, report) = feed(&[b"\x01ares\x01\x02"]);
        assert_eq!(report, Some(Report::default()));
    }
}
