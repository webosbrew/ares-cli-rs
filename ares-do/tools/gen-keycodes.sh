#!/bin/sh
# Generate src/keycode/table.rs from the kernel's input-event-codes.h.
#
#   ares-do/tools/gen-keycodes.sh > ares-do/src/keycode/table.rs
#
# Takes the header path as $1, defaulting to the system one. The output is
# committed, so the accepted key set never depends on the machine that builds
# the binary — release CI runs on macOS and Windows, where no such header
# exists.
set -eu

HEADER="${1:-/usr/include/linux/input-event-codes.h}"
[ -r "$HEADER" ] || { echo "cannot read $HEADER" >&2; exit 1; }

awk -v header="$HEADER" '
# Most defines carry a trailing /* ... */ comment, so strip comments first.
{ line = $0; sub(/\/\*.*/, "", line); sub(/[ \t]+$/, "", line) }

# KEY_FOO 123  /  KEY_FOO 0x7b
line ~ /^#define[ \t]+KEY_[A-Z0-9_]+[ \t]+(0x[0-9a-fA-F]+|[0-9]+)$/ {
    split(line, f, /[ \t]+/); num[f[2]] = strtonum(f[3]); next
}
# KEY_FOO KEY_BAR — one level of indirection, resolved below
line ~ /^#define[ \t]+KEY_[A-Z0-9_]+[ \t]+KEY_[A-Z0-9_]+$/ {
    split(line, f, /[ \t]+/); alias[f[2]] = f[3]; next
}
END {
    for (a in alias) if (alias[a] in num) num[a] = num[alias[a]]

    # Sentinels, not keys.
    delete num["KEY_RESERVED"]; delete num["KEY_MAX"]
    delete num["KEY_CNT"];      delete num["KEY_MIN_INTERESTING"]

    n = 0
    for (k in num) { short = substr(k, 5); code[short] = num[k]; names[++n] = short }
    sort_names(names, n)

    printf "//! Evdev key names, generated from `linux/input-event-codes.h`.\n"
    printf "//!\n"
    printf "//! Do not edit by hand. Regenerate with:\n"
    printf "//!\n"
    printf "//! ```sh\n"
    printf "//! ares-do/tools/gen-keycodes.sh > ares-do/src/keycode/table.rs\n"
    printf "//! ```\n"
    printf "//!\n"
    printf "//! Source: %s, %d names.\n", header, n
    printf "//!\n"
    printf "//! The `KEY_` prefix is stripped and entries are sorted by name, so\n"
    printf "//! [`slice::binary_search_by_key`] can find one.\n\n"
    printf "/// Every evdev key name the kernel knows, sorted, without the `KEY_` prefix.\n"
    printf "pub(super) static KEYCODES: &[(&str, u16)] = &[\n"
    for (i = 1; i <= n; i++) printf "    (%c%s%c, %d),\n", 34, names[i], 34, code[names[i]]
    printf "];\n"
}

# gawk has asort, mawk does not. Insertion sort keeps this portable.
function sort_names(a, len,   i, j, key) {
    for (i = 2; i <= len; i++) {
        key = a[i]; j = i - 1
        while (j > 0 && a[j] > key) { a[j + 1] = a[j]; j-- }
        a[j + 1] = key
    }
}
' "$HEADER"
