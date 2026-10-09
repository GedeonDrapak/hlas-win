//! One Hlas per user session. A second launch (Start menu, installer finish
//! page, autostart) must not install a second keyboard hook - every
//! dictation would paste twice. It asks the running copy to open Settings
//! and exits.

use super::wide;
use once_cell::sync::Lazy;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND, LPARAM, WPARAM,
};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, RegisterWindowMessageW};

/// Broadcast message the running instance answers by opening Settings.
pub static SHOW_MESSAGE: Lazy<u32> = Lazy::new(|| {
    let name = wide("HlasForWindows.ShowSettings");
    unsafe { RegisterWindowMessageW(PCWSTR(name.as_ptr())) }
});

/// Holds the mutex for the life of the process.
pub struct Guard(#[allow(dead_code)] HANDLE);

pub fn acquire() -> Option<Guard> {
    let name = wide("Local\\HlasForWindows.SingleInstance");
    unsafe {
        let handle = CreateMutexW(None, true, PCWSTR(name.as_ptr())).ok()?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return None;
        }
        Some(Guard(handle))
    }
}

pub fn signal_existing() {
    let msg = *SHOW_MESSAGE;
    unsafe {
        let _ = PostMessageW(HWND(0xffff as _), msg, WPARAM(0), LPARAM(0));
    }
}
