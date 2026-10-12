# CLAUDE.md

Context for Claude Code sessions working on ddpview.

## What it is

- A native desktop program that receives DDP (UDP 4048, the pixel
  protocol WLED devices accept) and shows the frames in a window: a
  virtual LED panel, for seeing what a sender puts out with no hardware.
- It stands alone: a general DDP viewer. It does not depend on, link to
  or mention any particular sender, in code or in documentation.
- Rust, with Bevy for the window and egui (bevy_egui) for the controls.
  Bevy was chosen on purpose, for the 3D rendering planned next; do not
  swap it for something lighter.
- The first version shows plain square pixels. What comes later, in
  order, is in the todo file, `../ddpview-todo.md` (outside the
  repository), which also records what was decided and why.

## Layout

The package is a library and a binary. **The library has no Bevy in it**
and must stay that way: the receiver is tested on its own, and the drawing
can be replaced without touching it.

Library (`src/lib.rs`):

- `src/ddp.rs` — the protocol: `Packet::parse` reads a header,
  `Assembler` is the display's buffer and gives a `Frame` on a push.
- `src/size.rs` — `Size`, the presets, `guess` (a size from a pixel
  count) and `scale_to_fit`.
- `src/listener.rs` — the UDP socket on a thread of its own; keeps the
  newest frame and the counts, and calls back for each frame.
- `src/settings.rs` — the TOML settings file.
- `src/cli.rs` — the command line, read by hand.

Binary (`src/main.rs`), all Bevy:

- `src/main.rs` — the app: plugins, the redraw policy, the wake-up from
  the listener's thread.
- `src/session.rs` — `Session`, the one resource the rest works from: the
  listener, size, last frame, what is remembered. The controls change it
  only through its methods, which also save.
- `src/view.rs` — the picture: the frame as a texture (`Picture`) on one
  sprite, placed in the `Room` the controls leave. The only file that
  draws the panel.
- `src/controls.rs` — the egui panel and the "nothing arriving" notice.

## Building

The toolchain is pinned in `rust-toolchain.toml`.

```sh
cargo run --release    # the window
./check.sh             # cargo fmt --check, clippy -D warnings, cargo test
```

Everything is built in release, tests included, so that one compiled Bevy
serves all of it. The first build takes a few minutes.

## Conventions

- Formatting is stock `cargo fmt`; lints are clippy's defaults with
  warnings denied. `./check.sh` must pass before a commit.
- Tests sit in a `mod tests` at the foot of the file they test, named as
  sentences (`a_lost_push_shows_nothing_until_the_next`), with a message
  on any assertion whose point is not obvious.
- Comments say why, in full sentences: `//!` at the top of each file for
  what it is for, `///` on items. Constants for protocol values carry the
  byte they come from.
- Errors on the command line and in the settings file say what is needed.
  A bad settings file never stops the program.
- `docs/ddp.md` owns the protocol and how each field is treated,
  `docs/using.md` the controls, the size guess, flags and the settings
  file; the README introduces the program and points at them. Update them
  with the behaviour.
- The Bevy and bevy_egui versions go together (bevy_egui's README names
  the Bevy it is for). Bevy's features are `2d` only: add `3d` when the 3D
  view starts.
- The window redraws on events, not continuously (`WinitSettings` in
  `main.rs`); the listener wakes it per frame. Anything that should
  animate by itself must ask for redraws.

## Checking the window

Run it and send to it; tests do not cover the window. Any DDP sender
will do, pointed at `127.0.0.1`.

Capturing the screen from a terminal needs macOS's Screen Recording
permission. Without it, add a temporary system that spawns Bevy's
`Screenshot::primary_window()` with `save_to_disk`, look at the PNG, and
take the system out again. A window that is hidden or covered renders
black that way.

## Version control

- jujutsu, colocated with git: use `jj`, never `git` commands.
- Small commits, one concern each, always with `-m`: a subject of at
  most 60 characters and an optional body of at most four lines of at
  most 60 characters. No attribution trailers.
- Keep the `main` bookmark on the latest commit
  (`jj bookmark set main -r @-`).
- Do not add a remote or push unless asked.
