# ares-novacom

Low-level device transport helpers (SSH key retrieval, port forwarding).

Part of [ares-cli-rs](https://github.com/webosbrew/ares-cli-rs), a Rust rewrite of
[@webosose/ares-cli](https://github.com/webosose/ares-cli). See the repository
README for install steps.

```text
Low-level device transport helpers (SSH key retrieval, port forwarding)

Usage: ares-novacom [OPTIONS]

Options:
  -d, --device <DEVICE>                 Specify DEVICE to use [env: ARES_DEVICE=]
  -k, --getkey                          Fetch the SSH private key (webos_rsa) from the device
      --passphrase <PASSPHRASE>         Passphrase for the device's SSH key (the code shown in Developer Mode)
  -f, --forward                         Forward a device port to the host machine (use with --port)
  -p, --port <DEVICE_PORT[:HOST_PORT]>  Port to forward: the device port, optionally mapped to a host port
  -h, --help                            Print help
```

`--getkey` writes the key to `~/.ssh` and points the device entry at it. Run it
once per device, after you add the device and turn on Developer Mode.

`--forward` keeps running until you stop it with Ctrl+C.

## Examples

```sh
# Fetch the SSH key. The passphrase is the code the Developer Mode app shows.
ares-novacom -d tv --getkey --passphrase ABC123

# Reach the device web inspector on http://localhost:9998
ares-novacom -d tv --forward --port 9998

# Map device port 9998 to host port 18998 instead.
ares-novacom -d tv --forward --port 9998:18998
```
