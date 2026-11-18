// Release builds on Windows would otherwise open a console window next to the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod art_cache;
mod epic;
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
mod xdvdfs;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use art_cache::ArtCache;
use cerberus_shared::{Backup, Game, InstallProgress, RecompEntry, SettingsView, Source};
use library::LocalLibrary;
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
        cloud_dir: chosen.cloud_dir.map(|d| d.to_string_lossy().into_owned()),
        sources: app.state::<Sources>().0.lock().unwrap_or_else(|p| p.into_inner()).iter().map(|(view, _)| view.clone()).collect(),
    }
}

/// The player's catalog sources, with their games, as last read.
#[derive(Default)]
struct Sources(std::sync::Mutex<Vec<(cerberus_shared::SourceView, Vec<recomp::CatalogEntry>)>>);

/// Hands the sources' games to the catalog and tells the window.
fn publish_sources(app: &AppHandle) {
    let lists = app.state::<Sources>().0.lock().unwrap_or_else(|p| p.into_inner()).iter().map(|(_, games)| games.clone()).collect();
    recomp::catalog::set_extras(lists);
    let _ = app.emit("catalog-changed", ());
}

fn remember_source(app: &AppHandle, url: &str, name: String, games: Vec<recomp::CatalogEntry>) {
    let view = cerberus_shared::SourceView { url: url.to_string(), name, games: games.len() };
    let sources = app.state::<Sources>();
    let mut list = sources.0.lock().unwrap_or_else(|p| p.into_inner());
    list.retain(|(v, _)| v.url != url);
    list.push((view, games));
}

#[tauri::command]
async fn add_source(app: AppHandle, http: State<'_, reqwest::Client>, url: String) -> Result<SettingsView, String> {
    let url = url.trim().to_string();
    let cache = app.state::<DataDir>().cache();
    let (name, games) = recomp::sources::fetch(&http, &cache, &url).await?;
    remember_source(&app, &url, name, games);
    let data_dir = app.state::<DataDir>().0.clone();
    let mut chosen = settings::load(&data_dir);
    if !chosen.sources.contains(&url) {
        chosen.sources.push(url);
        settings::save(&data_dir, &chosen)?;
    }
    publish_sources(&app);
    Ok(settings_view(&app))
}

#[tauri::command]
fn remove_source(app: AppHandle, url: String) -> Result<SettingsView, String> {
    let data_dir = app.state::<DataDir>().0.clone();
    let mut chosen = settings::load(&data_dir);
    chosen.sources.retain(|u| *u != url);
    settings::save(&data_dir, &chosen)?;
    app.state::<Sources>().0.lock().unwrap_or_else(|p| p.into_inner()).retain(|(v, _)| v.url != url);
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
    // The player's sources too; one that fails keeps its last copy.
    for url in settings::load(&app.state::<DataDir>().0).sources {
        if let Ok((name, games)) = recomp::sources::fetch(&http, &cache, &url).await {
            remember_source(&app, &url, name, games);
        }
    }
    publish_sources(&app);
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
async fn list_games(
    library: State<'_, LocalLibrary>,
    art: State<'_, Arc<ArtCache>>,
    http: State<'_, reqwest::Client>,
) -> Result<Vec<Game>, String> {
    let mut games = library.games();
    games.iter_mut().for_each(recomp::apply_art);
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
    let game = library.get(&id).ok_or("Esse jogo não está mais na biblioteca.")?;
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
    id: String,
) -> Result<(), String> {
    let Some(game) = library.get(&id) else {
        return Ok(());
    };
    if sessions.is_running(&id) {
        return Err("Feche o jogo antes de removê-lo.".into());
    }
    // Files first: if they can't go, the game stays listed. Saves are copied
    // before anything is deleted, and the copies outlive the uninstall.
    if let Source::Recomp { recomp_id, .. } = &game.source {
        recomp::backup_saves(&recomps, recomp_id, Path::new(&game.install_dir), "Antes de desinstalar")?;
        recomp::delete_files(&recomps, recomp_id, Path::new(&game.install_dir))?;
    }
    library.remove(&id)?;
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
    let mut entries = recomp::entries(&library, &owned_games(&app));
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
    let result = recomp::install(&http, &folders, &library, &installing, &id, &report).await;
    let game = match result {
        Ok(game) => game,
        Err(message) => {
            report(cerberus_shared::InstallStep::Failed { message: message.clone() });
            return Err(message);
        }
    };
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
    let game = library.get(&id).ok_or("Esse jogo não está mais na biblioteca.")?;
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
    let game = library.get(id).ok_or("Esse jogo não está mais na biblioteca.")?;
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
    let dialog = app.dialog().file().set_title("Onde o instalador colocou o jogo? Escolha o programa do jogo");
    let dialog = if cfg!(windows) { dialog.add_filter("Programa", &["exe"]) } else { dialog };
    let Some(program) = pick_one(dialog).await? else {
        return Ok(None);
    };
    setup_runner::use_program(&app, &id, &program).map(Some)
}

#[tauri::command]
fn set_launch_args(library: State<'_, LocalLibrary>, id: String, args: String) -> Result<Game, String> {
    let args = args.trim().to_string();
    library.update(&id, |game| game.launch_args = args)
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
        Source::Local { exe } | Source::Recomp { exe, .. } => return sessions::start(&app, &game, Path::new(exe)),
    };
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| format!("Não foi possível abrir o launcher da loja: {e}"))
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
    let game = library.get(id).ok_or("Esse jogo não está mais na biblioteca.")?;
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
async fn backup_now(library: State<'_, LocalLibrary>, folders: State<'_, recomp::Folders>, id: String) -> Result<Option<Backup>, String> {
    let (recomp_id, install_dir) = recomp_of(&library, &id)?;
    recomp::backup_saves(&folders, &recomp_id, &install_dir, "Manual")
}

#[tauri::command]
async fn restore_backup(
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
    recomp::restore_backup(&folders, &recomp_id, &install_dir, &name)
}

#[tauri::command]
fn open_folder(app: AppHandle, library: State<'_, LocalLibrary>, id: String) -> Result<(), String> {
    let game = library.get(&id).ok_or("Esse jogo não está mais na biblioteca.")?;
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
            let folders = recomp::Folders::new(&data_dir, chosen.recomps_dir.clone());
            folders.set_cloud(chosen.cloud_dir.clone());
            app.manage(folders);
            app.manage(recomp::Installing::default());
            app.manage(sessions::Sessions::default());

            // The catalog and the last ROM scan from disk, so the window has them at once.
            let data = DataDir(data_dir.clone());
            recomp::catalog::load_copy(&data.cache());
            app.manage(RomIndex(std::sync::Mutex::new(roms::load_index(&data.rom_index()))));
            app.manage(Sources::default());
            for url in &chosen.sources {
                if let Some((name, games)) = recomp::sources::load_copy(&data.cache(), url) {
                    remember_source(app.handle(), url, name, games);
                }
            }
            let lists = app.state::<Sources>().0.lock().unwrap_or_else(|p| p.into_inner()).iter().map(|(_, g)| g.clone()).collect();
            recomp::catalog::set_extras(lists);
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
            add_local_game,
            rename_game,
            find_cover,
            pick_cover,
            remove_game,
            list_recomps,
            install_recomp,
            open_project,
            cancel_install,
            running_games,
            list_backups,
            backup_now,
            restore_backup,
            open_folder,
            open_backups,
            get_settings,
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
            locate_game,
            set_cloud_dir,
            add_source,
            remove_source,
            list_mods,
            install_mod,
            remove_mod,
            play
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Cerberus Launcher");
}
