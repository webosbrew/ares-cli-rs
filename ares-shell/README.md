# ares-shell

Open an interactive shell on a webOS device.

Part of [ares-cli-rs](https://github.com/webosbrew/ares-cli-rs), a Rust rewrite of
[@webosose/ares-cli](https://github.com/webosose/ares-cli). See the repository
README for install steps.

```text
Open an interactive shell on a webOS device

Usage: ares-shell [OPTIONS]

Options:
  -d, --device <DEVICE>  Specify DEVICE to use [env: ARES_DEVICE=]
  -r, --run <COMMAND>    Run COMMAND
      --pty              Force pseudo-terminal allocation
      --no-pty           Disable pseudo-terminal allocation
  -h, --help             Print help
```

Without `--run`, you get an interactive shell. With `--run`, the command output
goes to stdout, so you can pipe it. Use `--pty` to force a terminal anyway, or
`--no-pty` to turn one off.

## Examples

```sh
ares-shell -d tv
ares-shell -d tv --run 'cat /etc/starfish-release'
ares-shell -d tv --no-pty --run 'ls -l /media/developer' > files.txt
```
