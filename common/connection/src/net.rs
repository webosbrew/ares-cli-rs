//! Open the TCP connections to a device.
//!
//! The functions that reach a device take an `open` function, which makes one
//! connection to one address within a timeout. [`TcpStream::connect_timeout`]
//! fits, and the plain variants pass it. An embedder passes its own to choose
//! the network a socket goes out on. Android needs that when its default
//! network is not the one that reaches the device.

use std::io::{Error, ErrorKind};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Resolve `host` and connect to its addresses in turn, through `open`.
///
/// `host` may be an IPv6 address in brackets.
///
/// # Errors
///
/// Returns the error from the last address tried, or [`ErrorKind::NotFound`]
/// if `host` resolves to no address.
pub fn connect_host<F>(
    host: &str,
    port: u16,
    timeout: Duration,
    open: F,
) -> Result<TcpStream, Error>
where
    F: Fn(&SocketAddr, Duration) -> Result<TcpStream, Error>,
{
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let mut last_error = None;
    for address in (host, port).to_socket_addrs()? {
        match open(&address, timeout) {
            Ok(stream) => return Ok(stream),
            Err(e) => last_error = Some(e),
        }
    }
    Err(last_error
        .unwrap_or_else(|| Error::new(ErrorKind::NotFound, format!("Can't resolve {host}"))))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::net::TcpListener;

    use super::*;

    #[test]
    fn connects_through_open() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let opened = RefCell::new(Vec::new());
        let stream = connect_host("127.0.0.1", port, Duration::from_secs(5), |a, t| {
            opened.borrow_mut().push(*a);
            TcpStream::connect_timeout(a, t)
        })
        .unwrap();
        assert_eq!(stream.peer_addr().unwrap().port(), port);
        assert_eq!(opened.into_inner(), vec![stream.peer_addr().unwrap()]);
    }

    #[test]
    fn strips_ipv6_brackets() {
        let opened = RefCell::new(Vec::new());
        let result = connect_host("[::1]", 22, Duration::from_secs(1), |a, _| {
            opened.borrow_mut().push(*a);
            Err(Error::from(ErrorKind::ConnectionRefused))
        });
        assert_eq!(result.unwrap_err().kind(), ErrorKind::ConnectionRefused);
        assert_eq!(opened.into_inner(), vec!["[::1]:22".parse().unwrap()]);
    }
}
