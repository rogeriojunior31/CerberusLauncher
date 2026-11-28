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

/// A game's achievements changed (an unlock seen live, or a store game's list
/// read again when it closed): while the page that asked is on screen.
pub fn on_achievements_changed_here(mut handler: impl FnMut() + 'static) {
    on_event_here("achievements-changed", move |_: serde::de::IgnoredAny| handler());
}

/// Calls `handler` for every `event` while the component that asked is on
/// screen: the listener goes when it does (one per visit otherwise, each
/// reading the component's state after it was gone).
fn on_event_here<T: DeserializeOwned + 'static>(event: &'static str, mut handler: impl FnMut(T) + 'static) {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    let closure = Rc::new(Closure::<dyn FnMut(JsValue)>::new(move |event: JsValue| {
        let payload = js_sys::Reflect::get(&event, &JsValue::from_str("payload")).unwrap_or(JsValue::NULL);
        if let Ok(value) = serde_wasm_bindgen::from_value(payload) {
            handler(value);
        }
    }));
    let stop: Rc<RefCell<Option<js_sys::Function>>> = Rc::new(RefCell::new(None));
    let gone = Rc::new(Cell::new(false));
    {
        let (closure, stop, gone) = (closure.clone(), stop.clone(), gone.clone());
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(unlisten) = listen(event, &closure).await {
                let unlisten: js_sys::Function = unlisten.unchecked_into();
                if gone.get() {
                    let _ = unlisten.call0(&JsValue::NULL);
                } else {
                    *stop.borrow_mut() = Some(unlisten);
                }
            }
        });
    }
    let held = send_wrapper::SendWrapper::new((closure, stop, gone));
    leptos::prelude::on_cleanup(move || {
        let (closure, stop, gone) = held.take();
        gone.set(true);
        if let Some(unlisten) = stop.borrow_mut().take() {
            let _ = unlisten.call0(&JsValue::NULL);
        }
        drop(closure);
    });
}

/// A picture was taken, while the asking component is on screen.
pub fn on_screenshot_here(handler: impl FnMut(String) + 'static) {
    on_event_here("screenshot-taken", handler);
}

/// The menu over a game was opened again (its window is kept between uses).
pub fn on_overlay_opened(mut handler: impl FnMut() + 'static) {
    on_event("overlay-opened", move |_: ()| handler());
}

/// A new achievement notice is waiting (in the notice's own window).
pub fn on_unlock_queued(mut handler: impl FnMut() + 'static) {
    on_event("unlock-queued", move |_: ()| handler());
}

/// The next achievement notice to show; `None` hides the window.
pub async fn unlock_next() -> Result<Option<cerberus_shared::UnlockNotice>, String> {
    call("unlock_next", &()).await
}

/// Shows a sample achievement notice.
pub async fn unlock_test() -> Result<(), String> {
    call("unlock_test", &()).await
}

/// The menu over a game asked for the game's page in the launcher.
pub fn on_show_game(handler: impl FnMut(String) + 'static) {
    on_event("show-game", handler);
}

pub async fn dlss_list(id: &str) -> Result<Vec<cerberus_shared::DlssFile>, String> {
    call("dlss_list", &Id { id }).await
}

#[derive(serde::Serialize)]
struct DlssArg<'a> {
    id: &'a str,
    path: &'a str,
}

pub async fn dlss_swap(id: &str, path: &str) -> Result<Option<String>, String> {
    call("dlss_swap", &DlssArg { id, path }).await
}

pub async fn dlss_restore(id: &str, path: &str) -> Result<(), String> {
    call("dlss_restore", &DlssArg { id, path }).await
}

pub async fn overlay_state() -> Result<cerberus_shared::OverlayView, String> {
    call("overlay_state", &()).await
}

pub async fn overlay_close() -> Result<(), String> {
    call("overlay_close", &()).await
}

pub async fn overlay_screenshot() -> Result<(), String> {
    call("overlay_screenshot", &()).await
}

pub async fn overlay_quit_game() -> Result<(), String> {
    call("overlay_quit_game", &()).await
}

pub async fn overlay_open_launcher() -> Result<(), String> {
    call("overlay_open_launcher", &()).await
}

/// A picture was taken of a game; the game's id.
pub fn on_screenshot(handler: impl FnMut(String) + 'static) {
    on_event("screenshot-taken", handler);
}

pub async fn list_screenshots(id: &str) -> Result<Vec<cerberus_shared::Screenshot>, String> {
    call("list_screenshots", &Id { id }).await
}

pub async fn open_screenshot(path: &str) -> Result<(), String> {
    #[derive(serde::Serialize)]
    struct Arg<'a> {
        path: &'a str,
    }
    call("open_screenshot", &Arg { path }).await
}

pub async fn open_screenshots_folder(id: &str) -> Result<(), String> {
    call("open_screenshots_folder", &Id { id }).await
}

pub fn on_install_progress(handler: impl FnMut(InstallProgress) + 'static) {
    on_event("recomp-progress", handler);
}

/// How a game's setup step is going.
pub fn on_prepare_progress(handler: impl FnMut(cerberus_shared::PrepareProgress) + 'static) {
    on_event("prepare-progress", handler);
}

/// A game the launcher started opened or closed.
pub fn on_session(handler: impl FnMut(SessionEvent) + 'static) {
    on_event("game-session", handler);
}

pub async fn pc_status() -> Result<cerberus_shared::PcStatus, String> {
    call("pc_status", &()).await
}

pub async fn install_vc() -> Result<cerberus_shared::PcStatus, String> {
    call("install_vc", &()).await
}

pub async fn install_dx() -> Result<cerberus_shared::PcStatus, String> {
    call("install_dx", &()).await
}

/// A desktop shortcut asked the running Cerberus to play a game.
pub fn on_play_request(handler: impl FnMut(String) + 'static) {
    on_event("play-request", handler);
}

/// The game a shortcut asked for when Cerberus started, if any.
pub async fn take_play_request() -> Option<String> {
    call::<(), Option<String>>("take_play_request", &()).await.ok().flatten()
}

/// Puts a shortcut that plays this game on the desktop; where it went.
pub async fn create_shortcut(id: &str) -> Result<String, String> {
    call("create_shortcut", &Id { id }).await
}

pub async fn add_to_steam(id: &str) -> Result<String, String> {
    call("add_to_steam", &Id { id }).await
}

pub async fn remove_from_steam(id: &str) -> Result<(), String> {
    call("remove_from_steam", &Id { id }).await
}

pub async fn in_steam(id: &str) -> Result<bool, String> {
    call("in_steam", &Id { id }).await
}

pub fn on_playtime_updated(mut handler: impl FnMut() + 'static) {
    on_event("playtime-updated", move |_: String| handler());
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
    /// "bigpicture" when Play was pressed in Big Picture mode, else "desktop".
    mode: &'a str,
    pad: Option<&'a str>,
}

pub async fn play(game: &Game, big_picture: bool) -> Result<(), String> {
    let mode = if big_picture { "bigpicture" } else { "desktop" };
    call("play", &PlayArgs { game, mode, pad: crate::tv::input::connected_pad() }).await
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
    remove_game_with(id, false, false).await
}

/// Uninstalls with the player's answer to what stopped it before: without the
/// saves backup, or keeping a folder Cerberus didn't make (only the entry goes).
pub async fn remove_game_with(id: &str, without_backup: bool, keep_folder: bool) -> Result<(), String> {
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Arg<'a> {
        id: &'a str,
        without_backup: bool,
        keep_folder: bool,
    }
    call("remove_game", &Arg { id, without_backup, keep_folder }).await
}

pub async fn list_recomps() -> Result<Vec<RecompEntry>, String> {
    call("list_recomps", &()).await
}

/// A page of the source lists' games, searched and sorted by the app (the
/// window never holds the lists whole).
pub async fn search_lists(needle: &str, console: &str, owned: bool, sort: &str, limit: usize) -> Result<cerberus_shared::CatalogPage, String> {
    #[derive(serde::Serialize)]
    struct Arg<'a> {
        needle: &'a str,
        console: &'a str,
        owned: bool,
        sort: &'a str,
        limit: usize,
    }
    call("search_lists", &Arg { needle, console, owned, sort, limit }).await
}

/// One catalog entry by id, a source list's game included.
pub async fn catalog_entry(id: &str) -> Result<Option<RecompEntry>, String> {
    #[derive(serde::Serialize)]
    struct Arg<'a> {
        id: &'a str,
    }
    call("catalog_entry", &Arg { id }).await
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

/// Moves a recomp to a folder the player picks; `None` when they cancel.
pub async fn move_game(id: &str) -> Result<Option<Game>, String> {
    call("move_game", &Id { id }).await
}

pub async fn pause_install(id: &str) -> Result<(), String> {
    call("pause_install", &Id { id }).await
}

pub async fn running_games() -> Result<Vec<String>, String> {
    call("running_games", &()).await
}

pub async fn list_backups(id: &str) -> Result<Vec<Backup>, String> {
    call("list_backups", &Id { id }).await
}

/// `None` when there was nothing new to copy.
/// Looks through a folder the player picks for recomps installed without Cerberus.
/// `None` when the picker is cancelled.
pub async fn scan_for_recomps() -> Result<Option<cerberus_shared::AdoptScan>, String> {
    call("scan_for_recomps", &()).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AdoptArgs<'a> {
    recomp_id: &'a str,
    program: Option<String>,
}

/// Brings a recomp installed without Cerberus into the library; with no
/// `program`, the player picks it. `None` when cancelled.
pub async fn adopt_recomp(recomp_id: &str, program: Option<String>) -> Result<Option<Game>, String> {
    call("adopt_recomp", &AdoptArgs { recomp_id, program }).await
}

/// The catalog recomps a game added by hand could be.
pub async fn recomp_suggestions(id: &str) -> Result<Vec<cerberus_shared::AdoptFound>, String> {
    call("recomp_suggestions", &Id { id }).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LinkArgs<'a> {
    id: &'a str,
    recomp_id: &'a str,
}

/// Turns a game added by hand into the catalog recomp it is.
pub async fn link_to_recomp(id: &str, recomp_id: &str) -> Result<Game, String> {
    call("link_to_recomp", &LinkArgs { id, recomp_id }).await
}

#[derive(Serialize)]
struct KeyArg {
    key: String,
}

/// Keeps the player's SteamGridDB key once SteamGridDB takes it; empty forgets it.
pub async fn set_steamgriddb_key(key: String) -> Result<SettingsView, String> {
    call("set_steamgriddb_key", &KeyArg { key }).await
}

pub async fn open_steamgriddb_keys() -> Result<(), String> {
    call("open_steamgriddb_keys", &()).await
}

#[derive(Serialize)]
struct TermArg<'a> {
    term: &'a str,
}

pub async fn art_search(term: &str) -> Result<Vec<cerberus_shared::ArtGame>, String> {
    call("art_search", &TermArg { term }).await
}

#[derive(Serialize)]
struct GameArg {
    game: u64,
}

pub async fn art_options(game: u64) -> Result<cerberus_shared::ArtOptions, String> {
    call("art_options", &GameArg { game }).await
}

#[derive(Serialize)]
struct UrlRef<'a> {
    url: &'a str,
}

/// A SteamGridDB picture as a `data:` address, fetched by the backend.
pub async fn art_fetch(url: &str) -> Result<String, String> {
    call("art_fetch", &UrlRef { url }).await
}

/// A picture file the player picks, as a `data:` address; `None` when cancelled.
pub async fn art_pick_file() -> Result<Option<String>, String> {
    call("art_pick_file", &()).await
}

#[derive(Serialize)]
struct ArtSaveArgs<'a> {
    id: &'a str,
    kind: &'a str,
    data: Option<String>,
}

/// Keeps the framed picture for one kind of art; `None` puts the original back.
pub async fn art_save(id: &str, kind: &str, data: Option<String>) -> Result<(), String> {
    call("art_save", &ArtSaveArgs { id, kind, data }).await
}

/// Whether a game can take a picture filter, and the one it has.
pub async fn look_status(id: &str) -> Result<cerberus_shared::LookStatus, String> {
    call("look_status", &Id { id }).await
}

#[derive(Serialize)]
struct LookArgs<'a> {
    id: &'a str,
    look: Option<cerberus_shared::Look>,
}

/// Picks a game's picture filter, or (`None`) turns it off.
pub async fn set_look(id: &str, look: Option<cerberus_shared::Look>) -> Result<cerberus_shared::LookStatus, String> {
    call("set_look", &LookArgs { id, look }).await
}

/// The game's own settings; `None` when the catalog offers none.
pub async fn game_settings(id: &str) -> Result<Option<cerberus_shared::GameSettings>, String> {
    call("game_settings", &Id { id }).await
}

#[derive(Serialize)]
struct SettingArgs<'a> {
    id: &'a str,
    setting: Option<String>,
    choice: Option<usize>,
    value: Option<f64>,
}

/// One setting on a choice, or (both `None`) every setting back on the game's defaults.
pub async fn set_game_setting(id: &str, setting: Option<String>, choice: Option<usize>) -> Result<cerberus_shared::GameSettings, String> {
    call("set_game_setting", &SettingArgs { id, setting, choice, value: None }).await
}

/// Moves a slider setting to a number.
pub async fn set_setting_number(id: &str, setting: String, value: f64) -> Result<cerberus_shared::GameSettings, String> {
    call("set_game_setting", &SettingArgs { id, setting: Some(setting), choice: None, value: Some(value) }).await
}

/// What the launcher did for a game lately, newest first.
/// Marks or unmarks a game as a favorite.
pub async fn set_hidden(id: &str, on: bool) -> Result<(), String> {
    #[derive(serde::Serialize)]
    struct Arg<'a> {
        id: &'a str,
        on: bool,
    }
    call("set_hidden", &Arg { id, on }).await
}

pub async fn list_collections() -> Result<Vec<String>, String> {
    call("list_collections", &()).await
}

#[derive(serde::Serialize)]
struct Name<'a> {
    name: &'a str,
}

pub async fn create_collection(name: &str) -> Result<String, String> {
    call("create_collection", &Name { name }).await
}

pub async fn set_in_collection(name: &str, id: &str, on: bool) -> Result<(), String> {
    #[derive(serde::Serialize)]
    struct Arg<'a> {
        name: &'a str,
        id: &'a str,
        on: bool,
    }
    call("set_in_collection", &Arg { name, id, on }).await
}

pub async fn rename_collection(old: &str, name: &str) -> Result<String, String> {
    #[derive(serde::Serialize)]
    struct Arg<'a> {
        old: &'a str,
        name: &'a str,
    }
    call("rename_collection", &Arg { old, name }).await
}

pub async fn delete_collection(name: &str) -> Result<(), String> {
    call("delete_collection", &Name { name }).await
}

pub async fn set_favorite(id: String, on: bool) -> Result<(), String> {
    #[derive(serde::Serialize)]
    struct Arg {
        id: String,
        on: bool,
    }
    call("set_favorite", &Arg { id, on }).await
}

/// Shows a classic's file in its folder.
pub async fn open_rom_folder(path: String) -> Result<(), String> {
    #[derive(serde::Serialize)]
    struct Arg {
        path: String,
    }
    call("open_rom_folder", &Arg { path }).await
}

pub async fn recent_activity() -> Result<Vec<cerberus_shared::TrailEntry>, String> {
    call("recent_activity", &()).await
}

pub async fn game_trail(id: &str) -> Result<Vec<cerberus_shared::TrailEntry>, String> {
    call("game_trail", &Id { id }).await
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
/// Stops the copy of the player's game going on now.
pub async fn cancel_copy() -> Result<(), String> {
    call("cancel_copy", &()).await
}

/// The paused and unfinished downloads kept from the last run.
pub async fn download_queue() -> Result<Vec<cerberus_shared::InstallProgress>, String> {
    call("download_queue", &()).await
}

/// Puts a recomp waiting in the download line first.
pub async fn download_first(id: &str) -> Result<bool, String> {
    call("download_first", &Id { id }).await
}

pub async fn forget_download(id: String) -> Result<(), String> {
    call("forget_download", &Id { id: &id }).await
}

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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetupArgs {
    start_in_tv: bool,
}

/// Ends the first-run setup with how the player plays.
pub async fn finish_setup(start_in_tv: bool) -> Result<SettingsView, String> {
    call("finish_setup", &SetupArgs { start_in_tv }).await
}

#[derive(Serialize)]
struct OnArg {
    on: bool,
}

#[derive(Serialize)]
struct PageArg {
    page: String,
}

/// The page the desktop window opens on: "home", "library" or "discover".
pub async fn set_autostart(on: bool) -> Result<SettingsView, String> {
    #[derive(serde::Serialize)]
    struct Arg {
        on: bool,
    }
    call("set_autostart", &Arg { on }).await
}

pub async fn set_start_hidden(on: bool) -> Result<SettingsView, String> {
    #[derive(serde::Serialize)]
    struct Arg {
        on: bool,
    }
    call("set_start_hidden", &Arg { on }).await
}

pub async fn list_monitors() -> Result<Vec<cerberus_shared::MonitorView>, String> {
    call("list_monitors", &()).await
}

pub async fn set_language(code: Option<String>) -> Result<SettingsView, String> {
    #[derive(serde::Serialize)]
    struct Arg {
        code: Option<String>,
    }
    call("set_language", &Arg { code }).await
}

pub async fn set_monitor(name: Option<String>) -> Result<SettingsView, String> {
    #[derive(serde::Serialize)]
    struct Arg {
        name: Option<String>,
    }
    call("set_monitor", &Arg { name }).await
}

pub async fn to_chosen_monitor() -> Result<Option<String>, String> {
    call("to_chosen_monitor", &()).await
}

pub async fn back_from_tv() -> Result<(), String> {
    call("back_from_tv", &()).await
}

pub async fn set_close_to_tray(on: bool) -> Result<SettingsView, String> {
    #[derive(serde::Serialize)]
    struct Arg {
        on: bool,
    }
    call("set_close_to_tray", &Arg { on }).await
}

pub async fn set_site_catalog(on: bool) -> Result<SettingsView, String> {
    #[derive(serde::Serialize)]
    struct Arg {
        on: bool,
    }
    call("set_site_catalog", &Arg { on }).await
}

pub async fn set_recomp_catalog(on: bool) -> Result<SettingsView, String> {
    #[derive(serde::Serialize)]
    struct Arg {
        on: bool,
    }
    call("set_recomp_catalog", &Arg { on }).await
}

pub async fn set_start_page(page: String) -> Result<SettingsView, String> {
    call("set_start_page", &PageArg { page }).await
}

/// Shows the first-run setup again next time.
pub async fn rerun_setup() -> Result<SettingsView, String> {
    call("rerun_setup", &()).await
}

#[derive(Serialize)]
struct RaArgs {
    user: String,
    key: String,
    password: Option<String>,
}

pub async fn ra_connect(user: String, key: String, password: Option<String>) -> Result<cerberus_shared::RaProfile, String> {
    call("ra_connect", &RaArgs { user, key, password }).await
}

/// A classic's RetroAchievements game and the player's progress, when there is one.
pub async fn classic_achievements(path: &str, console: &str) -> Result<Option<cerberus_shared::GameAchievements>, String> {
    #[derive(serde::Serialize)]
    struct Arg<'a> {
        path: &'a str,
        console: &'a str,
    }
    call("classic_achievements", &Arg { path, console }).await
}

/// This copy's version (the UI and the app are built together).
pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn on_update_available(handler: impl FnMut(cerberus_shared::AppUpdate) + 'static) {
    on_event("update-available", handler);
}

/// Bytes of the update downloaded so far, and the total.
pub fn on_update_progress(handler: impl FnMut((u64, u64)) + 'static) {
    on_event("update-progress", handler);
}

pub async fn update_check() -> Result<Option<cerberus_shared::AppUpdate>, String> {
    call("update_check", &()).await
}

pub async fn update_found() -> Result<Option<cerberus_shared::AppUpdate>, String> {
    call("update_found", &()).await
}

pub async fn update_install() -> Result<(), String> {
    call("update_install", &()).await
}

pub async fn whats_new() -> Result<Option<cerberus_shared::AppUpdate>, String> {
    call("whats_new", &()).await
}

pub async fn whats_new_seen() -> Result<(), String> {
    call("whats_new_seen", &()).await
}

/// The recomp download limit in MB/s (0 = none) and slowing down while playing.
pub async fn set_download_limit(mb: u32, slow_while_playing: bool) -> Result<cerberus_shared::SettingsView, String> {
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Arg {
        mb: u32,
        slow_while_playing: bool,
    }
    call("set_download_limit", &Arg { mb, slow_while_playing }).await
}

/// Pauses or resumes a recomp's updates.
pub async fn set_updates_paused(id: &str, on: bool) -> Result<(), String> {
    #[derive(serde::Serialize)]
    struct Arg<'a> {
        id: &'a str,
        on: bool,
    }
    call("set_updates_paused", &Arg { id, on }).await
}

/// Status, rating, notes and favorite of a classic (any id outside the library).
pub async fn player_note(id: &str) -> cerberus_shared::PlayerNote {
    #[derive(serde::Serialize)]
    struct Arg<'a> {
        id: &'a str,
    }
    call("player_note", &Arg { id }).await.unwrap_or_default()
}

/// The player's status, rating and notes for a game.
pub async fn set_game_note(id: &str, status: Option<cerberus_shared::GameStatus>, rating: u8, notes: String) -> Result<(), String> {
    #[derive(serde::Serialize)]
    struct Arg<'a> {
        id: &'a str,
        status: Option<cerberus_shared::GameStatus>,
        rating: u8,
        notes: String,
    }
    call("set_game_note", &Arg { id, status, rating, notes }).await
}

/// A game's play sessions, oldest first.
pub async fn game_sessions(id: &str) -> Result<Vec<cerberus_shared::PlaySession>, String> {
    call("game_sessions", &Id { id }).await
}

/// Opens a page about a Steam game in the browser: "store", "steamdb", "pcgw" or "site".
/// The Epic store, or an Xbox game's page in the Microsoft Store.
/// Opens the store's own place to uninstall (or `verify`) one of its games.
pub async fn store_manage(game: &Game, verify: bool) -> Result<(), String> {
    #[derive(serde::Serialize)]
    struct Arg<'a> {
        game: &'a Game,
        verify: bool,
    }
    call("store_manage", &Arg { game, verify }).await
}

pub async fn open_in_store(kind: &str, product: &str) {
    #[derive(serde::Serialize)]
    struct Arg<'a> {
        kind: &'a str,
        product: &'a str,
    }
    let _ = call::<_, ()>("open_in_store", &Arg { kind, product }).await;
}

pub async fn open_store_link(app_id: u32, kind: &str) -> Result<(), String> {
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Arg<'a> {
        app_id: u32,
        kind: &'a str,
    }
    call("open_store_link", &Arg { app_id, kind }).await
}

/// A Steam game's store page: description, pictures, trailers, requirements, languages.
/// An Epic or Xbox game's store page (Steam's when sold there, else the Microsoft Store's).
pub async fn store_page(name: &str, product: &str) -> Result<cerberus_shared::StoreDetails, String> {
    #[derive(serde::Serialize)]
    struct Arg<'a> {
        name: &'a str,
        product: &'a str,
    }
    call::<_, Option<cerberus_shared::StoreDetails>>("store_page", &Arg { name, product }).await?.ok_or_default()
}

trait OkOrDefault<T> {
    fn ok_or_default(self) -> Result<T, String>;
}

impl<T> OkOrDefault<T> for Option<T> {
    /// No page: an empty error (the page shows nothing in its place).
    fn ok_or_default(self) -> Result<T, String> {
        self.ok_or_else(String::new)
    }
}

pub async fn steam_details(app_id: u32) -> Result<cerberus_shared::StoreDetails, String> {
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Arg {
        app_id: u32,
    }
    call("steam_details", &Arg { app_id }).await
}

/// The Steam account on this PC: SteamID64 and name.
pub async fn steam_local_account() -> Result<Option<(String, String)>, String> {
    call("steam_local_account", &()).await
}

pub async fn steam_account() -> Result<Option<cerberus_shared::SteamAccount>, String> {
    call("steam_account", &()).await
}

pub async fn steam_connect(key: String, profile: Option<String>) -> Result<cerberus_shared::SteamAccount, String> {
    #[derive(serde::Serialize)]
    struct Arg {
        key: String,
        profile: Option<String>,
    }
    call("steam_connect", &Arg { key, profile }).await
}

pub async fn steam_disconnect() -> Result<(), String> {
    call("steam_disconnect", &()).await
}

/// "key", "privacy" or "profile".
pub async fn steam_open(page: &str) -> Result<(), String> {
    #[derive(serde::Serialize)]
    struct Arg<'a> {
        page: &'a str,
    }
    call("steam_open", &Arg { page }).await
}

/// `online` off: only what's on this PC, at once; on: asks the accounts too,
/// when due. `force` (the player pressed Update): asks a minute after the last time.
pub async fn achievements_hub(online: bool) -> Result<cerberus_shared::AchievementsHub, String> {
    achievements_hub_now(online, false).await
}

pub async fn achievements_hub_now(online: bool, force: bool) -> Result<cerberus_shared::AchievementsHub, String> {
    #[derive(serde::Serialize)]
    struct Arg {
        online: bool,
        force: bool,
    }
    call("achievements_hub", &Arg { online, force }).await
}

pub async fn game_achievements(source: &str, key: &str) -> Result<cerberus_shared::GameAchievements, String> {
    #[derive(serde::Serialize)]
    struct Arg<'a> {
        source: &'a str,
        key: &'a str,
    }
    call("game_achievements", &Arg { source, key }).await
}

pub async fn ra_profile() -> Result<Option<cerberus_shared::RaProfile>, String> {
    call("ra_profile", &()).await
}

pub async fn ra_disconnect() -> Result<SettingsView, String> {
    call("ra_disconnect", &()).await
}

#[derive(Serialize)]
struct KeysArg {
    keys: bool,
}

/// Opens the player's RetroAchievements profile, or (`keys`) the page with their API key.
pub async fn ra_open(keys: bool) -> Result<(), String> {
    call("ra_open", &KeysArg { keys }).await
}

pub async fn emulation_status() -> Result<cerberus_shared::EmulationView, String> {
    call("emulation_status", &()).await
}

/// Installs RetroArch; progress comes through `on_emu_progress`.
pub async fn install_retroarch() -> Result<cerberus_shared::EmulationView, String> {
    call("install_retroarch", &()).await
}

/// Emulator download progress, while the asking component is on screen.
pub fn on_emu_progress_here(handler: impl FnMut((u64, u64)) + 'static) {
    on_event_here("emu-progress", handler);
}


pub async fn pick_retroarch() -> Result<cerberus_shared::EmulationView, String> {
    call("pick_retroarch", &()).await
}

#[derive(Serialize)]
struct CoreArg {
    core: String,
}

pub async fn install_core(core: String) -> Result<cerberus_shared::EmulationView, String> {
    call("install_core", &CoreArg { core }).await
}

#[derive(Serialize)]
struct ClassicArgs {
    path: String,
    console: String,
    disc: Option<usize>,
}

/// Copies the player's BIOS files from a folder they pick; what was added, said for the player.
pub async fn import_bios() -> Result<(cerberus_shared::EmulationView, String), String> {
    call("import_bios", &()).await
}

/// Starts a classic from the ROM folders in RetroArch.
pub async fn play_classic(path: String, console: String, disc: Option<usize>) -> Result<(), String> {
    call("play_classic", &ClassicArgs { path, console, disc }).await
}

#[derive(Serialize)]
struct PasswordArg {
    password: String,
}

/// Logs RetroArch in to RetroAchievements; the password is traded for a token and not kept.
pub async fn link_retroarch_achievements(password: String) -> Result<cerberus_shared::EmulationView, String> {
    call("link_retroarch_achievements", &PasswordArg { password }).await
}

/// Whether Cerberus opens in the TV screen.
pub async fn set_start_in_tv(on: bool) -> Result<SettingsView, String> {
    call("set_start_in_tv", &OnArg { on }).await
}

pub async fn retry_save_uploads() -> Result<SettingsView, String> {
    call("retry_save_uploads", &()).await
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

pub async fn prepare_options(id: &str) -> Result<Vec<cerberus_shared::PrepareOption>, String> {
    call("prepare_options", &Id { id }).await
}

#[derive(Serialize)]
struct OptionArgs<'a> {
    id: &'a str,
    option: &'a str,
    on: bool,
}

pub async fn set_prepare_option(id: &str, option: &str, on: bool) -> Result<Vec<cerberus_shared::PrepareOption>, String> {
    call("set_prepare_option", &OptionArgs { id, option, on }).await
}

pub async fn open_prepare_license(id: &str, option: &str) -> Result<(), String> {
    call("open_prepare_license", &OptionArgs { id, option, on: true }).await
}

/// Who opens each console's classics on this PC.
pub async fn classic_players() -> Vec<cerberus_shared::ClassicPlayer> {
    call("classic_players", &()).await.unwrap_or_default()
}

pub async fn preparation_pending(id: &str) -> Result<bool, String> {
    call("preparation_pending", &Id { id }).await
}

pub async fn prepare_game(id: &str) -> Result<bool, String> {
    call("prepare_game", &Id { id }).await
}

pub async fn game_actions(id: &str) -> Result<Vec<cerberus_shared::GameAction>, String> {
    call("game_actions", &Id { id }).await
}

#[derive(Serialize)]
struct ActionArgs<'a> {
    id: &'a str,
    action: &'a str,
}

pub async fn run_game_action(id: &str, action: &str) -> Result<(), String> {
    call("run_game_action", &ActionArgs { id, action }).await
}

/// The files besides the copy the game's `recomplauncher.json` asks for.
pub async fn game_inputs(id: &str) -> Result<Vec<cerberus_shared::SetupInput>, String> {
    call("game_inputs", &Id { id }).await
}

#[derive(Serialize)]
struct InputArgs<'a> {
    id: &'a str,
    input: &'a str,
    clear: bool,
}

/// Picks the file for one of them (`clear` takes it away). `None` when cancelled.
pub async fn pick_game_input(id: &str, input: &str, clear: bool) -> Result<Option<Game>, String> {
    call("pick_game_input", &InputArgs { id, input, clear }).await
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

pub async fn tracking_mode(id: &str) -> Result<Option<String>, String> {
    call("tracking_mode", &Id { id }).await
}

pub async fn pick_tracking_exe(id: &str, automatic: bool) -> Result<Option<Game>, String> {
    #[derive(Serialize)]
    struct Pick<'a> { id: &'a str, automatic: bool }
    call("pick_tracking_exe", &Pick { id, automatic }).await
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

pub async fn refresh_source(url: String) -> Result<SettingsView, String> {
    call("refresh_source", &UrlArg { url }).await
}

pub async fn source_listings(url: &str, query: &str, offset: usize) -> Result<cerberus_shared::SourcePage, String> {
    #[derive(Serialize)]
    struct Search<'a> { url: &'a str, query: &'a str, offset: usize }
    call("source_listings", &Search { url, query, offset }).await
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

pub async fn set_in_game(choices: cerberus_shared::InGame) -> Result<cerberus_shared::SettingsView, String> {
    #[derive(serde::Serialize)]
    struct Arg {
        choices: cerberus_shared::InGame,
    }
    call("set_in_game", &Arg { choices }).await
}

pub async fn set_notices(choices: cerberus_shared::Notices) -> Result<cerberus_shared::SettingsView, String> {
    #[derive(serde::Serialize)]
    struct Arg {
        choices: cerberus_shared::Notices,
    }
    call("set_notices", &Arg { choices }).await
}

/// Whether the Epic Games Launcher is on this PC, and how many of its games are installed.
pub async fn epic_status() -> Result<(bool, usize), String> {
    call("epic_status", &()).await
}

pub async fn epic_open() -> Result<(), String> {
    call("epic_open", &()).await
}

/// A copy of the player's data, saved where they choose; how many files (None: cancelled).
pub async fn export_data() -> Result<Option<usize>, String> {
    call("export_data", &()).await
}

/// Restores a copy; Cerberus starts again after it. How many files (None: cancelled).
pub async fn import_data() -> Result<Option<usize>, String> {
    call("import_data", &()).await
}

/// Developers' posts for these Steam games (app id, name), newest first.
pub async fn steam_news(games: Vec<(u32, String)>) -> Result<Vec<cerberus_shared::NewsItem>, String> {
    #[derive(serde::Serialize)]
    struct Arg {
        games: Vec<(u32, String)>,
    }
    call("steam_news", &Arg { games }).await
}

pub async fn open_news(url: String) -> Result<(), String> {
    #[derive(serde::Serialize)]
    struct Arg {
        url: String,
    }
    call("open_news", &Arg { url }).await
}

/// The games played most recently, for the tray icon's menu.
pub async fn set_tray_games(games: Vec<cerberus_shared::Game>) {
    #[derive(serde::Serialize)]
    struct Arg {
        games: Vec<cerberus_shared::Game>,
    }
    let _: Result<(), String> = call("set_tray_games", &Arg { games }).await;
}

/// The original game's Wikipedia summary, in the player's language (None: no article).
pub async fn wiki_about(game: String) -> Result<Option<cerberus_shared::WikiAbout>, String> {
    #[derive(serde::Serialize)]
    struct Arg {
        game: String,
    }
    call("wiki_about", &Arg { game }).await
}

pub async fn open_wiki(url: String) -> Result<(), String> {
    #[derive(serde::Serialize)]
    struct Arg {
        url: String,
    }
    call("open_wiki", &Arg { url }).await
}

/// Puts every installed game that isn't Steam's into Steam; how many went in.
pub async fn add_all_to_steam() -> Result<usize, String> {
    call("add_all_to_steam", &()).await
}

/// The standalone emulators, who plays each console, and their game folders.
pub async fn standalone_status() -> Result<cerberus_shared::StandaloneStatus, String> {
    call("standalone_status", &()).await
}

#[derive(serde::Serialize)]
struct EmulatorId<'a> {
    id: &'a str,
}

/// Installs a standalone emulator; progress comes through `on_emu_progress`.
pub async fn install_standalone(id: &str) -> Result<cerberus_shared::StandaloneStatus, String> {
    call("install_standalone", &EmulatorId { id }).await
}

pub async fn pick_standalone(id: &str) -> Result<cerberus_shared::StandaloneStatus, String> {
    call("pick_standalone", &EmulatorId { id }).await
}

pub async fn remove_standalone(id: &str) -> Result<cerberus_shared::StandaloneStatus, String> {
    call("remove_standalone", &EmulatorId { id }).await
}

pub async fn set_play_with(console: &str, who: &str) -> Result<cerberus_shared::StandaloneStatus, String> {
    #[derive(serde::Serialize)]
    struct Arg<'a> {
        console: &'a str,
        who: &'a str,
    }
    call("set_play_with", &Arg { console, who }).await
}

pub async fn set_emulator_folders(on: bool) -> Result<cerberus_shared::StandaloneStatus, String> {
    #[derive(serde::Serialize)]
    struct Arg {
        on: bool,
    }
    call("set_emulator_folders", &Arg { on }).await
}

/// Opens an emulator by itself, to set it up.
pub async fn open_standalone(id: &str) -> Result<(), String> {
    call("open_standalone", &EmulatorId { id }).await
}

/// The player's BIOS into a standalone emulator (or the PS3 firmware through RPCS3); what was done.
pub async fn standalone_bios(id: &str) -> Result<(cerberus_shared::StandaloneStatus, String), String> {
    call("standalone_bios", &EmulatorId { id }).await
}

/// Every RetroArch core not installed yet; how many came.
pub async fn install_all_cores() -> Result<(cerberus_shared::EmulationView, usize), String> {
    call("install_all_cores", &()).await
}


pub async fn emulator_backups() -> Result<Vec<cerberus_shared::EmulatorBackups>, String> {
    call("emulator_backups", &()).await
}

pub async fn backup_emulator_saves(id: &str) -> Result<Option<cerberus_shared::Backup>, String> {
    call("backup_emulator_saves", &Id { id }).await
}

pub async fn restore_emulator_backup(id: &str, name: &str) -> Result<u32, String> {
    #[derive(serde::Serialize)]
    struct Arg<'a> {
        id: &'a str,
        name: &'a str,
    }
    call("restore_emulator_backup", &Arg { id, name }).await
}

pub async fn open_emulator_backups(id: &str) -> Result<(), String> {
    call("open_emulator_backups", &Id { id }).await
}


pub async fn set_game_pass(on: bool) -> Result<SettingsView, String> {
    #[derive(serde::Serialize)]
    struct Arg {
        on: bool,
    }
    call("set_game_pass", &Arg { on }).await
}

pub async fn xbox_open() -> Result<(), String> {
    call("xbox_open", &()).await
}


pub async fn epic_login() -> Result<(), String> {
    call("epic_login", &()).await
}

pub async fn epic_logout() -> Result<SettingsView, String> {
    call("epic_logout", &()).await
}


pub async fn xbox_login() -> Result<(), String> {
    call("xbox_login", &()).await
}

pub async fn xbox_logout() -> Result<SettingsView, String> {
    call("xbox_logout", &()).await
}
