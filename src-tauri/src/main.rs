// Release builds on Windows would otherwise open a console window next to the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod accounts;
mod art_cache;
mod authenticode;
mod emulator_saves;
mod state_file;
mod epic;
mod epic_account;
mod xbox_account;
mod ms_store;
mod xbox;
mod gamepass;
mod disk;
mod iso9660;
mod library;
mod mods;
mod gamebanana;
mod hedge;
mod recomp;
mod sessions;
mod roms;
mod settings;
mod setup_runner;
mod awake;
mod awake_time;
mod background;
mod favorites;
mod game_notes;
mod achievements;
mod language;
mod copying;
mod dlss;
mod downloads;
mod elevated;
mod overlay;
mod in_game;
mod data_backup;
mod unlock_toast;
mod disc_console;
mod disc_image;
mod screenshots;
mod monitors;
mod net;
mod cheevos;
mod collections;
mod notify;
mod crash;
mod store_time;
mod shortcut;
mod runtimes;
mod steam;
mod standalone;
mod steam_shortcuts;
mod steam_store;
mod steam_details;
mod steam_news;
mod wikipedia;
mod trail;
mod self_update;
mod play_log;
mod game_settings;
mod art_custom;
mod looks;
mod retroachievements;
mod emulation;
mod xdvdfs;

use cerberus_shared::tk;
use cerberus_shared::tr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use art_cache::ArtCache;
use cerberus_shared::{Backup, Game, InstallProgress, RecompEntry, SettingsView, Source};
use library::LocalLibrary;
use recomp::sources::{CatalogSource, Entries};
use cerberus_shared::{SourcePage, SourceView};
use tauri::{AppHandle, Emitter, Listener, Manager, State};
use tauri_plugin_dialog::{DialogExt, FileDialogBuilder};
use tauri_plugin_opener::OpenerExt;

/// Where covers the user picked are copied, so they keep working if the original moves.
struct CoverFolder(PathBuf);

/// The app's own data folder: library, settings, backups, caches.
struct DataDir(PathBuf);

impl DataDir {
    fn cache(&self) -> PathBuf {
        self.0.join("cache")
    }

    fn rom_index(&self) -> PathBuf {
        self.cache().join("roms.json")
    }

    /// What each file held at the last scan (see `roms::ScanCache`).
    fn rom_cache(&self) -> PathBuf {
        // v3: regions are read from every name tag now; what v2 said of each file is read again once.
        for old in ["rom-cache.json", "rom-cache-v2.json"] {
            let _ = std::fs::remove_file(self.cache().join(old));
        }
        self.cache().join("rom-cache-v3.json")
    }
}

/// The game files found at the last look through the player's ROM folders.
#[derive(Default)]
struct RomIndex(std::sync::Mutex<Vec<roms::RomFile>>);

impl RomIndex {
    fn get(&self) -> Vec<roms::RomFile> {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    fn set(&self, found: Vec<roms::RomFile>) {
        *self.0.lock().unwrap_or_else(|p| p.into_inner()) = found;
        ROM_GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Bumped each time the ROM index changes.
static ROM_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Recomp id -> the player's copy of its game. Matching the whole catalog
/// against every ROM takes a while; kept until the catalog or the ROMs change.
fn owned_games(app: &AppHandle) -> std::collections::HashMap<String, String> {
    type Kept = (Arc<Vec<recomp::catalog::CatalogEntry>>, u64, std::collections::HashMap<String, String>);
    static KEPT: std::sync::Mutex<Option<Kept>> = std::sync::Mutex::new(None);
    let catalog = recomp::catalog::all();
    let generation = ROM_GENERATION.load(std::sync::atomic::Ordering::SeqCst);
    if let Some((list, seen, owned)) = KEPT.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
        if Arc::ptr_eq(list, &catalog) && *seen == generation {
            return owned.clone();
        }
    }
    let owned = roms::match_catalog(&catalog, &app.state::<RomIndex>().get());
    *KEPT.lock().unwrap_or_else(|p| p.into_inner()) = Some((catalog, generation, owned.clone()));
    owned
}

fn settings_view(app: &AppHandle) -> SettingsView {
    let folders = app.state::<recomp::Folders>();
    let installs = folders.installs();
    let art = app.state::<Arc<ArtCache>>();
    let data = app.state::<DataDir>();
    let chosen = settings::load(&data.0);
    SettingsView {
        install_free_bytes: disk::available(&installs),
        // Recomps download beside the installs when those moved to another drive.
        download_free_bytes: disk::available(&folders.downloads()),
        recomps_dir_is_default: installs == folders.default_installs,
        recomps_dir: installs.to_string_lossy().into_owned(),
        data_dir: data.0.to_string_lossy().into_owned(),
        art_cache_bytes: settings::folder_size_kept(art.dir()),
        version: app.package_info().version.to_string(),
        rom_dirs: chosen.rom_dirs.iter().map(|d| d.to_string_lossy().into_owned()).collect(),
        roms_found: app.state::<RomIndex>().get().len(),
        roms_matched: owned_games(app).len(),
        catalog_count: recomp::catalog::all().len(),
        catalog_source: recomp::catalog::SOURCE_NAME.to_string(),
        pending_uploads: recomp::cloud::pending_count(&folders),
        cloud_dir: chosen.cloud_dir.map(|d| d.to_string_lossy().into_owned()),
        setup_done: chosen.setup_done,
        start_in_tv: chosen.start_in_tv,
        download_limit: chosen.download_limit,
        slow_while_playing: chosen.slow_while_playing,
        in_game: chosen.in_game.clone(),
        notices: chosen.notices.clone(),
        steamgriddb_key: chosen.steamgriddb_key.clone(),
        start_page: chosen.start_page.clone(),
        site_catalog: chosen.site_catalog,
        close_to_tray: chosen.close_to_tray,
        game_pass: chosen.game_pass,
        autostart: {
            use tauri_plugin_autostart::ManagerExt;
            app.autolaunch().is_enabled().unwrap_or(false)
        },
        start_hidden: chosen.start_hidden,
        monitor: chosen.monitor.clone(),
        language: chosen.language.clone(),
        ra_user: chosen.ra_user.clone(),
        epic_user: chosen.epic_user.clone(),
        xbox_user: chosen.xbox_user.clone(),
        ra_retroarch: chosen.ra_token.is_some(),
        roms: {
            let played = emulation::played(&data.0);
            app.state::<RomIndex>()
                .get()
                .into_iter()
                .map(|r| {
                    let path = r.path.to_string_lossy().into_owned();
                    let time = played.get(&path).cloned().unwrap_or_default();
                    cerberus_shared::RomView {
                        path,
                        title: r.title,
                        system: r.system,
                        region: r.region,
                        playtime_secs: time.seconds,
                        last_played: (time.last > 0).then_some(time.last),
                        discs: r.discs.iter().map(|d| d.to_string_lossy().into_owned()).collect(),
                    }
                })
                .collect()
        },
        sources: app.state::<Sources>().list.lock().unwrap_or_else(|p| p.into_inner()).iter().map(|(view, _)| view.clone()).collect(),
    }
}

/// The player's catalog sources, with their games, as last read.
#[derive(Default)]
struct Sources {
    list: std::sync::Mutex<Vec<(SourceView, Option<CatalogSource>)>>,
    // Serialize fetch, publication and removal to keep an older response from replacing a newer one.
    sync: tokio::sync::Mutex<()>,
}

fn publish_sources(app: &AppHandle) {
    let state = app.state::<Sources>();
    let list = state.list.lock().unwrap_or_else(|p| p.into_inner());
    let projects = list.iter().filter_map(|(_, source)| match &source.as_ref()?.entries {
        Entries::Projects(games) | Entries::Hydra { games, .. } => Some(games.clone()),
    }).collect();
    recomp::catalog::set_extras(projects);
    drop(list);
    let _ = app.emit("catalog-changed", ());
}

fn remember_source(app: &AppHandle, url: &str, source: Option<CatalogSource>) {
    let (hydra, games) = match source.as_ref().map(|s| &s.entries) {
        Some(Entries::Projects(items)) => (false, items.len()),
        Some(Entries::Hydra { listings, .. }) => (true, listings.len()),
        None => (false, 0),
    };
    let view = SourceView {
        url: url.to_owned(),
        // Without a name of its own, the site it comes from, not the whole address.
        name: source.as_ref().map(|s| s.name.trim().to_owned()).filter(|s| !s.is_empty()).unwrap_or_else(|| {
            url.split("://").nth(1).and_then(|rest| rest.split('/').next()).filter(|h| !h.is_empty()).unwrap_or(url).to_owned()
        }),
        hydra, games, synced_at: source.as_ref().and_then(|s| s.synced_at),
        error: source.is_none().then(|| tr!("Baixando a lista… Se continuar assim, clique em Atualizar.").to_owned()),
    };
    let state = app.state::<Sources>();
    let mut list = state.list.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(row) = list.iter_mut().find(|(v, _)| v.url == url) {
        *row = (view, source);
    } else {
        list.push((view, source));
    }
}

#[tauri::command(async)]
fn source_listings(sources: State<'_, Sources>, url: String, query: String, offset: usize) -> Result<SourcePage, String> {
    let list = sources.list.lock().unwrap_or_else(|p| p.into_inner());
    let source = list.iter().find(|(v, _)| v.url == url).and_then(|(_, s)| s.as_ref())
        .ok_or(tr!("Fonte indisponível."))?;
    source.listings(&query, offset)
}

#[tauri::command]
async fn add_source(app: AppHandle, http: State<'_, reqwest::Client>, url: String) -> Result<SettingsView, String> {
    let state = app.state::<Sources>();
    let _guard = state.sync.lock().await;
    let url = url.trim().to_string();
    let data = app.state::<DataDir>();
    let source = recomp::sources::fetch(&http, &data.cache(), &url).await?;
    settings::update(&data.0, |s| {
        if !s.sources.contains(&url) {
            s.sources.push(url.clone());
        }
    })?;
    remember_source(&app, &url, Some(source));
    publish_sources(&app);
    Ok(settings_view(&app))
}

async fn sync_source(app: &AppHandle, http: &reqwest::Client, url: &str) {
    match recomp::sources::fetch(http, &app.state::<DataDir>().cache(), url).await {
        Ok(source) => remember_source(app, url, Some(source)),
        Err(e) => {
            let state = app.state::<Sources>();
            let mut list = state.list.lock().unwrap_or_else(|p| p.into_inner());
            if let Some((view, _)) = list.iter_mut().find(|(v, _)| v.url == url) { view.error = Some(e); }
        }
    }
}

#[tauri::command]
async fn refresh_source(app: AppHandle, http: State<'_, reqwest::Client>, url: String) -> Result<SettingsView, String> {
    let state = app.state::<Sources>();
    let _guard = state.sync.lock().await;
    if !settings::load(&app.state::<DataDir>().0).sources.contains(&url) { return Err(tr!("Fonte removida.").into()); }
    sync_source(&app, &http, &url).await;
    publish_sources(&app);
    Ok(settings_view(&app))
}

#[tauri::command]
async fn remove_source(app: AppHandle, url: String) -> Result<SettingsView, String> {
    let state = app.state::<Sources>();
    let _guard = state.sync.lock().await;
    let data = app.state::<DataDir>();
    settings::update(&data.0, |s| s.sources.retain(|u| *u != url))?;
    state.list.lock().unwrap_or_else(|p| p.into_inner()).retain(|(v, _)| v.url != url);
    publish_sources(&app);
    Ok(settings_view(&app))
}

/// Asks for the synced folder (inside OneDrive, Google Drive, Dropbox...) that
/// save backups are copied to. `clear` turns it off instead.
#[tauri::command]
async fn set_cloud_dir(app: AppHandle, clear: bool) -> Result<SettingsView, String> {
    let dir = if clear {
        None
    } else {
        let dialog = app.dialog().file().set_title(tr!("Pasta sincronizada (OneDrive, Google Drive, Dropbox...)"));
        let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_folder());
        let Some(picked) = picked.await.map_err(|e| e.to_string())? else {
            return Ok(settings_view(&app));
        };
        Some(picked.into_path().map_err(|e| e.to_string())?)
    };
    let data_dir = app.state::<DataDir>().0.clone();
    settings::update(&data_dir, |s| s.cloud_dir = dir.clone())?;
    app.state::<recomp::Folders>().set_cloud(dir);
    Ok(settings_view(&app))
}

#[tauri::command]
async fn retry_save_uploads(app: AppHandle) -> Result<SettingsView, String> {
    let worker = app.clone();
    tauri::async_runtime::spawn_blocking(move || recomp::cloud::retry(&worker.state::<recomp::Folders>()))
        .await.map_err(|e| e.to_string())??;
    Ok(settings_view(&app))
}

/// The folders the classics come from: the player's, plus (unless turned off)
/// the ones their emulators already list games from.
fn classic_folders(data: &Path) -> Vec<PathBuf> {
    let chosen = settings::load(data);
    let mut dirs = chosen.rom_dirs.clone();
    if chosen.emulator_folders {
        let retroarch = emulation::retroarch(data, chosen.retroarch.as_deref());
        for folder in standalone::game_folders(data, &chosen.emulators, retroarch.as_deref()) {
            // An emulator's "last folder browsed" can be C:\ itself: not a ROM folder.
            if roms::too_wide(&folder) {
                continue;
            }
            if !dirs.iter().any(|d| folder.starts_with(d)) {
                dirs.push(folder);
            }
        }
    }
    dirs
}

/// Looks through the ROM folders again, off the UI's thread, and tells the window.
async fn rescan_roms(app: &AppHandle) -> Result<(), String> {
    // Only the scan that started last says what's there: an older, slower one
    // (the one at start) finishing later doesn't put back a list without a new folder.
    static LATEST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    static CACHE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let mine = LATEST.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    let data = app.state::<DataDir>().0.clone();
    let cache_file = app.state::<DataDir>().rom_cache();
    let before = app.state::<RomIndex>().get();
    // The folders (settings, and paths that may be on a drive that's away) and
    // the scan itself touch the disk: both off the async runtime.
    let scan = tauri::async_runtime::spawn_blocking(move || {
        let dirs = classic_folders(&data);
        let _one_at_a_time = CACHE.lock().unwrap_or_else(|p| p.into_inner());
        let mut cache = roms::load_cache(&cache_file);
        let found = roms::scan_cached(&dirs, &mut cache);
        roms::save_cache(&cache_file, &cache);
        // A drive unplugged now doesn't take its classics off the list.
        let found = roms::keep_unreachable(found, &before, &dirs);
        // One that moved (a folder to another drive) keeps its time, sessions and note.
        for (old, new) in roms::moved(&before, &found) {
            emulation::follow_moved(&data, &old, &new);
        }
        found
    });
    // A scan that fell over keeps the list from before, instead of an empty one.
    let Ok(found) = scan.await else {
        return Err(tr!("A busca nas pastas de ROMs falhou; a lista de antes foi mantida.").into());
    };
    if LATEST.load(std::sync::atomic::Ordering::SeqCst) != mine {
        return Ok(());
    }
    roms::save_index(&app.state::<DataDir>().rom_index(), &found);
    app.state::<RomIndex>().set(found);
    let _ = app.emit("catalog-changed", ());
    if roms::last_scan_stopped_early() {
        return Err(tr!("A busca parou depois de 300 mil arquivos: uma das pastas é grande demais (um disco inteiro?). Os jogos achados até ali estão na lista; escolha pastas menores para ver todos.").into());
    }
    Ok(())
}

/// Asks for a ROM folder to look through for games that have a recomp.
#[tauri::command]
async fn add_rom_dir(app: AppHandle) -> Result<SettingsView, String> {
    let dialog = app.dialog().file().set_title(tr!("Pasta com suas ROMs ou imagens de disco"));
    let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_folder());
    let Some(picked) = picked.await.map_err(|e| e.to_string())? else {
        return Ok(settings_view(&app));
    };
    let dir = picked.into_path().map_err(|e| e.to_string())?;
    let data_dir = app.state::<DataDir>().0.clone();
    settings::update(&data_dir, |s| {
        if !s.rom_dirs.contains(&dir) {
            s.rom_dirs.push(dir);
        }
    })?;
    // Said when it failed or stopped early, instead of "checked".
    rescan_roms(&app).await?;
    Ok(settings_view(&app))
}

#[tauri::command]
async fn remove_rom_dir(app: AppHandle, path: String) -> Result<SettingsView, String> {
    let data_dir = app.state::<DataDir>().0.clone();
    settings::update(&data_dir, |s| s.rom_dirs.retain(|d| d.to_string_lossy() != path))?;
    // Said when it failed or stopped early, instead of "checked".
    rescan_roms(&app).await?;
    Ok(settings_view(&app))
}

#[tauri::command]
async fn scan_roms(app: AppHandle) -> Result<SettingsView, String> {
    // Said when it failed or stopped early, instead of "checked".
    rescan_roms(&app).await?;
    Ok(settings_view(&app))
}

/// Asks the catalog's site for its newest list now, whatever the copy's age.
#[tauri::command]
async fn refresh_catalog(app: AppHandle, http: State<'_, reqwest::Client>) -> Result<SettingsView, String> {
    let cache = app.state::<DataDir>().cache();
    recomp::catalog::refresh(&http, &cache, true).await?;
    let state = app.state::<Sources>();
    let _guard = state.sync.lock().await;
    for url in settings::load(&app.state::<DataDir>().0).sources {
        sync_source(&app, &http, &url).await;
    }
    publish_sources(&app);
    Ok(settings_view(&app))
}

fn steamgriddb_key(app: &AppHandle) -> Result<String, String> {
    settings::load(&app.state::<DataDir>().0)
        .steamgriddb_key
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| tr!("Coloque sua chave do SteamGridDB primeiro (é gratuita).").to_string())
}

/// Keeps the player's SteamGridDB key after checking SteamGridDB takes it;
/// an empty key forgets it.
#[tauri::command]
async fn set_steamgriddb_key(app: AppHandle, http: State<'_, reqwest::Client>, key: String) -> Result<SettingsView, String> {
    let key = key.trim().to_string();
    if !key.is_empty() {
        art_custom::search(&http, &key, "Mario").await?;
    }
    let data = app.state::<DataDir>();
    settings::update(&data.0, |s| s.steamgriddb_key = (!key.is_empty()).then_some(key))?;
    Ok(settings_view(&app))
}

#[tauri::command]
async fn art_search(app: AppHandle, http: State<'_, reqwest::Client>, term: String) -> Result<Vec<cerberus_shared::ArtGame>, String> {
    let key = steamgriddb_key(&app)?;
    art_custom::search(&http, &key, &term).await
}

#[tauri::command]
async fn art_options(app: AppHandle, http: State<'_, reqwest::Client>, game: u64) -> Result<cerberus_shared::ArtOptions, String> {
    let key = steamgriddb_key(&app)?;
    art_custom::options(&http, &key, game).await
}

/// A SteamGridDB picture as a `data:` address the window can frame.
#[tauri::command]
async fn art_fetch(http: State<'_, reqwest::Client>, url: String) -> Result<String, String> {
    art_custom::fetch(&http, &url).await
}

/// A picture file the player picks, as a `data:` address. `None` when cancelled.
#[tauri::command]
async fn art_pick_file(app: AppHandle) -> Result<Option<String>, String> {
    let dialog = app.dialog().file().set_title(tr!("Escolha uma imagem")).add_filter(tr!("Imagens"), &["png", "jpg", "jpeg", "webp"]);
    let Some(path) = pick_one(dialog).await? else {
        return Ok(None);
    };
    let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    art_custom::data_url(&bytes).map(Some)
}

/// Keeps the framed picture for one kind of a game's art (`data: None` puts the
/// game's own art back). The window reloads the library to show it.
#[tauri::command(async)]
fn art_save(art: State<'_, art_custom::CustomArt>, id: String, kind: String, data: Option<String>) -> Result<(), String> {
    art.set(&id, &kind, data.as_deref())
}

/// The program a game's picture comes from: the one picked to be watched, else the one started.
fn drawing_program(game: &Game) -> Option<PathBuf> {
    if let Some(tracked) = &game.tracking_exe {
        return Some(PathBuf::from(tracked));
    }
    match &game.source {
        Source::Recomp { exe, .. } | Source::Local { exe } if !game.setup_pending => Some(PathBuf::from(exe)),
        _ => None,
    }
}

/// Whether a game can take a picture filter, and the one it has. Reads the
/// game's program, so off the main thread.
#[tauri::command(async)]
fn look_status(library: State<'_, LocalLibrary>, id: String) -> Result<cerberus_shared::LookStatus, String> {
    let game = library.get(&id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    let unsupported = match drawing_program(&game) {
        None => Some(tr!("Filtros funcionam em jogos que o Cerberus abre (recomps e jogos adicionados à mão).").to_string()),
        Some(exe) => match (recomp::look_rule(&game), looks::route(&exe)) {
            (Some(Err(why)), _) => Some(why),
            (Some(Ok(())), _) | (None, looks::Route::Dxgi { .. }) => None,
            (None, looks::Route::Unsupported(why)) => Some(why),
        },
    };
    Ok(cerberus_shared::LookStatus { unsupported, current: game.look })
}

/// Picks a game's picture filter (`None` turns it off). The first one downloads ReShade.
#[tauri::command]
async fn set_look(
    app: AppHandle,
    library: State<'_, LocalLibrary>,
    http: State<'_, reqwest::Client>,
    id: String,
    look: Option<cerberus_shared::Look>,
) -> Result<cerberus_shared::LookStatus, String> {
    if let Some(look) = &look {
        if !looks::LOOKS.iter().any(|(known, _, _)| *known == look.id) {
            return Err(tr!("Esse filtro não existe.").into());
        }
        let data = app.state::<DataDir>().0.clone();
        looks::ensure(&http, &data).await?;
    }
    library.update(&id, |game| game.look = look.clone())?;
    look_status(library, id)
}

/// Ends the first-run setup with how the player plays (desk or TV).
#[tauri::command(async)]
fn finish_setup(app: AppHandle, start_in_tv: bool) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>();
    settings::update(&data.0, |s| {
        s.setup_done = true;
        s.start_in_tv = start_in_tv;
    })?;
    Ok(settings_view(&app))
}

fn retroarch_choice(app: &AppHandle) -> (PathBuf, Option<PathBuf>) {
    let data = app.state::<DataDir>().0.clone();
    let picked = settings::load(&data).retroarch;
    (data, picked)
}

#[tauri::command(async)]
fn emulation_status(app: AppHandle) -> cerberus_shared::EmulationView {
    let (data, picked) = retroarch_choice(&app);
    emulation::status(&data, picked.as_deref())
}

/// Installs RetroArch from libretro's site; progress arrives as `emu-progress` events.
#[tauri::command]
async fn install_retroarch(app: AppHandle, http: State<'_, reqwest::Client>) -> Result<cerberus_shared::EmulationView, String> {
    let (data, _) = retroarch_choice(&app);
    let report = {
        let app = app.clone();
        move |done: u64, total: u64| {
            let _ = app.emit("emu-progress", (done, total));
        }
    };
    let result = emulation::install(&http, &data, report).await;
    trail::note(&app, "emulation", tk!("Instalar RetroArch"), &result.as_ref().map(|p| p.display().to_string()).map_err(Clone::clone));
    result?;
    // Cerberus's own copy wins over a pick that no longer exists.
    settings::update(&data, |s| s.retroarch = None)?;
    Ok(emulation::status(&data, None))
}

/// Uses a RetroArch the player already has.
#[tauri::command]
async fn pick_retroarch(app: AppHandle) -> Result<cerberus_shared::EmulationView, String> {
    let dialog = app.dialog().file().set_title(tr!("Escolha o retroarch.exe")).add_filter("RetroArch", &["exe"]);
    let (data, picked) = retroarch_choice(&app);
    let Some(exe) = pick_one(dialog).await? else {
        return Ok(emulation::status(&data, picked.as_deref()));
    };
    let name = exe.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    if name != "retroarch.exe" {
        return Err(tr!("Escolha o programa retroarch.exe.").into());
    }
    settings::update(&data, |s| s.retroarch = Some(exe.clone()))?;
    Ok(emulation::status(&data, Some(&exe)))
}

#[tauri::command]
async fn install_core(app: AppHandle, http: State<'_, reqwest::Client>, core: String) -> Result<cerberus_shared::EmulationView, String> {
    let (data, picked) = retroarch_choice(&app);
    let exe = emulation::retroarch(&data, picked.as_deref()).ok_or(tr!("Instale ou escolha o RetroArch primeiro."))?;
    emulation::install_core(&http, &exe, &core).await?;
    Ok(emulation::status(&data, picked.as_deref()))
}

/// Copies the player's own console BIOS files into RetroArch from a folder they pick.
#[tauri::command]
async fn import_bios(app: AppHandle) -> Result<(cerberus_shared::EmulationView, String), String> {
    let (data, picked) = retroarch_choice(&app);
    let exe = emulation::retroarch(&data, picked.as_deref()).ok_or(tr!("Instale ou escolha o RetroArch primeiro."))?;
    let dialog = app.dialog().file().set_title(tr!("Pasta com a BIOS do seu PlayStation ou PlayStation 2"));
    let Some(folder) = pick_folder(dialog).await? else {
        return Ok((emulation::status(&data, picked.as_deref()), String::new()));
    };
    let (ps1, ps2) = emulation::import_bios(&exe, &folder)?;
    let said = match (ps1, ps2) {
        (0, n) => tr!("{} BIOS de PlayStation 2 adicionada(s).", n),
        (n, 0) => tr!("{} BIOS de PlayStation adicionada(s).", n),
        (a, b) => tr!("{} BIOS de PlayStation e {} de PlayStation 2 adicionadas.", a, b),
    };
    Ok((emulation::status(&data, picked.as_deref()), said))
}

/// Starts a classic from the ROM folders in RetroArch.
#[tauri::command(async)]
/// `disc`: one disc of a game on several, for emulators that don't swap discs
/// from a list (PCSX2, for one): the player opens the one the game asks for.
fn play_classic(app: AppHandle, path: String, console: String, disc: Option<usize>) -> Result<(), String> {
    // Two at once would each change and put back the same emulator settings.
    if background::classic_running() {
        return Err(tr!("Um clássico já está aberto. Feche-o antes de abrir outro.").into());
    }
    let (data, picked) = retroarch_choice(&app);
    let rom = PathBuf::from(&path);
    // Only files from the player's own ROM folders are started.
    let Some(known) = app.state::<RomIndex>().get().into_iter().find(|r| r.path == rom) else {
        return Err(tr!("Esse arquivo não está nas suas pastas de ROMs.").into());
    };
    let (rom, discs) = match disc.and_then(|n| known.discs.get(n)) {
        Some(one) => (one.clone(), Vec::new()),
        None => (rom, known.discs.clone()),
    };
    // What the scan read from the file itself (cartridge type, the disc's own
    // header) wins over what the page guessed from folder names.
    let console = console_for(&known.system, console);
    let chosen = settings::load(&data);
    let retroarch = emulation::retroarch(&data, picked.as_deref());
    if let Some((emulator, exe)) = standalone_for(&data, &chosen, &console, retroarch.is_some()) {
        // DuckStation and Dolphin swap discs from a list too: all of them, not disc 1 alone.
        let rom = match emulator.id {
            "duckstation" | "dolphin" => emulation::disc_list(&data, &rom, &discs, &console)?,
            _ => rom,
        };
        return play_standalone(app, path, rom, emulator, &exe);
    }
    let exe = retroarch.ok_or(tr!("Instale o RetroArch, ou um emulador para esse console, em Configurações › Emulação para jogar os clássicos."))?;
    // A console nothing here emulates is a known limit, not a failure: refused without a line in the history.
    if emulation::core_for(&console).is_none() {
        return Err(tr!("Esse console ainda não tem emulação no Cerberus.").into());
    }
    let log = data.join("emulators").join("retroarch-session.log");
    connect_retroarch(&data, &exe);
    let mut session = cheevos::prepare(&data, "retroarch", &exe, None, Some(&log));
    session.from = 0;
    let id = format!("classic:{path}");
    let result = emulation::disc_list(&data, &rom, &discs, &console).and_then(|opened| {
        // A save made from disc 1 on its own comes along to the disc list's name.
        if let Some(first) = discs.first().filter(|_| opened != rom) {
            for copy in emulation::carry_disc_saves(&exe, first, &opened) {
                trail::note(&app, &id, tk!("Saves"), &Ok(tr!("Save do disco 1 copiado para {}", copy.display())));
            }
        }
        emulation::launch(&exe, &opened, &console, Some(&log))
    });
    trail::note(&app, &id, tk!("Jogar clássico"), &result.as_ref().map(|_| tr!("Aberto no RetroArch").to_string()).map_err(Clone::clone));
    let child = match result {
        Ok(child) => child,
        Err(e) => {
            cheevos::finish(&data, &session);
            return Err(e);
        }
    };
    cheevos::started(&data, child.id());
    watch_classic(app, path, child, session, ("retroarch", exe.clone()));
    Ok(())
}

/// While a classic runs: its achievements, from the emulator's log, shown by
/// Cerberus alone. When it closes: the time played is kept and the emulator's
/// settings go back to what the player had.
fn watch_classic(app: AppHandle, path: String, mut child: std::process::Child, session: cheevos::Session, emulator: (&'static str, PathBuf)) {
    let data = app.state::<DataDir>().0.clone();
    let id = format!("classic:{path}");
    // The awake clock: the PC sleeping with the classic open isn't play time.
    let started = awake_time::AwakeInstant::now();
    background::classic_started();
    std::thread::spawn(move || {
        let mut at = session.from;
        // Written down every minute, so a Cerberus that closes (tray, update,
        // Windows shutting down) mid-game keeps all but the last minute.
        let mut credited = 0u64;
        let mut last_credit = std::time::Instant::now();
        let unix_now = || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default();
        loop {
            let done = child.try_wait().ok().flatten().is_some();
            if !done && last_credit.elapsed() >= Duration::from_secs(60) {
                let so_far = started.elapsed().as_secs();
                if emulation::add_played(&data, &path, so_far - credited, unix_now()).is_ok() {
                    credited = so_far;
                }
                last_credit = std::time::Instant::now();
            }
            if let (Some(log), Some(format)) = (&session.log, session.format) {
                let (lines, next) = cheevos::read_new(log, at);
                at = next;
                for unlock in lines.iter().filter_map(|l| cheevos::parse(format, l)) {
                    trail::note(&app, &id, tk!("Conquista"), &Ok(unlock.title.clone()));
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move { announce(&app, unlock).await });
                }
            }
            if done {
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        cheevos::finish(&data, &session);
        let seconds = started.elapsed().as_secs();
        let now = unix_now();
        let saved = emulation::add_played(&data, &path, seconds.saturating_sub(credited), now);
        play_log::record(&data, &id, seconds, now);
        trail::note(&app, &id, tk!("Partida"), &saved.map(|_| if seconds < 60 { tr!("Menos de 1 min jogado").to_string() } else { sessions::played_text(seconds / 60 * 60) }));
        // The emulator's saves (memory cards, states), copied as the recomps' are.
        let (emulator_id, emulator_exe) = &emulator;
        let copied = emulator_saves::backup(&data, emulator_id, emulator_exe, tk!("Depois de jogar"));
        trail::note(&app, &id, tk!("Backup depois de jogar"), &sessions::backup_outcome(&copied));
        let _ = app.emit("catalog-changed", ());
        background::classic_ended(&app);
    });
}

/// Cerberus's notice for an achievement unlocked in an emulator: with its
/// picture and description from the game's list on RetroAchievements when
/// they come quickly, with just its name otherwise.
async fn announce(app: &AppHandle, unlock: cheevos::Unlock) {
    let http = app.state::<reqwest::Client>().inner().clone();
    let data = app.state::<DataDir>().0.clone();
    let account = retro_account(app);
    let details = async {
        let (user, key) = account.clone()?;
        let game = match unlock.game {
            Some(g) => g,
            // RetroArch's log doesn't say the game: RetroAchievements does.
            None => {
                let v = retroachievements::get(&http, "API_GetAchievementUnlocks.php", &user, &key, &format!("&a={}&c=1", unlock.id)).await.ok()?;
                v.pointer("/Game/ID").and_then(|g| g.as_u64().or_else(|| g.as_str()?.parse().ok()))?
            }
        };
        let list = achievements::game(&http, &data, cerberus_shared::AchievementSource::Retro, &game.to_string(), Some((user, key)), None).await.ok()?;
        let all_done = list.achievements.iter().all(|a| a.unlocked_at.is_some() || a.id == unlock.id.to_string());
        let found = list.achievements.into_iter().find(|a| a.id == unlock.id.to_string())?;
        Some((found, all_done))
    };
    let details = tokio::time::timeout(Duration::from_secs(4), details).await.ok().flatten();
    let notice = match details {
        Some((a, all_done)) => cerberus_shared::UnlockNotice {
            title: if all_done { tr!("Jogo completo!").to_string() } else { tr!("Conquista desbloqueada").to_string() },
            name: a.name,
            description: a.description,
            icon: a.icon,
            kind: if all_done {
                cerberus_shared::UnlockKind::Complete
            } else if a.rarity.is_some_and(|r| r < 10.0) {
                cerberus_shared::UnlockKind::Rare
            } else {
                cerberus_shared::UnlockKind::Normal
            },
        },
        None => cerberus_shared::UnlockNotice {
            title: tr!("Conquista desbloqueada").to_string(),
            name: unlock.title,
            description: String::new(),
            icon: None,
            kind: cerberus_shared::UnlockKind::Normal,
        },
    };
    unlock_toast::show(app, notice);
}

/// The standalone emulator that opens a console's games, when it's not RetroArch:
/// the one the player chose, or the only way to play a console RetroArch can't.
fn standalone_for(data: &Path, chosen: &settings::Settings, console: &str, have_retroarch: bool) -> Option<(&'static standalone::Emulator, PathBuf)> {
    let found = |e: &'static standalone::Emulator| standalone::locate(data, chosen.emulators.get(e.id).map(PathBuf::as_path), e).map(|(p, _)| (e, p));
    match chosen.play_with.get(console).map(String::as_str) {
        Some("retroarch") => None,
        Some(id) => standalone::find(id).filter(|e| e.consoles.contains(&console)).and_then(found),
        None if have_retroarch && emulation::core_for(console).is_some() => None,
        None => standalone::for_console(console).find_map(found),
    }
}

/// Who opens each console's classics, the way `play_classic` decides it, so a
/// page never offers a Play that would fail.
#[tauri::command(async)]
fn classic_players(app: AppHandle) -> Vec<cerberus_shared::ClassicPlayer> {
    let (data, picked) = retroarch_choice(&app);
    let chosen = settings::load(&data);
    let retroarch = emulation::retroarch(&data, picked.as_deref());
    let mut consoles: Vec<&str> = emulation::CORES.iter().map(|(c, _, _)| *c).collect();
    for emulator in standalone::ALL {
        consoles.extend(emulator.consoles.iter().copied());
    }
    consoles.sort_unstable();
    consoles.dedup();
    consoles
        .into_iter()
        .map(|console| {
            let (name, problem) = if let Some((emulator, _)) = standalone_for(&data, &chosen, console, retroarch.is_some()) {
                (Some(emulator.name.to_string()), None)
            } else {
                match (&retroarch, emulation::core_for(console)) {
                    (Some(exe), Some(_)) if emulation::has_core(exe, console) => (Some("RetroArch".to_string()), None),
                    (Some(_), Some(_)) => (Some("RetroArch".to_string()), Some(tr!("Falta o núcleo desse console no RetroArch. Instale em Configurações › Emulação.").to_string())),
                    (None, Some(_)) => (None, Some(tr!("Instale o RetroArch, ou um emulador para esse console, em Configurações › Emulação.").to_string())),
                    _ => (None, Some(tr!("Esse console ainda não tem emulação no Cerberus.").to_string())),
                }
            };
            cerberus_shared::ClassicPlayer { console: console.to_string(), name, problem }
        })
        .collect()
}

/// Plays a classic in a standalone emulator, with achievements and play time as with RetroArch.
fn play_standalone(app: AppHandle, path: String, rom: PathBuf, emulator: &'static standalone::Emulator, exe: &Path) -> Result<(), String> {
    let data = app.state::<DataDir>().0.clone();
    let chosen = settings::load(&data);
    let account = chosen.ra_user.as_deref().zip(chosen.ra_token.as_deref()).map(|(user, token)| cheevos::Account { user, token });
    cheevos::skip_first_run_wizard(emulator.id, exe);
    let session = cheevos::prepare(&data, emulator.id, exe, account, None);
    let id = format!("classic:{path}");
    let result = standalone::launch(exe, emulator, &rom, &session.args);
    trail::note(&app, &id, tk!("Jogar clássico"), &result.as_ref().map(|_| tr!("Aberto no {}", emulator.name)).map_err(Clone::clone));
    let child = match result {
        Ok(child) => child,
        Err(e) => {
            cheevos::finish(&data, &session);
            return Err(e);
        }
    };
    cheevos::started(&data, child.id());
    watch_classic(app, path, child, session, (emulator.id, exe.to_path_buf()));
    Ok(())
}

fn standalone_status_now(data: &Path) -> cerberus_shared::StandaloneStatus {
    let chosen = settings::load(data);
    let consoles = ["ps1", "ps2", "ps3", "psp", "gc", "wii", "wiiu", "xbox", "x360"];
    let retroarch = emulation::retroarch(data, chosen.retroarch.as_deref()).is_some();
    let play_with = consoles
        .iter()
        .map(|c| {
            let who = match standalone_for(data, &chosen, c, retroarch) {
                Some((e, _)) => e.id.to_string(),
                None if emulation::core_for(c).is_some() => "retroarch".to_string(),
                None => String::new(),
            };
            (c.to_string(), chosen.play_with.get(*c).cloned().unwrap_or(who))
        })
        .collect();
    cerberus_shared::StandaloneStatus {
        emulators: standalone::status(data, &chosen.emulators),
        play_with,
        use_folders: chosen.emulator_folders,
        folders: if chosen.emulator_folders {
            let retroarch = emulation::retroarch(data, chosen.retroarch.as_deref());
            standalone::game_folders(data, &chosen.emulators, retroarch.as_deref()).iter().map(|p| p.display().to_string()).collect()
        } else {
            Vec::new()
        },
    }
}

/// The standalone emulators: found or not, who plays each console, their game folders.
#[tauri::command]
async fn standalone_status(app: AppHandle) -> cerberus_shared::StandaloneStatus {
    let data = app.state::<DataDir>().0.clone();
    tauri::async_runtime::spawn_blocking(move || standalone_status_now(&data)).await.unwrap_or_default()
}

/// Installs a standalone emulator from its official release; progress as `emu-progress`.
#[tauri::command]
async fn install_standalone(app: AppHandle, http: State<'_, reqwest::Client>, id: String) -> Result<cerberus_shared::StandaloneStatus, String> {
    let emulator = standalone::find(&id).ok_or(tr!("Emulador desconhecido."))?;
    let data = app.state::<DataDir>().0.clone();
    let report = {
        let app = app.clone();
        move |done: u64, total: u64| {
            let _ = app.emit("emu-progress", (done, total));
        }
    };
    let result = standalone::install(&http, &data, emulator, report).await;
    trail::note(&app, "emulation", tk!("Instalar emulador"), &result.as_ref().map(|p| format!("{} · {}", emulator.name, p.display())).map_err(Clone::clone));
    result?;
    // Cerberus's own copy wins over a pick made before.
    settings::update(&data, |s| s.emulators.remove(&id))?;
    let _ = rescan_roms(&app).await;
    Ok(standalone_status_now(&data))
}

/// Uses a standalone emulator the player already has.
#[tauri::command]
async fn pick_standalone(app: AppHandle, id: String) -> Result<cerberus_shared::StandaloneStatus, String> {
    let emulator = standalone::find(&id).ok_or(tr!("Emulador desconhecido."))?;
    let dialog = app.dialog().file().set_title(tr!("Programa do {}", emulator.name)).add_filter(emulator.name, &["exe"]);
    let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_file()).await.map_err(|e| e.to_string())?;
    let data = app.state::<DataDir>().0.clone();
    let Some(picked) = picked else { return Ok(standalone_status_now(&data)) };
    let exe = picked.into_path().map_err(|e| e.to_string())?;
    let name = exe.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if !emulator.exe.iter().any(|w| name.eq_ignore_ascii_case(w)) {
        return Err(tr!("Esse não parece ser o programa do {} (esperado: {}).", emulator.name, emulator.exe[0]));
    }
    settings::update(&data, |s| s.emulators.insert(id, exe))?;
    let _ = rescan_roms(&app).await;
    Ok(standalone_status_now(&data))
}

/// Removes the copy Cerberus installed (saves and settings inside it go too), or forgets a pick.
#[tauri::command]
async fn remove_standalone(app: AppHandle, id: String) -> Result<cerberus_shared::StandaloneStatus, String> {
    let emulator = standalone::find(&id).ok_or(tr!("Emulador desconhecido."))?;
    let data = app.state::<DataDir>().0.clone();
    // The files are moved before the settings change (and outside their lock).
    if !settings::load(&data).emulators.contains_key(&id) {
        // Saves, BIOS and settings stay aside, and come back if it's installed again.
        standalone::remove_keeping(&data, emulator)?;
    }
    settings::update(&data, |s| {
        s.emulators.remove(&id);
        s.play_with.retain(|_, who| who != &id);
    })?;
    let _ = rescan_roms(&app).await;
    Ok(standalone_status_now(&data))
}

/// Opens an emulator on its own, to set controls, video or its BIOS folder.
#[tauri::command(async)]
fn open_standalone(app: AppHandle, id: String) -> Result<(), String> {
    let emulator = standalone::find(&id).ok_or(tr!("Emulador desconhecido."))?;
    let data = app.state::<DataDir>().0.clone();
    let chosen = settings::load(&data);
    let (exe, _) = standalone::locate(&data, chosen.emulators.get(&id).map(PathBuf::as_path), emulator).ok_or(tr!("O {} não está instalado.", emulator.name))?;
    let mut command = std::process::Command::new(&exe);
    if let Some(dir) = exe.parent() {
        command.current_dir(dir);
    }
    command.spawn().map(|_| ()).map_err(|e| tr!("Não foi possível abrir o {}: {}", emulator.name, e))
}

/// The emulator's program, found or picked.
fn standalone_exe(data: &Path, id: &str) -> Result<(&'static standalone::Emulator, PathBuf), String> {
    let emulator = standalone::find(id).ok_or(tr!("Emulador desconhecido."))?;
    let chosen = settings::load(data);
    let (exe, _) = standalone::locate(data, chosen.emulators.get(id).map(PathBuf::as_path), emulator).ok_or(tr!("O {} não está instalado.", emulator.name))?;
    Ok((emulator, exe))
}

/// Copies the player's own BIOS into a standalone emulator, or (RPCS3) has it install their PS3 firmware file.
#[tauri::command]
async fn standalone_bios(app: AppHandle, id: String) -> Result<(cerberus_shared::StandaloneStatus, String), String> {
    let data = app.state::<DataDir>().0.clone();
    let (emulator, exe) = standalone_exe(&data, &id)?;
    if standalone::needs(&id) == Some(standalone::Needs::Ps3Firmware) {
        let dialog = app.dialog().file().set_title(tr!("O arquivo de firmware do seu PS3 (PS3UPDAT.PUP)")).add_filter("PS3UPDAT.PUP", &["pup", "PUP"]);
        let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_file()).await.map_err(|e| e.to_string())?;
        let Some(picked) = picked else { return Ok((standalone_status_now(&data), String::new())) };
        standalone::install_firmware(&exe, &picked.into_path().map_err(|e| e.to_string())?)?;
        return Ok((standalone_status_now(&data), tr!("O RPCS3 abriu para instalar o firmware. Quando ele terminar, feche a janela dele.").to_string()));
    }
    let dialog = app.dialog().file().set_title(tr!("Pasta com a BIOS do seu console"));
    let Some(folder) = pick_folder(dialog).await? else { return Ok((standalone_status_now(&data), String::new())) };
    let copied = standalone::import_bios(&id, &exe, &folder)?;
    Ok((standalone_status_now(&data), tr!("{} BIOS adicionada(s) ao {}.", copied, emulator.name)))
}

/// Installs every RetroArch core Cerberus knows that isn't there yet; how many.
#[tauri::command]
async fn install_all_cores(app: AppHandle, http: State<'_, reqwest::Client>) -> Result<(cerberus_shared::EmulationView, usize), String> {
    let (data, picked) = retroarch_choice(&app);
    let exe = emulation::retroarch(&data, picked.as_deref()).ok_or(tr!("Instale ou escolha o RetroArch primeiro."))?;
    let mut missing: Vec<String> = emulation::status(&data, picked.as_deref()).cores.into_iter().filter(|c| !c.installed).map(|c| c.core).collect();
    missing.dedup();
    let mut done = 0;
    for (i, core) in missing.iter().enumerate() {
        let _ = app.emit("emu-progress", (i as u64, missing.len() as u64));
        emulation::install_core(&http, &exe, core).await?;
        done += 1;
    }
    Ok((emulation::status(&data, picked.as_deref()), done))
}

/// Which emulator opens a console's games: "retroarch" or a standalone id.
#[tauri::command(async)]
fn set_play_with(app: AppHandle, console: String, who: String) -> Result<cerberus_shared::StandaloneStatus, String> {
    let ok = who == "retroarch" && emulation::core_for(&console).is_some() || standalone::find(&who).is_some_and(|e| e.consoles.contains(&console.as_str()));
    if !ok {
        return Err(tr!("Esse emulador não roda jogos desse console.").into());
    }
    let data = app.state::<DataDir>().0.clone();
    settings::update(&data, |s| s.play_with.insert(console, who))?;
    Ok(standalone_status_now(&data))
}

/// Turns the emulators' own game folders on or off for the classics.
#[tauri::command]
async fn set_emulator_folders(app: AppHandle, on: bool) -> Result<cerberus_shared::StandaloneStatus, String> {
    let data = app.state::<DataDir>().0.clone();
    settings::update(&data, |s| s.emulator_folders = on)?;
    let _ = rescan_roms(&app).await;
    Ok(standalone_status_now(&data))
}

/// Logs RetroArch in to RetroAchievements with the linked account.
#[tauri::command]
async fn link_retroarch_achievements(app: AppHandle, http: State<'_, reqwest::Client>, password: String) -> Result<cerberus_shared::EmulationView, String> {
    let (data, picked) = retroarch_choice(&app);
    let user = settings::load(&data).ra_user.ok_or(tr!("Conecte sua conta em Configurações › RetroAchievements primeiro."))?;
    let (name, token) = emulation::connect_token(&http, &user, &password).await?;
    settings::update(&data, |s| s.ra_token = Some(token.clone()))?;
    // RetroArch gets it now when it's there, or at the next game otherwise.
    if let Some(exe) = emulation::retroarch(&data, picked.as_deref()) {
        emulation::write_token(&exe, &name, &token, true)?;
    }
    Ok(emulation::status(&data, picked.as_deref()))
}

/// Keeps RetroArch connected to the account given in Settings (a new RetroArch,
/// or one whose settings were reset, gets it back before the game opens).
fn connect_retroarch(data: &Path, exe: &Path) {
    let chosen = settings::load(data);
    if let (Some(user), Some(token)) = (chosen.ra_user, chosen.ra_token) {
        let _ = emulation::write_token(exe, &user, &token, false);
    }
}

/// The page the desktop window opens on.
#[tauri::command(async)]
fn set_start_page(app: AppHandle, page: String) -> Result<SettingsView, String> {
    if !matches!(page.as_str(), "home" | "library" | "discover") {
        return Err(tr!("Página inválida.").into());
    }
    let data = app.state::<DataDir>();
    settings::update(&data.0, |s| s.start_page = page)?;
    Ok(settings_view(&app))
}

/// Opens Cerberus with Windows, or stops it.
#[tauri::command(async)]
fn set_autostart(app: AppHandle, on: bool) -> Result<SettingsView, String> {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    let result = if on { manager.enable() } else { manager.disable() };
    result.map_err(|e| tr!("O Windows não deixou mudar a inicialização: {}", e))?;
    Ok(settings_view(&app))
}

/// Whether Cerberus, opened with Windows, starts in the tray.
#[tauri::command(async)]
fn set_start_hidden(app: AppHandle, on: bool) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>();
    settings::update(&data.0, |s| s.start_hidden = on)?;
    Ok(settings_view(&app))
}

fn steam_api_account(app: &AppHandle) -> Option<(String, String)> {
    let chosen = settings::load(&app.state::<DataDir>().0);
    chosen.steam_key.zip(chosen.steam_id)
}

/// The Steam account on this PC (the last to sign in), to offer it when connecting.
#[tauri::command(async)]
fn steam_local_account() -> Option<(String, String)> {
    steam::root().and_then(|root| achievements::steam_api::local_account(&root))
}

/// The connected Steam account, read again from Steam.
#[tauri::command]
async fn steam_account(app: AppHandle, http: State<'_, reqwest::Client>) -> Result<Option<cerberus_shared::SteamAccount>, String> {
    let Some((key, id)) = steam_api_account(&app) else { return Ok(None) };
    achievements::steam_api::account(&http, &key, &id).await.map(Some)
}

/// Turns on Steam's Web API with the player's key. `profile` (number or link) is
/// only needed when the account on this PC isn't the one to read.
#[tauri::command]
async fn steam_connect(app: AppHandle, http: State<'_, reqwest::Client>, key: String, profile: Option<String>) -> Result<cerberus_shared::SteamAccount, String> {
    let key = key.trim().to_string();
    if key.len() != 32 || !key.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(tr!("A chave da Web API da Steam tem 32 letras e números. Copie de novo da página da Steam.").into());
    }
    let id = match profile.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        Some(typed) => achievements::steam_api::resolve(&http, &key, typed).await?,
        None => steam_local_account().map(|(id, _)| id).ok_or(tr!("Nenhuma conta da Steam encontrada neste PC. Informe o seu perfil."))?,
    };
    let account = achievements::steam_api::account(&http, &key, &id).await?;
    let data = app.state::<DataDir>();
    settings::update(&data.0, |s| {
        s.steam_key = Some(key);
        s.steam_id = Some(id);
    })?;
    Ok(account)
}

#[tauri::command(async)]
fn steam_disconnect(app: AppHandle) -> Result<(), String> {
    let data = app.state::<DataDir>();
    settings::update(&data.0, |s| {
        s.steam_key = None;
        s.steam_id = None;
    })
}

/// Opens Steam's page for the key, the privacy settings, or the profile.
#[tauri::command(async)]
fn steam_open(app: AppHandle, page: String) -> Result<(), String> {
    let url = match page.as_str() {
        "key" => "https://steamcommunity.com/dev/apikey".to_string(),
        "privacy" => "https://steamcommunity.com/my/edit/settings".to_string(),
        _ => match settings::load(&app.state::<DataDir>().0).steam_id {
            Some(id) => format!("https://steamcommunity.com/profiles/{id}"),
            None => "https://steamcommunity.com/my".to_string(),
        },
    };
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

fn retro_account(app: &AppHandle) -> Option<(String, String)> {
    let chosen = settings::load(&app.state::<DataDir>().0);
    chosen.ra_user.zip(chosen.ra_key)
}

/// The achievements hub: Steam and RetroAchievements together.
#[tauri::command]
async fn achievements_hub(app: AppHandle, http: State<'_, reqwest::Client>, online: Option<bool>, force: Option<bool>) -> Result<cerberus_shared::AchievementsHub, String> {
    let data = app.state::<DataDir>().0.clone();
    Ok(net::secure_all(achievements::hub(&http, &data, retro_account(&app), steam_api_account(&app), online.unwrap_or(true), force.unwrap_or(false)).await))
}

/// Opens a page about a Steam game: the store, SteamDB, PCGamingWiki, or the
/// site its store page lists. Only these addresses, built here from the app id.
#[tauri::command]
async fn open_store_link(app: AppHandle, http: State<'_, reqwest::Client>, app_id: u32, kind: String) -> Result<(), String> {
    let url = match kind.as_str() {
        "store" => format!("https://store.steampowered.com/app/{app_id}/"),
        "steamdb" => format!("https://steamdb.info/app/{app_id}/"),
        "pcgw" => format!("https://www.pcgamingwiki.com/api/appid.php?appid={app_id}"),
        "site" => {
            let data = app.state::<DataDir>().0.clone();
            steam_details::details(&http, &data, app_id).await?.website.ok_or(tr!("Esse jogo não tem site na loja."))?
        }
        _ => return Err(tr!("Link inválido.").into()),
    };
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

/// A classic's RetroAchievements: its game found by the ROM's hash or name.
/// `None` when no account is connected or RetroAchievements has no such game.
/// A classic's console: the one read from its file, else the one the page sent.
fn console_for(read: &str, page: String) -> String {
    if read.is_empty() { page } else { read.to_string() }
}

#[tauri::command]
async fn classic_achievements(app: AppHandle, http: State<'_, reqwest::Client>, path: String, console: String) -> Result<Option<cerberus_shared::GameAchievements>, String> {
    let Some((user, key)) = retro_account(&app) else { return Ok(None) };
    let rom = PathBuf::from(&path);
    let Some(known) = app.state::<RomIndex>().get().into_iter().find(|r| r.path == rom) else {
        return Err(tr!("Esse arquivo não está nas suas pastas de ROMs.").into());
    };
    let stem = rom.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let data = app.state::<DataDir>().0.clone();
    // The console read from the file, as Play uses; the page's only when the file says none.
    let console = console_for(&known.system, console);
    let Some((id, by_hash)) = achievements::classic::game_for(&http, &data, &user, &key, &rom, &console, &[known.title.clone(), stem]).await? else {
        return Ok(None);
    };
    let (game, list) = achievements::retro::game(&http, &user, &key, &id.to_string()).await?;
    Ok(Some(cerberus_shared::GameAchievements { game, achievements: list, by_title: !by_hash }))
}

/// Asks GitHub now for a newer Cerberus.
#[tauri::command]
async fn update_check(app: AppHandle, http: State<'_, reqwest::Client>) -> Result<Option<cerberus_shared::AppUpdate>, String> {
    let found = self_update::check(&http).await?;
    *app.state::<self_update::Found>().0.lock().unwrap_or_else(|p| p.into_inner()) = found.clone();
    Ok(found)
}

/// The newer version found earlier, if any (for windows opened later).
#[tauri::command]
fn update_found(app: AppHandle) -> Option<cerberus_shared::AppUpdate> {
    app.state::<self_update::Found>().0.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

/// Downloads, checks and installs the newer version found; Cerberus closes and reopens.
#[tauri::command]
async fn update_install(app: AppHandle, http: State<'_, reqwest::Client>) -> Result<(), String> {
    let found = app.state::<self_update::Found>().0.lock().unwrap_or_else(|p| p.into_inner()).clone();
    let update = match found {
        Some(u) => u,
        None => self_update::check(&http).await?.ok_or(tr!("Você já está na versão mais nova."))?,
    };
    // A game, a store game being counted, or a download: the update waits for them.
    if let Some(why) = background::busy(&app) {
        return Err(tr!("Espere para atualizar o Cerberus. {}", why));
    }
    let data = app.state::<DataDir>().0.clone();
    self_update::install(&app, &http, &data, update).await
}

/// This version's notes once after an update; `None` on a first install or when seen.
#[tauri::command]
async fn whats_new(app: AppHandle, http: State<'_, reqwest::Client>) -> Result<Option<cerberus_shared::AppUpdate>, String> {
    let data = app.state::<DataDir>().0.clone();
    let chosen = settings::load(&data);
    let now = self_update::current();
    match chosen.seen_version.as_deref() {
        Some(seen) if self_update::is_newer(now, seen) => self_update::notes_of(&http, now).await,
        Some(_) => Ok(None),
        None => {
            // First run of a fresh install: nothing to tell, just remember this version.
            settings::update(&data, |s| s.seen_version = Some(now.to_string()))?;
            Ok(None)
        }
    }
}

#[tauri::command(async)]
fn whats_new_seen(app: AppHandle) -> Result<(), String> {
    let data = app.state::<DataDir>().0.clone();
    settings::update(&data, |s| s.seen_version = Some(self_update::current().to_string()))
}

/// A game's play sessions, oldest first, for its play-time chart.
#[tauri::command(async)]
fn game_sessions(app: AppHandle, id: String) -> Vec<cerberus_shared::PlaySession> {
    play_log::of_game(&app.state::<DataDir>().0, &id)
}

/// A Steam game's store page (description, pictures, requirements...).
#[tauri::command]
async fn steam_details(app: AppHandle, http: State<'_, reqwest::Client>, app_id: u32) -> Result<cerberus_shared::StoreDetails, String> {
    let data = app.state::<DataDir>().0.clone();
    steam_details::details(&http, &data, app_id).await
}

/// The store page of a game sold elsewhere (Epic, Xbox): the same game on the
/// Steam store when it's sold there (the fullest page), otherwise its Microsoft
/// Store page (by the Xbox product, or found by name).
#[tauri::command]
async fn store_page(app: AppHandle, http: State<'_, reqwest::Client>, name: String, product: Option<String>) -> Result<Option<cerberus_shared::StoreDetails>, String> {
    let data = app.state::<DataDir>().0.clone();
    let product = product.filter(|p| !p.is_empty());
    // A game whose store product is known gets its own page first: a Steam
    // game with the same name may be another one ("DOOM", "Prey").
    if let Some(p) = &product {
        if let Some(mut page) = ms_store::details(&http, &data, p).await {
            page.origin = "msstore".into();
            return Ok(Some(page));
        }
    }
    if let Some(steam) = steam_store::app_for(&http, &data, &name).await {
        if let Ok(mut page) = steam_details::details(&http, &data, steam).await {
            // Pages kept from before the origin was written say it too.
            page.origin = "steam".into();
            return Ok(Some(page));
        }
    }
    let product = match product {
        Some(p) => Some(p),
        None => ms_store::find(&http, &data, &name).await,
    };
    Ok(match product {
        Some(p) => ms_store::details(&http, &data, &p).await.map(|mut page| {
            page.origin = "msstore".into();
            page
        }),
        None => None,
    })
}

/// An Epic or Xbox game's store, when Cerberus has no page for it: the Epic
/// store, or the product's page in the Microsoft Store.
#[tauri::command]
fn open_in_store(app: AppHandle, kind: String, product: String) -> Result<(), String> {
    let url = match kind.as_str() {
        "epic" => "com.epicgames.launcher://store/".to_string(),
        // Store ids are letters and digits only: nothing else goes into the address.
        "xbox" if !product.is_empty() && product.chars().all(|c| c.is_ascii_alphanumeric()) => format!("ms-windows-store://pdp/?productid={product}"),
        "xbox" => "ms-windows-store://home".to_string(),
        _ => return Err(tr!("Essa loja não é conhecida.").into()),
    };
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

/// What the developers of these Steam games (app id, name) posted lately.
#[tauri::command]
async fn steam_news(app: AppHandle, http: State<'_, reqwest::Client>, games: Vec<(u32, String)>) -> Result<Vec<cerberus_shared::NewsItem>, String> {
    let data = app.state::<DataDir>().0.clone();
    Ok(steam_news::latest(&http, &data, games).await)
}

/// The original game's Wikipedia summary, in the player's language.
#[tauri::command]
async fn wiki_about(app: AppHandle, http: State<'_, reqwest::Client>, game: String) -> Result<Option<cerberus_shared::WikiAbout>, String> {
    let data = app.state::<DataDir>().0.clone();
    Ok(wikipedia::about(&http, &data, &game).await)
}

/// Opens a Wikipedia article in the browser; only Wikipedia's own links.
#[tauri::command(async)]
fn open_wiki(app: AppHandle, url: String) -> Result<(), String> {
    let host = url.strip_prefix("https://").and_then(|r| r.split('/').next()).unwrap_or_default();
    if !(host.ends_with(".wikipedia.org") && !host.contains(['@', ':'])) {
        return Err(tr!("Esse link não é da Wikipedia.").into());
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

/// Opens a post from Novidades in the browser; only Steam's own links.
#[tauri::command(async)]
fn open_news(app: AppHandle, url: String) -> Result<(), String> {
    if !steam_news::steam_link(&url) {
        return Err(tr!("Esse link não é da Steam.").into());
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

/// One game's achievements for its page in the hub.
#[tauri::command]
async fn game_achievements(app: AppHandle, http: State<'_, reqwest::Client>, source: String, key: String) -> Result<cerberus_shared::GameAchievements, String> {
    let source = cerberus_shared::AchievementSource::from_code(&source).ok_or(tr!("Jogo inválido."))?;
    let data = app.state::<DataDir>().0.clone();
    achievements::game(&http, &data, source, &key, retro_account(&app), steam_api_account(&app)).await.map(net::secure_all)
}

/// The monitors plugged in.
#[tauri::command]
fn list_monitors(app: AppHandle) -> Vec<cerberus_shared::MonitorView> {
    app.get_webview_window("main").map(|w| monitors::list(&w)).unwrap_or_default()
}

/// Picks the app's language (none: Windows'). The window reloads itself to show it.
#[tauri::command(async)]
fn set_language(app: AppHandle, code: Option<String>) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>();
    let language = code.filter(|c| !c.is_empty());
    settings::update(&data.0, |s| s.language = language.clone())?;
    language::apply(language.as_deref());
    background::retitle_tray(&app);
    // The in-game menu and the achievement notice, if open, redraw in it too.
    for label in [overlay::LABEL, unlock_toast::LABEL] {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.reload();
        }
    }
    Ok(settings_view(&app))
}

/// Picks the monitor Cerberus opens on (none: where Windows puts it) and moves there now.
#[tauri::command(async)]
fn set_monitor(app: AppHandle, name: Option<String>) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>();
    let monitor = name.filter(|n| !n.is_empty());
    settings::update(&data.0, |s| s.monitor = monitor.clone())?;
    if let Some(window) = app.get_webview_window("main") {
        let _ = monitors::place(&window, monitor.as_deref());
    }
    Ok(settings_view(&app))
}

/// The program's own icon as the window's big one (the one the taskbar and
/// Alt+Tab show).
#[cfg(windows)]
fn taskbar_icon(hwnd: isize) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ExtractIconExW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{ICON_BIG, SendMessageW, WM_SETICON};
    let Ok(exe) = std::env::current_exe() else { return };
    let path: Vec<u16> = exe.as_os_str().encode_wide().chain([0]).collect();
    let mut large = std::ptr::null_mut();
    // SAFETY: a nul-terminated path, and room for the one large icon asked for.
    let found = unsafe { ExtractIconExW(path.as_ptr(), 0, &mut large, std::ptr::null_mut(), 1) };
    if found > 0 && !large.is_null() {
        // SAFETY: a window of this process; the icon stays alive with the app.
        unsafe { SendMessageW(hwnd as _, WM_SETICON, ICON_BIG as usize, large as isize) };
    }
}

/// Where the window was before Big Picture moved it to the chosen monitor.
static BEFORE_TV: std::sync::Mutex<Option<monitors::Placed>> = std::sync::Mutex::new(None);

/// Before Big Picture fills the screen: the chosen monitor's. Says so when that
/// monitor isn't connected (Big Picture then opens on this one).
#[tauri::command]
fn to_chosen_monitor(app: AppHandle) -> Option<String> {
    let chosen = settings::load(&app.state::<DataDir>().0);
    let window = app.get_webview_window("main")?;
    let placed = monitors::place(&window, chosen.monitor.as_deref());
    if matches!(placed, monitors::Placed::Moved { .. }) {
        *BEFORE_TV.lock().unwrap_or_else(|p| p.into_inner()) = Some(placed);
    }
    (placed == monitors::Placed::Missing).then(|| tr!("O monitor escolhido para o Big Picture não está ligado: ele abriu neste.").to_string())
}

/// Leaving Big Picture: the window goes back where it was before it.
#[tauri::command]
fn back_from_tv(app: AppHandle) {
    let Some(monitors::Placed::Moved { from, maximized }) = BEFORE_TV.lock().unwrap_or_else(|p| p.into_inner()).take() else { return };
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_fullscreen(false);
        let _ = window.unmaximize();
        let _ = window.set_position(tauri::PhysicalPosition::new(from.0, from.1));
        if maximized {
            let _ = window.maximize();
        }
    }
}

/// Whether closing the window keeps Cerberus in the tray.
#[tauri::command(async)]
fn set_close_to_tray(app: AppHandle, on: bool) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>();
    settings::update(&data.0, |s| s.close_to_tray = on)?;
    Ok(settings_view(&app))
}

/// Shows PC Game Pass's games in the library, or not.
#[tauri::command(async)]
fn set_game_pass(app: AppHandle, on: bool) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>();
    settings::update(&data.0, |s| s.game_pass = on)?;
    let _ = app.emit("catalog-changed", ());
    Ok(settings_view(&app))
}

/// Opens the Xbox app (to sign in, install, manage Game Pass).
#[tauri::command(async)]
fn xbox_open(app: AppHandle) -> Result<(), String> {
    app.opener().open_url("msxbox://", None::<&str>).map_err(|e| tr!("Não foi possível abrir o app Xbox: {}", e))
}

/// Turns recomplauncher.com's catalog on or off; Cerberus's own list stays.
#[tauri::command]
async fn set_site_catalog(app: AppHandle, http: State<'_, reqwest::Client>, on: bool) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>();
    settings::update(&data.0, |s| s.site_catalog = on)?;
    recomp::catalog::use_site(on);
    if on {
        recomp::catalog::load_copy(&data.cache());
        let _ = recomp::catalog::refresh(&http, &data.cache(), false).await;
    }
    let _ = app.emit("catalog-changed", ());
    Ok(settings_view(&app))
}

/// Shows the first-run setup again the next time it is asked for.
#[tauri::command(async)]
fn rerun_setup(app: AppHandle) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>();
    settings::update(&data.0, |s| s.setup_done = false)?;
    Ok(settings_view(&app))
}

/// Links a RetroAchievements account once the site takes the username and key.
#[tauri::command]
async fn ra_connect(app: AppHandle, http: State<'_, reqwest::Client>, user: String, key: String, password: Option<String>) -> Result<cerberus_shared::RaProfile, String> {
    let (user, key) = (user.trim().to_string(), key.trim().to_string());
    if user.is_empty() || key.is_empty() {
        return Err(tr!("Preencha o usuário e a chave de API.").into());
    }
    let profile = retroachievements::profile(&http, &user, &key).await?;
    // The password, when given, connects RetroArch too; only the token it gives is kept.
    let token = match password.as_deref().filter(|p| !p.is_empty()) {
        Some(p) => Some(emulation::connect_token(&http, &profile.user, p).await?),
        None => None,
    };
    let (data, picked) = retroarch_choice(&app);
    let chosen = settings::load(&data);
    // Another account than the one before: the old one's token goes, here and in the emulators.
    let switched = chosen.ra_user.as_deref().is_some_and(|before| !before.eq_ignore_ascii_case(&profile.user));
    if switched {
        if let Some(old) = &chosen.ra_token {
            forget_ra_everywhere(&data, &chosen, old);
        }
    }
    if let Some((name, token)) = &token {
        if let Some(exe) = emulation::retroarch(&data, picked.as_deref()) {
            let _ = emulation::write_token(&exe, name, token, true);
        }
    }
    // The emulators' files above are touched outside the settings' lock; only the fields change under it.
    settings::update(&data, |s| {
        if switched {
            s.ra_token = None;
        }
        s.ra_user = Some(profile.user.clone());
        s.ra_key = Some(key);
        if let Some((_, token)) = &token {
            s.ra_token = Some(token.clone());
        }
    })?;
    Ok(profile)
}

/// The linked account's profile and recent games; `None` when none is linked.
#[tauri::command]
async fn ra_profile(app: AppHandle, http: State<'_, reqwest::Client>) -> Result<Option<cerberus_shared::RaProfile>, String> {
    let chosen = settings::load(&app.state::<DataDir>().0);
    let (Some(user), Some(key)) = (chosen.ra_user, chosen.ra_key) else { return Ok(None) };
    retroachievements::profile(&http, &user, &key).await.map(Some)
}

/// Forgets the RetroAchievements account on this PC.
#[tauri::command(async)]
fn ra_disconnect(app: AppHandle) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>();
    let chosen = settings::load(&data.0);
    // "Forget the account on this PC" includes every emulator it was written into.
    if let Some(token) = &chosen.ra_token {
        forget_ra_everywhere(&data.0, &chosen, token);
    }
    settings::update(&data.0, |s| {
        s.ra_user = None;
        s.ra_key = None;
        s.ra_token = None;
    })?;
    Ok(settings_view(&app))
}

/// Takes a RetroAchievements token Cerberus wrote out of RetroArch and the
/// standalone emulators found here (a token the player set there stays).
fn forget_ra_everywhere(data: &Path, chosen: &settings::Settings, token: &str) {
    if let Some(exe) = emulation::retroarch(data, chosen.retroarch.as_deref()) {
        emulation::forget_token(&exe, token);
    }
    for id in ["pcsx2", "dolphin", "duckstation"] {
        if let Some(emulator) = standalone::find(id) {
            if let Some((exe, _)) = standalone::locate(data, chosen.emulators.get(id).map(PathBuf::as_path), emulator) {
                cheevos::forget(id, &exe, token);
            }
        }
    }
}

/// Opens RetroAchievements: the player's profile, or the page with their API key.
#[tauri::command(async)]
fn ra_open(app: AppHandle, keys: bool) -> Result<(), String> {
    let chosen = settings::load(&app.state::<DataDir>().0);
    let url = match (keys, chosen.ra_user) {
        (false, Some(user)) => format!("https://retroachievements.org/user/{user}"),
        _ => "https://retroachievements.org/settings".to_string(),
    };
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

/// Whether the app opens in the TV screen; changed in Configurações.
/// The recomp download limit (MB/s, 0 for none) and slowing down while playing.
#[tauri::command(async)]
fn set_download_limit(app: AppHandle, mb: u32, slow_while_playing: bool) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>();
    let limit = mb.min(1000);
    settings::update(&data.0, |s| {
        s.download_limit = limit;
        s.slow_while_playing = slow_while_playing;
    })?;
    recomp::throttle::set_limit(limit, slow_while_playing);
    Ok(settings_view(&app))
}

/// Saves a copy of the player's Cerberus data where they choose. `None`: they cancelled.
#[tauri::command]
async fn export_data(app: AppHandle) -> Result<Option<usize>, String> {
    let day = chrono_day();
    let dialog = app.dialog().file().set_title(tr!("Salvar cópia dos dados")).set_file_name(format!("cerberus-{day}.zip")).add_filter(tr!("Cópia do Cerberus"), &["zip"]);
    let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_save_file()).await.map_err(|e| e.to_string())?;
    let Some(picked) = picked else { return Ok(None) };
    let out = picked.into_path().map_err(|e| e.to_string())?;
    let data = app.state::<DataDir>().0.clone();
    tauri::async_runtime::spawn_blocking(move || data_backup::export(&data, &out)).await.map_err(|e| e.to_string())?.map(Some)
}

/// Puts a copy back, then starts Cerberus again so everything reads it. `None`: they cancelled.
#[tauri::command]
async fn import_data(app: AppHandle) -> Result<Option<usize>, String> {
    let dialog = app.dialog().file().set_title(tr!("Escolha uma cópia do Cerberus")).add_filter(tr!("Cópia do Cerberus"), &["zip"]);
    let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_file()).await.map_err(|e| e.to_string())?;
    let Some(picked) = picked else { return Ok(None) };
    let from = picked.into_path().map_err(|e| e.to_string())?;
    // A game, a store game or a download would write over the restored files.
    if let Some(why) = background::busy(&app) {
        return Err(tr!("Espere para trazer uma cópia. {}", why));
    }
    let data = app.state::<DataDir>().0.clone();
    // Checked and unpacked now; put in place when Cerberus starts again, before anything reads it.
    let count = tauri::async_runtime::spawn_blocking(move || data_backup::stage(&data, &from)).await.map_err(|e| e.to_string())??;
    let again = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
        again.restart();
    });
    Ok(Some(count))
}

/// Today as YYYY-MM-DD (UTC), for file names.
fn chrono_day() -> String {
    day_of(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default())
}

fn day_of(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    // Civil date from days since 1970 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Whether the Epic Games Launcher is here, and how many of its games are installed.
#[tauri::command]
async fn epic_status() -> Result<(bool, usize), String> {
    tauri::async_runtime::spawn_blocking(|| (epic::launcher_found(), epic::installed_games().len())).await.map_err(|e| e.to_string())
}

/// Opens the Epic Games Launcher on its library.
#[tauri::command(async)]
fn epic_open(app: AppHandle) -> Result<(), String> {
    app.opener().open_url("com.epicgames.launcher://store/library", None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn set_notices(app: AppHandle, choices: cerberus_shared::Notices) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>();
    settings::update(&data.0, |s| s.notices = choices.clone())?;
    notify::set(choices);
    Ok(settings_view(&app))
}

#[tauri::command(async)]
fn set_in_game(app: AppHandle, choices: cerberus_shared::InGame) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>();
    let corner = if ["br", "bl", "tr", "tl"].contains(&choices.corner.as_str()) { choices.corner.clone() } else { "br".into() };
    let in_game = cerberus_shared::InGame { corner, ..choices };
    settings::update(&data.0, |s| s.in_game = in_game.clone())?;
    in_game::set(in_game);
    Ok(settings_view(&app))
}

#[tauri::command(async)]
fn set_start_in_tv(app: AppHandle, on: bool) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>();
    settings::update(&data.0, |s| s.start_in_tv = on)?;
    Ok(settings_view(&app))
}

#[tauri::command(async)]
fn get_settings(app: AppHandle) -> SettingsView {
    settings_view(&app)
}

/// Asks for a folder for new recomp installs. Games already installed stay put.
#[tauri::command]
async fn pick_recomps_dir(app: AppHandle) -> Result<SettingsView, String> {
    let dialog = app.dialog().file().set_title(tr!("Pasta para instalar recomps"));
    let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_folder());
    let Some(picked) = picked.await.map_err(|e| e.to_string())? else {
        return Ok(settings_view(&app));
    };
    let dir = picked.into_path().map_err(|e| e.to_string())?;
    use_recomps_dir(&app, Some(dir))
}

#[tauri::command(async)]
fn reset_recomps_dir(app: AppHandle) -> Result<SettingsView, String> {
    use_recomps_dir(&app, None)
}

fn use_recomps_dir(app: &AppHandle, dir: Option<PathBuf>) -> Result<SettingsView, String> {
    let folders = app.state::<recomp::Folders>();
    let target = dir.clone().unwrap_or_else(|| folders.default_installs.clone());
    fs::create_dir_all(&target).map_err(|e| tr!("Não foi possível usar {0}: {1}", target.display(), e))?;
    let data_dir = app.state::<DataDir>().0.clone();
    settings::update(&data_dir, |s| s.recomps_dir = dir)?;
    folders.set_installs(target);
    Ok(settings_view(app))
}

#[tauri::command(async)]
fn open_data_dir(app: AppHandle) -> Result<(), String> {
    let dir = app.state::<DataDir>().0.to_string_lossy().into_owned();
    app.opener().open_path(dir, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn clear_art_cache(app: AppHandle) -> Result<SettingsView, String> {
    app.state::<Arc<ArtCache>>().clear()?;
    Ok(settings_view(&app))
}

#[tauri::command(async)]
fn library_recovery_notice(library: State<'_, LocalLibrary>) -> Option<String> {
    library.recovery_notice().or_else(|| LOCAL_LIBRARY_PROBLEM.lock().ok().and_then(|p| p.clone()))
}

/// Why the games added here and the recomps couldn't be read on the last listing (the stores' games still show).
static LOCAL_LIBRARY_PROBLEM: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

#[tauri::command]
async fn list_games(
    app: AppHandle,
    library: State<'_, LocalLibrary>,
    art: State<'_, Arc<ArtCache>>,
    http: State<'_, reqwest::Client>,
) -> Result<Vec<Game>, String> {
    // A library file that can't be read takes only its own games away: the
    // stores' games still show, and the Library page says what happened.
    let mut games = match library.games_with_parts() {
        Ok(games) => {
            *LOCAL_LIBRARY_PROBLEM.lock().unwrap_or_else(|p| p.into_inner()) = None;
            games
        }
        Err(e) => {
            *LOCAL_LIBRARY_PROBLEM.lock().unwrap_or_else(|p| p.into_inner()) = Some(tr!("Os jogos adicionados e os recomps não puderam ser lidos agora ({}). Os das lojas continuam aqui; nada foi apagado.", e));
            Vec::new()
        }
    };
    games.iter_mut().for_each(recomp::apply_art);
    // Each recomp is matched on its own, so the kept matches of the whole catalog serve here.
    let owned = owned_games(&app);
    for game in &mut games {
        let found = match &game.source {
            Source::Recomp { recomp_id, .. } => owned.get(recomp_id),
            _ => None,
        };
        game.next_step = recomp::next_step(game, found);
    }
    let mut stores = store_games(&app, &http).await;
    steam_dlc_names(&app, &http, &mut stores);
    // Steam counts its games' time on every PC; that beats what Cerberus saw here.
    let steam_id = settings::load(&app.state::<DataDir>().0).steam_id;
    let data_dir = app.state::<DataDir>().0.clone();
    let steam_time = tauri::async_runtime::spawn_blocking(move || achievements::steam_playtimes(&data_dir, steam_id.as_deref())).await.unwrap_or_default();
    // Files and the achievements database read off the async runtime.
    let data_dir = app.state::<DataDir>().0.clone();
    let mut games = tauri::async_runtime::spawn_blocking(move || assemble(&data_dir, games, stores, &steam_time)).await.map_err(|e| e.to_string())?;
    let mut missing = Vec::new();
    art.use_copies_for_games(&mut games, &mut missing);
    // A cover picked for a game added by hand, kept by full path: after the data
    // folder moved (another PC, a backup restored) it's found by its name.
    let covers = app.state::<DataDir>().0.join("covers");
    for game in games.iter_mut() {
        // The name first: a disk check for each of ~1000 games' covers costs more than all the rest.
        if let Some(kept) = game.cover_path.clone().filter(|p| p.contains("covers") && !Path::new(p).is_file()) {
            game.cover_path = art_custom::in_folder(&covers, &kept);
        }
    }
    // The player's own pictures win over every other art.
    let custom = app.state::<art_custom::CustomArt>();
    games.iter_mut().for_each(|game| custom.apply(game));
    {
        static SYNCING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !SYNCING.swap(true, std::sync::atomic::Ordering::SeqCst) {
            let (app, http, data) = (app.clone(), http.inner().clone(), app.state::<DataDir>().0.clone());
            tauri::async_runtime::spawn(async move {
                let asked = achievements::sync_accounts(&http, &data).await;
                SYNCING.store(false, std::sync::atomic::Ordering::SeqCst);
                if asked {
                    let _ = app.emit("catalog-changed", ());
                }
            });
        }
    }
    art.fetch_later(http.inner().clone(), missing);
    Ok(games)
}

/// The library as the window gets it, from what was read: the games here
/// (library file, recomps), the stores', and Steam's own play times. Sorted
/// by name, pictures over TLS (the window's CSP loads no http ones), with the
/// player's time, favorites, collections, notes and paused updates on each.
fn assemble(data: &Path, mut games: Vec<Game>, stores: Vec<Game>, steam_time: &std::collections::HashMap<u32, (u64, Option<u64>)>) -> Vec<Game> {
    games.extend(stores);
    gamepass::drop_owned_elsewhere(&mut games);
    games.sort_by_key(|game| game.name.to_lowercase());
    for game in &mut games {
        for url in [&mut game.cover_url, &mut game.hero_url].into_iter().flatten() {
            *url = net::secure(url);
        }
    }
    store_time::apply(data, &mut games);
    // Steam counts its games' time on every PC; that beats what Cerberus saw here.
    for game in games.iter_mut() {
        let Source::Steam { app_id } = &game.source else { continue };
        if let Some((secs, last)) = steam_time.get(app_id) {
            game.playtime_secs = game.playtime_secs.max(*secs);
            game.last_played = match (game.last_played, *last) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            };
        }
    }
    // The Xbox and Epic accounts count their games' time too: kept in the
    // achievements store, brought up to date in the background when due.
    achievements::account_times(data, &mut games);
    let favorite = favorites::load(data);
    games.iter_mut().for_each(|game| game.favorite = favorite.contains(&game.id));
    collections::apply(data, &mut games);
    game_notes::apply(data, &mut games);
    let paused = settings::load(data).paused_updates;
    games.iter_mut().for_each(|g| g.updates_paused = paused.contains(&g.id));
    games
}

/// Steam DLCs by name: the names kept on disk replace the app ids, and the
/// ones not known yet are asked of the store in the background (a few at a
/// time, once each), the library listed again when they come.
fn steam_dlc_names(app: &AppHandle, http: &reqwest::Client, games: &mut [Game]) {
    let file = app.state::<DataDir>().cache().join("steam-dlc-names.json");
    let known: std::collections::HashMap<u32, String> = crate::state_file::load_or_default(&file);
    let mut missing = Vec::new();
    for game in games.iter_mut() {
        if !game.dlcs.iter().any(|d| d.starts_with(steam::DLC_ID)) {
            continue;
        }
        let mut names = Vec::new();
        for dlc in std::mem::take(&mut game.dlcs) {
            match dlc.strip_prefix(steam::DLC_ID).and_then(|id| id.parse::<u32>().ok()) {
                Some(id) => match known.get(&id) {
                    Some(name) if !name.is_empty() => names.push(name.clone()),
                    Some(_) => {}
                    None => missing.push(id),
                },
                None => names.push(dlc),
            }
        }
        names.sort();
        game.dlcs = names;
    }
    if missing.is_empty() {
        return;
    }
    static ASKING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if ASKING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let (app, http) = (app.clone(), http.clone());
    tauri::async_runtime::spawn(async move {
        let mut known: std::collections::HashMap<u32, String> = crate::state_file::load_or_default(&file);
        for id in missing.into_iter().take(30) {
            let url = format!("https://store.steampowered.com/api/appdetails?appids={id}&filters=basic");
            let name = match net::send(http.get(&url)).await {
                Ok(reply) if reply.status().is_success() => reply
                    .json::<serde_json::Value>()
                    .await
                    .ok()
                    .and_then(|v| v[id.to_string()]["data"]["name"].as_str().map(str::to_string))
                    .unwrap_or_default(),
                // Not answered now: asked again another time.
                _ => continue,
            };
            // Empty: the store doesn't know it (kept, so it isn't asked again).
            known.insert(id, name);
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }
        let _ = crate::state_file::write_bytes(&file, serde_json::to_string(&known).unwrap_or_default().as_bytes());
        ASKING.store(false, std::sync::atomic::Ordering::SeqCst);
        stores_changed();
        let _ = app.emit("catalog-changed", ());
    });
}

/// The stores' games, as last listed: a heart or a page change doesn't read
/// every store again. Listed afresh when a store says it changed
/// (`catalog-changed`) or after a short while (a game installed by the store itself).
static STORES: std::sync::Mutex<Option<(std::time::Instant, u64, Vec<Game>)>> = std::sync::Mutex::new(None);
static STORES_CHANGED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
const STORES_FRESH: std::time::Duration = std::time::Duration::from_secs(20);

/// What the stores' install folders hold now: each entry's name, size and
/// time, one level deep. A game installed or removed by its store changes it.
fn install_fingerprint(folders: &[PathBuf]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    for folder in folders {
        let Ok(entries) = std::fs::read_dir(folder) else { continue };
        let mut seen: Vec<(std::ffi::OsString, u64, Option<std::time::SystemTime>)> = entries
            .flatten()
            .map(|e| {
                let meta = e.metadata().ok();
                (e.file_name(), meta.as_ref().map_or(0, |m| m.len()), meta.and_then(|m| m.modified().ok()))
            })
            .collect();
        seen.sort();
        folder.hash(&mut hash);
        seen.hash(&mut hash);
    }
    hash.finish()
}

/// Watches the stores' install folders: a game installed or uninstalled by
/// Steam, Epic or the Xbox app shows in the library within seconds, without
/// pressing anything. A store downloading changes its files all the time:
/// the library is read again at most every 15 s.
fn watch_store_installs(app: AppHandle) {
    std::thread::spawn(move || {
        let folders = || {
            let mut all = steam::install_folders();
            all.extend(epic::install_folders());
            all.extend(xbox::install_folders());
            all
        };
        let mut watched = folders();
        let mut listed_at = std::time::Instant::now();
        let mut last = install_fingerprint(&watched);
        let mut pending = false;
        loop {
            std::thread::sleep(std::time::Duration::from_secs(4));
            // A library added in Steam, or a drive plugged in: the folders are found again now and then.
            if listed_at.elapsed() > std::time::Duration::from_secs(60) {
                watched = folders();
                listed_at = std::time::Instant::now();
            }
            let now = install_fingerprint(&watched);
            if now != last {
                last = now;
                pending = true;
            }
            if pending && LAST_INSTALL_NEWS.lock().map_or(true, |at| at.is_none_or(|t| t.elapsed() > std::time::Duration::from_secs(15))) {
                pending = false;
                if let Ok(mut at) = LAST_INSTALL_NEWS.lock() {
                    *at = Some(std::time::Instant::now());
                }
                stores_changed();
                let _ = app.emit("catalog-changed", ());
            }
        }
    });
}

static LAST_INSTALL_NEWS: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// The stores' listing is out of date.
fn stores_changed() {
    STORES_CHANGED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
}

async fn store_games(app: &AppHandle, http: &reqwest::Client) -> Vec<Game> {
    let generation = STORES_CHANGED.load(std::sync::atomic::Ordering::SeqCst);
    if let Some((at, seen, games)) = STORES.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
        if *seen == generation && at.elapsed() < STORES_FRESH {
            return games.clone();
        }
    }
    // Scanning library folders touches the disk; keep it off the async runtime.
    let scan = tauri::async_runtime::spawn_blocking(|| {
        let mut found = steam::installed_games();
        found.extend(epic::installed_games());
        found.extend(xbox::installed_games());
        found
    });
    let mut games = scan.await.unwrap_or_default();
    game_pass(app, http, &mut games);
    epic_library(app, http, &mut games);
    xbox_library(app, http, &mut games);
    xbox_art(app, http, &mut games);
    owned_not_installed(app, http, &mut games);
    *STORES.lock().unwrap_or_else(|p| p.into_inner()) = Some((std::time::Instant::now(), generation, games.clone()));
    games
}

/// The Epic account's games (when signed in): from disk at once, asked again in
/// the background when an hour old (renewing the token), the library listed again then.
fn epic_library(app: &AppHandle, http: &reqwest::Client, games: &mut Vec<Game>) {
    let data = app.state::<DataDir>().0.clone();
    if settings::load(&data).epic_refresh.is_none() {
        return;
    }
    epic_account::merge(games, &epic_account::cached(&data));
    if !epic_account::stale(&data) {
        return;
    }
    static ASKING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if ASKING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let (app, http) = (app.clone(), http.clone());
    tauri::async_runtime::spawn(async move {
        let result = async {
            // The same session the achievements use: one renewal, never two at once.
            let (access, _) = achievements::epic_session(&http, &data).await?;
            epic_account::library(&http, &access).await
        }
        .await;
        ASKING.store(false, std::sync::atomic::Ordering::SeqCst);
        match result {
            Ok(owned) => {
                epic_account::keep(&data, owned);
                let _ = app.emit("catalog-changed", ());
            }
            Err(e) => trail::note(&app, "epic", tk!("Biblioteca da Epic"), &Err(e)),
        }
    });
}

/// Opens Epic's own sign-in page in a window of its own (private: nothing stays
/// in it). When it reaches the page that shows the code, the window is sent to
/// an address that is never loaded (`https://cerberus.invalid/epic?code=…`):
/// Cerberus takes the code from that navigation, trades it for a token and
/// closes the window. The page never gets any access to the app.
// Async on purpose: on Windows, building a window from a synchronous command
// (the main thread) deadlocks it, and the new window stays blank.
#[tauri::command]
async fn epic_login(app: AppHandle) -> Result<(), String> {
    if let Some(open) = app.get_webview_window("epic-login") {
        let _ = open.set_focus();
        return Ok(());
    }
    let url = tauri::Url::parse(&epic_account::login_url()).map_err(|e| e.to_string())?;
    let handle = app.clone();
    // This sign-in's secret: only Epic's return page gets it, and the hand-off needs it.
    let secret = epic_account::nonce();
    let (on_load, opener) = (secret.clone(), app.clone());
    tauri::WebviewWindowBuilder::new(&app, "epic-login", tauri::WebviewUrl::External(url))
        .title(tr!("Entrar na Epic Games"))
        .inner_size(800.0, 860.0)
        .incognito(true)
        // "Entrar com Xbox/Google/Apple…" open the provider's page in a pop-up.
        // WebView2's own pop-up keeps the same private session and the link back
        // to Epic's page (a window of ours didn't: it stayed blank).
        .on_new_window(|target, _| {
            if epic_account::login_site(&target) {
                tauri::webview::NewWindowResponse::Allow
            } else {
                tauri::webview::NewWindowResponse::Deny
            }
        })
        .on_navigation(move |at| {
            // The hand-off: never loaded, the code read from the address, with this sign-in's secret.
            if at.host_str() == Some("cerberus.invalid") {
                if let Some(code) = epic_account::hand_off_code(at, &secret) {
                    let app = handle.clone();
                    tauri::async_runtime::spawn(async move { finish_epic_login(app, code).await });
                }
                return false;
            }
            // Only the sign-in sites open here; a link elsewhere (help, terms) opens in the browser.
            if epic_account::login_site(at) {
                return true;
            }
            if at.scheme() == "https" {
                use tauri_plugin_opener::OpenerExt;
                let _ = opener.opener().open_url(at.as_str(), None::<&str>);
            }
            false
        })
        .on_page_load(move |window, payload| {
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) && epic_account::is_return(payload.url()) {
                // The page shows {"authorizationCode": "…"}: carry it to the hand-off address.
                let _ = window.eval(format!(
                    "(function(){{var c='';try{{c=JSON.parse(document.body.innerText).authorizationCode||'';}}catch(e){{}}\
                     location.replace('https://cerberus.invalid/epic?state={on_load}&code='+encodeURIComponent(c));}})()"
                ));
            }
        })
        .build()
        .map_err(|e| tr!("Não foi possível abrir o login da Epic: {}", e))?;
    Ok(())
}

async fn finish_epic_login(app: AppHandle, code: String) {
    let result = async {
        let code = epic_account::valid_code(&code).ok_or(tr!("A Epic não confirmou o login."))?;
        let http = app.state::<reqwest::Client>().inner().clone();
        let session = epic_account::token(&http, epic_account::Grant::Code(&code)).await?;
        let data = app.state::<DataDir>().0.clone();
        let name = if session.name.is_empty() { tr!("Conta Epic").to_string() } else { session.name.clone() };
        settings::update(&data, |s| {
            s.epic_user = Some(name.clone());
            s.epic_refresh = Some(session.refresh.clone());
        })?;
        achievements::forget_epic_session();
        // Connected: said at once; the account's games follow in the background.
        let (app2, http2, data2, access) = (app.clone(), http.clone(), data.clone(), session.access.clone());
        tauri::async_runtime::spawn(async move {
            match epic_account::library(&http2, &access).await {
                Ok(owned) => {
                    epic_account::keep(&data2, owned);
                    let _ = app2.emit("catalog-changed", ());
                }
                Err(e) => trail::note(&app2, "epic", tk!("Biblioteca da Epic"), &Err(e)),
            }
        });
        Ok::<String, String>(name)
    }
    .await;
    // The sign-in window and any provider pop-up it opened.
    for (label, window) in app.webview_windows() {
        if label.starts_with("epic-login") {
            let _ = window.close();
        }
    }
    match result {
        Ok(name) => {
            trail::note(&app, "epic", tk!("Conta Epic"), &Ok(tr!("Conectada como {}", name)));
            let _ = app.emit("toast", tr!("Epic conectada como {}. Os jogos da conta entram na Biblioteca.", name));
            let _ = app.emit("accounts-changed", ());
            let _ = app.emit("catalog-changed", ());
        }
        Err(e) => {
            let _ = app.emit("toast", tr!("Não foi possível entrar na Epic: {}", e));
        }
    }
}

/// The Xbox account's PC games (when signed in), like the Epic account's.
fn xbox_library(app: &AppHandle, http: &reqwest::Client, games: &mut Vec<Game>) {
    let data = app.state::<DataDir>().0.clone();
    let Some(refresh) = settings::load(&data).xbox_refresh else { return };
    xbox_account::merge(games, &xbox_account::cached(&data));
    if !xbox_account::stale(&data) {
        return;
    }
    static ASKING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if ASKING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let (app, http) = (app.clone(), http.clone());
    tauri::async_runtime::spawn(async move {
        let result = async {
            // The same session the achievements use: one sign-in, never two at once.
            let session = achievements::xbox::session(&http, &refresh, |r| settings::renew_token(&data, settings::Account::Xbox, &refresh, r)).await?;
            let owned = xbox_account::library(&http, &session).await?;
            Ok::<_, String>(xbox_account::with_store_details(&http, owned).await)
        }
        .await;
        ASKING.store(false, std::sync::atomic::Ordering::SeqCst);
        match result {
            Ok(owned) => {
                xbox_account::keep(&data, owned);
                let _ = app.emit("catalog-changed", ());
            }
            Err(e) => trail::note(&app, "xbox", tk!("Biblioteca do Xbox"), &Err(e)),
        }
    });
}

/// Opens Microsoft's own sign-in page in a private window; the code comes back
/// in the return address, and is traded for the account's Xbox Live access.
// Async on purpose: on Windows, building a window from a synchronous command
// (the main thread) deadlocks it, and the new window stays blank.
#[tauri::command]
async fn xbox_login(app: AppHandle) -> Result<(), String> {
    if let Some(open) = app.get_webview_window("xbox-login") {
        let _ = open.set_focus();
        return Ok(());
    }
    let url = tauri::Url::parse(&xbox_account::login_url()).map_err(|e| e.to_string())?;
    let handle = app.clone();
    tauri::WebviewWindowBuilder::new(&app, "xbox-login", tauri::WebviewUrl::External(url))
        .title(tr!("Entrar com a conta Microsoft"))
        .inner_size(520.0, 720.0)
        .incognito(true)
        .on_navigation(move |at| {
            if let Some(code) = xbox_account::code_in(at) {
                let app = handle.clone();
                tauri::async_runtime::spawn(async move { finish_xbox_login(app, code).await });
                return false;
            }
            if xbox_account::refused(at) {
                if let Some(window) = handle.get_webview_window("xbox-login") {
                    let _ = window.close();
                }
                return false;
            }
            // Only the sign-in sites open here; anything else in the browser.
            if epic_account::login_site(at) {
                return true;
            }
            if at.scheme() == "https" {
                use tauri_plugin_opener::OpenerExt;
                let _ = handle.opener().open_url(at.as_str(), None::<&str>);
            }
            false
        })
        .build()
        .map_err(|e| tr!("Não foi possível abrir o login da Microsoft: {}", e))?;
    Ok(())
}

async fn finish_xbox_login(app: AppHandle, code: String) {
    if let Some(window) = app.get_webview_window("xbox-login") {
        let _ = window.close();
    }
    let http = app.state::<reqwest::Client>().inner().clone();
    let data = app.state::<DataDir>().0.clone();
    let result = async {
        let session = xbox_account::sign_in(&http, xbox_account::Grant::Code(&code)).await?;
        let gamertag = if session.gamertag.is_empty() { tr!("Conta Microsoft").to_string() } else { session.gamertag.clone() };
        settings::update(&data, |s| {
            s.xbox_user = Some(gamertag.clone());
            s.xbox_refresh = Some(session.refresh.clone());
        })?;
        achievements::xbox::forget_session();
        // Connected: said at once; the account's games (one catalog lookup each) follow in the background.
        let (app2, http2, data2) = (app.clone(), http.clone(), data.clone());
        tauri::async_runtime::spawn(async move {
            match xbox_account::library(&http2, &session).await {
                Ok(owned) => {
                    xbox_account::keep(&data2, xbox_account::with_store_details(&http2, owned).await);
                    let _ = app2.emit("catalog-changed", ());
                }
                Err(e) => trail::note(&app2, "xbox", tk!("Biblioteca do Xbox"), &Err(e)),
            }
        });
        Ok::<String, String>(gamertag)
    }
    .await;
    match result {
        Ok(name) => {
            trail::note(&app, "xbox", tk!("Conta Xbox"), &Ok(tr!("Conectada como {}", name)));
            let _ = app.emit("toast", tr!("Xbox conectado como {}. Os jogos de PC da conta entram na Biblioteca.", name));
            let _ = app.emit("catalog-changed", ());
        }
        Err(e) => {
            let _ = app.emit("toast", tr!("Não foi possível entrar no Xbox: {}", e));
        }
    }
}

/// Forgets the Xbox account on this PC (its token and its library list).
#[tauri::command(async)]
fn xbox_logout(app: AppHandle) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>().0.clone();
    let user = settings::update(&data, |s| {
        s.xbox_refresh = None;
        s.xbox_user.take()
    })?;
    achievements::xbox::forget_session();
    // Its achievements leave the hub with it.
    if let (Some(user), Ok(db)) = (user.as_deref(), achievements::store::Store::open(&data)) {
        db.forget(&achievements::store::Scope::new(cerberus_shared::AchievementSource::Xbox, user));
    }
    xbox_account::forget(&data);
    let _ = app.emit("catalog-changed", ());
    Ok(settings_view(&app))
}

/// Forgets the Epic account on this PC (its token and its library list).
#[tauri::command(async)]
fn epic_logout(app: AppHandle) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>().0.clone();
    let user = settings::update(&data, |s| {
        s.epic_refresh = None;
        s.epic_user.take()
    })?;
    achievements::forget_epic_session();
    // Its achievements leave the hub with it.
    if let (Some(user), Ok(db)) = (user.as_deref(), achievements::store::Store::open(&data)) {
        db.forget(&achievements::store::Scope::new(cerberus_shared::AchievementSource::Epic, user));
    }
    epic_account::forget(&data);
    let _ = app.emit("catalog-changed", ());
    Ok(settings_view(&app))
}

/// PC Game Pass (when the player keeps it on): official art on the Xbox games
/// installed, the others as not installed. The catalog comes from disk at once
/// and is asked again in the background when old; the library is listed again then.
fn game_pass(app: &AppHandle, http: &reqwest::Client, games: &mut Vec<Game>) {
    let data = app.state::<DataDir>().0.clone();
    if !settings::load(&data).game_pass {
        return;
    }
    let catalog = gamepass::cached(&data);
    // Installed now under its package's id: favorite, note and collections follow it.
    for (old, new) in gamepass::moved_ids(games, &catalog) {
        favorites::rename(&data, &old, &new);
        game_notes::rename(&data, &old, &new);
        collections::move_game(&data, &old, &new);
    }
    gamepass::merge(games, &catalog);
    if !gamepass::stale(&data) {
        return;
    }
    static ASKING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if ASKING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let (app, http) = (app.clone(), http.clone());
    tauri::async_runtime::spawn(async move {
        let fresh = gamepass::refresh(&http, &data).await;
        ASKING.store(false, std::sync::atomic::Ordering::SeqCst);
        if fresh.is_ok() {
            let _ = app.emit("catalog-changed", ());
        }
    });
}

/// The Xbox app's games get Steam's cover and backdrop: the ones found before
/// at once, the rest looked up in the background (exact name only), and the
/// library is listed again when something was found.
fn xbox_art(app: &AppHandle, http: &reqwest::Client, games: &mut [Game]) {
    let data = app.state::<DataDir>().0.clone();
    let ask = xbox::art::apply(&data, games);
    if ask.is_empty() {
        return;
    }
    static ASKING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if ASKING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let (app, http) = (app.clone(), http.clone());
    tauri::async_runtime::spawn(async move {
        let mut known = xbox::art::load(&data);
        let mut any = false;
        for (id, name) in ask {
            // Trademark signs and "for Windows" aren't in Steam's names.
            let plain: String = name.chars().filter(|c| !matches!(c, '™' | '®' | '©')).collect();
            let candidates = vec![plain.trim().to_string(), plain.replace(" for Windows", "").trim().to_string()];
            let found = steam_store::identify(&http, &candidates).await;
            any |= found.is_some();
            // A miss keeps its date: asked again in a month.
            let now = xbox::art::now();
            known.insert(id, Some(found.map_or_else(|| xbox::art::missed_now(now), |f| xbox::art::Found { cover: f.art.cover, hero: f.art.hero, missed: None })));
        }
        xbox::art::save(&data, &known);
        ASKING.store(false, std::sync::atomic::Ordering::SeqCst);
        if any {
            let _ = app.emit("catalog-changed", ());
        }
    });
}

/// With the Steam Web API on: the account's games this PC hasn't installed, from
/// the list kept on disk (asked of Steam again in the background once it's an
/// hour old, and the library is listed again when it changes).
fn owned_not_installed(app: &AppHandle, http: &reqwest::Client, games: &mut Vec<Game>) {
    let data = app.state::<DataDir>().0.clone();
    let chosen = settings::load(&data);
    let (Some(key), Some(id)) = (chosen.steam_key, chosen.steam_id) else { return };
    let file = data.join("cache").join("achievements").join("steam-api").join(format!("owned-{id}.json"));
    let owned: Vec<achievements::steam_api::Owned> = fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let covers = steam::root().map(|r| steam::cover_cache_dir(&r));
    add_owned(games, &owned, covers.as_deref());
    let old = fs::metadata(&file).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_none_or(|age| age > Duration::from_secs(3600));
    static ASKING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if old && !ASKING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        let (app, http) = (app.clone(), http.clone());
        tauri::async_runtime::spawn(async move {
            let before = owned.len();
            if let Ok(list) = achievements::steam_api::owned(&http, &data, &key, &id, true).await {
                if list.len() != before {
                    let _ = app.emit("catalog-changed", ());
                }
            }
            ASKING.store(false, std::sync::atomic::Ordering::SeqCst);
        });
    }
}

/// The account's Steam games this PC hasn't installed, added once each: one
/// installed already isn't shown again as not installed.
fn add_owned(games: &mut Vec<Game>, owned: &[achievements::steam_api::Owned], covers: Option<&Path>) {
    let installed: std::collections::HashSet<String> = games.iter().map(|g| g.id.clone()).collect();
    let mut added = std::collections::HashSet::new();
    for o in owned {
        let game_id = format!("steam:{}", o.app);
        if !installed.contains(&game_id) && !o.name.trim().is_empty() && added.insert(game_id) {
            games.push(steam::not_installed(o.app, &o.name, covers));
        }
    }
}

/// Asks for an executable and adds it to the library, with the name and cover
/// Steam has for it when it is a Steam game. `None` when the user cancels.
#[tauri::command]
async fn add_local_game(
    app: AppHandle,
    library: State<'_, LocalLibrary>,
    http: State<'_, reqwest::Client>,
) -> Result<Option<Game>, String> {
    let dialog = app.dialog().file().set_title(tr!("Escolha o executável do jogo"));
    let dialog = if cfg!(windows) { dialog.add_filter(tr!("Jogos"), &["exe"]) } else { dialog };
    let Some(exe) = pick_one(dialog).await? else {
        return Ok(None);
    };
    let mut game = library::game_from_exe(&exe);
    // Already here: that one opens, with its real data (not a new one never saved).
    if let Some(existing) = library.get(&game.id)? {
        let _ = app.emit("toast", tr!("{} já está na biblioteca.", existing.name));
        return Ok(Some(existing));
    }
    // A store's own game: it's in the library already, from the store.
    if let Some(store) = store_copy_of(&exe, &store_games(&app, &http).await) {
        return Err(tr!("{0} já está na biblioteca, pela {1}. Abra por lá: o tempo e as conquistas contam no lugar certo.", store.name, store_name(&store.source)));
    }
    // Offline or unknown to Steam: keep the guessed name; the user can edit it.
    if let Some(found) = steam_store::identify(&http, &library::name_candidates(&exe)).await {
        game.name = found.name;
        game.cover_url = found.art.cover;
        game.hero_url = found.art.hero;
    }
    library.add(game.clone())?;
    Ok(Some(game))
}

/// The installed store game whose folder holds `exe`, if any.
fn store_copy_of<'a>(exe: &Path, stores: &'a [Game]) -> Option<&'a Game> {
    let path = exe.to_string_lossy().replace('/', "\\").to_lowercase();
    stores.iter().filter(|g| !g.install_dir.trim().is_empty()).find(|g| {
        let mut dir = g.install_dir.replace('/', "\\").to_lowercase();
        while dir.ends_with('\\') {
            dir.pop();
        }
        path.starts_with(&format!("{dir}\\"))
    })
}

fn store_name(source: &Source) -> &'static str {
    match source {
        Source::Epic { .. } => "Epic",
        Source::Xbox { .. } => "Xbox",
        _ => "Steam",
    }
}

#[tauri::command(async)]
fn rename_game(library: State<'_, LocalLibrary>, id: String, name: String) -> Result<Game, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(tr!("O nome não pode ficar vazio.").into());
    }
    library.update(&id, |game| game.name = name)
}

/// Looks the game's current name up on Steam and uses that cover.
#[tauri::command]
async fn find_cover(
    library: State<'_, LocalLibrary>,
    http: State<'_, reqwest::Client>,
    folder: State<'_, CoverFolder>,
    id: String,
) -> Result<Game, String> {
    let game = library.get(&id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    let art = steam_store::art_for_name(&http, &game.name).await?;
    let Some(art) = art.filter(|art| art.cover.is_some()) else {
        return Err(tr!("Nenhuma capa encontrada na Steam para \"{}\".", game.name));
    };
    let mut old = None;
    let updated = library.update(&id, |game| {
        old = game.cover_path.take();
        game.cover_url = art.cover;
        game.hero_url = art.hero.or(game.hero_url.take());
    })?;
    forget_cover(&folder.0, old);
    Ok(updated)
}

/// Lets the user pick any image as the cover. `None` when they cancel.
#[tauri::command]
async fn pick_cover(
    app: AppHandle,
    library: State<'_, LocalLibrary>,
    folder: State<'_, CoverFolder>,
    id: String,
) -> Result<Option<Game>, String> {
    let dialog = app.dialog().file().set_title(tr!("Escolha a capa do jogo"));
    let dialog = dialog.add_filter(tr!("Imagens"), &["png", "jpg", "jpeg", "webp"]);
    let Some(source) = pick_one(dialog).await? else {
        return Ok(None);
    };
    let copy = copy_cover(&folder.0, &id, &source)?;
    let mut old = None;
    let updated = library.update(&id, |game| old = game.cover_path.replace(copy))?;
    forget_cover(&folder.0, old);
    Ok(Some(updated))
}

/// Removes a game from the library. A recomp's files are ours, so they go too;
/// anything else stays on disk, since it belongs to the user or another launcher.
#[tauri::command(async)]
fn remove_game(
    app: AppHandle,
    library: State<'_, LocalLibrary>,
    folder: State<'_, CoverFolder>,
    recomps: State<'_, recomp::Folders>,
    sessions: State<'_, sessions::Sessions>,
    installing: State<'_, recomp::Installing>,
    id: String,
    without_backup: Option<bool>,
    keep_folder: Option<bool>,
) -> Result<(), String> {
    use cerberus_shared::uninstall::{NOT_OURS, NO_BACKUP};
    let (without_backup, keep_folder) = (without_backup.unwrap_or(false), keep_folder.unwrap_or(false));
    let Some(game) = library.get(&id)? else {
        return Ok(());
    };
    if sessions.is_running_any(&id) {
        return Err(tr!("Feche o jogo antes de removê-lo.").into());
    }
    // Files first: if they can't go, the game stays listed. Saves are copied
    // before anything is deleted, and the copies outlive the uninstall.
    if let Source::Recomp { recomp_id, .. } = &game.source {
        installing.while_idle(recomp_id, || {
            if sessions.is_running_any(&id) {
                return Err(tr!("Feche o jogo antes de removê-lo.").into());
            }
            let dir = Path::new(&game.install_dir);
            // A folder the player installed on their own stays theirs: only the link goes.
            let delete = !game.external && !keep_folder;
            if delete {
                // Not a folder Cerberus made: the player chooses (keep it, only the entry goes).
                recomp::check_install_folder_pub(&recomps, recomp_id, dir).map_err(|e| format!("{NOT_OURS}{e}"))?;
                // The saves are copied before anything is deleted; if that fails, the player chooses.
                if !without_backup {
                    recomp::backup_saves(&recomps, recomp_id, dir, tk!("Antes de desinstalar")).map_err(|e| format!("{NO_BACKUP}{e}"))?;
                }
                recomp::delete_files(&recomps, recomp_id, dir)?;
            } else if !without_backup {
                // The folder stays: a backup is a bonus, never a reason to refuse.
                let _ = recomp::backup_saves(&recomps, recomp_id, dir, tk!("Antes de desinstalar"));
            }
            library.remove(&id)
        })?;
        // Said in the game's history, as every install and update is.
        let what = if !game.external && !keep_folder { tr!("Desinstalado: a pasta do jogo foi apagada") } else { tr!("Tirado da biblioteca: a pasta do jogo ficou onde estava") };
        trail::note(&app, &id, tk!("Desinstalar"), &Ok(what.to_string()));
    } else {
        library.remove(&id)?;
        trail::note(&app, &id, tk!("Desinstalar"), &Ok(tr!("Tirado da biblioteca").to_string()));
    }
    forget_cover(&folder.0, game.cover_path);
    Ok(())
}

#[tauri::command]
async fn list_recomps(
    app: AppHandle,
    library: State<'_, LocalLibrary>,
    http: State<'_, reqwest::Client>,
    art: State<'_, Arc<ArtCache>>,
) -> Result<Vec<RecompEntry>, String> {
    let cache = app.state::<DataDir>().cache();
    let first_time = !cache.join("pc-catalog.json").exists();
    if first_time {
        // Nothing on disk yet: wait for the list, falling back to the bundled one.
        let _ = recomp::catalog::refresh(&http, &cache, false).await;
    } else {
        // The copy shows now; a newer list, when due, replaces it in the background.
        let handle = app.clone();
        let client = http.inner().clone();
        tauri::async_runtime::spawn(async move {
            let cache = handle.state::<DataDir>().cache();
            // Replaced at all (new versions with the same games count too), not just grown.
            let before = recomp::catalog::all();
            let updated = recomp::catalog::refresh(&client, &cache, false).await;
            if updated.is_ok() && !std::sync::Arc::ptr_eq(&before, &recomp::catalog::all()) {
                let _ = handle.emit("catalog-changed", ());
            }
        });
    }
    let mut entries = recomp::entries(&library, &owned_games(&app))?;
    // The source lists' tens of thousands of games stay here: Discover asks
    // for a page of them (search_lists). The ones installed come, the library needs them.
    // So do the ones downloading, paused or waiting: Downloads shows them by name.
    let busy: std::collections::HashSet<String> = app
        .state::<recomp::Installing>()
        .busy_ids()
        .into_iter()
        .chain(downloads::waiting(&app.state::<DataDir>().0).into_iter().map(|p| p.recomp_id))
        .collect();
    entries.retain(|e| !e.from_source || e.installed.is_some() || busy.contains(&e.id));
    // Copies already on disk are used, but 1900 recomps' art is not fetched
    // ahead: the window loads what it shows, and games in the library keep theirs.
    let mut not_fetched = Vec::new();
    art.use_copies_for_recomps(&mut entries, &mut not_fetched);
    Ok(entries)
}

/// A page of the source lists' games for Discover, searched and sorted here.
#[tauri::command]
async fn search_lists(
    app: AppHandle,
    library: State<'_, LocalLibrary>,
    art: State<'_, Arc<ArtCache>>,
    needle: String,
    console: String,
    owned: bool,
    sort: String,
    limit: usize,
) -> Result<cerberus_shared::CatalogPage, String> {
    let entries = recomp::entries(&library, &owned_games(&app))?;
    let mut page = recomp::search_lists(entries, &recomp::ListQuery { needle: &needle, console: &console, owned, sort: &sort, limit: limit.min(2000) });
    let mut shown: Vec<RecompEntry> = page.games.iter().map(|(e, _)| e.clone()).collect();
    let mut not_fetched = Vec::new();
    art.use_copies_for_recomps(&mut shown, &mut not_fetched);
    for ((e, _), with_art) in page.games.iter_mut().zip(shown) {
        *e = with_art;
    }
    Ok(page)
}

/// One catalog entry by its id, source lists included (a page opened for a
/// list's game the window doesn't hold).
#[tauri::command]
async fn catalog_entry(app: AppHandle, library: State<'_, LocalLibrary>, art: State<'_, Arc<ArtCache>>, id: String) -> Result<Option<RecompEntry>, String> {
    let mut found: Vec<RecompEntry> = recomp::entries(&library, &owned_games(&app))?.into_iter().filter(|e| e.id == id).collect();
    let mut not_fetched = Vec::new();
    art.use_copies_for_recomps(&mut found, &mut not_fetched);
    Ok(found.pop())
}

/// Installs or updates a recomp; progress arrives as `recomp-progress` events.
#[tauri::command]
async fn install_recomp(
    app: AppHandle,
    library: State<'_, LocalLibrary>,
    http: State<'_, reqwest::Client>,
    folders: State<'_, recomp::Folders>,
    installing: State<'_, recomp::Installing>,
    id: String,
) -> Result<Game, String> {
    let data = app.state::<DataDir>().0.clone();
    let report = |step: cerberus_shared::InstallStep| {
        // Kept on disk too, so a pause or a failure is still listed after a restart.
        downloads::note(&data, &id, &step);
        let progress = InstallProgress { recomp_id: id.clone(), step };
        let _ = app.emit("recomp-progress", progress);
    };
    // One download at a time: the others say they're waiting their turn.
    downloads::queue(&id);
    // In line: the slot free and this one first (the player can put one first).
    let permit = match downloads::try_turn(&id) {
        Some(permit) => Some(permit),
        None => {
            report(cerberus_shared::InstallStep::Queued);
            downloads::wait_turn(&id).await
        }
    };
    // Paused or cancelled while it waited: it doesn't start.
    match downloads::turn(&id) {
        Some(true) => {
            downloads::forget(&data, &id);
            return Err(cerberus_shared::CANCELLED_INSTALL.into());
        }
        Some(false) => {
            report(cerberus_shared::InstallStep::Paused { done: 0, total: 0 });
            return Err(cerberus_shared::PAUSED.into());
        }
        None => {}
    }
    let sessions = app.state::<sessions::Sessions>();
    let result = recomp::install(&http, &folders, &library, &installing, &sessions, &id, &report).await;
    drop(permit);
    let game = match result {
        Ok(game) => game,
        // Paused: the bytes wait for the next install, nothing went wrong.
        Err(message) if message == cerberus_shared::PAUSED => return Err(message),
        // Cancelled: it leaves the list, it didn't fail.
        Err(message) if message == cerberus_shared::CANCELLED_INSTALL || message == cerberus_shared::i18n::t(cerberus_shared::CANCELLED_INSTALL) => {
            downloads::forget(&data, &id);
            return Err(message);
        }
        Err(message) => {
            report(cerberus_shared::InstallStep::Failed { message: message.clone() });
            // Keyed by the library id a successful install would have had.
            trail::note(&app, &format!("recomp:{id}"), tk!("Instalação"), &Err(message.clone()));
            return Err(message);
        }
    };
    let version = match &game.source {
        Source::Recomp { version, .. } => version.clone(),
        _ => String::new(),
    };
    trail::note(&app, &game.id, tk!("Instalação"), &Ok(tr!("Versão {} instalada", version)));
    // The download may have taken a while: say it's ready even if the player is elsewhere.
    if !app.get_webview_window("main").is_some_and(|w| w.is_focused().unwrap_or(false)) {
        notify::tell(&app, notify::Kind::Installed, &tr!("{} está pronto", game.name), &tr!("Versão {} instalada. Abra o Cerberus para jogar.", version));
    }
    // Easy Setup: the player's copy was found in their ROM folders, so the recomp
    // is set up with it right away. Failing that only means setting it by hand.
    let found = owned_games(&app).remove(&id);
    if let (Some(copy), None) = (found, &game.game_copy) {
        match recomp::use_game_copy(&library, &game.id, Path::new(&copy)) {
            Ok(ready) => return Ok(ready),
            Err(e) => trail::note(&app, &game.id, tk!("Cópia do jogo"), &Err(tr!("A cópia encontrada nas suas pastas não pôde ser usada; escolha-a na página do jogo. {}", e))),
        }
    }
    // An update replaced the folder: the copy chosen before goes back in.
    match recomp::restore_copy(&library, &game) {
        Some(Ok(ready)) => return Ok(ready),
        Some(Err(e)) => trail::note(&app, &game.id, tk!("Cópia do jogo"), &Err(tr!("A cópia escolhida não pôde voltar para a pasta do jogo: {}", e))),
        None => {}
    }
    Ok(game)
}

/// Sets a recomp up with a copy of its game: the one found in the ROM folders
/// when `use_found`, else one the player picks (a folder of extracted files
/// when `folder`).
#[tauri::command]
async fn set_game_copy(app: AppHandle, library: State<'_, LocalLibrary>, id: String, use_found: bool, folder: bool) -> Result<Option<Game>, String> {
    let game = library.get(&id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    let Source::Recomp { recomp_id, .. } = &game.source else {
        return Err(tr!("Só recomps usam uma cópia do jogo.").into());
    };
    // A recomp that takes only extracted folders always gets the folder picker.
    let folder = folder || recomp::entry_for(&game).is_some_and(|e| e.copy_accepts == "folder");
    let copy = if use_found {
        let owned = owned_games(&app);
        let entry = recomp::find(recomp_id).ok_or(tr!("Esse recomp não está no catálogo."))?;
        let found = owned.get(&entry.id).cloned();
        PathBuf::from(found.ok_or(tr!("Nenhuma cópia desse jogo nas suas pastas de ROMs."))?)
    } else if folder {
        let dialog = app.dialog().file().set_title(tr!("Escolha a pasta com os arquivos do jogo"));
        let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_folder());
        let Some(picked) = picked.await.map_err(|e| e.to_string())? else {
            return Ok(None);
        };
        picked.into_path().map_err(|e| e.to_string())?
    } else {
        let dialog = app.dialog().file().set_title(tr!("Escolha sua cópia do jogo (ROM ou imagem de disco)"));
        let Some(picked) = pick_one(dialog).await? else {
            return Ok(None);
        };
        picked
    };
    // Copying or unpacking a disc image can take minutes; not on an async worker.
    // Meanwhile the game's page shows how far it is, with Cancelar.
    let total = if copy.is_dir() { recomp::tree_size(&copy) } else { fs::metadata(&copy).map(|m| m.len()).unwrap_or(0) };
    copying::begin();
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let (app, id, done) = (app.clone(), id.clone(), done.clone());
        tauri::async_runtime::spawn(async move {
            // Small copies end before the first tick: nothing to show for them.
            tokio::time::sleep(Duration::from_millis(800)).await;
            while !done.load(std::sync::atomic::Ordering::SeqCst) {
                let copied = copying::copied();
                // A compressed image unpacks to more than its file: the bar waits at the end.
                let fraction = (total > 0).then(|| (copied as f32 / total as f32).min(0.99));
                let text = tr!("Copiando sua cópia do jogo · {}", format_bytes(copied));
                let _ = app.emit("prepare-progress", cerberus_shared::PrepareProgress { game_id: id.clone(), fraction, text, finished: false, cancellable: true });
                tokio::time::sleep(Duration::from_millis(400)).await;
            }
            let _ = app.emit("prepare-progress", cerberus_shared::PrepareProgress { game_id: id, fraction: Some(1.0), text: String::new(), finished: true, cancellable: false });
        });
    }
    let handle = app.clone();
    let game_id = id.clone();
    let setup = tauri::async_runtime::spawn_blocking(move || {
        let _awake = awake::hold();
        recomp::use_game_copy(&handle.state::<LocalLibrary>(), &game_id, &copy)
    });
    let result = setup.await.map_err(|e| e.to_string());
    done.store(true, std::sync::atomic::Ordering::SeqCst);
    copying::finish();
    match result? {
        Err(e) if copying::was_stopped(&e) => Err(tr!("Cópia cancelada.").into()),
        other => other.map(Some),
    }
}

/// Stops the copy of the player's game going on now.
#[tauri::command(async)]
fn cancel_copy() {
    copying::stop();
}

/// "1,2 GB", "350 MB".
fn format_bytes(bytes: u64) -> String {
    if bytes >= 1 << 30 {
        format!("{:.1} GB", bytes as f64 / (1u64 << 30) as f64)
    } else {
        format!("{} MB", bytes >> 20)
    }
}

/// Where a recomp's mods come from.
enum ModHost {
    Thunderstore(mods::ModSource),
    /// The game's id on GameBanana, and how the game reads mods.
    GameBanana(u64, gamebanana::Kind),
}

/// The mod source and mods folder of an installed recomp; `None` when it has no mods.
async fn mod_setup(app: &AppHandle, id: &str) -> Result<Option<(ModHost, PathBuf, PathBuf)>, String> {
    let library = app.state::<LocalLibrary>();
    let game = library.get(id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    let Source::Recomp { recomp_id, exe, .. } = &game.source else {
        return Ok(None);
    };
    let record = app.state::<DataDir>().0.join("mods").join(format!("{recomp_id}.json"));
    // The developer's own file says where its mods come from and go.
    if let Some(own) = recomp::entry_for(&game).and_then(|e| e.manifest).and_then(|m| m.mods.clone()) {
        let install = Path::new(&game.install_dir);
        if let (Some(community), Some(data)) = (own.thunderstore.filter(|c| mods::plain(c)), own.data.clone().filter(|d| mods::plain(d))) {
            let folder = mods::mods_dir(install, &data);
            return Ok(Some((ModHost::Thunderstore(mods::ModSource { community, data }), folder, record)));
        }
        if let Some(game_id) = own.gamebanana {
            let beside = gamebanana::mods_dir(Path::new(exe));
            match own.loader.as_deref() {
                Some("otr") => return Ok(Some((ModHost::GameBanana(game_id, gamebanana::Kind::Packs), beside, record))),
                Some("nrm") => {
                    if let Some(data) = own.data.filter(|d| mods::plain(d)) {
                        return Ok(Some((ModHost::GameBanana(game_id, gamebanana::Kind::Packs), mods::mods_dir(install, &data), record)));
                    }
                }
                Some("hmm") => {
                    if let Some(user) = own.user.filter(|u| mods::plain(u)) {
                        return Ok(Some((ModHost::GameBanana(game_id, gamebanana::Kind::Hedge(gamebanana::named(&user))), beside, record)));
                    }
                }
                _ => {}
            }
        }
    }
    if let Some((game_id, kind)) = gamebanana::game_for(recomp_id) {
        return Ok(Some((ModHost::GameBanana(game_id, kind), gamebanana::mods_dir(Path::new(exe)), record)));
    }
    let name = recomp::find(recomp_id).map(|e| e.name).unwrap_or_else(|| game.name.clone());
    let http = app.state::<reqwest::Client>().inner().clone();
    let cache = app.state::<DataDir>().cache().join("mods");
    let communities = mods::communities(&http, &cache).await;
    let Some(source) = mods::source_for(recomp_id, &name, Path::new(exe), &communities) else {
        return Ok(None);
    };
    let folder = mods::mods_dir(Path::new(&game.install_dir), &source.data);
    Ok(Some((ModHost::Thunderstore(source), folder, record)))
}

#[tauri::command]
async fn list_mods(app: AppHandle, http: State<'_, reqwest::Client>, id: String) -> Result<Option<cerberus_shared::ModList>, String> {
    let Some((host, folder, record)) = mod_setup(&app, &id).await? else {
        return Ok(None);
    };
    let cache = app.state::<DataDir>().cache().join("mods");
    let installed = mods::load_installed(&record);
    let folder = folder.to_string_lossy().into_owned();
    Ok(Some(match host {
        ModHost::Thunderstore(source) => {
            let packages = mods::packages(&http, &cache, &source.community).await?;
            let items = packages.iter().map(|p| mods::item(p, &installed)).collect();
            cerberus_shared::ModList { from: format!("thunderstore.io/c/{}", source.community), community: source.community, folder, items }
        }
        ModHost::GameBanana(game, _) => {
            let listed = gamebanana::list(&http, &cache, game).await?;
            let items = listed.iter().map(|m| gamebanana::item(m, &installed)).collect();
            cerberus_shared::ModList { from: "gamebanana.com".into(), community: String::new(), folder, items }
        }
    }))
}

/// Installs (or updates) a mod and the mods it needs.
#[tauri::command]
async fn install_mod(app: AppHandle, http: State<'_, reqwest::Client>, id: String, full_name: String) -> Result<(), String> {
    let (host, folder, record) = mod_setup(&app, &id).await?.ok_or(tr!("Esse jogo não tem mods."))?;
    let cache = app.state::<DataDir>().cache().join("mods");
    let source = match host {
        ModHost::Thunderstore(source) => source,
        ModHost::GameBanana(game, kind) => {
            let mod_id = gamebanana::id_of(&full_name).ok_or(tr!("Mod desconhecido."))?;
            let listed = gamebanana::list(&http, &cache, game).await?;
            let found = listed.iter().find(|m| m.id == mod_id);
            let version = found.map(|m| m.version.clone()).unwrap_or_default();
            let name = found.map(|m| m.name.clone()).unwrap_or_default();
            let file = gamebanana::file_of(&http, mod_id).await?;
            let archive = gamebanana::download(&http, &file, &app.state::<DataDir>().0.join("downloads").join("mods")).await?;
            let mods_folder = folder.clone();
            let mut installed = mods::load_installed(&record);
            let old = installed.0.get(&full_name).map(|(_, files)| files.clone()).unwrap_or_default();
            let old_files = old.clone();
            let others = installed.others_files(&full_name);
            let placed = tauri::async_runtime::spawn_blocking(move || match kind {
                gamebanana::Kind::Packs => gamebanana::place_packs(&archive, &mods_folder, &old_files),
                gamebanana::Kind::Hedge(user) => hedge::place(&archive, &mods_folder, user, mod_id, &name, &old_files),
            })
            .await
            .map_err(|e| e.to_string())??;
            // Packs the new version no longer has go; the ones it has were just replaced.
            if kind == gamebanana::Kind::Packs {
                let gone: Vec<String> = old.iter().filter(|f| !placed.contains(f)).cloned().collect();
                mods::remove_files(&folder, &gone, &others);
            }
            installed.0.insert(full_name, (version, placed));
            return mods::save_installed(&record, &installed);
        }
    };
    let packages = mods::packages(&http, &cache, &source.community).await?;
    let mut installed = mods::load_installed(&record);
    // The mod and what it needs, each once; a mod depending on itself in a loop ends here.
    let mut wanted = vec![full_name];
    let mut done: Vec<String> = Vec::new();
    while let Some(name) = wanted.pop() {
        if done.contains(&name) {
            continue;
        }
        let Some(package) = packages.iter().find(|p| p.full_name == name) else {
            // Shared libraries can live in another community; the game's page says what's missing.
            continue;
        };
        let current = installed.0.get(&name).is_some_and(|(v, _)| *v == package.version);
        if !current {
            let bytes = mods::download(&http, &package.download_url).await?;
            // The new version goes in first; only then do the old version's
            // files it no longer has go (a broken download keeps the old mod).
            let files = mods::unpack(&bytes, &folder, &installed.files_of(&name))?;
            if let Some((_, old)) = installed.0.get(&name) {
                let gone: Vec<String> = old.iter().filter(|f| !files.contains(f)).cloned().collect();
                mods::remove_files(&folder, &gone, &installed.others_files(&name));
            }
            installed.0.insert(name.clone(), (package.version.clone(), files));
            mods::save_installed(&record, &installed)?;
        }
        wanted.extend(package.dependencies.iter().cloned());
        done.push(name);
    }
    Ok(())
}

#[tauri::command]
async fn remove_mod(app: AppHandle, id: String, full_name: String) -> Result<(), String> {
    let (host, folder, record) = mod_setup(&app, &id).await?.ok_or(tr!("Esse jogo não tem mods."))?;
    let mut installed = mods::load_installed(&record);
    if let Some((_, files)) = installed.0.remove(&full_name) {
        match (host, gamebanana::id_of(&full_name)) {
            (ModHost::GameBanana(_, gamebanana::Kind::Hedge(user)), Some(mod_id)) => hedge::remove(&folder, user, mod_id, &files)?,
            _ => mods::remove_files(&folder, &files, &installed.others_files(&full_name)),
        }
        mods::save_installed(&record, &installed)?;
    }
    Ok(())
}

/// After an installer ran and the game wasn't found: the player points at its program.
#[tauri::command]
async fn locate_game(app: AppHandle, id: String) -> Result<Option<Game>, String> {
    let dialog = app.dialog().file().set_title(tr!("Onde está o jogo? Escolha o programa dele"));
    let dialog = if cfg!(windows) { dialog.add_filter(tr!("Programa"), &["exe"]) } else { dialog };
    let Some(program) = pick_one(dialog).await? else {
        return Ok(None);
    };
    let result = setup_runner::use_program(&app, &id, &program);
    let outcome = result.as_ref().map(|_| program.display().to_string()).map_err(Clone::clone);
    trail::note(&app, &id, tk!("Localizar programa"), &outcome);
    result.map(Some)
}

#[tauri::command(async)]
fn tracking_mode(library: State<'_, LocalLibrary>, id: String) -> Result<Option<String>, String> {
    Ok(library.get(&id)?.as_ref().and_then(sessions::tracking_mode))
}

#[tauri::command]
async fn pick_tracking_exe(app: AppHandle, id: String, automatic: bool) -> Result<Option<Game>, String> {
    let library = app.state::<LocalLibrary>();
    let game = library.get(&id)?.ok_or(tr!("Jogo removido da biblioteca."))?;
    if sessions::tracking_mode(&game).is_none() { return Err(tr!("A seleção de processo está indisponível para este jogo.").into()); }
    let selected = if automatic { None } else {
        let dialog = app.dialog().file().set_title(tr!("Escolha o executável que roda durante a partida")).add_filter(tr!("Programa"), &["exe"]);
        let Some(path) = pick_one(dialog).await? else { return Ok(None) };
        let path = dunce::canonicalize(path).map_err(|e| e.to_string())?;
        if !path.is_file() || !path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("exe")) {
            return Err(tr!("Escolha o arquivo .exe do jogo.").into());
        }
        Some(path.to_string_lossy().into_owned())
    };
    library.update(&id, |g| g.tracking_exe = selected).map(Some)
}

#[tauri::command(async)]
fn set_launch_args(library: State<'_, LocalLibrary>, id: String, args: String) -> Result<Game, String> {
    let args = args.trim().to_string();
    library.update(&id, |game| game.launch_args = args)
}

/// Opens the "Pedir um recomp" form in Cerberus's repository: how players
/// suggest projects for the catalog, with no server of our own.
#[tauri::command(async)]
fn open_recomp_request(app: AppHandle) -> Result<(), String> {
    let url = "https://github.com/rogeriojunior31/CerberusLauncher/issues/new?template=pedir-recomp.yml";
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

/// Opens SteamGridDB's page where players make their free API key.
#[tauri::command(async)]
fn open_steamgriddb_keys(app: AppHandle) -> Result<(), String> {
    app.opener().open_url("https://www.steamgriddb.com/profile/preferences/api", None::<&str>).map_err(|e| e.to_string())
}

/// Opens a catalog recomp's project page; only catalog repos, never an arbitrary link.
#[tauri::command(async)]
fn open_project(app: AppHandle, id: String) -> Result<(), String> {
    let entry = recomp::find(&id).ok_or(tr!("Esse recomp não está no catálogo."))?;
    let url = entry.forge().ok_or(tr!("O endereço do projeto no catálogo não é válido."))?.page();
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

/// The games played most recently, for the tray menu.
#[tauri::command]
fn set_tray_games(app: AppHandle, games: Vec<Game>) {
    background::set_recent(&app, games);
}

#[tauri::command(async)]
fn play(app: AppHandle, game: Game, mode: Option<String>, pad: Option<String>) -> Result<(), String> {
    // What a game reading RECOMPLAUNCHER_MODE and _PAD is told: where Play was pressed, and with what.
    recomp::manifest::set_context(mode.as_deref() == Some("bigpicture"), pad.as_deref() == Some("playstation"));
    // Store games go through their own launcher, which handles updates, DRM and
    // overlays; the rest are started, and watched, by us.
    let url = match &game.source {
        // Through Windows, as the Start menu does (Xbox app sign-in and Game Pass included).
        Source::Xbox { family, app: app_id, .. } if !game.install_dir.is_empty() => {
            return xbox::launch(family, app_id).map_err(|e| tr!("Não foi possível abrir o jogo pelo app Xbox: {}", e));
        }
        Source::Steam { .. } | Source::Epic { .. } | Source::Xbox { .. } => {
            let epic_found = !matches!(game.source, Source::Epic { .. }) || epic::launcher_found();
            match store_url(&game, epic_found) {
                Ok(url) => url,
                Err(e) if e == EPIC_MISSING => {
                    // Its download page instead of a link Windows can't open.
                    let _ = app.opener().open_url("https://store.epicgames.com/download", None::<&str>);
                    return Err(tr!("A Epic Games Launcher não está neste PC: abrimos a página para baixar. Instale, entre com sua conta e tente de novo.").into());
                }
                Err(e) => return Err(e),
            }
        }
        Source::Local { .. } => {
            let current = app.state::<LocalLibrary>().get(&game.id)?.ok_or(tr!("O jogo foi removido da biblioteca."))?;
            ready_to_play(&current)?;
            let Source::Local { exe } = &current.source else { return Err(tr!("Registro do jogo inválido.").into()) };
            return sessions::start(&app, &current, Path::new(exe));
        }
        Source::Recomp { recomp_id, .. } => {
            return app.state::<recomp::Installing>().while_idle(recomp_id, || {
                let current = app.state::<LocalLibrary>().get(&game.id)?.ok_or(tr!("O jogo foi removido da biblioteca."))?;
                ready_to_play(&current)?;
                let Source::Recomp { exe, .. } = &current.source else { return Err(tr!("Registro do recomp inválido.").into()) };
                sessions::start(&app, &current, Path::new(exe))
            });
        }
    };
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| tr!("Não foi possível abrir o launcher da loja: {}", e))
}

const EPIC_MISSING: &str = "epic-missing";

/// The address that plays or installs a store game through its own launcher.
/// Never one with an empty id: an Xbox game whose product isn't known opens
/// the store's search by its name. `epic_found`: the Epic launcher is on this PC.
fn store_url(game: &Game, epic_found: bool) -> Result<String, String> {
    let installed = !game.install_dir.is_empty();
    Ok(match &game.source {
        Source::Steam { app_id } if *app_id == 0 => return Err(tr!("Esse jogo não tem id da Steam.").into()),
        // Not on this PC yet: Steam's own install window.
        Source::Steam { app_id } if !installed => format!("steam://install/{app_id}"),
        Source::Steam { app_id } => format!("steam://rungameid/{app_id}"),
        Source::Epic { .. } if !epic_found => return Err(EPIC_MISSING.into()),
        Source::Epic { app_name, .. } if app_name.is_empty() => return Err(tr!("Esse jogo não tem id da Epic.").into()),
        // Owned, not on this PC: the Epic launcher installs it.
        Source::Epic { app_name, namespace, item_id } if !installed => epic_account::install_url(namespace, item_id, app_name),
        Source::Epic { app_name, namespace, item_id } => epic::launch_url(namespace, item_id, app_name),
        // Not on this PC: its page in the Xbox app, where it installs (Game Pass or bought).
        Source::Xbox { product, .. } if !product.is_empty() && product.chars().all(|c| c.is_ascii_alphanumeric()) => format!("msxbox://game/?productId={product}"),
        Source::Xbox { .. } => format!("ms-windows-store://search/?query={}", percent(&game.name)),
        _ => return Err(tr!("Esse jogo não é de uma loja.").into()),
    })
}

/// Where a store game is uninstalled or checked: the store's own window, or,
/// where the store has no address for it, the place to do it (Windows'
/// installed apps for the Xbox app, Epic's library).
fn store_manage_url(game: &Game, verify: bool) -> Result<String, String> {
    match (&game.source, verify) {
        (Source::Steam { app_id }, _) if *app_id == 0 => Err(tr!("Esse jogo não tem id da Steam.").into()),
        (Source::Steam { app_id }, false) => Ok(format!("steam://uninstall/{app_id}")),
        (Source::Steam { app_id }, true) => Ok(format!("steam://validate/{app_id}")),
        (Source::Xbox { .. }, false) => Ok("ms-settings:appsfeatures".into()),
        (Source::Epic { .. }, _) => Ok("com.epicgames.launcher://store/library".into()),
        _ => Err(tr!("Essa loja não verifica arquivos por aqui.").into()),
    }
}

/// Letters, digits and -_.~ as they are; everything else as %XX (UTF-8 bytes).
fn percent(text: &str) -> String {
    text.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

/// Opens the store's own place to uninstall (or, `verify`, check the files of)
/// one of its games. The store asks before removing anything.
#[tauri::command]
fn store_manage(app: AppHandle, game: Game, verify: bool) -> Result<(), String> {
    if matches!(game.source, Source::Epic { .. }) && !epic::launcher_found() {
        return Err(tr!("A Epic Games Launcher não está neste PC.").into());
    }
    let url = store_manage_url(&game, verify)?;
    app.opener().open_url(url, None::<&str>).map_err(|e| tr!("Não foi possível abrir o launcher da loja: {}", e))
}

/// Play refuses what the button wouldn't offer: a recomp without its game copy
/// would open to an error, a missing program can't open at all.
fn ready_to_play(game: &Game) -> Result<(), String> {
    match recomp::next_step(game, None) {
        // A copy is picked, but it isn't the one the game asks for: say why.
        cerberus_shared::NextStep::ChooseCopy { .. } => match recomp::copy_problem(game) {
            Some(why) => Err(tr!("A cópia escolhida não serve para {0}. {1}", game.name, why)),
            None => Err(tr!("{} precisa da sua cópia do jogo. Escolha o arquivo na página do jogo.", game.name)),
        },
        cerberus_shared::NextStep::ChooseInput { label } => Err(tr!("{0} ainda precisa de: {1}. Escolha na página do jogo, em \"O jogo também precisa\".", game.name, label)),
        cerberus_shared::NextStep::Locate { reason } => Err(tr!("{} Use \"Localizar programa\" na página do jogo.", reason)),
        _ => Ok(()),
    }
}

#[tauri::command]
fn running_games(sessions: State<'_, sessions::Sessions>) -> Vec<String> {
    sessions.running()
}

#[tauri::command(async)]
fn cancel_install(app: AppHandle, installing: State<'_, recomp::Installing>, id: String) {
    downloads::withdraw(&id, true);
    installing.cancel(&id);
    downloads::forget(&app.state::<DataDir>().0, &id);
}

/// The paused and unfinished downloads from the last run, for the Downloads page.
#[tauri::command(async)]
fn download_queue(app: AppHandle) -> Vec<InstallProgress> {
    downloads::waiting(&app.state::<DataDir>().0)
}

/// Puts a recomp waiting in the download line first.
#[tauri::command]
fn download_first(id: String) -> bool {
    downloads::put_first(&id)
}

/// A download the player dismissed from "Didn't finish".
#[tauri::command(async)]
fn forget_download(app: AppHandle, id: String) {
    downloads::forget(&app.state::<DataDir>().0, &id);
}

/// Moves an installed recomp to a folder the player picks (another drive, say).
/// `None` when the picker is cancelled.
#[tauri::command]
async fn move_game(app: AppHandle, id: String) -> Result<Option<Game>, String> {
    let game = app.state::<LocalLibrary>().get(&id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    let Source::Recomp { recomp_id, .. } = &game.source else {
        return Err(tr!("Só recomps instalados pelo Cerberus podem ser movidos por aqui.").into());
    };
    if app.state::<sessions::Sessions>().is_running_any(&id) {
        return Err(tr!("Feche o jogo antes de movê-lo.").into());
    }
    app.state::<recomp::Installing>().while_idle(recomp_id, || Ok(()))?;
    let dialog = app.dialog().file().set_title(tr!("Escolha para onde mover o jogo"));
    let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_folder());
    let Some(picked) = picked.await.map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let into = picked.into_path().map_err(|e| e.to_string())?;
    // Copying to another drive can take minutes: meanwhile the game is busy,
    // so Play, Update and Uninstall wait instead of racing the copy.
    let key = app.state::<recomp::Installing>().reserve(recomp_id)?;
    if app.state::<sessions::Sessions>().is_running_any(&id) {
        app.state::<recomp::Installing>().release(&key);
        return Err(tr!("Feche o jogo antes de movê-lo.").into());
    }
    let handle = app.clone();
    let moved = tauri::async_runtime::spawn_blocking(move || {
        recomp::relocate::move_install(&handle.state::<recomp::Folders>(), &handle.state::<LocalLibrary>(), &id, &into)
    })
    .await;
    app.state::<recomp::Installing>().release(&key);
    let moved = moved.map_err(|e| e.to_string())??;
    trail::note(&app, &moved.id, tk!("Mover"), &Ok(tr!("Movido para {}", moved.install_dir)));
    Ok(Some(moved))
}

#[tauri::command(async)]
fn pause_install(installing: State<'_, recomp::Installing>, id: String) {
    downloads::withdraw(&id, false);
    installing.pause(&id);
}

/// The recomp behind a library entry, and where it is installed.
fn recomp_of(library: &LocalLibrary, id: &str) -> Result<(String, PathBuf), String> {
    let game = library.get(id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    let Source::Recomp { recomp_id, .. } = game.source else {
        return Err(tr!("Só os recomps têm backup de saves.").into());
    };
    Ok((recomp_id, PathBuf::from(game.install_dir)))
}

#[tauri::command(async)]
fn list_backups(library: State<'_, LocalLibrary>, folders: State<'_, recomp::Folders>, id: String) -> Result<Vec<Backup>, String> {
    let (recomp_id, _) = recomp_of(&library, &id)?;
    Ok(recomp::list_backups(&folders, &recomp_id))
}

/// `None` when there was nothing new to copy.
#[tauri::command]
async fn backup_now(app: AppHandle, library: State<'_, LocalLibrary>, folders: State<'_, recomp::Folders>, id: String) -> Result<Option<Backup>, String> {
    // A game open by itself may be writing its save right now: a copy taken
    // then could hold half of it. (Sessions Cerberus started back up when they end.)
    if library.get(&id).ok().flatten().is_some_and(|g| sessions::open_outside(&g)) {
        return Err(tr!("O jogo está aberto: feche-o antes de fazer um backup, para não copiar um save pela metade.").into());
    }
    let (recomp_id, install_dir) = recomp_of(&library, &id)?;
    let result = recomp::backup_saves(&folders, &recomp_id, &install_dir, tk!("Manual"));
    trail::note(&app, &id, tk!("Backup manual"), &sessions::backup_outcome(&result));
    result
}

#[tauri::command]
async fn restore_backup(
    app: AppHandle,
    library: State<'_, LocalLibrary>,
    folders: State<'_, recomp::Folders>,
    sessions: State<'_, sessions::Sessions>,
    id: String,
    name: String,
) -> Result<u32, String> {
    // The game would overwrite the restored files with what it has in memory
    // (also when it was opened outside Cerberus, or before a restart).
    let open_now = library.get(&id).ok().flatten().is_some_and(|g| sessions::open_outside(&g));
    if sessions.is_running_any(&id) || open_now {
        return Err(tr!("Feche o jogo antes de restaurar um backup.").into());
    }
    let (recomp_id, install_dir) = recomp_of(&library, &id)?;
    let result = recomp::restore_backup(&folders, &recomp_id, &install_dir, &name);
    let outcome = result.as_ref().map(|(n, _)| tr!("{} arquivos de {} restaurados", n, name)).map_err(Clone::clone);
    trail::note(&app, &id, tk!("Restaurar backup"), &outcome);
    let (restored, warning) = result?;
    if let Some(warning) = warning {
        trail::note(&app, &id, tk!("Restaurar backup"), &Err(warning.clone()));
        let _ = app.emit("toast", warning);
    }
    Ok(restored)
}

fn found_view(found: recomp::adopt::Found) -> Option<cerberus_shared::AdoptFound> {
    let entry = recomp::find(&found.recomp_id)?;
    Some(cerberus_shared::AdoptFound {
        recomp_id: entry.id,
        name: entry.name,
        game: entry.game,
        console: entry.console,
        cover: entry.cover,
        program: found.program.to_string_lossy().into_owned(),
        sure: found.sure,
    })
}

/// Looks through a folder the player picks for recomps installed without
/// Cerberus. `None` when the picker is cancelled.
#[tauri::command]
async fn scan_for_recomps(app: AppHandle) -> Result<Option<cerberus_shared::AdoptScan>, String> {
    let dialog = app.dialog().file().set_title(tr!("Em que pasta estão os recomps que você já instalou?"));
    let Some(folder) = pick_folder(dialog).await? else {
        return Ok(None);
    };
    let shown = folder.display().to_string();
    let found = tauri::async_runtime::spawn_blocking(move || recomp::adopt::scan(&folder, &recomp::catalog::all()))
        .await
        .map_err(|e| e.to_string())?;
    Ok(Some(cerberus_shared::AdoptScan { folder: shown, found: found.into_iter().filter_map(found_view).collect() }))
}

/// Brings one recomp in, from its program: one found by the scan, or (with no
/// `program`) one the player picks.
#[tauri::command]
async fn adopt_recomp(app: AppHandle, recomp_id: String, program: Option<String>) -> Result<Option<Game>, String> {
    let entry = recomp::find(&recomp_id).ok_or(tr!("Esse recomp não está no catálogo."))?;
    let program = match program {
        Some(path) => PathBuf::from(path),
        None => {
            let title = tr!("Onde está o {}? Escolha o programa dele", entry.name);
            let dialog = app.dialog().file().set_title(&title);
            let dialog = if cfg!(windows) { dialog.add_filter(tr!("Programa"), &["exe"]) } else { dialog };
            let Some(program) = pick_one(dialog).await? else {
                return Ok(None);
            };
            program
        }
    };
    let library = app.state::<LocalLibrary>();
    let result = recomp::adopt::adopt(&library, &entry, &program);
    let outcome = result.as_ref().map(|_| tr!("Trazido de {}", program.display())).map_err(Clone::clone);
    trail::note(&app, &recomp::library_id(&entry.id), tk!("Trazer recomp"), &outcome);
    let _ = app.emit("catalog-changed", ());
    result.map(Some)
}

/// The catalog recomps a game added by hand could be, the surest first.
#[tauri::command(async)]
fn recomp_suggestions(library: State<'_, LocalLibrary>, id: String) -> Result<Vec<cerberus_shared::AdoptFound>, String> {
    let game = library.get(&id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    let Source::Local { exe } = &game.source else { return Ok(Vec::new()) };
    let found = recomp::adopt::candidates(Path::new(exe), &recomp::catalog::all());
    Ok(found.into_iter().filter_map(found_view).collect())
}

/// Turns a game added by hand into the catalog recomp it is, keeping its play time.
#[tauri::command(async)]
fn link_to_recomp(app: AppHandle, library: State<'_, LocalLibrary>, id: String, recomp_id: String) -> Result<Game, String> {
    let game = library.get(&id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    let Source::Local { exe } = &game.source else { return Err(tr!("Só jogos adicionados à mão podem ser vinculados.").into()) };
    let entry = recomp::find(&recomp_id).ok_or(tr!("Esse recomp não está no catálogo."))?;
    let linked = recomp::adopt::adopt(&library, &entry, Path::new(exe))?;
    trail::note(&app, &linked.id, tk!("Vincular ao catálogo"), &Ok(entry.name.clone()));
    let _ = app.emit("catalog-changed", ());
    Ok(linked)
}

/// A recomp's catalog settings offer and where it is installed.
fn settings_of(library: &LocalLibrary, id: &str) -> Result<(Game, game_settings::Offer), String> {
    let game = library.get(id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    let Source::Recomp { recomp_id, .. } = &game.source else {
        return Err(tr!("Só recomps têm ajustes do catálogo.").into());
    };
    let _ = recomp_id;
    let offer = recomp::entry_for(&game).map(|e| e.settings).unwrap_or_default();
    Ok((game, offer))
}

/// The setup step's ticks (a vendor's SDK), each on or off.
#[tauri::command(async)]
fn prepare_options(library: State<'_, LocalLibrary>, id: String) -> Result<Vec<cerberus_shared::PrepareOption>, String> {
    let game = library.get(&id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    Ok(recomp::prepare_options(&game))
}

#[tauri::command(async)]
fn set_prepare_option(library: State<'_, LocalLibrary>, id: String, option: String, on: bool) -> Result<Vec<cerberus_shared::PrepareOption>, String> {
    let game = recomp::set_prepare_option(&library, &id, &option, on)?;
    Ok(recomp::prepare_options(&game))
}

/// Opens the license a setup tick asks the player to accept (an https address from the game's file).
#[tauri::command(async)]
fn open_prepare_license(app: AppHandle, library: State<'_, LocalLibrary>, id: String, option: String) -> Result<(), String> {
    let game = library.get(&id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    let (_, url) = recomp::prepare_options(&game).into_iter().find(|o| o.id == option).and_then(|o| o.license).ok_or(tr!("Essa opção não tem licença."))?;
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

/// The game's setup step hasn't run yet (or must run again).
#[tauri::command(async)]
fn preparation_pending(app: AppHandle, library: State<'_, LocalLibrary>, id: String) -> Result<bool, String> {
    let game = library.get(&id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    Ok(!app.state::<sessions::Sessions>().is_running(&id) && recomp::needs_preparation(&game))
}

/// Runs the setup step now, without starting the game after.
#[tauri::command(async)]
fn prepare_game(app: AppHandle, library: State<'_, LocalLibrary>, id: String) -> Result<bool, String> {
    let game = library.get(&id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    sessions::prepare_now(&app, &game)
}

/// The buttons the game's developer adds under its settings.
#[tauri::command(async)]
fn game_actions(library: State<'_, LocalLibrary>, id: String) -> Result<Vec<cerberus_shared::GameAction>, String> {
    let game = library.get(&id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    Ok(recomp::game_actions(&game))
}

/// Opens the folder an action names, or empties it (the page asked first).
#[tauri::command(async)]
fn run_game_action(app: AppHandle, library: State<'_, LocalLibrary>, id: String, action: String) -> Result<(), String> {
    let game = library.get(&id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    if app.state::<sessions::Sessions>().is_running(&id) {
        return Err(tr!("Feche o jogo antes.").into());
    }
    let (folder, clear) = recomp::action_folder(&game, &action)?;
    if clear {
        return recomp::clear_folder(&folder);
    }
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    app.opener().open_path(folder.to_string_lossy(), None::<&str>).map_err(|e| tr!("Não foi possível abrir a pasta: {}", e))
}

/// The files besides the copy the game's `recomplauncher.json` asks for:
/// other discs, DLC, an update, keys.
#[tauri::command(async)]
fn game_inputs(library: State<'_, LocalLibrary>, id: String) -> Result<Vec<cerberus_shared::SetupInput>, String> {
    let game = library.get(&id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    Ok(recomp::setup_inputs(&game))
}

/// Picks the file (or folder) for one of those inputs; `clear` takes it away.
#[tauri::command]
async fn pick_game_input(app: AppHandle, library: State<'_, LocalLibrary>, id: String, input: String, clear: bool) -> Result<Option<Game>, String> {
    // A shared key is used by other games too: never swapped while one plays.
    let shared = library.get(&id)?.map(|g| recomp::setup_inputs(&g)).unwrap_or_default().into_iter().any(|i| i.id == input && i.shared);
    if shared && !app.state::<sessions::Sessions>().running().is_empty() {
        return Err(tr!("Feche os jogos abertos antes de trocar uma chave compartilhada.").into());
    }
    if clear {
        return recomp::use_input(&library, &id, &input, None).map(Some);
    }
    let game = library.get(&id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    let wanted = recomp::setup_inputs(&game).into_iter().find(|i| i.id == input).ok_or(tr!("Esse jogo não pede esse arquivo."))?;
    let dialog = app.dialog().file().set_title(wanted.label.clone());
    let picked = if wanted.folder { pick_folder(dialog).await? } else { pick_one(dialog).await? };
    let Some(picked) = picked else { return Ok(None) };
    // Checking a disc or copying an update can take a while: not on an async worker.
    let handle = app.clone();
    let done = tauri::async_runtime::spawn_blocking(move || recomp::use_input(&handle.state::<LocalLibrary>(), &id, &input, Some(&picked)));
    done.await.map_err(|e| e.to_string())?.map(Some)
}

/// The game's own settings and the choice each is on. `None` when the catalog offers none.
#[tauri::command(async)]
fn game_settings(library: State<'_, LocalLibrary>, id: String) -> Result<Option<cerberus_shared::GameSettings>, String> {
    let (game, offer) = settings_of(&library, &id)?;
    if offer.is_empty() {
        return Ok(None);
    }
    Ok(Some(offer.view(Path::new(&game.install_dir), &game.setting_picks)))
}

/// Puts one setting on a choice (`choice: None` puts every setting back on the
/// game's defaults), and returns the settings as they are now.
#[tauri::command(async)]
fn set_game_setting(
    app: AppHandle,
    library: State<'_, LocalLibrary>,
    sessions: State<'_, sessions::Sessions>,
    id: String,
    setting: Option<String>,
    choice: Option<usize>,
    value: Option<f64>,
) -> Result<cerberus_shared::GameSettings, String> {
    // The game writes its settings when it closes, over whatever was changed meanwhile.
    if sessions.is_running(&id) {
        return Err(tr!("Feche o jogo antes de mudar os ajustes.").into());
    }
    let (game, offer) = settings_of(&library, &id)?;
    let install = Path::new(&game.install_dir);
    let mut picks = game.setting_picks.clone();
    let result = match (&setting, choice, value) {
        (Some(setting), _, Some(value)) => offer.set_number(install, setting, value),
        (Some(setting), Some(choice), None) => offer.choose(install, &mut picks, setting, choice),
        _ => offer.reset(install, &mut picks),
    };
    if let Err(error) = &result {
        trail::note(&app, &id, tk!("Ajustes"), &Err(error.clone()));
    }
    result?;
    if picks != game.setting_picks {
        library.update(&id, |g| g.setting_picks = picks.clone())?;
    }
    let updated = library.get(&id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    Ok(offer.view(install, &updated.setting_picks))
}

/// What the launcher did for a game lately, newest first.
/// Marks or unmarks a game as a favorite.
/// Pauses (or resumes) a recomp's updates: it stays on its version meanwhile.
#[tauri::command(async)]
fn set_updates_paused(app: AppHandle, id: String, on: bool) -> Result<(), String> {
    let data = app.state::<DataDir>().0.clone();
    settings::update(&data, |s| {
        if on {
            s.paused_updates.insert(id);
        } else {
            s.paused_updates.remove(&id);
        }
    })
}

/// The player's status, rating and notes for a game.
#[tauri::command(async)]
fn set_game_note(app: AppHandle, id: String, status: Option<cerberus_shared::GameStatus>, rating: u8, notes: String) -> Result<(), String> {
    game_notes::set(&app.state::<DataDir>().0, &id, game_notes::Note { status, rating, notes })
}

/// Status, rating, notes and favorite for an id that isn't a library game (a classic).
#[tauri::command(async)]
fn player_note(app: AppHandle, id: String) -> cerberus_shared::PlayerNote {
    let data = &app.state::<DataDir>().0;
    let note = game_notes::load(data).remove(&id).unwrap_or_default();
    cerberus_shared::PlayerNote { status: note.status, rating: note.rating, notes: note.notes, favorite: favorites::load(data).contains(&id) }
}

#[tauri::command(async)]
fn set_favorite(app: AppHandle, id: String, on: bool) -> Result<(), String> {
    favorites::set(&app.state::<DataDir>().0, &id, on)
}

/// Hides a game from the library, or shows it again.
#[tauri::command(async)]
fn set_hidden(app: AppHandle, id: String, on: bool) -> Result<(), String> {
    collections::set_hidden(&app.state::<DataDir>().0, &id, on)
}

/// The player's collections, in the order they were made.
#[tauri::command(async)]
fn list_collections(app: AppHandle) -> Vec<String> {
    collections::names(&app.state::<DataDir>().0)
}

#[tauri::command(async)]
fn create_collection(app: AppHandle, name: String) -> Result<String, String> {
    collections::create(&app.state::<DataDir>().0, &name)
}

/// Puts a game in a collection (made when new) or takes it out.
#[tauri::command(async)]
fn set_in_collection(app: AppHandle, name: String, id: String, on: bool) -> Result<(), String> {
    collections::set_in(&app.state::<DataDir>().0, &name, &id, on)
}

#[tauri::command(async)]
fn rename_collection(app: AppHandle, old: String, name: String) -> Result<String, String> {
    collections::rename(&app.state::<DataDir>().0, &old, &name)
}

#[tauri::command(async)]
fn delete_collection(app: AppHandle, name: String) -> Result<(), String> {
    collections::delete(&app.state::<DataDir>().0, &name)
}

/// Opens the folder holding a classic; only files from the player's ROM folders.
#[tauri::command(async)]
fn open_rom_folder(app: AppHandle, path: String) -> Result<(), String> {
    let rom = PathBuf::from(&path);
    if !app.state::<RomIndex>().get().iter().any(|r| r.path == rom) {
        return Err(tr!("Esse arquivo não está nas suas pastas de ROMs.").into());
    }
    let folder = rom.parent().ok_or(tr!("Esse arquivo não tem pasta."))?;
    app.opener()
        .open_path(folder.to_string_lossy(), None::<&str>)
        .map_err(|e| tr!("Não foi possível abrir a pasta: {}", e))
}

/// A game asked for by a desktop shortcut (`--play <id>`) when Cerberus started.
#[derive(Default)]
struct PlayRequest(std::sync::Mutex<Option<String>>);

/// The game a shortcut asked for at start, once: the window plays it when ready.
#[tauri::command]
fn take_play_request(request: State<'_, PlayRequest>) -> Option<String> {
    request.0.lock().ok().and_then(|mut r| r.take())
}

/// Puts a shortcut on the desktop that plays this game through Cerberus.
#[tauri::command(async)]
fn create_shortcut(app: AppHandle, library: State<'_, LocalLibrary>, id: String) -> Result<String, String> {
    let game = match library.get(&id)? {
        Some(game) => game,
        None => {
            let mut found = steam::installed_games();
            found.extend(epic::installed_games());
            found.extend(xbox::installed_games());
            found.into_iter().find(|g| g.id == id).ok_or(tr!("Esse jogo não está mais na biblioteca."))?
        }
    };
    let icon = match &game.source {
        Source::Recomp { exe, .. } | Source::Local { exe } => Some(PathBuf::from(exe)),
        _ => None,
    };
    let desktop = app.path().desktop_dir().map_err(|e| e.to_string())?;
    let link = shortcut::create(&desktop, &game.name, &game.id, icon.as_deref())?;
    trail::note(&app, &game.id, tk!("Atalho"), &Ok(link.display().to_string()));
    Ok(link.display().to_string())
}

/// Any game in the library, Steam's and Epic's included.
fn any_game(library: &LocalLibrary, id: &str) -> Result<Game, String> {
    if let Some(game) = library.get(id)? {
        return Ok(game);
    }
    let mut found = steam::installed_games();
    found.extend(epic::installed_games());
    found.extend(xbox::installed_games());
    found.into_iter().find(|g| g.id == id).ok_or_else(|| tr!("Esse jogo não está mais na biblioteca.").to_string())
}

/// The folder a game's DLSS files are looked for in; none for recomps (they don't carry DLSS).
fn dlss_dir(game: &Game) -> Option<PathBuf> {
    if matches!(game.source, Source::Recomp { .. }) || game.install_dir.len() <= 3 {
        return None;
    }
    let dir = PathBuf::from(&game.install_dir);
    dir.is_dir().then_some(dir)
}

#[tauri::command]
async fn dlss_list(app: AppHandle, id: String) -> Result<Vec<cerberus_shared::DlssFile>, String> {
    let data = app.state::<DataDir>().0.clone();
    let game = any_game(&app.state::<LocalLibrary>(), &id)?;
    let Some(dir) = dlss_dir(&game) else { return Ok(Vec::new()) };
    tauri::async_runtime::spawn_blocking(move || dlss::list(&data, &id, &dir)).await.map_err(|e| e.to_string())
}

/// Puts a DLSS file the player picks in place of the game's; `None` when cancelled.
#[tauri::command]
async fn dlss_swap(app: AppHandle, id: String, path: String) -> Result<Option<String>, String> {
    let game = any_game(&app.state::<LocalLibrary>(), &id)?;
    let dir = dlss_dir(&game).ok_or(tr!("Esse jogo não tem DLSS."))?;
    let name = Path::new(&path).file_name().unwrap_or_default().to_string_lossy().into_owned();
    let dialog = app.dialog().file().set_title(tr!("Escolha o {} da versão que quer usar", name)).add_filter("DLSS", &["dll"]);
    let Some(picked) = pick_one(dialog).await? else { return Ok(None) };
    let data = app.state::<DataDir>().0.clone();
    let result = dlss::swap(&data, &id, &dir, &path, &picked);
    trail::note(&app, &id, tk!("Trocar DLSS"), &result.as_ref().map(|v| tr!("{} agora na versão {}", name, v)).map_err(|e| e.clone()));
    result.map(Some)
}

#[tauri::command(async)]
fn dlss_restore(app: AppHandle, id: String, path: String) -> Result<(), String> {
    let game = any_game(&app.state::<LocalLibrary>(), &id)?;
    let dir = dlss_dir(&game).ok_or(tr!("Esse jogo não tem DLSS."))?;
    let result = dlss::restore(&app.state::<DataDir>().0, &id, &dir, &path);
    trail::note(&app, &id, tk!("DLSS original de volta"), &result.as_ref().map(|_| path.clone()).map_err(|e| e.clone()));
    result
}

/// The game for a Steam shortcut: only games Steam doesn't already have.
fn game_for_steam(library: &LocalLibrary, id: &str) -> Result<Game, String> {
    let game = match library.get(id)? {
        Some(game) => game,
        None => epic::installed_games().into_iter().find(|g| g.id == id).ok_or(tr!("Esse jogo não está mais na biblioteca."))?,
    };
    if matches!(game.source, Source::Steam { .. }) {
        return Err(tr!("Esse jogo já é da Steam.").into());
    }
    Ok(game)
}

/// Steam rewrites its list of games when it closes, undoing any change made meanwhile.
fn steam_closed() -> Result<PathBuf, String> {
    let steam = steam::root().ok_or(tr!("A Steam não está instalada neste PC."))?;
    let running = store_time::running_programs().iter().any(|p| Path::new(p).file_name().is_some_and(|n| n.eq_ignore_ascii_case("steam.exe")));
    if running {
        return Err(tr!("Feche a Steam primeiro (Steam › Sair): ela regrava a lista de jogos quando fecha e desfaria a mudança.").into());
    }
    Ok(steam)
}

/// A picture's bytes: the file on disk, the cached copy, or the online address.
async fn art_bytes(http: &reqwest::Client, art: &ArtCache, path: Option<&str>, url: Option<&str>) -> Option<Vec<u8>> {
    let local = path.filter(|p| !p.starts_with("http")).map(PathBuf::from).or_else(|| url.and_then(|u| art.copy_of(u)));
    if let Some(file) = local.filter(|f| f.is_file()) {
        return std::fs::read(file).ok();
    }
    let url = url.filter(|u| u.starts_with("https://"))?;
    let reply = http.get(url).timeout(std::time::Duration::from_secs(30)).send().await.ok()?;
    if !reply.status().is_success() {
        return None;
    }
    reply.bytes().await.ok().map(|b| b.to_vec())
}

/// Puts the game in the Steam library as a non-Steam game, with its art.
#[tauri::command]
async fn add_to_steam(app: AppHandle, http: State<'_, reqwest::Client>, id: String) -> Result<String, String> {
    let game = game_for_steam(&app.state::<LocalLibrary>(), &id)?;
    let steam = steam_closed()?;
    let configs = steam_shortcuts::config_dirs(&steam);
    if configs.is_empty() {
        return Err(tr!("Entre na Steam uma vez antes: ela ainda não tem uma conta neste PC.").into());
    }
    put_in_steam(&app, &http, &configs, &game).await?;
    Ok(tr!("{} está na Steam. Ele aparece na biblioteca quando a Steam abrir.", game.name))
}

/// One game as a Steam shortcut, with its cover and background, in every Steam account here.
async fn put_in_steam(app: &AppHandle, http: &reqwest::Client, configs: &[PathBuf], game: &Game) -> Result<(), String> {
    let icon = match &game.source {
        Source::Recomp { exe, .. } | Source::Local { exe } => Some(PathBuf::from(exe)),
        _ => None,
    };
    let launcher = std::env::current_exe().map_err(|e| e.to_string())?;
    let shortcut = steam_shortcuts::Shortcut { name: game.name.clone(), launcher, game_id: game.id.clone(), icon };
    let art = app.state::<Arc<ArtCache>>().inner().clone();
    let cover = art_bytes(http, &art, game.cover_path.as_deref(), game.cover_url.as_deref()).await;
    let hero = art_bytes(http, &art, game.hero_path.as_deref(), game.hero_url.as_deref()).await;
    let mut pictures = 0;
    for config in configs {
        let mut list = steam_shortcuts::load(config)?;
        let old = steam_shortcuts::add(&mut list, &shortcut);
        steam_shortcuts::save(config, &list)?;
        // Renamed since: the pictures under the old number would stay orphaned.
        for number in old {
            steam_shortcuts::remove_art(config, number);
        }
        pictures += steam_shortcuts::place_art(config, shortcut.app_id(), cover.as_deref(), hero.as_deref());
    }
    let said = if pictures == 0 { tr!("Adicionado à Steam, sem arte: nenhuma imagem do jogo pôde ser copiada").to_string() } else { tr!("Adicionado à Steam").to_string() };
    trail::note(app, &game.id, tk!("Steam"), &Ok(said));
    Ok(())
}

/// Every installed game that isn't Steam's and isn't there yet (hidden ones
/// aside) goes into Steam. Returns how many were added.
#[tauri::command]
async fn add_all_to_steam(app: AppHandle, http: State<'_, reqwest::Client>) -> Result<usize, String> {
    let steam = steam_closed()?;
    let configs = steam_shortcuts::config_dirs(&steam);
    if configs.is_empty() {
        return Err(tr!("Entre na Steam uma vez antes: ela ainda não tem uma conta neste PC.").into());
    }
    let mut games = app.state::<LocalLibrary>().games()?;
    games.extend(epic::installed_games());
    // Hidden is the player's choice, kept apart from the library: applied here,
    // or every hidden game would go to Steam.
    collections::apply(&app.state::<DataDir>().0, &mut games);
    let already = |id: &str| configs.iter().any(|c| steam_shortcuts::load(c).is_ok_and(|list| steam_shortcuts::contains(&list, id)));
    let mut added = 0;
    for game in games {
        if game.hidden || game.install_dir.is_empty() || matches!(game.source, Source::Steam { .. }) || already(&game.id) {
            continue;
        }
        put_in_steam(&app, &http, &configs, &game).await?;
        added += 1;
    }
    Ok(added)
}

#[tauri::command(async)]
fn remove_from_steam(app: AppHandle, id: String) -> Result<(), String> {
    let steam = steam_closed()?;
    for config in steam_shortcuts::config_dirs(&steam) {
        let mut list = steam_shortcuts::load(&config)?;
        let gone = steam_shortcuts::remove(&mut list, &id);
        if !gone.is_empty() {
            steam_shortcuts::save(&config, &list)?;
            for app_id in gone {
                steam_shortcuts::remove_art(&config, app_id);
            }
        }
    }
    trail::note(&app, &id, tk!("Steam"), &Ok(tr!("Tirado da Steam").to_string()));
    Ok(())
}

/// Whether Cerberus put this game in Steam.
#[tauri::command(async)]
fn in_steam(id: String) -> bool {
    let Some(steam) = steam::root() else { return false };
    steam_shortcuts::config_dirs(&steam).iter().any(|config| steam_shortcuts::load(config).is_ok_and(|list| steam_shortcuts::contains(&list, &id)))
}

fn pc_now() -> cerberus_shared::PcStatus {
    let vc_version = runtimes::vc_version();
    let vc_ok = runtimes::vc_ok();
    cerberus_shared::PcStatus { vc_version, vc_ok, dx_missing: runtimes::dx_missing() }
}

/// What the PC has that games need.
#[tauri::command(async)]
fn pc_status() -> cerberus_shared::PcStatus {
    pc_now()
}

/// Installs or updates the Visual C++ runtime from Microsoft.
#[tauri::command]
async fn install_vc(app: AppHandle, http: State<'_, reqwest::Client>) -> Result<cerberus_shared::PcStatus, String> {
    let downloads = app.state::<DataDir>().0.join("downloads");
    let installed = runtimes::install_vc(&http, &downloads).await;
    trail::note(&app, "pc", tk!("Visual C++"), &installed.clone().map(|v| v.unwrap_or_else(|| tr!("Instalado").into())));
    installed?;
    Ok(pc_now())
}

/// Installs DirectX's older parts from Microsoft.
#[tauri::command]
async fn install_dx(app: AppHandle, http: State<'_, reqwest::Client>) -> Result<cerberus_shared::PcStatus, String> {
    let downloads = app.state::<DataDir>().0.join("downloads");
    let installed = runtimes::install_dx(&http, &downloads).await;
    trail::note(&app, "pc", tk!("DirectX"), &installed.clone().map(|still| if still.is_empty() { tr!("Instalado").into() } else { tr!("Ainda falta: {}", still.join(", ")) }));
    installed?;
    Ok(pc_now())
}

/// What happened lately across every game, newest first.
#[tauri::command(async)]
fn recent_activity(trail: State<'_, trail::Trail>) -> Vec<cerberus_shared::TrailEntry> {
    trail.recent(40)
}

#[tauri::command(async)]
fn game_trail(trail: State<'_, trail::Trail>, id: String) -> Vec<cerberus_shared::TrailEntry> {
    trail.for_game(&id, 30)
}

/// The pictures taken of a game, newest first.
#[tauri::command(async)]
fn list_screenshots(app: AppHandle, id: String) -> Vec<cerberus_shared::Screenshot> {
    screenshots::list(&app.state::<DataDir>().0, &id)
}

/// Opens one picture in the PC's image viewer; only pictures Cerberus took.
#[tauri::command(async)]
fn open_screenshot(app: AppHandle, path: String) -> Result<(), String> {
    if !screenshots::is_ours(&app.state::<DataDir>().0, Path::new(&path)) {
        return Err(tr!("Essa foto não é do Cerberus.").into());
    }
    app.opener().open_path(path, None::<&str>).map_err(|e| tr!("Não foi possível abrir a foto: {}", e))
}

#[tauri::command(async)]
fn open_screenshots_folder(app: AppHandle, id: String) -> Result<(), String> {
    let dir = screenshots::dir_of(&app.state::<DataDir>().0, &id);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|e| tr!("Não foi possível abrir a pasta: {}", e))
}

#[tauri::command(async)]
fn open_folder(app: AppHandle, library: State<'_, LocalLibrary>, id: String) -> Result<(), String> {
    let game = library.get(&id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    app.opener()
        .open_path(game.install_dir, None::<&str>)
        .map_err(|e| tr!("Não foi possível abrir a pasta: {}", e))
}

#[tauri::command(async)]
fn open_backups(app: AppHandle, library: State<'_, LocalLibrary>, folders: State<'_, recomp::Folders>, id: String) -> Result<(), String> {
    let (recomp_id, _) = recomp_of(&library, &id)?;
    let dir = folders.backups_dir(&recomp_id);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| tr!("Não foi possível abrir a pasta: {}", e))
}

/// The emulators on this PC (RetroArch and the dedicated ones) with where they
/// keep saves and the copies Cerberus made of them.
fn emulators_here(data: &Path) -> Vec<(&'static str, &'static str, PathBuf)> {
    let chosen = settings::load(data);
    let mut found = Vec::new();
    if let Some(exe) = emulation::retroarch(data, chosen.retroarch.as_deref()) {
        found.push(("retroarch", "RetroArch", exe));
    }
    for emulator in standalone::ALL {
        if let Some((exe, _)) = standalone::locate(data, chosen.emulators.get(emulator.id).map(PathBuf::as_path), emulator) {
            found.push((emulator.id, emulator.name, exe));
        }
    }
    found
}

#[tauri::command(async)]
fn emulator_backups(app: AppHandle) -> Vec<cerberus_shared::EmulatorBackups> {
    let data = app.state::<DataDir>().0.clone();
    emulators_here(&data)
        .into_iter()
        .filter_map(|(id, name, exe)| {
            let places = emulator_saves::places(id, &exe)?;
            Some(cerberus_shared::EmulatorBackups {
                id: id.to_string(),
                name: name.to_string(),
                folder: places.root.to_string_lossy().into_owned(),
                backups: emulator_saves::list(&data, id),
            })
        })
        .collect()
}

#[tauri::command(async)]
fn backup_emulator_saves(app: AppHandle, id: String) -> Result<Option<cerberus_shared::Backup>, String> {
    let data = app.state::<DataDir>().0.clone();
    let (_, _, exe) = emulators_here(&data).into_iter().find(|(e, _, _)| *e == id).ok_or(tr!("Esse emulador não está neste PC."))?;
    emulator_saves::backup(&data, &id, &exe, tk!("Manual"))
}

#[tauri::command(async)]
fn restore_emulator_backup(app: AppHandle, id: String, name: String) -> Result<u32, String> {
    if background::classic_running() {
        return Err(tr!("Feche o emulador antes de restaurar os saves.").into());
    }
    let data = app.state::<DataDir>().0.clone();
    let (_, label, exe) = emulators_here(&data).into_iter().find(|(e, _, _)| *e == id).ok_or(tr!("Esse emulador não está neste PC."))?;
    // Opened some other way (its own shortcut, "Open emulator"): on closing it
    // would write its memory card over what was restored.
    if running_from(&exe, &store_time::running_programs()) {
        return Err(tr!("Feche o {} antes de restaurar os saves: aberto, ele regrava o cartão de memória ao fechar.", label));
    }
    emulator_saves::restore(&data, &id, &exe, &name)
}

/// Whether the program at `exe` is among the running ones.
fn running_from(exe: &Path, programs: &[String]) -> bool {
    let want = exe.to_string_lossy().replace('/', "\\").to_lowercase();
    programs.iter().any(|p| p.replace('/', "\\").to_lowercase() == want)
}

#[tauri::command(async)]
fn open_emulator_backups(app: AppHandle, id: String) -> Result<(), String> {
    let dir = emulator_saves::backups_dir(&app.state::<DataDir>().0, &id);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|e| tr!("Não foi possível abrir a pasta: {}", e))
}

/// The native file dialog blocks until closed, so it must not run on an async worker.
async fn pick_one(dialog: FileDialogBuilder<tauri::Wry>) -> Result<Option<PathBuf>, String> {
    let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_file());
    let Some(picked) = picked.await.map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    picked.into_path().map(Some).map_err(|e| e.to_string())
}

async fn pick_folder(dialog: FileDialogBuilder<tauri::Wry>) -> Result<Option<PathBuf>, String> {
    let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_folder());
    let Some(picked) = picked.await.map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    picked.into_path().map(Some).map_err(|e| e.to_string())
}

fn copy_cover(folder: &Path, id: &str, source: &Path) -> Result<String, String> {
    fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    let ext = source.extension().unwrap_or_default().to_string_lossy().to_lowercase();
    // A new name on every change, so the webview does not show the old image from its cache.
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    let target = folder.join(format!("{}-{stamp}.{ext}", short_hash(id)));
    fs::copy(source, &target).map_err(|e| tr!("Não foi possível copiar a imagem: {}", e))?;
    Ok(target.to_string_lossy().into_owned())
}

/// Deletes a cover we copied earlier. Covers anywhere else belong to someone else
/// (Steam's cache, the user's own files), so those are left alone.
fn forget_cover(folder: &Path, cover: Option<String>) {
    let Some(cover) = cover else {
        return;
    };
    let cover = PathBuf::from(cover);
    if cover.starts_with(folder) {
        let _ = fs::remove_file(cover);
    }
}

fn short_hash(text: &str) -> String {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn main() {
    tauri::Builder::default()
        // One Cerberus at a time: two would write the same library and settings.
        // Opening it again brings the running one forward, even from the tray.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            background::show(app);
            // A desktop shortcut while Cerberus runs: the open window plays the game.
            // Kept first, then told: a window still loading takes it once it's ready
            // (the event alone would be lost), and it's taken once either way.
            if let Some(id) = shortcut::play_request(&args) {
                if let Some(request) = app.try_state::<PlayRequest>() {
                    *request.0.lock().unwrap_or_else(|p| p.into_inner()) = Some(id.clone());
                }
                let _ = app.emit("play-request", id);
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        // Started by Windows, Cerberus is told so, to start in the tray when asked to.
        .plugin(tauri_plugin_autostart::Builder::new().arg("--autostart").app_name(tr!("Cerberus Launcher")).build())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            // A copy of the data brought in before the restart goes in place first.
            if let Some(Err(e)) = data_backup::apply_pending(&data_dir) {
                eprintln!("restore: {e}");
            }
            let chosen = settings::load(&data_dir);
            language::apply(chosen.language.as_deref());
            app.manage(LocalLibrary::load(&data_dir));
            watch_store_installs(app.handle().clone());
            // Copies lost by an update made before this fix (or a folder cleaned by hand) go back now.
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || recomp::restore_copies(&handle, &handle.state::<LocalLibrary>()));
            }
            // Filters left beside a game by a crash or a forced close go now.
            looks::take_back_all(&data_dir);
            app.manage(trail::Trail::new(&data_dir));
            cheevos::restore_pending(&data_dir);
            app.manage(unlock_toast::Queue::default());
            app.manage(self_update::Found::default());
            let folders = recomp::Folders::new(&data_dir, chosen.recomps_dir.clone());
            folders.set_cloud(chosen.cloud_dir.clone());
            // An uninstall that couldn't finish deleting left its folder aside.
            recomp::sweep_removed(&folders);
            recomp::sweep_stale_downloads(&folders);
            recomp::sweep_unfinished_installs(&folders);
            recomp::sweep_save_leftovers(&folders);
            recomp::saves::keep_manifests_in(data_dir.join("cache").join("backup-manifests.json"));
            app.manage(folders);
            app.manage(recomp::Installing::default());
            awake::start(app.handle().clone());
            app.manage(sessions::Sessions::default());
            let asked: Vec<String> = std::env::args().collect();
            app.manage(PlayRequest(std::sync::Mutex::new(shortcut::play_request(&asked))));
            // A tray that fails (the taskbar not up yet) doesn't keep the app from opening.
            background::tray_or_later(app.handle());
            {
                let chosen = settings::load(&data_dir);
                recomp::throttle::set_limit(chosen.download_limit, chosen.slow_while_playing);
            }
            in_game::set(chosen.in_game.clone());
            notify::set(chosen.notices.clone());
            self_update::watch(app.handle().clone());
            app.manage(overlay::Overlay::default());
            screenshots::watch(app.handle().clone());
            if let Some(window) = app.get_webview_window("main") {
                let _ = monitors::place(&window, chosen.monitor.as_deref());
            }
            // Made hidden (tauri.conf.json): shown only now, on its monitor, and not
            // at all when it starts hidden with Windows (no flash of it first).
            if let Some(window) = app.get_webview_window("main") {
                // Made hidden, the window has only its small icon: the taskbar
                // shows the big one, set here from the program's own.
                #[cfg(windows)]
                if let Ok(hwnd) = window.hwnd() {
                    taskbar_icon(hwnd.0 as isize);
                }
                if !(chosen.start_hidden && std::env::args().any(|a| a == "--autostart")) {
                    let _ = window.show();
                }
            }
            // An installer that finished while Cerberus was closed.
            let recovering = app.handle().clone();
            std::thread::spawn(move || setup_runner::recover(&recovering));
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(15));
                sessions::checkpoint(&handle);
            });

            let cloud = app.handle().clone();
            std::thread::spawn(move || loop {
                let _ = recomp::cloud::retry(&cloud.state::<recomp::Folders>());
                std::thread::sleep(Duration::from_secs(60));
            });

            // The catalog and the last ROM scan from disk, so the window has them at once.
            let data = DataDir(data_dir.clone());
            recomp::manifest::set_keys_dir(data_dir.join("shared-keys"));
            recomp::catalog::use_site(chosen.site_catalog);
            recomp::catalog::load_copy(&data.cache());
            app.manage(RomIndex(std::sync::Mutex::new(roms::load_index(&data.rom_index()))));
            app.manage(Sources::default());
            // Any store (or account) change lists the stores afresh next time.
            app.listen_any("catalog-changed", |_| stores_changed());
            for url in &chosen.sources {
                remember_source(app.handle(), url, recomp::sources::load_copy(&data.cache(), url));
            }
            // Installs from before an entry was kept beside them keep theirs now,
            // while the catalog (and the lists) still have it.
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || recomp::keep_entries(&handle.state::<LocalLibrary>()));
            }
            publish_sources(app.handle());
            app.manage(data);
            if !chosen.rom_dirs.is_empty() {
                // Games added to the folders since last time show up once this finishes.
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move { let _ = rescan_roms(&handle).await; });
            }

            let http = reqwest::Client::builder()
                .user_agent("CerberusLauncher")
                .redirect(net::redirects())
                .timeout(Duration::from_secs(10))
                .build()?;
            // Lists with no copy on this PC, or one over a day old, are fetched
            // again in the background: an added list is never left empty.
            let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default();
            let stale: Vec<String> = app.state::<Sources>().list.lock().unwrap_or_else(|p| p.into_inner()).iter()
                .filter(|(view, _)| view.synced_at.is_none_or(|at| now.saturating_sub(at) > 24 * 60 * 60))
                .map(|(view, _)| view.url.clone())
                .collect();
            if !stale.is_empty() {
                let (handle, client) = (app.handle().clone(), http.clone());
                tauri::async_runtime::spawn(async move {
                    let state = handle.state::<Sources>();
                    let _guard = state.sync.lock().await;
                    for url in &stale {
                        sync_source(&handle, &client, url).await;
                    }
                    publish_sources(&handle);
                });
            }
            app.manage(http);
            // After the HTTP client is in place: a game already open at start
            // reaches the live achievements, which use it.
            // Store-game sessions Cerberus closed in the middle of reach the log now.
            play_log::close_left_open(&data_dir);
            store_time::watch(app.handle().clone());

            // The UI may only load local files from folders we allow here:
            // covers the user picked, and Steam's own art cache.
            let covers = data_dir.join("covers");
            fs::create_dir_all(&covers)?;
            app.asset_protocol_scope().allow_directory(&covers, true)?;
            app.manage(art_custom::CustomArt::new(&data_dir, &covers));
            app.manage(CoverFolder(covers));

            let art = data_dir.join("cache").join("art");
            fs::create_dir_all(&art)?;
            app.asset_protocol_scope().allow_directory(&art, true)?;
            let art_cache = Arc::new(ArtCache::new(art));
            // Pictures that wouldn't come: one line per site in the history.
            let handle = app.handle().clone();
            art_cache.report_failures_with(move |site, count, why| {
                let said = if count == 1 {
                    tr!("Uma imagem de {0} não baixou ({1}); é tentada de novo na próxima vez.", site, why)
                } else {
                    tr!("{0} imagens de {1} não baixaram ({2}); são tentadas de novo na próxima vez.", count, site, why)
                };
                trail::note(&handle, "art", tk!("Arte"), &Err(said));
            });
            app.manage(art_cache);

            let shots = screenshots::root(&data_dir);
            fs::create_dir_all(&shots)?;
            app.asset_protocol_scope().allow_directory(&shots, true)?;

            let steam_covers = steam::root().map(|root| steam::cover_cache_dir(&root));
            if let Some(Ok(steam_covers)) = steam_covers.map(dunce::canonicalize) {
                app.asset_protocol_scope().allow_directory(steam_covers, true)?;
            }
            // The Xbox app's games: their logo files only, not the folders.
            for game in xbox::installed_games() {
                if let Some(cover) = game.cover_path {
                    let _ = app.asset_protocol_scope().allow_file(cover);
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_games,
            library_recovery_notice,
            add_local_game,
            rename_game,
            find_cover,
            pick_cover,
            remove_game,
            list_recomps,
            search_lists,
            catalog_entry,
            install_recomp,
            open_project,
            open_recomp_request,
            cancel_install,
            pause_install,
            cancel_copy,
            download_queue,
            forget_download,
            download_first,
            move_game,
            running_games,
            list_backups,
            backup_now,
            restore_backup,
            game_trail,
            game_settings,
            set_steamgriddb_key,
            art_search,
            art_options,
            art_fetch,
            art_pick_file,
            art_save,
            look_status,
            set_look,
            open_steamgriddb_keys,
            scan_for_recomps,
            adopt_recomp,
            recomp_suggestions,
            link_to_recomp,
            set_game_setting,
            open_folder,
            open_backups,
            set_game_pass,
            epic_login,
            epic_logout,
            xbox_login,
            xbox_logout,
            xbox_open,
            emulator_backups,
            backup_emulator_saves,
            restore_emulator_backup,
            open_emulator_backups,
            get_settings,
            finish_setup,
            set_start_in_tv,
            set_start_page,
            emulation_status,
            install_retroarch,
            pick_retroarch,
            install_core,
            play_classic,
            set_site_catalog,
            set_close_to_tray,
            list_screenshots,
            dlss_list,
            dlss_swap,
            dlss_restore,
            overlay::overlay_state,
            overlay::overlay_close,
            unlock_toast::unlock_next,
            unlock_toast::unlock_test,
            overlay::overlay_screenshot,
            overlay::overlay_quit_game,
            overlay::overlay_open_launcher,
            open_screenshot,
            open_screenshots_folder,
            list_monitors,
            set_monitor,
            set_language,
            achievements_hub,
            steam_details,
            store_page,
            steam_news,
            open_in_store,
            store_manage,
            standalone_status,
            install_standalone,
            pick_standalone,
            remove_standalone,
            set_play_with,
            open_standalone,
            standalone_bios,
            install_all_cores,
            set_emulator_folders,
            add_all_to_steam,
            wiki_about,
            open_wiki,
            set_tray_games,
            open_news,
            game_sessions,
            set_game_note,
            player_note,
            set_updates_paused,
            set_download_limit,
            set_in_game,
            set_notices,
            epic_status,
            export_data,
            import_data,
            epic_open,
            update_check,
            update_found,
            update_install,
            whats_new,
            whats_new_seen,
            classic_achievements,
            open_store_link,
            steam_local_account,
            steam_account,
            steam_connect,
            steam_disconnect,
            steam_open,
            game_achievements,
            to_chosen_monitor,
            back_from_tv,
            set_autostart,
            set_start_hidden,
            recent_activity,
            open_rom_folder,
            take_play_request,
            create_shortcut,
            pc_status,
            install_vc,
            install_dx,
            add_to_steam,
            remove_from_steam,
            in_steam,
            set_favorite,
            set_hidden,
            list_collections,
            create_collection,
            set_in_collection,
            rename_collection,
            delete_collection,
            import_bios,
            link_retroarch_achievements,
            rerun_setup,
            ra_connect,
            ra_profile,
            ra_disconnect,
            ra_open,
            pick_recomps_dir,
            reset_recomps_dir,
            open_data_dir,
            clear_art_cache,
            add_rom_dir,
            remove_rom_dir,
            scan_roms,
            refresh_catalog,
            set_game_copy,
            game_inputs,
            pick_game_input,
            prepare_options,
            set_prepare_option,
            open_prepare_license,
            game_actions,
            preparation_pending,
            classic_players,
            prepare_game,
            run_game_action,
            set_launch_args,
            tracking_mode,
            pick_tracking_exe,
            locate_game,
            set_cloud_dir,
            retry_save_uploads,
            add_source,
            refresh_source,
            source_listings,
            remove_source,
            list_mods,
            install_mod,
            remove_mod,
            play
        ])
        .on_window_event(|window, event| {
            // Alt+Tab with the menu over the game open: it closes instead of
            // staying on top of everything, the size of the game.
            if window.label() == overlay::LABEL && matches!(event, tauri::WindowEvent::Focused(false)) {
                overlay::hide(window.app_handle());
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    if background::on_close(window, window.app_handle()) {
                        api.prevent_close();
                    } else {
                        // Closing for good: the hidden menu and notice windows would keep
                        // the process alive with no window, and reopening would find none.
                        window.app_handle().exit(0);
                    }
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("failed to start Cerberus Launcher")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) { sessions::checkpoint(app); }
        });
}

#[cfg(test)]
#[test]
fn the_library_is_put_together_with_the_players_data_on_each_game() {
    let data = tempfile::tempdir().unwrap();
    let added = library::game_from_exe(Path::new(r"D:\Games\Zeta\zeta.exe"));
    let mut steam = steam::not_installed(10, "alpha", None);
    steam.cover_url = Some("http://cdn.example.com/a.jpg".into());
    let other = steam::not_installed(20, "Beta", None);
    // The player's own data, as their files hold it.
    favorites::set(data.path(), &steam.id, true).unwrap();
    let steam_time = std::collections::HashMap::from([(10u32, (3600u64, Some(1_700_000_000u64)))]);
    let games = assemble(data.path(), vec![added.clone()], vec![steam.clone(), other], &steam_time);
    let ids: Vec<&str> = games.iter().map(|g| g.id.as_str()).collect();
    assert_eq!(ids, ["steam:10", "steam:20", added.id.as_str()], "by name, any case");
    let unique: std::collections::HashSet<&str> = ids.iter().copied().collect();
    assert_eq!(unique.len(), ids.len(), "each game once");
    let alpha = &games[0];
    assert!(alpha.favorite && !games[1].favorite);
    assert_eq!(alpha.cover_url.as_deref(), Some("https://cdn.example.com/a.jpg"));
    assert_eq!((alpha.playtime_secs, alpha.last_played), (3600, Some(1_700_000_000)), "Steam's own time");
}

#[cfg(test)]
#[test]
fn a_stores_own_program_is_known_as_its_game() {
    let mut steam: Game = serde_json::from_value(serde_json::json!({"id": "steam:1", "name": "Hades", "source": {"kind": "steam", "app_id": 1}, "install_dir": r"D:\Steam\steamapps\common\Hades"})).unwrap();
    let stores = vec![steam.clone()];
    assert_eq!(store_copy_of(Path::new(r"D:\Steam\steamapps\common\Hades\x64\Hades.exe"), &stores).map(|g| g.id.as_str()), Some("steam:1"));
    assert!(store_copy_of(Path::new(r"D:\Steam\steamapps\common\Hades II\Hades2.exe"), &stores).is_none(), "a folder with a longer name");
    steam.install_dir.clear();
    assert!(store_copy_of(Path::new(r"D:\Games\Hades.exe"), &[steam]).is_none(), "owned, not installed");
}

#[cfg(test)]
#[test]
fn an_owned_steam_game_installed_here_isnt_added_again() {
    let one = |app: u32, name: &str| achievements::steam_api::Owned { app, name: name.into(), minutes: 0, last_played: 0, stats: false };
    let owned = vec![one(1, "Installed"), one(2, "Not here"), one(2, "Not here"), one(3, " ")];
    let mut games = vec![steam::not_installed(1, "Installed", None)];
    games[0].install_dir = r"D:\Steam\common\Installed".into();
    add_owned(&mut games, &owned, None);
    let ids: Vec<&str> = games.iter().map(|g| g.id.as_str()).collect();
    assert_eq!(ids, ["steam:1", "steam:2"], "no copy of the installed one, one of a repeated one, none without a name");
}

#[cfg(test)]
#[test]
fn an_emulator_open_outside_cerberus_is_seen() {
    let exe = Path::new(r"C:\Emu\DuckStation\duckstation-qt.exe");
    assert!(running_from(exe, &[r"c:\emu\duckstation\DuckStation-QT.exe".into()]));
    assert!(!running_from(exe, &[r"C:\Other\duckstation-qt.exe".into()]), "another copy elsewhere");
    assert!(!running_from(exe, &[]));
}

#[cfg(test)]
#[test]
fn a_classic_uses_the_console_its_file_says() {
    assert_eq!(console_for("ps1", "ps2".into()), "ps1");
    assert_eq!(console_for("", "ps2".into()), "ps2");
}

#[cfg(test)]
#[test]
fn days_are_named_as_on_the_calendar() {
    assert_eq!(day_of(0), "1970-01-01");
    assert_eq!(day_of(951_782_400), "2000-02-29");
    assert_eq!(day_of(1_795_392_000), "2026-11-23");
}

#[cfg(test)]
#[test]
fn windows_get_no_wildcard_or_remote_permissions_and_the_csp_stays_on() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    for entry in fs::read_dir(dir.join("capabilities")).unwrap().flatten() {
        let text = fs::read_to_string(entry.path()).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(value.get("remote").is_none(), "{}: a capability for remote pages", entry.path().display());
        let permissions = value["permissions"].as_array().cloned().unwrap_or_default();
        assert!(!permissions.iter().any(|p| p.as_str().is_some_and(|p| p.contains('*'))), "{}: a wildcard permission", entry.path().display());
    }
    let conf: serde_json::Value = serde_json::from_str(&fs::read_to_string(dir.join("tauri.conf.json")).unwrap()).unwrap();
    let csp = conf["app"]["security"]["csp"].as_str().unwrap_or_default();
    assert!(csp.contains("default-src 'self'") && !csp.contains("'unsafe-eval'") && csp.contains("object-src 'none'"), "the CSP: {csp}");
}

#[cfg(test)]
#[test]
fn settings_are_changed_under_their_lock_not_loaded_and_saved_whole() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    // Split so this test's own text doesn't count.
    let needle = concat!("settings", "::save(");
    // The restore writes the whole merged copy on purpose, even over a damaged
    // file (which `settings::update` refuses to change).
    let allowed = concat!("crate::settings", "::save(data, &settings)?;");
    for file in ["src/main.rs", "src/data_backup.rs"] {
        let text = fs::read_to_string(dir.join(file)).unwrap();
        for (n, line) in text.lines().enumerate() {
            if line.contains(needle) && !line.contains(allowed) {
                panic!("{file}:{}: use settings::update instead of load + save: {}", n + 1, line.trim());
            }
        }
    }
}

#[cfg(test)]
mod store_url_tests {
    use super::*;

    fn game(source: Source, installed: bool) -> Game {
        Game { name: "Forza Horizon 5".into(), source, install_dir: if installed { r"C:\Games\x".into() } else { String::new() }, ..library::blank_game() }
    }

    #[test]
    fn no_button_opens_an_address_with_an_empty_id() {
        let xbox = |product: &str| Source::Xbox { family: "f".into(), app: "Game".into(), product: product.into(), game_pass: false };
        assert_eq!(store_url(&game(xbox("9NKX70BBCDRN"), false), true).unwrap(), "msxbox://game/?productId=9NKX70BBCDRN");
        assert_eq!(store_url(&game(xbox(""), false), true).unwrap(), "ms-windows-store://search/?query=Forza%20Horizon%205", "no product: the store's search by name");
        assert!(store_url(&game(Source::Steam { app_id: 0 }, false), true).is_err());
        assert_eq!(store_url(&game(Source::Steam { app_id: 10 }, false), true).unwrap(), "steam://install/10");
        assert_eq!(store_url(&game(Source::Steam { app_id: 10 }, true), true).unwrap(), "steam://rungameid/10");
        let epic = Source::Epic { app_name: "Fortnite".into(), namespace: "fn".into(), item_id: "i".into() };
        assert_eq!(store_url(&game(epic.clone(), false), false).unwrap_err(), EPIC_MISSING, "without the Epic launcher: its download page instead");
        assert!(store_url(&game(epic, false), true).unwrap().ends_with("?action=install"));
        assert!(store_url(&game(Source::Local { exe: "x.exe".into() }, true), true).is_err());
    }

    #[test]
    fn each_store_has_a_way_to_uninstall() {
        let xbox = Source::Xbox { family: "f".into(), app: "Game".into(), product: "p".into(), game_pass: false };
        let epic = Source::Epic { app_name: "a".into(), namespace: "n".into(), item_id: "i".into() };
        assert_eq!(store_manage_url(&game(Source::Steam { app_id: 10 }, true), false).unwrap(), "steam://uninstall/10");
        assert_eq!(store_manage_url(&game(Source::Steam { app_id: 10 }, true), true).unwrap(), "steam://validate/10");
        assert_eq!(store_manage_url(&game(xbox.clone(), true), false).unwrap(), "ms-settings:appsfeatures");
        assert!(store_manage_url(&game(xbox, true), true).is_err(), "the Xbox app checks files only in its own window");
        assert_eq!(store_manage_url(&game(epic, true), false).unwrap(), "com.epicgames.launcher://store/library");
    }
}

#[cfg(test)]
mod install_watch_tests {
    #[test]
    fn a_manifest_removed_or_added_changes_the_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let folders = vec![dir.path().to_path_buf(), dir.path().join("not-there")];
        std::fs::write(dir.path().join("appmanifest_1.acf"), "a").unwrap();
        let first = super::install_fingerprint(&folders);
        assert_eq!(super::install_fingerprint(&folders), first, "nothing changed: the same");
        std::fs::remove_file(dir.path().join("appmanifest_1.acf")).unwrap();
        let removed = super::install_fingerprint(&folders);
        assert_ne!(removed, first, "uninstalled: changed");
        std::fs::write(dir.path().join("appmanifest_2.acf"), "b").unwrap();
        assert_ne!(super::install_fingerprint(&folders), removed, "installed: changed");
    }
}
