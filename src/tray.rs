//! System-tray icon, menu, and the main-thread Win32 message loop.
//!
//! tray-icon creates a message-only window that needs messages pumped on the
//! thread that built it, so the tray lives on the main thread and we run a
//! classic GetMessage loop here. Menu selections arrive on a global channel that
//! a small consumer thread drains - it never touches window handles, so running
//! off-thread is safe.

use crate::config::{Config, Engine};
use crate::{autostart, settings};
use anyhow::Result;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIconBuilder};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE, WM_QUIT,
};

pub fn run(config: Arc<Mutex<Config>>) -> Result<()> {
    let menu = Menu::new();

    let engine_local = CheckMenuItem::new("On this PC (local)", true, false, None);
    let engine_groq = CheckMenuItem::new("Groq cloud", true, false, None);
    let engine_openai = CheckMenuItem::new("OpenAI cloud", true, false, None);
    let engine_menu = Submenu::new("Engine", true);
    engine_menu.append_items(&[&engine_local, &engine_groq, &engine_openai])?;

    let launch = CheckMenuItem::new("Launch at login", true, autostart::is_enabled(), None);
    let settings_item = MenuItem::new("Settings…", true, None);
    let quit = MenuItem::new("Quit Hlas", true, None);

    menu.append_items(&[
        &engine_menu,
        &PredefinedMenuItem::separator(),
        &settings_item,
        &launch,
        &PredefinedMenuItem::separator(),
        &quit,
    ])?;

    reflect_engine(&config, &engine_local, &engine_groq, &engine_openai);

    let _tray = TrayIconBuilder::new()
        .with_tooltip("Hlas - hold to dictate")
        .with_icon(app_icon())
        .with_menu(Box::new(menu))
        .build()?;

    // Menu items and their ids are !Send (Rc-backed), so everything stays on
    // this thread. A PeekMessage loop pumps the tray's window messages and
    // drains the menu-event channel in the same pass.
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
            let id = event.id;
            if id == quit.id() {
                return Ok(());
            } else if id == settings_item.id() {
                settings::open(config.clone());
            } else if id == launch.id() {
                let enabled = !autostart::is_enabled();
                let _ = autostart::set(enabled);
                launch.set_checked(autostart::is_enabled());
                let mut cfg = config.lock().unwrap();
                cfg.launch_at_login = enabled;
                let _ = cfg.save();
            } else if id == engine_local.id() || id == engine_groq.id() || id == engine_openai.id()
            {
                let engine = if id == engine_local.id() {
                    Engine::Local
                } else if id == engine_groq.id() {
                    Engine::Groq
                } else {
                    Engine::OpenAI
                };
                {
                    let mut cfg = config.lock().unwrap();
                    cfg.engine = engine;
                    let _ = cfg.save();
                }
                engine_local.set_checked(engine == Engine::Local);
                engine_groq.set_checked(engine == Engine::Groq);
                engine_openai.set_checked(engine == Engine::OpenAI);
            }
        }

        std::thread::sleep(Duration::from_millis(16));
    }
}

fn reflect_engine(
    config: &Arc<Mutex<Config>>,
    local: &CheckMenuItem,
    groq: &CheckMenuItem,
    openai: &CheckMenuItem,
) {
    let engine = config.lock().unwrap().engine;
    local.set_checked(engine == Engine::Local);
    groq.set_checked(engine == Engine::Groq);
    openai.set_checked(engine == Engine::OpenAI);
}

fn app_icon() -> Icon {
    // A 32x32 bio-digital-green rounded square as a stand-in mark. The installer
    // ships a real .ico; this keeps the tray populated even if that is missing.
    const S: u32 = 32;
    let mut rgba = Vec::with_capacity((S * S * 4) as usize);
    for y in 0..S {
        for x in 0..S {
            let edge = x < 2 || y < 2 || x >= S - 2 || y >= S - 2;
            if edge {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                rgba.extend_from_slice(&[0x00, 0xE6, 0x7A, 0xFF]);
            }
        }
    }
    Icon::from_rgba(rgba, S, S).expect("valid icon")
}
