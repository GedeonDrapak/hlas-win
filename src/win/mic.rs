//! Microphone capture via cpal (WASAPI).
//!
//! The stream stays open ("warm") for a few seconds after a dictation, so the
//! next one starts instantly instead of losing its first word to device
//! start-up. Samples are only kept while a dictation is running. The stream is
//! rebuilt when the chosen device changes, the Windows default changes, or the
//! device reports an error (unplugged, Bluetooth route switch).
//!
//! `HLAS_FAKE_AUDIO=<file>` replaces the microphone with an audio file, so the
//! whole pipeline can be tested on machines without one (CI).

use crate::core::audio::{self, MAX_SAMPLES};
use crate::core::errors::LocalError;
use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample, Stream, StreamConfig};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long the stream stays open after the last dictation.
pub const KEEP_WARM: Duration = Duration::from_secs(10);

struct Shared {
    capturing: AtomicBool,
    buffer: Mutex<Vec<f32>>,
    frames: AtomicUsize,
    limit_frames: AtomicUsize,
    level: AtomicU32,
    broken: AtomicBool,
}

struct Active {
    _stream: Stream,
    device_name: String,
    follows_default: bool,
    channels: usize,
    rate: usize,
}

pub struct Mic {
    shared: Arc<Shared>,
    active: Option<Active>,
    last_used: Instant,
    fake: Option<PathBuf>,
    fake_started: Option<Instant>,
}

impl Default for Mic {
    fn default() -> Self {
        Self::new()
    }
}

impl Mic {
    pub fn new() -> Mic {
        Mic {
            shared: Arc::new(Shared {
                capturing: AtomicBool::new(false),
                buffer: Mutex::new(Vec::new()),
                frames: AtomicUsize::new(0),
                limit_frames: AtomicUsize::new(usize::MAX),
                level: AtomicU32::new(0),
                broken: AtomicBool::new(false),
            }),
            active: None,
            last_used: Instant::now(),
            fake: std::env::var_os("HLAS_FAKE_AUDIO").map(PathBuf::from),
            fake_started: None,
        }
    }

    /// Starts collecting samples. `device` is a device name, or empty for the
    /// Windows default microphone. Returns the device name in use.
    pub fn begin(&mut self, device: &str) -> Result<String> {
        if self.fake.is_some() {
            self.fake_started = Some(Instant::now());
            return Ok("Test audio file".into());
        }
        self.ensure_stream(device)?;
        let active = self.active.as_ref().expect("stream ready");
        {
            let mut buf = self.shared.buffer.lock().unwrap();
            buf.clear();
            buf.reserve(active.rate * active.channels * 15);
        }
        self.shared.frames.store(0, Ordering::SeqCst);
        self.shared
            .limit_frames
            .store(active.rate * 600, Ordering::SeqCst);
        self.shared.capturing.store(true, Ordering::SeqCst);
        self.last_used = Instant::now();
        Ok(active.device_name.clone())
    }

    /// Stops collecting and returns the recording as 16 kHz mono.
    pub fn end(&mut self) -> Result<Vec<f32>> {
        self.last_used = Instant::now();
        if let Some(path) = &self.fake {
            self.fake_started = None;
            let samples = crate::core::decode::decode_file(path, &AtomicBool::new(false))?;
            return Ok(samples);
        }
        self.shared.capturing.store(false, Ordering::SeqCst);
        self.shared.level.store(0, Ordering::Relaxed);
        let (channels, rate) = match &self.active {
            Some(a) => (a.channels, a.rate),
            None => return Ok(Vec::new()),
        };
        let interleaved = std::mem::take(&mut *self.shared.buffer.lock().unwrap());
        let mut samples = audio::to_whisper(&interleaved, channels, rate)?;
        samples.truncate(MAX_SAMPLES);
        Ok(samples)
    }

    /// Drops the current recording.
    pub fn cancel(&mut self) {
        self.fake_started = None;
        self.shared.capturing.store(false, Ordering::SeqCst);
        self.shared.level.store(0, Ordering::Relaxed);
        self.shared.buffer.lock().unwrap().clear();
        self.last_used = Instant::now();
    }

    /// Closes the warm stream once it has been idle long enough.
    pub fn tick(&mut self) {
        let capturing = self.shared.capturing.load(Ordering::SeqCst);
        if !capturing && self.active.is_some() && self.last_used.elapsed() > KEEP_WARM {
            self.active = None;
            log::info!("microphone closed after idle");
        }
    }

    /// Current input level for the overlay meter, 0.0 to 1.0.
    pub fn level(&self) -> f32 {
        if let Some(started) = self.fake_started {
            // Gentle synthetic movement so the overlay is testable.
            return 0.3 + 0.2 * (started.elapsed().as_secs_f32() * 6.0).sin().abs();
        }
        f32::from_bits(self.shared.level.load(Ordering::Relaxed))
    }

    /// True once the 10-minute limit is reached; the coordinator then stops.
    pub fn at_limit(&self) -> bool {
        let limit = self.shared.limit_frames.load(Ordering::SeqCst);
        self.shared.capturing.load(Ordering::SeqCst)
            && self.shared.frames.load(Ordering::SeqCst) >= limit
    }

    fn ensure_stream(&mut self, wanted: &str) -> Result<()> {
        let host = cpal::default_host();
        if let Some(active) = &self.active {
            let broken = self.shared.broken.load(Ordering::SeqCst);
            let same_choice = if wanted.is_empty() {
                active.follows_default
                    && host
                        .default_input_device()
                        .and_then(|d| d.name().ok())
                        .is_some_and(|n| n == active.device_name)
            } else {
                !active.follows_default && active.device_name == wanted
            };
            if !broken && same_choice {
                return Ok(());
            }
            log::info!("rebuilding microphone stream (broken={broken})");
            self.active = None;
        }
        self.shared.broken.store(false, Ordering::SeqCst);

        let (device, follows_default) = pick_device(&host, wanted)?;
        let name = device.name().unwrap_or_else(|_| "Microphone".into());
        let supported = device.default_input_config().map_err(|e| {
            log::error!("input config failed: {e}");
            blocked_or(LocalError::NoMicrophone)
        })?;
        let channels = supported.channels() as usize;
        let rate = supported.sample_rate().0 as usize;
        let config: StreamConfig = supported.config();
        let shared = self.shared.clone();

        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => build::<f32>(&device, &config, shared, channels),
            cpal::SampleFormat::I16 => build::<i16>(&device, &config, shared, channels),
            cpal::SampleFormat::U16 => build::<u16>(&device, &config, shared, channels),
            cpal::SampleFormat::I32 => build::<i32>(&device, &config, shared, channels),
            cpal::SampleFormat::U8 => build::<u8>(&device, &config, shared, channels),
            cpal::SampleFormat::I8 => build::<i8>(&device, &config, shared, channels),
            cpal::SampleFormat::F64 => build::<f64>(&device, &config, shared, channels),
            other => return Err(anyhow!("unsupported sample format {other:?}")),
        }
        .map_err(|e| {
            log::error!("input stream failed: {e}");
            blocked_or(LocalError::NoMicrophone)
        })?;
        stream.play().map_err(|e| {
            log::error!("input stream start failed: {e}");
            blocked_or(LocalError::NoMicrophone)
        })?;
        log::info!("microphone open: {rate} Hz, {channels} ch, default={follows_default}");
        self.active = Some(Active {
            _stream: stream,
            device_name: name,
            follows_default,
            channels,
            rate,
        });
        Ok(())
    }
}

/// A failure caused by the privacy switch gets the actionable message.
fn blocked_or(fallback: LocalError) -> anyhow::Error {
    if super::privacy::mic_access().is_allowed() {
        fallback.into()
    } else {
        LocalError::MicrophoneBlocked.into()
    }
}

fn pick_device(host: &cpal::Host, wanted: &str) -> Result<(cpal::Device, bool)> {
    if !wanted.is_empty() {
        if let Ok(devices) = host.input_devices() {
            for d in devices {
                if d.name().is_ok_and(|n| n == wanted) {
                    return Ok((d, false));
                }
            }
        }
        log::warn!("chosen microphone not found; using the Windows default");
    }
    host.default_input_device()
        .map(|d| (d, true))
        .ok_or_else(|| blocked_or(LocalError::NoMicrophone))
}

/// Names of all input devices, for Settings and onboarding.
pub fn device_names() -> Vec<String> {
    cpal::default_host()
        .input_devices()
        .map(|it| it.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

fn build<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    shared: Arc<Shared>,
    channels: usize,
) -> Result<Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let err_shared = shared.clone();
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            if !shared.capturing.load(Ordering::Relaxed) {
                return;
            }
            let limit = shared.limit_frames.load(Ordering::Relaxed);
            let frames = shared.frames.load(Ordering::Relaxed);
            if frames >= limit {
                return;
            }
            let mut level = 0.0;
            if let Ok(mut buf) = shared.buffer.lock() {
                let start = buf.len();
                buf.extend(data.iter().map(|&s| f32::from_sample(s)));
                level = audio::level(&buf[start..]);
            }
            shared
                .frames
                .fetch_add(data.len() / channels.max(1), Ordering::Relaxed);
            shared.level.store(level.to_bits(), Ordering::Relaxed);
        },
        move |e| {
            log::error!("audio stream error: {e}");
            err_shared.broken.store(true, Ordering::SeqCst);
        },
        None,
    )?;
    Ok(stream)
}
