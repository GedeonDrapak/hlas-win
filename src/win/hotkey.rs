//! Global push-to-talk via a low-level keyboard hook (WH_KEYBOARD_LL).
//!
//! Windows has no Fn scancode, so Hlas watches a normal key (Right Ctrl by
//! default). The hook reports:
//! - `Down` / `Up` edges of the push-to-talk key (auto-repeat filtered);
//! - `Chord` when another key goes down while it is held, so Right Ctrl+C
//!   stays a copy shortcut instead of starting a dictation;
//! - `Escape` while a dictation is active (and swallows that Esc).
//!
//! Keys Hlas injects itself (the Ctrl+V paste) carry `INJECTED_TAG` and are
//! ignored. Windows silently drops low-level hooks that stall, so the hook is
//! re-installed every ten minutes as a safety net.

use crossbeam_channel::Sender;
use once_cell::sync::OnceCell;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetTimer, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP,
    WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TIMER,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    Down,
    Up,
    Chord,
    Escape,
}

/// `dwExtraInfo` marker on input Hlas synthesizes ("HLAS").
pub const INJECTED_TAG: usize = 0x484C_4153;

const VK_ESCAPE: u32 = 0x1B;
const REINSTALL_MS: u32 = 10 * 60 * 1000;

static SENDER: OnceCell<Sender<HotkeyEvent>> = OnceCell::new();
static WATCH_VK: AtomicU32 = AtomicU32::new(0xA3);
static IS_DOWN: AtomicBool = AtomicBool::new(false);
static CHORD_SENT: AtomicBool = AtomicBool::new(false);
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Starts the hook thread. Events go to `tx`.
pub fn start(watch_vk: u32, tx: Sender<HotkeyEvent>) {
    let _ = SENDER.set(tx);
    WATCH_VK.store(watch_vk, Ordering::SeqCst);

    std::thread::Builder::new()
        .name("hlas-hotkey".into())
        .spawn(|| unsafe {
            let mut hook = install();
            // A thread timer (no window) arrives as WM_TIMER in this loop.
            SetTimer(None, 0, REINSTALL_MS, None);
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                if msg.message == WM_TIMER {
                    if let Some(h) = hook.take() {
                        let _ = UnhookWindowsHookEx(h);
                    }
                    hook = install();
                    continue;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        })
        .expect("hotkey thread");
}

unsafe fn install() -> Option<HHOOK> {
    match SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_proc), None, 0) {
        Ok(h) => Some(h),
        Err(e) => {
            log::error!("keyboard hook install failed: {e}");
            None
        }
    }
}

/// Changes the push-to-talk key at runtime (Settings).
pub fn set_watch_vk(vk: u32) {
    if WATCH_VK.swap(vk, Ordering::SeqCst) != vk {
        IS_DOWN.store(false, Ordering::SeqCst);
    }
}

/// While a dictation is active, Esc cancels it instead of reaching the app.
pub fn set_active(active: bool) {
    ACTIVE.store(active, Ordering::SeqCst);
}

/// True from the start of a dictation until its result is delivered.
pub fn is_active() -> bool {
    ACTIVE.load(Ordering::SeqCst)
}

unsafe extern "system" fn low_level_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        if info.dwExtraInfo != INJECTED_TAG {
            let vk = info.vkCode;
            let msg = wparam.0 as u32;
            let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
            let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
            if vk == WATCH_VK.load(Ordering::SeqCst) {
                if is_down && !IS_DOWN.swap(true, Ordering::SeqCst) {
                    CHORD_SENT.store(false, Ordering::SeqCst);
                    emit(HotkeyEvent::Down);
                } else if is_up && IS_DOWN.swap(false, Ordering::SeqCst) {
                    emit(HotkeyEvent::Up);
                }
            } else if is_down {
                if vk == VK_ESCAPE && ACTIVE.load(Ordering::SeqCst) {
                    emit(HotkeyEvent::Escape);
                    return LRESULT(1);
                }
                if IS_DOWN.load(Ordering::SeqCst) && !CHORD_SENT.swap(true, Ordering::SeqCst) {
                    emit(HotkeyEvent::Chord);
                }
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

fn emit(event: HotkeyEvent) {
    if let Some(tx) = SENDER.get() {
        let _ = tx.send(event);
    }
}
