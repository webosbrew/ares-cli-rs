# ares-push

Copy files from your computer to a webOS device.

Part of [ares-cli-rs](https://github.com/webosbrew/ares-cli-rs), a Rust rewrite of
[@webosose/ares-cli](https://github.com/webosose/ares-cli). See the repository
README for install steps.

```text
Copy files from your computer to a webOS device

Usage: ares-push [OPTIONS] <SOURCE>... <DESTINATION>

Arguments:
  <SOURCE>...    Path in the host machine, where files exist.
  <DESTINATION>  Path in the DEVICE, where multiple files can be copied

Options:
  -d, --device <DEVICE>  Specify DEVICE to use [env: ARES_DEVICE=]
  -i, --ignore           Hide the detailed copy messages
  -k, --keep-going       Continue on errors instead of stopping at the first failure
  -h, --help             Print help
```

## Where files land

The layout comes from what DESTINATION already is on the device, the same way
`@webosose/ares-cli` decides it. A trailing `/` changes nothing.

| SOURCE      | DESTINATION on the device | Result                    |
|-------------|---------------------------|---------------------------|
| `build`     | missing, or a directory   | `DESTINATION/build`       |
| `build`     | a file                    | error                     |
| `a.txt`     | a directory               | `DESTINATION/a.txt`       |
| `a.txt`     | missing, or a file        | `DESTINATION`             |
| `a.txt b.txt` | anything but a file     | `DESTINATION/a.txt`, `DESTINATION/b.txt` |

Missing parent directories are made along the way, like `mkdir -p`.

A directory keeps its own name, so `ares-push build /media/developer/apps` puts
it at `/media/developer/apps/build`. To copy the contents instead of the
directory, run the command from inside it and push `.`, the way `cp -r . DEST`
works.

## Examples

```sh
ares-push -d tv ./build /media/developer/apps/usr/palm/applications
ares-push -d tv --keep-going ./a.txt ./b.txt /tmp
```

## Devices without SFTP

Some firmware has no SFTP subsystem. Set that device to stream, and files move
over `cat` on a plain exec channel instead:

```sh
ares-setup-device -m NAME -i files=stream
```

The path rules and the output are the same either way. `ares-push` also falls
back to streaming on its own when a device says SFTP but the session will not
start.

## Differences from @webosose/ares-cli

- `-k, --keep-going` skips a file that fails and goes on. The original always
  stops at the first failure. The exit code is still non-zero.
- A symlink to a file is copied as its content, as in the original. A symlink to
  a directory is skipped with a message instead of failing the copy.
- Missing parents of DESTINATION are made over SFTP where the original always
  shells out to `mkdir -p`.
