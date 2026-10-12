//! DDP (Distributed Display Protocol) receiver: reading a packet's header
//! and putting frames back together. A display holds a buffer; packets
//! write runs of bytes into it at an offset, and a push shows it. Nothing
//! here knows about sockets or windows.

use std::fmt;

/// The UDP port displays receive on.
pub const PORT: u16 = 4048;

/// Header is 10 bytes, all multi-byte fields big-endian.
const HEADER: usize = 10;
/// The timecode, when the T flag says one follows the header.
const TIMECODE: usize = 4;
/// Byte 0, top two bits: the protocol version, 1 in every packet.
const VERSION: u8 = 0xC0;
const VER1: u8 = 0x40;
/// Byte 0, the flags `T S R Q P`.
const TIMECODED: u8 = 0x10;
const STORAGE: u8 = 0x08;
const REPLY: u8 = 0x04;
const QUERY: u8 = 0x02;
const PUSH: u8 = 0x01;
/// Byte 1: the sequence number is the low four bits, the rest reserved.
const SEQUENCE: u8 = 0x0F;
/// Byte 2, bits 3..5: the kind of data. Only RGBW changes how bytes are
/// read; everything else, "undefined" included, is taken as RGB, which is
/// what WLED does and what older senders (type 0 or 1) rely on.
const KIND: u8 = 0x38;
const KIND_RGBW: u8 = 0x18;
/// Byte 3: the IDs that carry JSON, not pixels.
const ID_CONTROL: u8 = 246;
const ID_CONFIG: u8 = 250;
const ID_STATUS: u8 = 251;
/// The largest buffer a sender can make this hold: a 1024x1024 panel of
/// RGBW. The offset is 32 bits, and one stray packet must not ask for 4 GB.
pub const MAX_BUFFER: usize = 1024 * 1024 * 4;

/// A header's fields, but for the flags that take a packet out of the
/// pixels' conversation: sequence, type, ID, offset, length, push, timecode.
type Header = (u8, u8, u8, u32, u16, bool, Option<u32>);

/// One packet, its header read and its data borrowed from the datagram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Packet<'a> {
    /// Show the buffer now.
    pub push: bool,
    /// A request to read from the display; no data follows it.
    pub query: bool,
    /// Sent by a display in answer to a query.
    pub reply: bool,
    /// The data names something in the display's storage.
    pub storage: bool,
    /// 1 to 15, or 0 for "not used".
    pub sequence: u8,
    /// Byte 2 as sent, `C R TTT SSS`.
    pub data_type: u8,
    /// The destination: 1 is the default output.
    pub id: u8,
    /// Where the data goes in the buffer, in bytes.
    pub offset: u32,
    /// Bytes of data, or for a query how many to read.
    pub length: u16,
    /// When to show the frame, the middle 32 bits of an NTP time.
    pub timecode: Option<u32>,
    pub data: &'a [u8],
}

/// Why a datagram is not a DDP packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    /// Shorter than the header it claims to have.
    TooShort(usize),
    /// Not version 1; the value is the two version bits.
    Version(u8),
    /// The header promises more data than arrived.
    Truncated { declared: usize, arrived: usize },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort(len) => write!(f, "{len} bytes is too short for a DDP header"),
            Self::Version(v) => write!(f, "DDP version {v}, not 1"),
            Self::Truncated { declared, arrived } => {
                write!(f, "header says {declared} bytes of data, {arrived} arrived")
            }
        }
    }
}

impl std::error::Error for ParseError {}

impl<'a> Packet<'a> {
    /// Reads a datagram. Bytes after the declared length are ignored; a
    /// query's length counts what it asks for, so none are expected.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ParseError> {
        if bytes.len() < HEADER {
            return Err(ParseError::TooShort(bytes.len()));
        }
        let flags = bytes[0];
        if flags & VERSION != VER1 {
            return Err(ParseError::Version((flags & VERSION) >> 6));
        }
        let (timecode, start) = if flags & TIMECODED != 0 {
            let Some(t) = bytes.get(HEADER..HEADER + TIMECODE) else {
                return Err(ParseError::TooShort(bytes.len()));
            };
            (
                Some(u32::from_be_bytes([t[0], t[1], t[2], t[3]])),
                HEADER + TIMECODE,
            )
        } else {
            (None, HEADER)
        };
        let length = u16::from_be_bytes([bytes[8], bytes[9]]);
        let query = flags & QUERY != 0;
        let data = if query {
            &[][..]
        } else {
            let arrived = bytes.len() - start;
            bytes
                .get(start..start + usize::from(length))
                .ok_or(ParseError::Truncated {
                    declared: usize::from(length),
                    arrived,
                })?
        };
        Ok(Self {
            push: flags & PUSH != 0,
            query,
            reply: flags & REPLY != 0,
            storage: flags & STORAGE != 0,
            sequence: bytes[1] & SEQUENCE,
            data_type: bytes[2],
            id: bytes[3],
            offset: u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            length,
            timecode,
            data,
        })
    }

    /// Bytes per pixel: four for RGBW, three for anything else.
    pub fn channels(&self) -> usize {
        if self.data_type & KIND == KIND_RGBW {
            4
        } else {
            3
        }
    }

    /// Whether this is addressed to the pixels at all. Queries, replies,
    /// storage and the JSON IDs are other conversations.
    fn for_pixels(&self) -> bool {
        !(self.query || self.reply || self.storage)
            && !matches!(self.id, ID_CONTROL | ID_CONFIG | ID_STATUS)
    }

    /// What makes two packets the same one sent twice: the whole header.
    fn header(&self) -> Header {
        (
            self.sequence,
            self.data_type,
            self.id,
            self.offset,
            self.length,
            self.push,
            self.timecode,
        )
    }
}

/// A frame as it stood when a push arrived: RGB, three bytes a pixel, in
/// the order sent (a matrix is row-major, top left first).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub rgb: Vec<u8>,
    /// The push's timecode, if it had one. Kept for whoever wants it; the
    /// frame is handed over at once all the same.
    pub timecode: Option<u32>,
}

impl Frame {
    pub fn pixels(&self) -> usize {
        self.rgb.len() / 3
    }

    /// The frame as opaque RGBA for a panel of `pixels`: cut off if it is
    /// longer, black where it is shorter.
    pub fn rgba(&self, pixels: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(pixels * 4);
        for p in self.rgb.as_chunks::<3>().0.iter().take(pixels) {
            out.extend_from_slice(&[p[0], p[1], p[2], 255]);
        }
        out.resize(pixels * 4, 0);
        for alpha in out.iter_mut().skip(3).step_by(4) {
            *alpha = 255;
        }
        out
    }
}

/// The display's buffer. It behaves as a real display does and as the
/// specification describes: packets are written where their offset says,
/// in whatever order they come, the buffer is not cleared between frames,
/// and a push hands over what it holds. So:
///
/// - a packet that arrives out of order within its frame costs nothing;
/// - a lost data packet leaves that run showing the previous frame, for one
///   frame;
/// - a lost push leaves the frame unshown, and the next push shows the
///   buffer as it then stands;
/// - a data packet that arrives after its frame's push is written anyway
///   and shows with the next one.
///
/// Nothing is held back waiting for a missing packet: a display feed wants
/// the newest picture, and every sender here repeats the whole frame.
#[derive(Debug, Default)]
pub struct Assembler {
    buffer: Vec<u8>,
    /// Bytes per pixel of what the buffer holds.
    channels: usize,
    /// The last packet's header, to know its repeat.
    last: Option<Header>,
}

impl Assembler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets the buffer and its length: for a new sender, whose frames
    /// may be a different size.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The buffer's length in bytes: the furthest any packet has written.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Takes a packet, and gives back a frame if it pushed one.
    ///
    /// The sequence number is used for one thing, the one the specification
    /// gives it: a packet whose number is not 0 and whose header matches the
    /// packet just before it is that packet sent again for redundancy, and
    /// is dropped. Numbers are not otherwise compared, so a sender that
    /// numbers frames (as WLED's senders do) and one that numbers packets
    /// both work, and so does a sender that restarts its count.
    pub fn accept(&mut self, packet: &Packet) -> Option<Frame> {
        if !packet.for_pixels() {
            return None;
        }
        let header = packet.header();
        let repeat = packet.sequence != 0 && self.last == Some(header);
        self.last = Some(header);
        if repeat {
            return None;
        }
        self.write(packet);
        if !packet.push || self.buffer.is_empty() {
            return None;
        }
        Some(Frame {
            rgb: self.rgb(),
            timecode: packet.timecode,
        })
    }

    fn write(&mut self, packet: &Packet) {
        if packet.data.is_empty() {
            return;
        }
        let Ok(offset) = usize::try_from(packet.offset) else {
            return;
        };
        let end = offset.saturating_add(packet.data.len());
        if end > MAX_BUFFER {
            return;
        }
        // RGB after RGBW, or the other way: the old bytes mean nothing now.
        if self.channels != packet.channels() {
            self.buffer.clear();
            self.channels = packet.channels();
        }
        if self.buffer.len() < end {
            self.buffer.resize(end, 0);
        }
        self.buffer[offset..end].copy_from_slice(packet.data);
    }

    /// The buffer as RGB. A white channel is added to the other three, as
    /// a white LED beside them would add its light.
    fn rgb(&self) -> Vec<u8> {
        if self.channels != 4 {
            let whole = self.buffer.len() / 3 * 3;
            return self.buffer[..whole].to_vec();
        }
        self.buffer
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| {
                [
                    p[0].saturating_add(p[3]),
                    p[1].saturating_add(p[3]),
                    p[2].saturating_add(p[3]),
                ]
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RGB8: u8 = 0x0B;
    const RGBW8: u8 = 0x1B;

    /// A packet as a sender builds it.
    fn datagram(flags: u8, sequence: u8, data_type: u8, offset: u32, data: &[u8]) -> Vec<u8> {
        let mut out = vec![VER1 | (flags & !TIMECODED), sequence, data_type, 1];
        out.extend_from_slice(&offset.to_be_bytes());
        out.extend_from_slice(&(data.len() as u16).to_be_bytes());
        out.extend_from_slice(data);
        out
    }

    fn feed(assembler: &mut Assembler, bytes: &[u8]) -> Option<Frame> {
        assembler.accept(&Packet::parse(bytes).expect("a valid packet"))
    }

    /// A frame of `pixels`, each byte telling its place and its frame.
    fn picture(pixels: usize, frame: u8) -> Vec<u8> {
        (0..pixels * 3).map(|i| (i % 251) as u8 ^ frame).collect()
    }

    /// Sends a frame the way the senders this was written against do:
    /// nine even packets for 64x64, one sequence number, push on the last.
    fn send(
        assembler: &mut Assembler,
        rgb: &[u8],
        sequence: u8,
        skip: Option<usize>,
    ) -> Vec<Frame> {
        let each = 1368;
        let count = rgb.len().div_ceil(each);
        let mut frames = Vec::new();
        for (i, chunk) in rgb.chunks(each).enumerate() {
            if skip == Some(i) {
                continue;
            }
            let flags = if i + 1 == count { PUSH } else { 0 };
            let bytes = datagram(flags, sequence, RGB8, (i * each) as u32, chunk);
            frames.extend(feed(assembler, &bytes));
        }
        frames
    }

    #[test]
    fn reads_a_header() {
        let bytes = [
            0x41, 0x07, 0x0B, 0x01, 0x00, 0x00, 0x2A, 0xC0, 0x00, 0x03, 9, 8, 7,
        ];
        let p = Packet::parse(&bytes).unwrap();
        assert!(p.push && !p.query && !p.reply && !p.storage);
        assert_eq!((p.sequence, p.data_type, p.id), (7, 0x0B, 1));
        assert_eq!((p.offset, p.length), (10944, 3), "big-endian");
        assert_eq!(p.timecode, None);
        assert_eq!(p.data, [9, 8, 7]);
        assert_eq!(p.channels(), 3);

        let mut high = bytes;
        high[1] = 0xF7;
        assert_eq!(
            Packet::parse(&high).unwrap().sequence,
            7,
            "the high bits are reserved"
        );
        let mut long = bytes.to_vec();
        long.push(6);
        assert_eq!(
            Packet::parse(&long).unwrap().data,
            [9, 8, 7],
            "the length field decides"
        );
    }

    #[test]
    fn refuses_what_is_not_ddp() {
        assert_eq!(Packet::parse(&[0x41; 9]), Err(ParseError::TooShort(9)));
        assert_eq!(Packet::parse(&[0x01; 12]), Err(ParseError::Version(0)));
        assert_eq!(Packet::parse(&[0x81; 12]), Err(ParseError::Version(2)));
        let short = [0x41, 1, 0x0B, 1, 0, 0, 0, 0, 0, 6, 1, 2, 3];
        assert_eq!(
            Packet::parse(&short),
            Err(ParseError::Truncated {
                declared: 6,
                arrived: 3
            })
        );
        let timecoded = [0x51, 1, 0x0B, 1, 0, 0, 0, 0, 0, 0, 1, 2];
        assert_eq!(
            Packet::parse(&timecoded),
            Err(ParseError::TooShort(12)),
            "half a timecode"
        );
    }

    #[test]
    fn a_timecode_moves_the_data() {
        let bytes = [
            0x51, 1, 0x0B, 1, 0, 0, 0, 0, 0, 3, 0x12, 0x34, 0x56, 0x78, 10, 20, 30,
        ];
        let p = Packet::parse(&bytes).unwrap();
        assert_eq!(p.timecode, Some(0x1234_5678));
        assert_eq!(p.data, [10, 20, 30], "the data starts at byte 14");
        assert_eq!(p.length, 3, "the timecode is not counted in the length");

        let mut assembler = Assembler::new();
        let frame = assembler
            .accept(&p)
            .expect("shown at once, whatever the time");
        assert_eq!(frame.rgb, [10, 20, 30]);
        assert_eq!(frame.timecode, Some(0x1234_5678));

        let short = &bytes[..15];
        assert_eq!(
            Packet::parse(short),
            Err(ParseError::Truncated {
                declared: 3,
                arrived: 1
            })
        );
    }

    #[test]
    fn a_frame_is_put_together_from_its_packets() {
        let rgb = picture(64 * 64, 0);
        let mut assembler = Assembler::new();
        let frames = send(&mut assembler, &rgb, 1, None);
        assert_eq!(frames.len(), 1, "nine packets, one push");
        assert_eq!(frames[0].rgb, rgb);
        assert_eq!(frames[0].pixels(), 4096);
    }

    #[test]
    fn packets_may_come_in_any_order() {
        let rgb = picture(1000, 0);
        let mut assembler = Assembler::new();
        assert!(feed(&mut assembler, &datagram(0, 1, RGB8, 2000, &rgb[2000..])).is_none());
        assert!(feed(
            &mut assembler,
            &datagram(0, 1, RGB8, 1000, &rgb[1000..2000])
        )
        .is_none());
        let frame = feed(&mut assembler, &datagram(PUSH, 1, RGB8, 0, &rgb[..1000])).unwrap();
        assert_eq!(frame.rgb, rgb);
    }

    #[test]
    fn nothing_shows_before_the_push() {
        let mut assembler = Assembler::new();
        assert!(feed(&mut assembler, &datagram(0, 1, RGB8, 0, &[1, 2, 3])).is_none());
        assert!(feed(&mut assembler, &datagram(0, 1, RGB8, 3, &[4, 5, 6])).is_none());
        assert_eq!(assembler.len(), 6);
        // A push may travel alone, as it does when it is broadcast.
        let frame = feed(&mut assembler, &datagram(PUSH, 2, RGB8, 0, &[])).unwrap();
        assert_eq!(frame.rgb, [1, 2, 3, 4, 5, 6]);
        assert!(
            feed(&mut Assembler::new(), &datagram(PUSH, 1, RGB8, 0, &[])).is_none(),
            "nothing to show"
        );
    }

    #[test]
    fn a_lost_packet_leaves_the_frame_before_showing_through() {
        let (first, second) = (picture(64 * 64, 0), picture(64 * 64, 0xFF));
        let mut assembler = Assembler::new();
        send(&mut assembler, &first, 1, None);
        let frames = send(&mut assembler, &second, 2, Some(3));
        assert_eq!(frames.len(), 1, "the frame is still shown");
        let lost = 3 * 1368..4 * 1368;
        assert_eq!(
            frames[0].rgb[lost.clone()],
            first[lost.clone()],
            "the old pixels"
        );
        assert_eq!(frames[0].rgb[..lost.start], second[..lost.start]);
        assert_eq!(frames[0].rgb[lost.end..], second[lost.end..]);
        let frames = send(&mut assembler, &second, 3, None);
        assert_eq!(frames[0].rgb, second, "and the next frame mends it");
    }

    #[test]
    fn a_lost_push_shows_nothing_until_the_next() {
        let (first, second) = (picture(64 * 64, 0), picture(64 * 64, 0xFF));
        let mut assembler = Assembler::new();
        assert!(send(&mut assembler, &first, 1, Some(8)).is_empty());
        let frames = send(&mut assembler, &second, 2, None);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].rgb, second);
    }

    #[test]
    fn a_repeated_packet_is_dropped() {
        let mut assembler = Assembler::new();
        let push = datagram(PUSH, 5, RGB8, 0, &[1, 2, 3]);
        assert!(feed(&mut assembler, &push).is_some());
        assert!(
            feed(&mut assembler, &push).is_none(),
            "the same packet again"
        );
        assert!(feed(&mut assembler, &push).is_none(), "and again");
        let next = datagram(PUSH, 6, RGB8, 0, &[1, 2, 3]);
        assert!(
            feed(&mut assembler, &next).is_some(),
            "a new number is a new packet"
        );
    }

    #[test]
    fn sequence_numbers_are_not_otherwise_compared() {
        let mut assembler = Assembler::new();
        // One number for a whole frame: the offsets differ, so not repeats.
        assert!(feed(&mut assembler, &datagram(0, 9, RGB8, 0, &[1, 2, 3])).is_none());
        assert!(feed(&mut assembler, &datagram(PUSH, 9, RGB8, 3, &[4, 5, 6])).is_some());
        assert_eq!(assembler.len(), 6);
        // Zero is "not used": two identical packets are two packets.
        let unnumbered = datagram(PUSH, 0, RGB8, 0, &[7, 7, 7]);
        assert!(feed(&mut assembler, &unnumbered).is_some());
        assert!(feed(&mut assembler, &unnumbered).is_some());
        // A lower number is not "old": 15 wraps to 1, and senders restart.
        assert!(feed(&mut assembler, &datagram(PUSH, 15, RGB8, 0, &[8, 8, 8])).is_some());
        let wrapped = feed(&mut assembler, &datagram(PUSH, 1, RGB8, 0, &[9, 9, 9])).unwrap();
        assert_eq!(wrapped.rgb[..3], [9, 9, 9]);
    }

    #[test]
    fn other_conversations_leave_the_pixels_alone() {
        let mut assembler = Assembler::new();
        feed(&mut assembler, &datagram(0, 1, RGB8, 0, &[1, 2, 3]));
        let mut query = datagram(QUERY | PUSH, 2, 0, 0, &[]);
        query[3] = ID_STATUS;
        query[9] = 200;
        let parsed = Packet::parse(&query).unwrap();
        assert!(parsed.query && parsed.data.is_empty() && parsed.length == 200);
        assert!(assembler.accept(&parsed).is_none());
        assert!(feed(
            &mut assembler,
            &datagram(REPLY | PUSH, 3, RGB8, 0, &[9, 9, 9])
        )
        .is_none());
        assert!(feed(
            &mut assembler,
            &datagram(STORAGE | PUSH, 4, RGB8, 0, b"name")
        )
        .is_none());
        let mut config = datagram(PUSH, 5, 0, 0, b"{}");
        config[3] = ID_CONFIG;
        assert!(feed(&mut assembler, &config).is_none());
        let frame = feed(&mut assembler, &datagram(PUSH, 6, RGB8, 0, &[])).unwrap();
        assert_eq!(frame.rgb, [1, 2, 3]);
    }

    #[test]
    fn an_absurd_offset_is_ignored() {
        let mut assembler = Assembler::new();
        assert!(feed(
            &mut assembler,
            &datagram(PUSH, 1, RGB8, u32::MAX - 2, &[1, 2, 3])
        )
        .is_none());
        assert!(assembler.is_empty());
        let edge = (MAX_BUFFER - 3) as u32;
        assert!(feed(&mut assembler, &datagram(PUSH, 2, RGB8, edge, &[1, 2, 3])).is_some());
        assert_eq!(assembler.len(), MAX_BUFFER);
    }

    #[test]
    fn rgbw_is_four_bytes_a_pixel() {
        let mut assembler = Assembler::new();
        let data = [10, 20, 30, 5, 250, 0, 0, 10];
        let frame = feed(&mut assembler, &datagram(PUSH, 1, RGBW8, 0, &data)).unwrap();
        assert_eq!(
            frame.rgb,
            [15, 25, 35, 255, 10, 10],
            "white adds to each, up to full"
        );
        // Older senders say type 1 or nothing at all; both are RGB.
        for data_type in [0x00, 0x01, RGB8] {
            let packet = datagram(PUSH, 2, data_type, 0, &[1, 2, 3]);
            assert_eq!(Packet::parse(&packet).unwrap().channels(), 3);
        }
        let frame = feed(&mut assembler, &datagram(PUSH, 3, RGB8, 0, &[1, 2, 3])).unwrap();
        assert_eq!(
            frame.rgb,
            [1, 2, 3],
            "a change of type starts the buffer again"
        );
    }

    #[test]
    fn reset_forgets_the_length() {
        let mut assembler = Assembler::new();
        send(&mut assembler, &picture(64 * 64, 0), 1, None);
        assembler.reset();
        let small = picture(32 * 32, 0);
        let frames = send(&mut assembler, &small, 1, None);
        assert_eq!(frames[0].pixels(), 1024);
    }

    #[test]
    fn a_frame_fills_a_panel_of_any_size() {
        let frame = Frame {
            rgb: vec![1, 2, 3, 4, 5, 6],
            timecode: None,
        };
        assert_eq!(frame.rgba(2), [1, 2, 3, 255, 4, 5, 6, 255]);
        assert_eq!(frame.rgba(1), [1, 2, 3, 255], "cut off");
        assert_eq!(
            frame.rgba(3),
            [1, 2, 3, 255, 4, 5, 6, 255, 0, 0, 0, 255],
            "black where short"
        );
    }
}
