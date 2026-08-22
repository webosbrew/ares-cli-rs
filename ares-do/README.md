# ares-do

Send key events and take screenshots on a webOS device.

Part of [ares-cli-rs](https://github.com/webosbrew/ares-cli-rs), a Rust rewrite of
[@webosose/ares-cli](https://github.com/webosose/ares-cli). See the repository
README for install steps.

`ares-do` drives the TV the way a remote control does, and captures what
happens, so a UI flow can be scripted and replayed instead of walked by hand.

**It needs a root session.** Both services it uses are on the private luna bus.
A dev-mode (`prisoner`) device cannot use them, and `ares-do` says so before it
sends anything.

```text
Send key events and take screenshots on a webOS device

Usage: ares-do [OPTIONS] <COMMAND>

Commands:
  key         Press one or more buttons
  text        Type lowercase text, one key per character
  screenshot  Capture the screen [aliases: shot]
  wait        Do nothing for a while [aliases: sleep]
  launch      Launch an app
  close       Close a running app
  echo        Print a message, to annotate a flow
  run         Replay a flow file, or `-` for stdin
  keys        List the key names this tool accepts

Options:
  -d, --device <DEVICE>  Specify DEVICE to use [env: ARES_DEVICE=]
      --json             Print one JSON object per action on stdout
  -q, --quiet            Only print errors
  -v, --verbose...       Print every luna call before it is sent
      --dry-run          Resolve everything and print it, without touching the device
      --allow-non-root   Run even when the session is not root
  -h, --help             Print help
  -V, --version          Print version
```

## Start with TAB

The single most common mistake, worth stating before anything else: **a web
view starts with nothing focused.** `ENTER` does nothing at all until `TAB` has
established focus — and the key still reports success, because the platform
accepted it. If a flow seems to do nothing, this is usually why.

```sh
ares-do -d tv key TAB TAB TAB ENTER
```

## Examples

```sh
ares-do -d tv key OK                    # press OK
ares-do -d tv key DOWN --repeat 5       # five times
ares-do -d tv key TAB OK --delay 300    # slower, for a busy app
ares-do -d tv key 28                    # a raw evdev code
ares-do -d tv text "demo"               # type it, one key per character

ares-do -d tv screenshot shot.png       # save it
ares-do -d tv screenshot > shot.png     # or send the PNG to stdout
ares-do -d tv screenshot --method GRAPHIC ui.png

ares-do keys                            # the buttons a remote has
ares-do keys volume --json              # filtered, machine-readable
```

## Key names

A key is a name, an alias, or a raw
[evdev](https://github.com/torvalds/linux/blob/master/include/uapi/linux/input-event-codes.h)
code from 0 to 767. Names are case insensitive and the `KEY_` prefix is
optional, so `OK`, `ok`, `ENTER` and `KEY_ENTER` are the same key.

`ares-do keys` lists the ones a remote has; `ares-do keys --all` lists all 512.
Unknown names suggest the nearest match, so a typo is one line of output away
from being fixed.

Which evdev code each button sends is LG's choice, and it is not always the
name you would guess. The mappings below come from
`/usr/share/X11/xkb/keycodes/lg` in the firmware, and the surprising ones are
flagged in `ares-do keys`:

| Remote button | evdev | Name to use            |
|---------------|-------|------------------------|
| OK            | 28    | `OK`, `ENTER`          |
| Back          | 412   | `GOBACK`, `PREVIOUS`   |
| Home          | 172   | `HOMEPAGE`             |
| Launcher      | 125   | `SUPER`, `META`        |
| Guide         | 362   | `GUIDE`, `PROGRAM`     |
| Input/Source  | 241   | `SOURCE`, `INPUT`      |
| Rec List      | 144   | `RECLIST`              |
| Voice         | 428   | `VOICE`                |

Two traps hide in that table. `BACK` (158) is a real key and is **not** the
remote's Back button — it is swallowed before it reaches any window, which is
what makes it look broken. Use `GOBACK`. Likewise `HOME` (102) is the keyboard
Home key; the remote's Home is `HOMEPAGE`.

## Typing text

`text` sends one key per character, so it can only type what a key can produce
unmodified: `a-z`, `0-9`, space, tab, newline, and ``-=[]\;',./` ``.

Capitals and shifted symbols are impossible, not merely unimplemented.
`sendKeyCode` takes one code per call and injects a complete press, so there is
no held modifier for a following key to combine with. `ares-do` refuses them
up front, naming every character it cannot type, rather than sending half a
string:

```console
$ ares-do -d tv text 'Demo!'
text: 2 characters cannot be typed with sendKeyCode:
  index 0: 'D' needs Shift, which sendKeyCode cannot express (pass --lower to send 'd')
  index 4: '!' needs Shift (it sits on '1'), which sendKeyCode cannot express
```

What the app *receives* is still up to its keymap. `text` sends keys, not
characters.

## Flows

A flow file is one command per line, in exactly the syntax above — the command
line and the flow language are the same parser, so anything you can type you
can script.

```text
# frames/ gets one PNG per shot
launch com.webos.app.livetv
wait 2s
shot 01-livetv
key SUPER
wait 1s
shot 02-launcher          # named shots keep their name
key RIGHT RIGHT OK --delay 300
shot                      # unnamed ones are numbered: shot-003.png
echo "should be in the app now"
```

```sh
ares-do -d tv run flow.txt --out ./frames
```

Comments run to the end of a line. `"…"` and `'…'` group words, and `\` escapes.
Times are milliseconds unless they carry a unit: `wait 500`, `wait 1.5s`.

The whole file is parsed before anything runs, and **every** error is reported
at once with its line number, so one pass fixes the file:

```console
$ ares-do run flow.txt
flow.txt:4: unknown key "OKAY"; did you mean OK, PLAY?
      4 | key OKAY
flow.txt:9: unterminated " quote opened at column 6
      9 | text "demo
```

`--settle` (300 ms by default) waits before each shot, because a key is not
finished when the call returns — see [Timing](#timing).

## Driving it from a script or an agent

stdout is data and stderr is for people, so redirection always does the
obvious thing. On success `ares-do` is silent.

Read a whole flow from stdin, and the run costs **one** SSH handshake instead
of one per key:

```sh
ares-do -d tv --json run - --out ./frames <<'EOF'
key SUPER
wait 2s
shot launcher.png
EOF
```

`--json` prints one JSON object per action, flushed, so a caller can react
before the flow finishes:

```json
{"event":"key","ok":true,"keys":[{"name":"LEFTMETA","code":125}],"ms":96}
{"event":"screenshot","ok":true,"path":"/abs/launcher.png","bytes":124717,
 "method":"DISPLAY","service":"com.webos.service.tv.capture",
 "foregroundAppId":"com.webos.app.livetv","ms":1124}
{"event":"summary","ok":true,"steps":3,"ran":3,"ms":3220}
```

A failure always ends the stream with an `error` object whose `code` is the
process exit code, so reading only the last line is enough.

With `--json`, `screenshot` needs a filename — stdout is already carrying the
JSON.

`--dry-run` resolves everything and prints it without connecting. It does not
even look the device up, so it can check a generated flow with no TV in the
room. `ares-do keys` needs no device either.

```console
$ ares-do key OK TAB --dry-run
luna://com.webos.service.networkinput/test/sendKeyCode {"keyCode":28}   # ENTER (28)
luna://com.webos.service.networkinput/test/sendKeyCode {"keyCode":15}   # TAB (15)
```

### Exit codes

| Code | Meaning                                                      |
|------|--------------------------------------------------------------|
| 0    | success                                                      |
| 1    | unexpected internal failure                                  |
| 2    | bad usage: unknown flag or key, untypeable text, flow syntax |
| 3    | device not found in the device list                          |
| 4    | could not connect or authenticate                            |
| 5    | not root                                                     |
| 6    | luna service unavailable                                     |
| 7    | the device answered `returnValue: false`                     |
| 8    | host I/O error                                               |
| 9    | capture failed, or timed out waiting for the device          |
| 101  | panic — a bug, please report it                              |
| 130  | interrupted                                                  |

`run` exits with the failing step's own code, so the same branch works whether
you ran one command or a flow. Codes 10-19 are reserved.

Note this differs from `ares-shell`, which uses 255 for local errors because it
forwards the remote command's status. `ares-do` never forwards one.

## Timing

`sendKeyCode` returns as soon as the platform accepts the code, **not** when
the focused window has done anything with it. Everything after a key is a race,
and there is no completion signal to wait on.

- `--delay` (100 ms) goes between keys. `--delay 0` is allowed and will lose
  keys on an app whose event loop is slower than the injection rate — the luna
  call still succeeds, so the loss is invisible.
- `--settle` (300 ms) goes before each screenshot in a flow. It is a guess.
  Raise it for an app that animates.

Input belongs to whichever window has focus, and any system surface — a volume
OSD, an update prompt, the screensaver — can take it mid-flow while every step
still reports success. Each `screenshot` event records `foregroundAppId`, so
that drift is at least visible afterwards.

## Screenshots

The capture goes to a temporary file on the device, comes back over SFTP, and
is deleted. `ares-do` waits for the file to appear, then checks it is a whole
PNG before believing it — the service reports success before it has finished
writing, so reading too early gives a valid-looking prefix. Files are written
to `<name>.part` and renamed, so nothing ever observes a half-written PNG.

`--method` picks the plane: `DISPLAY` (default), `VIDEO`, or `GRAPHIC`. Under
HDCP, `VIDEO` and often `DISPLAY` come back black, with the call reporting
success. That is the platform, not the tool.

Two services provide this and firmware has one or the other;
`com.webos.service.capture` is tried first and `com.webos.service.tv.capture`
second, with the winner remembered for the rest of the run.

If the capture service cannot write to `/tmp` on your firmware, point
`--remote-dir` somewhere it can, such as `/media/developer/temp`.

`Drop` does not run on Ctrl-C, so an interrupted run can leave a
`/tmp/ares-do-*.png` behind. Each run sweeps its own leftovers on the way in.

## Regenerating the key table

`src/keycode/table.rs` is generated and committed, so the accepted key set is
the same on every platform — release builds run on macOS and Windows, which
have no kernel headers at all.

```sh
ares-do/tools/gen-keycodes.sh > ares-do/src/keycode/table.rs
cargo test -p ares-do
```

## Differences from @webosose/ares-cli

There is no `ares-do` in the original. The key injection and capture services
it uses are undocumented, and were found by
[reading the firmware](https://github.com/webosbrew/samples/pull/1#issuecomment-5381189984):
`test/sendKeyCode` reaches `UInputWriter::sendKeyPress`, which writes to
`/dev/uinput` — which is why the codes are ordinary evdev, and why they stop at
767.
