// Hlas for Windows - ultra-minimal push-to-talk dictation.
//
// Release builds hide the console: this is a tray app, not a CLI. The
// `--transcribe` mode attaches to the parent console when it has one.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#![cfg_attr(not(windows), allow(dead_code))]

mod core;
#[cfg(windows)]
mod win;

fn main() {
    #[cfg(windows)]
    {
        std::process::exit(win::app::main());
    }
    #[cfg(not(windows))]
    {
        eprintln!("Hlas for Windows only runs on Windows. `cargo test` checks the platform-independent core.");
        std::process::exit(1);
    }
}
