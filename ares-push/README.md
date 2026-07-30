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
  -i, --ignore           Continue on errors instead of stopping at the first failure
  -h, --help             Print help
```

Directories are copied with their contents. Without `--ignore`, the first
failure stops the copy and the exit code is non-zero.

## Examples

```sh
ares-push -d tv ./build /media/developer/apps/usr/palm/applications
ares-push -d tv --ignore ./a.txt ./b.txt /tmp
```
