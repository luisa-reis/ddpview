//! ddpview: a virtual LED panel. Everything here is plain Rust with no Bevy
//! in it; the window is the binary, `src/main.rs`.

pub mod announce;
pub mod cli;
pub mod ddp;
pub mod diffuser;
pub mod forward;
pub mod listener;
pub mod panel;
pub mod settings;
pub mod size;
pub mod wled;
