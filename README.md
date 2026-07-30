# ares-cli-rs

Rust rewrite of [@webosose/ares-cli](https://github.com/webosose/ares-cli).

This tool focuses on reducing dependencies, and performance.
Key features and options will be added from time to time.

Each tool is a single native binary. No Node.js, no npm.
The device list is shared with the official CLI, so you can mix both.

## Tools

| Command                                        | What it does                                   |
|------------------------------------------------|------------------------------------------------|
| [`ares-setup-device`](ares-setup-device#readme)| Add, change or remove devices                  |
| [`ares-device`](ares-device#readme)            | Show device information, list devices          |
| [`ares-package`](ares-package#readme)          | Pack an app directory into an `.ipk`           |
| [`ares-install`](ares-install#readme)          | Install, remove and list apps                  |
| [`ares-launch`](ares-launch#readme)            | Launch or close an app, list running apps      |
| [`ares-shell`](ares-shell#readme)              | Open a shell, or run one command on the device  |
| [`ares-push`](ares-push#readme)                | Copy files to the device                       |
| [`ares-pull`](ares-pull#readme)                | Copy files from the device                     |
| [`ares-novacom`](ares-novacom#readme)          | Fetch the device SSH key, forward a port       |

Each command has its own README with the full option list and examples. Every
tool also accepts `--help`. Every device-facing tool accepts `-d NAME`, or reads
the device name from the `ARES_DEVICE` environment variable.

## Install

All install methods below give you the **whole set** of tools at once.

### Prebuilt binaries

Download one archive per platform from the
[latest release](https://github.com/webosbrew/ares-cli-rs/releases/latest).
It holds all nine binaries.

Linux and macOS:

```sh
# One of: linux-x86_64, linux-aarch64, macos-aarch64, macos-x86_64
PLATFORM=linux-x86_64
TAG=$(curl -fsSL https://api.github.com/repos/webosbrew/ares-cli-rs/releases/latest \
      | grep '"tag_name"' | cut -d '"' -f 4)
curl -fsSL "https://github.com/webosbrew/ares-cli-rs/releases/download/$TAG/ares-cli-rs-$TAG-$PLATFORM.tar.gz" \
  | tar xz
sudo install -m 755 "ares-cli-rs-$TAG-$PLATFORM"/ares-* /usr/local/bin/
```

Windows (PowerShell):

```powershell
$tag = (Invoke-RestMethod https://api.github.com/repos/webosbrew/ares-cli-rs/releases/latest).tag_name
$zip = "$env:TEMP\ares-cli-rs.zip"
$dst = "$env:LOCALAPPDATA\Programs\ares-cli"
Invoke-WebRequest "https://github.com/webosbrew/ares-cli-rs/releases/download/$tag/ares-cli-rs-$tag-windows-x86_64.zip" -OutFile $zip
Expand-Archive $zip -DestinationPath $env:TEMP -Force
New-Item -ItemType Directory -Force $dst | Out-Null
Copy-Item "$env:TEMP\ares-cli-rs-$tag-windows-x86_64\*.exe" $dst -Force
```

Then add `%LOCALAPPDATA%\Programs\ares-cli` to your `PATH`.

Each archive has a `.sha256` file next to it. Check it before you unpack:

```sh
sha256sum -c "ares-cli-rs-$TAG-$PLATFORM.tar.gz.sha256"
```

### Debian and Ubuntu

Releases also carry one `.deb` per tool, for `amd64` and `arm64`.
With the [GitHub CLI](https://cli.github.com/):

```sh
gh release download --repo webosbrew/ares-cli-rs --pattern '*_amd64.deb'
sudo apt install ./*_amd64.deb
```

Without it, download the `.deb` files from the release page, then run the
`apt install` line.

### From source with cargo

Needs a [Rust](https://rustup.rs/) toolchain, plus the build dependencies below.

```sh
cargo install --locked --git https://github.com/webosbrew/ares-cli-rs \
  ares-setup-device ares-device ares-package ares-install ares-launch \
  ares-shell ares-push ares-pull ares-novacom
```

Cargo puts the binaries in `~/.cargo/bin`, which is already on your `PATH`
after a `rustup` install.

### Build dependencies

- **Linux**: `pkg-config`, `libssl-dev` and `libgtk-3-dev`
  (GTK is only for the `ares-device` device picker).
- **macOS**: Xcode command line tools. OpenSSL is built from source, so
  Homebrew is not needed.
- **Windows**: MSVC build tools, and [Strawberry Perl](https://strawberryperl.com/)
  to build the bundled OpenSSL. To use an OpenSSL you already have instead:

  ```powershell
  $env:OPENSSL_NO_VENDOR = "1"
  $env:OPENSSL_DIR = "C:\Program Files\OpenSSL-Win64"
  $env:OPENSSL_LIB_DIR = "C:\Program Files\OpenSSL-Win64\lib\VC\x64\MD"
  $env:OPENSSL_INCLUDE_DIR = "C:\Program Files\OpenSSL-Win64\include"
  ```

To build the whole workspace from a clone:

```sh
cargo build --release --workspace
```

## First steps

Turn on Developer Mode on the TV first, and note the passphrase it shows.

```sh
# Add the TV. Use the address and passphrase from the Developer Mode app.
ares-setup-device --add tv \
  --info host=192.168.1.42 --info username=prisoner --info passphrase=ABC123

# Fetch the SSH key for it.
ares-novacom -d tv --getkey --passphrase ABC123

# Check the connection.
ares-device -d tv

# Build and install an app.
ares-package ./my-app
ares-install -d tv ./com.example.myapp_1.0.0_all.ipk
ares-launch -d tv com.example.myapp
```

Set `ARES_DEVICE=tv` to drop the `-d tv` from every command.

A device in Developer Mode listens on port 9922 as the user `prisoner`, which is
what the example above uses. A rooted device usually listens on port 22 as
`root`, so add `--info username=root --info port=22` instead.

The device list is stored in `~/.webos/ose/novacom-devices.json`
(`%AppData%\.webos\ose\novacom-devices.json` on Windows), the same file the
official CLI uses.

## License

Apache-2.0. See [LICENSE](LICENSE).
