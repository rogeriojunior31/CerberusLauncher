//! The achievement notice over the game: a small see-through window in a corner
//! of the screen the game is on. It never takes the focus or the mouse from the
//! game, and shows the notices one after the other.
//!
//! Like the in-game menu, nothing can be drawn over a game in exclusive full
//! screen; borderless and windowed games show it.

use std::collections::VecDeque;
use std::sync::Mutex;

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

/// Shows a notice over the game. When the window can't be made, a Windows
/// notification says it instead. Never call it on the main thread (a sync
/// command): making the window there waits on itself.
pub fn show(app: &AppHandle, notice: UnlockNotice) {
    let fallback = (notice.title.clone(), notice.name.clone());
    app.state::<Queue>().0.lock().unwrap_or_else(|p| p.into_inner()).push_back(notice);
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
                    crate::notify::tell(app, &fallback.0, &fallback.1);
                    return;
                }
            }
        }
    };
    place(app, &window);
    let _ = window.set_ignore_cursor_events(true);
    win::show_quietly(&window);
    let _ = window.emit("unlock-queued", ());
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
    let _ = window.set_position(PhysicalPosition::new(mx + mw - w - margin, my + mh - h - margin));
}

/// The next notice, for the window to show; `None` hides it.
#[tauri::command]
pub fn unlock_next(app: AppHandle) -> Option<UnlockNotice> {
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
