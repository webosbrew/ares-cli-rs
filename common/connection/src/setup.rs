use std::io::{Error, ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use ares_device_lib::DeviceManager;
use httparse::{Response, Status};
use libssh_rs::SshKey;

use crate::DeviceSetupManager;

/// The port the key server on a device in developer mode listens on.
pub const NOVACOM_KEY_PORT: u16 = 9991;

/// The largest response the key server may send. The key is a few kilobytes,
/// so this only stops a wrong server from filling memory.
const MAX_KEY_RESPONSE: usize = 65536;

/// How long to wait for the connection, and for each read or write on it.
const TIMEOUT: Duration = Duration::from_secs(10);

/// How many redirects to follow before giving up.
const MAX_REDIRECTS: usize = 5;

/// Fetch `webos_rsa` from the key server on a device in developer mode.
///
/// This speaks HTTP/1.1 over a plain socket instead of using an HTTP client.
/// The key server has no TLS, and Android blocks cleartext HTTP by default, so
/// an HTTP client can't reach it from dev-manager-desktop on Android. A raw
/// socket is not blocked. It also keeps an HTTP client out of the dependencies.
///
/// The request carries the headers an HTTP client sends. The response may
/// redirect to another port or path on the same host, and its body may be
/// chunked.
///
/// # Errors
///
/// Returns an error if the host does not resolve, the connection fails, the
/// response is not HTTP, a redirect leaves the host, or the final status is not
/// 200.
pub fn fetch_key(host: &str, port: u16) -> Result<String, Error> {
    let mut target = Target {
        host: host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_string(),
        port,
        path: "/webos_rsa".to_string(),
    };
    for _ in 0..=MAX_REDIRECTS {
        match get(&target)? {
            Reply::Body(body) => return Ok(String::from_utf8_lossy(&body).into_owned()),
            Reply::Redirect(location) => target = target.redirect(&location)?,
        }
    }
    Err(Error::new(
        ErrorKind::InvalidData,
        format!("The key server redirected more than {MAX_REDIRECTS} times"),
    ))
}

/// Where a request goes. `path` includes the query, if any.
#[derive(Debug, PartialEq)]
struct Target {
    host: String,
    port: u16,
    path: String,
}

impl Target {
    /// The value of the `Host` header. An IPv6 address goes in brackets.
    fn authority(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    /// Resolve a `Location` header against this target.
    ///
    /// Only plain `http` on the same host is followed. The key must come from
    /// the device the user named, and there is no TLS to speak.
    fn redirect(&self, location: &str) -> Result<Target, Error> {
        let location = location.split('#').next().unwrap_or_default();
        if location.starts_with("//") {
            return self.redirect(&format!("http:{location}"));
        }
        if let Some(rest) = location
            .get(..7)
            .filter(|scheme| scheme.eq_ignore_ascii_case("http://"))
            .map(|_| &location[7..])
        {
            let (authority, path) = rest.split_at(rest.find(['/', '?']).unwrap_or(rest.len()));
            let (host, port) = split_authority(authority)?;
            if !host.eq_ignore_ascii_case(&self.host) {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("The key server redirected to another host: {location}"),
                ));
            }
            return Ok(Target {
                host: self.host.clone(),
                port,
                path: format!("/{}", path.trim_start_matches('/')),
            });
        }
        if location.contains("://") {
            return Err(Error::new(
                ErrorKind::Unsupported,
                format!("The key server redirected to a URL that is not plain HTTP: {location}"),
            ));
        }
        let path = if location.starts_with('/') {
            location.to_string()
        } else {
            let base = self.path.split('?').next().unwrap_or_default();
            format!(
                "{}{location}",
                &base[..=base.rfind('/').unwrap_or_default()]
            )
        };
        Ok(Target {
            host: self.host.clone(),
            port: self.port,
            path,
        })
    }
}

/// Split `host`, `host:port`, `[v6]` or `[v6]:port`. The port defaults to 80.
fn split_authority(authority: &str) -> Result<(&str, u16), Error> {
    let invalid = || {
        Error::new(
            ErrorKind::InvalidData,
            format!("The key server redirected to an invalid host: {authority}"),
        )
    };
    let (host, port) = if let Some(rest) = authority.strip_prefix('[') {
        let (host, rest) = rest.split_once(']').ok_or_else(invalid)?;
        let port = match rest {
            "" => None,
            rest => Some(rest.strip_prefix(':').ok_or_else(invalid)?),
        };
        (host, port)
    } else {
        match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        }
    };
    if host.is_empty() {
        return Err(invalid());
    }
    let port = match port {
        Some(port) => port.parse().map_err(|_| invalid())?,
        None => 80,
    };
    Ok((host, port))
}

enum Reply {
    Body(Vec<u8>),
    Redirect(String),
}

/// How the end of the body is marked.
enum Framing {
    Chunked,
    Length(usize),
    Close,
}

/// The parts of a status line and headers that `get` acts on.
struct Head {
    code: u16,
    location: Option<String>,
    framing: Framing,
    len: usize,
}

/// Send one `GET` and read one response.
///
/// The body is read up to where its framing says it ends, so a server that
/// keeps the connection open does not stall the read.
fn get(target: &Target) -> Result<Reply, Error> {
    let address = (target.host.as_str(), target.port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| {
            Error::new(
                ErrorKind::NotFound,
                format!("Can't resolve {}", target.host),
            )
        })?;
    let mut stream = TcpStream::connect_timeout(&address, TIMEOUT)?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nAccept: */*\r\nConnection: close\r\n\r\n",
        target.path,
        target.authority()
    );
    stream.write_all(request.as_bytes())?;

    let mut reader = ResponseReader {
        stream,
        buffer: Vec::new(),
        received: 0,
    };
    let head = loop {
        if let Some(head) = parse_head(&reader.buffer)? {
            break head;
        }
        reader.fill_or_eof()?;
    };
    match head.code {
        200 => {}
        301 | 302 | 303 | 307 | 308 => {
            return head.location.map(Reply::Redirect).ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("The key server answered {} without a Location", head.code),
                )
            });
        }
        code => {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("The key server answered {code}"),
            ));
        }
    }

    reader.buffer.drain(..head.len);
    match head.framing {
        Framing::Chunked => read_chunked(&mut reader).map(Reply::Body),
        Framing::Length(length) => {
            if length > MAX_KEY_RESPONSE {
                return Err(too_large());
            }
            while reader.buffer.len() < length {
                reader.fill_or_eof()?;
            }
            reader.buffer.truncate(length);
            Ok(Reply::Body(reader.buffer))
        }
        Framing::Close => {
            while reader.fill()? {}
            Ok(Reply::Body(reader.buffer))
        }
    }
}

/// Parse the status line and headers, or return `None` if they are not all in
/// `buffer` yet.
fn parse_head(buffer: &[u8]) -> Result<Option<Head>, Error> {
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut response = Response::new(&mut headers);
    let Status::Complete(len) = response
        .parse(buffer)
        .map_err(|e| Error::new(ErrorKind::InvalidData, e))?
    else {
        return Ok(None);
    };
    let header = |name: &str| {
        response
            .headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case(name))
            .map(|header| String::from_utf8_lossy(header.value).trim().to_string())
    };
    // Chunked is the last coding when present, and it wins over Content-Length.
    let chunked = header("Transfer-Encoding").is_some_and(|codings| {
        codings
            .rsplit(',')
            .next()
            .is_some_and(|coding| coding.trim().eq_ignore_ascii_case("chunked"))
    });
    let framing = if chunked {
        Framing::Chunked
    } else if let Some(length) = header("Content-Length") {
        Framing::Length(length.parse().map_err(|_| {
            Error::new(
                ErrorKind::InvalidData,
                format!("The key server sent an invalid Content-Length: {length}"),
            )
        })?)
    } else {
        Framing::Close
    };
    Ok(Some(Head {
        code: response.code.unwrap_or_default(),
        location: header("Location"),
        framing,
        len,
    }))
}

/// Decode a chunked body. `reader.buffer` starts at the first chunk size.
/// Trailers after the last chunk are not read.
fn read_chunked(reader: &mut ResponseReader) -> Result<Vec<u8>, Error> {
    let mut body = Vec::new();
    let mut pos = 0;
    loop {
        let (size_len, size) = loop {
            match httparse::parse_chunk_size(&reader.buffer[pos..]).map_err(|_| {
                Error::new(
                    ErrorKind::InvalidData,
                    "The key server sent an invalid chunk size",
                )
            })? {
                Status::Complete(parsed) => break parsed,
                Status::Partial => reader.fill_or_eof()?,
            }
        };
        if size == 0 {
            return Ok(body);
        }
        let size = usize::try_from(size)
            .ok()
            .filter(|size| *size <= MAX_KEY_RESPONSE)
            .ok_or_else(too_large)?;
        let start = pos + size_len;
        let end = start + size;
        while reader.buffer.len() < end + 2 {
            reader.fill_or_eof()?;
        }
        if &reader.buffer[end..end + 2] != b"\r\n" {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "The key server sent a chunk that does not end with CRLF",
            ));
        }
        body.extend_from_slice(&reader.buffer[start..end]);
        pos = end + 2;
    }
}

fn too_large() -> Error {
    Error::new(
        ErrorKind::InvalidData,
        format!("The key server sent more than {MAX_KEY_RESPONSE} bytes"),
    )
}

/// The bytes of one response read so far.
struct ResponseReader {
    stream: TcpStream,
    buffer: Vec<u8>,
    received: usize,
}

impl ResponseReader {
    /// Read more of the response into `buffer`. Returns `false` at the end of
    /// the stream.
    fn fill(&mut self) -> Result<bool, Error> {
        let mut chunk = [0u8; 4096];
        let read = loop {
            match self.stream.read(&mut chunk) {
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                result => break result?,
            }
        };
        self.received += read;
        if self.received > MAX_KEY_RESPONSE {
            return Err(too_large());
        }
        self.buffer.extend_from_slice(&chunk[..read]);
        Ok(read > 0)
    }

    /// Like `fill`, but the end of the stream is an error.
    fn fill_or_eof(&mut self) -> Result<(), Error> {
        if self.fill()? {
            Ok(())
        } else {
            Err(Error::new(
                ErrorKind::UnexpectedEof,
                "The key server closed the connection before the response ended",
            ))
        }
    }
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
    use std::net::TcpListener;
    use std::thread::{self, JoinHandle};

    use httptest::matchers::request;
    use httptest::responders::status_code;
    use httptest::{Expectation, Server};

    use super::*;

    const KEY: &str =
        "-----BEGIN ENCRYPTED PRIVATE KEY-----\n...\n-----END ENCRYPTED PRIVATE KEY-----\n";

    /// A server's requests, and the connections it left open.
    type Served = (Vec<String>, Vec<TcpStream>);

    /// Answer one connection per response, in order, with that exact response.
    ///
    /// With `keep_open`, the server holds every connection open until the test
    /// joins the handle. A client that reads until the server closes then
    /// fails with a timeout.
    fn serve(responses: Vec<String>, keep_open: bool) -> (u16, JoinHandle<Served>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            let mut open = Vec::new();
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let mut byte = [0u8; 1];
                while !request.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                stream.write_all(response.as_bytes()).unwrap();
                requests.push(String::from_utf8(request).unwrap());
                if keep_open {
                    open.push(stream);
                }
            }
            (requests, open)
        });
        (port, handle)
    }

    fn target(port: u16, path: &str) -> Target {
        Target {
            host: "127.0.0.1".to_string(),
            port,
            path: path.to_string(),
        }
    }

    #[test]
    fn fetch_key_sends_host_header() {
        let (port, server) = serve(
            vec![format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{KEY}",
                KEY.len()
            )],
            false,
        );
        assert_eq!(fetch_key("127.0.0.1", port).unwrap(), KEY);
        let (requests, _) = server.join().unwrap();
        assert!(requests[0].starts_with("GET /webos_rsa HTTP/1.1\r\n"));
        assert!(requests[0].contains(&format!("\r\nHost: 127.0.0.1:{port}\r\n")));
    }

    #[test]
    fn fetch_key_stops_at_content_length() {
        let (port, server) = serve(
            vec![format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{KEY}",
                KEY.len()
            )],
            true,
        );
        assert_eq!(fetch_key("127.0.0.1", port).unwrap(), KEY);
        server.join().unwrap();
    }

    #[test]
    fn fetch_key_decodes_chunked_body() {
        let (first, second) = KEY.split_at(20);
        let (port, server) = serve(
            vec![format!(
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
                 {:x};name=value\r\n{first}\r\n{:X}\r\n{second}\r\n0\r\nTrailer: x\r\n\r\n",
                first.len(),
                second.len()
            )],
            true,
        );
        assert_eq!(fetch_key("127.0.0.1", port).unwrap(), KEY);
        server.join().unwrap();
    }

    #[test]
    fn fetch_key_rejects_truncated_chunk() {
        let (port, server) = serve(
            vec!["HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n40\r\nshort".to_string()],
            false,
        );
        assert_eq!(
            fetch_key("127.0.0.1", port).unwrap_err().kind(),
            ErrorKind::UnexpectedEof
        );
        server.join().unwrap();
    }

    #[test]
    fn fetch_key_follows_redirect_to_another_port() {
        let (key_port, key_server) = serve(
            vec![format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{KEY}",
                KEY.len()
            )],
            false,
        );
        let (port, server) = serve(
            vec![format!(
                "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{key_port}/key?x=1\r\n\
                 Content-Length: 0\r\n\r\n"
            )],
            true,
        );
        assert_eq!(fetch_key("127.0.0.1", port).unwrap(), KEY);
        server.join().unwrap();
        let (requests, _) = key_server.join().unwrap();
        assert!(requests[0].starts_with("GET /key?x=1 HTTP/1.1\r\n"));
        assert!(requests[0].contains(&format!("\r\nHost: 127.0.0.1:{key_port}\r\n")));
    }

    #[test]
    fn fetch_key_follows_relative_redirect() {
        let (port, server) = serve(
            vec![
                "HTTP/1.1 301 Moved Permanently\r\nLocation: /keys/webos_rsa\r\n\r\n".to_string(),
                format!("HTTP/1.0 200 OK\r\n\r\n{KEY}"),
            ],
            false,
        );
        assert_eq!(fetch_key("127.0.0.1", port).unwrap(), KEY);
        let (requests, _) = server.join().unwrap();
        assert!(requests[1].starts_with("GET /keys/webos_rsa HTTP/1.1\r\n"));
    }

    #[test]
    fn fetch_key_gives_up_on_redirect_loop() {
        let (port, server) = serve(
            vec![
                "HTTP/1.1 302 Found\r\nLocation: /webos_rsa\r\n\r\n".to_string();
                MAX_REDIRECTS + 1
            ],
            false,
        );
        assert_eq!(
            fetch_key("127.0.0.1", port).unwrap_err().kind(),
            ErrorKind::InvalidData
        );
        server.join().unwrap();
    }

    #[test]
    fn fetch_key_rejects_large_response() {
        let (port, server) = serve(
            vec![format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                MAX_KEY_RESPONSE + 1
            )],
            true,
        );
        assert_eq!(
            fetch_key("127.0.0.1", port).unwrap_err().kind(),
            ErrorKind::InvalidData
        );
        server.join().unwrap();
    }

    #[test]
    fn redirect_resolves_relative_paths() {
        let from = target(9991, "/dir/webos_rsa?x=1");
        assert_eq!(from.redirect("/key").unwrap(), target(9991, "/key"));
        assert_eq!(from.redirect("key#part").unwrap(), target(9991, "/dir/key"));
        assert_eq!(
            from.redirect("//127.0.0.1:9922").unwrap(),
            target(9922, "/")
        );
    }

    #[test]
    fn redirect_keeps_to_the_same_host() {
        let from = target(9991, "/webos_rsa");
        assert_eq!(
            from.redirect("HTTP://127.0.0.1/key").unwrap(),
            target(80, "/key")
        );
        assert_eq!(
            from.redirect("http://10.0.0.1:9991/webos_rsa")
                .unwrap_err()
                .kind(),
            ErrorKind::InvalidData
        );
        assert_eq!(
            from.redirect("https://127.0.0.1/webos_rsa")
                .unwrap_err()
                .kind(),
            ErrorKind::Unsupported
        );
    }

    #[test]
    fn redirect_handles_ipv6() {
        let from = Target {
            host: "fe80::1".to_string(),
            port: 9991,
            path: "/webos_rsa".to_string(),
        };
        assert_eq!(from.authority(), "[fe80::1]:9991");
        let to = from.redirect("http://[FE80::1]:9922/webos_rsa").unwrap();
        assert_eq!((to.host.as_str(), to.port), ("fe80::1", 9922));
    }

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
