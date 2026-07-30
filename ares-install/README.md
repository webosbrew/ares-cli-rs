# ares-install

Install or remove an app on a webOS device.

Part of [ares-cli-rs](https://github.com/webosbrew/ares-cli-rs), a Rust rewrite of
[@webosose/ares-cli](https://github.com/webosose/ares-cli). See the repository
README for install steps.

```text
Install or remove an app on a webOS device

Usage: ares-install [OPTIONS] [PACKAGE_FILE]

Arguments:
  [PACKAGE_FILE]  webOS package with .ipk extension

Options:
  -d, --device <DEVICE>  Specify DEVICE to use [env: ARES_DEVICE=]
  -l, --list             List the installed apps
  -F, --listfull         List the installed apps with detailed information
  -t, --type <APP_TYPE>  Filter the listed apps by APP_TYPE
  -r, --remove <APP_ID>  Remove app with APP_ID
  -h, --help             Print help
```

## Examples

```sh
ares-install -d tv ./com.example.myapp_1.0.0_all.ipk
ares-install -d tv --list
ares-install -d tv --remove com.example.myapp
```
