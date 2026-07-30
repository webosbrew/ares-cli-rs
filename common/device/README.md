# ares-device-lib

Device list and SSH key handling shared by the
[ares-cli-rs](https://github.com/webosbrew/ares-cli-rs) tools.

The crate reads and writes `~/.webos/ose/novacom-devices.json`
(`%AppData%\.webos\ose\novacom-devices.json` on Windows), the same file the
official `@webosose/ares-cli` uses.

This is an internal library. It has no stability promise, so pin an exact
version if you use it outside this repository.

```rust
use ares_device_lib::DeviceManager;

let manager = DeviceManager::default();
for device in manager.list()? {
    println!("{} -> {}@{}:{}", device.name, device.username, device.host, device.port);
}
```
