#!/bin/sh
# Generate src/keycode/remote.rs from a TV's xkb keycode table.
#
#   ares-do/tools/gen-remote-keys.sh /path/to/rootfs/usr/share/X11/xkb/keycodes/lg \
#       > ares-do/src/keycode/remote.rs
#
# That file is LG's own answer to "which code does this button send", and it is
# not always the code the mainline evdev name would suggest — the Back button
# sends 412, not KEY_BACK's 158. xkb keycodes are the evdev code plus 8.
#
# Only codes at or below KEY_MAX are emitted. sendKeyCode reaches
# UInputWriter::sendKeyPress, which writes to /dev/uinput, so anything above
# that cannot be delivered however it is named — which rules out the remote's
# own Home button (773) among others.
set -eu

FILE="${1:-}"
[ -n "$FILE" ] && [ -r "$FILE" ] || { echo "usage: $0 <xkb keycodes/lg>" >&2; exit 1; }

awk -v src="$FILE" '
# CamelCase -> UPPER_SNAKE, so Qt::Key_webOS_ChannelUp becomes CHANNEL_UP.
function snake(s,   out, i, c, prev) {
    out = ""; prev = ""
    for (i = 1; i <= length(s); i++) {
        c = substr(s, i, 1)
        if (c ~ /[A-Z]/ && out != "" && prev !~ /[A-Z]/) out = out "_"
        out = out toupper(c); prev = c
    }
    return out
}
function emit(name, code) {
    if (name == "" || (name in seen)) return
    seen[name] = code
    order[++n] = name
}
/^[ \t]*<[A-Z0-9_]+>[ \t]*=/ {
    tag = $0; sub(/^[ \t]*</, "", tag); sub(/>.*/, "", tag)
    val = $0; sub(/^[^=]*=[ \t]*/, "", val); sub(/[ \t]*;.*/, "", val)
    code = val + 0 - 8
    if (code < 0 || code > 767) next

    # The button, as LG names it: IR_KEY_CH_UP -> CH_UP, RF_KEY_VOICE -> VOICE.
    stem = tag
    sub(/^(IR|RF|DSC)_KEY_/, "", stem)
    sub(/^KEY_/, "", stem)
    emit("REMOTE_" stem, code)

    # What it means, as LG maps it: Qt::Key_webOS_ChannelUp -> CHANNEL_UP.
    if (match($0, /Qt::Key_webOS_[A-Za-z0-9]+/)) {
        emit("REMOTE_" snake(substr($0, RSTART + 14, RLENGTH - 14)), code)
    } else if (match($0, /Qt::Key_[A-Za-z0-9]+/)) {
        emit("REMOTE_" snake(substr($0, RSTART + 8, RLENGTH - 8)), code)
    }
}
END {
    # Insertion sort: mawk has no asort.
    for (i = 2; i <= n; i++) {
        key = order[i]; j = i - 1
        while (j > 0 && order[j] > key) { order[j + 1] = order[j]; j-- }
        order[j + 1] = key
    }
    printf "//! What each button on the remote actually sends, generated from a TV.\n"
    printf "//!\n"
    printf "//! Do not edit by hand. Regenerate with:\n"
    printf "//!\n"
    printf "//! ```sh\n"
    printf "//! ares-do/tools/gen-remote-keys.sh <rootfs>/usr/share/X11/xkb/keycodes/lg \\\\\n"
    printf "//!     > ares-do/src/keycode/remote.rs\n"
    printf "//! ```\n"
    printf "//!\n"
    printf "//! Source: %s, %d names.\n", src, n
    printf "//!\n"
    printf "//! Each button appears under the name LG gives it and, where they differ,\n"
    printf "//! under what LG says it means — so the Input button is both\n"
    printf "//! `REMOTE_TV_VIDEO` and `REMOTE_INPUT_SOURCE`. Sorted, for binary search.\n\n"
    printf "/// Remote buttons, by name, to the evdev code that button sends.\n"
    printf "pub(super) static REMOTE: &[(&str, u16)] = &[\n"
    for (i = 1; i <= n; i++) printf "    (%c%s%c, %d),\n", 34, order[i], 34, seen[order[i]]
    printf "];\n"
}
' "$FILE"
