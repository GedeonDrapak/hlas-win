//! Opts Hlas out of Windows 11 power throttling (EcoQoS).
//!
//! Hlas transcribes while another app is in front, which is exactly when
//! Windows parks it on efficiency cores at reduced clocks. A dictation should
//! finish as fast as the CPU allows, so the process asks to run at full speed.
//! Idle cost is unchanged: Hlas sleeps in message loops when not dictating.

use windows::Win32::System::Threading::{
    GetCurrentProcess, ProcessPowerThrottling, SetProcessInformation,
    PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
    PROCESS_POWER_THROTTLING_STATE,
};

pub fn disable_throttling() {
    let state = PROCESS_POWER_THROTTLING_STATE {
        Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        StateMask: 0,
    };
    let result = unsafe {
        SetProcessInformation(
            GetCurrentProcess(),
            ProcessPowerThrottling,
            &state as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        )
    };
    if let Err(e) = result {
        log::warn!("could not opt out of power throttling: {e}");
    }
}
