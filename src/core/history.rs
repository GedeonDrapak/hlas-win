//! Local dictation history: the last 50 results, optional retention, stored
//! only on this PC. Same shape and rules as the macOS History.

use super::config::OutputMode;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const MAX_ENTRIES: usize = 50;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: u64,
    pub text: String,
    /// The plain transcript when `text` was changed (Smart text, edits).
    #[serde(default)]
    pub raw_text: Option<String>,
    #[serde(default)]
    pub mode: OutputMode,
    #[serde(default)]
    pub engine: Option<String>,
    /// Seconds of audio.
    #[serde(default)]
    pub duration: Option<f64>,
    /// Unix seconds.
    pub date: i64,
}

impl Entry {
    pub fn original(&self) -> &str {
        self.raw_text.as_deref().unwrap_or(&self.text)
    }
}

pub struct History {
    path: PathBuf,
    entries: Vec<Entry>,
}

pub struct NewEntry<'a> {
    pub text: &'a str,
    pub raw: Option<&'a str>,
    pub mode: OutputMode,
    pub engine: Option<&'a str>,
    pub duration: Option<f64>,
}

impl History {
    /// Loads history from `path`. A missing or unreadable file is empty history.
    pub fn load(path: &Path) -> History {
        let entries = std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str::<Vec<Entry>>(t.trim_start_matches('\u{feff}')).ok())
            .unwrap_or_default();
        History {
            path: path.to_path_buf(),
            entries,
        }
    }

    #[cfg(test)]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn add(&mut self, new: NewEntry<'_>, now: i64, retention_days: u32) -> u64 {
        let id = self.entries.iter().map(|e| e.id).max().unwrap_or(0) + 1;
        let raw = new.raw.filter(|r| *r != new.text).map(str::to_string);
        self.entries.insert(
            0,
            Entry {
                id,
                text: new.text.to_string(),
                raw_text: raw,
                mode: new.mode,
                engine: new.engine.map(str::to_string),
                duration: new.duration,
                date: now,
            },
        );
        self.prune(retention_days, now);
        id
    }

    /// Replaces an entry's text, remembering the original transcript.
    pub fn update(&mut self, id: u64, text: &str, mode: OutputMode) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
            if e.raw_text.is_none() {
                e.raw_text = Some(e.text.clone());
            }
            e.text = text.to_string();
            e.mode = mode;
        }
    }

    pub fn prune(&mut self, retention_days: u32, now: i64) {
        if retention_days > 0 {
            let cutoff = now - retention_days as i64 * 86_400;
            self.entries.retain(|e| e.date >= cutoff);
        }
        self.entries
            .sort_by(|a, b| b.date.cmp(&a.date).then(b.id.cmp(&a.id)));
        self.entries.truncate(MAX_ENTRIES);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        let _ = std::fs::remove_file(&self.path);
    }

    /// Case-insensitive search over the text and the original transcript.
    pub fn search(&self, query: &str) -> Vec<&Entry> {
        let q = query.trim().to_lowercase();
        self.entries
            .iter()
            .filter(|e| {
                q.is_empty()
                    || e.text.to_lowercase().contains(&q)
                    || e.original().to_lowercase().contains(&q)
            })
            .collect()
    }

    pub fn get(&self, id: u64) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn save(&self) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string(&self.entries)?)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(text: &str) -> NewEntry<'_> {
        NewEntry {
            text,
            raw: None,
            mode: OutputMode::Transcript,
            engine: Some("local"),
            duration: Some(1.0),
        }
    }

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("hlas-history-{name}-{}.json", std::process::id()))
    }

    #[test]
    fn newest_first_capped_at_50() {
        let mut h = History::load(&temp("cap"));
        for i in 0..60 {
            h.add(entry(&format!("t{i}")), 1_000 + i, 0);
        }
        assert_eq!(h.entries().len(), MAX_ENTRIES);
        assert_eq!(h.entries()[0].text, "t59");
    }

    #[test]
    fn retention_drops_old_entries() {
        let mut h = History::load(&temp("ret"));
        h.add(entry("old"), 0, 0);
        h.add(entry("new"), 10 * 86_400, 7);
        assert_eq!(h.entries().len(), 1);
        assert_eq!(h.entries()[0].text, "new");
    }

    #[test]
    fn update_keeps_original_and_search_finds_both() {
        let mut h = History::load(&temp("upd"));
        let id = h.add(entry("ahoj evo"), 5, 0);
        h.update(id, "Ahoj Evo.", OutputMode::Smart);
        let e = h.get(id).unwrap();
        assert_eq!(e.original(), "ahoj evo");
        assert_eq!(e.mode, OutputMode::Smart);
        assert_eq!(h.search("EVO.").len(), 1);
        assert_eq!(h.search("ahoj evo").len(), 1);
        assert_eq!(h.search("nic").len(), 0);
        assert_eq!(h.search("").len(), 1);
    }

    #[test]
    fn save_load_and_clear() {
        let path = temp("io");
        let mut h = History::load(&path);
        h.add(
            NewEntry {
                raw: Some("raw"),
                ..entry("smart")
            },
            42,
            0,
        );
        h.save().unwrap();
        let loaded = History::load(&path);
        assert_eq!(loaded.entries(), h.entries());
        assert_eq!(loaded.entries()[0].original(), "raw");
        let mut loaded = loaded;
        loaded.clear();
        assert!(!path.exists());
        assert!(History::load(&path).entries().is_empty());
    }

    #[test]
    fn identical_raw_is_not_duplicated() {
        let mut h = History::load(&temp("dup"));
        let id = h.add(
            NewEntry {
                raw: Some("same"),
                ..entry("same")
            },
            1,
            0,
        );
        assert_eq!(h.get(id).unwrap().raw_text, None);
    }
}
