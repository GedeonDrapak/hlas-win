//! Delivering the transcript to the app the user dictated into.
//!
//! Same etiquette as the macOS TextInjector: remember which app was in front
//! when dictation started, paste only if it still is, put the text on the
//! clipboard tagged private, send Ctrl+V, and restore the previous clipboard
//! 0.4 s later - unless something else was copied in the meantime.

use super::clipboard;
use super::hotkey::INJECTED_TAG;
use std::time::Duration;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_RMENU, VK_RSHIFT,
    VK_RWIN, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

/// The app in front when a dictation started.
#[derive(Debug, Clone, Copy)]
pub struct Target {
    pid: u32,
}

/// Why a transcript was not pasted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocked {
    /// The user switched to another app (or Hlas's own window is in front).
    AppChanged,
    /// The target runs as administrator; Windows blocks synthetic input to it.
    Elevated,
}

impl Blocked {
    pub fn message(self) -> &'static str {
        match self {
            Blocked::AppChanged => "You switched applications. Your text is ready to copy.",
            Blocked::Elevated => {
                "This app runs as administrator, so Windows blocks pasting into it. Your text is ready to copy."
            }
        }
    }
}

fn foreground_pid() -> Option<u32> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        (pid != 0).then_some(pid)
    }
}

impl Target {
    pub fn capture() -> Option<Target> {
        foreground_pid().map(|pid| Target { pid })
    }

    fn check(&self) -> Result<(), Blocked> {
        match foreground_pid() {
            // Hlas's own windows count too: the welcome tour has a try-it box.
            Some(pid) if pid == self.pid => {
                if is_elevated(Some(pid)) && !is_elevated(None) {
                    Err(Blocked::Elevated)
                } else {
                    Ok(())
                }
            }
            _ => Err(Blocked::AppChanged),
        }
    }
}

/// Whether a process (or this one, for `None`) runs elevated. A process whose
/// token we may not even query runs above us, which is treated as elevated.
fn is_elevated(pid: Option<u32>) -> bool {
    unsafe {
        let process: HANDLE = match pid {
            Some(pid) => match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                Ok(h) => h,
                Err(_) => return true,
            },
            None => GetCurrentProcess(),
        };
        let mut token = HANDLE::default();
        let opened = OpenProcessToken(process, TOKEN_QUERY, &mut token).is_ok();
        if pid.is_some() {
            let _ = CloseHandle(process);
        }
        if !opened {
            return pid.is_some();
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok && elevation.TokenIsElevated != 0
    }
}

/// Pastes `text` into `target`. On `Err` nothing was pasted and the caller
/// shows the text in a window instead.
pub fn insert(text: &str, target: Option<&Target>) -> Result<(), Blocked> {
    let Some(target) = target else {
        return Err(Blocked::AppChanged);
    };
    target.check()?;

    let saved = clipboard::snapshot();
    let ours = match clipboard::set_text(text, true) {
        Ok(seq) => seq,
        Err(e) => {
            log::error!("clipboard write failed: {e}");
            return Err(Blocked::AppChanged);
        }
    };
    release_modifiers();
    std::thread::sleep(Duration::from_millis(30));
    send_ctrl_v();

    // Restore once the paste has landed, unless the user copied something.
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(400));
        if clipboard::sequence() != ours {
            return;
        }
        if let Some(saved) = saved {
            if !saved.is_empty() {
                if let Err(e) = clipboard::restore(&saved) {
                    log::warn!("clipboard restore failed: {e}");
                }
            }
        }
    });
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
                    KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: INJECTED_TAG,
            },
        },
    }
}

/// A held Shift, Alt or Win would turn Ctrl+V into a different shortcut.
fn release_modifiers() {
    let held: Vec<INPUT> = [VK_LSHIFT, VK_RSHIFT, VK_LMENU, VK_RMENU, VK_LWIN, VK_RWIN]
        .into_iter()
        .filter(|vk| unsafe { GetAsyncKeyState(vk.0 as i32) } < 0)
        .map(|vk| key(vk, true))
        .collect();
    if !held.is_empty() {
        unsafe {
            SendInput(&held, std::mem::size_of::<INPUT>() as i32);
        }
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

/// True while Shift is held (forces a plain transcript, like Option on macOS).
pub fn shift_held() -> bool {
    unsafe { GetAsyncKeyState(0x10) < 0 }
}
