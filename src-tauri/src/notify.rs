//! Telling the player something. With Cerberus's window in front, a notice
//! inside it; otherwise (during a game, or hidden in the tray) a Windows
//! notification, so an achievement or a finished download isn't missed.

use std::sync::RwLock;

use cerberus_shared::Notices;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

static CHOSEN: RwLock<Option<Notices>> = RwLock::new(None);

/// What a notice is about, for the player's choices in Settings › Notifications.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Installed,
    Crashed,
    Screenshot,
    /// The answer to something the player just did: always given.
    Reply,
}

pub fn set(choices: Notices) {
    *CHOSEN.write().unwrap_or_else(|p| p.into_inner()) = Some(choices);
}

fn chosen() -> Notices {
    CHOSEN.read().unwrap_or_else(|p| p.into_inner()).clone().unwrap_or_default()
}

/// Whether the player wants this kind of notice at all.
pub fn wanted(kind: Kind, n: &Notices) -> bool {
    match kind {
        Kind::Installed => n.installed,
        Kind::Crashed => n.crashed,
        Kind::Screenshot => n.screenshots,
        Kind::Reply => true,
    }
}

fn window_in_front(app: &AppHandle) -> bool {
    app.get_webview_window("main").is_some_and(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false))
}

/// A notice with a title, shown where the player will see it, if they want it.
pub fn tell(app: &AppHandle, kind: Kind, title: &str, body: &str) {
    let n = chosen();
    if !wanted(kind, &n) {
        return;
    }
    if window_in_front(app) || !n.windows {
        let text = if body.is_empty() { title.to_string() } else { format!("{title}: {body}") };
        let _ = app.emit("toast", text);
        return;
    }
    let shown = app.notification().builder().title(title).body(body).show();
    if shown.is_err() {
        // Notifications turned off in Windows: the window still says it.
        let _ = app.emit("toast", format!("{title}: {body}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_always_come_and_the_rest_follow_the_choices() {
        let off = Notices { installed: false, crashed: false, screenshots: false, windows: false };
        assert!(wanted(Kind::Reply, &off));
        assert!(!wanted(Kind::Installed, &off) && !wanted(Kind::Crashed, &off) && !wanted(Kind::Screenshot, &off));
        let on = Notices::default();
        assert!(wanted(Kind::Installed, &on) && wanted(Kind::Crashed, &on) && wanted(Kind::Screenshot, &on) && on.windows);
        assert_eq!(serde_json::from_str::<Notices>("{}").unwrap(), on);
    }
}
