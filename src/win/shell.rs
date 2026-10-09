//! Opening URLs, folders and Windows Settings pages.

use super::wide;
use windows::core::PCWSTR;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

pub fn open(target: &str) {
    let op = wide("open");
    let file = wide(target);
    unsafe {
        ShellExecuteW(
            None,
            PCWSTR(op.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

pub const MIC_PRIVACY: &str = "ms-settings:privacy-microphone";
pub const PRIVACY: &str = "https://github.com/GedeonDrapak/hlas/blob/main/docs/PRIVACY.md";
