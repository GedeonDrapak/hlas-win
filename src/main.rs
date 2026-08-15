// Hlas for Windows - ultra-minimal push-to-talk dictation.
//
// Hide the console window in release: this is a tray app, not a CLI.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod autostart;
mod config;
mod hotkey;
mod inject;
mod keystore;
mod overlay;
mod settings;
mod transcribe;
mod tray;

use config::Config;
use hotkey::KeyEdge;
use std::sync::{Arc, Mutex};

fn main() -> anyhow::Result<()> {
    init_logging();
    log::info!("Hlas starting");

    let config = Arc::new(Mutex::new(Config::load()));

    // Overlay pill thread + control channel.
    let overlay_tx = overlay::start();

    // Push-to-talk hook.
    let watch_vk = config.lock().unwrap().hotkey_vk;
    let key_events = hotkey::start(watch_vk);

    // Dictation control loop: key edges drive record -> transcribe -> paste.
    spawn_control_loop(config.clone(), overlay_tx.clone(), key_events);

    // Tray icon owns the main-thread message loop and menu handling.
    tray::run(config)
}

fn spawn_control_loop(
    config: Arc<Mutex<Config>>,
    overlay_tx: crossbeam_channel::Sender<overlay::State>,
    key_events: crossbeam_channel::Receiver<KeyEdge>,
) {
    std::thread::spawn(move || {
        let mut recorder: Option<audio::Recorder> = None;
        for edge in key_events.iter() {
            match edge {
                KeyEdge::Down => match audio::Recorder::start() {
                    Ok(r) => {
                        recorder = Some(r);
                        let _ = overlay_tx.send(overlay::State::Recording);
                    }
                    Err(e) => log::error!("record start failed: {e}"),
                },
                KeyEdge::Up => {
                    let Some(r) = recorder.take() else { continue };
                    let _ = overlay_tx.send(overlay::State::Transcribing);
                    let samples = match r.stop() {
                        Ok(s) => s,
                        Err(e) => {
                            log::error!("record stop failed: {e}");
                            let _ = overlay_tx.send(overlay::State::Hidden);
                            continue;
                        }
                    };
                    let cfg = config.lock().unwrap().clone();
                    match transcribe::transcribe(&cfg, &samples) {
                        Ok(text) if !text.is_empty() => {
                            if let Err(e) = inject::paste_text(&text) {
                                log::error!("paste failed: {e}");
                            }
                        }
                        Ok(_) => log::info!("empty transcript"),
                        Err(e) => log::error!("transcription failed: {e}"),
                    }
                    let _ = overlay_tx.send(overlay::State::Hidden);
                }
            }
        }
    });
}

fn init_logging() {
    use simplelog::{Config as LogConfig, LevelFilter, WriteLogger};
    if let Ok(dir) = Config::dir() {
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("debug.log"))
        {
            let _ = WriteLogger::init(LevelFilter::Info, LogConfig::default(), file);
        }
    }
}
