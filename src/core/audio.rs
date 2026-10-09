//! Turning raw device audio into what Whisper expects: 16 kHz mono f32 with a
//! short silent lead-in and a minimum length.

use anyhow::Result;
use rubato::{FftFixedIn, Resampler};

pub const SAMPLE_RATE: usize = 16_000;
/// Recordings and imports are bounded to 10 minutes, as on macOS.
pub const MAX_SAMPLES: usize = SAMPLE_RATE * 10 * 60;

/// Shorter than 0.3 s is an accidental tap or warm-up noise.
const MIN_SAMPLES: usize = 4_800;
/// 0.25 s of silence before speech stops Whisper clipping the first word
/// ("Ahoj" decoding as "Hoj").
const LEAD_IN: usize = 4_000;
/// whisper.cpp refuses input under one second; pad to 1.2 s.
const MIN_TOTAL: usize = 19_200;
/// Nothing louder than this means the microphone heard silence.
const PEAK_GATE: f32 = 0.01;

/// Prepares a 16 kHz mono recording for Whisper, or returns `None` when it is
/// too short or silent to be worth transcribing. Mirrors
/// `AudioPreparation.padded` in the macOS app.
pub fn padded(samples: &[f32]) -> Option<Vec<f32>> {
    if samples.len() <= MIN_SAMPLES || !samples.iter().any(|s| s.abs() > PEAK_GATE) {
        return None;
    }
    let mut out = Vec::with_capacity((samples.len() + LEAD_IN).max(MIN_TOTAL));
    out.resize(LEAD_IN, 0.0);
    out.extend_from_slice(samples);
    if out.len() < MIN_TOTAL {
        out.resize(MIN_TOTAL, 0.0);
    }
    Some(out)
}

/// Averages interleaved channels into one.
pub fn downmix(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Converts mono audio between sample rates.
///
/// Feeds the resampler in fixed chunks, flushes it with silence and drops its
/// output delay, so the result covers exactly the input - no leading silence,
/// no missing tail. (Hlas for Windows 0.1.0 fed the whole recording as one
/// chunk and lost the second half of every dictation on 48 kHz microphones.)
pub fn resample(input: &[f32], from: usize, to: usize) -> Result<Vec<f32>> {
    if input.is_empty() || from == to {
        return Ok(input.to_vec());
    }
    const CHUNK: usize = 1024;
    let mut resampler = FftFixedIn::<f32>::new(from, to, CHUNK, 2, 1)?;
    let delay = resampler.output_delay();
    let expected = (input.len() as u128 * to as u128 / from as u128) as usize;
    let mut out = Vec::with_capacity(expected + delay + CHUNK);

    let mut pos = 0;
    while input.len() - pos >= resampler.input_frames_next() {
        let n = resampler.input_frames_next();
        let block = resampler.process(&[&input[pos..pos + n]], None)?;
        out.extend_from_slice(&block[0]);
        pos += n;
    }
    if pos < input.len() {
        let block = resampler.process_partial(Some(&[&input[pos..]]), None)?;
        out.extend_from_slice(&block[0]);
    }
    while out.len() < delay + expected {
        let block = resampler.process_partial::<&[f32]>(None, None)?;
        if block[0].is_empty() {
            break;
        }
        out.extend_from_slice(&block[0]);
    }
    let start = delay.min(out.len());
    let mut result = out.split_off(start);
    result.truncate(expected);
    Ok(result)
}

/// Device audio (any rate, any channel count) to Whisper's 16 kHz mono.
pub fn to_whisper(interleaved: &[f32], channels: usize, rate: usize) -> Result<Vec<f32>> {
    let mono = downmix(interleaved, channels.max(1));
    resample(&mono, rate, SAMPLE_RATE)
}

/// Loudness of a block for the level meter, 0.0 to 1.0.
pub fn level(block: &[f32]) -> f32 {
    if block.is_empty() {
        return 0.0;
    }
    let rms = (block.iter().map(|s| s * s).sum::<f32>() / block.len() as f32).sqrt();
    // Speech RMS sits around 0.02 to 0.2; stretch it so the meter moves.
    (rms * 6.0).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tone whose amplitude encodes the second it belongs to.
    fn staircase(rate: usize, seconds: usize) -> Vec<f32> {
        (0..rate * seconds)
            .map(|i| {
                let t = i as f32 / rate as f32;
                (t.floor() + 1.0) * 0.08 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            })
            .collect()
    }

    fn peak(s: &[f32]) -> f32 {
        s.iter().fold(0.0, |m, v| m.max(v.abs()))
    }

    #[test]
    fn resample_keeps_every_second_of_a_48k_recording() {
        let input = staircase(48_000, 10);
        let out = resample(&input, 48_000, 16_000).unwrap();
        assert_eq!(out.len(), 160_000);
        for sec in 0..10 {
            // Skip the edges of each second where the amplitude steps.
            let s = &out[sec * 16_000 + 2_000..(sec + 1) * 16_000 - 2_000];
            let expected = (sec as f32 + 1.0) * 0.08;
            let got = peak(s);
            assert!(
                (got - expected).abs() < 0.02,
                "second {sec}: peak {got}, expected {expected}"
            );
        }
    }

    #[test]
    fn resample_handles_44_1k_and_odd_lengths() {
        let input = staircase(44_100, 3);
        let input = &input[..input.len() - 777];
        let out = resample(input, 44_100, 16_000).unwrap();
        let expected = input.len() * 16_000 / 44_100;
        assert_eq!(out.len(), expected);
        assert!(peak(&out[out.len() - 4_000..]) > 0.2, "tail must survive");
    }

    #[test]
    fn resample_short_and_empty_inputs() {
        assert!(resample(&[], 48_000, 16_000).unwrap().is_empty());
        let short = vec![0.5f32; 300];
        assert_eq!(resample(&short, 48_000, 16_000).unwrap().len(), 100);
        let same = vec![0.1f32; 50];
        assert_eq!(resample(&same, 16_000, 16_000).unwrap(), same);
    }

    #[test]
    fn downmix_averages_channels() {
        assert_eq!(downmix(&[1.0, 0.0, 0.5, 0.5], 2), vec![0.5, 0.5]);
        assert_eq!(downmix(&[0.3, 0.1], 1), vec![0.3, 0.1]);
    }

    #[test]
    fn padding_matches_the_mac_rules() {
        assert!(padded(&vec![0.5; 4_800]).is_none(), "0.3 s is too short");
        assert!(padded(&vec![0.005; 20_000]).is_none(), "silence is skipped");
        let short = padded(&vec![0.5; 5_000]).unwrap();
        assert_eq!(short.len(), 19_200);
        assert!(short[..4_000].iter().all(|s| *s == 0.0));
        assert_eq!(short[4_000], 0.5);
        let long = padded(&vec![0.5; 40_000]).unwrap();
        assert_eq!(long.len(), 44_000);
    }

    #[test]
    fn level_is_bounded() {
        assert_eq!(level(&[]), 0.0);
        assert!(level(&[0.0; 10]) == 0.0);
        assert!(level(&[1.0; 10]) <= 1.0);
        assert!(level(&[0.05; 10]) > 0.2);
    }
}
