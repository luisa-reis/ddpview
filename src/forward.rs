//! Passing DDP on: every packet that arrives is sent, unchanged, to other
//! receivers as well, so that a panel on the desk shows what the window
//! does. Only the way out: what those receivers send back is not relayed.

use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs, UdpSocket};

use crate::ddp::PORT;

/// Reads a receiver's address: a host name or IP address, with `:PORT`
/// when it is not DDP's own (an IPv6 address with a port in brackets,
/// `[::1]:4049`). A name is looked up now, once; where it has both, the
/// IPv4 address is taken, as a sender given `localhost` usually does.
pub fn target(text: &str) -> io::Result<SocketAddr> {
    let text = text.trim();
    if let Ok(ip) = text.parse::<IpAddr>() {
        return Ok(SocketAddr::new(ip, PORT));
    }
    let has_port = text
        .rsplit_once(':')
        .is_some_and(|(_, port)| port.parse::<u16>().is_ok());
    let found: Vec<SocketAddr> = if has_port {
        text.to_socket_addrs()?.collect()
    } else {
        (text.trim_start_matches('[').trim_end_matches(']'), PORT)
            .to_socket_addrs()?
            .collect()
    };
    let addr = found
        .iter()
        .find(|addr| addr.is_ipv4())
        .or(found.first())
        .copied()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no address found"))?;
    if addr.port() == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "port 0 cannot be sent to",
        ));
    }
    Ok(addr)
}

/// Whether a packet sent to `target` would come back to a socket bound to
/// `local`, and round again for ever. Only what can be told from the
/// addresses: a socket bound to every address is not known to be reached
/// through this computer's own network address.
pub fn loops_back(local: SocketAddr, target: SocketAddr) -> bool {
    if local.port() != target.port() {
        return false;
    }
    let (local, target) = (local.ip(), target.ip());
    local == target
        || target.is_unspecified()
        || (target.is_loopback() && (local.is_loopback() || local.is_unspecified()))
}

/// The receivers packets go on to, each with a socket of its own on a port
/// the system picks, so that sending works whatever is listened on: a
/// socket bound to 127.0.0.1 cannot reach another computer.
pub struct Forwarder {
    targets: Vec<(SocketAddr, UdpSocket)>,
}

impl Forwarder {
    pub fn new(targets: &[SocketAddr]) -> io::Result<Self> {
        let targets = targets
            .iter()
            .map(|&target| {
                let any = match target {
                    SocketAddr::V4(_) => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                    SocketAddr::V6(_) => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
                };
                Ok((target, UdpSocket::bind(SocketAddr::new(any, 0))?))
            })
            .collect::<io::Result<_>>()?;
        Ok(Self { targets })
    }

    pub fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }

    /// Sends `datagram` to every receiver. A receiver that cannot be
    /// reached does not stop the others; the last such error is returned.
    pub fn send(&self, datagram: &[u8]) -> Result<(), (SocketAddr, io::Error)> {
        let mut failed = Ok(());
        for (target, socket) in &self.targets {
            if let Err(e) = socket.send_to(datagram, target) {
                failed = Err((*target, e));
            }
        }
        failed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(text: &str) -> SocketAddr {
        text.parse().unwrap()
    }

    #[test]
    fn a_target_takes_ddps_port_unless_given_one() {
        assert_eq!(target("192.168.1.50").unwrap(), addr("192.168.1.50:4048"));
        assert_eq!(target(" 10.0.0.7:4049 ").unwrap(), addr("10.0.0.7:4049"));
        assert_eq!(target("::1").unwrap(), addr("[::1]:4048"));
        assert_eq!(target("[::1]").unwrap(), addr("[::1]:4048"));
        assert_eq!(target("[::1]:4050").unwrap(), addr("[::1]:4050"));
        assert_eq!(
            target("localhost").unwrap(),
            addr("127.0.0.1:4048"),
            "IPv4 first, as senders do"
        );
        assert_eq!(target("localhost:5000").unwrap(), addr("127.0.0.1:5000"));
    }

    #[test]
    fn a_target_that_cannot_be_sent_to_is_an_error() {
        for text in ["", "10.0.0.7:0", "10.0.0.7:70000", "no such host.invalid"] {
            assert!(target(text).is_err(), "{text:?}");
        }
    }

    #[test]
    fn sending_back_to_the_same_socket_is_a_loop() {
        let local = addr("127.0.0.1:4048");
        assert!(loops_back(local, addr("127.0.0.1:4048")));
        assert!(loops_back(local, addr("0.0.0.0:4048")));
        assert!(!loops_back(local, addr("127.0.0.1:4049")), "another port");
        assert!(!loops_back(local, addr("192.168.1.50:4048")), "elsewhere");

        let every = addr("0.0.0.0:4048");
        assert!(loops_back(every, addr("127.0.0.1:4048")));
        assert!(!loops_back(every, addr("192.168.1.50:4048")));

        let lan = addr("192.168.1.20:4048");
        assert!(loops_back(lan, addr("192.168.1.20:4048")));
        assert!(
            !loops_back(lan, addr("127.0.0.1:4048")),
            "not heard: the socket is bound to another address"
        );
    }

    #[test]
    fn every_target_gets_the_datagram_unchanged() {
        let a = UdpSocket::bind("127.0.0.1:0").unwrap();
        let b = UdpSocket::bind("127.0.0.1:0").unwrap();
        let forwarder =
            Forwarder::new(&[a.local_addr().unwrap(), b.local_addr().unwrap()]).unwrap();
        assert!(!forwarder.is_empty());
        forwarder
            .send(&[0x41, 1, 0x0B, 1, 0, 0, 0, 0, 0, 3, 1, 2, 3])
            .unwrap();
        for socket in [a, b] {
            let mut got = [0; 64];
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let len = socket.recv(&mut got).unwrap();
            assert_eq!(got[..len], [0x41, 1, 0x0B, 1, 0, 0, 0, 0, 0, 3, 1, 2, 3]);
        }
    }
}
