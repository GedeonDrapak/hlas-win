//! Local Whisper (whisper.cpp via whisper-rs).
//!
//! The model loads on first use (or is prefetched while the user is still
//! speaking) and is freed `keep_alive` seconds after the last use, so idle RAM
//! stays tiny - the same lifecycle as macOS.

use super::model;
use crate::core::errors::LocalError;
use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

struct Holder {
    ctx: Option<WhisperContext>,
    generation: u64,
}

static HOLDER: Lazy<Mutex<Holder>> = Lazy::new(|| {
    Mutex::new(Holder {
        ctx: None,
        generation: 0,
    })
});

const GPU_BUILD: bool = cfg!(feature = "vulkan");

unsafe extern "C" fn abort_requested(user_data: *mut std::ffi::c_void) -> bool {
    !user_data.is_null() && (*(user_data as *const AtomicBool)).load(Ordering::Relaxed)
}

/// The build targets the portable AVX2 baseline (see .cargo/config.toml).
/// Older CPUs would crash with an illegal instruction, so they are told to use
/// a cloud engine instead.
pub fn cpu_supported() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::arch::is_x86_feature_detected!("avx2")
            && std::arch::is_x86_feature_detected!("fma")
            && std::arch::is_x86_feature_detected!("f16c")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

/// whisper.cpp's compiled feature list, for the log.
pub fn system_info() -> &'static str {
    whisper_rs::print_system_info()
}

fn threads() -> i32 {
    // QA/benchmark override: HLAS_THREADS=N.
    if let Some(n) = std::env::var("HLAS_THREADS")
        .ok()
        .and_then(|v| v.parse::<i32>().ok())
    {
        return n.clamp(1, 64);
    }
    // whisper.cpp scales with physical cores; logical/2 approximates them.
    let logical = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    (logical / 2 + logical % 2).clamp(2, 8) as i32
}

/// QA/benchmark override: HLAS_AUDIO_CTX=auto|N. The encoder always runs a
/// full 30 s window (1500 frames, 50 per second) however short the clip;
/// "auto" shrinks it to the clip length plus a margin, N sets it directly.
fn audio_ctx(n_samples: usize) -> Option<i32> {
    let value = std::env::var("HLAS_AUDIO_CTX").ok()?;
    let frames = if value.eq_ignore_ascii_case("auto") {
        (n_samples as f32 / 16_000.0 * 50.0).ceil() as i32 + 64
    } else {
        value.parse::<i32>().ok()?
    };
    Some(frames.clamp(64, 1500))
}

/// Prints whisper.cpp's load/encode/decode timings to stderr (CLI benchmark).
pub fn print_timings() {
    if let Some(ctx) = HOLDER.lock().unwrap().ctx.as_ref() {
        ctx.print_timings();
    }
}

pub fn thread_count() -> i32 {
    threads()
}

fn load(holder: &mut Holder) -> Result<()> {
    if holder.ctx.is_some() {
        return Ok(());
    }
    if !cpu_supported() {
        return Err(LocalError::CpuUnsupported.into());
    }
    if !model::present() {
        return Err(LocalError::ModelMissing.into());
    }
    let path = model::path()?;
    let began = Instant::now();
    log::info!("whisper.cpp: {}", system_info().trim());
    let mut params = WhisperContextParameters::default();
    // Flash attention pays off on a GPU (macOS uses it with Metal) but halves
    // CPU speed, and far worse in MSVC builds; enable it only for Vulkan.
    params.use_gpu(GPU_BUILD).flash_attn(GPU_BUILD);
    let ctx = WhisperContext::new_with_params(
        path.to_str()
            .ok_or_else(|| anyhow!("non-UTF-8 model path"))?,
        params,
    )
    .map_err(|e| {
        log::error!("model load failed: {e}");
        LocalError::ModelLoad
    })?;
    log::info!("local model loaded in {} ms", began.elapsed().as_millis());
    holder.ctx = Some(ctx);
    Ok(())
}

fn schedule_unload(holder: &mut Holder, keep_alive: u64) {
    holder.generation += 1;
    let generation = holder.generation;
    if keep_alive == 0 {
        holder.ctx = None;
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(keep_alive));
        let mut h = HOLDER.lock().unwrap();
        if h.generation == generation && h.ctx.is_some() {
            h.ctx = None;
            log::info!("local model unloaded after idle");
        }
    });
}

/// Loads the model in the background while the user is still speaking.
pub fn prepare(keep_alive: u64) {
    std::thread::spawn(move || {
        let mut h = HOLDER.lock().unwrap();
        h.generation += 1;
        if let Err(e) = load(&mut h) {
            log::warn!("prefetch skipped: {e}");
        }
        // Keep it at least long enough for the dictation to finish.
        schedule_unload(&mut h, keep_alive.max(600));
    });
}

pub fn unload_now() {
    let mut h = HOLDER.lock().unwrap();
    h.generation += 1;
    h.ctx = None;
}

pub fn transcribe(
    samples: &[f32],
    language: Option<&str>,
    prompt: Option<&str>,
    cancel: &Arc<AtomicBool>,
    keep_alive: u64,
) -> Result<String> {
    let mut holder = HOLDER.lock().unwrap();
    holder.generation += 1;
    let result = run(&mut holder, samples, language, prompt, cancel);
    schedule_unload(&mut holder, keep_alive);
    result
}

fn run(
    holder: &mut Holder,
    samples: &[f32],
    language: Option<&str>,
    prompt: Option<&str>,
    cancel: &Arc<AtomicBool>,
) -> Result<String> {
    let cold = holder.ctx.is_none();
    load(holder)?;
    let ctx = holder.ctx.as_ref().expect("loaded");
    let mut state = ctx.create_state()?;

    // Beam search decodes Czech measurably better than greedy (macOS benchmark).
    // QA/benchmark override: HLAS_GREEDY=1 decodes greedily.
    let strategy = if std::env::var_os("HLAS_GREEDY").is_some() {
        SamplingStrategy::Greedy { best_of: 1 }
    } else {
        SamplingStrategy::BeamSearch {
            beam_size: 5,
            patience: -1.0,
        }
    };
    let mut params = FullParams::new(strategy);
    params.set_n_threads(threads());
    if let Some(n) = audio_ctx(samples.len()) {
        params.set_audio_ctx(n);
    }
    params.set_translate(false);
    params.set_no_timestamps(true);
    params.set_print_progress(false);
    params.set_print_special(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_suppress_blank(true);
    params.set_temperature(0.0);
    params.set_language(Some(language.unwrap_or("auto")));
    if let Some(prompt) = prompt {
        params.set_initial_prompt(prompt);
    }
    // whisper-rs 0.16's set_abort_callback_safe stores the closure as one type
    // and reads it back as another, so whisper.cpp polls garbage memory and
    // aborts the encoder at random ("failed to encode", -6). Use the raw hook
    // with the cancel flag itself as user data; it outlives `state.full`.
    unsafe {
        params.set_abort_callback(Some(abort_requested));
        params.set_abort_callback_user_data(Arc::as_ptr(cancel) as *mut std::ffi::c_void);
    }

    let began = Instant::now();
    let status = state.full(params, samples);
    if cancel.load(Ordering::Relaxed) {
        return Err(LocalError::Cancelled.into());
    }
    status?;
    let mut out = String::new();
    for segment in state.as_iter() {
        out.push_str(&segment.to_str_lossy()?);
    }
    log::info!(
        "local inference: cold={cold} audio_s={:.1} elapsed_ms={}",
        samples.len() as f32 / 16_000.0,
        began.elapsed().as_millis()
    );
    Ok(out.trim().to_string())
}
