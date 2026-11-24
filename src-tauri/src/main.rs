// Release builds on Windows would otherwise open a console window next to the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod art_cache;
mod epic;
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
mod background;
mod favorites;
mod game_notes;
mod achievements;
mod language;
mod dlss;
mod overlay;
mod in_game;
mod data_backup;
mod unlock_toast;
mod disc_image;
mod screenshots;
mod monitors;
mod collections;
mod notify;
mod crash;
mod store_time;
mod shortcut;
mod runtimes;
mod steam;
mod steam_shortcuts;
mod steam_store;
mod steam_details;
mod steam_news;
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
        download_limit: chosen.download_limit,
        slow_while_playing: chosen.slow_while_playing,
        in_game: chosen.in_game.clone(),
        notices: chosen.notices.clone(),
        steamgriddb_key: chosen.steamgriddb_key.clone(),
        start_page: chosen.start_page.clone(),
        site_catalog: chosen.site_catalog,
        close_to_tray: chosen.close_to_tray,
        autostart: {
            use tauri_plugin_autostart::ManagerExt;
            app.autolaunch().is_enabled().unwrap_or(false)
        },
        start_hidden: chosen.start_hidden,
        monitor: chosen.monitor.clone(),
        language: chosen.language.clone(),
        ra_user: chosen.ra_user.clone(),
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
        error: source.is_none().then(|| tr!("Sem cópia local. Atualize a fonte para consultá-la.").to_owned()),
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
        let dialog = app.dialog().file().set_title(tr!("Pasta sincronizada (OneDrive, Google Drive, Dropbox...)"));
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
    let dialog = app.dialog().file().set_title(tr!("Pasta com suas ROMs ou imagens de disco"));
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
    let mut chosen = settings::load(&data.0);
    chosen.steamgriddb_key = (!key.is_empty()).then_some(key);
    settings::save(&data.0, &chosen)?;
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
#[tauri::command]
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

/// Whether a game can take a picture filter, and the one it has.
#[tauri::command]
fn look_status(library: State<'_, LocalLibrary>, id: String) -> Result<cerberus_shared::LookStatus, String> {
    let game = library.get(&id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    let unsupported = match drawing_program(&game) {
        None => Some(tr!("Filtros funcionam em jogos que o Cerberus abre (recomps e jogos adicionados à mão).").to_string()),
        Some(exe) => match looks::route(&exe) {
            looks::Route::Dxgi { .. } => None,
            looks::Route::Unsupported(why) => Some(why),
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
#[tauri::command]
fn finish_setup(app: AppHandle, start_in_tv: bool) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>();
    let mut chosen = settings::load(&data.0);
    chosen.setup_done = true;
    chosen.start_in_tv = start_in_tv;
    settings::save(&data.0, &chosen)?;
    Ok(settings_view(&app))
}

fn retroarch_choice(app: &AppHandle) -> (PathBuf, Option<PathBuf>) {
    let data = app.state::<DataDir>().0.clone();
    let picked = settings::load(&data).retroarch;
    (data, picked)
}

#[tauri::command]
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
    let mut chosen = settings::load(&data);
    chosen.retroarch = None;
    settings::save(&data, &chosen)?;
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
    let mut chosen = settings::load(&data);
    chosen.retroarch = Some(exe.clone());
    settings::save(&data, &chosen)?;
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
#[tauri::command]
fn play_classic(app: AppHandle, path: String, console: String) -> Result<(), String> {
    let (data, picked) = retroarch_choice(&app);
    let exe = emulation::retroarch(&data, picked.as_deref()).ok_or(tr!("Instale o RetroArch em Configurações › Emulação para jogar os clássicos."))?;
    let rom = PathBuf::from(&path);
    // Only files from the player's own ROM folders are started.
    let Some(known) = app.state::<RomIndex>().get().into_iter().find(|r| r.path == rom) else {
        return Err(tr!("Esse arquivo não está nas suas pastas de ROMs.").into());
    };
    let log = data.join("emulators").join("retroarch-session.log");
    connect_retroarch(&data, &exe);
    let result = emulation::disc_list(&data, &rom, &known.discs, &console).and_then(|opened| emulation::launch(&exe, &opened, &console, Some(&log)));
    let id = format!("classic:{path}");
    trail::note(&app, &id, tk!("Jogar clássico"), &result.as_ref().map(|_| tr!("Aberto no RetroArch").to_string()).map_err(Clone::clone));
    let mut child = result?;
    let started = std::time::Instant::now();
    background::classic_started();
    // While it runs: achievements from RetroArch's log appear at once. When it
    // closes: the time played is kept.
    std::thread::spawn(move || {
        let mut read = 0usize;
        loop {
            let done = child.try_wait().ok().flatten().is_some();
            if let Ok(text) = fs::read_to_string(&log) {
                if text.len() > read {
                    for line in text[read..].lines() {
                        if let Some(title) = emulation::unlocked(line) {
                            trail::note(&app, &id, tk!("Conquista"), &Ok(title.clone()));
                            unlock_toast::show(
                                &app,
                                cerberus_shared::UnlockNotice {
                                    title: tr!("Conquista desbloqueada").to_string(),
                                    name: title.clone(),
                                    description: String::new(),
                                    icon: None,
                                    kind: cerberus_shared::UnlockKind::Normal,
                                },
                            );
                        }
                    }
                    read = text.len();
                }
            }
            if done {
                break;
            }
            std::thread::sleep(Duration::from_millis(700));
        }
        let seconds = started.elapsed().as_secs();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default();
        let saved = emulation::add_played(&data, &path, seconds, now);
        play_log::record(&data, &id, seconds, now);
        trail::note(&app, &id, tk!("Partida"), &saved.map(|_| if seconds < 60 { tr!("Menos de 1 min jogado").to_string() } else { format!("{} min jogados", seconds / 60) }));
        let _ = app.emit("catalog-changed", ());
        background::classic_ended(&app);
    });
    Ok(())
}

/// Logs RetroArch in to RetroAchievements with the linked account.
#[tauri::command]
async fn link_retroarch_achievements(app: AppHandle, http: State<'_, reqwest::Client>, password: String) -> Result<cerberus_shared::EmulationView, String> {
    let (data, picked) = retroarch_choice(&app);
    let mut chosen = settings::load(&data);
    let user = chosen.ra_user.clone().ok_or(tr!("Conecte sua conta em Configurações › RetroAchievements primeiro."))?;
    let (name, token) = emulation::connect_token(&http, &user, &password).await?;
    chosen.ra_token = Some(token.clone());
    settings::save(&data, &chosen)?;
    // RetroArch gets it now when it's there, or at the next game otherwise.
    if let Some(exe) = emulation::retroarch(&data, picked.as_deref()) {
        emulation::write_token(&exe, &name, &token)?;
    }
    Ok(emulation::status(&data, picked.as_deref()))
}

/// Keeps RetroArch connected to the account given in Settings (a new RetroArch,
/// or one whose settings were reset, gets it back before the game opens).
fn connect_retroarch(data: &Path, exe: &Path) {
    let chosen = settings::load(data);
    if let (Some(user), Some(token)) = (chosen.ra_user, chosen.ra_token) {
        let _ = emulation::write_token(exe, &user, &token);
    }
}

/// The page the desktop window opens on.
#[tauri::command]
fn set_start_page(app: AppHandle, page: String) -> Result<SettingsView, String> {
    if !matches!(page.as_str(), "home" | "library" | "discover") {
        return Err(tr!("Página inválida.").into());
    }
    let data = app.state::<DataDir>();
    let mut chosen = settings::load(&data.0);
    chosen.start_page = page;
    settings::save(&data.0, &chosen)?;
    Ok(settings_view(&app))
}

/// Opens Cerberus with Windows, or stops it.
#[tauri::command]
fn set_autostart(app: AppHandle, on: bool) -> Result<SettingsView, String> {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    let result = if on { manager.enable() } else { manager.disable() };
    result.map_err(|e| tr!("O Windows não deixou mudar a inicialização: {}", e))?;
    Ok(settings_view(&app))
}

/// Whether Cerberus, opened with Windows, starts in the tray.
#[tauri::command]
fn set_start_hidden(app: AppHandle, on: bool) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>();
    let mut chosen = settings::load(&data.0);
    chosen.start_hidden = on;
    settings::save(&data.0, &chosen)?;
    Ok(settings_view(&app))
}

fn steam_api_account(app: &AppHandle) -> Option<(String, String)> {
    let chosen = settings::load(&app.state::<DataDir>().0);
    chosen.steam_key.zip(chosen.steam_id)
}

/// The Steam account on this PC (the last to sign in), to offer it when connecting.
#[tauri::command]
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
    let mut chosen = settings::load(&data.0);
    chosen.steam_key = Some(key);
    chosen.steam_id = Some(id);
    settings::save(&data.0, &chosen)?;
    Ok(account)
}

#[tauri::command]
fn steam_disconnect(app: AppHandle) -> Result<(), String> {
    let data = app.state::<DataDir>();
    let mut chosen = settings::load(&data.0);
    chosen.steam_key = None;
    chosen.steam_id = None;
    settings::save(&data.0, &chosen)
}

/// Opens Steam's page for the key, the privacy settings, or the profile.
#[tauri::command]
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
async fn achievements_hub(app: AppHandle, http: State<'_, reqwest::Client>, online: Option<bool>) -> Result<cerberus_shared::AchievementsHub, String> {
    let data = app.state::<DataDir>().0.clone();
    Ok(achievements::hub(&http, &data, retro_account(&app), steam_api_account(&app), online.unwrap_or(true)).await)
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
#[tauri::command]
async fn classic_achievements(app: AppHandle, http: State<'_, reqwest::Client>, path: String, console: String) -> Result<Option<cerberus_shared::GameAchievements>, String> {
    let Some((user, key)) = retro_account(&app) else { return Ok(None) };
    let rom = PathBuf::from(&path);
    let Some(known) = app.state::<RomIndex>().get().into_iter().find(|r| r.path == rom) else {
        return Err(tr!("Esse arquivo não está nas suas pastas de ROMs.").into());
    };
    let stem = rom.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let data = app.state::<DataDir>().0.clone();
    let Some(id) = achievements::classic::game_for(&http, &data, &user, &key, &rom, &console, &[known.title.clone(), stem]).await? else {
        return Ok(None);
    };
    let (game, list) = achievements::retro::game(&http, &user, &key, &id.to_string()).await?;
    Ok(Some(cerberus_shared::GameAchievements { game, achievements: list }))
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
    if running_games(app.state::<sessions::Sessions>()).len() > 0 {
        return Err(tr!("Feche o jogo antes de atualizar o Cerberus.").into());
    }
    let data = app.state::<DataDir>().0.clone();
    self_update::install(&app, &http, &data, update).await
}

/// This version's notes once after an update; `None` on a first install or when seen.
#[tauri::command]
async fn whats_new(app: AppHandle, http: State<'_, reqwest::Client>) -> Result<Option<cerberus_shared::AppUpdate>, String> {
    let data = app.state::<DataDir>().0.clone();
    let mut chosen = settings::load(&data);
    let now = self_update::current();
    match chosen.seen_version.as_deref() {
        Some(seen) if self_update::is_newer(now, seen) => self_update::notes_of(&http, now).await,
        Some(_) => Ok(None),
        None => {
            // First run of a fresh install: nothing to tell, just remember this version.
            chosen.seen_version = Some(now.to_string());
            settings::save(&data, &chosen)?;
            Ok(None)
        }
    }
}

#[tauri::command]
fn whats_new_seen(app: AppHandle) -> Result<(), String> {
    let data = app.state::<DataDir>().0.clone();
    let mut chosen = settings::load(&data);
    chosen.seen_version = Some(self_update::current().to_string());
    settings::save(&data, &chosen)
}

/// A game's play sessions, oldest first, for its play-time chart.
#[tauri::command]
fn game_sessions(app: AppHandle, id: String) -> Vec<cerberus_shared::PlaySession> {
    play_log::of_game(&app.state::<DataDir>().0, &id)
}

/// A Steam game's store page (description, pictures, requirements...).
#[tauri::command]
async fn steam_details(app: AppHandle, http: State<'_, reqwest::Client>, app_id: u32) -> Result<cerberus_shared::StoreDetails, String> {
    let data = app.state::<DataDir>().0.clone();
    steam_details::details(&http, &data, app_id).await
}

/// What the developers of these Steam games (app id, name) posted lately.
#[tauri::command]
async fn steam_news(app: AppHandle, http: State<'_, reqwest::Client>, games: Vec<(u32, String)>) -> Result<Vec<cerberus_shared::NewsItem>, String> {
    let data = app.state::<DataDir>().0.clone();
    Ok(steam_news::latest(&http, &data, games).await)
}

/// Opens a post from Novidades in the browser; only Steam's own links.
#[tauri::command]
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
    achievements::game(&http, &data, source, &key, retro_account(&app), steam_api_account(&app)).await
}

/// The monitors plugged in.
#[tauri::command]
fn list_monitors(app: AppHandle) -> Vec<cerberus_shared::MonitorView> {
    app.get_webview_window("main").map(|w| monitors::list(&w)).unwrap_or_default()
}

/// Picks the app's language (none: Windows'). The window reloads itself to show it.
#[tauri::command]
fn set_language(app: AppHandle, code: Option<String>) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>();
    let mut chosen = settings::load(&data.0);
    chosen.language = code.filter(|c| !c.is_empty());
    settings::save(&data.0, &chosen)?;
    language::apply(chosen.language.as_deref());
    background::retitle_tray(&app);
    Ok(settings_view(&app))
}

/// Picks the monitor Cerberus opens on (none: where Windows puts it) and moves there now.
#[tauri::command]
fn set_monitor(app: AppHandle, name: Option<String>) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>();
    let mut chosen = settings::load(&data.0);
    chosen.monitor = name.filter(|n| !n.is_empty());
    settings::save(&data.0, &chosen)?;
    if let Some(window) = app.get_webview_window("main") {
        monitors::place(&window, chosen.monitor.as_deref());
    }
    Ok(settings_view(&app))
}

/// Before Big Picture fills the screen: the chosen monitor's.
#[tauri::command]
fn to_chosen_monitor(app: AppHandle) {
    let chosen = settings::load(&app.state::<DataDir>().0);
    if let Some(window) = app.get_webview_window("main") {
        monitors::place(&window, chosen.monitor.as_deref());
    }
}

/// Whether closing the window keeps Cerberus in the tray.
#[tauri::command]
fn set_close_to_tray(app: AppHandle, on: bool) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>();
    let mut chosen = settings::load(&data.0);
    chosen.close_to_tray = on;
    settings::save(&data.0, &chosen)?;
    Ok(settings_view(&app))
}

/// Turns recomplauncher.com's catalog on or off; Cerberus's own list stays.
#[tauri::command]
async fn set_site_catalog(app: AppHandle, http: State<'_, reqwest::Client>, on: bool) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>();
    let mut chosen = settings::load(&data.0);
    chosen.site_catalog = on;
    settings::save(&data.0, &chosen)?;
    recomp::catalog::use_site(on);
    if on {
        recomp::catalog::load_copy(&data.cache());
        let _ = recomp::catalog::refresh(&http, &data.cache(), false).await;
    }
    let _ = app.emit("catalog-changed", ());
    Ok(settings_view(&app))
}

/// Shows the first-run setup again the next time it is asked for.
#[tauri::command]
fn rerun_setup(app: AppHandle) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>();
    let mut chosen = settings::load(&data.0);
    chosen.setup_done = false;
    settings::save(&data.0, &chosen)?;
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
    let mut chosen = settings::load(&data);
    chosen.ra_user = Some(profile.user.clone());
    chosen.ra_key = Some(key);
    if let Some((name, token)) = &token {
        chosen.ra_token = Some(token.clone());
        if let Some(exe) = emulation::retroarch(&data, picked.as_deref()) {
            let _ = emulation::write_token(&exe, name, token);
        }
    } else if chosen.ra_user.as_deref() != Some(profile.user.as_str()) {
        chosen.ra_token = None;
    }
    settings::save(&data, &chosen)?;
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
#[tauri::command]
fn ra_disconnect(app: AppHandle) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>();
    let mut chosen = settings::load(&data.0);
    chosen.ra_user = None;
    chosen.ra_key = None;
    chosen.ra_token = None;
    settings::save(&data.0, &chosen)?;
    Ok(settings_view(&app))
}

/// Opens RetroAchievements: the player's profile, or the page with their API key.
#[tauri::command]
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
#[tauri::command]
fn set_download_limit(app: AppHandle, mb: u32, slow_while_playing: bool) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>();
    let mut chosen = settings::load(&data.0);
    chosen.download_limit = mb.min(1000);
    chosen.slow_while_playing = slow_while_playing;
    settings::save(&data.0, &chosen)?;
    recomp::throttle::set_limit(chosen.download_limit, chosen.slow_while_playing);
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
    let data = app.state::<DataDir>().0.clone();
    let (count, _) = tauri::async_runtime::spawn_blocking(move || data_backup::restore(&data, &from)).await.map_err(|e| e.to_string())??;
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
#[tauri::command]
fn epic_open(app: AppHandle) -> Result<(), String> {
    app.opener().open_url("com.epicgames.launcher://store/library", None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_notices(app: AppHandle, choices: cerberus_shared::Notices) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>();
    let mut chosen = settings::load(&data.0);
    chosen.notices = choices;
    settings::save(&data.0, &chosen)?;
    notify::set(chosen.notices.clone());
    Ok(settings_view(&app))
}

#[tauri::command]
fn set_in_game(app: AppHandle, choices: cerberus_shared::InGame) -> Result<SettingsView, String> {
    let data = app.state::<DataDir>();
    let mut chosen = settings::load(&data.0);
    let corner = if ["br", "bl", "tr", "tl"].contains(&choices.corner.as_str()) { choices.corner.clone() } else { "br".into() };
    chosen.in_game = cerberus_shared::InGame { corner, ..choices };
    settings::save(&data.0, &chosen)?;
    in_game::set(chosen.in_game.clone());
    Ok(settings_view(&app))
}

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
    let dialog = app.dialog().file().set_title(tr!("Pasta para instalar recomps"));
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
    fs::create_dir_all(&target).map_err(|e| tr!("Não foi possível usar {0}: {1}", target.display(), e))?;
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
    owned_not_installed(&app, &http, &mut games);
    games.sort_by_key(|game| game.name.to_lowercase());
    let mut missing = Vec::new();
    art.use_copies_for_games(&mut games, &mut missing);
    // The player's own pictures win over every other art.
    let custom = app.state::<art_custom::CustomArt>();
    games.iter_mut().for_each(|game| custom.apply(game));
    store_time::apply(&app.state::<DataDir>().0, &mut games);
    // Steam counts its games' time on every PC; that beats what Cerberus saw here.
    let steam_id = settings::load(&app.state::<DataDir>().0).steam_id;
    let data_dir = app.state::<DataDir>().0.clone();
    let steam_time = tauri::async_runtime::spawn_blocking(move || achievements::steam_playtimes(&data_dir, steam_id.as_deref())).await.unwrap_or_default();
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
    let favorite = favorites::load(&app.state::<DataDir>().0);
    games.iter_mut().for_each(|game| game.favorite = favorite.contains(&game.id));
    collections::apply(&app.state::<DataDir>().0, &mut games);
    game_notes::apply(&app.state::<DataDir>().0, &mut games);
    let paused = settings::load(&app.state::<DataDir>().0).paused_updates;
    games.iter_mut().for_each(|g| g.updates_paused = paused.contains(&g.id));
    art.fetch_later(http.inner().clone(), missing);
    Ok(games)
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
    let installed: std::collections::HashSet<String> = games.iter().map(|g| g.id.clone()).collect();
    let covers = steam::root().map(|r| steam::cover_cache_dir(&r));
    for o in &owned {
        let game_id = format!("steam:{}", o.app);
        if !installed.contains(&game_id) && !o.name.trim().is_empty() {
            games.push(steam::not_installed(o.app, &o.name, covers.as_deref()));
        }
    }
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
        return Err(tr!("Feche o jogo antes de removê-lo.").into());
    }
    // Files first: if they can't go, the game stays listed. Saves are copied
    // before anything is deleted, and the copies outlive the uninstall.
    if let Source::Recomp { recomp_id, .. } = &game.source {
        installing.while_idle(recomp_id, || {
            if sessions.is_running(&id) {
                return Err(tr!("Feche o jogo antes de removê-lo.").into());
            }
            recomp::backup_saves(&recomps, recomp_id, Path::new(&game.install_dir), tk!("Antes de desinstalar"))?;
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
        // Paused: the bytes wait for the next install, nothing went wrong.
        Err(message) if message == cerberus_shared::PAUSED => return Err(message),
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
    let game = library.get(&id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    let Source::Recomp { recomp_id, .. } = &game.source else {
        return Err(tr!("Só recomps usam uma cópia do jogo.").into());
    };
    // A recomp that takes only extracted folders always gets the folder picker.
    let folder = folder || recomp::find(recomp_id).is_some_and(|e| e.copy_accepts == "folder");
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
    let handle = app.clone();
    let setup = tauri::async_runtime::spawn_blocking(move || recomp::use_game_copy(&handle.state::<LocalLibrary>(), &id, &copy));
    setup.await.map_err(|e| e.to_string())?.map(Some)
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
            let placed = tauri::async_runtime::spawn_blocking(move || match kind {
                gamebanana::Kind::Packs => gamebanana::place_packs(&archive, &mods_folder),
                gamebanana::Kind::Hedge(user) => hedge::place(&archive, &mods_folder, user, mod_id, &name, &old_files),
            })
            .await
            .map_err(|e| e.to_string())??;
            // Packs the new version no longer has go; the ones it has were just replaced.
            if kind == gamebanana::Kind::Packs {
                let gone: Vec<String> = old.iter().filter(|f| !placed.contains(f)).cloned().collect();
                mods::remove_files(&folder, &gone);
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
    let (host, folder, record) = mod_setup(&app, &id).await?.ok_or(tr!("Esse jogo não tem mods."))?;
    let mut installed = mods::load_installed(&record);
    if let Some((_, files)) = installed.0.remove(&full_name) {
        match (host, gamebanana::id_of(&full_name)) {
            (ModHost::GameBanana(_, gamebanana::Kind::Hedge(user)), Some(mod_id)) => hedge::remove(&folder, user, mod_id, &files)?,
            _ => mods::remove_files(&folder, &files),
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

#[tauri::command]
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

/// Opens SteamGridDB's page where players make their free API key.
#[tauri::command]
fn open_steamgriddb_keys(app: AppHandle) -> Result<(), String> {
    app.opener().open_url("https://www.steamgriddb.com/profile/preferences/api", None::<&str>).map_err(|e| e.to_string())
}

/// Opens a catalog recomp's project page; only catalog repos, never an arbitrary link.
#[tauri::command]
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

#[tauri::command]
fn play(app: AppHandle, game: Game) -> Result<(), String> {
    // Store games go through their own launcher, which handles updates, DRM and
    // overlays; the rest are started, and watched, by us.
    let url = match &game.source {
        // Not on this PC yet: Steam's own install window.
        Source::Steam { app_id } if game.install_dir.is_empty() => format!("steam://install/{app_id}"),
        Source::Steam { app_id } => format!("steam://rungameid/{app_id}"),
        Source::Epic { app_name, namespace, item_id } => epic::launch_url(namespace, item_id, app_name),
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

/// Play refuses what the button wouldn't offer: a recomp without its game copy
/// would open to an error, a missing program can't open at all.
fn ready_to_play(game: &Game) -> Result<(), String> {
    match recomp::next_step(game, None) {
        cerberus_shared::NextStep::ChooseCopy { .. } => Err(tr!("{} precisa da sua cópia do jogo. Escolha o arquivo na página do jogo.", game.name)),
        cerberus_shared::NextStep::Locate { reason } => Err(tr!("{} Use \"Localizar programa\" na página do jogo.", reason)),
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

/// Moves an installed recomp to a folder the player picks (another drive, say).
/// `None` when the picker is cancelled.
#[tauri::command]
async fn move_game(app: AppHandle, id: String) -> Result<Option<Game>, String> {
    let game = app.state::<LocalLibrary>().get(&id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    let Source::Recomp { recomp_id, .. } = &game.source else {
        return Err(tr!("Só recomps instalados pelo Cerberus podem ser movidos por aqui.").into());
    };
    if app.state::<sessions::Sessions>().is_running(&id) {
        return Err(tr!("Feche o jogo antes de movê-lo.").into());
    }
    app.state::<recomp::Installing>().while_idle(recomp_id, || Ok(()))?;
    let dialog = app.dialog().file().set_title(tr!("Escolha para onde mover o jogo"));
    let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_folder());
    let Some(picked) = picked.await.map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let into = picked.into_path().map_err(|e| e.to_string())?;
    // Copying to another drive can take minutes.
    let handle = app.clone();
    let moved = tauri::async_runtime::spawn_blocking(move || {
        recomp::relocate::move_install(&handle.state::<recomp::Folders>(), &handle.state::<LocalLibrary>(), &id, &into)
    })
    .await
    .map_err(|e| e.to_string())??;
    trail::note(&app, &moved.id, tk!("Mover"), &Ok(tr!("Movido para {}", moved.install_dir)));
    Ok(Some(moved))
}

#[tauri::command]
fn pause_install(installing: State<'_, recomp::Installing>, id: String) {
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

#[tauri::command]
fn list_backups(library: State<'_, LocalLibrary>, folders: State<'_, recomp::Folders>, id: String) -> Result<Vec<Backup>, String> {
    let (recomp_id, _) = recomp_of(&library, &id)?;
    Ok(recomp::list_backups(&folders, &recomp_id))
}

/// `None` when there was nothing new to copy.
#[tauri::command]
async fn backup_now(app: AppHandle, library: State<'_, LocalLibrary>, folders: State<'_, recomp::Folders>, id: String) -> Result<Option<Backup>, String> {
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
    // The game would overwrite the restored files with what it has in memory.
    if sessions.is_running(&id) {
        return Err(tr!("Feche o jogo antes de restaurar um backup.").into());
    }
    let (recomp_id, install_dir) = recomp_of(&library, &id)?;
    let result = recomp::restore_backup(&folders, &recomp_id, &install_dir, &name);
    let outcome = result.as_ref().map(|n| tr!("{} arquivos de {} restaurados", n, name)).map_err(Clone::clone);
    trail::note(&app, &id, tk!("Restaurar backup"), &outcome);
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
#[tauri::command]
fn recomp_suggestions(library: State<'_, LocalLibrary>, id: String) -> Result<Vec<cerberus_shared::AdoptFound>, String> {
    let game = library.get(&id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    let Source::Local { exe } = &game.source else { return Ok(Vec::new()) };
    let found = recomp::adopt::candidates(Path::new(exe), &recomp::catalog::all());
    Ok(found.into_iter().filter_map(found_view).collect())
}

/// Turns a game added by hand into the catalog recomp it is, keeping its play time.
#[tauri::command]
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
#[tauri::command]
fn set_updates_paused(app: AppHandle, id: String, on: bool) -> Result<(), String> {
    let data = app.state::<DataDir>().0.clone();
    let mut chosen = settings::load(&data);
    if on {
        chosen.paused_updates.insert(id);
    } else {
        chosen.paused_updates.remove(&id);
    }
    settings::save(&data, &chosen)
}

/// The player's status, rating and notes for a game.
#[tauri::command]
fn set_game_note(app: AppHandle, id: String, status: Option<cerberus_shared::GameStatus>, rating: u8, notes: String) -> Result<(), String> {
    game_notes::set(&app.state::<DataDir>().0, &id, game_notes::Note { status, rating, notes })
}

#[tauri::command]
fn set_favorite(app: AppHandle, id: String, on: bool) -> Result<(), String> {
    favorites::set(&app.state::<DataDir>().0, &id, on)
}

/// Hides a game from the library, or shows it again.
#[tauri::command]
fn set_hidden(app: AppHandle, id: String, on: bool) -> Result<(), String> {
    collections::set_hidden(&app.state::<DataDir>().0, &id, on)
}

/// The player's collections, in the order they were made.
#[tauri::command]
fn list_collections(app: AppHandle) -> Vec<String> {
    collections::names(&app.state::<DataDir>().0)
}

#[tauri::command]
fn create_collection(app: AppHandle, name: String) -> Result<String, String> {
    collections::create(&app.state::<DataDir>().0, &name)
}

/// Puts a game in a collection (made when new) or takes it out.
#[tauri::command]
fn set_in_collection(app: AppHandle, name: String, id: String, on: bool) -> Result<(), String> {
    collections::set_in(&app.state::<DataDir>().0, &name, &id, on)
}

#[tauri::command]
fn rename_collection(app: AppHandle, old: String, name: String) -> Result<String, String> {
    collections::rename(&app.state::<DataDir>().0, &old, &name)
}

#[tauri::command]
fn delete_collection(app: AppHandle, name: String) -> Result<(), String> {
    collections::delete(&app.state::<DataDir>().0, &name)
}

/// Opens the folder holding a classic; only files from the player's ROM folders.
#[tauri::command]
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
#[tauri::command]
fn create_shortcut(app: AppHandle, library: State<'_, LocalLibrary>, id: String) -> Result<String, String> {
    let game = match library.get(&id)? {
        Some(game) => game,
        None => {
            let mut found = steam::installed_games();
            found.extend(epic::installed_games());
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

#[tauri::command]
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
    let icon = match &game.source {
        Source::Recomp { exe, .. } | Source::Local { exe } => Some(PathBuf::from(exe)),
        _ => None,
    };
    let launcher = std::env::current_exe().map_err(|e| e.to_string())?;
    let shortcut = steam_shortcuts::Shortcut { name: game.name.clone(), launcher, game_id: game.id.clone(), icon };
    let art = app.state::<Arc<ArtCache>>().inner().clone();
    let cover = art_bytes(&http, &art, game.cover_path.as_deref(), game.cover_url.as_deref()).await;
    let hero = art_bytes(&http, &art, game.hero_path.as_deref(), game.hero_url.as_deref()).await;
    for config in &configs {
        let mut list = steam_shortcuts::load(config)?;
        steam_shortcuts::add(&mut list, &shortcut);
        steam_shortcuts::save(config, &list)?;
        steam_shortcuts::place_art(config, shortcut.app_id(), cover.as_deref(), hero.as_deref());
    }
    trail::note(&app, &game.id, tk!("Steam"), &Ok(tr!("Adicionado à Steam").to_string()));
    Ok(tr!("{} está na Steam. Ele aparece na biblioteca quando a Steam abrir.", game.name))
}

#[tauri::command]
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
#[tauri::command]
fn in_steam(id: String) -> bool {
    let Some(steam) = steam::root() else { return false };
    steam_shortcuts::config_dirs(&steam).iter().any(|config| steam_shortcuts::load(config).is_ok_and(|list| steam_shortcuts::contains(&list, &id)))
}

fn pc_now() -> cerberus_shared::PcStatus {
    let vc_version = runtimes::vc_version();
    let vc_ok = vc_version.as_deref().is_some_and(runtimes::recent_enough);
    cerberus_shared::PcStatus { vc_version, vc_ok, dx_missing: runtimes::dx_missing() }
}

/// What the PC has that games need.
#[tauri::command]
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
#[tauri::command]
fn recent_activity(trail: State<'_, trail::Trail>) -> Vec<cerberus_shared::TrailEntry> {
    trail.recent(40)
}

#[tauri::command]
fn game_trail(trail: State<'_, trail::Trail>, id: String) -> Vec<cerberus_shared::TrailEntry> {
    trail.for_game(&id, 30)
}

/// The pictures taken of a game, newest first.
#[tauri::command]
fn list_screenshots(app: AppHandle, id: String) -> Vec<cerberus_shared::Screenshot> {
    screenshots::list(&app.state::<DataDir>().0, &id)
}

/// Opens one picture in the PC's image viewer; only pictures Cerberus took.
#[tauri::command]
fn open_screenshot(app: AppHandle, path: String) -> Result<(), String> {
    if !screenshots::is_ours(&app.state::<DataDir>().0, Path::new(&path)) {
        return Err(tr!("Essa foto não é do Cerberus.").into());
    }
    app.opener().open_path(path, None::<&str>).map_err(|e| tr!("Não foi possível abrir a foto: {}", e))
}

#[tauri::command]
fn open_screenshots_folder(app: AppHandle, id: String) -> Result<(), String> {
    let dir = screenshots::dir_of(&app.state::<DataDir>().0, &id);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|e| tr!("Não foi possível abrir a pasta: {}", e))
}

#[tauri::command]
fn open_folder(app: AppHandle, library: State<'_, LocalLibrary>, id: String) -> Result<(), String> {
    let game = library.get(&id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    app.opener()
        .open_path(game.install_dir, None::<&str>)
        .map_err(|e| tr!("Não foi possível abrir a pasta: {}", e))
}

#[tauri::command]
fn open_backups(app: AppHandle, library: State<'_, LocalLibrary>, folders: State<'_, recomp::Folders>, id: String) -> Result<(), String> {
    let (recomp_id, _) = recomp_of(&library, &id)?;
    let dir = folders.backups_dir(&recomp_id);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| tr!("Não foi possível abrir a pasta: {}", e))
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
            if let Some(id) = shortcut::play_request(&args) {
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
            let chosen = settings::load(&data_dir);
            language::apply(chosen.language.as_deref());
            app.manage(LocalLibrary::load(&data_dir));
            // Filters left beside a game by a crash or a forced close go now.
            looks::take_back_all(&data_dir);
            app.manage(trail::Trail::new(&data_dir));
            app.manage(unlock_toast::Queue::default());
            app.manage(self_update::Found::default());
            let folders = recomp::Folders::new(&data_dir, chosen.recomps_dir.clone());
            folders.set_cloud(chosen.cloud_dir.clone());
            app.manage(folders);
            app.manage(recomp::Installing::default());
            awake::start(app.handle().clone());
            app.manage(sessions::Sessions::default());
            let asked: Vec<String> = std::env::args().collect();
            app.manage(PlayRequest(std::sync::Mutex::new(shortcut::play_request(&asked))));
            background::tray(app.handle())?;
            {
                let chosen = settings::load(&data_dir);
                recomp::throttle::set_limit(chosen.download_limit, chosen.slow_while_playing);
            }
            in_game::set(chosen.in_game.clone());
            notify::set(chosen.notices.clone());
            store_time::watch(app.handle().clone());
            self_update::watch(app.handle().clone());
            app.manage(overlay::Overlay::default());
            screenshots::watch(app.handle().clone());
            if let Some(window) = app.get_webview_window("main") {
                monitors::place(&window, chosen.monitor.as_deref());
            }
            if chosen.start_hidden && std::env::args().any(|a| a == "--autostart") {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
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
            recomp::catalog::use_site(chosen.site_catalog);
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
            app.manage(art_custom::CustomArt::new(&data_dir, &covers));
            app.manage(CoverFolder(covers));

            let art = data_dir.join("cache").join("art");
            fs::create_dir_all(&art)?;
            app.asset_protocol_scope().allow_directory(&art, true)?;
            app.manage(Arc::new(ArtCache::new(art)));

            let shots = screenshots::root(&data_dir);
            fs::create_dir_all(&shots)?;
            app.asset_protocol_scope().allow_directory(&shots, true)?;

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
            pause_install,
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
            steam_news,
            set_tray_games,
            open_news,
            game_sessions,
            set_game_note,
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
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" && background::on_close(window, window.app_handle()) {
                    api.prevent_close();
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
fn days_are_named_as_on_the_calendar() {
    assert_eq!(day_of(0), "1970-01-01");
    assert_eq!(day_of(951_782_400), "2000-02-29");
    assert_eq!(day_of(1_795_392_000), "2026-11-23");
}
