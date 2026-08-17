//! Deliver transcribed text to whatever window has focus.
//!
//! Strategy mirrors the macOS TextInjector: stash the current clipboard, put our
//! text on it, synthesize Ctrl+V, then restore the clipboard. Paste is used
//! rather than per-character SendInput because it is instant and layout-safe
//! (Czech QWERTZ, dead keys, emoji all survive).

use anyhow::Result;
use arboard::Clipboard;
use std::{thread, time::Duration};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    VK_CONTROL, VK_V,
};

pub fn paste_text(text: &str) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    let mut clipboard = Clipboard::new()?;
    let saved = clipboard.get_text().ok();

    clipboard.set_text(text.to_owned())?;
    // Give the target app a beat to observe the new clipboard contents.
    thread::sleep(Duration::from_millis(30));
    send_ctrl_v();
    thread::sleep(Duration::from_millis(60));

    // Restore only if the clipboard still contains our text. This prevents us
    // from overwriting something the user copied while the target app handled
    // the paste shortcut.
    if let Some(prev) = saved {
        if clipboard.get_text().ok().as_deref() == Some(text) {
            let _ = clipboard.set_text(prev);
        }
    }
    Ok(())
}

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    Default::default()
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send_ctrl_v() {
    let inputs = [
        key(VK_CONTROL, false),
        key(VK_V, false),
        key(VK_V, true),
        key(VK_CONTROL, true),
    ];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}
