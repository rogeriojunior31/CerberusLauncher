//! What Cerberus does while a game is open, as the player chose in Settings ›
//! While playing: the menu over the game (Ctrl+F12), pictures with F12, the
//! achievement notice and its corner, and stepping out of the way (minimizing
//! the window while the game runs, coming back when it closes).
//!
//! The choices are kept here so the threads that watch keys and games read
//! them without opening the settings file each time.

use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

use cerberus_shared::InGame;
use tauri::{AppHandle, Manager};

static NOW: RwLock<Option<InGame>> = RwLock::new(None);
/// Cerberus minimized itself for a game, so it comes back when the game closes.
static STEPPED_ASIDE: AtomicBool = AtomicBool::new(false);

pub fn set(choices: InGame) {
    *NOW.write().unwrap_or_else(|p| p.into_inner()) = Some(choices);
}

pub fn get() -> InGame {
    NOW.read().unwrap_or_else(|p| p.into_inner()).clone().unwrap_or_default()
}

/// A game started or stopped: minimize the window for it, or bring it back.
pub fn session(app: &AppHandle, running: bool) {
    let Some(window) = app.get_webview_window("main") else { return };
    if running {
        if get().minimize && window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(false) {
            STEPPED_ASIDE.store(true, Ordering::Relaxed);
            let _ = window.minimize();
        }
    } else if STEPPED_ASIDE.swap(false, Ordering::Relaxed) {
        let _ = window.unminimize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_starts_on_except_minimizing() {
        let d = InGame::default();
        assert!(d.menu && d.screenshots && d.unlock_notice && !d.minimize);
        assert_eq!(d.corner, "br");
        // An older settings file without the section reads the same defaults.
        let old: InGame = serde_json::from_str("{}").unwrap();
        assert_eq!(old, d);
    }
}
