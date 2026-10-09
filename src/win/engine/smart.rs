//! Smart text: an LLM pass that formats the transcript by intent (prose,
//! list or procedure) without changing its meaning. Prompt and validation
//! are shared with macOS (`core::text`).

use super::cloud::classify;
use crate::core::errors::NetError;
use crate::core::text::{self, SmartProvider};
use anyhow::Result;
use std::time::Duration;

pub fn process(provider: SmartProvider, api_key: &str, input: &str) -> Result<String> {
    let name = provider.name();
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(25))
        .build()?;
    let resp = client
        .post(provider.endpoint())
        .bearer_auth(api_key)
        .json(&text::smart_request(provider, input))
        .send()
        .map_err(|e| classify(&e, name))?;
    let status = resp.status();
    let body = resp
        .text()
        .map_err(|_| NetError::InvalidResponse { provider: name })?;
    if !status.is_success() {
        log::warn!("{name} smart text HTTP {}", status.as_u16());
        return Err(NetError::Http {
            status: status.as_u16(),
            provider: name,
        }
        .into());
    }
    Ok(text::parse_smart_reply(&body, input)?)
}
