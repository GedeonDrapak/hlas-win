//! Local Whisper model files: presence, SHA-256 verification and moving the
//! model out of the 0.1.0 roaming folder.

use super::errors::LocalError;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Cheap presence check used at startup: the exact byte count. The SHA-256 is
/// verified once, right after download, exactly like macOS.
pub fn is_present(path: &Path, bytes: u64) -> bool {
    std::fs::metadata(path)
        .map(|m| m.len() == bytes)
        .unwrap_or(false)
}

pub fn sha256_hex(path: &Path, cancel: &AtomicBool) -> Result<String, LocalError> {
    let mut file = std::fs::File::open(path).map_err(|_| LocalError::ModelInvalid)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(LocalError::Cancelled);
        }
        let n = file.read(&mut buf).map_err(|_| LocalError::ModelInvalid)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// Verifies size and checksum of a downloaded file before it is installed.
pub fn verify(
    path: &Path,
    bytes: u64,
    sha256: &str,
    cancel: &AtomicBool,
) -> Result<(), LocalError> {
    if !is_present(path, bytes) {
        return Err(LocalError::ModelInvalid);
    }
    let digest = sha256_hex(path, cancel)?;
    if digest.eq_ignore_ascii_case(sha256) {
        Ok(())
    } else {
        Err(LocalError::ModelInvalid)
    }
}

/// Moves a model left by 0.1.0 into the new folder. Returns true when moved.
pub fn migrate_legacy(
    old_dir: &Path,
    new_dir: &Path,
    file: &str,
    bytes: u64,
) -> std::io::Result<bool> {
    let (old, new) = (old_dir.join(file), new_dir.join(file));
    if is_present(&new, bytes) || !is_present(&old, bytes) {
        return Ok(false);
    }
    std::fs::create_dir_all(new_dir)?;
    if std::fs::rename(&old, &new).is_err() {
        // Different volume: copy, then remove the original.
        std::fs::copy(&old, &new)?;
        std::fs::remove_file(&old)?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_size_and_checksum() {
        let dir = std::env::temp_dir().join(format!("hlas-model-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("m.bin");
        std::fs::write(&file, b"abc").unwrap();
        let no = AtomicBool::new(false);
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(sha256_hex(&file, &no).unwrap(), abc);
        assert!(verify(&file, 3, abc, &no).is_ok());
        assert_eq!(verify(&file, 4, abc, &no), Err(LocalError::ModelInvalid));
        assert_eq!(
            verify(&file, 3, &"0".repeat(64), &no),
            Err(LocalError::ModelInvalid)
        );
        assert_eq!(
            verify(&file, 3, abc, &AtomicBool::new(true)),
            Err(LocalError::Cancelled)
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn migrates_only_a_complete_legacy_model() {
        let base = std::env::temp_dir().join(format!("hlas-migrate-{}", std::process::id()));
        let (old, new) = (base.join("old"), base.join("new"));
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("m.bin"), b"12345").unwrap();
        assert!(
            !migrate_legacy(&old, &new, "m.bin", 9).unwrap(),
            "wrong size stays put"
        );
        assert!(migrate_legacy(&old, &new, "m.bin", 5).unwrap());
        assert!(is_present(&new.join("m.bin"), 5));
        assert!(!old.join("m.bin").exists());
        assert!(
            !migrate_legacy(&old, &new, "m.bin", 5).unwrap(),
            "idempotent"
        );
        std::fs::remove_dir_all(&base).ok();
    }
}
