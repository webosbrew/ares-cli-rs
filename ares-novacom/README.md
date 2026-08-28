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
  -r, --reverse                         Publish a host port on the device (reverse forward; use with --port)
  -p, --port <DEVICE_PORT[:HOST_PORT]>  Port to forward: the device port, optionally mapped to a host port
  -h, --help                            Print help
```

`--getkey` writes the key to `~/.ssh` and points the device entry at it. Run it
once per device, after you add the device and turn on Developer Mode.

The file is named `webos_<digest>`, where the digest is the first 10 hex digits
of the key's SHA-256. The name follows the key, so fetching the same key again
overwrites one file instead of leaving a copy per device name. Turning
Developer Mode off and on makes the device generate a new key, which lands in a
new file.

`--forward` carries connections one way: you connect on the host, and the
device's own service answers. `--reverse` turns that around, so something
running on the device — an app under development, a script in a shell — can
reach a server on your machine at `localhost:<device port>`.

`--port` reads the same way for both: the device port first, the host port
after it. Which end listens is what changes. `--forward --port 9998:18998`
listens on host port 18998 and connects to device port 9998; `--reverse --port
9977:18080` listens on device port 9977 and connects to host port 18080.

The device only ever binds its own loopback, so a port you publish is reachable
from the device itself and not from the rest of the network. Ports below 1024
need a device that logs you in as root; a dev-mode `prisoner` login cannot bind
them.

Both keep running until you stop them with Ctrl+C.

A device that goes away without closing the connection — one dropping off
Wi-Fi, rather than one shutting the session down — would otherwise leave the
forward waiting on a tunnel that no longer carries anything. Every connection
is kept alive, so the forward notices within about a minute and exits saying
so.

## Examples

```sh
# Fetch the SSH key. The passphrase is the code the Developer Mode app shows.
ares-novacom -d tv --getkey --passphrase ABC123

# Reach the device web inspector on http://localhost:9998
ares-novacom -d tv --forward --port 9998

# Map device port 9998 to host port 18998 instead.
ares-novacom -d tv --forward --port 9998:18998

# The other way: let the TV reach a dev server running on the host at :18080,
# as http://localhost:9977 on the device.
ares-novacom -d tv --reverse --port 9977:18080
```
