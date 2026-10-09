//! Groq and OpenAI transcription. Both speak the OpenAI
//! /audio/transcriptions multipart API; only endpoint and model differ.

use crate::core::errors::NetError;
use crate::core::wav;
use anyhow::Result;
use std::time::Duration;

#[derive(Clone, Copy)]
pub enum Provider {
    Groq,
    OpenAI,
}

impl Provider {
    fn name(self) -> &'static str {
        match self {
            Provider::Groq => "Groq",
            Provider::OpenAI => "OpenAI",
        }
    }
    fn endpoint(self) -> &'static str {
        match self {
            Provider::Groq => "https://api.groq.com/openai/v1/audio/transcriptions",
            Provider::OpenAI => "https://api.openai.com/v1/audio/transcriptions",
        }
    }
    fn model(self) -> &'static str {
        match self {
            Provider::Groq => "whisper-large-v3-turbo",
            Provider::OpenAI => "gpt-transcribe",
        }
    }
}

/// Maps a transport error to the user-facing kind, like macOS does.
pub fn classify(err: &reqwest::Error, provider: &'static str) -> NetError {
    if err.is_timeout() {
        NetError::TimedOut { provider }
    } else if err.is_connect() {
        NetError::Offline
    } else {
        NetError::Unavailable { provider }
    }
}

pub fn transcribe(
    provider: Provider,
    api_key: &str,
    samples: &[f32],
    language: Option<&str>,
    prompt: Option<&str>,
) -> Result<String> {
    let name = provider.name();
    let audio_secs = samples.len() as f64 / 16_000.0;
    let part = reqwest::blocking::multipart::Part::bytes(wav::encode(samples, 16_000))
        .file_name("audio.wav")
        .mime_str("audio/wav")?;
    let mut form = reqwest::blocking::multipart::Form::new()
        .text("model", provider.model())
        .text("response_format", "json")
        .text("temperature", "0")
        .part("file", part);
    if let Some(lang) = language {
        form = form.text("language", lang.to_string());
    }
    if let Some(prompt) = prompt {
        form = form.text("prompt", prompt.to_string());
    }

    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs_f64(
            (audio_secs * 0.3).clamp(30.0, 120.0),
        ))
        .build()?;
    let resp = client
        .post(provider.endpoint())
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .map_err(|e| classify(&e, name))?;
    let status = resp.status();
    if !status.is_success() {
        log::warn!("{name} transcription HTTP {}", status.as_u16());
        return Err(NetError::Http {
            status: status.as_u16(),
            provider: name,
        }
        .into());
    }
    let value: serde_json::Value = resp
        .json()
        .map_err(|_| NetError::InvalidResponse { provider: name })?;
    let text = value
        .get("text")
        .and_then(|t| t.as_str())
        .ok_or(NetError::InvalidResponse { provider: name })?;
    Ok(text.trim().to_string())
}
