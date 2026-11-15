//! Reading a flow file.
//!
//! The grammar is not invented here. A line is split into words, and those
//! words go to the same `clap` parser the command line uses — so anything you
//! can type, you can script, with the same flags and the same error messages,
//! and neither can drift from the other.
//!
//! ```text
//! # comments run to the end of the line
//! launch com.webos.app.home
//! wait 2s
//! key TAB                    # a web view starts with nothing focused
//! key TAB TAB ENTER --delay 300
//! text "hello world"
//! shot menu.png
//! ```

use std::fmt::{Display, Formatter};

/// One parsed line, kept with where it came from so a failure can point back.
#[derive(Debug)]
pub(crate) struct Step {
    pub line: usize,
    pub source: String,
    pub tokens: Vec<String>,
}

/// A line that did not parse.
#[derive(Debug)]
pub(crate) struct FlowError {
    pub origin: String,
    pub line: usize,
    pub source: String,
    pub message: String,
}

impl Display for FlowError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // Compiler style, so an editor or a caller can find the line.
        write!(
            f,
            "{}:{}: {}\n  {:>4} | {}",
            self.origin,
            self.line,
            self.message.trim_end(),
            self.line,
            self.source
        )
    }
}

/// Stop after this many, so a badly generated file reports something readable
/// instead of a thousand lines of the same mistake.
const MAX_ERRORS: usize = 20;

/// Split one line into words.
///
/// Whitespace separates; `"…"` and `'…'` group; `\` escapes the next character
/// anywhere; `#` starts a comment, but only where a word could start, so
/// `shot before#after.png` and `text "a # b"` both keep their hash.
///
/// # Errors
///
/// A quote that never closes, with the column it opened at.
pub(crate) fn tokenize(line: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut chars = line.char_indices().peekable();

    while let Some((column, ch)) = chars.next() {
        match ch {
            '\\' => {
                // A trailing backslash is a literal one; there is no line
                // continuation, because a flow line is a unit.
                match chars.next() {
                    Some((_, escaped)) => current.push(escaped),
                    None => current.push('\\'),
                }
                started = true;
            }
            '"' | '\'' => {
                let quote = ch;
                started = true;
                loop {
                    match chars.next() {
                        Some((_, c)) if c == quote => break,
                        // Only double quotes honour escapes, as in a shell.
                        Some((_, '\\')) if quote == '"' => match chars.next() {
                            Some((_, escaped)) => current.push(escaped),
                            None => return Err(unterminated(quote, column)),
                        },
                        Some((_, c)) => current.push(c),
                        None => return Err(unterminated(quote, column)),
                    }
                }
            }
            '#' if !started => break,
            c if c.is_whitespace() => {
                if started {
                    tokens.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            c => {
                current.push(c);
                started = true;
            }
        }
    }

    if started {
        tokens.push(current);
    }
    Ok(tokens)
}

fn unterminated(quote: char, column: usize) -> String {
    format!("unterminated {quote} quote opened at column {}", column + 1)
}

/// Split a whole flow into steps, collecting every error rather than stopping
/// at the first.
///
/// Reporting all of them at once matters: the caller fixes the file in one
/// pass instead of one round trip per mistake.
///
/// # Errors
///
/// Every line that failed to tokenize, capped at [`MAX_ERRORS`].
pub(crate) fn parse(origin: &str, input: &str) -> Result<Vec<Step>, Vec<FlowError>> {
    let mut steps = Vec::new();
    let mut errors = Vec::new();

    for (index, raw) in input.lines().enumerate() {
        let line = index + 1;
        // A flow authored on Windows, or saved with a BOM, should still run.
        let source = raw.trim_end_matches('\r');
        let source = if line == 1 {
            source.trim_start_matches('\u{feff}')
        } else {
            source
        };

        match tokenize(source) {
            Ok(tokens) if tokens.is_empty() => {}
            Ok(tokens) => steps.push(Step {
                line,
                source: source.to_string(),
                tokens,
            }),
            Err(message) if errors.len() < MAX_ERRORS => errors.push(FlowError {
                origin: origin.to_string(),
                line,
                source: source.to_string(),
                message,
            }),
            Err(_) => {}
        }
    }

    if errors.is_empty() {
        Ok(steps)
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::{parse, tokenize};

    fn words(line: &str) -> Vec<String> {
        tokenize(line).unwrap()
    }

    #[test]
    fn plain_words_split_on_whitespace() {
        assert_eq!(words("key TAB TAB ENTER"), ["key", "TAB", "TAB", "ENTER"]);
        assert_eq!(words("  key\tOK  "), ["key", "OK"]);
    }

    #[test]
    fn blank_and_comment_lines_are_not_steps() {
        let steps = parse("f", "\n# just a note\n   \nkey OK\n").unwrap();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].line, 4);
    }

    #[test]
    fn a_trailing_comment_is_dropped() {
        assert_eq!(words("key OK   # press it"), ["key", "OK"]);
    }

    #[test]
    fn a_hash_inside_a_word_or_a_quote_is_kept() {
        assert_eq!(words("shot a#b.png"), ["shot", "a#b.png"]);
        assert_eq!(words(r#"text "a # b""#), ["text", "a # b"]);
    }

    #[test]
    fn quotes_group_and_disappear() {
        assert_eq!(words(r#"text "hello world""#), ["text", "hello world"]);
        assert_eq!(words("text 'hello world'"), ["text", "hello world"]);
        assert_eq!(words(r#"text """#), ["text", ""]);
    }

    #[test]
    fn double_quotes_honour_escapes_and_single_quotes_do_not() {
        assert_eq!(
            words(r#"text "he said \"hi\"""#),
            ["text", r#"he said "hi""#]
        );
        assert_eq!(words(r"text 'a\b'"), ["text", r"a\b"]);
    }

    #[test]
    fn a_backslash_escapes_outside_quotes_too() {
        assert_eq!(words(r"text a\ b"), ["text", "a b"]);
        assert_eq!(words(r"text \#notacomment"), ["text", "#notacomment"]);
    }

    #[test]
    fn an_unterminated_quote_says_where_it_opened() {
        let message = tokenize(r#"text "demo"#).unwrap_err();
        assert!(message.contains("unterminated"), "{message}");
        assert!(message.contains("column 6"), "{message}");
    }

    #[test]
    fn crlf_and_a_bom_do_not_leak_into_tokens() {
        let steps = parse("f", "\u{feff}key OK\r\nkey TAB\r\n").unwrap();
        assert_eq!(steps[0].tokens, ["key", "OK"]);
        assert_eq!(steps[1].tokens, ["key", "TAB"]);
    }

    #[test]
    fn every_bad_line_is_reported_with_its_number() {
        let errors = parse("flow.txt", "key OK\ntext \"one\ntext 'two\n").unwrap_err();
        assert_eq!(errors.len(), 2);
        assert_eq!(errors[0].line, 2);
        assert_eq!(errors[1].line, 3);
        let rendered = errors[0].to_string();
        assert!(rendered.starts_with("flow.txt:2:"), "{rendered}");
        assert!(rendered.contains("text \"one"), "{rendered}");
    }

    #[test]
    fn a_flood_of_errors_is_capped() {
        let input = "text \"x\n".repeat(100);
        assert_eq!(parse("f", &input).unwrap_err().len(), 20);
    }
}
