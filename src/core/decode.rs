//! Audio file import: decodes wav, mp3, m4a/aac, flac and ogg to Whisper's
//! 16 kHz mono. Files over 10 minutes are rejected, never silently cut.

use super::audio::{self, MAX_SAMPLES};
use super::errors::LocalError;
use anyhow::Result;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

pub fn decode_file(path: &Path, cancel: &AtomicBool) -> Result<Vec<f32>> {
    let file = std::fs::File::open(path)?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|_| LocalError::UnsupportedAudio)?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or(LocalError::UnsupportedAudio)?;
    let track_id = track.id;
    let mut rate = track.codec_params.sample_rate.unwrap_or(0) as usize;
    let mut channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(1);
    if let (Some(frames), true) = (track.codec_params.n_frames, rate > 0) {
        if frames as usize / rate > 600 {
            return Err(LocalError::TooLong.into());
        }
    }
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|_| LocalError::UnsupportedAudio)?;

    let mut interleaved: Vec<f32> = Vec::new();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(LocalError::Cancelled.into());
        }
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            Err(_) => return Err(LocalError::UnsupportedAudio.into()),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                rate = spec.rate as usize;
                channels = spec.channels.count();
                let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                buf.copy_interleaved_ref(decoded);
                interleaved.extend_from_slice(buf.samples());
                if rate > 0 && interleaved.len() / channels.max(1) > rate * 600 {
                    return Err(LocalError::TooLong.into());
                }
            }
            // A corrupt packet is skipped, like every media player does.
            Err(SymError::DecodeError(_)) => continue,
            Err(_) => return Err(LocalError::UnsupportedAudio.into()),
        }
    }
    if rate == 0 {
        return Err(LocalError::UnsupportedAudio.into());
    }
    let samples = audio::to_whisper(&interleaved, channels, rate)?;
    if samples.len() > MAX_SAMPLES {
        return Err(LocalError::TooLong.into());
    }
    Ok(samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes a 16-bit stereo WAV at `rate` with a tone in both channels.
    fn stereo_wav(path: &Path, rate: u32, seconds: f32) {
        let frames = (rate as f32 * seconds) as usize;
        let mut data = Vec::with_capacity(frames * 4);
        for i in 0..frames {
            let v = (0.4
                * (2.0 * std::f32::consts::PI * 300.0 * i as f32 / rate as f32).sin()
                * 32767.0) as i16;
            data.extend_from_slice(&v.to_le_bytes());
            data.extend_from_slice(&v.to_le_bytes());
        }
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&rate.to_le_bytes());
        wav.extend_from_slice(&(rate * 4).to_le_bytes());
        wav.extend_from_slice(&4u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
        wav.extend_from_slice(&data);
        std::fs::write(path, wav).unwrap();
    }

    #[test]
    fn decodes_stereo_44k_wav_to_16k_mono() {
        let dir = std::env::temp_dir().join(format!("hlas-decode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tone.wav");
        stereo_wav(&path, 44_100, 2.0);
        let samples = decode_file(&path, &AtomicBool::new(false)).unwrap();
        assert_eq!(samples.len(), 32_000);
        let peak = samples[30_000..].iter().fold(0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.3, "tail present, peak {peak}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_garbage_and_honours_cancel() {
        let dir = std::env::temp_dir().join(format!("hlas-decode-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bad = dir.join("bad.mp3");
        std::fs::write(&bad, b"definitely not audio").unwrap();
        assert!(decode_file(&bad, &AtomicBool::new(false)).is_err());
        let good = dir.join("tone.wav");
        stereo_wav(&good, 16_000, 1.0);
        let err = decode_file(&good, &AtomicBool::new(true)).unwrap_err();
        assert_eq!(
            err.downcast_ref::<LocalError>(),
            Some(&LocalError::Cancelled)
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn decodes_the_czech_fixture() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/czech-48k.wav");
        let samples = decode_file(&path, &AtomicBool::new(false)).unwrap();
        let seconds = samples.len() as f32 / 16_000.0;
        assert!((seconds - 6.13).abs() < 0.05, "{seconds} s");
        // Speech is present near the end, not only at the start.
        let tail = &samples[samples.len() - 32_000..];
        assert!(tail.iter().any(|s| s.abs() > 0.05));
    }
}
