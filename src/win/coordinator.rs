//! The dictation brain, ported from the macOS DictationCoordinator.
//!
//! One thread owns the microphone and the state machine. Key events, file
//! imports and worker results arrive on one channel. Transcription runs on a
//! worker thread per session; a newer session or Esc makes older results
//! irrelevant (session id + cancel flag), so nothing stale is ever pasted.

use super::engine;
use super::hotkey::{self, HotkeyEvent};
use super::inject::{self, Target};
use super::mic::Mic;
use super::overlay::{self, Pill};
use super::state;
use super::ui::{self, Command};
use crate::core::audio;
use crate::core::config::{Config, Engine, OutputMode};
use crate::core::errors::{user_message, LocalError, SmartTextError};
use crate::core::press::{PressAction, PressTracker};
use crate::core::text;
use crossbeam_channel::{Receiver, Sender};
use once_cell::sync::OnceCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub enum Event {
    Hotkey(HotkeyEvent),
    ImportFile(PathBuf),
    /// Reformat an earlier result with Smart text (History, tray).
    MakeSmart {
        raw: String,
        history_id: Option<u64>,
    },
    Done {
        session: u64,
        outcome: Outcome,
    },
}

pub enum Outcome {
    Hidden,
    Delivered,
    Review {
        text: String,
        original: String,
        message: String,
    },
    Error(String),
    Cancelled,
}

static TX: OnceCell<Sender<Event>> = OnceCell::new();

pub fn send(event: Event) {
    if let Some(tx) = TX.get() {
        let _ = tx.send(event);
    }
}

/// Starts the coordinator thread and the hotkey hook feeding it.
pub fn start(hotkey_vk: u32) {
    let (tx, rx) = crossbeam_channel::unbounded::<Event>();
    let _ = TX.set(tx.clone());
    let (key_tx, key_rx) = crossbeam_channel::unbounded::<HotkeyEvent>();
    hotkey::start(hotkey_vk, key_tx);
    std::thread::Builder::new()
        .name("hlas-keys".into())
        .spawn(move || {
            for e in key_rx.iter() {
                let _ = tx.send(Event::Hotkey(e));
            }
        })
        .expect("key forwarder");
    std::thread::Builder::new()
        .name("hlas-coordinator".into())
        .spawn(move || Coordinator::new().run(rx))
        .expect("coordinator thread");
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Recording,
    Processing,
}

struct Session {
    id: u64,
    cfg: Config,
    mode: OutputMode,
    target: Option<Target>,
    cancel: Arc<AtomicBool>,
}

struct Coordinator {
    phase: Phase,
    next_id: u64,
    session: Option<Session>,
    press: PressTracker,
    mic: Mic,
    prefetched: bool,
}

impl Coordinator {
    fn new() -> Coordinator {
        Coordinator {
            phase: Phase::Idle,
            next_id: 1,
            session: None,
            press: PressTracker::default(),
            mic: Mic::new(),
            prefetched: false,
        }
    }

    fn run(mut self, rx: Receiver<Event>) {
        loop {
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(event) => self.handle(event),
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return,
            }
            self.tick();
        }
    }

    fn tick(&mut self) {
        if self.phase == Phase::Recording {
            let level = self.mic.level();
            overlay::set_level(level);
            if self.mic.at_limit() {
                log::info!("recording reached the 10-minute limit");
                self.stop();
                return;
            }
            if let Some(s) = &self.session {
                if !self.prefetched
                    && level > 0.12
                    && s.cfg.engine == Engine::Local
                    && engine::model::present()
                {
                    self.prefetched = true;
                    engine::local::prepare(s.cfg.model_keep_alive_secs);
                }
            }
        } else {
            self.mic.tick();
        }
    }

    fn handle(&mut self, event: Event) {
        match event {
            Event::Hotkey(HotkeyEvent::Down) => {
                match self
                    .press
                    .press(self.phase == Phase::Recording, Instant::now())
                {
                    PressAction::Start => self.start(),
                    PressAction::Stop => self.stop(),
                    PressAction::Ignore => {}
                }
            }
            Event::Hotkey(HotkeyEvent::Up) => {
                if self.phase == Phase::Recording
                    && self.press.release(Instant::now()) == PressAction::Stop
                {
                    self.stop();
                }
            }
            Event::Hotkey(HotkeyEvent::Chord) => {
                // Right Ctrl was a shortcut modifier, not push-to-talk.
                if self.phase == Phase::Recording && self.press.press_pending() {
                    self.cancel(false);
                }
            }
            Event::Hotkey(HotkeyEvent::Escape) => {
                if self.phase != Phase::Idle {
                    self.cancel(true);
                }
            }
            Event::ImportFile(path) => self.import(path),
            Event::MakeSmart { raw, history_id } => self.make_smart(raw, history_id),
            Event::Done { session, outcome } => {
                if self.session.as_ref().map(|s| s.id) != Some(session) {
                    return; // a newer session or a cancel superseded this one
                }
                self.finish(outcome);
            }
        }
    }

    fn new_session(&mut self, target: Option<Target>, mode: OutputMode) -> u64 {
        if let Some(old) = self.session.take() {
            old.cancel.store(true, Ordering::SeqCst);
        }
        let id = self.next_id;
        self.next_id += 1;
        self.session = Some(Session {
            id,
            cfg: state::config(),
            mode,
            target,
            cancel: Arc::new(AtomicBool::new(false)),
        });
        id
    }

    fn start(&mut self) {
        let cfg = state::config();
        // Shift held at the start forces a plain transcript, like Option on macOS.
        let mode = if inject::shift_held() {
            OutputMode::Transcript
        } else {
            cfg.output_mode
        };
        let target = Target::capture();
        self.new_session(target, mode);
        self.prefetched = false;
        match self.mic.begin(&cfg.input_device) {
            Ok(device) => {
                log::info!(
                    "recording started (engine={}, mode={:?}, device_custom={})",
                    cfg.engine.id(),
                    mode,
                    !cfg.input_device.is_empty() && device == cfg.input_device
                );
                self.phase = Phase::Recording;
                hotkey::set_active(true);
                overlay::show(Pill::Recording {
                    smart: mode == OutputMode::Smart,
                });
            }
            Err(e) => {
                log::error!("microphone start failed: {e}");
                self.press.reset();
                self.session = None;
                self.phase = Phase::Idle;
                overlay::show_for(
                    Pill::Message {
                        text: user_message(&e),
                        error: true,
                    },
                    Duration::from_secs(5),
                );
            }
        }
    }

    fn stop(&mut self) {
        if self.phase != Phase::Recording {
            return;
        }
        self.press.reset();
        let samples = match self.mic.end() {
            Ok(s) => s,
            Err(e) => {
                log::error!("recording stop failed: {e}");
                self.finish(Outcome::Error(user_message(&e)));
                return;
            }
        };
        let Some(s) = &self.session else { return };
        self.phase = Phase::Processing;
        overlay::show(Pill::Transcribing);
        spawn_worker(
            s.id,
            samples,
            s.cfg.clone(),
            s.mode,
            s.target,
            s.cancel.clone(),
            false,
        );
    }

    fn cancel(&mut self, visible: bool) {
        if let Some(s) = self.session.take() {
            s.cancel.store(true, Ordering::SeqCst);
        }
        if self.phase == Phase::Recording {
            self.mic.cancel();
        }
        self.press.reset();
        self.phase = Phase::Idle;
        hotkey::set_active(false);
        log::info!("dictation cancelled (visible={visible})");
        if visible {
            overlay::show_for(Pill::Cancelled, Duration::from_millis(800));
        } else {
            overlay::hide();
        }
    }

    fn import(&mut self, path: PathBuf) {
        if self.phase != Phase::Idle {
            return;
        }
        let cfg = state::config();
        let id = self.new_session(None, cfg.output_mode);
        let s = self.session.as_ref().expect("session");
        self.phase = Phase::Processing;
        hotkey::set_active(true);
        overlay::show(Pill::Transcribing);
        let (cfg, mode, cancel) = (s.cfg.clone(), s.mode, s.cancel.clone());
        std::thread::spawn(move || {
            let outcome = match crate::core::decode::decode_file(&path, &cancel) {
                Ok(samples) => process(samples, &cfg, mode, None, &cancel, true),
                Err(e) => Outcome::Error(user_message(&e)),
            };
            send(Event::Done {
                session: id,
                outcome,
            });
        });
    }

    fn make_smart(&mut self, raw: String, history_id: Option<u64>) {
        if self.phase != Phase::Idle {
            return;
        }
        let cfg = state::config();
        let id = self.new_session(None, OutputMode::Smart);
        let cancel = self.session.as_ref().expect("session").cancel.clone();
        self.phase = Phase::Processing;
        hotkey::set_active(true);
        overlay::show(Pill::Formatting);
        std::thread::spawn(move || {
            let outcome = match engine::smart_provider(&cfg) {
                None => Outcome::Review {
                    text: raw.clone(),
                    original: raw,
                    message: SmartTextError::MissingKey.to_string(),
                },
                Some((provider, key)) => match engine::smart::process(provider, &key, &raw) {
                    _ if cancel.load(Ordering::SeqCst) => Outcome::Cancelled,
                    Ok(smart) => {
                        let text = text::apply_replacements(&cfg.replacements, &smart);
                        state::set_last(&text, &raw);
                        match history_id {
                            Some(h) => state::update_history(h, &text, OutputMode::Smart),
                            None => {
                                state::add_history(
                                    &text,
                                    &raw,
                                    OutputMode::Smart,
                                    cfg.engine.id(),
                                    None,
                                );
                            }
                        }
                        Outcome::Review {
                            text,
                            original: raw,
                            message: "Smart text is ready".into(),
                        }
                    }
                    Err(e) => Outcome::Review {
                        text: raw.clone(),
                        original: raw,
                        message: user_message(&e),
                    },
                },
            };
            send(Event::Done {
                session: id,
                outcome,
            });
        });
    }

    fn finish(&mut self, outcome: Outcome) {
        self.session = None;
        self.phase = Phase::Idle;
        self.press.reset();
        hotkey::set_active(false);
        match outcome {
            Outcome::Hidden | Outcome::Delivered | Outcome::Cancelled => overlay::hide(),
            Outcome::Error(message) => overlay::show_for(
                Pill::Message {
                    text: message,
                    error: true,
                },
                Duration::from_secs(5),
            ),
            Outcome::Review {
                text,
                original,
                message,
            } => {
                overlay::hide();
                ui::send(Command::ShowResult {
                    text,
                    original,
                    message,
                });
            }
        }
    }
}

fn spawn_worker(
    id: u64,
    samples: Vec<f32>,
    cfg: Config,
    mode: OutputMode,
    target: Option<Target>,
    cancel: Arc<AtomicBool>,
    is_import: bool,
) {
    std::thread::Builder::new()
        .name("hlas-transcribe".into())
        .spawn(move || {
            let outcome = process(samples, &cfg, mode, target.as_ref(), &cancel, is_import);
            send(Event::Done {
                session: id,
                outcome,
            });
        })
        .expect("worker thread");
}

/// The pipeline after recording: pad, transcribe, filter, replace, Smart
/// text, history, then paste or show for review. Mirrors macOS step by step.
fn process(
    samples: Vec<f32>,
    cfg: &Config,
    mode: OutputMode,
    target: Option<&Target>,
    cancel: &Arc<AtomicBool>,
    is_import: bool,
) -> Outcome {
    let cancelled = || cancel.load(Ordering::SeqCst);
    let duration = samples.len() as f64 / audio::SAMPLE_RATE as f64;
    let Some(prepared) = audio::padded(&samples) else {
        log::info!("recording too short or silent ({duration:.1} s)");
        return Outcome::Hidden;
    };

    if cfg.engine == Engine::Local && !engine::model::present() {
        let progress = |p: u8| {
            if !cancel.load(Ordering::SeqCst) {
                overlay::show(Pill::Downloading(p));
            }
        };
        if let Err(e) = engine::model::ensure(progress, cancel) {
            if cancelled() {
                return Outcome::Cancelled;
            }
            return Outcome::Error(e.to_string());
        }
        overlay::show(Pill::Transcribing);
    }

    let began = Instant::now();
    let raw = match engine::transcribe(cfg, &prepared, cancel) {
        Ok(t) => t.trim().to_string(),
        Err(_) if cancelled() => return Outcome::Cancelled,
        Err(e) => {
            if e.downcast_ref::<LocalError>() == Some(&LocalError::Cancelled) {
                return Outcome::Cancelled;
            }
            log::error!("transcription failed: {e}");
            return Outcome::Error(user_message(&e));
        }
    };
    if cancelled() {
        return Outcome::Cancelled;
    }
    log::info!(
        "stt done: engine={} audio_s={duration:.1} elapsed_ms={} chars={}",
        cfg.engine.id(),
        began.elapsed().as_millis(),
        raw.chars().count()
    );
    if raw.is_empty() {
        return Outcome::Hidden;
    }
    state::set_last(&raw, &raw);

    if text::filter_hallucination(&raw).is_empty() {
        state::add_history(
            &raw,
            &raw,
            OutputMode::Transcript,
            cfg.engine.id(),
            Some(duration),
        );
        return Outcome::Review {
            text: raw.clone(),
            original: raw,
            message: "This may be background audio. Review before copying.".into(),
        };
    }

    let mut text_out = text::apply_replacements(&cfg.replacements, &raw);
    let mut recovery: Option<String> = None;
    if mode == OutputMode::Smart {
        overlay::show(Pill::Formatting);
        match engine::smart_provider(cfg) {
            None => recovery = Some(SmartTextError::MissingKey.to_string()),
            Some((provider, key)) => match engine::smart::process(provider, &key, &text_out) {
                Ok(smart) => text_out = text::apply_replacements(&cfg.replacements, &smart),
                Err(_) if cancelled() => return Outcome::Cancelled,
                Err(e) => recovery = Some(user_message(&e)),
            },
        }
    }
    if cancelled() {
        return Outcome::Cancelled;
    }

    state::set_last(&text_out, &raw);
    let saved_mode = if recovery.is_none() {
        mode
    } else {
        OutputMode::Transcript
    };
    state::add_history(&text_out, &raw, saved_mode, cfg.engine.id(), Some(duration));

    if let Some(message) = recovery {
        return Outcome::Review {
            text: text_out,
            original: raw,
            message,
        };
    }
    if is_import {
        return Outcome::Review {
            text: text_out,
            original: raw,
            message: "Audio file transcribed. Copy your text below.".into(),
        };
    }
    match inject::insert(&text_out, target) {
        Ok(()) => {
            log::info!("paste posted");
            Outcome::Delivered
        }
        Err(blocked) => {
            log::info!("delivery review: {blocked:?}");
            Outcome::Review {
                text: text_out,
                original: raw,
                message: blocked.message().into(),
            }
        }
    }
}
