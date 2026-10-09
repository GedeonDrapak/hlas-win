//! User preferences, persisted as JSON in %LOCALAPPDATA%\Hlas\config.json.
//! API keys are never stored here; they live in the Windows Credential Manager.

use super::text::Replacement;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Which transcription backend runs. Variant names stay as in 0.1.0 so old
/// config files keep loading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Engine {
    /// whisper.cpp on this PC. Private, free, offline.
    #[default]
    Local,
    /// Groq cloud, whisper-large-v3-turbo. Fastest, BYOK.
    Groq,
    /// OpenAI cloud, gpt-transcribe. Best accuracy, BYOK.
    OpenAI,
}

impl Engine {
    pub const ALL: [Engine; 3] = [Engine::Local, Engine::Groq, Engine::OpenAI];

    pub fn id(self) -> &'static str {
        match self {
            Engine::Local => "local",
            Engine::Groq => "groq",
            Engine::OpenAI => "openai",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Engine::Local => "On this PC - private & free",
            Engine::Groq => "Groq cloud - fastest, about $3/month",
            Engine::OpenAI => "OpenAI cloud - best accuracy, about $8/month",
        }
    }
}

/// Plain transcript, or transcript reformatted by an LLM ("Smart text").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    #[default]
    Transcript,
    Smart,
}

impl OutputMode {
    pub fn title(self) -> &'static str {
        match self {
            OutputMode::Transcript => "Transcript",
            OutputMode::Smart => "Smart text",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub engine: Engine,
    /// Active language code, or "auto".
    pub language: String,
    /// Quick-switch languages shown in the tray.
    pub favorite_languages: Vec<String>,
    /// Virtual-key code of the push-to-talk key. Default: Right Ctrl (0xA3).
    /// Windows has no Fn scancode, so the Mac "hold Fn" gesture maps here.
    pub hotkey_vk: u32,
    pub launch_at_login: bool,
    /// Names and jargon the model should learn to spell.
    pub vocabulary: Vec<String>,
    /// Exact spellings applied after transcription.
    pub replacements: Vec<Replacement>,
    pub output_mode: OutputMode,
    pub history_enabled: bool,
    /// 0 keeps history until cleared.
    pub history_retention_days: u32,
    /// Seconds the local model stays in RAM after use. 0 frees it at once.
    pub model_keep_alive_secs: u64,
    /// Input device name. Empty follows the Windows default microphone.
    pub input_device: String,
    pub has_onboarded: bool,
    /// 0.1.0 stored favourites here; migrated on load, never written back.
    #[serde(skip_serializing)]
    pub languages: Option<Vec<String>>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            engine: Engine::Local,
            language: "cs".into(),
            favorite_languages: vec!["cs".into(), "en".into()],
            hotkey_vk: 0xA3,
            launch_at_login: false,
            vocabulary: Vec::new(),
            replacements: Vec::new(),
            output_mode: OutputMode::Transcript,
            history_enabled: true,
            history_retention_days: 0,
            model_keep_alive_secs: 180,
            input_device: String::new(),
            has_onboarded: false,
            languages: None,
        }
    }
}

impl Config {
    /// %LOCALAPPDATA%\Hlas - config, history, models and log. Local, not
    /// roaming: a 547 MB model and private transcripts must not sync.
    pub fn dir() -> Result<PathBuf> {
        Ok(dirs::data_local_dir()
            .context("no local app data directory")?
            .join("Hlas"))
    }

    /// Where 0.1.0 kept its files (%APPDATA%\Hlas).
    pub fn legacy_dir() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("Hlas"))
    }

    pub fn models_dir() -> Result<PathBuf> {
        Ok(Self::dir()?.join("models"))
    }

    /// The active language for Whisper, `None` meaning auto-detect.
    pub fn whisper_language(&self) -> Option<&str> {
        match self.language.as_str() {
            "" | "auto" => None,
            code => Some(code),
        }
    }

    pub fn from_json(text: &str) -> Config {
        let mut cfg: Config = serde_json::from_str(text).unwrap_or_default();
        if let Some(old) = cfg.languages.take() {
            let old: Vec<String> = old.into_iter().filter(|l| !l.trim().is_empty()).collect();
            if !old.is_empty() {
                cfg.language = old[0].clone();
                cfg.favorite_languages = old;
            }
        }
        cfg.sanitize();
        cfg
    }

    fn sanitize(&mut self) {
        if self.language.trim().is_empty() {
            self.language = "auto".into();
        }
        self.favorite_languages
            .retain(|l| !l.trim().is_empty() && l != "auto");
        self.favorite_languages.dedup();
        if self.hotkey_vk == 0 {
            self.hotkey_vk = 0xA3;
        }
    }

    /// Loads `dir/config.json`, falling back to the 0.1.0 location, then to
    /// defaults. A missing or corrupt file never stops the app from starting.
    pub fn load_from(dir: &Path, legacy: Option<&Path>) -> Config {
        let path = dir.join("config.json");
        if let Ok(text) = std::fs::read_to_string(&path) {
            return Self::from_json(&text);
        }
        if let Some(legacy) = legacy {
            if let Ok(text) = std::fs::read_to_string(legacy.join("config.json")) {
                return Self::from_json(&text);
            }
        }
        Config::default()
    }

    pub fn load() -> Config {
        match Self::dir() {
            Ok(dir) => Self::load_from(&dir, Self::legacy_dir().as_deref()),
            Err(_) => Config::default(),
        }
    }

    pub fn save_to(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join("config.json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, dir.join("config.json"))?;
        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::dir()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_the_0_1_config() {
        let old = r#"{"engine":"Groq","languages":["en","cs"],"hotkey_vk":163,"launch_at_login":true,"vocabulary":["Hlas"]}"#;
        let cfg = Config::from_json(old);
        assert_eq!(cfg.engine, Engine::Groq);
        assert_eq!(cfg.language, "en");
        assert_eq!(cfg.favorite_languages, vec!["en", "cs"]);
        assert!(cfg.launch_at_login);
        assert!(cfg.history_enabled, "new fields take defaults");
        assert!(!cfg.has_onboarded);
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(
            !json.contains("\"languages\""),
            "legacy field is not written back"
        );
    }

    #[test]
    fn corrupt_or_empty_falls_back_to_defaults() {
        assert_eq!(Config::from_json("{not json"), Config::default());
        let cfg = Config::from_json(
            r#"{"language":"","favorite_languages":["auto","","de"],"hotkey_vk":0}"#,
        );
        assert_eq!(cfg.language, "auto");
        assert_eq!(cfg.whisper_language(), None);
        assert_eq!(cfg.favorite_languages, vec!["de"]);
        assert_eq!(cfg.hotkey_vk, 0xA3);
    }

    #[test]
    fn round_trip_and_legacy_location() {
        let base = std::env::temp_dir().join(format!("hlas-cfg-{}", std::process::id()));
        let (dir, legacy) = (base.join("local"), base.join("roaming"));
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("config.json"), r#"{"engine":"OpenAI"}"#).unwrap();
        assert_eq!(
            Config::load_from(&dir, Some(&legacy)).engine,
            Engine::OpenAI
        );

        let cfg = Config {
            output_mode: OutputMode::Smart,
            replacements: vec![Replacement {
                from: "a".into(),
                to: "b".into(),
            }],
            ..Default::default()
        };
        cfg.save_to(&dir).unwrap();
        assert_eq!(Config::load_from(&dir, Some(&legacy)), cfg);
        std::fs::remove_dir_all(&base).ok();
    }
}
