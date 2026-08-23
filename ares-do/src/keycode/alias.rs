//! The hand-written layer over the generated [`super::table`]: friendly names
//! for the keys a TV remote actually has, the subset worth listing by default,
//! and the gotchas worth warning about.
//!
//! # Where the LG mappings come from
//!
//! `sendKeyCode` ends up in `UInputWriter::sendKeyPress`, which writes to
//! `/dev/uinput` — so the codes are ordinary evdev, capped at
//! [`super::KEY_MAX`]. Which evdev code each remote button produces is LG's
//! choice, and it is not always the mainline name you would guess. The
//! authority is `/usr/share/X11/xkb/keycodes/lg` in the firmware (xkb keycodes
//! are the evdev code plus 8):
//!
//! That table is now generated into [`super::remote`] as `REMOTE_*` names, so
//! the Back button is `REMOTE_BACK` (412) and the kernel's `BACK` stays 158.
//! What is left here is the small hand-written layer: short spellings people
//! reach for, the curated listing order, and the caveats.

/// Friendly name -> canonical name in [`super::table::KEYCODES`].
///
/// Mostly for names the kernel does not define. An alias may shadow a real
/// key when the remote genuinely sends something else — `OK` does — but never
/// silently: [`super::tests`] requires a [`NOTES`] entry for any that do, and
/// `ares-do keys` prints it.
///
/// Where the remote's button and a kernel key merely share a name, the alias
/// gets a different one instead. That is why LG's Back is `GOBACK` and not
/// `BACK`: `BACK` (158) is a real key, just not the one this remote sends.
pub(super) static ALIASES: &[(&str, &str)] = &[
    ("CHDOWN", "CHANNELDOWN"),
    ("CHUP", "CHANNELUP"),
    ("DASH", "MINUS"),
    ("ESCAPE", "ESC"),
    ("FF", "FASTFORWARD"),
    ("LAUNCHER", "LEFTMETA"),
    ("META", "LEFTMETA"),
    ("OK", "ENTER"),
    ("PERIOD", "DOT"),
    ("QUIT", "EXIT"),
    ("RETURN", "ENTER"),
    ("REW", "REWIND"),
    ("SUPER", "LEFTMETA"),
    ("VOLDOWN", "VOLUMEDOWN"),
    ("VOLUP", "VOLUMEUP"),
];

/// What `ares-do keys` shows without `--all`: the buttons a remote has, in the
/// order they sit on one, rather than 512 names sorted alphabetically.
pub(super) static TV_KEYS: &[&str] = &[
    // D-pad and the two ways out.
    "UP",
    "DOWN",
    "LEFT",
    "RIGHT",
    "ENTER",
    "PREVIOUS",
    "EXIT",
    // Shells.
    "HOMEPAGE",
    "LEFTMETA",
    "MENU",
    "INFO",
    "PROGRAM",
    "SEARCH",
    // Focus, for web views that start with nothing focused.
    "TAB",
    "ESC",
    // Hardware.
    "POWER",
    "VOLUMEUP",
    "VOLUMEDOWN",
    "MUTE",
    "CHANNELUP",
    "CHANNELDOWN",
    "TV",
    "VIDEO_NEXT",
    // Transport.
    "PLAY",
    "PAUSE",
    "PLAYPAUSE",
    "STOP",
    "REWIND",
    "FASTFORWARD",
    "RECORD",
    // Coloured buttons.
    "RED",
    "GREEN",
    "YELLOW",
    "BLUE",
    // Digits and text entry.
    "0",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
    "SPACE",
    "BACKSPACE",
];

/// Field-tested caveats, shown next to the key by `ares-do keys`.
///
/// A key here still resolves and still gets sent — the note only says what was
/// observed on real hardware.
pub(super) static NOTES: &[(&str, &str)] = &[
    (
        "OK",
        "sends ENTER (28), which is what LG's OK button produces. The kernel's \
         own KEY_OK is 352 — pass 352 if that is what you want",
    ),
    (
        "HOME",
        "the keyboard Home key, not the remote's — that is HOMEPAGE (172)",
    ),
    (
        "BACK",
        "not the Back button on a remote — that is REMOTE_BACK (412). 158 is XF86Back, \
         and is swallowed before it reaches any window",
    ),
    (
        "PREVIOUS",
        "what the Back button sends; spelled REMOTE_BACK",
    ),
    (
        "LEFTMETA",
        "opens the launcher; aliased SUPER, META and LAUNCHER",
    ),
    (
        "ENTER",
        "does nothing in a web view until TAB has established focus",
    ),
];
