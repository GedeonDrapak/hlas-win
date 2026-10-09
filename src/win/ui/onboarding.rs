//! Welcome tour, ported from the macOS onboarding: Welcome, Microphone,
//! Engine & language, Try it. The left panel is a pre-rendered image per step
//! (tools/render_assets.py) so it matches the macOS split layout.

use super::controls::{self as c, Theme};
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

pub struct Onboarding {
    window: nwg::Window,
    panel: nwg::ImageFrame,
    bitmaps: Vec<nwg::Bitmap>,
    title: nwg::Label,
    subtitle: nwg::Label,
    // Step 0
    features: Vec<nwg::Label>,
    // Step 1
    mic_status: nwg::Label,
    mic_label: nwg::Label,
    mic_choice: nwg::ComboBox<String>,
    mic_test: nwg::Button,
    mic_meter: nwg::ProgressBar,
    mic_privacy: nwg::Button,
    mic_hint: nwg::Label,
    // Step 2
    local: nwg::RadioButton,
    groq: nwg::RadioButton,
    openai: nwg::RadioButton,
    model_status: nwg::Label,
    model_button: nwg::Button,
    model_progress: nwg::ProgressBar,
    key_label: nwg::Label,
    key: nwg::TextInput,
    lang_label: nwg::Label,
    language: nwg::ComboBox<String>,
    // Step 3
    try_box: nwg::TextBox,
    last: nwg::Label,
    launch: nwg::CheckBox,
    tips: nwg::Label,
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

impl Onboarding {
    pub fn build(theme: Rc<Theme>) -> Result<Rc<Onboarding>, NwgError> {
        let t = &*theme;
        let w = c::window(t, "Welcome to Hlas", (760, 500), false)?;
        let bitmaps = panel_bitmaps();
        let mut panel = nwg::ImageFrame::default();
        nwg::ImageFrame::builder()
            .position((0, 0))
            .size((280, 500))
            .bitmap(bitmaps.first())
            .background_color(Some([15, 15, 15]))
            .parent(&w)
            .build(&mut panel)?;

        let x = 312;
        let wide = 420;
        let title = c::label(&w, TITLES[0], (x, 30), (wide, 40), &t.title)?;
        let subtitle = c::label(&w, "", (x, 76), (wide, 44), &t.body)?;

        let key = hotkeys::name(state::config().hotkey_vk);
        let feature_text = [
            format!("Hold {key} and speak Czech or English. Release, and the text pastes at your cursor in any app."),
            format!("Quick-tap {key} to keep listening hands-free. Tap again to stop. Esc cancels."),
            "Local by default: audio never leaves this PC. Cloud engines are opt-in.".to_string(),
            "A tiny app. The model loads when you dictate and unloads when you stop.".to_string(),
        ];
        let mut features = Vec::new();
        for (i, f) in feature_text.iter().enumerate() {
            features.push(c::label(
                &w,
                &format!("\u{2022}  {f}"),
                (x, 134 + i as i32 * 58),
                (wide, 50),
                &t.body,
            )?);
        }

        let mic_status = c::label(&w, "", (x, 132), (wide, 44), &t.bold)?;
        let mic_label = c::label(&w, "Microphone", (x, 190), (110, 22), &t.body)?;
        let mic_choice = c::combo(&w, vec![], None, (x + 116, 186), wide - 116, &t.body)?;
        let mic_test = c::button(&w, "Test microphone", (x, 230), (150, 32), &t.body)?;
        let mic_meter = c::progress(&w, (x + 164, 243), (wide - 164, 8))?;
        let mic_privacy = c::button(
            &w,
            "Open microphone privacy settings",
            (x, 276),
            (260, 32),
            &t.body,
        )?;
        let mic_hint = c::label(
            &w,
            "Hlas listens only while you hold the key. Windows shows a microphone icon in the taskbar while it is in use.",
            (x, 322),
            (wide, 44),
            &t.small,
        )?;

        let local = c::radio(&w, t, Engine::Local.label(), (x, 132), (wide, 24), true)?;
        let groq = c::radio(&w, t, Engine::Groq.label(), (x, 158), (wide, 24), false)?;
        let openai = c::radio(&w, t, Engine::OpenAI.label(), (x, 184), (wide, 24), false)?;
        let model_status = c::label(&w, "", (x, 226), (290, 22), &t.small)?;
        let model_button = c::button(&w, "Download", (x + 300, 220), (120, 30), &t.body)?;
        let model_progress = c::progress(&w, (x, 256), (wide, 6))?;
        let key_label = c::label(&w, "API key", (x, 226), (80, 22), &t.body)?;
        let key_input = c::input(&w, "", (x + 90, 222), (wide - 90, 26), &t.body, true)?;
        let lang_label = c::label(&w, "Language", (x, 290), (90, 22), &t.body)?;
        let lang_items: Vec<String> = std::iter::once(languages::AUTO)
            .chain(languages::ALL.iter().copied())
            .map(|(code, name)| format!("{name} ({code})"))
            .collect();
        let language = c::combo(&w, lang_items, None, (x + 90, 286), wide - 90, &t.body)?;

        let try_box = c::text_box(&w, (x, 132), (wide, 110), &t.body, false)?;
        let last = c::label(&w, "", (x, 252), (wide, 44), &t.small)?;
        let launch = c::check(
            &w,
            t,
            "Start Hlas when I sign in",
            (x, 302),
            (wide, 24),
            false,
        )?;
        let tips = c::label(
            &w,
            "Add names and jargon in Settings > Vocabulary.\nThe tray icon opens Settings, History and this tour.",
            (x, 336),
            (wide, 44),
            &t.small,
        )?;

        let back = c::button(&w, "Back", (x, 444), (100, 34), &t.body)?;
        let next = c::button(&w, "Continue", (x + wide - 140, 444), (140, 34), &t.bold)?;
        let timer = c::timer(&w, 100)?;

        let ui = Rc::new(Onboarding {
            window: w,
            panel,
            bitmaps,
            title,
            subtitle,
            features,
            mic_status,
            mic_label,
            mic_choice,
            mic_test,
            mic_meter,
            mic_privacy,
            mic_hint,
            local,
            groq,
            openai,
            model_status,
            model_button,
            model_progress,
            key_label,
            key: key_input,
            lang_label,
            language,
            try_box,
            last,
            launch,
            tips,
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
                E::OnButtonClick
                    if handle == ui.local.handle
                        || handle == ui.groq.handle
                        || handle == ui.openai.handle =>
                {
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
        if c::radio_on(&self.groq) {
            Engine::Groq
        } else if c::radio_on(&self.openai) {
            Engine::OpenAI
        } else {
            Engine::Local
        }
    }

    fn go(&self, step: usize) {
        self.step.set(step);
        if let Some(b) = self.bitmaps.get(step) {
            self.panel.set_bitmap(Some(b));
        }
        self.title.set_text(TITLES[step]);
        let key = hotkeys::name(state::config().hotkey_vk);
        let subtitle = match step {
            0 => "Your own dictation app. No subscription, no account, no cloud unless you ask for it.".to_string(),
            1 => "Hlas needs the microphone. Windows controls it under Privacy & security.".to_string(),
            2 => "Where transcription runs. You can switch any time from the tray icon.".to_string(),
            _ => format!("Click into the box, hold {key}, say something, release."),
        };
        self.subtitle.set_text(&subtitle);

        for f in &self.features {
            f.set_visible(step == 0);
        }
        for v in [&self.mic_status, &self.mic_label, &self.mic_hint] {
            v.set_visible(step == 1);
        }
        self.mic_choice.set_visible(step == 1);
        self.mic_test.set_visible(step == 1);
        self.mic_meter.set_visible(step == 1);
        self.mic_privacy.set_visible(step == 1);
        for r in [&self.local, &self.groq, &self.openai] {
            r.set_visible(step == 2);
        }
        self.lang_label.set_visible(step == 2);
        self.language.set_visible(step == 2);
        self.try_box.set_visible(step == 3);
        self.last.set_visible(step == 3);
        self.launch.set_visible(step == 3);
        self.tips.set_visible(step == 3);
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
                self.mic_meter.set_pos(0);
            }
            2 => {
                c::set_radio(&self.local, cfg.engine == Engine::Local);
                c::set_radio(&self.groq, cfg.engine == Engine::Groq);
                c::set_radio(&self.openai, cfg.engine == Engine::OpenAI);
                self.language.set_selection(Some(
                    languages::ALL
                        .iter()
                        .position(|(code, _)| *code == cfg.language)
                        .map(|i| i + 1)
                        .unwrap_or(0),
                ));
            }
            3 => {
                c::set_checked(&self.launch, autostart::is_enabled());
                self.try_box.set_focus();
            }
            _ => {}
        }
        self.layout_engine();
        self.tick();
    }

    /// Engine step: model download for Local, key field for cloud engines.
    fn layout_engine(&self) {
        let on_step = self.step.get() == 2;
        let local = self.engine() == Engine::Local;
        self.model_status.set_visible(on_step && local);
        self.model_button.set_visible(on_step && local);
        self.model_progress.set_visible(on_step && local);
        self.key_label.set_visible(on_step && !local);
        self.key.set_visible(on_step && !local);
        if !local {
            let account = if self.engine() == Engine::Groq {
                keystore::GROQ
            } else {
                keystore::OPENAI
            };
            self.key
                .set_text(&keystore::get_key(account).unwrap_or_default());
            self.key_label.set_text(if self.engine() == Engine::Groq {
                "Groq key"
            } else {
                "OpenAI key"
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
                let on = c::is_checked(&self.launch);
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
        self.mic_meter.set_pos(0);
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
                            self.mic_meter.set_pos((m.level() * 100.0) as u32);
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
                    let (text, button, pos) = if model::present() {
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
                    if self.model_status.text() != text {
                        self.model_status.set_text(&text);
                    }
                    if self.model_button.text() != button {
                        self.model_button.set_text(button);
                    }
                    self.model_button.set_enabled(!model::present());
                    self.model_progress.set_pos(pos);
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
                if self.last.text() != text {
                    self.last.set_text(&text);
                }
            }
            _ => {}
        }
    }
}
