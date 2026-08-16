# ares-pull

Copy files from a webOS device to your computer.

Part of [ares-cli-rs](https://github.com/webosbrew/ares-cli-rs), a Rust rewrite of
[@webosose/ares-cli](https://github.com/webosose/ares-cli). See the repository
README for install steps.

```text
Copy files from a webOS device to your computer

Usage: ares-pull [OPTIONS] <SOURCE> [DESTINATION]

Arguments:
  <SOURCE>       Path on the DEVICE, where files exist
  [DESTINATION]  Path on the host machine, where files are copied to [default: .]

Options:
  -d, --device <DEVICE>  Specify DEVICE to use [env: ARES_DEVICE=]
  -i, --ignore           Hide the detailed copy messages
  -k, --keep-going       Continue on errors instead of stopping at the first failure
  -h, --help             Print help
```

## Where files land

The layout comes from what SOURCE is on the device and what DESTINATION already
is on your computer, the same way `@webosose/ares-cli` decides it. A trailing
`/` changes nothing.

| SOURCE on the device | DESTINATION on the host | Result                |
|----------------------|-------------------------|-----------------------|
| a directory          | anything but a file     | `DESTINATION/<name>`  |
| a directory          | a file                  | error                 |
| a file               | a directory             | `DESTINATION/<name>`  |
| a file               | missing, or a file      | `DESTINATION`         |

Missing parent directories are made along the way.

A directory keeps its own name, so `ares-pull /var/log ./out` puts it at
`./out/log`.

## Examples

```sh
ares-pull -d tv /var/log/messages
ares-pull -d tv /media/developer/apps ./backup
```

## Differences from @webosose/ares-cli

- `-k, --keep-going` skips a file that fails and goes on. The original always
  stops at the first failure. The exit code is still non-zero.
- Pulling a file to a path whose parent does not exist works. The original
  fails with `ENOENT`.
- Symlinks are followed, as in the original. A broken symlink is skipped with a
  message, and nesting past 64 levels stops with an error instead of looping.
- The copy runs over SFTP, so the device needs no `find` binary.
