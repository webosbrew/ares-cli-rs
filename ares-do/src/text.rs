//! Typing a string, one key at a time.
//!
//! # Why there is no Shift
//!
//! `sendKeyCode` takes one code per call and injects a complete press. There
//! is no held state between calls, so sending `LEFTSHIFT` and then `A` sends
//! two independent presses — the shift is long over by the time the second
//! call crosses the network. Capitals and shifted symbols are therefore not
//! expressible, and pretending otherwise with a `--shift` flag that quietly
//! does nothing would be worse than refusing.
//!
//! So this refuses, up front, naming every character it cannot type. The check
//! runs before a single key is sent, which makes the command all-or-nothing
//! rather than leaving half a password in a field.
//!
//! What the app *receives* is still its own business: a key code becomes a
//! character through whatever keymap the focused window uses. This sends keys.

use std::fmt::Write as _;
use std::thread::sleep;
use std::time::Duration;

use libssh_rs::Session;
use serde_json::json;

use crate::error::DoError;
use crate::key::SendKey;
use crate::keycode::{self, Key};
use crate::output::{Reporter, Timer};

/// Characters with a key of their own, and the key's name.
///
/// Letters and digits are handled in code; this is everything else that can be
/// typed without a modifier on a US layout.
static PUNCTUATION: &[(char, &str)] = &[
    (' ', "SPACE"),
    ('\n', "ENTER"),
    ('\t', "TAB"),
    ('-', "MINUS"),
    ('=', "EQUAL"),
    ('[', "LEFTBRACE"),
    (']', "RIGHTBRACE"),
    ('\\', "BACKSLASH"),
    (';', "SEMICOLON"),
    ('\'', "APOSTROPHE"),
    (',', "COMMA"),
    ('.', "DOT"),
    ('/', "SLASH"),
    ('`', "GRAVE"),
];

/// Shifted characters, and the unshifted key they sit on.
///
/// Only used to explain the refusal — knowing that `!` lives on `1` makes the
/// error actionable instead of a flat no.
static SHIFTED: &[(char, char)] = &[
    ('!', '1'),
    ('@', '2'),
    ('#', '3'),
    ('$', '4'),
    ('%', '5'),
    ('^', '6'),
    ('&', '7'),
    ('*', '8'),
    ('(', '9'),
    (')', '0'),
    ('_', '-'),
    ('+', '='),
    ('{', '['),
    ('}', ']'),
    ('|', '\\'),
    (':', ';'),
    ('"', '\''),
    ('<', ','),
    ('>', '.'),
    ('?', '/'),
    ('~', '`'),
];

/// Why one character could not be typed.
struct Rejected {
    index: usize,
    reason: String,
}

fn key_for(ch: char) -> Option<Key> {
    let name = if ch.is_ascii_lowercase() {
        ch.to_ascii_uppercase().to_string()
    } else if ch.is_ascii_digit() {
        ch.to_string()
    } else {
        PUNCTUATION
            .iter()
            .find(|(c, _)| *c == ch)
            .map(|(_, name)| (*name).to_string())?
    };
    keycode::by_name(&name)
}

/// Turn a string into the keys that type it.
///
/// With `lower`, `A-Z` is folded to lower case first; shifted symbols are
/// still refused, because folding them would change what was asked for.
///
/// # Errors
///
/// [`DoError::Usage`] listing every character that cannot be typed, so one
/// round trip is enough to fix the whole string.
pub(crate) fn text_to_keys(text: &str, lower: bool) -> Result<Vec<Key>, DoError> {
    let mut keys = Vec::new();
    let mut rejected: Vec<Rejected> = Vec::new();

    // char_indices, not bytes: a rejected 'é' must not report a byte offset
    // that lands mid-character in the message.
    for (index, ch) in text.chars().enumerate() {
        let ch = if lower { ch.to_ascii_lowercase() } else { ch };

        if let Some(key) = key_for(ch) {
            keys.push(key);
            continue;
        }

        let reason = if ch.is_ascii_uppercase() {
            format!(
                "'{ch}' needs Shift, which sendKeyCode cannot express (pass --lower to send \
                 '{}')",
                ch.to_ascii_lowercase()
            )
        } else if let Some((_, base)) = SHIFTED.iter().find(|(c, _)| *c == ch) {
            format!("'{ch}' needs Shift (it sits on '{base}'), which sendKeyCode cannot express")
        } else if ch.is_ascii() {
            format!("'{}' has no key of its own", ch.escape_debug())
        } else {
            format!("'{ch}' is not ASCII; sendKeyCode carries key codes, not characters")
        };
        rejected.push(Rejected { index, reason });
    }

    if rejected.is_empty() {
        return Ok(keys);
    }

    let plural = if rejected.len() == 1 {
        "character"
    } else {
        "characters"
    };
    let mut message = format!(
        "text: {} {plural} cannot be typed with sendKeyCode:",
        rejected.len()
    );
    for Rejected { index, reason } in &rejected {
        let _ = write!(message, "\n  index {index}: {reason}");
    }
    Err(DoError::Usage(message))
}

pub(crate) trait TypeText {
    /// # Errors
    ///
    /// [`DoError::Usage`] before anything is sent when the string cannot be
    /// typed, otherwise whatever sending a key failed with.
    fn type_text(
        &self,
        text: &str,
        lower: bool,
        delay: Duration,
        timeout: Option<Duration>,
        reporter: &Reporter,
    ) -> Result<(), DoError>;
}

impl TypeText for Session {
    fn type_text(
        &self,
        text: &str,
        lower: bool,
        delay: Duration,
        timeout: Option<Duration>,
        reporter: &Reporter,
    ) -> Result<(), DoError> {
        let keys = text_to_keys(text, lower)?;
        let timer = Timer::start();
        for (i, key) in keys.iter().enumerate() {
            if i > 0 && !delay.is_zero() {
                sleep(delay);
            }
            self.send_key(*key, timeout, reporter)?;
        }
        reporter.info(&format!("Typed {text:?}"));
        reporter.event(&json!({
            "event": "text",
            "ok": true,
            "text": text,
            "keys": keys.iter().map(|k| json!({"name": k.name, "code": k.code}))
                .collect::<Vec<_>>(),
            "ms": timer.ms(),
        }));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::text_to_keys;

    fn codes(text: &str, lower: bool) -> Vec<u16> {
        text_to_keys(text, lower)
            .unwrap()
            .into_iter()
            .map(|k| k.code)
            .collect()
    }

    #[test]
    fn lowercase_digits_and_punctuation_all_type() {
        assert_eq!(codes("a", false), vec![30]);
        assert_eq!(codes("z", false), vec![44]);
        assert_eq!(codes("0", false), vec![11]);
        assert_eq!(codes("9", false), vec![10]);
        assert_eq!(codes(" ", false), vec![57]);
        assert_eq!(codes("\n", false), vec![28]);
        assert_eq!(codes("\t", false), vec![15]);
        assert_eq!(codes("-.,/", false), vec![12, 52, 51, 53]);
    }

    #[test]
    fn a_whole_lowercase_string_types() {
        assert_eq!(codes("demo", false).len(), 4);
        assert_eq!(codes("demo 42", false).len(), 7);
    }

    #[test]
    fn capitals_are_refused_with_a_way_out() {
        let message = text_to_keys("Demo", false).unwrap_err().to_string();
        assert!(message.contains("index 0"), "{message}");
        assert!(message.contains("--lower"), "{message}");
    }

    #[test]
    fn lower_folds_capitals_but_not_symbols() {
        assert_eq!(codes("Demo", true), codes("demo", false));
        assert!(text_to_keys("demo!", true).is_err());
    }

    #[test]
    fn every_bad_character_is_reported_at_once() {
        let message = text_to_keys("A!B", false).unwrap_err().to_string();
        assert!(message.contains("3 characters"), "{message}");
        assert!(message.contains("index 0"), "{message}");
        assert!(message.contains("index 1"), "{message}");
        assert!(message.contains("index 2"), "{message}");
    }

    #[test]
    fn a_shifted_symbol_names_the_key_it_sits_on() {
        let message = text_to_keys("!", false).unwrap_err().to_string();
        assert!(message.contains("sits on '1'"), "{message}");
    }

    #[test]
    fn non_ascii_is_counted_by_characters_not_bytes() {
        // 'é' is two bytes; the index must still be 1, not 2.
        let message = text_to_keys("aéb", false).unwrap_err().to_string();
        assert!(message.contains("index 1"), "{message}");
        assert!(message.contains("not ASCII"), "{message}");
    }

    #[test]
    fn nothing_is_sent_when_anything_is_unsendable() {
        assert!(text_to_keys("good then B", false).is_err());
    }
}
