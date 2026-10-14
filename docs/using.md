# Using ddpview

## The window

The picture is the panel: one square per pixel, with no smoothing, or
one LED per pixel (see the panel's type, below). Each pixel is a whole
number of screen pixels wide, the largest that fits, so every pixel is
the same size and the picture is centred with a margin left over. A
panel too big for the window is shrunk to fit, and only then are pixels
not whole.

The controls are down the right. **Tab** hides them, giving the panel the
whole window, and shows them again.

The window redraws when a frame arrives or when it is touched, and a few
times a second otherwise, whether or not it has the keyboard.

## The panel's type

The first control under **Panel** sets what the pixels look like:

- **Pixel Art**: plain square pixels touching each other, the colours
  exactly as sent. The default.
- **P3 SMD2121 LED**: the panel as an LED matrix at 3 mm pitch, seen
  face-on. Each pixel is a black SMD2121 package, 2.1 mm square, with a
  round lens over a red, a green and a blue die; each die is lit by its
  own channel, and its light spreads a little through the lens. The gaps
  between packages are dark board, and an unlit LED still shows faintly,
  so the grid stays visible on black.
- **P1.875 SMD1415 LED**: the same at 1.875 mm pitch, each pixel a black
  SMD1415 package, 1.4 mm wide and 1.5 mm tall. The packages fill more
  of each cell, so the gaps are narrower; the lens and dies are smaller
  in proportion.

For an LED type the controls give the panel's size in millimetres.

The LED look is fitted to the window as the plain pixels are, so the
pitch sets the proportion of LED to gap, not the size on screen. Small on
screen, the dies are a screen pixel or less, and the panel looks darker
than Pixel Art: lit dies cover less of each cell. White shows as three
coloured dots up close, as on a real panel.

The type is remembered.

## The panel's size

DDP does not say how wide a panel is, only how many pixels a frame has.
The size is set in one of three ways, all under **Panel**:

- **Typed**: the width and height fields, 1 to 1024 each. Drag a field, or
  click it and type.
- **Presets**: 8×8, 16×16, 32×8, 32×16, 32×32, 64×32, 64×64, 128×32,
  128×64, 192×64 and 128×128.
- **Guess from the frames**: on to start with. The size is guessed from
  the first frame, and again whenever a frame has a different number of
  pixels, as when another sender takes over.

Typing a size or picking a preset turns the guess off; ticking it turns it
back on. When the size is held and the frames have a different number of
pixels, the controls say so; the frame is cut off at the end of the panel,
or the rest of the panel is black.

### The guess

A count of pixels can be many rectangles: 4096 is 64×64, 128×32 and
256×16. The guess takes the likeliest, in this order:

1. A preset with exactly that many pixels; the squarest if there are
   several (64×64 for 4096, 16×16 for 256).
2. A square (48×48 for 2304).
3. A rectangle whose sides are both multiples of 8, wider than tall and no
   longer than 8 to 1; the squarest there is (48×40 for 1920).
4. Otherwise the count is **not a tidy rectangle**. The size is rounded up
   to the first of 32×32, 64×32, 64×64, 96×64, 96×96, 128×96 … that holds
   the pixels, and the rest of the panel is black. 4099 pixels become
   96×64 with 2045 unused; 1000 become 32×32 with 24 unused.

A rounded-up size is very likely wrong, and the controls say so in yellow
with the number of unused pixels: set the size by hand. Step 3 wants
multiples of 8 because panels are made that way; without it 1000 pixels
would be a confident 40×25.

A tall panel is never guessed (64×32, not 32×64), and neither is a tiling
of panels that is not the squarest (128×32 gives 64×64). Set those by
hand.

## Listening

`ddpview` listens on UDP port 4048 at `127.0.0.1`: only programs on the
same computer can send to it.

- **The port** can be changed in the controls. Type a number and press
  Return or click elsewhere; it listens there at once and remembers it. If
  the port cannot be had (another program has it), the controls say why
  and the old number is still remembered.
- **The address** is set by `--bind` when starting, for that run only, and
  is never remembered, so the port cannot be left open to the network by
  accident. `--bind 0.0.0.0` listens on every network the computer is on;
  an address of the computer's own listens on that network. Anything that
  can reach the port can then draw on the panel: DDP has no security.

## Forwarding

`--forward HOST[:PORT]` sends every DDP packet that arrives on to another
DDP receiver as well, unchanged: a panel on the desk, or another
`ddpview`, shows the same stream as the window.

```sh
ddpview --forward 192.168.1.50                # a WLED device, port 4048
ddpview --forward wled-desk.local --forward 127.0.0.1:4049
```

The host is a name or an IP address, with the port after a colon when it
is not 4048 (an IPv6 address with a port goes in brackets:
`[fd00::50]:4049`). A name is looked up once, at the start, and the IPv4
address taken if there is one. `--forward` can be given more than once.

Forwarding is for the run only, never remembered, like `--bind`: it sends
to the network. The controls show **Forwarding** with each receiver, the
packets passed on, and, if any could not be sent, how many and the last
error. [ddp.md](ddp.md#forwarding) has what exactly is passed on.

A receiver on this computer cannot share the port `ddpview` listens on:
run it on another port and forward to that. Forwarding to `ddpview`'s
own address and port is refused, as it would send every packet round in
a loop.

## Arriving

- **Frames a second**, measured over the last two seconds.
- **From**: the address and port of the last packet's sender.
- The pixels in a frame, and frames since the port was opened.
- A count of packets that were not DDP, if any came.

When no frame has arrived since the port was opened, the window says
"Nothing is arriving" across the panel with the address it is listening
on. When frames stop for 2.5 seconds (the time after which WLED gives up
on a sender), it says how long ago the last one came, and leaves that
frame in view.

## Flags

```
ddpview [--size WxH] [--port N] [--bind ADDRESS] [--forward HOST[:PORT]]
```

| Flag | Meaning |
|------|---------|
| `--size WxH` | Holds the panel at this size for the run, e.g. `--size 128x64` |
| `--port N` | Listens on this port for the run |
| `--bind ADDRESS` | Listens on this address; the default is `127.0.0.1` |
| `--forward HOST[:PORT]` | Also sends every DDP packet on to this receiver; may be repeated |
| `--help`, `--version` | Print and leave |

Flags override what is remembered without changing it; `--bind` and
`--forward` are never remembered. A size or port
then set in the window is remembered as usual.

## The settings file

| System | File |
|--------|------|
| macOS | `~/Library/Application Support/ddpview/settings.toml` |
| Linux | `~/.config/ddpview/settings.toml` |
| Windows | `%APPDATA%\ddpview\config\settings.toml` |

```toml
port = 4048
auto_size = true
width = 64
height = 64
panel = "pixel-art"
```

`auto_size` is "Guess from the frames"; `width` and `height` are the size
set, or the last guess, which is what the window opens at. `panel` is
`"pixel-art"`, `"p3-smd2121"` or `"p1.875-smd1415"`. The file is
written when one of these changes in the window, and can be edited by hand
while `ddpview` is not running. Settings left out take their defaults, the
ones above. A file that cannot be read is ignored, with a message on the
terminal, and replaced at the next change. Deleting it starts afresh.
