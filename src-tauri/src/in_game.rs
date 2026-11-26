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

/// The games open now (Cerberus's own sessions and the stores'), by id.
static OPEN: std::sync::Mutex<std::collections::BTreeSet<String>> = std::sync::Mutex::new(std::collections::BTreeSet::new());

/// Notes a game opening or closing; whether none is left open.
fn note(open: &mut std::collections::BTreeSet<String>, id: &str, running: bool) -> bool {
    if running {
        open.insert(id.to_string());
    } else {
        open.remove(id);
    }
    open.is_empty()
}

/// A game started or stopped: minimize the window for it, or bring it back
/// once no game is left open (two games: the first to close doesn't pull the
/// window over the other).
pub fn session(app: &AppHandle, id: &str, running: bool) {
    let none_left = note(&mut OPEN.lock().unwrap_or_else(|p| p.into_inner()), id, running);
    let Some(window) = app.get_webview_window("main") else { return };
    if running {
        if get().minimize && window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(false) {
            STEPPED_ASIDE.store(true, Ordering::Relaxed);
            let _ = window.minimize();
        }
    } else if none_left && STEPPED_ASIDE.swap(false, Ordering::Relaxed) {
        let _ = window.unminimize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_comes_back_when_the_last_game_closes() {
        let mut open = std::collections::BTreeSet::new();
        assert!(!note(&mut open, "recomp:a", true));
        assert!(!note(&mut open, "steam:1", true));
        assert!(!note(&mut open, "recomp:a", false), "the Steam game is still open");
        assert!(!note(&mut open, "steam:1", true), "told twice: still one");
        assert!(note(&mut open, "steam:1", false));
        assert!(note(&mut open, "steam:1", false), "an extra close is harmless");
    }

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
