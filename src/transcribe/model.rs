//! Local Whisper model management: the same large-v3-turbo-q5_0 weights the
//! macOS build uses, downloaded on demand from Hugging Face into
//! %APPDATA%\Hlas\models\.

use crate::config::Config;
use anyhow::{anyhow, Context, Result};
use std::io::Write;
use std::path::PathBuf;

pub const FILE: &str = "ggml-large-v3-turbo-q5_0.bin";
const URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin";
/// Expected size in bytes, used as a cheap integrity gate before we trust a file.
const EXPECTED_BYTES: u64 = 574_041_195;

pub fn path() -> Result<PathBuf> {
    Ok(Config::models_dir()?.join(FILE))
}

pub fn is_present() -> bool {
    match path() {
        Ok(p) => std::fs::metadata(&p)
            .map(|m| m.len() == EXPECTED_BYTES)
            .unwrap_or(false),
        Err(_) => false,
    }
}

/// Downloads the model if missing. `progress` receives (downloaded, total).
pub fn ensure(progress: impl Fn(u64, u64)) -> Result<PathBuf> {
    let dest = path()?;
    if is_present() {
        return Ok(dest);
    }
    std::fs::create_dir_all(Config::models_dir()?)?;

    let client = reqwest::blocking::Client::builder().timeout(None).build()?;
    let mut resp = client.get(URL).send()?.error_for_status()?;
    let total = resp.content_length().unwrap_or(EXPECTED_BYTES);

    // Download to a temp file, then rename, so a cancelled run never leaves a
    // half file that looks valid.
    let tmp = dest.with_extension("part");
    let mut file = std::fs::File::create(&tmp)?;
    let mut downloaded = 0u64;
    let mut buf = [0u8; 1 << 16];
    loop {
        let n = std::io::Read::read(&mut resp, &mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        downloaded += n as u64;
        progress(downloaded, total);
    }
    file.flush()?;
    drop(file);

    let got = std::fs::metadata(&tmp)?.len();
    if got != EXPECTED_BYTES {
        let _ = std::fs::remove_file(&tmp);
        return Err(anyhow!(
            "model download size mismatch: got {got}, expected {EXPECTED_BYTES}"
        ));
    }
    std::fs::rename(&tmp, &dest).context("failed to finalize model file")?;
    Ok(dest)
}
