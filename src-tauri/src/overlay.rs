//! The menu over a game: Ctrl+F12 while a game Cerberus opened is in front
//! lays a see-through Cerberus window over the game's window, with how long
//! it's been played, a picture, closing the game and going back to Cerberus.
//!
//! It is a window of its own, placed over the game's: a game in exclusive full
//! screen can't have anything over it, so this works for windowed and
//! borderless ones. The window is made the first time and hidden after.

use cerberus_shared::tr;
use std::sync::Mutex;

use cerberus_shared::OverlayView;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

use crate::screenshots::win;

pub const LABEL: &str = "overlay";

/// The game the menu is over: its id and window (as a number, to be Send).
#[derive(Default)]
pub struct Overlay(Mutex<Option<(String, isize)>>);

impl Overlay {
    fn get(&self) -> Option<(String, isize)> {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    fn set(&self, value: Option<(String, isize)>) {
        *self.0.lock().unwrap_or_else(|p| p.into_inner()) = value;
    }
}

fn shown(app: &AppHandle) -> bool {
    app.get_webview_window(LABEL).and_then(|w| w.is_visible().ok()).unwrap_or(false)
}

/// Ctrl+F12: opens the menu over the game in front, or closes it when it's open.
pub fn toggle(app: &AppHandle) {
    if shown(app) {
        close(app);
        return;
    }
    let Some((window, game)) = crate::screenshots::game_in_front(app) else { return };
    let Some((x, y, width, height)) = win::client_rect(window) else {
        crate::notify::tell(app, crate::notify::Kind::Reply, tr!("Não foi possível abrir o menu"), tr!("o jogo está minimizado ou em tela cheia exclusiva"));
        return;
    };
    // A small window in front is a notice the game is showing (a message box),
    // not the game: the menu would be squeezed into it, and closing it doesn't
    // close the game.
    if width < 480 || height < 270 {
        crate::notify::tell(app, crate::notify::Kind::Reply, tr!("Não foi possível abrir o menu"), tr!("o jogo está mostrando um aviso numa janela pequena; responda o aviso primeiro"));
        return;
    }
    app.state::<Overlay>().set(Some((game.id.clone(), window as isize)));
    let place = |w: &tauri::WebviewWindow| {
        let _ = w.set_position(PhysicalPosition::new(x, y));
        let _ = w.set_size(PhysicalSize::new(width as u32, height as u32));
    };
    if let Some(existing) = app.get_webview_window(LABEL) {
        place(&existing);
        let _ = existing.emit("overlay-opened", ());
        let _ = existing.show();
        let _ = existing.set_focus();
        win::bring_to_front(existing.hwnd().map(|h| h.0 as isize).unwrap_or(0));
        return;
    }
    let made = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()))
        .title("Cerberus")
        .initialization_script("window.__CERBERUS_OVERLAY = true;")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(true)
        .visible(false)
        .build();
    match made {
        Ok(window) => {
            place(&window);
            let _ = window.show();
            let _ = window.set_focus();
            win::bring_to_front(window.hwnd().map(|h| h.0 as isize).unwrap_or(0));
        }
        Err(e) => crate::notify::tell(app, crate::notify::Kind::Reply, tr!("Não foi possível abrir o menu"), &e.to_string()),
    }
}

/// Hides the menu and gives the game its window back.
pub fn close(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
    }
    if let Some((_, game_window)) = app.state::<Overlay>().get() {
        win::bring_to_front(game_window);
    }
}

/// The game the menu is over, and its window, while it still runs.
fn target(app: &AppHandle) -> Result<(cerberus_shared::Game, isize), String> {
    let (id, window) = app.state::<Overlay>().get().ok_or(tr!("O menu não está aberto sobre um jogo."))?;
    if !app.state::<crate::sessions::Sessions>().is_running(&id) {
        return Err(tr!("O jogo já fechou.").into());
    }
    let game = app.state::<crate::library::LocalLibrary>().get(&id)?.ok_or(tr!("O jogo não está mais na biblioteca."))?;
    Ok((game, window))
}

#[tauri::command]
pub fn overlay_state(app: AppHandle) -> Result<OverlayView, String> {
    let (game, _) = target(&app)?;
    let session_secs = app.state::<crate::sessions::Sessions>().playing_for(&game.id).unwrap_or(0);
    let mut shots = crate::screenshots::list(&app.state::<crate::DataDir>().0, &game.id);
    shots.truncate(4);
    Ok(OverlayView { game, session_secs, shots })
}

#[tauri::command]
pub fn overlay_close(app: AppHandle) {
    close(&app);
}

/// A picture of the game: the menu steps aside for it, then comes back.
#[tauri::command]
pub async fn overlay_screenshot(app: AppHandle) -> Result<(), String> {
    let (game, window) = target(&app)?;
    let menu = app.get_webview_window(LABEL);
    if let Some(menu) = &menu {
        let _ = menu.hide();
    }
    // The game draws its next frame without the menu over it.
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    let taken = crate::screenshots::shoot(&app, window as _, &game);
    if let Some(menu) = &menu {
        let _ = menu.show();
        let _ = menu.set_focus();
    }
    taken
}

/// Asks the game to close, as its window's X would (it can still ask to save).
#[tauri::command]
pub fn overlay_quit_game(app: AppHandle) -> Result<(), String> {
    let (_, window) = target(&app)?;
    if let Some(menu) = app.get_webview_window(LABEL) {
        let _ = menu.hide();
    }
    win::ask_to_close(window);
    Ok(())
}

/// Leaves the game running and opens its page in Cerberus.
#[tauri::command]
pub fn overlay_open_launcher(app: AppHandle) -> Result<(), String> {
    let (game, _) = target(&app)?;
    if let Some(menu) = app.get_webview_window(LABEL) {
        let _ = menu.hide();
    }
    crate::background::show(&app);
    let _ = app.emit_to("main", "show-game", game.id);
    Ok(())
}

/// The game closed: the menu over it goes too.
pub fn game_closed(app: &AppHandle, id: &str) {
    let over_it = app.state::<Overlay>().get().is_some_and(|(game, _)| game == id);
    if over_it {
        app.state::<Overlay>().set(None);
        if let Some(menu) = app.get_webview_window(LABEL) {
            let _ = menu.hide();
        }
    }
}
