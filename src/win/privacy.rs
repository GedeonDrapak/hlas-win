//! Windows microphone privacy switches. Desktop apps are blocked when any of
//! the three registry toggles says "Deny"; capture then fails or records
//! silence, so onboarding and errors point the user to the right page.

use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use winreg::RegKey;

const KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicAccess {
    Allowed,
    /// "Microphone access" is off for the whole device.
    BlockedDevice,
    /// "Let apps access your microphone" is off.
    BlockedApps,
    /// "Let desktop apps access your microphone" is off.
    BlockedDesktopApps,
}

impl MicAccess {
    pub fn is_allowed(self) -> bool {
        self == MicAccess::Allowed
    }

    pub fn message(self) -> &'static str {
        match self {
            MicAccess::Allowed => "Microphone access is on.",
            MicAccess::BlockedDevice => "Microphone access is off for this device.",
            MicAccess::BlockedApps => "Apps are not allowed to use the microphone.",
            MicAccess::BlockedDesktopApps => "Desktop apps are not allowed to use the microphone.",
        }
    }
}

fn denied(root: &RegKey, path: &str) -> bool {
    root.open_subkey(path)
        .and_then(|k| k.get_value::<String, _>("Value"))
        .map(|v| v.eq_ignore_ascii_case("Deny"))
        .unwrap_or(false)
}

pub fn mic_access() -> MicAccess {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if denied(&hklm, KEY) {
        MicAccess::BlockedDevice
    } else if denied(&hkcu, KEY) {
        MicAccess::BlockedApps
    } else if denied(&hkcu, &format!(r"{KEY}\NonPackaged")) {
        MicAccess::BlockedDesktopApps
    } else {
        MicAccess::Allowed
    }
}
