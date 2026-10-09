//! Transcription engines and Smart text.

pub mod cloud;
pub mod local;
pub mod model;
pub mod smart;

use super::keystore;
use crate::core::config::{Config, Engine};
use crate::core::errors::NetError;
use crate::core::text::{self, SmartProvider};
use anyhow::Result;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Transcribes 16 kHz mono audio (already padded) with the configured engine.
pub fn transcribe(cfg: &Config, samples: &[f32], cancel: &Arc<AtomicBool>) -> Result<String> {
    let language = cfg.whisper_language();
    match cfg.engine {
        Engine::Local => {
            let prompt = text::whisper_prompt(language, &cfg.vocabulary);
            local::transcribe(
                samples,
                language,
                prompt.as_deref(),
                cancel,
                cfg.model_keep_alive_secs,
            )
        }
        Engine::Groq => {
            let key = keystore::get_key(keystore::GROQ)
                .ok_or(NetError::MissingKey { provider: "Groq" })?;
            let prompt = text::vocabulary_prompt(&cfg.vocabulary);
            cloud::transcribe(
                cloud::Provider::Groq,
                &key,
                samples,
                language,
                prompt.as_deref(),
            )
        }
        Engine::OpenAI => {
            let key = keystore::get_key(keystore::OPENAI)
                .ok_or(NetError::MissingKey { provider: "OpenAI" })?;
            let prompt = text::vocabulary_prompt(&cfg.vocabulary);
            cloud::transcribe(
                cloud::Provider::OpenAI,
                &key,
                samples,
                language,
                prompt.as_deref(),
            )
        }
    }
}

/// The Smart text provider and its key, if any key is configured.
pub fn smart_provider(cfg: &Config) -> Option<(SmartProvider, String)> {
    let groq = keystore::get_key(keystore::GROQ);
    let openai = keystore::get_key(keystore::OPENAI);
    let provider =
        text::choose_smart_provider(cfg.engine == Engine::Groq, groq.is_some(), openai.is_some())?;
    let key = match provider {
        SmartProvider::Groq => groq?,
        SmartProvider::OpenAI => openai?,
    };
    Some((provider, key))
}
