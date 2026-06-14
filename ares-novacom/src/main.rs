use std::path::Path;
use std::process::exit;

use ares_connection_lib::DeviceSetupManager;
use ares_device_lib::{DeviceManager, PrivateKey};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(about)]
struct Cli {
    #[arg(
        short,
        long,
        value_name = "DEVICE",
        env = "ARES_DEVICE",
        help = "Specify DEVICE to use"
    )]
    device: Option<String>,
    #[arg(
        short = 'k',
        long,
        help = "Fetch the SSH private key (webos_rsa) from the device"
    )]
    getkey: bool,
    #[arg(
        long,
        value_name = "PASSPHRASE",
        help = "Passphrase for the device's SSH key (the code shown in Developer Mode)"
    )]
    passphrase: Option<String>,
}

fn main() {
    let cli = Cli::parse();
    let manager = DeviceManager::default();

    if cli.getkey {
        get_key(
            &manager,
            cli.device.as_deref(),
            cli.passphrase.as_deref().unwrap_or(""),
        );
    } else {
        Cli::parse_from(["", "--help"]);
    }
}

fn get_key(manager: &DeviceManager, device: Option<&str>, passphrase: &str) {
    let Some(device) = unwrap_or_exit(manager.find_or_default(device.as_ref()), "find device")
    else {
        eprintln!("Device not found");
        exit(1);
    };

    println!("Fetching key from {}...", device.host);
    let content = unwrap_or_exit(
        manager.novacom_getkey(&device.host, passphrase),
        "fetch key",
    );

    let key_name = key_file_name(&device.name);
    let key_dir = unwrap_or_exit(manager.ssh_key_dir(), "resolve ssh directory");
    let key_path = key_dir.join(&key_name);
    unwrap_or_exit(write_key(&key_path, &content), "save key");

    // Wire the fetched key into the device config (webOS dev mode is
    // prisoner@<host>:9922 with a passphrase-protected key).
    let mut updated = device.clone();
    updated.private_key = Some(PrivateKey::Name {
        name: key_name.clone(),
    });
    updated.passphrase = (!passphrase.is_empty()).then(|| passphrase.to_string());
    updated.password = None;
    updated.username = String::from("prisoner");
    updated.port = 9922;
    unwrap_or_exit(manager.modify(&device.name, &updated), "update device");

    println!(
        "Saved key to {} and updated device {}.",
        key_path.display(),
        device.name
    );
}

/// Builds the local key filename for a device, matching the repo convention
/// of a `webos_` prefix (so `ares-setup-device --remove` can clean it up).
fn key_file_name(device_name: &str) -> String {
    let sanitized: String = device_name
        .chars()
        .map(|c| if c.is_whitespace() { '_' } else { c })
        .collect();
    format!("webos_{sanitized}")
}

fn write_key(path: &Path, content: &str) -> Result<(), std::io::Error> {
    std::fs::write(path, content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn unwrap_or_exit<T>(result: Result<T, std::io::Error>, action: &str) -> T {
    result.unwrap_or_else(|e| {
        eprintln!("Failed to {action}: {e}");
        exit(1);
    })
}
