# DDP, as ddpview receives it

DDP, the Distributed Display Protocol, is a small open protocol from
3waylabs for sending pixels to LED displays over UDP: a 10-byte header,
then raw bytes. This page is what `ddpview` does with it. The
specification is short: <http://www.3waylabs.com/ddp/> (plain HTTP only).

## The model

A display holds a buffer. A sender writes runs of bytes into it, each
packet saying where its run goes (a byte offset) and how long it is. The
buffer is not cleared between frames. A *push* tells the display to show
what the buffer holds.

DDP knows nothing of rows or of a matrix and its size. A frame is a flat
run of pixels, and `ddpview` lays it out row by row from the top left,
at the width it was given or has guessed ([using.md](using.md)).

## Packet format

Multi-byte fields are big-endian.

| Byte  | Field     | Meaning |
|-------|-----------|---------|
| 0     | flags     | `V V x T S R Q P`, below |
| 1     | sequence  | low 4 bits: 1–15, or 0 for unused; high 4 bits reserved |
| 2     | data type | `C R TTT SSS`, below |
| 3     | ID        | destination: 1 is the default output |
| 4–7   | offset    | where the data goes in the buffer, in bytes |
| 8–9   | length    | bytes of data that follow |
| 10–13 | timecode  | only when the T flag is set; the data then starts at byte 14 |

| Flag bit | Name     | Meaning |
|----------|----------|---------|
| 7–6      | version  | `01` for version 1, so every packet has `0x40` set |
| 4        | T        | a timecode follows the header |
| 3        | S        | the data names something in the display's storage |
| 2        | R        | a reply from a display |
| 1        | Q        | a query: read `length` bytes at `offset`; no data follows |
| 0        | P        | push: show the buffer now |

The data type's `TTT` bits are the kind (0 undefined, 1 RGB, 2 HSL,
3 RGBW, 4 greyscale) and `SSS` the bits per channel; RGB at 8 bits is
`0x0B`, RGBW `0x1B`. The specification limits a packet to 1,440 bytes of
data (480 RGB pixels) so that it fits a 1,500-byte Ethernet MTU.

## What ddpview does with each part

`src/ddp.rs` reads the header (`Packet::parse`) and keeps the buffer
(`Assembler`); `src/listener.rs` is the socket.

- **Version**: a datagram that is not version 1, is shorter than its
  header, or carries less data than its length says, is not DDP. It is
  counted, and the controls show the count. Bytes beyond the length are
  ignored.
- **Offset and length**: the data is written at the offset. The buffer is
  as long as the furthest byte any packet has written, which is how the
  number of pixels in a frame is known. Writes beyond 4 MiB (a 1024×1024
  RGBW panel) are ignored, since the offset is 32 bits and one stray
  packet must not ask for 4 GB.
- **Push**: a push hands the buffer to the window as one frame. Nothing is
  shown before it. A push may come with the last data packet or alone with
  no data. A sender that never pushes shows nothing.
- **Sequence**: used for one thing, the one the specification gives it. A
  packet whose number is not 0 and whose whole header matches the packet
  just before it is that packet sent twice for redundancy, and is dropped.
  Numbers are not otherwise compared: a sender may number each packet or
  give a whole frame one number, wrap from 15 to 1, or restart, and a
  lower number is never "old".
- **Timecode**: read, and the data taken from byte 14. The time itself is
  not used; a frame is shown when its push arrives.
- **Data type**: RGBW is four bytes a pixel, and the white is added to the
  red, green and blue. Everything else is read as RGB at three bytes a
  pixel, "undefined" included: older senders say `0x01` or nothing. Other
  bit depths are not supported. A change between RGB and RGBW starts the
  buffer again.
- **ID**: every ID writes the one buffer, except the JSON IDs (246
  control, 250 config, 251 status), which are left alone.
- **Query, reply, storage**: left alone. `ddpview` sends nothing back.

## Lost and late packets

UDP delivers a packet whole or not at all, and in any order. The buffer
behaves as a real display's does:

- A packet that arrives **out of order within its frame** costs nothing:
  each is written where its offset says.
- A **lost data packet** leaves its run of the buffer holding the frame
  before, so that part of the picture is one frame stale. The frame is
  still shown. The next frame mends it.
- A **lost push** leaves the frame unshown. The next push shows the buffer
  as it then stands.
- A data packet that arrives **after its frame's push** is written anyway
  and shows with the next push.

Nothing is held back to wait for a missing packet, and a frame with a hole
is not discarded: a display feed wants the newest picture, and senders
repeat the whole frame, so a fault lasts one frame either way. Holding
frames back would trade a stale patch for a stutter.

## Senders

- The buffer is started again when a packet comes from a **different
  address or port**, because the new sender's frames may be shorter and
  the buffer remembers its length. So two programs sending at once
  flicker and show each other's leftovers, as they would on a real
  display; one at a time.
- A sender that sends **only what changed** works, as long as its first
  frames reach the end of the panel so that the buffer is the right
  length.
- A frame that is **not collected** before the next arrives is replaced,
  not queued: the window shows the latest. The window is woken for each
  frame, so in practice every frame is drawn up to the screen's refresh
  rate.

## What it does not do

- Answer queries or announce itself (DDP's JSON status and discovery).
- Bit depths other than 8, HSL, or greyscale: all are read as 8-bit RGB.
- Use the timecode to synchronise.
- IPv6 unless asked: it binds `127.0.0.1` by default, so a sender given
  `localhost` must resolve that to IPv4. `--bind ::1` listens on IPv6.

## References

- DDP specification, 3waylabs: <http://www.3waylabs.com/ddp/>
- WLED's receiver, the behaviour most senders are written against:
  `handleDDPPacket` in `wled00/e131.cpp`, <https://github.com/wled/WLED>
