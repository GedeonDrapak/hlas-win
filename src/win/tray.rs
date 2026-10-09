//! Tray icon and menu. tray-icon needs its messages pumped on the thread that
//! created it, so the tray owns the main thread. The menu is rebuilt whenever
//! the config changes (favourite languages, hotkey), and the icon switches
//! while a dictation is active.

use super::ui::{self, Command};
use super::{autostart, hotkey, shell, state, update};
use crate::core::config::{Engine, OutputMode};
use crate::core::{hotkeys, languages};
use anyhow::Result;
use std::time::Duration;
use tray_icon::menu::{
    CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu,
};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE, WM_QUIT,
};

const IDLE: &[u8] = include_bytes!("../../assets/tray-idle.rgba");
const BUSY: &[u8] = include_bytes!("../../assets/tray-recording.rgba");

fn icon(bytes: &[u8]) -> Option<Icon> {
    Icon::from_rgba(bytes.to_vec(), 32, 32).ok()
}

struct Items {
    menu: Menu,
    languages: Vec<(CheckMenuItem, String)>,
    more_languages: MenuItem,
    outputs: Vec<(CheckMenuItem, OutputMode)>,
    engines: Vec<(CheckMenuItem, Engine)>,
    last: MenuItem,
    history: MenuItem,
    import: MenuItem,
    settings: MenuItem,
    tour: MenuItem,
    launch: CheckMenuItem,
    updates: MenuItem,
    log: MenuItem,
    privacy: MenuItem,
    quit: MenuItem,
}

fn build_menu() -> Result<Items> {
    let cfg = state::config();
    let key = hotkeys::name(cfg.hotkey_vk);
    let menu = Menu::new();
    let header = MenuItem::new(format!("Hold {key} to dictate"), false, None);

    let lang_menu = Submenu::new("Language", true);
    let mut langs = Vec::new();
    let mut codes: Vec<String> = vec!["auto".into()];
    codes.extend(cfg.favorite_languages.iter().cloned());
    if !codes.contains(&cfg.language) {
        codes.push(cfg.language.clone());
    }
    for code in codes {
        let item = CheckMenuItem::new(languages::name(&code), true, code == cfg.language, None);
        lang_menu.append(&item)?;
        langs.push((item, code));
    }
    let more_languages = MenuItem::new("More languages...", true, None);
    lang_menu.append_items(&[&PredefinedMenuItem::separator(), &more_languages])?;

    let out_menu = Submenu::new("Output", true);
    let mut outputs = Vec::new();
    for mode in [OutputMode::Transcript, OutputMode::Smart] {
        let item = CheckMenuItem::new(mode.title(), true, mode == cfg.output_mode, None);
        out_menu.append(&item)?;
        outputs.push((item, mode));
    }

    let engine_menu = Submenu::new("Engine", true);
    let mut engines = Vec::new();
    for engine in Engine::ALL {
        let label = match engine {
            Engine::Local => "On this PC (local)",
            Engine::Groq => "Groq cloud",
            Engine::OpenAI => "OpenAI cloud",
        };
        let item = CheckMenuItem::new(label, true, engine == cfg.engine, None);
        engine_menu.append(&item)?;
        engines.push((item, engine));
    }

    let last = MenuItem::new("Show last result", state::last().is_some(), None);
    let history = MenuItem::new("History...", true, None);
    let import = MenuItem::new("Transcribe audio file...", true, None);
    let settings = MenuItem::new("Settings...", true, None);
    let tour = MenuItem::new("Welcome tour...", true, None);
    let launch = CheckMenuItem::new("Start at sign-in", true, autostart::is_enabled(), None);
    let updates = MenuItem::new("Check for updates...", true, None);
    let log = MenuItem::new("Open log folder", true, None);
    let privacy = MenuItem::new("Privacy", true, None);
    let quit = MenuItem::new("Quit Hlas", true, None);

    menu.append_items(&[
        &header,
        &PredefinedMenuItem::separator(),
        &lang_menu,
        &out_menu,
        &engine_menu,
        &PredefinedMenuItem::separator(),
        &last,
        &history,
        &import,
        &PredefinedMenuItem::separator(),
        &settings,
        &tour,
        &launch,
        &PredefinedMenuItem::separator(),
        &updates,
        &log,
        &privacy,
        &quit,
    ])?;
    Ok(Items {
        menu,
        languages: langs,
        more_languages,
        outputs,
        engines,
        last,
        history,
        import,
        settings,
        tour,
        launch,
        updates,
        log,
        privacy,
        quit,
    })
}

/// Handles one menu click. Returns false when Hlas should quit.
fn on_click(items: &Items, id: &MenuId) -> bool {
    if id == items.quit.id() {
        return false;
    }
    if let Some((_, code)) = items.languages.iter().find(|(i, _)| i.id() == id) {
        let code = code.clone();
        state::update_config(|c| c.language = code);
    } else if let Some((_, mode)) = items.outputs.iter().find(|(i, _)| i.id() == id) {
        let mode = *mode;
        state::update_config(|c| c.output_mode = mode);
    } else if let Some((_, engine)) = items.engines.iter().find(|(i, _)| i.id() == id) {
        let engine = *engine;
        state::update_config(|c| c.engine = engine);
        if engine == Engine::Local && !super::engine::model::present() {
            ui::send(Command::OpenSettings);
        }
    } else if id == items.more_languages.id() || id == items.settings.id() {
        ui::send(Command::OpenSettings);
    } else if id == items.last.id() {
        if let Some(last) = state::last() {
            ui::send(Command::ShowResult {
                text: last.text,
                original: last.raw,
                message: "Your last dictation".into(),
            });
        }
    } else if id == items.history.id() {
        ui::send(Command::OpenHistory);
    } else if id == items.import.id() {
        ui::send(Command::PickAudioFile);
    } else if id == items.tour.id() {
        ui::send(Command::OpenOnboarding(0));
    } else if id == items.launch.id() {
        let on = !autostart::is_enabled();
        if let Err(e) = autostart::set(on) {
            log::error!("autostart change failed: {e}");
        }
        state::update_config(|c| c.launch_at_login = on);
    } else if id == items.updates.id() {
        update::check_in_background(true);
    } else if id == items.log.id() {
        if let Ok(dir) = crate::core::config::Config::dir() {
            shell::open(&dir.to_string_lossy());
        }
    } else if id == items.privacy.id() {
        shell::open(shell::PRIVACY);
    }
    true
}

pub fn run() -> Result<()> {
    let mut items = build_menu()?;
    let tray: TrayIcon = TrayIconBuilder::new()
        .with_tooltip(tooltip())
        .with_icon(icon(IDLE).expect("tray icon"))
        .with_menu(Box::new(items.menu.clone()))
        .build()?;
    let mut seen = state::generation();
    let mut busy = false;
    let menu_rx = MenuEvent::receiver();
    let mut msg = MSG::default();
    loop {
        unsafe {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    return Ok(());
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        while let Ok(event) = menu_rx.try_recv() {
            if !on_click(&items, &event.id) {
                return Ok(());
            }
        }
        let generation = state::generation();
        if generation != seen {
            seen = generation;
            match build_menu() {
                Ok(fresh) => {
                    tray.set_menu(Some(Box::new(fresh.menu.clone())));
                    items = fresh;
                }
                Err(e) => log::error!("menu rebuild failed: {e}"),
            }
            let _ = tray.set_tooltip(Some(tooltip()));
        }
        let now_busy = hotkey::is_active();
        if now_busy != busy {
            busy = now_busy;
            let _ = tray.set_icon(icon(if busy { BUSY } else { IDLE }));
        }
        std::thread::sleep(Duration::from_millis(16));
    }
}

fn tooltip() -> String {
    format!(
        "Hlas - hold {} to dictate",
        hotkeys::name(state::config().hotkey_vk)
    )
}
