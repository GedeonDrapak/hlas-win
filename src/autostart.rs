//! Launch-at-login via the per-user Run registry key. No admin rights needed.

use anyhow::Result;
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "Hlas";

fn exe_path() -> Result<String> {
    Ok(std::env::current_exe()?.to_string_lossy().into_owned())
}

pub fn set(enabled: bool) -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (run, _) = hkcu.create_subkey(RUN_KEY)?;
    if enabled {
        run.set_value(VALUE, &format!("\"{}\"", exe_path()?))?;
    } else {
        let _ = run.delete_value(VALUE);
    }
    Ok(())
}

pub fn is_enabled() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey(RUN_KEY)
        .and_then(|run| run.get_value::<String, _>(VALUE))
        .is_ok()
}
