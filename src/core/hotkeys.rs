//! Push-to-talk keys offered in Settings. All of them are harmless when they
//! also reach the focused app, so Hlas never has to swallow them.

pub const DEFAULT_VK: u32 = 0xA3;

pub const KEYS: &[(u32, &str)] = &[
    (0xA3, "Right Ctrl"),
    (0xA5, "Right Alt (AltGr)"),
    (0xA1, "Right Shift"),
    (0x91, "Scroll Lock"),
    (0x13, "Pause / Break"),
    (0x7C, "F13"),
    (0x7D, "F14"),
    (0x7E, "F15"),
    (0x7F, "F16"),
];

pub fn name(vk: u32) -> String {
    KEYS.iter()
        .find(|(k, _)| *k == vk)
        .map(|(_, n)| n.to_string())
        .unwrap_or_else(|| format!("Key 0x{vk:02X}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_key_is_listed_first() {
        assert_eq!(KEYS[0].0, DEFAULT_VK);
        assert_eq!(name(0xA3), "Right Ctrl");
        assert_eq!(name(0x41), "Key 0x41");
    }
}
