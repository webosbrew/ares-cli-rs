# ares-setup-device

Add, change or remove webOS devices.

Part of [ares-cli-rs](https://github.com/webosbrew/ares-cli-rs), a Rust rewrite of
[@webosose/ares-cli](https://github.com/webosose/ares-cli). See the repository
README for install steps.

The device list is stored in `~/.webos/ose/novacom-devices.json`
(`%AppData%\.webos\ose\novacom-devices.json` on Windows), the same file the
official CLI uses.

```text
Add, change or remove webOS devices

Usage: ares-setup-device [OPTIONS]

Options:
  -l, --list            List the devices
  -F, --listfull        List the devices with detailed information
  -a, --add <NAME>      Add a device with NAME (use --info to provide details)
  -m, --modify <NAME>   Modify the device with NAME (use --info to provide changes)
  -r, --remove <NAME>   Remove the device with NAME
  -f, --default <NAME>  Set the device with NAME as default
  -R, --reset           Reset the device list to the default
  -i, --info <INFO>     Device details as JSON or key=value (repeatable) for --add/--modify
  -h, --help            Print help
```

## `--info` fields

`host` (or `ipAddress`), `port`, `username` (or `user`), `password`,
`passphrase`, `profile`, `description`, `privateKey` (or `openSsh`),
`openSshPath` (or `keyPath`), `files`, `default`.

Only `host` is required. The defaults are `username=root`, `port=9922` and
`profile=ose`.

## Examples

```sh
# A TV in Developer Mode.
ares-setup-device --add tv \
  --info host=192.168.1.42 --info username=prisoner --info passphrase=ABC123

# A rooted device.
ares-setup-device --add tv --info host=192.168.1.42 --info port=22

# Same thing, as JSON.
ares-setup-device --add tv --info '{"host":"192.168.1.42","port":22}'

ares-setup-device --default tv
ares-setup-device --listfull
```
