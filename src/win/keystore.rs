//! API keys in the Windows Credential Manager (via `keyring` with its
//! `windows-native` backend). Keys never touch config.json or the log.

use anyhow::Result;
use keyring::Entry;

const SERVICE: &str = "com.gedeon.hlas";
pub const GROQ: &str = "groq-api-key";
pub const OPENAI: &str = "openai-api-key";

fn entry(account: &str) -> Result<Entry> {
    Ok(Entry::new(SERVICE, account)?)
}

pub fn set_key(account: &str, secret: &str) -> Result<()> {
    entry(account)?.set_password(secret)?;
    Ok(())
}

pub fn get_key(account: &str) -> Option<String> {
    entry(account)
        .ok()?
        .get_password()
        .ok()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
}

pub fn has_key(account: &str) -> bool {
    get_key(account).is_some()
}

pub fn clear_key(account: &str) {
    if let Ok(e) = entry(account) {
        // Deleting a missing credential is not an error we care about.
        let _ = e.delete_credential();
    }
}

/// Stores a key, or removes it when the field was cleared.
pub fn persist(account: &str, value: &str) {
    let value = value.trim();
    if value.is_empty() {
        clear_key(account);
    } else if let Err(e) = set_key(account, value) {
        log::error!("credential store write failed for {account}: {e}");
    }
}
