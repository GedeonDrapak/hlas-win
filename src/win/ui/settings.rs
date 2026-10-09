//! Settings: dictation, engine, vocabulary, replacements, history, about.
//! Two columns so it fits a 1366x768 laptop at 125 % scaling.

use super::controls::{self as c, Theme};
use crate::core::config::{Engine, OutputMode};
use crate::core::text::{format_replacements, parse_replacements};
use crate::core::{hotkeys, languages};
use crate::win::engine::model;
use crate::win::{autostart, keystore, mic, shell, state};
use native_windows_gui as nwg;
use nwg::NwgError;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const MEMORY: [(u64, &str); 3] = [
    (0, "Save memory - unload at once"),
    (180, "Balanced - keep 3 minutes"),
    (600, "Fast start - keep 10 minutes"),
];
const RETENTION: [(u32, &str); 4] = [
    (0, "Until cleared"),
    (1, "1 day"),
    (7, "7 days"),
    (30, "30 days"),
];

pub struct SettingsWindow {
    window: nwg::Window,
    hotkey: nwg::ComboBox<String>,
    language: nwg::ComboBox<String>,
    favorites: nwg::TextInput,
    output: nwg::ComboBox<String>,
    microphone: nwg::ComboBox<String>,
    launch: nwg::CheckBox,
    local: nwg::RadioButton,
    groq: nwg::RadioButton,
    openai: nwg::RadioButton,
    model_status: nwg::Label,
    model_button: nwg::Button,
    model_progress: nwg::ProgressBar,
    memory: nwg::ComboBox<String>,
    groq_key: nwg::TextInput,
    openai_key: nwg::TextInput,
    vocabulary: nwg::TextBox,
    replacements: nwg::TextBox,
    history_on: nwg::CheckBox,
    retention: nwg::ComboBox<String>,
    open_history: nwg::Button,
    clear_history: nwg::Button,
    open_log: nwg::Button,
    privacy: nwg::Button,
    updates: nwg::Button,
    save: nwg::Button,
    status: nwg::Label,
    timer: nwg::AnimationTimer,
    mic_names: RefCell<Vec<String>>,
    clear_armed: Cell<bool>,
    _static: Vec<nwg::Label>,
    handler: RefCell<Option<nwg::EventHandler>>,
}

fn language_items() -> Vec<String> {
    std::iter::once(languages::AUTO)
        .chain(languages::ALL.iter().copied())
        .map(|(code, name)| format!("{name} ({code})"))
        .collect()
}

fn language_code(index: usize) -> String {
    if index == 0 {
        "auto".into()
    } else {
        languages::ALL
            .get(index - 1)
            .map(|(c, _)| c.to_string())
            .unwrap_or_else(|| "auto".into())
    }
}

fn language_index(code: &str) -> usize {
    languages::ALL
        .iter()
        .position(|(c, _)| *c == code)
        .map(|i| i + 1)
        .unwrap_or(0)
}

impl SettingsWindow {
    pub fn build(theme: Rc<Theme>) -> Result<Rc<SettingsWindow>, NwgError> {
        let t = &*theme;
        let w = c::window(t, "Hlas Settings", (780, 570), false)?;
        let mut stat = Vec::new();
        let (lx, rx) = (24, 410);
        let section =
            |text: &str, x: i32, y: i32| c::label(&w, text, (x, y), (340, 18), &t.section);

        // Left column: dictation.
        stat.push(section("DICTATION", lx, 18)?);
        stat.push(c::label(
            &w,
            "Push-to-talk key",
            (lx, 46),
            (150, 22),
            &t.body,
        )?);
        let hotkey = c::combo(
            &w,
            hotkeys::KEYS.iter().map(|(_, n)| n.to_string()).collect(),
            None,
            (lx + 160, 42),
            196,
            &t.body,
        )?;
        stat.push(c::label(&w, "Language", (lx, 80), (150, 22), &t.body)?);
        let language = c::combo(&w, language_items(), None, (lx + 160, 76), 196, &t.body)?;
        stat.push(c::label(
            &w,
            "Tray languages",
            (lx, 114),
            (150, 22),
            &t.body,
        )?);
        let favorites = c::input(&w, "", (lx + 160, 110), (196, 26), &t.body, false)?;
        stat.push(c::label(&w, "Output", (lx, 148), (150, 22), &t.body)?);
        let output = c::combo(
            &w,
            vec!["Transcript".into(), "Smart text".into()],
            None,
            (lx + 160, 144),
            196,
            &t.body,
        )?;
        stat.push(c::label(
            &w,
            "Hold Shift when you start to get a plain transcript once.",
            (lx, 174),
            (356, 18),
            &t.small,
        )?);
        stat.push(c::label(&w, "Microphone", (lx, 202), (150, 22), &t.body)?);
        let microphone = c::combo(&w, vec![], None, (lx + 160, 198), 196, &t.body)?;
        let launch = c::check(
            &w,
            t,
            "Start Hlas when I sign in",
            (lx, 234),
            (356, 24),
            false,
        )?;

        // Left column: engine.
        stat.push(section("ENGINE", lx, 274)?);
        let local = c::radio(&w, t, Engine::Local.label(), (lx, 296), (356, 24), true)?;
        let groq = c::radio(&w, t, Engine::Groq.label(), (lx, 320), (356, 24), false)?;
        let openai = c::radio(&w, t, Engine::OpenAI.label(), (lx, 344), (356, 24), false)?;
        let model_status = c::label(&w, "", (lx, 378), (236, 22), &t.small)?;
        let model_button = c::button(&w, "Download", (lx + 240, 372), (116, 28), &t.body)?;
        let model_progress = c::progress(&w, (lx, 404), (356, 6))?;
        stat.push(c::label(&w, "Model memory", (lx, 422), (150, 22), &t.body)?);
        let memory = c::combo(
            &w,
            MEMORY.iter().map(|(_, n)| n.to_string()).collect(),
            None,
            (lx + 160, 418),
            196,
            &t.body,
        )?;
        stat.push(c::label(&w, "Groq API key", (lx, 458), (150, 22), &t.body)?);
        let groq_key = c::input(&w, "", (lx + 160, 454), (196, 26), &t.body, true)?;
        stat.push(c::label(
            &w,
            "OpenAI API key",
            (lx, 492),
            (150, 22),
            &t.body,
        )?);
        let openai_key = c::input(&w, "", (lx + 160, 488), (196, 26), &t.body, true)?;
        stat.push(c::label(
            &w,
            "Keys stay in Windows Credential Manager.",
            (lx, 518),
            (356, 18),
            &t.small,
        )?);

        // Right column.
        stat.push(section("VOCABULARY", rx, 18)?);
        stat.push(c::label(
            &w,
            "Names and jargon to recognize, one per line.",
            (rx, 38),
            (346, 18),
            &t.small,
        )?);
        let vocabulary = c::text_box(&w, (rx, 58), (346, 92), &t.body, false)?;
        stat.push(section("REPLACEMENTS", rx, 162)?);
        stat.push(c::label(
            &w,
            "Exact spellings, one per line:  eden makers => Edenmakers",
            (rx, 182),
            (346, 18),
            &t.small,
        )?);
        let replacements = c::text_box(&w, (rx, 202), (346, 92), &t.body, false)?;
        stat.push(section("HISTORY", rx, 308)?);
        let history_on = c::check(
            &w,
            t,
            "Save dictation history on this PC",
            (rx, 328),
            (346, 24),
            true,
        )?;
        stat.push(c::label(&w, "Keep history", (rx, 362), (130, 22), &t.body)?);
        let retention = c::combo(
            &w,
            RETENTION.iter().map(|(_, n)| n.to_string()).collect(),
            None,
            (rx + 150, 358),
            196,
            &t.body,
        )?;
        let open_history = c::button(&w, "Open history", (rx, 394), (168, 30), &t.body)?;
        let clear_history = c::button(&w, "Clear history", (rx + 178, 394), (168, 30), &t.body)?;
        stat.push(section("ABOUT", rx, 438)?);
        stat.push(c::label(
            &w,
            &format!("Hlas for Windows {}", env!("CARGO_PKG_VERSION")),
            (rx, 458),
            (346, 20),
            &t.body,
        )?);
        let open_log = c::button(&w, "Log folder", (rx, 482), (110, 28), &t.small)?;
        let privacy = c::button(&w, "Privacy", (rx + 118, 482), (110, 28), &t.small)?;
        let updates = c::button(
            &w,
            "Check for updates",
            (rx + 236, 482),
            (110, 28),
            &t.small,
        )?;

        let status = c::label(&w, "", (rx, 528), (220, 22), &t.small)?;
        let save = c::button(&w, "Save", (rx + 236, 522), (110, 32), &t.bold)?;
        let timer = c::timer(&w, 400)?;

        let ui = Rc::new(SettingsWindow {
            window: w,
            hotkey,
            language,
            favorites,
            output,
            microphone,
            launch,
            local,
            groq,
            openai,
            model_status,
            model_button,
            model_progress,
            memory,
            groq_key,
            openai_key,
            vocabulary,
            replacements,
            history_on,
            retention,
            open_history,
            clear_history,
            open_log,
            privacy,
            updates,
            save,
            status,
            timer,
            mic_names: RefCell::new(Vec::new()),
            clear_armed: Cell::new(false),
            _static: stat,
            handler: RefCell::new(None),
        });

        let weak = Rc::downgrade(&ui);
        let handler = nwg::full_bind_event_handler(&ui.window.handle, move |evt, data, handle| {
            let Some(ui) = weak.upgrade() else { return };
            use nwg::Event as E;
            match evt {
                E::OnWindowClose if handle == ui.window.handle => {
                    if let nwg::EventData::OnWindowClose(d) = data {
                        d.close(false);
                    }
                    ui.window.set_visible(false);
                }
                E::OnButtonClick if handle == ui.save.handle => ui.save(),
                E::OnButtonClick if handle == ui.model_button.handle => {
                    if model::status().active {
                        model::cancel();
                    } else {
                        model::start();
                    }
                    ui.refresh_model();
                }
                E::OnButtonClick if handle == ui.open_history.handle => {
                    super::send(super::Command::OpenHistory)
                }
                E::OnButtonClick if handle == ui.clear_history.handle => {
                    if ui.clear_armed.get() {
                        state::clear_history();
                        ui.clear_armed.set(false);
                        ui.clear_history.set_text("Clear history");
                        ui.status.set_text("History cleared.");
                    } else {
                        ui.clear_armed.set(true);
                        ui.clear_history.set_text("Click again to clear");
                    }
                }
                E::OnButtonClick if handle == ui.open_log.handle => {
                    if let Ok(dir) = crate::core::config::Config::dir() {
                        shell::open(&dir.to_string_lossy());
                    }
                }
                E::OnButtonClick if handle == ui.privacy.handle => shell::open(shell::PRIVACY),
                E::OnButtonClick if handle == ui.updates.handle => {
                    crate::win::update::check_in_background(true)
                }
                E::OnTimerTick if handle == ui.timer.handle => {
                    if ui.window.visible() {
                        ui.refresh_model();
                    }
                }
                _ => {}
            }
        });
        *ui.handler.borrow_mut() = Some(handler);
        ui.timer.start();
        Ok(ui)
    }

    pub fn show(&self) {
        self.load();
        c::present(&self.window);
    }

    /// Fills every control from the current config.
    fn load(&self) {
        let cfg = state::config();
        self.hotkey.set_selection(
            hotkeys::KEYS
                .iter()
                .position(|(vk, _)| *vk == cfg.hotkey_vk)
                .or(Some(0)),
        );
        self.language
            .set_selection(Some(language_index(&cfg.language)));
        self.favorites.set_text(&cfg.favorite_languages.join(", "));
        self.output
            .set_selection(Some(if cfg.output_mode == OutputMode::Smart {
                1
            } else {
                0
            }));

        let mut names = vec![String::new()];
        names.extend(mic::device_names());
        if !cfg.input_device.is_empty() && !names.contains(&cfg.input_device) {
            names.push(cfg.input_device.clone());
        }
        let labels: Vec<String> = names
            .iter()
            .map(|n| {
                if n.is_empty() {
                    "Windows default".to_string()
                } else {
                    n.clone()
                }
            })
            .collect();
        self.microphone.set_collection(labels);
        self.microphone.set_selection(
            names
                .iter()
                .position(|n| *n == cfg.input_device)
                .or(Some(0)),
        );
        *self.mic_names.borrow_mut() = names;

        c::set_checked(&self.launch, autostart::is_enabled());
        c::set_radio(&self.local, cfg.engine == Engine::Local);
        c::set_radio(&self.groq, cfg.engine == Engine::Groq);
        c::set_radio(&self.openai, cfg.engine == Engine::OpenAI);
        self.memory.set_selection(Some(
            MEMORY
                .iter()
                .position(|(s, _)| *s == cfg.model_keep_alive_secs)
                .unwrap_or(1),
        ));
        self.groq_key
            .set_text(&keystore::get_key(keystore::GROQ).unwrap_or_default());
        self.openai_key
            .set_text(&keystore::get_key(keystore::OPENAI).unwrap_or_default());
        c::set_box_text(&self.vocabulary, &cfg.vocabulary.join("\n"));
        c::set_box_text(&self.replacements, &format_replacements(&cfg.replacements));
        c::set_checked(&self.history_on, cfg.history_enabled);
        self.retention.set_selection(Some(
            RETENTION
                .iter()
                .position(|(d, _)| *d == cfg.history_retention_days)
                .unwrap_or(0),
        ));
        self.clear_armed.set(false);
        self.clear_history.set_text("Clear history");
        self.status.set_text("");
        self.refresh_model();
    }

    fn refresh_model(&self) {
        let s = model::status();
        if model::present() {
            self.model_status
                .set_text("Local model ready (547 MB, large-v3-turbo)");
            self.model_button.set_text("Ready");
            self.model_button.set_enabled(false);
            self.model_progress.set_pos(100);
        } else if s.active {
            let text = if s.verifying {
                "Verifying download...".to_string()
            } else {
                format!("Downloading model... {}%", s.percent)
            };
            self.model_status.set_text(&text);
            self.model_button.set_text("Cancel");
            self.model_button.set_enabled(true);
            self.model_progress.set_pos(s.percent as u32);
        } else {
            let text = s
                .error
                .unwrap_or_else(|| "Local engine needs a one-time 547 MB download.".into());
            self.model_status.set_text(&text);
            self.model_button.set_text("Download");
            self.model_button.set_enabled(true);
            self.model_progress.set_pos(0);
        }
    }

    fn save(&self) {
        let engine = if c::radio_on(&self.groq) {
            Engine::Groq
        } else if c::radio_on(&self.openai) {
            Engine::OpenAI
        } else {
            Engine::Local
        };
        let hotkey = self
            .hotkey
            .selection()
            .and_then(|i| hotkeys::KEYS.get(i))
            .map(|(vk, _)| *vk)
            .unwrap_or(hotkeys::DEFAULT_VK);
        let language = language_code(self.language.selection().unwrap_or(0));
        let favorites = languages::parse_list(&self.favorites.text());
        let output = if self.output.selection() == Some(1) {
            OutputMode::Smart
        } else {
            OutputMode::Transcript
        };
        let device = self
            .microphone
            .selection()
            .and_then(|i| self.mic_names.borrow().get(i).cloned())
            .unwrap_or_default();
        let memory = self
            .memory
            .selection()
            .and_then(|i| MEMORY.get(i))
            .map(|(s, _)| *s)
            .unwrap_or(180);
        let vocabulary: Vec<String> = c::box_text(&self.vocabulary)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        let replacements = parse_replacements(&c::box_text(&self.replacements));
        let retention = self
            .retention
            .selection()
            .and_then(|i| RETENTION.get(i))
            .map(|(d, _)| *d)
            .unwrap_or(0);
        let history_on = c::is_checked(&self.history_on);
        let launch = c::is_checked(&self.launch);

        keystore::persist(keystore::GROQ, &self.groq_key.text());
        keystore::persist(keystore::OPENAI, &self.openai_key.text());
        if let Err(e) = autostart::set(launch) {
            log::error!("autostart change failed: {e}");
        }
        state::update_config(|cfg| {
            cfg.engine = engine;
            cfg.hotkey_vk = hotkey;
            cfg.language = language;
            if !favorites.is_empty() {
                cfg.favorite_languages = favorites;
            }
            cfg.output_mode = output;
            cfg.input_device = device;
            cfg.model_keep_alive_secs = memory;
            cfg.vocabulary = vocabulary;
            cfg.replacements = replacements;
            cfg.history_retention_days = retention;
            cfg.history_enabled = history_on;
            cfg.launch_at_login = launch;
        });
        let warning = match engine {
            Engine::Groq if !keystore::has_key(keystore::GROQ) => {
                "Saved. Add a Groq key to use Groq."
            }
            Engine::OpenAI if !keystore::has_key(keystore::OPENAI) => {
                "Saved. Add an OpenAI key to use OpenAI."
            }
            Engine::Local if !model::present() && !model::status().active => {
                "Saved. Download the model to use Local."
            }
            _ => "Saved.",
        };
        self.status.set_text(warning);
        self.load_favorites_only();
    }

    fn load_favorites_only(&self) {
        let cfg = state::config();
        self.favorites.set_text(&cfg.favorite_languages.join(", "));
    }
}
