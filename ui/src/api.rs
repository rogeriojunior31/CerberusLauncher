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

pub async fn pc_status() -> Result<cerberus_shared::PcStatus, String> {
    call("pc_status", &()).await
}

pub async fn install_vc() -> Result<cerberus_shared::PcStatus, String> {
    call("install_vc", &()).await
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
}

pub async fn ra_connect(user: String, key: String) -> Result<cerberus_shared::RaProfile, String> {
    call("ra_connect", &RaArgs { user, key }).await
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

pub fn on_emu_progress(handler: impl FnMut((u64, u64)) + 'static) {
    on_event("emu-progress", handler);
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
}

/// Copies the player's BIOS files from a folder they pick; what was added, said for the player.
pub async fn import_bios() -> Result<(cerberus_shared::EmulationView, String), String> {
    call("import_bios", &()).await
}

/// Starts a classic from the ROM folders in RetroArch.
pub async fn play_classic(path: String, console: String) -> Result<(), String> {
    call("play_classic", &ClassicArgs { path, console }).await
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
