//! Global push-to-talk via a low-level keyboard hook (WH_KEYBOARD_LL).
//!
//! Windows has no Fn scancode, so we watch a normal virtual key (Right Ctrl by
//! default) and emit Down/Up edges. This mirrors the macOS CGEventTap approach:
//! hold to talk, quick-tap to latch. The hook callback runs on the thread that
//! installed it, which must pump a message loop - we own a dedicated thread for
//! exactly that.

use crossbeam_channel::{Receiver, Sender};
use once_cell::sync::OnceCell;
use std::sync::atomic::{AtomicU32, Ordering};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage, HHOOK,
    KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEdge {
    Down,
    Up,
}

static SENDER: OnceCell<Sender<KeyEdge>> = OnceCell::new();
/// Virtual-key code to watch. Set before the hook thread starts.
static WATCH_VK: AtomicU32 = AtomicU32::new(0xA3); // VK_RCONTROL
/// True while the watched key is held, so we debounce auto-repeat WM_KEYDOWNs.
static IS_DOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Starts the hook on a dedicated thread and returns the edge stream.
pub fn start(watch_vk: u32) -> Receiver<KeyEdge> {
    let (tx, rx) = crossbeam_channel::unbounded();
    let _ = SENDER.set(tx);
    WATCH_VK.store(watch_vk, Ordering::SeqCst);

    std::thread::spawn(|| unsafe {
        let hook: HHOOK = SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_proc), None, 0)
            .expect("failed to install keyboard hook");
        let _ = hook;

        // Pump messages so the hook keeps firing on this thread.
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });

    rx
}

/// Updates the watched key at runtime (Settings change). Wired once the
/// settings window exposes hotkey rebinding.
#[allow(dead_code)]
pub fn set_watch_vk(vk: u32) {
    WATCH_VK.store(vk, Ordering::SeqCst);
}

unsafe extern "system" fn low_level_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let vk = info.vkCode;
        if vk == WATCH_VK.load(Ordering::SeqCst) {
            let msg = wparam.0 as u32;
            match msg {
                WM_KEYDOWN | WM_SYSKEYDOWN => {
                    // Suppress OS key-repeat: only the first down edge matters.
                    if !IS_DOWN.swap(true, Ordering::SeqCst) {
                        emit(KeyEdge::Down);
                    }
                }
                WM_KEYUP | WM_SYSKEYUP => {
                    if IS_DOWN.swap(false, Ordering::SeqCst) {
                        emit(KeyEdge::Up);
                    }
                }
                _ => {}
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

fn emit(edge: KeyEdge) {
    if let Some(tx) = SENDER.get() {
        let _ = tx.send(edge);
    }
}
