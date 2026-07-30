# ares-launch

Launch or close an app on a webOS device.

Part of [ares-cli-rs](https://github.com/webosbrew/ares-cli-rs), a Rust rewrite of
[@webosose/ares-cli](https://github.com/webosose/ares-cli). See the repository
README for install steps.

```text
Launch or close an app on a webOS device

Usage: ares-launch [OPTIONS] [APP_ID]

Arguments:
  [APP_ID]  An app id described in appinfo.json

Options:
  -d, --device <DEVICE>  Specify DEVICE to use [env: ARES_DEVICE=]
  -c, --close            Close a running app
  -r, --running          List running apps
  -p, --params <PARAMS>  Launch/Close an app with the specified parameters
  -h, --help             Print help
```

## Examples

```sh
ares-launch -d tv com.example.myapp
ares-launch -d tv --params '{"key":"value"}' com.example.myapp
ares-launch -d tv --close com.example.myapp
ares-launch -d tv --running
```
