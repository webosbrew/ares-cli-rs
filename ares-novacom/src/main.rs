use std::io::{Error as IoError, ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
#[cfg(unix)]
use std::os::fd::{AsRawFd, BorrowedFd};
#[cfg(windows)]
use std::os::windows::io::{AsRawSocket, BorrowedSocket};
use std::path::Path;
use std::process::exit;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ares_connection_lib::DeviceSetupManager;
use ares_connection_lib::session::{DeviceSession, NewSession};
use ares_device_lib::cli::unwrap_or_exit;
use ares_device_lib::{DeviceManager, PrivateKey};
use clap::Parser;
use libssh_rs::{Channel, Error as SshError};
use socket2::{SockRef, TcpKeepalive};

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
        group = "action",
        help = "Fetch the SSH private key (webos_rsa) from the device"
    )]
    getkey: bool,
    #[arg(
        long,
        value_name = "PASSPHRASE",
        help = "Passphrase for the device's SSH key (the code shown in Developer Mode)"
    )]
    passphrase: Option<String>,
    #[arg(
        short = 'f',
        long,
        group = "action",
        requires = "port",
        help = "Forward a device port to the host machine (use with --port)"
    )]
    forward: bool,
    #[arg(
        short = 'r',
        long,
        group = "action",
        requires = "port",
        help = "Publish a host port on the device (reverse forward; use with --port)"
    )]
    reverse: bool,
    #[arg(
        short = 'p',
        long,
        value_name = "DEVICE_PORT[:HOST_PORT]",
        help = "Port to forward: the device port, optionally mapped to a host port"
    )]
    port: Option<String>,
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
    } else if cli.forward {
        forward(&manager, cli.device.as_deref(), cli.port.as_deref());
    } else if cli.reverse {
        reverse(&manager, cli.device.as_deref(), cli.port.as_deref());
    } else {
        Cli::parse_from(["", "--help"]);
    }
}

/// A forward sits idle for hours, and a device that goes away without closing
/// the connection - a TV dropping off Wi-Fi, rather than one that shuts the
/// session down - leaves the host holding a socket it still believes in. The
/// forward then waits forever for connections that can no longer arrive.
///
/// TCP keepalive is what notices. When the probes go unanswered the socket
/// fails, libssh sees the error, and the loop below finds `is_connected()`
/// false and says so.
fn keep_alive(session: &DeviceSession) {
    let keepalive = TcpKeepalive::new()
        .with_time(Duration::from_secs(30))
        .with_interval(Duration::from_secs(10));
    // Windows counts its own retries and has no knob for it.
    #[cfg(not(windows))]
    let keepalive = keepalive.with_retries(3);

    // Best effort: a forward that cannot set this still works, it just goes on
    // trusting a dead socket for as long as TCP does.
    #[cfg(unix)]
    let socket = unsafe { BorrowedFd::borrow_raw(session.as_raw_fd()) };
    #[cfg(windows)]
    let socket = unsafe { BorrowedSocket::borrow_raw(session.as_raw_socket()) };
    if let Err(e) = SockRef::from(&socket).set_tcp_keepalive(&keepalive) {
        eprintln!("Could not set keepalive on the connection: {e}");
    }
}

/// Local port-forward: accept TCP connections on a host port and tunnel each
/// through the device's SSH session to `localhost:<device_port>` on the device.
fn forward(manager: &DeviceManager, device: Option<&str>, port_spec: Option<&str>) {
    let Some(port_spec) = port_spec else {
        eprintln!("--port is required with --forward (DEVICE_PORT[:HOST_PORT])");
        exit(1);
    };
    let (device_port, host_port) = match parse_port(port_spec) {
        Ok(ports) => ports,
        Err(e) => {
            eprintln!("{e}");
            exit(1);
        }
    };

    let Some(device) = unwrap_or_exit(manager.find_or_default(device.as_ref()), "find device")
    else {
        eprintln!("Device not found");
        exit(1);
    };

    let session = unwrap_or_exit(device.new_session(), &format!("connect to {}", device.host));
    keep_alive(&session);
    let session = Arc::new(session);

    let listener = unwrap_or_exit(
        TcpListener::bind(("127.0.0.1", host_port)),
        "bind the host port",
    );
    println!(
        "Forwarding 127.0.0.1:{host_port} -> localhost:{device_port} on {}. Press Ctrl+C to stop.",
        device.name
    );

    for stream in listener.incoming() {
        match stream {
            Ok(tcp) => {
                let session = Arc::clone(&session);
                thread::spawn(move || {
                    if let Err(e) = bridge(&session, tcp, device_port) {
                        eprintln!("Forward connection closed: {e}");
                    }
                });
            }
            Err(e) => eprintln!("Failed to accept connection: {e}"),
        }
    }
}

/// How long one poll for a new device-side connection may hold the session, and
/// how long to leave the session alone afterwards. The sum is the worst-case
/// delay before a new connection is picked up.
const ACCEPT_POLL: Duration = Duration::from_millis(10);
const ACCEPT_BACKOFF: Duration = Duration::from_millis(40);

/// Remote port-forward: ask the device to listen on a port of its own, and pump
/// every connection it accepts back to `127.0.0.1:<host_port>` on this machine.
fn reverse(manager: &DeviceManager, device: Option<&str>, port_spec: Option<&str>) {
    let Some(port_spec) = port_spec else {
        eprintln!("--port is required with --reverse (DEVICE_PORT[:HOST_PORT])");
        exit(1);
    };
    let (device_port, host_port) = match parse_port(port_spec) {
        Ok(ports) => ports,
        Err(e) => {
            eprintln!("{e}");
            exit(1);
        }
    };

    let Some(device) = unwrap_or_exit(manager.find_or_default(device.as_ref()), "find device")
    else {
        eprintln!("Device not found");
        exit(1);
    };

    let session = unwrap_or_exit(device.new_session(), &format!("connect to {}", device.host));
    keep_alive(&session);

    // Bind the device's loopback, not all of its interfaces: the point is to
    // let something running on the device reach the host, not to put the host
    // port on the network the device sits on.
    let bound = unwrap_or_exit(
        session.listen_forward(Some("localhost"), device_port),
        &format!("ask {} to listen on port {device_port}", device.name),
    );
    // A device only names the port it picked when it was asked for port 0.
    let bound = if bound == 0 { device_port } else { bound };
    println!(
        "Publishing 127.0.0.1:{host_port} as localhost:{bound} on {}. Press Ctrl+C to stop.",
        device.name
    );

    loop {
        // A poll holds libssh's session lock for the whole timeout it is given,
        // and asking for the lock again right away starves the threads pumping
        // the connections already open. So glance for a moment, then stand back
        // and let them have the session.
        match session.accept_forward(ACCEPT_POLL) {
            Ok((_, channel)) => {
                thread::spawn(move || {
                    if let Err(e) = reverse_bridge(&channel, host_port) {
                        eprintln!("Reverse forward connection closed: {e}");
                    }
                });
                continue;
            }
            // An idle poll window. libssh reports it as `TryAgain` only when no
            // earlier error is still hanging around the session, so anything
            // else counts as idle too until the connection itself is gone.
            Err(_) if session.is_connected() => {}
            Err(_) => {
                // Whatever libssh last recorded is rarely the reason: a session
                // that has gone away usually surfaces as a stale `TryAgain`.
                eprintln!("Lost the connection to {}.", device.name);
                exit(1);
            }
        }
        thread::sleep(ACCEPT_BACKOFF);
    }
}

/// Parses a `DEVICE_PORT[:HOST_PORT]` spec. The host port defaults to the
/// device port when omitted.
fn parse_port(spec: &str) -> Result<(u16, u16), String> {
    let mut parts = spec.splitn(2, ':');
    let device_raw = parts.next().unwrap_or("").trim();
    let device_port: u16 = device_raw
        .parse()
        .map_err(|_| format!("Invalid device port: {device_raw:?}"))?;
    let host_port = match parts.next() {
        Some(host_raw) => host_raw
            .trim()
            .parse()
            .map_err(|_| format!("Invalid host port: {:?}", host_raw.trim()))?,
        None => device_port,
    };
    Ok((device_port, host_port))
}

/// How long to wait on the device for more data, while a connection is still
/// moving bytes and once it has gone quiet.
const BUSY_POLL: Duration = Duration::from_millis(1);
const IDLE_POLL: Duration = Duration::from_millis(50);

/// Opens a forwarding channel to `localhost:<device_port>` on the device and
/// pumps `tcp` through it.
fn bridge(session: &DeviceSession, tcp: TcpStream, device_port: u16) -> Result<(), IoError> {
    let channel = session.new_channel().map_err(to_io)?;
    channel
        .open_forward("localhost", device_port, "127.0.0.1", 0)
        .map_err(to_io)?;
    pump(&channel, tcp)
}

/// Connects to `127.0.0.1:<host_port>` on this machine and pumps a channel the
/// device opened through it.
fn reverse_bridge(channel: &Channel, host_port: u16) -> Result<(), IoError> {
    let tcp = TcpStream::connect(("127.0.0.1", host_port))?;
    pump(channel, tcp)
}

/// Pumps bytes both ways between a local TCP connection and an SSH forwarding
/// channel until either side closes. Uses short polling timeouts so a single
/// SSH session can service several connections without one blocking the others.
fn pump(channel: &Channel, mut tcp: TcpStream) -> Result<(), IoError> {
    tcp.set_read_timeout(Some(Duration::from_millis(10)))?;

    let mut buf = [0u8; 16 * 1024];
    let mut socket_eof = false;
    // A quiet connection should not spin, but waiting 50ms on the device after
    // every buffer would throttle a busy one to a few hundred KB/s. So poll the
    // device briefly while bytes are still moving, and settle down once they
    // stop.
    let mut busy = false;
    loop {
        if !socket_eof {
            match tcp.read(&mut buf) {
                Ok(0) => {
                    socket_eof = true;
                    let _ = channel.send_eof();
                }
                Ok(n) => {
                    channel.stdin().write_all(&buf[..n])?;
                    busy = true;
                }
                Err(e) if would_block(&e) => {}
                Err(e) => return Err(e),
            }
        }
        let poll = if busy { BUSY_POLL } else { IDLE_POLL };
        busy = false;
        match channel.read_timeout(&mut buf, false, Some(poll)) {
            Ok(0) => {
                if channel.is_eof() {
                    break;
                }
            }
            Ok(n) => {
                tcp.write_all(&buf[..n])?;
                busy = true;
            }
            // No device data yet within the poll window; keep going.
            Err(SshError::TryAgain) => {}
            Err(e) => return Err(to_io(e)),
        }
        if channel.is_closed() {
            break;
        }
    }
    let _ = channel.close();
    Ok(())
}

fn would_block(e: &IoError) -> bool {
    matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
}

fn to_io(e: SshError) -> IoError {
    IoError::new(ErrorKind::Other, e.to_string())
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

    let key_name = key_file_name(&content);
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

/// Builds the local key filename from the key itself, as `webos_` plus the
/// first 10 hex digits of its SHA-256. The `webos_` prefix follows the repo
/// convention, so `ares-setup-device --remove` still cleans the file up.
///
/// The name follows the key, not the device, so fetching the same key again
/// overwrites one file instead of leaving a copy per device name. Surrounding
/// whitespace is cut before hashing, because the device sends a trailing
/// newline only some of the time.
fn key_file_name(key: &str) -> String {
    let digest = sha256::digest(key.trim());
    format!("webos_{}", &digest[..10])
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

#[cfg(test)]
mod tests {
    use super::key_file_name;

    const KEY: &str = "-----BEGIN RSA PRIVATE KEY-----\nAAAA\n-----END RSA PRIVATE KEY-----";

    #[test]
    fn name_has_prefix_and_short_digest() {
        let name = key_file_name(KEY);
        assert!(name.starts_with("webos_"), "{name}");
        assert_eq!(name.len(), "webos_".len() + 10);
        assert!(name[6..].chars().all(|c| c.is_ascii_hexdigit()), "{name}");
    }

    #[test]
    fn same_key_gives_same_name() {
        assert_eq!(key_file_name(KEY), key_file_name(KEY));
    }

    #[test]
    fn surrounding_whitespace_is_ignored() {
        assert_eq!(key_file_name(KEY), key_file_name(&format!("{KEY}\n")));
        assert_eq!(key_file_name(KEY), key_file_name(&format!("  {KEY}  ")));
    }

    #[test]
    fn different_keys_give_different_names() {
        assert_ne!(
            key_file_name(KEY),
            key_file_name("-----BEGIN RSA PRIVATE KEY-----\nBBBB\n-----END RSA PRIVATE KEY-----")
        );
    }
}
