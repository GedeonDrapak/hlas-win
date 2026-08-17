//! Microphone capture via cpal (WASAPI on Windows), downmixed to mono and
//! resampled to the 16 kHz f32 that Whisper expects.
//!
//! `Recorder::start` opens the default input stream and accumulates samples on
//! a background callback. `stop` tears the stream down and hands back the
//! finished 16 kHz mono buffer.

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Stream, StreamConfig};
use rubato::{FftFixedIn, Resampler};
use std::sync::{Arc, Mutex};

const TARGET_HZ: usize = 16_000;

pub struct Recorder {
    stream: Stream,
    buffer: Arc<Mutex<Vec<f32>>>,
    src_hz: u32,
    channels: u16,
}

impl Recorder {
    pub fn start() -> Result<Recorder> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow!("no input device"))?;
        let default = device.default_input_config()?;
        let src_hz = default.sample_rate().0;
        let channels = default.channels();
        let config: StreamConfig = default.clone().into();

        let buffer = Arc::new(Mutex::new(Vec::<f32>::with_capacity(src_hz as usize * 8)));
        let sink = buffer.clone();

        let err_fn = |e| log::error!("audio stream error: {e}");

        // Whisper wants f32; convert from whatever sample format the device uses.
        let stream = match default.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &config,
                move |data: &[f32], _| {
                    if let Ok(mut b) = sink.lock() {
                        b.extend_from_slice(data);
                    }
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::I16 => device.build_input_stream(
                &config,
                move |data: &[i16], _| {
                    if let Ok(mut b) = sink.lock() {
                        b.extend(data.iter().map(|s| *s as f32 / i16::MAX as f32));
                    }
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::U16 => device.build_input_stream(
                &config,
                move |data: &[u16], _| {
                    if let Ok(mut b) = sink.lock() {
                        b.extend(
                            data.iter()
                                .map(|s| (*s as f32 / u16::MAX as f32) * 2.0 - 1.0),
                        );
                    }
                },
                err_fn,
                None,
            )?,
            other => return Err(anyhow!("unsupported sample format {other:?}")),
        };

        stream.play()?;
        Ok(Recorder {
            stream,
            buffer,
            src_hz,
            channels,
        })
    }

    /// Stops capture and returns the recording as 16 kHz mono f32.
    pub fn stop(self) -> Result<Vec<f32>> {
        drop(self.stream); // stop the callback before reading the buffer
        let interleaved = self
            .buffer
            .lock()
            .map_err(|_| anyhow!("audio buffer poisoned"))?
            .clone();

        let mono = downmix(&interleaved, self.channels);
        if self.src_hz as usize == TARGET_HZ {
            return Ok(mono);
        }
        resample(&mono, self.src_hz as usize, TARGET_HZ)
    }
}

fn downmix(interleaved: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    let ch = channels as usize;
    interleaved
        .chunks(ch)
        .map(|frame| frame.iter().sum::<f32>() / ch as f32)
        .collect()
}

fn resample(input: &[f32], from: usize, to: usize) -> Result<Vec<f32>> {
    if input.is_empty() {
        return Ok(Vec::new());
    }
    let mut resampler = FftFixedIn::<f32>::new(from, to, input.len(), 1, 1)?;
    let out = resampler.process(&[input.to_vec()], None)?;
    Ok(out.into_iter().next().unwrap_or_default())
}
