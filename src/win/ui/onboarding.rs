//! Welcome tour, ported from the macOS onboarding: Welcome, Microphone,
//! Engine & language, Try it. The left panel is a pre-rendered image per step
//! (tools/render_assets.py) so it matches the macOS split layout; the right
//! side uses the shared dark skin.

use super::controls::{self as c, Theme};
use super::skin::{self, ds, Kind, Meter, Segmented, Skin, Text};
use crate::core::config::Engine;
use crate::core::{hotkeys, languages};
use crate::win::engine::model;
use crate::win::mic::Mic;
use crate::win::privacy::{self, MicAccess};
use crate::win::{autostart, keystore, mic, shell, state};
use native_windows_gui as nwg;
use nwg::NwgError;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

const PANELS: [&[u8]; 4] = [
    include_bytes!("../../../assets/onboarding/step-0.jpg"),
    include_bytes!("../../../assets/onboarding/step-1.jpg"),
    include_bytes!("../../../assets/onboarding/step-2.jpg"),
    include_bytes!("../../../assets/onboarding/step-3.jpg"),
];
const TITLES: [&str; 4] = [
    "Welcome to Hlas",
    "Your microphone",
    "Engine & language",
    "Try it now",
];

// Visibility groups: steps 0-3 are groups 1-4; on the engine step the model
// row (local) or the key field (cloud) is added.
const LOCAL: u8 = 6;
const CLOUD: u8 = 7;
const ENGINES: [Engine; 3] = [Engine::Local, Engine::Groq, Engine::OpenAI];
const X: i32 = 312;
const WIDE: i32 = 424;

pub struct Onboarding {
    window: nwg::Window,
    skin: Rc<Skin>,
    /// Kept alive: the skin paints them as the left panel.
    _bitmaps: Vec<nwg::Bitmap>,
    title: Text,
    subtitle: Text,
    // Step 1
    mic_status: Text,
    mic_choice: nwg::ComboBox<String>,
    mic_test: nwg::Button,
    mic_meter: Meter,
    mic_privacy: nwg::Button,
    // Step 2
    engine: Segmented,
    engine_note: Text,
    model_status: Text,
    model_button: nwg::Button,
    model_meter: Meter,
    key_label: Text,
    key: nwg::TextInput,
    language: nwg::ComboBox<String>,
    // Step 3
    try_box: nwg::TextBox,
    last: Text,
    launch: nwg::Button,
    // Footer
    back: nwg::Button,
    next: nwg::Button,
    timer: nwg::AnimationTimer,
    step: Cell<usize>,
    mic_names: RefCell<Vec<String>>,
    tester: RefCell<Option<(Mic, Instant)>>,
    ticks: Cell<u32>,
    handler: RefCell<Option<nwg::EventHandler>>,
}

fn panel_bitmaps() -> Vec<nwg::Bitmap> {
    let Ok(decoder) = nwg::ImageDecoder::new() else {
        return Vec::new();
    };
    let scale = nwg::scale_factor();
    let size = [
        (280.0 * scale).round() as u32,
        (500.0 * scale).round() as u32,
    ];
    PANELS
        .iter()
        .filter_map(|bytes| {
            let source = decoder.from_stream(bytes).ok()?;
            let frame = source.frame(0).ok()?;
            let sized = decoder.resize_image(&frame, size).ok()?;
            sized.as_bitmap().ok()
        })
        .collect()
}

fn engine_note(engine: Engine) -> &'static str {
    match engine {
        Engine::Local => "Runs on this PC. Private, free and offline after a one-time download.",
        Engine::Groq => "Groq cloud. The fastest, about $3 a month with your own key.",
        Engine::OpenAI => "OpenAI cloud. Best accuracy, about $8 a month with your own key.",
    }
}

impl Onboarding {
    pub fn build(theme: Rc<Theme>) -> Result<Rc<Onboarding>, NwgError> {
        let t = theme.clone();
        let w = c::window(&t, "Welcome to Hlas", (760, 500), false)?;
        let skin = Skin::new(theme, &w, ds::BG);
        let s = &*skin;
        let bitmaps = panel_bitmaps();
        s.fill(0, (0, 0, 280, 500), 0x0F0F0F);
        for (step, b) in bitmaps.iter().enumerate() {
            s.bitmap(step as u8 + 1, (0, 0, 280, 500), b);
        }

        let title = s.label(
            0,
            TITLES[0],
            (X, 32, WIDE, 40),
            &t.title,
            ds::FG,
            skin::LINE,
        );
        let subtitle = s.label(0, "", (X, 76, WIDE, 44), &t.body, ds::FG2, skin::WRAP);

        // Step 0: what Hlas is.
        let key = hotkeys::name(state::config().hotkey_vk);
        let features = [
            (
                '\u{E720}',
                format!(
                    "Hold {key} and speak. Release, and the text pastes at your cursor in any app."
                ),
            ),
            (
                '\u{E7C9}',
                format!(
                    "Quick-tap {key} to keep listening hands-free. Tap again to stop. Esc cancels."
                ),
            ),
            (
                '\u{E72E}',
                "Local by default: audio never leaves this PC. Cloud engines are opt-in."
                    .to_string(),
            ),
            (
                '\u{E945}',
                "A tiny app. The model loads when you dictate and unloads when you stop."
                    .to_string(),
            ),
        ];
        for (i, (icon, text)) in features.iter().enumerate() {
            let y = 134 + i as i32 * 72;
            s.card(1, (X, y, WIDE, 62));
            s.glyph(1, *icon, (X + 12, y, 32, 62), ds::BRAND);
            s.label(
                1,
                text,
                (X + 52, y + 11, WIDE - 66, 42),
                &t.body,
                ds::FG,
                skin::WRAP,
            );
        }

        // Step 1: microphone.
        s.card(2, (X, 134, WIDE, 158));
        let mic_status = s.label(
            2,
            "",
            (X + 18, 134 + 14, WIDE - 36, 40),
            &t.medium,
            ds::FG,
            skin::WRAP,
        );
        s.label(
            2,
            "Microphone",
            (X + 18, 134 + 58, 100, 30),
            &t.body,
            ds::FG2,
            skin::LINE,
        );
        let mic_choice = s.combo(2, vec![], (X + 120, 134 + 58, WIDE - 138))?;
        let mic_test = s.button(
            2,
            "Test microphone",
            (X + 18, 134 + 106, 150, 34),
            Kind::Secondary,
            ds::SURFACE,
        )?;
        let mic_meter = s.meter(2, (X + 186, 134 + 121, WIDE - 204, 4));
        let mic_privacy = s.button(
            2,
            "Open microphone privacy settings",
            (X, 304, 272, 34),
            Kind::Ghost,
            ds::BG,
        )?;
        s.label(
            2,
            "Hlas listens only while you hold the key. Windows shows a microphone icon in the taskbar while it is in use.",
            (X, 350, WIDE, 40),
            &t.small,
            ds::FG3,
            skin::WRAP,
        );

        // Step 2: engine and language.
        let engine = s.segmented(3, &["On this PC", "Groq", "OpenAI"], (X, 134, WIDE, 36), 0)?;
        let engine_note = s.label(3, "", (X, 178, WIDE, 36), &t.small, ds::FG2, skin::WRAP);
        s.card(3, (X, 222, WIDE, 84));
        let model_status = s.label(
            LOCAL,
            "",
            (X + 18, 222 + 14, WIDE - 176, 38),
            &t.small,
            ds::FG2,
            skin::WRAP,
        );
        let model_button = s.button(
            LOCAL,
            "Download",
            (X + WIDE - 138, 222 + 14, 120, 34),
            Kind::Secondary,
            ds::SURFACE,
        )?;
        let model_meter = s.meter(LOCAL, (X + 18, 222 + 64, WIDE - 36, 4));
        let key_label = s.label(
            CLOUD,
            "API key",
            (X + 18, 222 + 12, WIDE - 36, 20),
            &t.small,
            ds::FG2,
            skin::LINE,
        );
        let key_input = s.input(CLOUD, (X + 18, 222 + 38, WIDE - 36, 34), true)?;
        s.label(
            3,
            "Language",
            (X, 322, 100, 30),
            &t.body,
            ds::FG2,
            skin::LINE,
        );
        let lang_items: Vec<String> = std::iter::once(languages::AUTO)
            .chain(languages::ALL.iter().copied())
            .map(|(code, name)| format!("{name} ({code})"))
            .collect();
        let language = s.combo(3, lang_items, (X + 110, 322, WIDE - 110))?;

        // Step 3: try it.
        let try_box = s.text_box(4, (X, 134, WIDE, 108), false)?;
        let last = s.label(4, "", (X, 250, WIDE, 38), &t.small, ds::FG2, skin::WRAP);
        s.card(4, (X, 296, WIDE, 60));
        s.label(
            4,
            "Start Hlas when I sign in",
            (X + 18, 296 + 10, WIDE - 96, 22),
            &t.medium,
            ds::FG,
            skin::LINE,
        );
        s.label(
            4,
            "Keep dictation ready in the tray.",
            (X + 18, 296 + 32, WIDE - 96, 18),
            &t.small,
            ds::FG3,
            skin::LINE,
        );
        let launch = s.toggle(4, X + WIDE - 58, 296 + 19, ds::SURFACE, false)?;
        s.label(
            4,
            "Add names and jargon in Settings > Vocabulary. The tray icon opens Settings, History and this tour.",
            (X, 368, WIDE, 40),
            &t.small,
            ds::FG3,
            skin::WRAP,
        );

        let back = s.button(0, "Back", (X, 444, 100, 36), Kind::Ghost, ds::BG)?;
        let next = s.button(
            0,
            "Continue",
            (X + WIDE - 170, 444, 170, 36),
            Kind::Primary,
            ds::BG,
        )?;
        let timer = c::timer(&w, 100)?;

        let ui = Rc::new(Onboarding {
            window: w,
            skin,
            _bitmaps: bitmaps,
            title,
            subtitle,
            mic_status,
            mic_choice,
            mic_test,
            mic_meter,
            mic_privacy,
            engine,
            engine_note,
            model_status,
            model_button,
            model_meter,
            key_label,
            key: key_input,
            language,
            try_box,
            last,
            launch,
            back,
            next,
            timer,
            step: Cell::new(0),
            mic_names: RefCell::new(Vec::new()),
            tester: RefCell::new(None),
            ticks: Cell::new(0),
            handler: RefCell::new(None),
        });

        let weak = Rc::downgrade(&ui);
        let handler = nwg::full_bind_event_handler(&ui.window.handle, move |evt, data, handle| {
            let Some(ui) = weak.upgrade() else { return };
            use nwg::Event as E;
            if evt == E::OnButtonClick {
                ui.skin.click(&handle);
            }
            match evt {
                E::OnWindowClose if handle == ui.window.handle => {
                    if let nwg::EventData::OnWindowClose(d) = data {
                        d.close(false);
                    }
                    ui.finish(false);
                }
                E::OnButtonClick if handle == ui.next.handle => {
                    ui.commit_step();
                    if ui.step.get() == 3 {
                        ui.finish(true);
                    } else {
                        ui.go(ui.step.get() + 1);
                    }
                }
                E::OnButtonClick if handle == ui.back.handle => {
                    ui.commit_step();
                    ui.go(ui.step.get().saturating_sub(1));
                }
                E::OnButtonClick if handle == ui.mic_privacy.handle => {
                    shell::open(shell::MIC_PRIVACY)
                }
                E::OnButtonClick if handle == ui.mic_test.handle => ui.start_mic_test(),
                E::OnButtonClick if handle == ui.model_button.handle => {
                    if model::status().active {
                        model::cancel();
                    } else {
                        model::start();
                    }
                }
                E::OnButtonClick if ui.engine.index_of(&handle).is_some() => {
                    ui.commit_step();
                    ui.layout_engine();
                }
                E::OnTimerTick if handle == ui.timer.handle => ui.tick(),
                _ => {}
            }
        });
        *ui.handler.borrow_mut() = Some(handler);
        ui.timer.start();
        Ok(ui)
    }

    pub fn show(&self, step: usize) {
        self.go(step.min(3));
        c::present(&self.window);
    }

    fn engine(&self) -> Engine {
        ENGINES[self.skin.selected(&self.engine).min(ENGINES.len() - 1)]
    }

    fn go(&self, step: usize) {
        self.step.set(step);
        self.title.set_text(TITLES[step]);
        let key = hotkeys::name(state::config().hotkey_vk);
        let subtitle = match step {
            0 => "Your own dictation app. No subscription, no account, no cloud unless you ask for it.".to_string(),
            1 => "Hlas needs the microphone. Windows controls it under Privacy & security.".to_string(),
            2 => "Where transcription runs. You can switch any time from the tray icon.".to_string(),
            _ => format!("Click into the box, hold {key}, say something, release."),
        };
        self.subtitle.set_text(&subtitle);
        self.back.set_visible(step > 0);
        self.next.set_text(if step == 3 {
            "Start dictating"
        } else {
            "Continue"
        });

        let cfg = state::config();
        match step {
            1 => {
                let mut names = vec![String::new()];
                names.extend(mic::device_names());
                let labels: Vec<String> = names
                    .iter()
                    .map(|n| {
                        if n.is_empty() {
                            "Windows default".into()
                        } else {
                            n.clone()
                        }
                    })
                    .collect();
                self.mic_choice.set_collection(labels);
                self.mic_choice.set_selection(
                    names
                        .iter()
                        .position(|n| *n == cfg.input_device)
                        .or(Some(0)),
                );
                *self.mic_names.borrow_mut() = names;
                self.mic_meter.set(0.0);
            }
            2 => {
                self.skin.select(
                    &self.engine,
                    ENGINES.iter().position(|e| *e == cfg.engine).unwrap_or(0),
                );
                self.language.set_selection(Some(
                    languages::ALL
                        .iter()
                        .position(|(code, _)| *code == cfg.language)
                        .map(|i| i + 1)
                        .unwrap_or(0),
                ));
            }
            3 => {
                self.skin
                    .set_on(&self.launch.handle, autostart::is_enabled());
                self.try_box.set_focus();
            }
            _ => {}
        }
        self.layout_engine();
        self.tick();
    }

    /// Shows the current step; on the engine step, the model download for
    /// Local or the key field for a cloud engine.
    fn layout_engine(&self) {
        let step = self.step.get();
        let local = self.engine() == Engine::Local;
        let mut mask = 1u64 << (step + 1);
        if step == 2 {
            mask |= 1u64 << if local { LOCAL } else { CLOUD };
            self.engine_note.set_text(engine_note(self.engine()));
        }
        self.skin.set_visible(mask);
        if !local {
            let account = if self.engine() == Engine::Groq {
                keystore::GROQ
            } else {
                keystore::OPENAI
            };
            self.key
                .set_text(&keystore::get_key(account).unwrap_or_default());
            self.key_label.set_text(if self.engine() == Engine::Groq {
                "Groq API key"
            } else {
                "OpenAI API key"
            });
        }
    }

    /// Saves what the current step edits.
    fn commit_step(&self) {
        match self.step.get() {
            1 => {
                let device = self
                    .mic_choice
                    .selection()
                    .and_then(|i| self.mic_names.borrow().get(i).cloned())
                    .unwrap_or_default();
                state::update_config(|cfg| cfg.input_device = device);
                self.stop_mic_test();
            }
            2 => {
                let engine = self.engine();
                if engine != Engine::Local {
                    let account = if engine == Engine::Groq {
                        keystore::GROQ
                    } else {
                        keystore::OPENAI
                    };
                    if !self.key.text().trim().is_empty() {
                        keystore::persist(account, &self.key.text());
                    }
                }
                let index = self.language.selection().unwrap_or(0);
                let language = if index == 0 {
                    "auto".to_string()
                } else {
                    languages::ALL
                        .get(index - 1)
                        .map(|(c, _)| c.to_string())
                        .unwrap_or_else(|| "auto".into())
                };
                state::update_config(|cfg| {
                    cfg.engine = engine;
                    if language != "auto" && !cfg.favorite_languages.contains(&language) {
                        cfg.favorite_languages.insert(0, language.clone());
                    }
                    cfg.language = language;
                });
            }
            3 => {
                let on = self.skin.is_on(&self.launch.handle);
                if let Err(e) = autostart::set(on) {
                    log::error!("autostart change failed: {e}");
                }
                state::update_config(|cfg| cfg.launch_at_login = on);
            }
            _ => {}
        }
    }

    fn finish(&self, completed: bool) {
        if completed {
            self.commit_step();
        }
        self.stop_mic_test();
        state::update_config(|cfg| cfg.has_onboarded = true);
        self.window.set_visible(false);
    }

    fn start_mic_test(&self) {
        self.stop_mic_test();
        let device = self
            .mic_choice
            .selection()
            .and_then(|i| self.mic_names.borrow().get(i).cloned())
            .unwrap_or_default();
        let mut m = Mic::new();
        match m.begin(&device) {
            Ok(name) => {
                self.mic_status
                    .set_text(&format!("Listening on {name}. Say something..."));
                *self.tester.borrow_mut() = Some((m, Instant::now()));
            }
            Err(e) => self
                .mic_status
                .set_text(&crate::core::errors::user_message(&e)),
        }
    }

    fn stop_mic_test(&self) {
        if let Some((mut m, _)) = self.tester.borrow_mut().take() {
            m.cancel();
        }
        self.mic_meter.set(0.0);
    }

    fn tick(&self) {
        if !self.window.visible() {
            return;
        }
        let tick = self.ticks.get().wrapping_add(1);
        self.ticks.set(tick);
        match self.step.get() {
            1 => {
                let finished = {
                    let tester = self.tester.borrow();
                    match tester.as_ref() {
                        Some((m, started)) => {
                            self.mic_meter.set(m.level());
                            started.elapsed() > Duration::from_secs(5)
                        }
                        None => false,
                    }
                };
                if finished {
                    self.stop_mic_test();
                    self.mic_status
                        .set_text("Test finished. If the bar moved, Hlas can hear you.");
                } else if self.tester.borrow().is_none() && tick % 10 == 1 {
                    let access = privacy::mic_access();
                    let devices = mic::device_names().len();
                    let text = match (access, devices) {
                        (MicAccess::Allowed, 0) => {
                            "No microphone found. Connect one, then test it.".to_string()
                        }
                        (MicAccess::Allowed, _) => {
                            "Microphone access is on. Test it to be sure.".to_string()
                        }
                        (blocked, _) => {
                            format!("{} Turn it on in the privacy settings.", blocked.message())
                        }
                    };
                    if self.mic_status.text() != text
                        && !self.mic_status.text().starts_with("Test finished")
                    {
                        self.mic_status.set_text(&text);
                    }
                }
            }
            2 => {
                if self.engine() == Engine::Local {
                    let s = model::status();
                    let (text, button, pos) = if !crate::win::engine::local::cpu_supported() {
                        (
                            "This processor cannot run the local engine. Pick Groq above."
                                .to_string(),
                            "Unavailable",
                            0,
                        )
                    } else if model::present() {
                        ("Model ready. Works offline.".to_string(), "Ready", 100)
                    } else if s.active {
                        let t = if s.verifying {
                            "Verifying...".to_string()
                        } else {
                            format!("Downloading... {}%", s.percent)
                        };
                        (t, "Cancel", s.percent as u32)
                    } else {
                        (
                            s.error.unwrap_or_else(|| {
                                "One-time 547 MB download. You can continue meanwhile.".into()
                            }),
                            "Download",
                            0,
                        )
                    };
                    self.model_status.set_text(&text);
                    if self.model_button.text() != button {
                        self.model_button.set_text(button);
                    }
                    self.model_button.set_enabled(
                        !model::present() && crate::win::engine::local::cpu_supported(),
                    );
                    self.model_meter.set(pos as f32 / 100.0);
                }
            }
            3 => {
                let text = match state::last() {
                    Some(last) => format!(
                        "Last transcript: {}",
                        last.text.chars().take(160).collect::<String>()
                    ),
                    None => String::new(),
                };
                self.last.set_text(&text);
            }
            _ => {}
        }
    }
}
