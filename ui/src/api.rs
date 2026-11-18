//! Calls into the Rust backend through Tauri's IPC.

use cerberus_shared::{Backup, Game, InstallProgress, RecompEntry, SessionEvent, SettingsView};
use serde::Serialize;
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    // Exposed because tauri.conf.json sets `withGlobalTauri`.
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;

    /// Turns a local file path into a URL the webview can load (asset protocol).
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = convertFileSrc)]
    pub fn convert_file_src(path: &str) -> String;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], catch)]
    async fn listen(event: &str, handler: &Closure<dyn FnMut(JsValue)>) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "window"], js_name = getCurrentWindow)]
    fn current_window() -> JsValue;
}

/// The window has no system frame, so its buttons call these.
pub fn window_action(action: &str) {
    let window = current_window();
    let Ok(method) = js_sys::Reflect::get(&window, &JsValue::from_str(action)) else {
        return;
    };
    if let Some(method) = method.dyn_ref::<js_sys::Function>() {
        let _ = method.call0(&window);
    }
}

/// Calls `handler` for every `event` the backend sends, for as long as the app runs.
fn on_event<T: DeserializeOwned + 'static>(event: &'static str, mut handler: impl FnMut(T) + 'static) {
    let closure = Closure::<dyn FnMut(JsValue)>::new(move |event: JsValue| {
        let payload = js_sys::Reflect::get(&event, &JsValue::from_str("payload")).unwrap_or(JsValue::NULL);
        if let Ok(value) = serde_wasm_bindgen::from_value(payload) {
            handler(value);
        }
    });
    wasm_bindgen_futures::spawn_local(async move {
        let _ = listen(event, &closure).await;
        // The listener lives as long as the window, so its closure must too.
        closure.forget();
    });
}

pub fn on_install_progress(handler: impl FnMut(InstallProgress) + 'static) {
    on_event("recomp-progress", handler);
}

/// A game the launcher started opened or closed.
pub fn on_session(handler: impl FnMut(SessionEvent) + 'static) {
    on_event("game-session", handler);
}

/// The catalog or the ROM folders' matches changed; the recomp list should be read again.
pub fn on_catalog_changed(mut handler: impl FnMut() + 'static) {
    on_event("catalog-changed", move |_: serde::de::IgnoredAny| handler());
}

async fn call<A: Serialize, R: DeserializeOwned>(cmd: &str, args: &A) -> Result<R, String> {
    let args = serde_wasm_bindgen::to_value(args).map_err(|e| e.to_string())?;
    let reply = invoke(cmd, args).await.map_err(|e| e.as_string().unwrap_or_else(|| format!("{e:?}")))?;
    serde_wasm_bindgen::from_value(reply).map_err(|e| e.to_string())
}

#[derive(Serialize)]
struct Id<'a> {
    id: &'a str,
}

pub async fn library_recovery_notice() -> Result<Option<String>, String> {
    call("library_recovery_notice", &()).await
}

pub async fn list_games() -> Result<Vec<Game>, String> {
    call("list_games", &()).await
}

#[derive(Serialize)]
struct PlayArgs<'a> {
    game: &'a Game,
}

pub async fn play(game: &Game) -> Result<(), String> {
    call("play", &PlayArgs { game }).await
}

/// Opens a file picker; `None` when the user cancels.
pub async fn add_local_game() -> Result<Option<Game>, String> {
    call("add_local_game", &()).await
}

#[derive(Serialize)]
struct RenameArgs<'a> {
    id: &'a str,
    name: &'a str,
}

pub async fn rename_game(id: &str, name: &str) -> Result<Game, String> {
    call("rename_game", &RenameArgs { id, name }).await
}

pub async fn find_cover(id: &str) -> Result<Game, String> {
    call("find_cover", &Id { id }).await
}

/// Opens an image picker; `None` when the user cancels.
pub async fn pick_cover(id: &str) -> Result<Option<Game>, String> {
    call("pick_cover", &Id { id }).await
}

pub async fn remove_game(id: &str) -> Result<(), String> {
    call("remove_game", &Id { id }).await
}

pub async fn list_recomps() -> Result<Vec<RecompEntry>, String> {
    call("list_recomps", &()).await
}

/// Resolves when the install finishes; progress comes through `on_install_progress`.
pub async fn install_recomp(id: &str) -> Result<Game, String> {
    call("install_recomp", &Id { id }).await
}

pub async fn open_project(id: &str) -> Result<(), String> {
    call("open_project", &Id { id }).await
}

/// Opens the "Pedir um recomp" form on GitHub.
pub async fn open_recomp_request() -> Result<(), String> {
    call("open_recomp_request", &()).await
}

pub async fn cancel_install(id: &str) -> Result<(), String> {
    call("cancel_install", &Id { id }).await
}

pub async fn running_games() -> Result<Vec<String>, String> {
    call("running_games", &()).await
}

pub async fn list_backups(id: &str) -> Result<Vec<Backup>, String> {
    call("list_backups", &Id { id }).await
}

/// `None` when there was nothing new to copy.
pub async fn backup_now(id: &str) -> Result<Option<Backup>, String> {
    call("backup_now", &Id { id }).await
}

#[derive(Serialize)]
struct RestoreArgs<'a> {
    id: &'a str,
    name: &'a str,
}

/// How many files were put back.
pub async fn restore_backup(id: &str, name: &str) -> Result<u32, String> {
    call("restore_backup", &RestoreArgs { id, name }).await
}

pub async fn open_folder(id: &str) -> Result<(), String> {
    call("open_folder", &Id { id }).await
}

pub async fn open_backups(id: &str) -> Result<(), String> {
    call("open_backups", &Id { id }).await
}

pub async fn get_settings() -> Result<SettingsView, String> {
    call("get_settings", &()).await
}

/// Opens a folder picker; the settings as they are afterwards.
pub async fn pick_recomps_dir() -> Result<SettingsView, String> {
    call("pick_recomps_dir", &()).await
}

pub async fn reset_recomps_dir() -> Result<SettingsView, String> {
    call("reset_recomps_dir", &()).await
}

pub async fn open_data_dir() -> Result<(), String> {
    call("open_data_dir", &()).await
}

pub async fn clear_art_cache() -> Result<SettingsView, String> {
    call("clear_art_cache", &()).await
}

/// Opens a folder picker, then looks through the new folder.
pub async fn add_rom_dir() -> Result<SettingsView, String> {
    call("add_rom_dir", &()).await
}

#[derive(Serialize)]
struct PathArg<'a> {
    path: &'a str,
}

pub async fn remove_rom_dir(path: &str) -> Result<SettingsView, String> {
    call("remove_rom_dir", &PathArg { path }).await
}

/// The same, owning its path, for code that boxes the future.
pub async fn remove_rom_dir_owned(path: String) -> Result<SettingsView, String> {
    remove_rom_dir(&path).await
}

pub async fn scan_roms() -> Result<SettingsView, String> {
    call("scan_roms", &()).await
}

pub async fn refresh_catalog() -> Result<SettingsView, String> {
    call("refresh_catalog", &()).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CopyArgs<'a> {
    id: &'a str,
    use_found: bool,
    folder: bool,
}

/// Sets a recomp up with a copy of its game: the one found in the ROM folders,
/// or one picked in a dialog, a file or (with `folder`) a folder of extracted
/// files. `None` when cancelled.
pub async fn set_game_copy(id: &str, use_found: bool, folder: bool) -> Result<Option<Game>, String> {
    call("set_game_copy", &CopyArgs { id, use_found, folder }).await
}

/// After an installer ran: the player points at the game's program. `None` when cancelled.
pub async fn locate_game(id: &str) -> Result<Option<Game>, String> {
    call("locate_game", &Id { id }).await
}

/// An installer closed and the game it installed wasn't found; carries the game's id.
pub fn on_needs_location(handler: impl FnMut(String) + 'static) {
    on_event("setup-needs-location", handler);
}

#[derive(Serialize)]
struct LaunchArgs<'a> {
    id: &'a str,
    args: &'a str,
}

pub async fn set_launch_args(id: &str, args: &str) -> Result<Game, String> {
    call("set_launch_args", &LaunchArgs { id, args }).await
}

/// `None` when the game has no mods on Thunderstore.
pub async fn list_mods(id: &str) -> Result<Option<cerberus_shared::ModList>, String> {
    call("list_mods", &Id { id }).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModArgs<'a> {
    id: &'a str,
    full_name: &'a str,
}

pub async fn install_mod(id: &str, full_name: &str) -> Result<(), String> {
    call("install_mod", &ModArgs { id, full_name }).await
}

pub async fn remove_mod(id: &str, full_name: &str) -> Result<(), String> {
    call("remove_mod", &ModArgs { id, full_name }).await
}

#[derive(Serialize)]
struct ClearArg {
    clear: bool,
}

/// Picks the synced folder for save backups, or turns it off with `clear`.
pub async fn set_cloud_dir(clear: bool) -> Result<SettingsView, String> {
    call("set_cloud_dir", &ClearArg { clear }).await
}

#[derive(Serialize)]
struct UrlArg {
    url: String,
}

pub async fn add_source(url: String) -> Result<SettingsView, String> {
    call("add_source", &UrlArg { url }).await
}

pub async fn remove_source(url: String) -> Result<SettingsView, String> {
    call("remove_source", &UrlArg { url }).await
}

/// A message from the backend for the player (cloud saves brought down, say).
pub fn on_toast(handler: impl FnMut(String) + 'static) {
    on_event("toast", handler);
}

/// Calls a window method that takes one true/false, like `setFullscreen`.
pub fn window_toggle(action: &str, on: bool) {
    let window = current_window();
    let Ok(method) = js_sys::Reflect::get(&window, &JsValue::from_str(action)) else {
        return;
    };
    if let Some(method) = method.dyn_ref::<js_sys::Function>() {
        let _ = method.call1(&window, &JsValue::from_bool(on));
    }
}
