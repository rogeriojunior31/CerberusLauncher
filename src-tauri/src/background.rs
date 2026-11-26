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

/// What would be lost if Cerberus quit now, in the player's words; `None` when nothing.
pub fn busy(app: &AppHandle) -> Option<String> {
    if CLASSICS.load(Ordering::SeqCst) > 0 || !app.state::<crate::sessions::Sessions>().running().is_empty() {
        return Some(tr!("Um jogo aberto pelo Cerberus ainda está rodando: o tempo de jogo e o backup dos saves do fim da partida dependem dele.").into());
    }
    if crate::store_time::store_playing() {
        return Some(tr!("Um jogo da Steam, Epic ou Xbox está aberto e o Cerberus está contando o tempo dele.").into());
    }
    if app.state::<crate::recomp::Installing>().any_active() {
        return Some(tr!("Um download ou instalação está em andamento e seria interrompido.").into());
    }
    None
}

fn playing(app: &AppHandle) -> bool {
    busy(app).is_some()
}

/// While the window is hidden only to finish something, checks every few
/// seconds and quits once nothing is left (a download ending, a store game closing).
fn watch_until_idle(app: &AppHandle) {
    static WATCHING: AtomicBool = AtomicBool::new(false);
    if WATCHING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        while HIDDEN_FOR_GAME.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_secs(3));
            game_ended(&app);
        }
        WATCHING.store(false, Ordering::SeqCst);
    });
}

/// "Sair" in the tray: at once when nothing is going on; otherwise the player is asked first.
fn quit_asked(app: &AppHandle) {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    let Some(why) = busy(app) else {
        app.exit(0);
        return;
    };
    let handle = app.clone();
    app.dialog()
        .message(tr!("{}\n\nSair mesmo assim?", why))
        .title(tr!("Sair do Cerberus"))
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(tr!("Sair").into(), tr!("Continuar").into()))
        .show(move |quit| {
            if quit {
                handle.exit(0);
            }
        });
}

/// The tray icon is there (it can fail at sign-in, before the Explorer is ready).
static TRAY_READY: AtomicBool = AtomicBool::new(false);

/// Kept in the tray only when there is a tray to come back from.
fn keep_in_tray(app: &AppHandle) -> bool {
    TRAY_READY.load(Ordering::SeqCst) && app.path().app_data_dir().is_ok_and(|data| crate::settings::load(&data).close_to_tray)
}

/// The tray, without keeping the app from opening when it fails: tried again
/// every 5 s a few times (opened with Windows, the taskbar may not be up yet).
pub fn tray_or_later(app: &AppHandle) {
    match tray(app) {
        Ok(()) => TRAY_READY.store(true, Ordering::SeqCst),
        Err(e) => {
            eprintln!("tray: {e}");
            let app = app.clone();
            std::thread::spawn(move || {
                for _ in 0..6 {
                    std::thread::sleep(std::time::Duration::from_secs(5));
                    let again = app.clone();
                    let (tell, heard) = std::sync::mpsc::channel();
                    let _ = app.run_on_main_thread(move || {
                        let _ = tell.send(tray(&again).is_ok());
                    });
                    if heard.recv().unwrap_or(false) {
                        TRAY_READY.store(true, Ordering::SeqCst);
                        return;
                    }
                }
            });
        }
    }
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
    if !in_tray {
        // Hidden only to finish something that isn't a game: the player is told where it went.
        let working = app.state::<crate::recomp::Installing>().any_active() && CLASSICS.load(Ordering::SeqCst) == 0 && app.state::<crate::sessions::Sessions>().running().is_empty();
        if working {
            crate::notify::tell(app, crate::notify::Kind::Reply, &tr!("O Cerberus continua aberto"), &tr!("Ele fecha sozinho quando o download terminar. Para abrir de novo, use o ícone na bandeja."));
        }
        watch_until_idle(app);
    }
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
            "quit" => quit_asked(app),
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
