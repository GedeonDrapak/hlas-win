//! Text rules shared with Hlas for macOS.
//!
//! `shared/hlas-shared.json` is generated from the macOS Swift sources by
//! `tools/shared_from_mac.py`, and CI fails when the two drift apart. Edit the
//! Swift sources, then regenerate - never edit the JSON by hand.

use once_cell::sync::Lazy;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct ModelInfo {
    pub file: String,
    pub url: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Deserialize)]
pub struct SmartModels {
    pub groq: String,
    pub openai: String,
}

#[derive(Debug, Deserialize)]
pub struct Shared {
    pub smart_text_prompt: String,
    pub smart_text_models: SmartModels,
    pub priming_prompts: HashMap<String, String>,
    pub hallucination_patterns: Vec<String>,
    pub hallucination_ratio: f64,
    pub model: ModelInfo,
}

pub static SHARED: Lazy<Shared> = Lazy::new(|| {
    serde_json::from_str(include_str!("../../shared/hlas-shared.json"))
        .expect("shared/hlas-shared.json must be valid")
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_data_is_complete() {
        assert!(SHARED.smart_text_prompt.starts_with("You are Hlas"));
        assert!(SHARED.priming_prompts.contains_key("cs"));
        assert!(SHARED.hallucination_patterns.len() >= 5);
        assert_eq!(SHARED.model.sha256.len(), 64);
        assert!(SHARED.model.bytes > 500_000_000);
    }

    #[test]
    fn shared_data_has_no_long_dashes() {
        let raw = include_str!("../../shared/hlas-shared.json");
        assert!(!raw.contains('\u{2014}') && !raw.contains('\u{2013}'));
    }
}
