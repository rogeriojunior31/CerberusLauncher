//! Telling the player something. With Cerberus's window in front, a notice
//! inside it; otherwise (during a game, or hidden in the tray) a Windows
//! notification, so an achievement or a finished download isn't missed.

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

fn window_in_front(app: &AppHandle) -> bool {
    app.get_webview_window("main").is_some_and(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false))
}

/// A notice with a title, shown where the player will see it.
pub fn tell(app: &AppHandle, title: &str, body: &str) {
    if window_in_front(app) {
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
