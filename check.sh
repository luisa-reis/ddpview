#!/bin/sh
# Everything a change must pass: formatting, lints, tests. Release builds,
# so that this and `cargo run --release` share one compiled Bevy.
set -e
cd "$(dirname "$0")"
cargo fmt --check
cargo clippy --release --all-targets -- -D warnings
cargo test --release
