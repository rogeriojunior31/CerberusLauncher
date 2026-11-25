//! Keeps the PC from going to sleep while a recomp downloads or installs, so a
//! long download left running at night is there in the morning. Only sleep is
//! held off: the screen still turns off as Windows is set to.

use std::time::Duration;

use tauri::{AppHandle, Manager};

/// Windows forgets the request after its idle timeout, so it's renewed well before.
const EVERY: Duration = Duration::from_secs(30);

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(EVERY);
            if app.state::<crate::recomp::Installing>().any_active() {
                stay_awake();
            }
        }
    });
}

#[cfg(windows)]
fn stay_awake() {
    use windows_sys::Win32::System::Power::{ES_SYSTEM_REQUIRED, SetThreadExecutionState};
    // Without ES_CONTINUOUS this only resets the sleep timer, once; nothing to undo later.
    unsafe {
        SetThreadExecutionState(ES_SYSTEM_REQUIRED);
    }
}

#[cfg(not(windows))]
fn stay_awake() {}
