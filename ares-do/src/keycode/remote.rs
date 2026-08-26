//! What each button on the remote actually sends, generated from a TV.
//!
//! Do not edit by hand. Regenerate with:
//!
//! ```sh
//! ares-do/tools/gen-remote-keys.sh <rootfs>/usr/share/X11/xkb/keycodes/lg \\
//!     > ares-do/src/keycode/remote.rs
//! ```
//!
//! Source: /home/mariotaku/Projects/webos-firmwares/05.70.50.01-HE_DTV_W16R_AFAAABAA/rootfs/usr/share/X11/xkb/keycodes/lg, 44 names.
//!
//! Each button appears under the name LG gives it and, where they differ,
//! under what LG says it means — so the Input button is both
//! `REMOTE_TV_VIDEO` and `REMOTE_INPUT_SOURCE`. Sorted, for binary search.

/// Remote buttons, by name, to the evdev code that button sends.
pub(super) static REMOTE: &[(&str, u16)] = &[
    ("REMOTE_AUDIO_FORWARD", 208),
    ("REMOTE_AUDIO_REWIND", 168),
    ("REMOTE_BACK", 412),
    ("REMOTE_BLUE", 401),
    ("REMOTE_CHANNEL_DOWN", 403),
    ("REMOTE_CHANNEL_UP", 402),
    ("REMOTE_CH_DOWN", 403),
    ("REMOTE_CH_UP", 402),
    ("REMOTE_EXIT", 174),
    ("REMOTE_FAVORITE", 364),
    ("REMOTE_FF", 208),
    ("REMOTE_GREEN", 399),
    ("REMOTE_GUIDE", 362),
    ("REMOTE_INFO", 358),
    ("REMOTE_INPUT_SOURCE", 241),
    ("REMOTE_INPUT_TV", 377),
    ("REMOTE_MEDIA_PAUSE", 119),
    ("REMOTE_MEDIA_PLAY", 207),
    ("REMOTE_MEDIA_RECORD", 167),
    ("REMOTE_MEDIA_STOP", 128),
    ("REMOTE_MENU", 139),
    ("REMOTE_MUTE", 113),
    ("REMOTE_PAUSE", 119),
    ("REMOTE_PLAY", 207),
    ("REMOTE_POWER", 116),
    ("REMOTE_POWER_ON_OFF", 116),
    ("REMOTE_REC", 167),
    ("REMOTE_RECLIST", 144),
    ("REMOTE_RECORD_LIST", 144),
    ("REMOTE_RED", 398),
    ("REMOTE_REW", 168),
    ("REMOTE_SEARCH", 217),
    ("REMOTE_SETTINGS", 139),
    ("REMOTE_STOP", 128),
    ("REMOTE_TV", 377),
    ("REMOTE_TVGUIDE", 362),
    ("REMOTE_TV_VIDEO", 241),
    ("REMOTE_VOICE", 428),
    ("REMOTE_VOLUME_DOWN", 114),
    ("REMOTE_VOLUME_MUTE", 113),
    ("REMOTE_VOLUME_UP", 115),
    ("REMOTE_VOL_DOWN", 114),
    ("REMOTE_VOL_UP", 115),
    ("REMOTE_YELLOW", 400),
];
