//! Whisper-supported languages with native names, same list as macOS.

pub const AUTO: (&str, &str) = ("auto", "Auto-detect");

pub const ALL: &[(&str, &str)] = &[
    ("cs", "Čeština"),
    ("sk", "Slovenčina"),
    ("en", "English"),
    ("de", "Deutsch"),
    ("es", "Español"),
    ("fr", "Français"),
    ("it", "Italiano"),
    ("pl", "Polski"),
    ("pt", "Português"),
    ("ru", "Русский"),
    ("uk", "Українська"),
    ("nl", "Nederlands"),
    ("sv", "Svenska"),
    ("no", "Norsk"),
    ("da", "Dansk"),
    ("fi", "Suomi"),
    ("hu", "Magyar"),
    ("ro", "Română"),
    ("bg", "Български"),
    ("hr", "Hrvatski"),
    ("sr", "Srpski"),
    ("sl", "Slovenščina"),
    ("el", "Ελληνικά"),
    ("tr", "Türkçe"),
    ("ar", "العربية"),
    ("he", "עברית"),
    ("hi", "हिन्दी"),
    ("zh", "中文 (Mandarin)"),
    ("yue", "粵語 (Cantonese)"),
    ("ja", "日本語"),
    ("ko", "한국어"),
    ("vi", "Tiếng Việt"),
    ("th", "ไทย"),
    ("id", "Bahasa Indonesia"),
    ("ms", "Bahasa Melayu"),
    ("ca", "Català"),
    ("gl", "Galego"),
    ("eu", "Euskara"),
    ("et", "Eesti"),
    ("lv", "Latviešu"),
    ("lt", "Lietuvių"),
    ("fa", "فارسی"),
    ("ur", "اردو"),
    ("ta", "தமிழ்"),
    ("te", "తెలుగు"),
    ("bn", "বাংলা"),
    ("mr", "मराठी"),
    ("sw", "Kiswahili"),
    ("af", "Afrikaans"),
    ("kk", "Қазақша"),
    ("az", "Azərbaycanca"),
    ("ka", "ქართული"),
    ("hy", "Հայերեն"),
    ("sq", "Shqip"),
    ("mk", "Македонски"),
    ("bs", "Bosanski"),
    ("is", "Íslenska"),
    ("ga", "Gaeilge"),
    ("cy", "Cymraeg"),
    ("mt", "Malti"),
    ("lb", "Lëtzebuergesch"),
    ("be", "Беларуская"),
    ("tl", "Tagalog"),
    ("la", "Latina"),
];

pub fn name(code: &str) -> String {
    if code == AUTO.0 {
        return AUTO.1.to_string();
    }
    ALL.iter()
        .find(|(c, _)| *c == code)
        .map(|(_, n)| n.to_string())
        .unwrap_or_else(|| code.to_uppercase())
}

pub fn is_known(code: &str) -> bool {
    code == AUTO.0 || ALL.iter().any(|(c, _)| *c == code)
}

/// Parses "cs, en de" into known codes, keeping order and dropping duplicates.
pub fn parse_list(input: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for code in input.split(|c: char| c == ',' || c.is_whitespace()) {
        let code = code.trim().to_lowercase();
        if !code.is_empty() && code != "auto" && is_known(&code) && !out.contains(&code) {
            out.push(code);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_parsing() {
        assert_eq!(name("cs"), "Čeština");
        assert_eq!(name("auto"), "Auto-detect");
        assert_eq!(name("xx"), "XX");
        assert_eq!(parse_list("cs, EN de cs xx auto"), vec!["cs", "en", "de"]);
        assert!(ALL.len() >= 60);
    }
}
