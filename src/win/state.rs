//! Process-wide state shared by the coordinator, tray and windows: the
//! config, dictation history and the last result.

use crate::core::config::{Config, OutputMode};
use crate::core::history::{History, NewEntry};
use once_cell::sync::OnceCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

pub struct AppState {
    config: Mutex<Config>,
    history: Mutex<History>,
    last: Mutex<Option<LastResult>>,
    generation: AtomicU64,
}

#[derive(Debug, Clone)]
pub struct LastResult {
    pub text: String,
    pub raw: String,
}

static STATE: OnceCell<AppState> = OnceCell::new();

fn state() -> &'static AppState {
    STATE.get().expect("state::init must run first")
}

pub fn now_unix() -> i64 {
    chrono::Utc::now().timestamp()
}

pub fn init(config: Config) {
    let path = Config::dir()
        .map(|d| d.join("history.json"))
        .unwrap_or_else(|_| "history.json".into());
    let mut history = History::load(&path);
    history.prune(config.history_retention_days, now_unix());
    let _ = STATE.set(AppState {
        config: Mutex::new(config),
        history: Mutex::new(history),
        last: Mutex::new(None),
        generation: AtomicU64::new(1),
    });
}

pub fn config() -> Config {
    state().config.lock().unwrap().clone()
}

/// Changes the config, saves it and lets the tray, hotkey and windows react.
pub fn update_config(f: impl FnOnce(&mut Config)) {
    let vk = {
        let mut cfg = state().config.lock().unwrap();
        f(&mut cfg);
        if let Err(e) = cfg.save() {
            log::error!("config save failed: {e}");
        }
        cfg.hotkey_vk
    };
    super::hotkey::set_watch_vk(vk);
    state().generation.fetch_add(1, Ordering::SeqCst);
}

/// Increments whenever config or history changed; the tray and windows poll it.
pub fn generation() -> u64 {
    state().generation.load(Ordering::SeqCst)
}

pub fn bump() {
    state().generation.fetch_add(1, Ordering::SeqCst);
}

pub fn add_history(
    text: &str,
    raw: &str,
    mode: OutputMode,
    engine: &str,
    duration: Option<f64>,
) -> Option<u64> {
    let cfg = config();
    if !cfg.history_enabled {
        return None;
    }
    let id = {
        let mut h = state().history.lock().unwrap();
        let id = h.add(
            NewEntry {
                text,
                raw: Some(raw),
                mode,
                engine: Some(engine),
                duration,
            },
            now_unix(),
            cfg.history_retention_days,
        );
        if let Err(e) = h.save() {
            log::error!("history write failed: {e}");
        }
        id
    };
    bump();
    Some(id)
}

pub fn update_history(id: u64, text: &str, mode: OutputMode) {
    {
        let mut h = state().history.lock().unwrap();
        h.update(id, text, mode);
        if let Err(e) = h.save() {
            log::error!("history write failed: {e}");
        }
    }
    bump();
}

pub fn with_history<R>(f: impl FnOnce(&mut History) -> R) -> R {
    let mut h = state().history.lock().unwrap();
    let cfg = config();
    h.prune(cfg.history_retention_days, now_unix());
    f(&mut h)
}

pub fn clear_history() {
    state().history.lock().unwrap().clear();
    bump();
}

pub fn set_last(text: &str, raw: &str) {
    *state().last.lock().unwrap() = Some(LastResult {
        text: text.to_string(),
        raw: raw.to_string(),
    });
    bump();
}

pub fn last() -> Option<LastResult> {
    state().last.lock().unwrap().clone()
}
