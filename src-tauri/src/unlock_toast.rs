//! The achievement notice over the game: a small see-through window in a corner
//! of the screen the game is on. It never takes the focus or the mouse from the
//! game, and shows the notices one after the other.
//!
//! Like the in-game menu, nothing can be drawn over a game in exclusive full
//! screen; borderless and windowed games show it.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use cerberus_shared::UnlockNotice;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "unlock";
/// The notice's size, in the screen's logical pixels, and its distance from the edges.
const WIDTH: f64 = 420.0;
const HEIGHT: f64 = 118.0;
const MARGIN: f64 = 28.0;

/// Notices waiting to be shown.
#[derive(Default)]
pub struct Queue(Mutex<VecDeque<UnlockNotice>>);

/// The window asks for the next notice every 6 s while it shows them: this
/// long without a word, with notices waiting, means it's stuck (crashed,
/// never loaded, reloaded).
const STUCK_AFTER: Duration = Duration::from_secs(10);

/// Notices kept while a game holds the screen in exclusive full screen, where
/// nothing can be drawn over it (the chime alone would say nothing).
static HELD: Mutex<Vec<UnlockNotice>> = Mutex::new(Vec::new());
static WAITING_SCREEN: AtomicBool = AtomicBool::new(false);

/// Windows' "a Direct3D game is in exclusive full screen" (QUNS_RUNNING_D3D_FULL_SCREEN).
const EXCLUSIVE_FULL_SCREEN: i32 = 3;

/// Whether a notice has to wait, for this state of the screen.
fn must_wait(state: i32) -> bool {
    state == EXCLUSIVE_FULL_SCREEN
}

#[cfg(windows)]
fn screen_state() -> i32 {
    use windows_sys::Win32::UI::Shell::SHQueryUserNotificationState;
    let mut state = 0;
    // SAFETY: a plain out-parameter.
    let ok = unsafe { SHQueryUserNotificationState(&mut state) } == 0;
    if ok { state } else { 0 }
}

#[cfg(not(windows))]
fn screen_state() -> i32 {
    0
}

/// What the held notices become once the screen is free: the one, or a summary.
fn summary(held: &[UnlockNotice]) -> Option<UnlockNotice> {
    match held {
        [] => None,
        [one] => Some(one.clone()),
        many => {
            let mut first = many[0].clone();
            first.title = cerberus_shared::tr!("{} conquistas durante a partida", many.len());
            first.name = many.iter().map(|n| n.name.as_str()).take(3).collect::<Vec<_>>().join(", ");
            first.description = if many.len() > 3 { cerberus_shared::tr!("e mais {}", many.len() - 3) } else { String::new() };
            Some(first)
        }
    }
}

/// Waits for the game to leave exclusive full screen, then shows what was held.
fn wait_for_screen(app: AppHandle) {
    if WAITING_SCREEN.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        while must_wait(screen_state()) {
            std::thread::sleep(Duration::from_secs(3));
        }
        let held: Vec<UnlockNotice> = std::mem::take(&mut *HELD.lock().unwrap_or_else(|p| p.into_inner()));
        WAITING_SCREEN.store(false, Ordering::SeqCst);
        if let Some(notice) = summary(&held) {
            show(&app, notice);
        }
    });
}

/// When the window last asked for a notice, or one was queued.
static LAST_SIGN: Mutex<Option<Instant>> = Mutex::new(None);
static WATCHING: AtomicBool = AtomicBool::new(false);

fn sign_of_life() {
    *LAST_SIGN.lock().unwrap_or_else(|p| p.into_inner()) = Some(Instant::now());
}

/// Notices are waiting and the window hasn't asked for one in `STUCK_AFTER`.
fn stuck(waiting: bool, since_last_sign: Duration) -> bool {
    waiting && since_last_sign > STUCK_AFTER
}

/// Watches the window while notices wait: a stuck one is made again; stuck
/// twice, the notices left go as Windows notifications and it's closed.
fn watch(app: AppHandle) {
    if WATCHING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        let mut strikes = 0;
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let waiting = !app.state::<Queue>().0.lock().unwrap_or_else(|p| p.into_inner()).is_empty();
            if !waiting {
                break;
            }
            let since = LAST_SIGN.lock().unwrap_or_else(|p| p.into_inner()).map_or(Duration::ZERO, |t| t.elapsed());
            if !stuck(waiting, since) {
                continue;
            }
            strikes += 1;
            if let Some(w) = app.get_webview_window(LABEL) {
                let _ = w.destroy();
            }
            if strikes == 1 {
                // Made again; it asks for the notices still waiting as it loads.
                sign_of_life();
                // In the same order; this watch goes on (`present` sees it running).
                present(&app);
                continue;
            }
            let left: Vec<UnlockNotice> = app.state::<Queue>().0.lock().unwrap_or_else(|p| p.into_inner()).drain(..).collect();
            for notice in left {
                crate::notify::tell(&app, crate::notify::Kind::Reply, &notice.title, &notice.name);
            }
            break;
        }
        WATCHING.store(false, Ordering::SeqCst);
    });
}

/// Shows a notice over the game. When the window can't be made, a Windows
/// notification says it instead. Never call it on the main thread (a sync
/// command): making the window there waits on itself.
pub fn show(app: &AppHandle, notice: UnlockNotice) {
    // Turned off in Settings › While playing.
    if !crate::in_game::get().unlock_notice {
        return;
    }
    // A game in exclusive full screen: nothing can be drawn over it. Kept, and
    // shown (one, or a summary) once it leaves the screen.
    if must_wait(screen_state()) {
        HELD.lock().unwrap_or_else(|p| p.into_inner()).push(notice);
        wait_for_screen(app.clone());
        return;
    }
    app.state::<Queue>().0.lock().unwrap_or_else(|p| p.into_inner()).push_back(notice);
    present(app);
}

/// Shows the notice window (made when it isn't there) for the notices queued.
fn present(app: &AppHandle) {
    let fallback = app.state::<Queue>().0.lock().unwrap_or_else(|p| p.into_inner()).front().map(|n| (n.title.clone(), n.name.clone())).unwrap_or_default();
    let window = match app.get_webview_window(LABEL) {
        Some(w) => w,
        None => {
            let made = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()))
                .title("Cerberus")
                .initialization_script("window.__CERBERUS_UNLOCK = true;")
                .decorations(false)
                .transparent(true)
                .shadow(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .focused(false)
                .focusable(false)
                .visible(false)
                .build();
            match made {
                Ok(w) => w,
                Err(_) => {
                    app.state::<Queue>().0.lock().unwrap_or_else(|p| p.into_inner()).clear();
                    crate::notify::tell(app, crate::notify::Kind::Reply, &fallback.0, &fallback.1);
                    return;
                }
            }
        }
    };
    place(app, &window);
    let _ = window.set_ignore_cursor_events(true);
    win::show_quietly(&window);
    let _ = window.emit("unlock-queued", ());
    // Nothing waiting sits forever behind a window that stopped asking.
    if !WATCHING.load(Ordering::SeqCst) {
        sign_of_life();
    }
    watch(app.clone());
}

/// Bottom right of the screen the game in front is on.
fn place(app: &AppHandle, window: &tauri::WebviewWindow) {
    let in_front = crate::screenshots::win::foreground().and_then(|(w, _)| crate::screenshots::win::client_rect(w));
    let monitor = match in_front {
        Some((x, y, w, h)) => app.monitor_from_point((x + w / 2) as f64, (y + h / 2) as f64).ok().flatten(),
        None => None,
    }
    .or_else(|| app.primary_monitor().ok().flatten());
    let Some(m) = monitor else { return };
    let scale = m.scale_factor();
    // The work area: above the taskbar.
    let area = m.work_area();
    let (mx, my) = (area.position.x, area.position.y);
    let (mw, mh) = (area.size.width as i32, area.size.height as i32);
    let (w, h, margin) = ((WIDTH * scale) as i32, (HEIGHT * scale) as i32, (MARGIN * scale) as i32);
    let _ = window.set_size(PhysicalSize::new(w as u32, h as u32));
    let corner = crate::in_game::get().corner;
    let x = if corner.ends_with('l') { mx + margin } else { mx + mw - w - margin };
    let y = if corner.starts_with('t') { my + margin } else { my + mh - h - margin };
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

/// The next notice, for the window to show; `None` hides it.
#[tauri::command]
pub fn unlock_next(app: AppHandle) -> Option<UnlockNotice> {
    sign_of_life();
    let next = app.state::<Queue>().0.lock().unwrap_or_else(|p| p.into_inner()).pop_front();
    if next.is_none() {
        if let Some(w) = app.get_webview_window(LABEL) {
            let _ = w.hide();
        }
    }
    next
}

/// Settings › Steam: shows a sample notice. Async on purpose: making a window
/// from a command that runs on the main thread would wait on itself forever.
#[tauri::command]
pub async fn unlock_test(app: AppHandle) {
    show(
        &app,
        UnlockNotice {
            title: cerberus_shared::tr!("Conquista rara desbloqueada ({}% dos jogadores)", "4,2").to_string(),
            name: cerberus_shared::tr!("Exemplo de conquista").to_string(),
            description: cerberus_shared::tr!("É assim que o Cerberus avisa durante o jogo.").to_string(),
            icon: None,
            kind: cerberus_shared::UnlockKind::Rare,
        },
    );
}

#[cfg(windows)]
mod win {
    use windows_sys::Win32::UI::WindowsAndMessaging::{HWND_TOPMOST, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos, ShowWindow};

    /// Shows the window on top without taking the focus from the game.
    pub fn show_quietly(window: &tauri::WebviewWindow) {
        let Ok(hwnd) = window.hwnd() else { return };
        let hwnd = hwnd.0 as windows_sys::Win32::Foundation::HWND;
        // SAFETY: a live window of this process; only how it's shown and its z-order change.
        unsafe {
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
}

#[cfg(not(windows))]
mod win {
    pub fn show_quietly(window: &tauri::WebviewWindow) {
        let _ = window.show();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notices_wait_out_exclusive_full_screen_and_come_as_one_summary() {
        assert!(must_wait(3));
        assert!(!must_wait(2), "a borderless full-screen game shows it over itself");
        assert!(!must_wait(5));
        let notice = |name: &str| UnlockNotice { title: "t".into(), name: name.into(), description: String::new(), icon: None, kind: cerberus_shared::UnlockKind::Normal };
        assert_eq!(summary(&[notice("a")]).unwrap().name, "a");
        let many = summary(&[notice("a"), notice("b"), notice("c"), notice("d")]).unwrap();
        assert!(many.title.contains('4') && many.name == "a, b, c" && many.description.contains('1'));
        assert!(summary(&[]).is_none());
    }

    #[test]
    fn a_window_that_stops_asking_is_stuck_only_with_notices_waiting() {
        assert!(!stuck(true, Duration::from_secs(6)), "one notice on screen is 6 s");
        assert!(stuck(true, Duration::from_secs(11)));
        assert!(!stuck(false, Duration::from_secs(60)), "nothing waiting: nothing to unstick");
    }
}
