//! API key storage backed by the Windows Credential Manager (via `keyring`).
//! Keys never touch config.json or the log.

use anyhow::Result;
use keyring::Entry;

const SERVICE: &str = "com.gedeon.hlas";

fn entry(account: &str) -> Result<Entry> {
    Ok(Entry::new(SERVICE, account)?)
}

pub fn set_key(account: &str, secret: &str) -> Result<()> {
    entry(account)?.set_password(secret)?;
    Ok(())
}

pub fn get_key(account: &str) -> Option<String> {
    entry(account).ok()?.get_password().ok()
}

pub fn clear_key(account: &str) -> Result<()> {
    if let Ok(e) = entry(account) {
        // Deleting a missing credential is not an error we care about.
        let _ = e.delete_credential();
    }
    Ok(())
}

pub const GROQ: &str = "groq-api-key";
pub const OPENAI: &str = "openai-api-key";
