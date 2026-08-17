//! Transcription backends. `transcribe` dispatches on the configured engine and
//! returns plain text for the 16 kHz mono f32 recording.

mod cloud;
mod local;
pub mod model;

use crate::config::{Config, Engine};
use crate::keystore;
use anyhow::{anyhow, Result};

pub fn transcribe(config: &Config, samples: &[f32]) -> Result<String> {
    if samples.is_empty() {
        return Ok(String::new());
    }
    match config.engine {
        Engine::Local => local::transcribe(config, samples),
        Engine::Groq => {
            let key =
                keystore::get_key(keystore::GROQ).ok_or_else(|| anyhow!("no Groq API key set"))?;
            cloud::transcribe(cloud::Provider::Groq, &key, config, samples)
        }
        Engine::OpenAI => {
            let key = keystore::get_key(keystore::OPENAI)
                .ok_or_else(|| anyhow!("no OpenAI API key set"))?;
            cloud::transcribe(cloud::Provider::OpenAI, &key, config, samples)
        }
    }
}

/// Download the local model before a first local dictation. Kept separate from
/// `transcribe` so the UI can clearly show why the first request takes longer.
pub fn ensure_local_model() -> Result<()> {
    model::ensure(|downloaded, total| {
        if downloaded % (32 * 1024 * 1024) < 64 * 1024 {
            log::info!(
                "model download: {} / {} MiB",
                downloaded / 1024 / 1024,
                total / 1024 / 1024
            );
        }
    })?;
    Ok(())
}

pub fn local_model_present() -> bool {
    model::is_present()
}
