//! Cerberus in the notification area (tray). Closing the window hides it there
//! when the player chose so, and always while a game is running: the playtime,
//! the save backup at the end of a session and RetroArch's achievements are
//! counted by this program, so it must outlive the window. Hidden only for a
//! game, it quits by itself once the last game closes.

use cerberus_shared::tr;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use cerberus_shared::Game;
use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime, Window};

/// RetroArch sessions running (recomps and PC games are in `Sessions`).
static CLASSICS: AtomicUsize = AtomicUsize::new(0);
/// The window was hidden only because a game was running.
static HIDDEN_FOR_GAME: AtomicBool = AtomicBool::new(false);

pub fn classic_started() {
    CLASSICS.fetch_add(1, Ordering::SeqCst);
}

/// A classic is open in an emulator right now.
pub fn classic_running() -> bool {
    CLASSICS.load(Ordering::SeqCst) > 0
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

/// The games played most recently, offered in the tray menu (the window sends them).
static RECENT: Mutex<Vec<Game>> = Mutex::new(Vec::new());
/// At most this many games in the tray menu.
pub const RECENT_GAMES: usize = 5;

/// The tray icon: a click opens the window; its menu plays a recent game, opens or quits.
fn tray_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let recent = RECENT.lock().map(|r| r.clone()).unwrap_or_default();
    let mut items: Vec<Box<dyn IsMenuItem<tauri::Wry>>> = Vec::new();
    if !recent.is_empty() {
        items.push(Box::new(MenuItem::with_id(app, "recent", tr!("Jogados recentemente"), false, None::<&str>)?));
        for (i, game) in recent.iter().enumerate() {
            // An ampersand would underline the next letter in a Windows menu.
            let name = game.name.replace('&', "&&");
            items.push(Box::new(MenuItem::with_id(app, format!("play:{i}"), format!("▶  {name}"), true, None::<&str>)?));
        }
        items.push(Box::new(PredefinedMenuItem::separator(app)?));
    }
    items.push(Box::new(MenuItem::with_id(app, "open", tr!("Abrir Cerberus"), true, None::<&str>)?));
    items.push(Box::new(MenuItem::with_id(app, "quit", tr!("Sair"), true, None::<&str>)?));
    let refs: Vec<&dyn IsMenuItem<tauri::Wry>> = items.iter().map(|i| i.as_ref()).collect();
    Menu::with_items(app, &refs)
}

/// The recent games for the tray menu; rebuilt only when they changed.
pub fn set_recent(app: &AppHandle, games: Vec<Game>) {
    let games: Vec<Game> = games.into_iter().take(RECENT_GAMES).collect();
    {
        let Ok(mut now) = RECENT.lock() else { return };
        let same = now.len() == games.len() && now.iter().zip(&games).all(|(a, b)| a.id == b.id && a.name == b.name);
        if same {
            return;
        }
        *now = games;
    }
    retitle_tray(app);
}

/// Plays a game picked in the tray menu, telling the player when it can't open.
fn play_recent(app: &AppHandle, index: usize) {
    let Some(game) = RECENT.lock().ok().and_then(|r| r.get(index).cloned()) else { return };
    if let Err(e) = crate::play(app.clone(), game.clone(), None, None) {
        crate::notify::tell(app, crate::notify::Kind::Reply, &tr!("{} não abriu", game.name), &e);
    }
}

/// The tray menu again, in the language just picked.
pub fn retitle_tray(app: &AppHandle) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id("cerberus"), tray_menu(app)) {
        let _ = tray.set_menu(Some(menu));
    }
}

pub fn tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = tray_menu(app)?;
    let mut tray = TrayIconBuilder::with_id("cerberus")
        .tooltip(tr!("Cerberus Launcher"))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show(app),
            "quit" => app.exit(0),
            id => {
                if let Some(i) = id.strip_prefix("play:").and_then(|i| i.parse().ok()) {
                    play_recent(app, i);
                }
            }
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
