//! Win32 clipboard with full-format snapshots.
//!
//! Pasting goes through the clipboard, so Hlas saves everything that was on it
//! (text, rich text, HTML, images, file lists...), puts the transcript there
//! tagged as private, and restores the snapshot afterwards. The private tags
//! keep dictations out of Win+V clipboard history and cloud clipboard sync.

use super::wide;
use anyhow::{anyhow, Result};
use std::time::Duration;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
    GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};

const CF_UNICODETEXT: u32 = 13;

/// Formats whose handles are GDI objects or owner-drawn, not memory blocks.
fn is_memory_format(format: u32) -> bool {
    !matches!(format, 2 | 3 | 9 | 14 | 0x80 | 0x82 | 0x83 | 0x8E)
        && !(0x200..=0x3FF).contains(&format)
}

/// Everything that was on the clipboard, as raw bytes per format.
pub struct Snapshot {
    formats: Vec<(u32, Vec<u8>)>,
}

impl Snapshot {
    pub fn is_empty(&self) -> bool {
        self.formats.is_empty()
    }
}

struct Open;

impl Open {
    /// Another app may hold the clipboard for a moment; retry briefly.
    fn new() -> Result<Open> {
        for _ in 0..20 {
            if unsafe { OpenClipboard(None) }.is_ok() {
                return Ok(Open);
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        Err(anyhow!("clipboard is busy"))
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

pub fn sequence() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

pub fn snapshot() -> Option<Snapshot> {
    let _open = Open::new().ok()?;
    let mut formats = Vec::new();
    let mut format = 0u32;
    loop {
        format = unsafe { EnumClipboardFormats(format) };
        if format == 0 {
            break;
        }
        if !is_memory_format(format) {
            continue;
        }
        if let Some(bytes) = unsafe { read_format(format) } {
            formats.push((format, bytes));
        }
    }
    Some(Snapshot { formats })
}

unsafe fn read_format(format: u32) -> Option<Vec<u8>> {
    let handle = GetClipboardData(format).ok()?;
    let mem = HGLOBAL(handle.0);
    let size = GlobalSize(mem);
    if size == 0 {
        return None;
    }
    let ptr = GlobalLock(mem) as *const u8;
    if ptr.is_null() {
        return None;
    }
    let bytes = std::slice::from_raw_parts(ptr, size).to_vec();
    let _ = GlobalUnlock(mem);
    Some(bytes)
}

unsafe fn write_format(format: u32, bytes: &[u8]) -> Result<()> {
    let mem = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1))?;
    let ptr = GlobalLock(mem) as *mut u8;
    if ptr.is_null() {
        let _ = GlobalFree(mem);
        return Err(anyhow!("GlobalLock failed"));
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
    let _ = GlobalUnlock(mem);
    if SetClipboardData(format, HANDLE(mem.0)).is_err() {
        // Ownership only passes to the system on success.
        let _ = GlobalFree(mem);
        return Err(anyhow!("SetClipboardData({format}) failed"));
    }
    Ok(())
}

fn utf16_bytes(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(|u| u.to_le_bytes())
        .collect()
}

fn register(name: &str) -> u32 {
    let w = wide(name);
    unsafe { RegisterClipboardFormatW(PCWSTR(w.as_ptr())) }
}

/// Puts `text` on the clipboard. With `private`, Windows clipboard history,
/// cloud clipboard and clipboard monitors are told to skip it. Returns the
/// clipboard sequence number right after the write.
pub fn set_text(text: &str, private: bool) -> Result<u32> {
    {
        let _open = Open::new()?;
        unsafe {
            EmptyClipboard()?;
            write_format(CF_UNICODETEXT, &utf16_bytes(text))?;
            if private {
                let zero = 0u32.to_le_bytes();
                let _ = write_format(
                    register("ExcludeClipboardContentFromMonitorProcessing"),
                    &zero,
                );
                let _ = write_format(register("CanIncludeInClipboardHistory"), &zero);
                let _ = write_format(register("CanUploadToCloudClipboard"), &zero);
            }
        }
    }
    Ok(sequence())
}

/// Restores a snapshot taken earlier.
pub fn restore(snapshot: &Snapshot) -> Result<()> {
    let _open = Open::new()?;
    unsafe {
        EmptyClipboard()?;
        for (format, bytes) in &snapshot.formats {
            if let Err(e) = write_format(*format, bytes) {
                log::warn!("clipboard restore skipped a format: {e}");
            }
        }
    }
    Ok(())
}
