//! Platform-independent core of Hlas for Windows.
//!
//! Nothing in here touches Win32, audio devices or the network, so every
//! module is unit-tested on any OS with `cargo test`. The Windows layer in
//! `crate::win` wires these pieces to the real hardware.

pub mod audio;
pub mod config;
pub mod decode;
pub mod errors;
pub mod history;
pub mod hotkeys;
pub mod languages;
pub mod model;
pub mod press;
pub mod shared;
pub mod text;
pub mod version;
pub mod wav;
