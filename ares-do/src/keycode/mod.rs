//! Turning what a person or an agent typed into an evdev key code.
//!
//! Three shapes are accepted for the same argument, because all three turn up
//! in practice: a friendly name (`OK`), a canonical kernel name (`KEY_ENTER`),
//! and a raw code (`28`, `0x1c`) for keys this table has never heard of.

use std::fmt::{Display, Formatter};

mod alias;
mod remote;
mod table;

#[cfg(test)]
mod tests;

/// `KEY_MAX` from `linux/input-event-codes.h`. Codes above this are not keys.
pub(crate) const KEY_MAX: u16 = 0x2ff;

/// How many suggestions an unknown name is worth. Enough to be useful, few
/// enough that an agent re-reading the error is not swamped.
const MAX_SUGGESTIONS: usize = 5;

/// A resolved key: always a code, plus the canonical name when it came from
/// one. A raw code has no name, and that is not an error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Key {
    pub code: u16,
    pub name: Option<&'static str>,
}

impl Display for Key {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.name {
            Some(name) => write!(f, "{name} ({})", self.code),
            None => write!(f, "{}", self.code),
        }
    }
}

/// A remote button, by the name LG gives it.
///
/// Checked before the kernel's table, because the two disagree: the Back
/// button sends 412, while `KEY_BACK` — the name that looks right, and
/// `XF86Back` in xkb — is 158 and is swallowed before any window sees it.
fn remote(name: &str) -> Option<u16> {
    remote::REMOTE
        .binary_search_by_key(&name, |(n, _)| *n)
        .ok()
        .map(|i| remote::REMOTE[i].1)
}

/// Look up one canonical name, after aliases have been resolved.
fn lookup(name: &str) -> Option<&'static (&'static str, u16)> {
    table::KEYCODES
        .binary_search_by_key(&name, |(n, _)| *n)
        .ok()
        .map(|i| &table::KEYCODES[i])
}

/// Resolve an alias, or hand back what came in.
fn unalias(name: &str) -> &str {
    alias::ALIASES
        .iter()
        .find(|(from, _)| *from == name)
        .map_or(name, |(_, to)| *to)
}

/// Parse a raw code, decimal or `0x`-prefixed. `None` when it is not a number
/// at all — an out-of-range number is a different answer from a name.
fn parse_code(value: &str) -> Option<Result<u16, String>> {
    let (digits, radix) = match value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        Some(hex) => (hex, 16),
        None => (value, 10),
    };
    if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
        return None;
    }
    Some(match u32::from_str_radix(digits, radix) {
        Ok(code) if code <= u32::from(KEY_MAX) => Ok(u16::try_from(code).unwrap_or(KEY_MAX)),
        _ => Err(format!(
            "key code {value} is out of range; evdev codes run from 0 to {KEY_MAX}"
        )),
    })
}

/// Edit distance, capped: anything further than `max` is just "far".
///
/// Two rows rather than a full matrix — the strings are short and this runs
/// over every name in the table.
fn distance_within(a: &str, b: &str, max: usize) -> Option<usize> {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len().abs_diff(b.len()) > max {
        return None;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];
    for (i, &ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            curr[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(curr[j] + 1);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    (prev[b.len()] <= max).then_some(prev[b.len()])
}

/// Every name this tool knows, aliases first so a friendly name wins.
fn all_names() -> impl Iterator<Item = &'static str> {
    remote::REMOTE
        .iter()
        .map(|(name, _)| *name)
        .chain(alias::ALIASES.iter().map(|(from, _)| *from))
        .chain(table::KEYCODES.iter().map(|(name, _)| *name))
}

/// Names close enough to `wanted` to be worth offering back.
///
/// Tried in order of how likely each is to be what was meant: an unfinished
/// name, an overshot one (`OKAY` for `OK`), a typo, then anything containing
/// it. A caller recovering from a bad guess reads only this, so the ordering
/// is worth getting right.
fn suggestions(wanted: &str) -> Vec<&'static str> {
    let mut found: Vec<&'static str> = Vec::new();
    let mut add = |name: &'static str, found: &mut Vec<&'static str>| {
        if found.len() < MAX_SUGGESTIONS && !found.contains(&name) {
            found.push(name);
        }
    };

    // Typed a prefix of the real name: ENTE -> ENTER.
    for name in all_names() {
        add_if(name.starts_with(wanted), name, &mut found, &mut add);
    }
    // Typed past the real name: OKAY -> OK. Two characters minimum, or every
    // long input matches half the table.
    for name in all_names() {
        add_if(
            name.len() >= 2 && wanted.starts_with(name),
            name,
            &mut found,
            &mut add,
        );
    }
    // Ordinary typo: ENTR -> ENTER.
    if found.len() < MAX_SUGGESTIONS && wanted.len() >= 3 {
        let mut near: Vec<(usize, &'static str)> = all_names()
            .filter_map(|name| distance_within(wanted, name, 2).map(|d| (d, name)))
            .collect();
        near.sort_unstable();
        for (_, name) in near {
            add(name, &mut found);
        }
    }
    // Last resort: the name is in there somewhere.
    if found.len() < MAX_SUGGESTIONS && wanted.len() >= 2 {
        for name in all_names() {
            add_if(name.contains(wanted), name, &mut found, &mut add);
        }
    }
    found
}

fn add_if(
    condition: bool,
    name: &'static str,
    found: &mut Vec<&'static str>,
    add: &mut impl FnMut(&'static str, &mut Vec<&'static str>),
) {
    if condition {
        add(name, found);
    }
}

/// Resolve a name, never a number.
///
/// [`parse`] reads `0` as raw code zero, which is what someone typing
/// `ares-do key 0` means. Someone typing `ares-do text 0` means the digit, so
/// character lookup has to skip the numeric path entirely.
pub(crate) fn by_name(name: &str) -> Option<Key> {
    let upper = name.to_ascii_uppercase();
    if let Some(code) = remote(&upper) {
        return Some(Key { code, name: None });
    }
    let bare = upper.strip_prefix("KEY_").unwrap_or(&upper);
    lookup(unalias(bare)).map(|(name, code)| Key {
        code: *code,
        name: Some(name),
    })
}

/// The `clap` value parser behind every `KEY` argument.
///
/// # Errors
///
/// Returns a message naming what was wrong and, for an unrecognised name, what
/// to try instead.
pub(crate) fn parse(value: &str) -> Result<Key, String> {
    if let Some(code) = parse_code(value) {
        return code.map(|code| Key { code, name: None });
    }

    let upper = value.to_ascii_uppercase();

    // A REMOTE_ name is the button, not the kernel's key of that name.
    if let Some(code) = remote(&upper) {
        return Ok(Key {
            code,
            name: Some(
                remote::REMOTE[remote::REMOTE
                    .binary_search_by_key(&upper.as_str(), |(n, _)| *n)
                    .unwrap()]
                .0,
            ),
        });
    }

    // KEY_ and XF86 both say "the kernel's key of this name, and I mean it",
    // so they are never ambiguous. A bare name might be.
    let explicit = upper.starts_with("KEY_") || upper.starts_with("XF86");
    let bare = upper
        .strip_prefix("KEY_")
        .or_else(|| upper.strip_prefix("XF86"))
        .unwrap_or(&upper);

    if !explicit && let Some((name, why)) = alias::AMBIGUOUS.iter().find(|(n, _)| *n == bare) {
        return Err(format!("\"{name}\" is ambiguous: {why}"));
    }

    let canonical = unalias(bare);

    if let Some((name, code)) = lookup(canonical) {
        return Ok(Key {
            code: *code,
            name: Some(name),
        });
    }

    let close = suggestions(bare);
    let mut message = if close.is_empty() {
        format!("unknown key \"{value}\"")
    } else {
        format!(
            "unknown key \"{value}\"; did you mean {}?",
            close.join(", ")
        )
    };
    message.push_str(
        "\n  Run `ares-do keys` for the names a remote has, `ares-do keys --all` for every \
         name, or pass a raw evdev code from 0 to 767.",
    );
    Err(message)
}

/// One row of `ares-do keys`.
pub(crate) struct Listed {
    pub name: &'static str,
    pub code: u16,
    pub aliases: Vec<&'static str>,
    pub note: Option<&'static str>,
}

/// The rows `ares-do keys` prints.
///
/// Without `all`, the curated remote-control set in remote order. With it,
/// every name the kernel defines, sorted. `pattern` filters either, case
/// insensitively, against the name and its aliases.
pub(crate) fn list(all: bool, pattern: Option<&str>) -> Vec<Listed> {
    let wanted = pattern.map(str::to_ascii_uppercase);
    let names: Vec<&'static str> = if all {
        table::KEYCODES.iter().map(|(name, _)| *name).collect()
    } else {
        alias::TV_KEYS.to_vec()
    };

    names
        .into_iter()
        .filter_map(|name| {
            let (name, code) = *lookup(name)?;
            let mut aliases: Vec<&'static str> = alias::ALIASES
                .iter()
                .filter(|(_, to)| *to == name)
                .map(|(from, _)| *from)
                .collect();
            // A REMOTE_ name belongs to whichever key carries its code.
            aliases.extend(
                remote::REMOTE
                    .iter()
                    .filter(|(_, c)| *c == code)
                    .map(|(n, _)| *n),
            );
            let note = alias::NOTES
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, note)| *note);
            let matches = wanted.as_ref().is_none_or(|want| {
                name.contains(want.as_str()) || aliases.iter().any(|a| a.contains(want.as_str()))
            });
            matches.then_some(Listed {
                name,
                code,
                aliases,
                note,
            })
        })
        .collect()
}
