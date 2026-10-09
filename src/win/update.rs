//! Manual "Check for updates". Hlas never phones home on its own: this runs
//! only when the user clicks it, and talks only to the GitHub releases API.

use super::wide;
use crate::core::version;
use std::time::Duration;
use windows::core::PCWSTR;
use windows::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDYES, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND, MB_TOPMOST, MB_YESNO,
};

const API: &str = "https://api.github.com/repos/GedeonDrapak/hlas-win/releases?per_page=10";

fn latest() -> anyhow::Result<(String, String)> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(concat!("Hlas-Windows/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let releases: serde_json::Value = client.get(API).send()?.error_for_status()?.json()?;
    let newest = releases
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| !r["draft"].as_bool().unwrap_or(false))
        .filter_map(|r| {
            Some((
                r["tag_name"].as_str()?.to_string(),
                r["html_url"].as_str()?.to_string(),
            ))
        })
        .max_by_key(|(tag, _)| version::parse(tag).unwrap_or((0, 0, 0)))
        .ok_or_else(|| anyhow::anyhow!("no releases"))?;
    Ok(newest)
}

fn message(text: &str, ask: bool) -> bool {
    let body = wide(text);
    let title = wide("Hlas");
    let style = MB_TOPMOST
        | MB_SETFOREGROUND
        | if ask {
            MB_YESNO | MB_ICONINFORMATION
        } else {
            MB_OK | MB_ICONINFORMATION
        };
    unsafe { MessageBoxW(None, PCWSTR(body.as_ptr()), PCWSTR(title.as_ptr()), style) == IDYES }
}

pub fn check_in_background(report_up_to_date: bool) {
    std::thread::spawn(move || {
        let current = env!("CARGO_PKG_VERSION");
        match latest() {
            Ok((tag, url)) if version::is_newer(&tag, current) => {
                if message(
                    &format!(
                        "Hlas {tag} is available (you have {current}).\n\nOpen the download page?"
                    ),
                    true,
                ) {
                    super::shell::open(&url);
                }
            }
            Ok(_) => {
                if report_up_to_date {
                    message(&format!("You have the latest Hlas ({current})."), false);
                }
            }
            Err(e) => {
                log::warn!("update check failed: {e}");
                if report_up_to_date {
                    message(
                        "Could not check for updates. Check your connection and try again.",
                        false,
                    );
                }
            }
        }
    });
}
