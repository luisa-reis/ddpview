//! The UDP socket, on a thread of its own so that nothing it does can hold
//! up a window: it reads datagrams, passes them on to any receivers to
//! forward to, feeds them to the assembler and keeps the newest frame for
//! whoever draws to collect.

use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::ddp::{Assembler, Frame, Packet};
use crate::forward::{loops_back, Forwarder};

/// How long a read waits before looking whether to stop.
const POLL: Duration = Duration::from_millis(50);
/// More than any datagram: UDP's limit.
const DATAGRAM: usize = 65_536;

/// What has arrived so far.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    /// Frames pushed since the socket was bound.
    pub frames: u64,
    /// Datagrams that were DDP, and those that were not.
    pub packets: u64,
    pub rejected: u64,
    /// Where the last packet came from.
    pub sender: Option<SocketAddr>,
    /// When the last frame was pushed.
    pub last_frame: Option<Instant>,
    /// Packets passed on to the receivers forwarded to, and of those, the
    /// ones that could not be sent to one or more of them.
    pub forwarded: u64,
    pub forward_failed: u64,
    /// The last time sending on failed: where to, and why.
    pub forward_error: Option<String>,
}

#[derive(Default)]
struct Shared {
    status: Status,
    /// The newest frame not yet collected. A frame nobody collected in
    /// time is replaced, not queued: a display shows the latest.
    frame: Option<Frame>,
}

/// A bound socket and the thread reading it. Dropping it stops the thread
/// and frees the port.
pub struct Listener {
    local: SocketAddr,
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Listener {
    /// Binds `addr` and starts reading. Port 0 picks a free one; see
    /// `local`. Every DDP packet is also sent, unchanged, to each address
    /// in `forward`; one that would come straight back here is refused.
    /// `on_frame` is called, on the listener's thread, each time a frame
    /// is ready to collect: the way to wake whoever draws without their
    /// having to keep looking.
    pub fn bind(
        addr: SocketAddr,
        forward: &[SocketAddr],
        on_frame: impl Fn() + Send + 'static,
    ) -> io::Result<Self> {
        let socket = UdpSocket::bind(addr)?;
        socket.set_read_timeout(Some(POLL))?;
        let local = socket.local_addr()?;
        if let Some(target) = forward.iter().find(|&&t| loops_back(local, t)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("forwarding to {target} would send every packet back here"),
            ));
        }
        let forwarder = Forwarder::new(forward)?;
        let shared = Arc::new(Mutex::new(Shared::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (shared, stop) = (Arc::clone(&shared), Arc::clone(&stop));
            thread::Builder::new()
                .name("ddp".into())
                .spawn(move || receive(&socket, &forwarder, &shared, &stop, &on_frame))?
        };
        Ok(Self {
            local,
            shared,
            stop,
            thread: Some(thread),
        })
    }

    /// The address the socket is bound to.
    pub fn local(&self) -> SocketAddr {
        self.local
    }

    pub fn status(&self) -> Status {
        lock(&self.shared).status.clone()
    }

    /// The newest frame, once.
    pub fn take_frame(&self) -> Option<Frame> {
        lock(&self.shared).frame.take()
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// A panic elsewhere while holding the lock leaves the data as good as it
/// was; carry on with it.
fn lock(shared: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    shared
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn receive(
    socket: &UdpSocket,
    forwarder: &Forwarder,
    shared: &Mutex<Shared>,
    stop: &AtomicBool,
    on_frame: &dyn Fn(),
) {
    let mut assembler = Assembler::new();
    let mut datagram = vec![0; DATAGRAM];
    let mut sender = None;
    while !stop.load(Ordering::Relaxed) {
        // A timeout is the chance to look at `stop`. Any other error (on
        // some systems a reply to nobody shows up as one) is not worth
        // ending for either: wait for the next datagram.
        let Ok((len, from)) = socket.recv_from(&mut datagram) else {
            continue;
        };
        let Ok(packet) = Packet::parse(&datagram[..len]) else {
            lock(shared).status.rejected += 1;
            continue;
        };
        // Passed on first, as it came, so that what is forwarded waits for
        // nothing here: whatever this receiver makes of it, the next
        // receiver makes its own.
        let forwarded = (!forwarder.is_empty()).then(|| forwarder.send(&datagram[..len]));
        // Another sender's frames may be another size, and the buffer
        // remembers its length: start it again.
        if sender != Some(from) {
            assembler.reset();
            sender = Some(from);
        }
        let frame = assembler.accept(&packet);
        let pushed = frame.is_some();
        {
            let mut shared = lock(shared);
            shared.status.packets += 1;
            shared.status.sender = sender;
            if let Some(result) = forwarded {
                shared.status.forwarded += 1;
                if let Err((target, e)) = result {
                    shared.status.forward_failed += 1;
                    shared.status.forward_error = Some(format!("{target}: {e}"));
                }
            }
            if let Some(frame) = frame {
                shared.status.frames += 1;
                shared.status.last_frame = Some(Instant::now());
                shared.frame = Some(frame);
            }
        }
        // With the lock let go: what is woken will want it.
        if pushed {
            on_frame();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(push: bool, offset: u32, data: &[u8]) -> Vec<u8> {
        let mut out = vec![0x40 | u8::from(push), 1, 0x0B, 1];
        out.extend_from_slice(&offset.to_be_bytes());
        out.extend_from_slice(&(data.len() as u16).to_be_bytes());
        out.extend_from_slice(data);
        out
    }

    /// Waits for the thread to have seen `packets` datagrams, good or bad.
    fn settle(listener: &Listener, datagrams: u64) -> Status {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            let status = listener.status();
            if status.packets + status.rejected >= datagrams {
                return status;
            }
            assert!(Instant::now() < until, "only {status:?} after five seconds");
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn frames_arrive_over_a_socket() {
        let woken = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let count = Arc::clone(&woken);
        let on_frame = move || {
            count.fetch_add(1, Ordering::Relaxed);
        };
        let listener = Listener::bind("127.0.0.1:0".parse().unwrap(), &[], on_frame).unwrap();
        assert_eq!(listener.status(), Status::default());
        assert!(listener.take_frame().is_none());

        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        sender
            .send_to(&packet(false, 0, &[1, 2, 3]), listener.local())
            .unwrap();
        // A digit's top two bits are 00, which is not version 1.
        sender
            .send_to(b"1234 is not ddp", listener.local())
            .unwrap();
        sender
            .send_to(&packet(true, 3, &[4, 5, 6]), listener.local())
            .unwrap();
        let status = settle(&listener, 3);
        assert_eq!((status.packets, status.rejected, status.frames), (2, 1, 1));
        assert_eq!(status.sender, Some(sender.local_addr().unwrap()));
        assert!(status.last_frame.is_some());
        assert_eq!(listener.take_frame().unwrap().rgb, [1, 2, 3, 4, 5, 6]);
        assert!(listener.take_frame().is_none(), "a frame is collected once");

        // A second sender starts the buffer again, at its own length.
        let other = UdpSocket::bind("127.0.0.1:0").unwrap();
        other
            .send_to(&packet(true, 0, &[7, 8, 9]), listener.local())
            .unwrap();
        let status = settle(&listener, 4);
        assert_eq!(status.frames, 2);
        assert_eq!(status.sender, Some(other.local_addr().unwrap()));
        assert_eq!(listener.take_frame().unwrap().rgb, [7, 8, 9]);
        assert_eq!(
            woken.load(Ordering::Relaxed),
            2,
            "once a frame, not once a packet"
        );
    }

    #[test]
    fn ddp_is_forwarded_unchanged_and_anything_else_is_not() {
        let downstream = UdpSocket::bind("127.0.0.1:0").unwrap();
        downstream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let listener = Listener::bind(
            "127.0.0.1:0".parse().unwrap(),
            &[downstream.local_addr().unwrap()],
            || {},
        )
        .unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        let first = packet(false, 0, &[1, 2, 3]);
        let last = packet(true, 3, &[4, 5, 6]);
        sender.send_to(&first, listener.local()).unwrap();
        sender
            .send_to(b"1234 is not ddp", listener.local())
            .unwrap();
        sender.send_to(&last, listener.local()).unwrap();

        let mut got = [0; 64];
        for sent in [&first, &last] {
            let len = downstream.recv(&mut got).unwrap();
            assert_eq!(&got[..len], &sent[..], "in order, as sent");
        }
        let status = settle(&listener, 3);
        assert_eq!((status.forwarded, status.forward_failed), (2, 0));
        assert_eq!(listener.take_frame().unwrap().rgb, [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn forwarding_back_to_itself_is_refused() {
        let free = UdpSocket::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let error = Listener::bind(free, &[free], || {}).err().expect("a loop");
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(error.to_string().contains("back here"), "{error}");
        Listener::bind(free, &[], || {}).expect("the port was let go");
    }

    #[test]
    fn dropping_it_frees_the_port() {
        let listener = Listener::bind("127.0.0.1:0".parse().unwrap(), &[], || {}).unwrap();
        let addr = listener.local();
        assert!(
            Listener::bind(addr, &[], || {}).is_err(),
            "taken while it lives"
        );
        drop(listener);
        Listener::bind(addr, &[], || {}).expect("free again");
    }
}
