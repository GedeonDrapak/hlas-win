//! Cloud transcription for Groq and OpenAI. Both speak the OpenAI
//! /audio/transcriptions multipart API, so one code path covers them; only the
//! endpoint and model id differ. The 16 kHz mono f32 buffer is encoded to a WAV
//! in memory and uploaded.

use crate::config::Config;
use anyhow::{anyhow, Result};

#[derive(Clone, Copy)]
pub enum Provider {
    Groq,
    OpenAI,
}

impl Provider {
    fn endpoint(self) -> &'static str {
        match self {
            Provider::Groq => "https://api.groq.com/openai/v1/audio/transcriptions",
            Provider::OpenAI => "https://api.openai.com/v1/audio/transcriptions",
        }
    }
    fn model(self) -> &'static str {
        match self {
            Provider::Groq => "whisper-large-v3-turbo",
            Provider::OpenAI => "gpt-4o-transcribe",
        }
    }
}

pub fn transcribe(
    provider: Provider,
    api_key: &str,
    config: &Config,
    samples: &[f32],
) -> Result<String> {
    let wav = encode_wav(samples, 16_000);

    let part = reqwest::blocking::multipart::Part::bytes(wav)
        .file_name("audio.wav")
        .mime_str("audio/wav")?;
    let mut form = reqwest::blocking::multipart::Form::new()
        .text("model", provider.model())
        .text("response_format", "json")
        .part("file", part);
    if let Some(lang) = config.languages.first() {
        form = form.text("language", lang.clone());
    }

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(provider.endpoint())
        .bearer_auth(api_key)
        .multipart(form)
        .send()?;

    if !resp.status().is_success() {
        let code = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(anyhow!("transcription HTTP {code}: {body}"));
    }

    let value: serde_json::Value = resp.json()?;
    let text = value
        .get("text")
        .and_then(|t| t.as_str())
        .unwrap_or_default()
        .trim()
        .to_string();
    Ok(text)
}

/// Minimal 16-bit PCM WAV encoder for mono f32 samples.
fn encode_wav(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let bits = 16u16;
    let channels = 1u16;
    let byte_rate = sample_rate * channels as u32 * (bits / 8) as u32;
    let block_align = channels * (bits / 8);
    let data_len = samples.len() as u32 * (bits / 8) as u32;

    let mut buf = Vec::with_capacity(44 + data_len as usize);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_len).to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // PCM fmt chunk size
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&bits.to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_len.to_le_bytes());
    for &s in samples {
        let clamped = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        buf.extend_from_slice(&clamped.to_le_bytes());
    }
    buf
}
