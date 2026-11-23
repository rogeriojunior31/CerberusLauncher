//! Cerberus in the notification area (tray). Closing the window hides it there
//! when the player chose so, and always while a game is running: the playtime,
//! the save backup at the end of a session and RetroArch's achievements are
//! counted by this program, so it must outlive the window. Hidden only for a
//! game, it quits by itself once the last game closes.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime, Window};

/// RetroArch sessions running (recomps and PC games are in `Sessions`).
static CLASSICS: AtomicUsize = AtomicUsize::new(0);
/// The window was hidden only because a game was running.
static HIDDEN_FOR_GAME: AtomicBool = AtomicBool::new(false);

pub fn classic_started() {
    CLASSICS.fetch_add(1, Ordering::SeqCst);
}

pub fn classic_ended(app: &AppHandle) {
    let _ = CLASSICS.try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1));
    game_ended(app);
}

fn playing(app: &AppHandle) -> bool {
    CLASSICS.load(Ordering::SeqCst) > 0 || !app.state::<crate::sessions::Sessions>().running().is_empty()
}

fn keep_in_tray(app: &AppHandle) -> bool {
    app.path().app_data_dir().is_ok_and(|data| crate::settings::load(&data).close_to_tray)
}

/// What closing the main window does: true when it was hidden instead.
pub fn on_close<R: Runtime>(window: &Window<R>, app: &AppHandle) -> bool {
    let in_tray = keep_in_tray(app);
    let game = playing(app);
    if !in_tray && !game {
        return false;
    }
    HIDDEN_FOR_GAME.store(!in_tray, Ordering::SeqCst);
    let _ = window.hide();
    true
}

/// After a game closes: quit, if the window was hidden only to keep counting it.
pub fn game_ended(app: &AppHandle) {
    if !HIDDEN_FOR_GAME.load(Ordering::SeqCst) || playing(app) || keep_in_tray(app) {
        return;
    }
    let visible = app.get_webview_window("main").and_then(|w| w.is_visible().ok()).unwrap_or(true);
    if !visible {
        app.exit(0);
    }
}

pub fn show(app: &AppHandle) {
    HIDDEN_FOR_GAME.store(false, Ordering::SeqCst);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// The tray icon: a click opens the window; its menu opens or quits.
pub fn tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Abrir Cerberus", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let mut tray = TrayIconBuilder::with_id("cerberus")
        .tooltip("Cerberus Launcher")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}
