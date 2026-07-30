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
  -i, --ignore           Continue on errors instead of stopping at the first failure
  -h, --help             Print help
```

## Examples

```sh
ares-pull -d tv /var/log/messages
ares-pull -d tv /media/developer/apps ./backup
```
