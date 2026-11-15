# ares-device

Show information about the webOS devices you set up.

Part of [ares-cli-rs](https://github.com/webosbrew/ares-cli-rs), a Rust rewrite of
[@webosose/ares-cli](https://github.com/webosose/ares-cli). See the repository
README for install steps.

```text
Show information about the webOS devices you set up

Usage: ares-device [OPTIONS]

Options:
  -d, --device [<DEVICE>]  Specify DEVICE to use, show picker if no value specified [env: ARES_DEVICE=]
  -D, --device-list        List the available devices
  -h, --help               Print help
```

`-d` with no value opens a graphical picker. The picker is native on Windows and
uses GTK on other systems, except macOS, where it is not available yet.

The GTK version is chosen when building. GTK 4 is the default; the `gtk3`
feature selects GTK 3, and building with neither leaves the picker out:

```sh
cargo install --locked --path ares-device --no-default-features --features gtk3
cargo install --locked --path ares-device --no-default-features
```

## Examples

```sh
ares-device --device-list
ares-device -d tv
```
