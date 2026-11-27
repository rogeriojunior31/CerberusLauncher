//! Keeps the PC from going to sleep while a recomp downloads or installs (or a
//! game's setup step or a big copy runs), so a long job left running at night
//! is there in the morning. Only sleep is
//! held off: the screen still turns off as Windows is set to.

use std::time::Duration;

use tauri::{AppHandle, Manager};

/// Windows forgets the request after its idle timeout, so it's renewed well before.
const EVERY: Duration = Duration::from_secs(30);

/// Long work other than installs (a game's setup step, a big copy) holding sleep off.
static HELD: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Holds sleep off until the returned guard is dropped.
pub fn hold() -> Held {
    HELD.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    Held
}

pub struct Held;

impl Drop for Held {
    fn drop(&mut self) {
        HELD.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

fn held() -> bool {
    HELD.load(std::sync::atomic::Ordering::SeqCst) > 0
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(EVERY);
            if app.state::<crate::recomp::Installing>().any_active() || held() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_holds_sleep_off_while_it_runs() {
        let before = held();
        let guard = hold();
        assert!(held());
        drop(guard);
        assert_eq!(held(), before);
    }
}
