//! Downloading the local Whisper model (same file and SHA-256 as macOS).
//!
//! One download at a time, shared by onboarding, Settings and a first
//! dictation. The file lands in a staging name, is verified, then replaces the
//! model atomically; an invalid or cancelled download never touches a working
//! model.

use crate::core::config::Config;
use crate::core::errors::LocalError;
use crate::core::model as files;
use crate::core::shared::SHARED;
use anyhow::Result;
use once_cell::sync::Lazy;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct Status {
    pub active: bool,
    /// 0 to 100.
    pub percent: u8,
    pub verifying: bool,
    pub error: Option<String>,
}

static STATUS: Lazy<Mutex<Status>> = Lazy::new(|| Mutex::new(Status::default()));
static CANCEL: AtomicBool = AtomicBool::new(false);

pub fn path() -> Result<PathBuf> {
    Ok(Config::models_dir()?.join(&SHARED.model.file))
}

pub fn present() -> bool {
    path()
        .map(|p| files::is_present(&p, SHARED.model.bytes))
        .unwrap_or(false)
}

pub fn status() -> Status {
    STATUS.lock().unwrap().clone()
}

/// Moves a model downloaded by 0.1.0 out of the roaming profile.
pub fn migrate_legacy() {
    let (Some(old), Ok(new)) = (Config::legacy_dir(), Config::models_dir()) else {
        return;
    };
    match files::migrate_legacy(
        &old.join("models"),
        &new,
        &SHARED.model.file,
        SHARED.model.bytes,
    ) {
        Ok(true) => log::info!("moved the local model out of the roaming profile"),
        Ok(false) => {}
        Err(e) => log::warn!("model migration failed: {e}"),
    }
}

/// Starts a background download unless one is running or the model is there.
pub fn start() {
    {
        let mut s = STATUS.lock().unwrap();
        if s.active || present() {
            return;
        }
        *s = Status {
            active: true,
            ..Default::default()
        };
    }
    CANCEL.store(false, Ordering::SeqCst);
    std::thread::spawn(|| {
        let result = download();
        let mut s = STATUS.lock().unwrap();
        s.active = false;
        s.verifying = false;
        match result {
            Ok(()) => {
                s.percent = 100;
                s.error = None;
                log::info!("local model installed");
            }
            Err(e) => {
                let cancelled =
                    matches!(e.downcast_ref::<LocalError>(), Some(LocalError::Cancelled));
                s.error = (!cancelled).then(|| crate::core::errors::user_message(&e));
                log::warn!("model download ended: {e}");
            }
        }
    });
}

pub fn cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}

/// Blocks until the model is present, downloading it if needed. `progress`
/// receives the percentage. Used when a dictation needs the local engine.
pub fn ensure(progress: impl Fn(u8), cancel: &AtomicBool) -> Result<()> {
    if present() {
        return Ok(());
    }
    start();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(LocalError::Cancelled.into());
        }
        let s = status();
        if !s.active {
            if present() {
                return Ok(());
            }
            return Err(anyhow::anyhow!(s
                .error
                .unwrap_or_else(|| LocalError::ModelDownload.to_string())));
        }
        progress(s.percent);
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn download() -> Result<()> {
    let dest = path()?;
    let dir = Config::models_dir()?;
    std::fs::create_dir_all(&dir)?;
    let staging = dir.join(format!("{}.download", SHARED.model.file));
    let _ = std::fs::remove_file(&staging);

    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .timeout(None)
        .build()?;
    let mut resp = client
        .get(&SHARED.model.url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| {
            log::warn!("model request failed: {e}");
            LocalError::ModelDownload
        })?;
    let total = resp.content_length().unwrap_or(SHARED.model.bytes).max(1);
    let mut file = std::fs::File::create(&staging)?;
    let mut done = 0u64;
    let mut buf = vec![0u8; 1 << 16];
    loop {
        if CANCEL.load(Ordering::Relaxed) {
            drop(file);
            let _ = std::fs::remove_file(&staging);
            return Err(LocalError::Cancelled.into());
        }
        let n = resp.read(&mut buf).map_err(|_| LocalError::ModelDownload)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        done += n as u64;
        STATUS.lock().unwrap().percent = ((done * 100) / total).min(99) as u8;
    }
    file.flush()?;
    drop(file);

    STATUS.lock().unwrap().verifying = true;
    if let Err(e) = files::verify(&staging, SHARED.model.bytes, &SHARED.model.sha256, &CANCEL) {
        let _ = std::fs::remove_file(&staging);
        return Err(e.into());
    }
    // Replace atomically; a model being read by Whisper is unloaded first.
    super::local::unload_now();
    let _ = std::fs::remove_file(&dest);
    std::fs::rename(&staging, &dest)?;
    Ok(())
}
