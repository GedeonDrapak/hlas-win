//! Settings window (native Win32 via native-windows-gui - no webview).
//!
//! Opened from the tray on its own thread with a dedicated nwg event loop, so it
//! never contends with the tray's message pump. Reads and writes the shared
//! Config and stores API keys in the Credential Manager.

use crate::config::{Config, Engine};
use crate::keystore;
use native_windows_gui as nwg;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

struct Controls {
    window: nwg::Window,
    _local: nwg::RadioButton,
    groq: nwg::RadioButton,
    openai: nwg::RadioButton,
    languages: nwg::TextInput,
    groq_key: nwg::TextInput,
    openai_key: nwg::TextInput,
    save: nwg::Button,
    status: nwg::Label,
}

/// Opens the settings window. Non-blocking: runs on a background thread.
pub fn open(config: Arc<Mutex<Config>>) {
    std::thread::spawn(move || {
        if nwg::init().is_err() {
            log::error!("nwg init failed");
            return;
        }
        let _ = nwg::Font::set_global_family("Segoe UI");

        let ui = match build(&config) {
            Ok(ui) => ui,
            Err(e) => {
                log::error!("settings build failed: {e}");
                return;
            }
        };

        let ui = Rc::new(ui);
        let handler_ui = ui.clone();
        let handler_config = config.clone();
        let window_handle = ui.window.handle;

        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _data, handle| {
            use nwg::Event as E;
            match evt {
                E::OnButtonClick if handle == handler_ui.save => {
                    save(&handler_ui, &handler_config);
                }
                E::OnWindowClose if handle == handler_ui.window => {
                    nwg::stop_thread_dispatch();
                }
                _ => {}
            }
        });

        nwg::dispatch_thread_events();
        nwg::unbind_event_handler(&handler);
    });
}

fn build(config: &Arc<Mutex<Config>>) -> Result<Controls, nwg::NwgError> {
    let cfg = config.lock().unwrap().clone();

    let mut window = nwg::Window::default();
    nwg::Window::builder()
        .size((380, 440))
        .position((300, 200))
        .title("Hlas Settings")
        .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
        .build(&mut window)?;

    let mut local = nwg::RadioButton::default();
    nwg::RadioButton::builder()
        .text("On this PC - private & free")
        .position((20, 20))
        .size((330, 24))
        .parent(&window)
        .build(&mut local)?;

    let mut groq = nwg::RadioButton::default();
    nwg::RadioButton::builder()
        .text("Groq cloud - fastest (BYOK)")
        .position((20, 50))
        .size((330, 24))
        .parent(&window)
        .build(&mut groq)?;

    let mut openai = nwg::RadioButton::default();
    nwg::RadioButton::builder()
        .text("OpenAI cloud - best accuracy (BYOK)")
        .position((20, 80))
        .size((330, 24))
        .parent(&window)
        .build(&mut openai)?;

    match cfg.engine {
        Engine::Local => local.set_check_state(nwg::RadioButtonState::Checked),
        Engine::Groq => groq.set_check_state(nwg::RadioButtonState::Checked),
        Engine::OpenAI => openai.set_check_state(nwg::RadioButtonState::Checked),
    }

    let mut lang_label = nwg::Label::default();
    nwg::Label::builder()
        .text("Languages (comma-separated, e.g. cs,en)")
        .position((20, 120))
        .size((330, 20))
        .parent(&window)
        .build(&mut lang_label)?;

    let mut languages = nwg::TextInput::default();
    nwg::TextInput::builder()
        .text(&cfg.languages.join(","))
        .position((20, 142))
        .size((330, 24))
        .parent(&window)
        .build(&mut languages)?;

    let mut groq_label = nwg::Label::default();
    nwg::Label::builder()
        .text("Groq API key")
        .position((20, 182))
        .size((330, 20))
        .parent(&window)
        .build(&mut groq_label)?;

    let mut groq_key = nwg::TextInput::default();
    nwg::TextInput::builder()
        .text(&keystore::get_key(keystore::GROQ).unwrap_or_default())
        .position((20, 204))
        .size((330, 24))
        .parent(&window)
        .build(&mut groq_key)?;

    let mut openai_label = nwg::Label::default();
    nwg::Label::builder()
        .text("OpenAI API key")
        .position((20, 244))
        .size((330, 20))
        .parent(&window)
        .build(&mut openai_label)?;

    let mut openai_key = nwg::TextInput::default();
    nwg::TextInput::builder()
        .text(&keystore::get_key(keystore::OPENAI).unwrap_or_default())
        .position((20, 266))
        .size((330, 24))
        .parent(&window)
        .build(&mut openai_key)?;

    let mut hint = nwg::Label::default();
    nwg::Label::builder()
        .text("Push-to-talk: hold Right Ctrl, speak, release.")
        .position((20, 306))
        .size((330, 20))
        .parent(&window)
        .build(&mut hint)?;

    let mut save = nwg::Button::default();
    nwg::Button::builder()
        .text("Save")
        .position((20, 344))
        .size((100, 32))
        .parent(&window)
        .build(&mut save)?;

    let mut status = nwg::Label::default();
    nwg::Label::builder()
        .text("")
        .position((140, 350))
        .size((210, 20))
        .parent(&window)
        .build(&mut status)?;

    // Keep the on-window-only controls alive without fields.
    std::mem::forget(lang_label);
    std::mem::forget(groq_label);
    std::mem::forget(openai_label);
    std::mem::forget(hint);

    Ok(Controls {
        window,
        _local: local,
        groq,
        openai,
        languages,
        groq_key,
        openai_key,
        save,
        status,
    })
}

fn save(ui: &Controls, config: &Arc<Mutex<Config>>) {
    let engine = if ui.groq.check_state() == nwg::RadioButtonState::Checked {
        Engine::Groq
    } else if ui.openai.check_state() == nwg::RadioButtonState::Checked {
        Engine::OpenAI
    } else {
        Engine::Local
    };

    let languages: Vec<String> = ui
        .languages
        .text()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    {
        let mut cfg = config.lock().unwrap();
        cfg.engine = engine;
        cfg.languages = languages;
        if let Err(e) = cfg.save() {
            log::error!("config save failed: {e}");
        }
    }

    // API keys go to the Credential Manager, or are cleared if blanked.
    persist_key(keystore::GROQ, &ui.groq_key.text());
    persist_key(keystore::OPENAI, &ui.openai_key.text());

    ui.status.set_text("Saved.");
}

fn persist_key(account: &str, value: &str) {
    if value.trim().is_empty() {
        let _ = keystore::clear_key(account);
    } else {
        let _ = keystore::set_key(account, value.trim());
    }
}
