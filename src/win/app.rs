//! Startup and shutdown.

use super::{cli, coordinator, engine, overlay, privacy, single_instance, state, tray, ui};
use crate::core::config::Config;

/// QA hooks, the Windows twin of macOS `HLAS_STEP`:
/// - `HLAS_STEP=N` opens the welcome tour at step N;
/// - `HLAS_SHOW=settings|history|result|pill-recording|pill-transcribing|pill-error`
///   opens one surface for screenshots.
fn qa_hooks() {
    if let Ok(step) = std::env::var("HLAS_STEP") {
        ui::send(ui::Command::OpenOnboarding(step.parse().unwrap_or(0)));
    }
    match std::env::var("HLAS_SHOW").as_deref() {
        Ok("settings") => ui::send(ui::Command::OpenSettings),
        Ok("history") => ui::send(ui::Command::OpenHistory),
        Ok("result") => ui::send(ui::Command::ShowResult {
            text:
                "Ahoj Evo, dneska proberu návrh, doplním podklady a večer ti to pošlu. Souhlasíš?"
                    .into(),
            original:
                "ahoj evo dneska proberu návrh doplním podklady a večer ti to pošlu souhlasíš"
                    .into(),
            message: "You switched applications. Your text is ready to copy.".into(),
        }),
        Ok("pill-recording") => {
            overlay::show(overlay::Pill::Recording { smart: false });
            std::thread::spawn(|| loop {
                let t = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as f32;
                overlay::set_level(0.25 + 0.25 * (t / 120.0).sin().abs());
                std::thread::sleep(std::time::Duration::from_millis(30));
            });
        }
        Ok("pill-transcribing") => overlay::show(overlay::Pill::Transcribing),
        Ok("pill-error") => overlay::show(overlay::Pill::Message {
            text: "Your Groq API key was rejected. Check it in Settings.".into(),
            error: true,
        }),
        _ => {}
    }
}

fn init_logging() {
    use simplelog::{ConfigBuilder, LevelFilter, WriteLogger};
    let Ok(dir) = Config::dir() else { return };
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("debug.log");
    // Keep the log small: rotate once it passes 1 MB.
    if std::fs::metadata(&path)
        .map(|m| m.len() > 1_000_000)
        .unwrap_or(false)
    {
        let _ = std::fs::rename(&path, dir.join("debug.old.log"));
    }
    if let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let config = ConfigBuilder::new().set_time_format_rfc3339().build();
        let _ = WriteLogger::init(LevelFilter::Info, config, file);
    }
}

pub fn main() -> i32 {
    let args: Vec<String> = std::env::args().collect();
    if cli::wants_cli(&args) {
        super::power::disable_throttling();
        return cli::run(&args);
    }

    let Some(_instance) = single_instance::acquire() else {
        single_instance::signal_existing();
        return 0;
    };
    init_logging();
    log::info!("Hlas for Windows {} starting", env!("CARGO_PKG_VERSION"));
    super::power::disable_throttling();

    let config = Config::load();
    state::init(config.clone());
    engine::model::migrate_legacy();
    // Persist the migrated 0.1.0 config in its new home.
    state::update_config(|_| {});
    // Launch-at-login must point at this exe, wherever it was installed.
    if super::autostart::is_enabled() {
        let _ = super::autostart::set(true);
    }

    overlay::start();
    coordinator::start(config.hotkey_vk);
    ui::start();

    let blocked = !privacy::mic_access().is_allowed();
    if !config.has_onboarded || blocked {
        ui::send(ui::Command::OpenOnboarding(if config.has_onboarded {
            1
        } else {
            0
        }));
    }
    qa_hooks();

    let code = match tray::run() {
        Ok(()) => 0,
        Err(e) => {
            log::error!("tray failed: {e}");
            1
        }
    };
    engine::local::unload_now();
    log::info!("Hlas quit");
    code
}
