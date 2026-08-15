//! User configuration, persisted as JSON under %APPDATA%\Hlas\config.json.
//! API keys are NOT stored here - they live in the Windows Credential Manager
//! via `keystore`. This file only records non-secret preferences.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Which transcription backend runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Engine {
    /// whisper.cpp on this PC. Private, free, offline.
    Local,
    /// Groq cloud, whisper-large-v3-turbo. Fastest, BYOK.
    Groq,
    /// OpenAI cloud, gpt-4o-transcribe. Best accuracy, BYOK.
    OpenAI,
}

impl Default for Engine {
    fn default() -> Self {
        Engine::Local
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub engine: Engine,
    /// Whisper language hints, e.g. ["cs", "en"]. Empty = auto-detect.
    pub languages: Vec<String>,
    /// Virtual-key code of the push-to-talk key. Default: Right Ctrl (0xA3).
    /// Windows has no Fn scancode, so the Mac "hold Fn" gesture maps here.
    pub hotkey_vk: u32,
    /// Launch Hlas when the user signs in.
    pub launch_at_login: bool,
    /// Words the model should learn to spell (names, jargon).
    pub vocabulary: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            engine: Engine::Local,
            languages: vec!["cs".into(), "en".into()],
            hotkey_vk: 0xA3, // VK_RCONTROL
            launch_at_login: false,
            vocabulary: Vec::new(),
        }
    }
}

impl Config {
    pub fn dir() -> Result<PathBuf> {
        let base = dirs::config_dir().context("no %APPDATA% directory")?;
        Ok(base.join("Hlas"))
    }

    pub fn path() -> Result<PathBuf> {
        Ok(Self::dir()?.join("config.json"))
    }

    pub fn models_dir() -> Result<PathBuf> {
        Ok(Self::dir()?.join("models"))
    }

    pub fn load() -> Config {
        // A missing or corrupt config is not fatal: fall back to defaults so the
        // app always starts.
        match Self::path().and_then(|p| Ok(std::fs::read_to_string(p)?)) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => Config::default(),
        }
    }

    pub fn save(&self) -> Result<()> {
        let dir = Self::dir()?;
        std::fs::create_dir_all(&dir)?;
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(Self::path()?, text)?;
        Ok(())
    }
}
