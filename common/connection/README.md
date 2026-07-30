# ares-connection-lib

Device connection and file transfer shared by the
[ares-cli-rs](https://github.com/webosbrew/ares-cli-rs) tools.

It builds on [ares-device-lib](https://crates.io/crates/ares-device-lib) and
provides:

- `session` — open an SSH session to a device
- `luna` — call Luna service methods, and subscribe to them
- `transfer` — copy files to and from a device

This is an internal library. It has no stability promise, so pin an exact
version if you use it outside this repository.

On Windows and macOS, libssh and OpenSSL are built from source. On other systems
the crate links the system libssh when it is 0.9.7 or newer, so you need
`pkg-config` and `libssl-dev`.
