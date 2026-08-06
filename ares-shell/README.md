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
      --no-prompt        Disable the local prompt and line editor used without a
                         pseudo-terminal
  -h, --help             Print help
```

Without `--run`, you get an interactive shell. With `--run`, the command output
goes to stdout, so you can pipe it. Use `--pty` to force a terminal anyway, or
`--no-pty` to turn one off.

## Without a pseudo-terminal

Some devices refuse a pseudo-terminal. The remote shell then runs
non-interactive: it prints no prompt and echoes nothing. `ares-shell` draws
those locally instead, so the session still feels like a shell:

```text
ares root@tv:/media/developer # cd apps
ares root@tv:/media/developer/apps # false
ares root@tv:/media/developer/apps [1]#
```

The prompt shows the remote user, the device name, the working directory, and
the exit status of the last command when it is not zero. Arrow keys edit the
line and walk the history. History lasts for the session only, and no file is
written. An open quote gives you an `ares >` prompt for the rest of the
command.

Tab completes file names, directory names, and command names. There is no local
copy of the device file system, so each Tab asks the device and waits for the
answer. It cannot do what `bash-completion` does, such as git subcommands or
per-command flags: those scripts are not on the device.

To keep this up to date, `ares-shell` sends one hidden `printf` command after
each of your commands. `--no-prompt` turns the whole thing off and goes back to
a plain byte pump. The prompt is also off when you pipe input or output, or use
`--run`, so scripts see clean output.

Two things a pseudo-terminal would give you that this cannot:

- Ctrl+C cannot signal the remote program. `ares-shell` still tries, and press
  it twice to close the connection.
- Ctrl+D during a running command sends a byte, not an end of file.

## Examples

```sh
ares-shell -d tv
ares-shell -d tv --run 'cat /etc/starfish-release'
ares-shell -d tv --no-pty --run 'ls -l /media/developer' > files.txt
```
