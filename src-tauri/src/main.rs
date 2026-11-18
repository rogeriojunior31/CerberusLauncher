// Release builds on Windows would otherwise open a console window next to the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod art_cache;
mod epic;
mod disk;
mod iso9660;
mod library;
mod mods;
mod recomp;
mod sessions;
mod roms;
mod settings;
mod setup_runner;
mod steam;
mod steam_store;
mod trail;
mod game_settings;
mod xdvdfs;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use art_cache::ArtCache;
use cerberus_shared::{Backup, Game, InstallProgress, RecompEntry, SettingsView, Source};
use library::LocalLibrary;
use recomp::sources::{CatalogSource, Entries};
use cerberus_shared::{SourcePage, SourceView};
use tauri::{AppHandle, Emitter, Manager, State};
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
    }
}

/// Recomp id -> the player's copy of its game.
fn owned_games(app: &AppHandle) -> std::collections::HashMap<String, String> {
    let found = app.state::<RomIndex>().get();
    roms::match_catalog(&recomp::catalog::all(), &found)
}

fn settings_view(app: &AppHandle) -> SettingsView {
    let folders = app.state::<recomp::Folders>();
    let installs = folders.installs();
    let art = app.state::<Arc<ArtCache>>();
    let data = app.state::<DataDir>();
    let chosen = settings::load(&data.0);
    SettingsView {
        install_free_bytes: disk::available(&installs),
        download_free_bytes: disk::available(&data.0.join("downloads")),
        recomps_dir_is_default: installs == folders.default_installs,
        recomps_dir: installs.to_string_lossy().into_owned(),
        data_dir: data.0.to_string_lossy().into_owned(),
        art_cache_bytes: settings::folder_size(art.dir()),
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
        Entries::Projects(games) => Some(games.clone()),
        Entries::Hydra(_) => None,
    }).collect();
    recomp::catalog::set_extras(projects);
    drop(list);
    let _ = app.emit("catalog-changed", ());
}

fn remember_source(app: &AppHandle, url: &str, source: Option<CatalogSource>) {
    let (hydra, games) = match source.as_ref().map(|s| &s.entries) {
        Some(Entries::Projects(items)) => (false, items.len()),
        Some(Entries::Hydra(items)) => (true, items.len()),
        None => (false, 0),
    };
    let view = SourceView {
        url: url.to_owned(),
        name: source.as_ref().map(|s| s.name.as_str()).filter(|s| !s.trim().is_empty()).unwrap_or(url).to_owned(),
        hydra, games, synced_at: source.as_ref().and_then(|s| s.synced_at),
        error: source.is_none().then(|| "Sem cópia local. Atualize a fonte para consultá-la.".to_owned()),
    };
    let state = app.state::<Sources>();
    let mut list = state.list.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(row) = list.iter_mut().find(|(v, _)| v.url == url) {
        *row = (view, source);
    } else {
        list.push((view, source));
    }
}

#[tauri::command]
fn source_listings(sources: State<'_, Sources>, url: String, query: String, offset: usize) -> Result<SourcePage, String> {
    let list = sources.list.lock().unwrap_or_else(|p| p.into_inner());
    let source = list.iter().find(|(v, _)| v.url == url).and_then(|(_, s)| s.as_ref())
        .ok_or("Fonte indisponível.")?;
    source.listings(&query, offset)
}

#[tauri::command]
async fn add_source(app: AppHandle, http: State<'_, reqwest::Client>, url: String) -> Result<SettingsView, String> {
    let state = app.state::<Sources>();
    let _guard = state.sync.lock().await;
    let url = url.trim().to_string();
    let data = app.state::<DataDir>();
    let source = recomp::sources::fetch(&http, &data.cache(), &url).await?;
    let mut chosen = settings::load(&data.0);
    if !chosen.sources.contains(&url) {
        chosen.sources.push(url.clone());
        settings::save(&data.0, &chosen)?;
    }
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
    if !settings::load(&app.state::<DataDir>().0).sources.contains(&url) { return Err("Fonte removida.".into()); }
    sync_source(&app, &http, &url).await;
    publish_sources(&app);
    Ok(settings_view(&app))
}

#[tauri::command]
async fn remove_source(app: AppHandle, url: String) -> Result<SettingsView, String> {
    let state = app.state::<Sources>();
    let _guard = state.sync.lock().await;
    let data = app.state::<DataDir>();
    let mut chosen = settings::load(&data.0);
    chosen.sources.retain(|u| *u != url);
    settings::save(&data.0, &chosen)?;
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
        let dialog = app.dialog().file().set_title("Pasta sincronizada (OneDrive, Google Drive, Dropbox...)");
        let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_folder());
        let Some(picked) = picked.await.map_err(|e| e.to_string())? else {
            return Ok(settings_view(&app));
        };
        Some(picked.into_path().map_err(|e| e.to_string())?)
    };
    let data_dir = app.state::<DataDir>().0.clone();
    let mut chosen = settings::load(&data_dir);
    chosen.cloud_dir = dir.clone();
    settings::save(&data_dir, &chosen)?;
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

/// Looks through the ROM folders again, off the UI's thread, and tells the window.
async fn rescan_roms(app: &AppHandle) {
    let dirs = settings::load(&app.state::<DataDir>().0).rom_dirs;
    let found = tauri::async_runtime::spawn_blocking(move || roms::scan(&dirs)).await.unwrap_or_default();
    roms::save_index(&app.state::<DataDir>().rom_index(), &found);
    app.state::<RomIndex>().set(found);
    let _ = app.emit("catalog-changed", ());
}

/// Asks for a ROM folder to look through for games that have a recomp.
#[tauri::command]
async fn add_rom_dir(app: AppHandle) -> Result<SettingsView, String> {
    let dialog = app.dialog().file().set_title("Pasta com suas ROMs ou imagens de disco");
    let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_folder());
    let Some(picked) = picked.await.map_err(|e| e.to_string())? else {
        return Ok(settings_view(&app));
    };
    let dir = picked.into_path().map_err(|e| e.to_string())?;
    let data_dir = app.state::<DataDir>().0.clone();
    let mut chosen = settings::load(&data_dir);
    if !chosen.rom_dirs.contains(&dir) {
        chosen.rom_dirs.push(dir);
        settings::save(&data_dir, &chosen)?;
    }
    rescan_roms(&app).await;
    Ok(settings_view(&app))
}

#[tauri::command]
async fn remove_rom_dir(app: AppHandle, path: String) -> Result<SettingsView, String> {
    let data_dir = app.state::<DataDir>().0.clone();
    let mut chosen = settings::load(&data_dir);
    chosen.rom_dirs.retain(|d| d.to_string_lossy() != path);
    settings::save(&data_dir, &chosen)?;
    rescan_roms(&app).await;
    Ok(settings_view(&app))
}

#[tauri::command]
async fn scan_roms(app: AppHandle) -> Result<SettingsView, String> {
    rescan_roms(&app).await;
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

/// Ends the first-run setup with how the player plays (desk or TV).
#[tauri::command]
fn finish_setup(app: AppHandle, start_in_tv: bool) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>();
    let mut chosen = settings::load(&data.0);
    chosen.setup_done = true;
    chosen.start_in_tv = start_in_tv;
    settings::save(&data.0, &chosen)?;
    Ok(settings_view(&app))
}

/// Whether the app opens in the TV screen; changed in Configurações.
#[tauri::command]
fn set_start_in_tv(app: AppHandle, on: bool) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>();
    let mut chosen = settings::load(&data.0);
    chosen.start_in_tv = on;
    settings::save(&data.0, &chosen)?;
    Ok(settings_view(&app))
}

#[tauri::command]
fn get_settings(app: AppHandle) -> SettingsView {
    settings_view(&app)
}

/// Asks for a folder for new recomp installs. Games already installed stay put.
#[tauri::command]
async fn pick_recomps_dir(app: AppHandle) -> Result<SettingsView, String> {
    let dialog = app.dialog().file().set_title("Pasta para instalar recomps");
    let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_folder());
    let Some(picked) = picked.await.map_err(|e| e.to_string())? else {
        return Ok(settings_view(&app));
    };
    let dir = picked.into_path().map_err(|e| e.to_string())?;
    use_recomps_dir(&app, Some(dir))
}

#[tauri::command]
fn reset_recomps_dir(app: AppHandle) -> Result<SettingsView, String> {
    use_recomps_dir(&app, None)
}

fn use_recomps_dir(app: &AppHandle, dir: Option<PathBuf>) -> Result<SettingsView, String> {
    let folders = app.state::<recomp::Folders>();
    let target = dir.clone().unwrap_or_else(|| folders.default_installs.clone());
    fs::create_dir_all(&target).map_err(|e| format!("Não foi possível usar {}: {e}", target.display()))?;
    let data_dir = app.state::<DataDir>().0.clone();
    let mut chosen = settings::load(&data_dir);
    chosen.recomps_dir = dir;
    settings::save(&data_dir, &chosen)?;
    folders.set_installs(target);
    Ok(settings_view(app))
}

#[tauri::command]
fn open_data_dir(app: AppHandle) -> Result<(), String> {
    let dir = app.state::<DataDir>().0.to_string_lossy().into_owned();
    app.opener().open_path(dir, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
fn clear_art_cache(app: AppHandle) -> Result<SettingsView, String> {
    app.state::<Arc<ArtCache>>().clear()?;
    Ok(settings_view(&app))
}

#[tauri::command]
fn library_recovery_notice(library: State<'_, LocalLibrary>) -> Option<String> {
    library.recovery_notice()
}

#[tauri::command]
async fn list_games(
    app: AppHandle,
    library: State<'_, LocalLibrary>,
    art: State<'_, Arc<ArtCache>>,
    http: State<'_, reqwest::Client>,
) -> Result<Vec<Game>, String> {
    let mut games = library.games()?;
    games.iter_mut().for_each(recomp::apply_art);
    let owned = owned_games(&app);
    for game in &mut games {
        let found = match &game.source {
            Source::Recomp { recomp_id, .. } => owned.get(recomp_id),
            _ => None,
        };
        game.next_step = recomp::next_step(game, found);
    }
    // Scanning library folders touches the disk; keep it off the async runtime.
    let scan = tauri::async_runtime::spawn_blocking(|| {
        let mut found = steam::installed_games();
        found.extend(epic::installed_games());
        found
    });
    games.extend(scan.await.unwrap_or_default());
    games.sort_by_key(|game| game.name.to_lowercase());
    let mut missing = Vec::new();
    art.use_copies_for_games(&mut games, &mut missing);
    art.fetch_later(http.inner().clone(), missing);
    Ok(games)
}

/// Asks for an executable and adds it to the library, with the name and cover
/// Steam has for it when it is a Steam game. `None` when the user cancels.
#[tauri::command]
async fn add_local_game(
    app: AppHandle,
    library: State<'_, LocalLibrary>,
    http: State<'_, reqwest::Client>,
) -> Result<Option<Game>, String> {
    let dialog = app.dialog().file().set_title("Escolha o executável do jogo");
    let dialog = if cfg!(windows) { dialog.add_filter("Jogos", &["exe"]) } else { dialog };
    let Some(exe) = pick_one(dialog).await? else {
        return Ok(None);
    };
    let mut game = library::game_from_exe(&exe);
    // Offline or unknown to Steam: keep the guessed name; the user can edit it.
    if let Some(found) = steam_store::identify(&http, &library::name_candidates(&exe)).await {
        game.name = found.name;
        game.cover_url = found.art.cover;
        game.hero_url = found.art.hero;
    }
    library.add(game.clone())?;
    Ok(Some(game))
}

#[tauri::command]
fn rename_game(library: State<'_, LocalLibrary>, id: String, name: String) -> Result<Game, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("O nome não pode ficar vazio.".into());
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
    let game = library.get(&id)?.ok_or("Esse jogo não está mais na biblioteca.")?;
    let art = steam_store::art_for_name(&http, &game.name).await?;
    let Some(art) = art.filter(|art| art.cover.is_some()) else {
        return Err(format!("Nenhuma capa encontrada na Steam para \"{}\".", game.name));
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
    let dialog = app.dialog().file().set_title("Escolha a capa do jogo");
    let dialog = dialog.add_filter("Imagens", &["png", "jpg", "jpeg", "webp"]);
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
#[tauri::command]
fn remove_game(
    library: State<'_, LocalLibrary>,
    folder: State<'_, CoverFolder>,
    recomps: State<'_, recomp::Folders>,
    sessions: State<'_, sessions::Sessions>,
    installing: State<'_, recomp::Installing>,
    id: String,
) -> Result<(), String> {
    let Some(game) = library.get(&id)? else {
        return Ok(());
    };
    if sessions.is_running(&id) {
        return Err("Feche o jogo antes de removê-lo.".into());
    }
    // Files first: if they can't go, the game stays listed. Saves are copied
    // before anything is deleted, and the copies outlive the uninstall.
    if let Source::Recomp { recomp_id, .. } = &game.source {
        installing.while_idle(recomp_id, || {
            if sessions.is_running(&id) {
                return Err("Feche o jogo antes de removê-lo.".into());
            }
            recomp::backup_saves(&recomps, recomp_id, Path::new(&game.install_dir), "Antes de desinstalar")?;
            // A folder the player installed on their own stays theirs: only the link goes.
            if !game.external {
                recomp::delete_files(&recomps, recomp_id, Path::new(&game.install_dir))?;
            }
            library.remove(&id)
        })?;
    } else {
        library.remove(&id)?;
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
            let before = recomp::catalog::all().len();
            let updated = recomp::catalog::refresh(&client, &cache, false).await;
            if updated.is_ok() && recomp::catalog::all().len() != before {
                let _ = handle.emit("catalog-changed", ());
            }
        });
    }
    let mut entries = recomp::entries(&library, &owned_games(&app))?;
    // Copies already on disk are used, but 1900 recomps' art is not fetched
    // ahead: the window loads what it shows, and games in the library keep theirs.
    let mut not_fetched = Vec::new();
    art.use_copies_for_recomps(&mut entries, &mut not_fetched);
    Ok(entries)
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
    let report = |step| {
        let progress = InstallProgress { recomp_id: id.clone(), step };
        let _ = app.emit("recomp-progress", progress);
    };
    let sessions = app.state::<sessions::Sessions>();
    let result = recomp::install(&http, &folders, &library, &installing, &sessions, &id, &report).await;
    let game = match result {
        Ok(game) => game,
        Err(message) => {
            report(cerberus_shared::InstallStep::Failed { message: message.clone() });
            // Keyed by the library id a successful install would have had.
            trail::note(&app, &format!("recomp:{id}"), "Instalação", &Err(message.clone()));
            return Err(message);
        }
    };
    let version = match &game.source {
        Source::Recomp { version, .. } => version.clone(),
        _ => String::new(),
    };
    trail::note(&app, &game.id, "Instalação", &Ok(format!("Versão {version} instalada")));
    // Easy Setup: the player's copy was found in their ROM folders, so the recomp
    // is set up with it right away. Failing that only means setting it by hand.
    let found = owned_games(&app).remove(&id);
    if let (Some(copy), None) = (found, &game.game_copy) {
        if let Ok(ready) = recomp::use_game_copy(&library, &game.id, Path::new(&copy)) {
            return Ok(ready);
        }
    }
    Ok(game)
}

/// Sets a recomp up with a copy of its game: the one found in the ROM folders
/// when `use_found`, else one the player picks (a folder of extracted files
/// when `folder`).
#[tauri::command]
async fn set_game_copy(app: AppHandle, library: State<'_, LocalLibrary>, id: String, use_found: bool, folder: bool) -> Result<Option<Game>, String> {
    let game = library.get(&id)?.ok_or("Esse jogo não está mais na biblioteca.")?;
    let Source::Recomp { recomp_id, .. } = &game.source else {
        return Err("Só recomps usam uma cópia do jogo.".into());
    };
    let copy = if use_found {
        let owned = owned_games(&app);
        let entry = recomp::find(recomp_id).ok_or("Esse recomp não está no catálogo.")?;
        let found = owned.get(&entry.id).cloned();
        PathBuf::from(found.ok_or("Nenhuma cópia desse jogo nas suas pastas de ROMs.")?)
    } else if folder {
        let dialog = app.dialog().file().set_title("Escolha a pasta com os arquivos do jogo");
        let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_folder());
        let Some(picked) = picked.await.map_err(|e| e.to_string())? else {
            return Ok(None);
        };
        picked.into_path().map_err(|e| e.to_string())?
    } else {
        let dialog = app.dialog().file().set_title("Escolha sua cópia do jogo (ROM ou imagem de disco)");
        let Some(picked) = pick_one(dialog).await? else {
            return Ok(None);
        };
        picked
    };
    // Copying or unpacking a disc image can take minutes; not on an async worker.
    let handle = app.clone();
    let setup = tauri::async_runtime::spawn_blocking(move || recomp::use_game_copy(&handle.state::<LocalLibrary>(), &id, &copy));
    setup.await.map_err(|e| e.to_string())?.map(Some)
}

/// The mod source and mods folder of an installed recomp; `None` when it has no mods.
async fn mod_setup(app: &AppHandle, id: &str) -> Result<Option<(mods::ModSource, PathBuf, PathBuf)>, String> {
    let library = app.state::<LocalLibrary>();
    let game = library.get(id)?.ok_or("Esse jogo não está mais na biblioteca.")?;
    let Source::Recomp { recomp_id, exe, .. } = &game.source else {
        return Ok(None);
    };
    let name = recomp::find(recomp_id).map(|e| e.name).unwrap_or_else(|| game.name.clone());
    let http = app.state::<reqwest::Client>().inner().clone();
    let cache = app.state::<DataDir>().cache().join("mods");
    let communities = mods::communities(&http, &cache).await;
    let Some(source) = mods::source_for(recomp_id, &name, Path::new(exe), &communities) else {
        return Ok(None);
    };
    let folder = mods::mods_dir(Path::new(&game.install_dir), &source.data);
    let record = app.state::<DataDir>().0.join("mods").join(format!("{recomp_id}.json"));
    Ok(Some((source, folder, record)))
}

#[tauri::command]
async fn list_mods(app: AppHandle, http: State<'_, reqwest::Client>, id: String) -> Result<Option<cerberus_shared::ModList>, String> {
    let Some((source, folder, record)) = mod_setup(&app, &id).await? else {
        return Ok(None);
    };
    let cache = app.state::<DataDir>().cache().join("mods");
    let packages = mods::packages(&http, &cache, &source.community).await?;
    let installed = mods::load_installed(&record);
    let items = packages.iter().map(|p| mods::item(p, &installed)).collect();
    Ok(Some(cerberus_shared::ModList { community: source.community, folder: folder.to_string_lossy().into_owned(), items }))
}

/// Installs (or updates) a mod and the mods it needs.
#[tauri::command]
async fn install_mod(app: AppHandle, http: State<'_, reqwest::Client>, id: String, full_name: String) -> Result<(), String> {
    let (source, folder, record) = mod_setup(&app, &id).await?.ok_or("Esse jogo não tem mods.")?;
    let cache = app.state::<DataDir>().cache().join("mods");
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
            if let Some((_, old)) = installed.0.get(&name) {
                mods::remove_files(&folder, old);
            }
            let files = mods::unpack(&bytes, &folder)?;
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
    let (_, folder, record) = mod_setup(&app, &id).await?.ok_or("Esse jogo não tem mods.")?;
    let mut installed = mods::load_installed(&record);
    if let Some((_, files)) = installed.0.remove(&full_name) {
        mods::remove_files(&folder, &files);
        mods::save_installed(&record, &installed)?;
    }
    Ok(())
}

/// After an installer ran and the game wasn't found: the player points at its program.
#[tauri::command]
async fn locate_game(app: AppHandle, id: String) -> Result<Option<Game>, String> {
    let dialog = app.dialog().file().set_title("Onde está o jogo? Escolha o programa dele");
    let dialog = if cfg!(windows) { dialog.add_filter("Programa", &["exe"]) } else { dialog };
    let Some(program) = pick_one(dialog).await? else {
        return Ok(None);
    };
    let result = setup_runner::use_program(&app, &id, &program);
    let outcome = result.as_ref().map(|_| program.display().to_string()).map_err(Clone::clone);
    trail::note(&app, &id, "Localizar programa", &outcome);
    result.map(Some)
}

#[tauri::command]
fn tracking_mode(library: State<'_, LocalLibrary>, id: String) -> Result<Option<String>, String> {
    Ok(library.get(&id)?.as_ref().and_then(sessions::tracking_mode))
}

#[tauri::command]
async fn pick_tracking_exe(app: AppHandle, id: String, automatic: bool) -> Result<Option<Game>, String> {
    let library = app.state::<LocalLibrary>();
    let game = library.get(&id)?.ok_or("Jogo removido da biblioteca.")?;
    if sessions::tracking_mode(&game).is_none() { return Err("A seleção de processo está indisponível para este jogo.".into()); }
    let selected = if automatic { None } else {
        let dialog = app.dialog().file().set_title("Escolha o executável que roda durante a partida").add_filter("Programa", &["exe"]);
        let Some(path) = pick_one(dialog).await? else { return Ok(None) };
        let path = dunce::canonicalize(path).map_err(|e| e.to_string())?;
        if !path.is_file() || !path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("exe")) {
            return Err("Escolha o arquivo .exe do jogo.".into());
        }
        Some(path.to_string_lossy().into_owned())
    };
    library.update(&id, |g| g.tracking_exe = selected).map(Some)
}

#[tauri::command]
fn set_launch_args(library: State<'_, LocalLibrary>, id: String, args: String) -> Result<Game, String> {
    let args = args.trim().to_string();
    library.update(&id, |game| game.launch_args = args)
}

/// Opens the "Pedir um recomp" form in Cerberus's repository: how players
/// suggest projects for the catalog, with no server of our own.
#[tauri::command]
fn open_recomp_request(app: AppHandle) -> Result<(), String> {
    let url = "https://github.com/rogeriojunior31/CerberusLauncher/issues/new?template=pedir-recomp.yml";
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

/// Opens a catalog recomp's project page; only catalog repos, never an arbitrary link.
#[tauri::command]
fn open_project(app: AppHandle, id: String) -> Result<(), String> {
    let entry = recomp::find(&id).ok_or("Esse recomp não está no catálogo.")?;
    let url = format!("https://github.com/{}", entry.repo);
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
fn play(app: AppHandle, game: Game) -> Result<(), String> {
    // Store games go through their own launcher, which handles updates, DRM and
    // overlays; the rest are started, and watched, by us.
    let url = match &game.source {
        Source::Steam { app_id } => format!("steam://rungameid/{app_id}"),
        Source::Epic { app_name, namespace, item_id } => epic::launch_url(namespace, item_id, app_name),
        Source::Local { .. } => {
            let current = app.state::<LocalLibrary>().get(&game.id)?.ok_or("O jogo foi removido da biblioteca.")?;
            ready_to_play(&current)?;
            let Source::Local { exe } = &current.source else { return Err("Registro do jogo inválido.".into()) };
            return sessions::start(&app, &current, Path::new(exe));
        }
        Source::Recomp { recomp_id, .. } => {
            return app.state::<recomp::Installing>().while_idle(recomp_id, || {
                let current = app.state::<LocalLibrary>().get(&game.id)?.ok_or("O jogo foi removido da biblioteca.")?;
                ready_to_play(&current)?;
                let Source::Recomp { exe, .. } = &current.source else { return Err("Registro do recomp inválido.".into()) };
                sessions::start(&app, &current, Path::new(exe))
            });
        }
    };
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| format!("Não foi possível abrir o launcher da loja: {e}"))
}

/// Play refuses what the button wouldn't offer: a recomp without its game copy
/// would open to an error, a missing program can't open at all.
fn ready_to_play(game: &Game) -> Result<(), String> {
    match recomp::next_step(game, None) {
        cerberus_shared::NextStep::ChooseCopy { .. } => Err(format!("{} precisa da sua cópia do jogo. Escolha o arquivo na página do jogo.", game.name)),
        cerberus_shared::NextStep::Locate { reason } => Err(format!("{reason} Use \"Localizar programa\" na página do jogo.")),
        _ => Ok(()),
    }
}

#[tauri::command]
fn running_games(sessions: State<'_, sessions::Sessions>) -> Vec<String> {
    sessions.running()
}

#[tauri::command]
fn cancel_install(installing: State<'_, recomp::Installing>, id: String) {
    installing.cancel(&id);
}

/// The recomp behind a library entry, and where it is installed.
fn recomp_of(library: &LocalLibrary, id: &str) -> Result<(String, PathBuf), String> {
    let game = library.get(id)?.ok_or("Esse jogo não está mais na biblioteca.")?;
    let Source::Recomp { recomp_id, .. } = game.source else {
        return Err("Só os recomps têm backup de saves.".into());
    };
    Ok((recomp_id, PathBuf::from(game.install_dir)))
}

#[tauri::command]
fn list_backups(library: State<'_, LocalLibrary>, folders: State<'_, recomp::Folders>, id: String) -> Result<Vec<Backup>, String> {
    let (recomp_id, _) = recomp_of(&library, &id)?;
    Ok(recomp::list_backups(&folders, &recomp_id))
}

/// `None` when there was nothing new to copy.
#[tauri::command]
async fn backup_now(app: AppHandle, library: State<'_, LocalLibrary>, folders: State<'_, recomp::Folders>, id: String) -> Result<Option<Backup>, String> {
    let (recomp_id, install_dir) = recomp_of(&library, &id)?;
    let result = recomp::backup_saves(&folders, &recomp_id, &install_dir, "Manual");
    trail::note(&app, &id, "Backup manual", &sessions::backup_outcome(&result));
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
    // The game would overwrite the restored files with what it has in memory.
    if sessions.is_running(&id) {
        return Err("Feche o jogo antes de restaurar um backup.".into());
    }
    let (recomp_id, install_dir) = recomp_of(&library, &id)?;
    let result = recomp::restore_backup(&folders, &recomp_id, &install_dir, &name);
    let outcome = result.as_ref().map(|n| format!("{n} arquivos de {name} restaurados")).map_err(Clone::clone);
    trail::note(&app, &id, "Restaurar backup", &outcome);
    result
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
    let dialog = app.dialog().file().set_title("Em que pasta estão os recomps que você já instalou?");
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
    let entry = recomp::find(&recomp_id).ok_or("Esse recomp não está no catálogo.")?;
    let program = match program {
        Some(path) => PathBuf::from(path),
        None => {
            let title = format!("Onde está o {}? Escolha o programa dele", entry.name);
            let dialog = app.dialog().file().set_title(&title);
            let dialog = if cfg!(windows) { dialog.add_filter("Programa", &["exe"]) } else { dialog };
            let Some(program) = pick_one(dialog).await? else {
                return Ok(None);
            };
            program
        }
    };
    let library = app.state::<LocalLibrary>();
    let result = recomp::adopt::adopt(&library, &entry, &program);
    let outcome = result.as_ref().map(|_| format!("Trazido de {}", program.display())).map_err(Clone::clone);
    trail::note(&app, &recomp::library_id(&entry.id), "Trazer recomp", &outcome);
    let _ = app.emit("catalog-changed", ());
    result.map(Some)
}

/// The catalog recomps a game added by hand could be, the surest first.
#[tauri::command]
fn recomp_suggestions(library: State<'_, LocalLibrary>, id: String) -> Result<Vec<cerberus_shared::AdoptFound>, String> {
    let game = library.get(&id)?.ok_or("Esse jogo não está mais na biblioteca.")?;
    let Source::Local { exe } = &game.source else { return Ok(Vec::new()) };
    let found = recomp::adopt::candidates(Path::new(exe), &recomp::catalog::all());
    Ok(found.into_iter().filter_map(found_view).collect())
}

/// Turns a game added by hand into the catalog recomp it is, keeping its play time.
#[tauri::command]
fn link_to_recomp(app: AppHandle, library: State<'_, LocalLibrary>, id: String, recomp_id: String) -> Result<Game, String> {
    let game = library.get(&id)?.ok_or("Esse jogo não está mais na biblioteca.")?;
    let Source::Local { exe } = &game.source else { return Err("Só jogos adicionados à mão podem ser vinculados.".into()) };
    let entry = recomp::find(&recomp_id).ok_or("Esse recomp não está no catálogo.")?;
    let linked = recomp::adopt::adopt(&library, &entry, Path::new(exe))?;
    trail::note(&app, &linked.id, "Vincular ao catálogo", &Ok(entry.name.clone()));
    let _ = app.emit("catalog-changed", ());
    Ok(linked)
}

/// A recomp's catalog settings offer and where it is installed.
fn settings_of(library: &LocalLibrary, id: &str) -> Result<(Game, game_settings::Offer), String> {
    let game = library.get(id)?.ok_or("Esse jogo não está mais na biblioteca.")?;
    let Source::Recomp { recomp_id, .. } = &game.source else {
        return Err("Só recomps têm ajustes do catálogo.".into());
    };
    let offer = recomp::find(recomp_id).map(|e| e.settings).unwrap_or_default();
    Ok((game, offer))
}

/// The game's own settings and the choice each is on. `None` when the catalog offers none.
#[tauri::command]
fn game_settings(library: State<'_, LocalLibrary>, id: String) -> Result<Option<cerberus_shared::GameSettings>, String> {
    let (game, offer) = settings_of(&library, &id)?;
    if offer.is_empty() {
        return Ok(None);
    }
    Ok(Some(offer.view(Path::new(&game.install_dir), &game.setting_picks)))
}

/// Puts one setting on a choice (`choice: None` puts every setting back on the
/// game's defaults), and returns the settings as they are now.
#[tauri::command]
fn set_game_setting(
    app: AppHandle,
    library: State<'_, LocalLibrary>,
    sessions: State<'_, sessions::Sessions>,
    id: String,
    setting: Option<String>,
    choice: Option<usize>,
) -> Result<cerberus_shared::GameSettings, String> {
    // The game writes its settings when it closes, over whatever was changed meanwhile.
    if sessions.is_running(&id) {
        return Err("Feche o jogo antes de mudar os ajustes.".into());
    }
    let (game, offer) = settings_of(&library, &id)?;
    let install = Path::new(&game.install_dir);
    let mut picks = game.setting_picks.clone();
    let result = match (&setting, choice) {
        (Some(setting), Some(choice)) => offer.choose(install, &mut picks, setting, choice),
        _ => offer.reset(install, &mut picks),
    };
    if let Err(error) = &result {
        trail::note(&app, &id, "Ajustes", &Err(error.clone()));
    }
    result?;
    if picks != game.setting_picks {
        library.update(&id, |g| g.setting_picks = picks.clone())?;
    }
    let updated = library.get(&id)?.ok_or("Esse jogo não está mais na biblioteca.")?;
    Ok(offer.view(install, &updated.setting_picks))
}

/// What the launcher did for a game lately, newest first.
#[tauri::command]
fn game_trail(trail: State<'_, trail::Trail>, id: String) -> Vec<cerberus_shared::TrailEntry> {
    trail.for_game(&id, 30)
}

#[tauri::command]
fn open_folder(app: AppHandle, library: State<'_, LocalLibrary>, id: String) -> Result<(), String> {
    let game = library.get(&id)?.ok_or("Esse jogo não está mais na biblioteca.")?;
    app.opener()
        .open_path(game.install_dir, None::<&str>)
        .map_err(|e| format!("Não foi possível abrir a pasta: {e}"))
}

#[tauri::command]
fn open_backups(app: AppHandle, library: State<'_, LocalLibrary>, folders: State<'_, recomp::Folders>, id: String) -> Result<(), String> {
    let (recomp_id, _) = recomp_of(&library, &id)?;
    let dir = folders.backups_dir(&recomp_id);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| format!("Não foi possível abrir a pasta: {e}"))
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
    fs::copy(source, &target).map_err(|e| format!("Não foi possível copiar a imagem: {e}"))?;
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
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let chosen = settings::load(&data_dir);
            app.manage(LocalLibrary::load(&data_dir));
            app.manage(trail::Trail::new(&data_dir));
            let folders = recomp::Folders::new(&data_dir, chosen.recomps_dir.clone());
            folders.set_cloud(chosen.cloud_dir.clone());
            app.manage(folders);
            app.manage(recomp::Installing::default());
            app.manage(sessions::Sessions::default());
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
            recomp::catalog::load_copy(&data.cache());
            app.manage(RomIndex(std::sync::Mutex::new(roms::load_index(&data.rom_index()))));
            app.manage(Sources::default());
            for url in &chosen.sources {
                remember_source(app.handle(), url, recomp::sources::load_copy(&data.cache(), url));
            }
            publish_sources(app.handle());
            app.manage(data);
            if !chosen.rom_dirs.is_empty() {
                // Games added to the folders since last time show up once this finishes.
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move { rescan_roms(&handle).await });
            }

            let http = reqwest::Client::builder()
                .user_agent("CerberusLauncher")
                .timeout(Duration::from_secs(10))
                .build()?;
            app.manage(http);

            // The UI may only load local files from folders we allow here:
            // covers the user picked, and Steam's own art cache.
            let covers = data_dir.join("covers");
            fs::create_dir_all(&covers)?;
            app.asset_protocol_scope().allow_directory(&covers, true)?;
            app.manage(CoverFolder(covers));

            let art = data_dir.join("cache").join("art");
            fs::create_dir_all(&art)?;
            app.asset_protocol_scope().allow_directory(&art, true)?;
            app.manage(Arc::new(ArtCache::new(art)));

            let steam_covers = steam::root().map(|root| steam::cover_cache_dir(&root));
            if let Some(Ok(steam_covers)) = steam_covers.map(dunce::canonicalize) {
                app.asset_protocol_scope().allow_directory(steam_covers, true)?;
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
            install_recomp,
            open_project,
            open_recomp_request,
            cancel_install,
            running_games,
            list_backups,
            backup_now,
            restore_backup,
            game_trail,
            game_settings,
            scan_for_recomps,
            adopt_recomp,
            recomp_suggestions,
            link_to_recomp,
            set_game_setting,
            open_folder,
            open_backups,
            get_settings,
            finish_setup,
            set_start_in_tv,
            pick_recomps_dir,
            reset_recomps_dir,
            open_data_dir,
            clear_art_cache,
            add_rom_dir,
            remove_rom_dir,
            scan_roms,
            refresh_catalog,
            set_game_copy,
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
        .build(tauri::generate_context!())
        .expect("failed to start Cerberus Launcher")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) { sessions::checkpoint(app); }
        });
}
