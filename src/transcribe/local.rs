//! Local transcription with whisper-rs (whisper.cpp). The context is built lazily
//! on first use and cached for the process lifetime.

use super::model;
use crate::config::Config;
use anyhow::{anyhow, Result};
use once_cell::sync::OnceCell;
use std::sync::Mutex;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

static CONTEXT: OnceCell<Mutex<WhisperContext>> = OnceCell::new();

fn context() -> Result<&'static Mutex<WhisperContext>> {
    if let Some(ctx) = CONTEXT.get() {
        return Ok(ctx);
    }
    let path = model::path()?;
    if !model::is_present() {
        return Err(anyhow!("local model not downloaded yet"));
    }
    let ctx = WhisperContext::new_with_params(
        path.to_str().ok_or_else(|| anyhow!("non-utf8 model path"))?,
        WhisperContextParameters::default(),
    )?;
    let _ = CONTEXT.set(Mutex::new(ctx));
    CONTEXT.get().ok_or_else(|| anyhow!("context init race"))
}

pub fn transcribe(config: &Config, samples: &[f32]) -> Result<String> {
    let ctx_lock = context()?;
    let ctx = ctx_lock.lock().map_err(|_| anyhow!("whisper ctx poisoned"))?;
    let mut state = ctx.create_state()?;

    let mut params = FullParams::new(SamplingStrategy::BeamSearch {
        beam_size: 5,
        patience: 0.0,
    });
    params.set_translate(false);
    params.set_print_progress(false);
    params.set_print_special(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_suppress_blank(true);

    // First configured language wins as a hint; empty list means auto-detect.
    if let Some(lang) = config.languages.first() {
        params.set_language(Some(lang.as_str()));
    } else {
        params.set_language(Some("auto"));
    }

    state.full(params, samples)?;

    let n = state.full_n_segments()?;
    let mut out = String::new();
    for i in 0..n {
        out.push_str(&state.full_get_segment_text(i)?);
    }
    Ok(out.trim().to_string())
}
