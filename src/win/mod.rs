//! Everything that touches Windows: hotkey hook, microphone, clipboard,
//! overlay pill, tray, native windows and the dictation coordinator.

pub mod app;
pub mod autostart;
pub mod cli;
pub mod clipboard;
pub mod coordinator;
pub mod engine;
pub mod hotkey;
pub mod inject;
pub mod keystore;
pub mod mic;
pub mod overlay;
pub mod privacy;
pub mod shell;
pub mod single_instance;
pub mod state;
pub mod tray;
pub mod ui;
pub mod update;

/// Wide, NUL-terminated string for Win32 calls.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
