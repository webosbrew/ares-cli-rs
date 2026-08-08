use std::io::{Error, ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use ares_device_lib::DeviceManager;
use httparse::{Response, Status};
use libssh_rs::SshKey;

use crate::DeviceSetupManager;

/// The port the key server on a device in developer mode listens on.
pub const NOVACOM_KEY_PORT: u16 = 9991;

/// The largest key the server may return. The key is a few kilobytes, so this
/// only stops a wrong server from filling memory.
const MAX_KEY_RESPONSE: u64 = 65536;

/// Fetch `webos_rsa` from the key server on a device in developer mode.
///
/// This speaks HTTP/1.0 over a plain socket instead of using an HTTP client.
/// The key server has no TLS, and Android blocks cleartext HTTP by default, so
/// an HTTP client can't reach it from dev-manager-desktop on Android. A raw
/// socket is not blocked. It also keeps an HTTP client out of the dependencies.
///
/// # Errors
///
/// Returns an error if the host does not resolve, the connection fails, the
/// response is not HTTP, or the status is not 200.
pub fn fetch_key(host: &str, port: u16) -> Result<String, Error> {
    let address = format!("{host}:{port}")
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| Error::new(ErrorKind::NotFound, format!("Can't resolve {host}")))?;
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(10))?;
    stream.write_all(b"GET /webos_rsa HTTP/1.0\r\n")?;
    stream.write_all(b"Connection: close\r\n")?;
    stream.write_all(b"\r\n")?;

    let mut limited_stream = stream.take(MAX_KEY_RESPONSE);
    let mut buffer = Vec::new();
    limited_stream.read_to_end(&mut buffer)?;

    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut response = Response::new(&mut headers);
    let Status::Complete(size_to_skip) = response
        .parse(&buffer)
        .map_err(|e| Error::new(ErrorKind::InvalidData, e))?
    else {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "The key server sent an incomplete response",
        ));
    };
    if response.code != Some(200) {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("The key server answered {:?}", response.code),
        ));
    }
    Ok(String::from_utf8_lossy(&buffer[size_to_skip..]).to_string())
}

impl DeviceSetupManager for DeviceManager {
    fn novacom_getkey(&self, address: &str, passphrase: &str) -> Result<String, Error> {
        let content = fetch_key(address, NOVACOM_KEY_PORT)
            .map_err(|e| Error::new(e.kind(), format!("Can't request private key: {e}")))?;

        match SshKey::from_privkey_base64(&content, Some(passphrase)) {
            Ok(_) => Ok(content),
            _ => Err(Error::other(if passphrase.is_empty() {
                "Passphrase is empty".to_string()
            } else {
                "Passphrase is incorrect".to_string()
            })),
        }
    }

    fn localkey_verify(&self, name: &str, passphrase: &str) -> Result<(), Error> {
        todo!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httptest::matchers::request;
    use httptest::responders::status_code;
    use httptest::{Expectation, Server};

    #[test]
    fn fetch_key_404() {
        let server = Server::run();
        server.expect(
            Expectation::matching(request::method_path("GET", "/webos_rsa"))
                .respond_with(status_code(404).body("Not Found")),
        );
        let addr = server.addr();
        let result = fetch_key(addr.ip().to_string().as_str(), addr.port());
        assert!(result.is_err());
    }

    #[test]
    fn fetch_key_success() {
        let server = Server::run();
        let expected_key =
            "-----BEGIN ENCRYPTED PRIVATE KEY-----\n...\n-----END ENCRYPTED PRIVATE KEY-----\n";
        server.expect(
            Expectation::matching(request::method_path("GET", "/webos_rsa"))
                .respond_with(status_code(200).body(expected_key)),
        );
        let addr = server.addr();
        let result = fetch_key(addr.ip().to_string().as_str(), addr.port());
        assert_eq!(result.unwrap(), expected_key);
    }

    #[test]
    fn fetch_key_refused() {
        let result = fetch_key("127.0.0.1", 9991);
        assert_eq!(
            result.expect_err("nothing listens there").kind(),
            ErrorKind::ConnectionRefused
        );
    }
}
