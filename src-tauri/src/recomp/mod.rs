//! Recomps: native PC versions of console games, installed from each
//! developer's own GitHub releases. The player brings their own copy of the
//! game; every recomp in the catalog asks for it the first time it opens.

pub mod adopt;
mod assets;
pub mod catalog;
pub(crate) mod cloud;
mod forge;
mod hoster;
mod releases;
mod install;
pub mod manifest;
pub(crate) mod saves;
pub mod sources;
mod translate;
pub(crate) mod update;
pub mod relocate;
pub mod throttle;

use cerberus_shared::tk;
use cerberus_shared::tr;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use cerberus_shared::{Backup, CopyHandoff, Game, InstallStep, NextStep, Release, RecompEntry, Source};
use reqwest::Client;

use crate::library::LocalLibrary;
pub use assets::Asset;
pub use install::Halt;
use assets::Os;
pub use catalog::{CatalogEntry, find};

/// Downloads a file with resume and retries, for other parts of the launcher.
pub async fn download_file(
    client: &Client,
    asset: &Asset,
    dir: &Path,
    cancel: &install::Halt,
    report: impl Fn(u64, u64) + Send,
) -> Result<PathBuf, String> {
    install::download(client, asset, dir, cancel, report).await
}

/// Unpacks a .zip archive into a folder.
pub fn extract_zip(archive: &Path, to: &Path) -> Result<(), String> {
    install::unzip(archive, to)
}

/// Unpacks a .7z archive into a folder.
pub fn extract_7z(archive: &Path, to: &Path) -> Result<(), String> {
    install::un7z(archive, to)
}

/// This PC's system, as the catalog names it.
fn os_key() -> &'static str {
    match Os::current() {
        Os::Windows => "windows",
        Os::Linux => "linux",
        Os::MacOs => "macos",
    }
}

/// Where recomps live on disk.
pub struct Folders {
    /// Where new installs go; the player can move it to another drive in Settings.
    installs: RwLock<PathBuf>,
    /// Where installs go when the player hasn't picked a folder.
    pub default_installs: PathBuf,
    downloads: PathBuf,
    cache: PathBuf,
    /// Save backups; kept when a recomp is uninstalled, so a reinstall can have them back.
    saves: PathBuf,
    /// A folder another program syncs (OneDrive, Google Drive, Dropbox), where
    /// backups are copied so other PCs get them.
    cloud: RwLock<Option<PathBuf>>,
}

/// Written into every install folder we make. Uninstalling deletes a folder only
/// when it has this file, so a wrong path in the library can never delete
/// someone else's files.
const MARKER: &str = ".cerberus-install";

impl Folders {
    pub fn new(data_dir: &Path, installs: Option<PathBuf>) -> Self {
        let default_installs = data_dir.join("recomps");
        Self {
            installs: RwLock::new(installs.unwrap_or_else(|| default_installs.clone())),
            default_installs,
            downloads: data_dir.join("downloads"),
            cache: data_dir.join("cache").join("releases"),
            saves: data_dir.join("saves"),
            cloud: RwLock::new(None),
        }
    }

    pub fn set_cloud(&self, dir: Option<PathBuf>) {
        *self.cloud.write().unwrap_or_else(|p| p.into_inner()) = dir;
    }

    /// This recomp's folder inside the cloud folder, when one is set.
    fn cloud_dir(&self, recomp_id: &str) -> Option<PathBuf> {
        let cloud = self.cloud.read().unwrap_or_else(|p| p.into_inner()).clone()?;
        Some(cloud.join("Cerberus Saves").join(recomp_id))
    }

    pub fn installs(&self) -> PathBuf {
        self.installs.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// New installs go here from now on; games already installed stay where they are.
    pub fn set_installs(&self, dir: PathBuf) {
        *self.installs.write().unwrap_or_else(|p| p.into_inner()) = dir;
    }

    pub fn backups_dir(&self, recomp_id: &str) -> PathBuf {
        self.saves.join(recomp_id)
    }

    fn install_dir(&self, id: &str) -> PathBuf {
        self.installs().join(id)
    }
}

/// Beside the install folder, so moving files in is a rename on the same drive.
fn staging_for(target: &Path, id: &str) -> PathBuf {
    let parent = target.parent().unwrap_or(target);
    parent.join(format!(".staging-{id}"))
}

/// Recomps being installed right now, with the flag that cancels each. Also stops
/// a second click from starting a second copy.
/// Paused downloads are kept too, so cancelling one later throws its bytes away.
#[derive(Default)]
pub struct Installing(Mutex<HashMap<String, Arc<install::Halt>>>, Mutex<HashMap<String, Arc<install::Halt>>>);

impl Installing {
    pub fn while_idle<T>(&self, id: &str, action: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let key = find(id).map(|entry| entry.id).unwrap_or_else(|| id.to_owned());
        let busy = self.0.lock().unwrap_or_else(|p| p.into_inner());
        if busy.contains_key(&key) {
            return Err(tr!("Aguarde: o jogo está sendo instalado, atualizado ou movido.").into());
        }
        action()
    }

    /// Holds the recomp as busy (Play, Update and Uninstall wait) for work
    /// outside an install, like moving its folder. Released with `release`.
    pub fn reserve(&self, id: &str) -> Result<String, String> {
        let key = find(id).map(|entry| entry.id).unwrap_or_else(|| id.to_owned());
        let mut busy = self.0.lock().unwrap_or_else(|p| p.into_inner());
        if busy.contains_key(&key) {
            return Err(tr!("Aguarde: o jogo está sendo instalado, atualizado ou movido.").into());
        }
        busy.insert(key.clone(), Arc::new(install::Halt::default()));
        Ok(key)
    }

    pub fn release(&self, key: &str) {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).remove(key);
    }

    pub fn cancel(&self, id: &str) {
        let key = find(id).map(|entry| entry.id).unwrap_or_else(|| id.to_owned());
        if let Some(flag) = self.0.lock().unwrap_or_else(|p| p.into_inner()).get(&key) {
            flag.cancel();
        }
        if let Some(paused) = self.1.lock().unwrap_or_else(|p| p.into_inner()).remove(&key) {
            paused.discard();
        }
    }

    /// A recomp is downloading or installing right now.
    pub fn any_active(&self) -> bool {
        !self.0.lock().unwrap_or_else(|p| p.into_inner()).is_empty()
    }

    /// Stops the download but keeps its bytes for the next install.
    pub fn pause(&self, id: &str) {
        let key = find(id).map(|entry| entry.id).unwrap_or_else(|| id.to_owned());
        if let Some(flag) = self.0.lock().unwrap_or_else(|p| p.into_inner()).get(&key) {
            flag.pause();
        }
    }
}

/// Copies a recomp's saves now. `Ok(None)` when there is nothing new to keep.
/// With a cloud folder set, the copy goes there too.
pub fn backup_saves(folders: &Folders, recomp_id: &str, install_dir: &Path, reason: &str) -> Result<Option<Backup>, String> {
    backup_saves_to(folders, recomp_id, install_dir, reason, true)
}

/// `to_cloud: false` for a safety copy of saves about to be replaced: sending
/// it up would make the old saves the cloud's newest, and bring them back.
fn backup_saves_to(folders: &Folders, recomp_id: &str, install_dir: &Path, reason: &str, to_cloud: bool) -> Result<Option<Backup>, String> {
    let entry = entry_at(recomp_id, install_dir).ok_or(tr!("Esse recomp não está no catálogo."))?;
    let places = saves::places(&entry.saves);
    let backups = folders.backups_dir(recomp_id);
    let cloud = folders.cloud_dir(recomp_id).filter(|_| to_cloud);
    let made = saves::backup(&backups, &places, install_dir, reason, cloud.as_deref())?;
    if cloud.is_some() {
        // Playing and local backups remain available while the sync folder is offline.
        let _ = cloud::retry_game(folders, recomp_id);
    }
    Ok(made)
}

/// Before playing: when another PC left newer saves in the cloud folder, they
/// are put in place. The safety copy of the current saves stays on this PC.
/// Returns what happened, for a message, or `None` when nothing changed.
pub fn pull_from_cloud(folders: &Folders, recomp_id: &str, install_dir: &Path) -> Result<Option<String>, String> {
    let Some(cloud) = folders.cloud_dir(recomp_id) else {
        return Ok(None);
    };
    let Some((cloud_newest, cloud_print)) = saves::newest_with_fingerprint(&cloud) else {
        return Ok(None);
    };
    let entry = entry_at(recomp_id, install_dir).ok_or(tr!("Esse recomp não está no catálogo."))?;
    let places = saves::places(&entry.saves);
    let local_dir = folders.backups_dir(recomp_id);
    let here = saves::current_fingerprint(&places, install_dir);
    if here.as_deref() == Some(cloud_print.as_str()) {
        saves::set_anchor(&local_dir, &cloud, &cloud_print);
        return Ok(None);
    }
    // Decided by what changed since this PC and the cloud last agreed, never by
    // the clocks (a PC a day ahead would otherwise always win).
    let anchor = saves::anchor_of(&local_dir, &cloud);
    let Some(anchor) = anchor else {
        // First sync on this PC (nothing agreed on yet): the newer copy wins, as
        // before; from the next one on, the anchor decides.
        let local_newest = saves::newest_with_fingerprint(&local_dir);
        let local_is_newer = local_newest.as_ref().is_some_and(|(b, _)| (b.created, &b.name) >= (cloud_newest.created, &cloud_newest.name));
        if local_is_newer {
            return Ok(None);
        }
        return bring_down(folders, recomp_id, install_dir, &cloud, &cloud_newest.name, &cloud_print, &places);
    };
    let cloud_changed = anchor != cloud_print;
    let here_changed = here.as_deref().is_some_and(|h| h != anchor);
    if !cloud_changed {
        // Only this PC changed: its saves go up after playing.
        return Ok(None);
    }
    if here_changed {
        // Changed on both sides: nothing is replaced. The other PC's copy is kept
        // here, ready to restore; this PC's saves stay and go up after playing.
        let source = fs::File::open(cloud.join(&cloud_newest.name)).map_err(|e| e.to_string())?;
        let temp = cloud::stage(source, &local_dir).map_err(|e| e.to_string())?;
        cloud::publish(temp, &local_dir.join(&cloud_newest.name)).map_err(|e| e.to_string())?;
        saves::set_anchor(&local_dir, &cloud, &cloud_print);
        return Ok(Some(tr!("Os saves mudaram neste PC e em outro desde a última sincronização. Nada foi trocado: os deste PC continuam, e os do outro PC ficaram em Backups, prontos para restaurar se forem os certos.").into()));
    }
    bring_down(folders, recomp_id, install_dir, &cloud, &cloud_newest.name, &cloud_print, &places)
}

/// Puts the cloud's copy in place, after a safety copy of this PC's saves.
fn bring_down(folders: &Folders, recomp_id: &str, install_dir: &Path, cloud: &Path, name: &str, cloud_print: &str, places: &[String]) -> Result<Option<String>, String> {
    let local_dir = folders.backups_dir(recomp_id);
    let source = fs::File::open(cloud.join(name)).map_err(|e| e.to_string())?;
    let temp = cloud::stage(source, &local_dir).map_err(|e| e.to_string())?;
    let zip = zip::ZipArchive::new(temp.reopen().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    backup_saves_to(folders, recomp_id, install_dir, saves::BEFORE_CLOUD, false)?;
    if saves::restore_open(zip, install_dir, Some(places))? == 0 {
        return Err(tr!("O backup na nuvem não contém saves que este PC possa restaurar.").into());
    }
    cloud::publish(temp, &local_dir.join(name)).map_err(|e| e.to_string())?;
    saves::set_anchor(&local_dir, cloud, cloud_print);
    Ok(Some(tr!("Saves mais novos de outro PC foram trazidos da nuvem; os deste PC ficaram num backup.").into()))
}

pub fn list_backups(folders: &Folders, recomp_id: &str) -> Vec<Backup> {
    saves::list(&folders.backups_dir(recomp_id))
}

/// Puts a backup back, after keeping a copy of the saves as they are now, so
/// restoring the wrong one can be undone.
pub fn restore_backup(folders: &Folders, recomp_id: &str, install_dir: &Path, name: &str) -> Result<u32, String> {
    // Creating the safety copy can rotate the selected backup out of the ten kept on disk.
    let selected = saves::open_backup(&folders.backups_dir(recomp_id), name)?;
    // The saves about to be replaced stay on this PC only: sent to the cloud
    // they would be its newest, and come back on the other PC.
    backup_saves_to(folders, recomp_id, install_dir, tk!("Antes de restaurar"), false)?;
    let restored = saves::restore_open(selected, install_dir, None)?;
    // The restored saves become the newest copy, here and in the cloud, so the
    // next Play doesn't bring the cloud's older-chosen ones back over them.
    let _ = backup_saves(folders, recomp_id, install_dir, tk!("Restaurado"));
    Ok(restored)
}

pub fn library_id(recomp_id: &str) -> String {
    format!("recomp:{recomp_id}")
}

/// Gives an installed recomp the catalog's current art, unless the player picked
/// a cover of their own. Done on every listing, so art added to the catalog later
/// reaches games installed before it.
/// An installed recomp's catalog entry, with what its own `recomplauncher.json` says.
pub fn entry_for(game: &Game) -> Option<CatalogEntry> {
    let Source::Recomp { recomp_id, exe, .. } = &game.source else { return None };
    let entry = find(recomp_id)?;
    Some(catalog::with_manifest(entry, manifest::read(Path::new(&game.install_dir), Some(Path::new(exe)))))
}

/// The same, by the recomp and the folder it's installed in.
pub fn entry_at(recomp_id: &str, install_dir: &Path) -> Option<CatalogEntry> {
    Some(catalog::with_manifest(find(recomp_id)?, manifest::read(install_dir, None)))
}

/// The games a release carries: the developer's list, with the site's art
/// for each; the site's list when the release has none. Fewer than two: none.
pub fn parts_of(game: &Game) -> Vec<catalog::CatalogPart> {
    if game.part_of.is_some() {
        return Vec::new();
    }
    let Some(entry) = entry_for(game) else { return Vec::new() };
    let own = entry.manifest.as_ref().map(|m| m.games.clone()).unwrap_or_default();
    let parts: Vec<catalog::CatalogPart> = if own.is_empty() {
        entry.parts.clone()
    } else {
        own.iter()
            .map(|p| {
                let art = entry.parts.iter().find(|c| c.id == p.id).cloned().unwrap_or_default();
                catalog::CatalogPart { id: p.id.clone(), name: p.name.clone(), ..art }
            })
            .collect()
    };
    if parts.len() < 2 { Vec::new() } else { parts }
}

/// A part's library id: `recomp:cogbox--gow2`.
pub fn part_id(release: &str, part: &str) -> String {
    format!("{release}--{part}")
}

/// One game of a release, as a game of its own: the release's files and
/// setup, its own name, art and play time.
pub fn part_view(release: &Game, part: &catalog::CatalogPart) -> Game {
    let mut game = release.clone();
    let state = release.parts.get(&part.id).cloned().unwrap_or_default();
    game.id = part_id(&release.id, &part.id);
    game.name = part.name.clone();
    game.cover_path = None;
    game.cover_url = part.cover.clone().or(part.icon.clone()).or(release.cover_url.clone());
    game.hero_path = None;
    game.hero_url = part.hero.clone().or(release.hero_url.clone());
    game.logo_path = None;
    game.playtime_secs = state.playtime_secs;
    game.last_played = state.last_played;
    game.last_session_secs = state.last_session_secs;
    game.launch_args = state.launch_args;
    game.part_of = Some(release.id.clone());
    game.part_ids = Vec::new();
    game.parts = Default::default();
    game.favorite = false;
    game.hidden = false;
    game.collections = Vec::new();
    game.status = None;
    game.rating = 0;
    game.notes = String::new();
    game
}

/// The part a library game is, inside its release (`gow2`).
fn part_of(game: &Game) -> Option<&str> {
    let release = game.part_of.as_deref()?;
    game.id.strip_prefix(release)?.strip_prefix("--")
}

/// Every file the player gave a manifest's inputs: the copy under the main input's id.
fn picked_inputs(game: &Game, manifest: &manifest::Manifest) -> std::collections::BTreeMap<String, String> {
    let mut picked = game.inputs.clone();
    if let (Some(main), Some(copy)) = (manifest.main_input(), &game.game_copy) {
        picked.insert(main.id.clone(), copy.clone());
    }
    // Shared keys: the one copy the launcher keeps, when this game's copy needs it.
    let copy = game.game_copy.as_deref();
    for input in manifest.inputs.iter().filter(|i| i.shared_key().is_some()) {
        picked.remove(&input.id);
        if manifest.key_wanted(input, copy) {
            if let Some(kept) = input.shared_key().and_then(manifest::shared_key) {
                picked.insert(input.id.clone(), kept.display().to_string());
            }
        }
    }
    picked
}

pub fn apply_art(game: &mut Game) {
    let Source::Recomp { recomp_id, .. } = &game.source else {
        return;
    };
    // A game of a several-game release has its own art.
    if game.part_of.is_some() {
        return;
    }
    let Some(entry) = find(recomp_id) else {
        return;
    };
    if game.cover_path.is_none() {
        game.cover_url = entry.cover.or(entry.icon);
    }
    game.hero_url = entry.hero;
}

/// What to start a game with: its own options, plus the player's copy of the
/// original game handed over the way its recomp expects.
pub struct LaunchPlan {
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    /// The program the developer's file names, when the one on record is gone.
    pub program: Option<PathBuf>,
}

/// Before every start: the settings files the developer's file keeps up
/// (the path to the player's copy, keys set once or every time).
pub fn prepare_start(game: &Game) -> Result<(), String> {
    let Some(entry) = entry_for(game) else { return Ok(()) };
    let Some(manifest) = &entry.manifest else { return Ok(()) };
    let configs = crate::game_settings::offer_of(Some(&manifest.raw));
    let picked = picked_inputs(game, manifest);
    let copy = manifest.main_input().and_then(|m| picked.get(&m.id)).map(String::as_str);
    let install = Path::new(&game.install_dir);
    let _ = fs::create_dir_all(manifest::data_dir(install));
    configs.before_start(install, copy)
}

/// A setup step the game's release runs once with the player's copy, before
/// the first start: unpacking the disc, building a cache.
#[derive(Debug, Clone)]
pub struct Preparation {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub label: String,
    /// A hard limit, when the developer gives one.
    pub timeout: Option<std::time::Duration>,
    /// Written when it went well: what it was done with.
    pub marker: PathBuf,
    pub done_with: String,
    /// Its scratch folder: emptied before, taken away after.
    pub temp: Option<PathBuf>,
}

const PREPARED: &str = ".cerberus-prepared";

/// The developer's setup ticks for this game, each on or off.
pub fn prepare_options(game: &Game) -> Vec<cerberus_shared::PrepareOption> {
    let Some(prepare) = entry_for(game).and_then(|e| e.manifest).and_then(|m| m.prepare.clone()) else { return Vec::new() };
    prepare
        .options
        .iter()
        .filter(|o| o.here() && !o.id.is_empty() && !o.args.is_empty())
        .map(|o| cerberus_shared::PrepareOption {
            id: o.id.clone(),
            label: o.label.clone(),
            note: o.note.clone(),
            license: o.license.as_ref().filter(|l| l.url.starts_with("https://")).map(|l| (l.name.clone(), l.url.clone())),
            required: o.required,
            on: game.setting_picks.get(&format!("prepare:{}", o.id)).map_or(o.starts_on(), |v| *v == 1),
        })
        .collect()
}

/// The setup step this game still needs, if any. It runs again when the
/// player picks another copy, turns an option on or off, after a new release,
/// or when what it makes is gone.
/// The setup step still has to run before the game can start.
pub fn needs_preparation(game: &Game) -> bool {
    matches!(preparation(game), Ok(Some(_)))
}

pub fn preparation(game: &Game) -> Result<Option<Preparation>, String> {
    let Some(entry) = entry_for(game) else { return Ok(None) };
    let install = Path::new(&game.install_dir);
    let marker = install.join(PREPARED);
    let last = fs::read_to_string(&marker).ok();
    let version = match &game.source {
        Source::Recomp { version, .. } => version.clone(),
        _ => String::new(),
    };
    if let Some(manifest) = &entry.manifest {
        let Some(prepare) = &manifest.prepare else { return Ok(None) };
        let options = prepare_options(game);
        if let Some(needed) = options.iter().find(|o| o.required && !o.on) {
            return Err(tr!("Marque “{}” na configuração do jogo: a preparação não roda sem isso.", needed.label));
        }
        let on: Vec<&str> = options.iter().filter(|o| o.on).map(|o| o.id.as_str()).collect();
        let copy = game.game_copy.clone().filter(|c| Path::new(c).exists());
        let key = format!("{version}\n{}\n{}", copy.clone().unwrap_or_default(), on.join(","));
        let built = manifest::done(prepare, install);
        let unchanged = last.as_deref() == Some(key.as_str());
        // A game brought in with its build already made counts as set up.
        if (unchanged && (prepare.done.is_empty() || built)) || (last.is_none() && built) {
            return Ok(None);
        }
        // Without the copy, only a step that keeps what it needs can run.
        if copy.is_none() && !prepare.keeps_copy {
            return Ok(None);
        }
        // A copy picked before Cerberus read the developer's checks gets them now:
        // the quick ones (name, size), and the full hash only when those don't
        // already vouch for it, so a page visit never hashes a whole disc.
        if let (Some(main), Some(copy)) = (manifest.main_input(), &copy) {
            if !manifest::still_fits(main, Path::new(copy)) {
                manifest::check(main, Path::new(copy))?;
            }
        }
        let program = prepare.program.iter().filter_map(|p| manifest::inside(p)).map(|p| install.join(p)).find(|p| p.is_file());
        let Some(program) = program else { return Ok(None) };
        let mut picked = picked_inputs(game, manifest);
        if copy.is_none() {
            if let Some(main) = manifest.main_input() {
                picked.remove(&main.id);
            }
        }
        let fill = manifest::Fill { install, inputs: &picked };
        let mut args: Vec<String> = prepare.args.iter().filter_map(|a| fill.text(a)).collect();
        for option in prepare.options.iter().filter(|o| on.contains(&o.id.as_str())) {
            args.extend(option.args.iter().filter_map(|a| fill.text(a)));
        }
        let temp = install.join(".recomplauncher-tmp");
        let data = manifest::data_dir(install);
        let mut env: Vec<(String, String)> = prepare.env.iter().filter(|(k, _)| manifest::settable(k)).filter_map(|(k, v)| Some((k.clone(), fill.text(v)?))).collect();
        env.push(("RECOMPLAUNCHER".into(), "1".into()));
        env.push(("RECOMPLAUNCHER_DATA".into(), data.display().to_string()));
        for name in ["TEMP", "TMP", "TMPDIR"] {
            env.push((name.into(), temp.display().to_string()));
        }
        let label = if prepare.label.trim().is_empty() { tr!("Preparando o jogo").to_string() } else { prepare.label.clone() };
        let timeout = (prepare.timeout > 0).then(|| std::time::Duration::from_secs(prepare.timeout));
        return Ok(Some(Preparation { program, args, env, label, timeout, marker, done_with: key, temp: Some(temp) }));
    }
    let Some(copy) = game.game_copy.clone() else { return Ok(None) };
    let Some(setup) = entry.setup.as_deref().and_then(manifest::inside) else { return Ok(None) };
    let made = !entry.makes.is_empty() && entry.makes.iter().filter_map(|m| manifest::inside(m)).all(|m| install.join(m).exists());
    if made || last.as_deref().map(str::trim) == Some(copy.as_str()) {
        return Ok(None);
    }
    let program = install.join(setup);
    if !program.is_file() {
        return Ok(None);
    }
    // `--install <disc>`: unpacks it beside the game with no window, and checks it ran.
    let args = vec!["--install".to_string(), copy.clone()];
    let label = tr!("Preparando o jogo a partir do seu disco").to_string();
    Ok(Some(Preparation { program, args, env: Vec::new(), label, timeout: None, marker, done_with: copy, temp: None }))
}

/// Turns one of the setup step's ticks on or off; the step runs again on the next Play.
pub fn set_prepare_option(library: &LocalLibrary, game_id: &str, option: &str, on: bool) -> Result<Game, String> {
    let game = library.get(game_id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    if !prepare_options(&game).iter().any(|o| o.id == option) {
        return Err(tr!("Essa opção não existe.").into());
    }
    library.update(game_id, |g| {
        g.setting_picks.insert(format!("prepare:{option}"), usize::from(on));
    })
}

/// The buttons the game's developer adds under its settings.
pub fn game_actions(game: &Game) -> Vec<cerberus_shared::GameAction> {
    let Some(manifest) = entry_for(game).and_then(|e| e.manifest) else { return Vec::new() };
    manifest
        .actions
        .iter()
        .map(|a| cerberus_shared::GameAction { id: a.id.clone(), label: a.label.clone(), help: a.help.clone(), clear: a.open.is_empty() })
        .collect()
}

/// The folder an action opens or empties. Only the game's folder and the
/// player's own folders, never a whole one of those.
pub fn action_folder(game: &Game, action: &str) -> Result<(PathBuf, bool), String> {
    let manifest = entry_for(game).and_then(|e| e.manifest).ok_or(tr!("Esse jogo não tem essa ação."))?;
    let action = manifest.actions.iter().find(|a| a.id == action).ok_or(tr!("Esse jogo não tem essa ação."))?;
    let install = Path::new(&game.install_dir);
    let (name, sub, clear) = if action.open.is_empty() {
        let (name, sub) = action.clear.split_once('/').unwrap_or((action.clear.as_str(), ""));
        (name, sub, true)
    } else {
        (action.open.as_str(), "", false)
    };
    let templates = manifest.folders.get(name).cloned().unwrap_or_default();
    let template = templates.iter().find(|t| crate::game_settings::safe_place(t)).ok_or(tr!("Essa pasta do jogo não está num lugar seguro."))?;
    let none = std::collections::BTreeMap::new();
    let base = PathBuf::from(manifest::Fill { install, inputs: &none }.text(template).unwrap_or_default());
    let sub = manifest::inside(sub).unwrap_or_default();
    let folder = base.join(sub);
    // Emptying a whole system folder (Documents, AppData) or the game's own is never what a game means.
    let root = template.split_once('}').and_then(|(token, _)| saves::resolve(&format!("{token}}}"), install));
    if clear && (root.as_ref() == Some(&folder) || folder == install) {
        return Err(tr!("Essa pasta não pode ser esvaziada daqui.").into());
    }
    Ok((folder, clear))
}

/// Empties a folder an action names: what's inside goes, the folder stays.
pub fn clear_folder(folder: &Path) -> Result<(), String> {
    let Ok(entries) = fs::read_dir(folder) else { return Ok(()) };
    for entry in entries.flatten() {
        let path = entry.path();
        let gone = if path.is_dir() { fs::remove_dir_all(&path) } else { fs::remove_file(&path) };
        gone.map_err(|e| tr!("Não foi possível apagar {0}: {1}", path.display(), e))?;
    }
    Ok(())
}

/// Whether picture filters can go on the game, by what its developer says.
/// `None`: the developer says nothing, and the program is looked at.
pub fn look_rule(game: &Game) -> Option<Result<(), String>> {
    let entry = entry_for(game)?;
    let looks = entry.manifest.as_ref()?.looks.clone()?;
    if looks.allowed == Some(false) {
        let reason = looks.reason.filter(|r| !r.trim().is_empty()).unwrap_or_else(|| tr!("O desenvolvedor desligou os filtros para esse jogo.").to_string());
        return Some(Err(reason));
    }
    // The API from the game's own settings file, else the one the developer names.
    let from_file = looks.graphics_key.as_deref().and_then(|key| entry.settings.value(Path::new(&game.install_dir), looks.file.as_deref().unwrap_or("main"), key));
    let api = from_file.or(looks.graphics)?.to_lowercase();
    if api.contains("d3d12") || api.contains("d3d11") || api.contains("dx12") || api.contains("dx11") {
        return Some(Ok(()));
    }
    let name = if api.contains("vulkan") { "Vulkan" } else if api.contains("opengl") { "OpenGL" } else if api.contains("d3d9") { "Direct3D 9" } else { return None };
    Some(Err(tr!("Esse jogo usa {}. Por enquanto os filtros funcionam em jogos com Direct3D 10, 11 ou 12.", name)))
}

/// The line count the developer gives for the CRT look.
pub fn look_lines(game: &Game) -> Option<u32> {
    entry_for(game)?.manifest.as_ref()?.looks.as_ref()?.lines
}

/// The Windows build of this PC (CurrentBuildNumber), read once.
pub fn this_windows_build() -> Option<u32> {
    #[cfg(windows)]
    {
        static BUILD: std::sync::OnceLock<Option<u32>> = std::sync::OnceLock::new();
        *BUILD.get_or_init(|| {
            let key = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion").ok()?;
            key.get_value::<String, _>("CurrentBuildNumber").ok()?.parse().ok()
        })
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// The files besides the copy a recomp's `recomplauncher.json` asks for.
pub fn setup_inputs(game: &Game) -> Vec<cerberus_shared::SetupInput> {
    let Some(manifest) = entry_for(game).and_then(|e| e.manifest) else { return Vec::new() };
    let copy = game.game_copy.as_deref();
    manifest
        .other_inputs()
        // A key the copy's kind doesn't need isn't asked for, nor another game's disc.
        .filter(|input| input.shared_key().is_none() || manifest.key_wanted(input, copy))
        .filter(|input| manifest.belongs(&input.id, part_of(game)))
        .map(|input| cerberus_shared::SetupInput {
            id: input.id.clone(),
            label: input.label.clone(),
            role: input.role.clone(),
            optional: input.optional,
            folder: input.takes_folder() && !input.takes_file(),
            shared: input.shared_key().is_some(),
            chosen: match (input.shared_key(), manifest::placed_at(input, Path::new(&game.install_dir), None)) {
                (Some(kind), _) => manifest::shared_key(kind).map(|p| p.display().to_string()),
                (None, Some(target)) if input.name.is_some() => target.exists().then(|| target.display().to_string()),
                _ => game.inputs.get(&input.id).cloned(),
            },
        })
        .collect()
}

/// Gives one of a manifest's inputs its file (or takes it away with `None`):
/// checked, put in the game's folder when the developer asks, and kept.
pub fn use_input(library: &LocalLibrary, game_id: &str, input_id: &str, picked: Option<&Path>) -> Result<Game, String> {
    let game = library.get(game_id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    let manifest = entry_for(&game).and_then(|e| e.manifest).ok_or(tr!("Esse jogo não pede outros arquivos."))?;
    let input = manifest.input(input_id).ok_or(tr!("Esse jogo não pede esse arquivo."))?;
    // A shared key is kept once, for every game asking for its kind.
    if let Some(kind) = input.shared_key() {
        match picked {
            Some(path) => {
                manifest::keep_shared_key(kind, path)?;
            }
            None => manifest::forget_shared_key(kind),
        }
        return Ok(game);
    }
    let Some(path) = picked else {
        return library.update(game_id, |g| {
            g.inputs.remove(input_id);
        });
    };
    manifest::check(input, path)?;
    manifest::place(input, Path::new(&game.install_dir), path)?;
    let path = path.to_string_lossy().into_owned();
    library.update(game_id, |g| {
        g.inputs.insert(input_id.to_string(), path);
    })
}

/// Why the copy the player picked won't do, when one is picked: the developer's
/// check, or what the game asks for.
pub fn copy_problem(game: &Game) -> Option<String> {
    let copy = game.game_copy.as_deref().filter(|c| Path::new(c).exists())?;
    let entry = entry_for(game)?;
    if let Some(main) = entry.manifest.as_ref().and_then(|m| m.main_input()) {
        if let Err(why) = manifest::check(main, Path::new(copy)) {
            return Some(why);
        }
    }
    let needs = translate::shown_needs(&entry.needs);
    (!needs.trim().is_empty()).then(|| tr!("Ele pede: {}. Escolha outra na página do jogo.", needs.trim().trim_end_matches('.')).to_string())
}

/// What stands between the player and the game. One answer for every screen
/// and for Play itself, so a button never offers what Play would refuse.
/// `found` is the player's copy from the ROM folders, if the scan saw one.
pub fn next_step(game: &Game, found: Option<&String>) -> NextStep {
    let entry = entry_for(game);
    // A copy found by its name that the developer's checks refuse isn't offered.
    let main = entry.as_ref().and_then(|e| e.manifest.as_ref()).and_then(|m| m.main_input().cloned());
    let found = found.filter(|f| main.as_ref().is_none_or(|m| manifest::still_fits(m, Path::new(f.as_str()))));
    let handoff = entry.as_ref().and_then(|e| e.handoff.clone());
    let step = next_step_with(game, handoff.as_ref(), found);
    // The developer's file names every file the game needs: all must be there.
    if let (NextStep::Play, Some(manifest)) = (&step, entry.as_ref().and_then(|e| e.manifest.as_ref())) {
        // A copy picked before the developer's checks were read (another game's disc) is asked for again.
        if let (Some(main), Some(copy)) = (manifest.main_input(), &game.game_copy) {
            if !manifest::still_fits(main, Path::new(copy)) {
                return NextStep::ChooseCopy { found: found.cloned() };
            }
        }
        if !manifest::missing_for(manifest, Path::new(&game.install_dir), &picked_inputs(game, manifest), part_of(game)).is_empty() {
            return NextStep::ChooseCopy { found: found.cloned() };
        }
    }
    // A folder copy the recomp reads named parts from: they must be in its folder.
    if let (NextStep::Play, Some(entry), Some(CopyHandoff::Folder { folder, file_name: None, .. })) = (&step, &entry, &handoff) {
        let dir = Path::new(&game.install_dir).join(folder);
        if !entry.copy_contains.is_empty() && !entry.copy_contains.iter().all(|name| holds(&dir, name)) {
            return NextStep::ChooseCopy { found: found.cloned() };
        }
    }
    step
}

fn next_step_with(game: &Game, handoff: Option<&CopyHandoff>, found: Option<&String>) -> NextStep {
    let exe = match &game.source {
        Source::Recomp { exe, .. } | Source::Local { exe } => Path::new(exe),
        // Store games start through their own launcher.
        _ => return NextStep::Play,
    };
    if !exe.is_file() {
        let what = if game.setup_pending { tr!("O instalador") } else { tr!("O programa do jogo") };
        return NextStep::Locate { reason: tr!("{1} não está mais em {0}.", exe.display(), what) };
    }
    if game.setup_pending {
        return NextStep::RunInstaller;
    }
    let copy_missing = match (handoff, &game.game_copy) {
        // Passed on every start: the file itself must still be there.
        (Some(CopyHandoff::Flag { .. } | CopyHandoff::Env { .. }), copy) => !copy.as_deref().is_some_and(|c| Path::new(c).exists()),
        // Copied into the game's folder under a fixed name: that file must be there
        // (the original may be gone since). Unpacked discs have no single file to check.
        (Some(CopyHandoff::Folder { folder, file_name: Some(name), .. }), _) => {
            !Path::new(&game.install_dir).join(folder).join(name).is_file()
        }
        (Some(CopyHandoff::Folder { .. }), copy) => copy.is_none(),
        // The copy, given to the developer's arguments or setup step; the rest is checked input by input.
        (Some(CopyHandoff::Inputs), copy) => !copy.as_deref().is_some_and(|c| Path::new(c).exists()),
        (Some(CopyHandoff::AsksItself) | None, _) => false,
    };
    if copy_missing {
        return NextStep::ChooseCopy { found: found.cloned() };
    }
    NextStep::Play
}

pub fn launch_plan(game: &Game) -> LaunchPlan {
    let mut args = Vec::new();
    let mut env = Vec::new();
    let mut program = None;
    let entry = entry_for(game);
    // The developer's own start line comes first, with the player's files filled in.
    if let Some(manifest) = entry.as_ref().and_then(|e| e.manifest.as_ref()) {
        let picked = picked_inputs(game, manifest);
        let launch = manifest::launch_part(manifest, &manifest::Fill { install: Path::new(&game.install_dir), inputs: &picked }, part_of(game));
        args.extend(launch.args);
        env.extend(launch.env);
        program = launch.program;
    }
    if let (Some(copy), Some(handoff)) = (&game.game_copy, entry.as_ref().and_then(|e| e.handoff.clone())) {
        match handoff {
            // "--path=" takes the copy in the same argument; "--rom" before it.
            CopyHandoff::Flag { flag } if flag.ends_with('=') => args.push(format!("{flag}{copy}")),
            CopyHandoff::Flag { flag } => args.extend([flag, copy.clone()]),
            CopyHandoff::Env { name } => env.push((name, copy.clone())),
            // Already in the game's folder, asked for by the game itself, or in the developer's arguments.
            CopyHandoff::Folder { .. } | CopyHandoff::AsksItself | CopyHandoff::Inputs => {}
        }
    }
    // The game's own command-line settings, picked in its Ajustes tab.
    if let Some(entry) = &entry {
        args.extend(entry.settings.args(&game.setting_picks));
    }
    args.extend(split_args(&game.launch_args));
    LaunchPlan { args, env, program }
}

/// Splits launch options like a command line: spaces separate, quotes keep
/// spaces in. `-fullscreen "C:\My Games\x.iso"` gives two options.
pub fn split_args(text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut started = false;
    for c in text.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    args.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            c => {
                current.push(c);
                started = true;
            }
        }
    }
    if started {
        args.push(current);
    }
    args
}

/// Sets a recomp up with the player's copy of its game: checked against the
/// version the recomp needs, then handed over the way it expects. A recomp that
/// reads its own folder gets the file (or the extracted folder, or an Xbox disc
/// image's files) put there.
/// A copy the player already chose, put back where the game reads it when it
/// went missing: an update replaces the game's folder, and with it the copy
/// that was placed inside. `None` when nothing needed (or could be) done.
pub fn restore_copy(library: &LocalLibrary, game: &Game) -> Option<Game> {
    let copy = game.game_copy.as_deref().filter(|c| Path::new(c).exists())?;
    let entry = entry_for(game)?;
    // Only the file an update can take away: a copy put in the game's folder
    // under a fixed name. Everything else (a developer's checked input, a
    // second disc not given yet) is left for the player, and never re-copied
    // or re-checked on every start.
    if entry.manifest.as_ref().and_then(|m| m.main_input()).is_some() {
        return None;
    }
    let Some(CopyHandoff::Folder { folder, file_name: Some(name), .. }) = &entry.handoff else { return None };
    if Path::new(&game.install_dir).join(folder).join(name).is_file() {
        return None;
    }
    use_game_copy(library, &game.id, Path::new(copy)).ok()
}

/// The same for every installed recomp, when Cerberus starts.
pub fn restore_copies(library: &LocalLibrary) {
    for game in library.games().unwrap_or_default() {
        if matches!(game.source, Source::Recomp { .. }) {
            let _ = restore_copy(library, &game);
        }
    }
}

pub fn use_game_copy(library: &LocalLibrary, game_id: &str, copy: &Path) -> Result<Game, String> {
    if !copy.exists() {
        return Err(tr!("{} não existe mais.", copy.display()));
    }
    let game = library.get(game_id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    let Source::Recomp { .. } = &game.source else {
        return Err(tr!("Só recomps usam uma cópia do jogo.").into());
    };
    let entry = entry_for(&game);
    // The wrong region or revision of an N64 game opens to a black screen; say so now instead.
    let wanted = entry.as_ref().and_then(|e| e.n64.clone());
    if let (Some(wanted), Some(code)) = (wanted, crate::roms::n64_code_of(copy)) {
        if let Some(problem) = crate::roms::version_problem(&code, &wanted) {
            return Err(problem);
        }
    }
    // The developer's file says what the copy must be, and where it goes.
    if let Some(manifest) = entry.as_ref().and_then(|e| e.manifest.clone()) {
        if let Some(main) = manifest.main_input() {
            manifest::check(main, copy)?;
            manifest::place(main, Path::new(&game.install_dir), copy)?;
            let verified = manifest::verified(main, copy);
            let path = copy.to_string_lossy().into_owned();
            return library.update(game_id, |game| {
                game.game_copy = Some(path);
                game.copy_verified = verified;
            });
        }
    }
    if let Some(entry) = &entry {
        copy_fits(entry, copy)?;
    }
    let accepts_folder = entry.as_ref().is_some_and(|e| e.accepts_folder);
    match entry.and_then(|e| e.handoff) {
        Some(CopyHandoff::Folder { folder, file_name, extract }) => {
            put_in_folder(&game.install_dir, &folder, file_name, extract, accepts_folder, copy)?;
        }
        _ if copy.is_dir() => return Err(tr!("Esse jogo precisa de um arquivo, não de uma pasta.").into()),
        _ => {}
    }
    let path = copy.to_string_lossy().into_owned();
    library.update(game_id, |game| {
        game.game_copy = Some(path);
        game.copy_verified = None;
    })
}

/// Whether a copy the player picked can be this recomp's game: a cartridge of
/// the right console, a folder when only a folder will do, and a folder that
/// holds what the recomp reads from it.
fn copy_fits(entry: &CatalogEntry, copy: &Path) -> Result<(), String> {
    let wants = if entry.copy_contains.is_empty() { String::new() } else { tr!(" (com {})", entry.copy_contains.join(" e ")) };
    if copy.is_dir() {
        let missing: Vec<&String> = entry.copy_contains.iter().filter(|name| !holds(copy, name)).collect();
        if !missing.is_empty() {
            let names: Vec<&str> = missing.iter().map(|s| s.as_str()).collect();
            return Err(tr!("Essa pasta não tem {0}. Escolha a pasta do jogo{1}.", names.join(" nem "), wants));
        }
        return Ok(());
    }
    if let Some(system) = crate::roms::cartridge_of(copy) {
        if !crate::roms::console_fits(system, &entry.origin) {
            return Err(tr!("Esse arquivo é de {}, mas {} é de {}. Escolha a sua cópia de {}.", crate::roms::console_name(system), entry.name, entry.console, entry.game));
        }
    }
    if entry.copy_accepts == "folder" {
        return Err(tr!("{0} precisa da pasta com os arquivos do jogo{1}, não de um arquivo.", entry.name, wants));
    }
    Ok(())
}

/// The folder has an entry of that name, in any letter case.
fn holds(dir: &Path, name: &str) -> bool {
    fs::read_dir(dir).is_ok_and(|entries| entries.flatten().any(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name)))
}

/// A sibling of `dir` with `suffix` added to its name.
fn beside(dir: &Path, suffix: &str) -> PathBuf {
    let mut name = dir.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(suffix);
    dir.with_file_name(name)
}

/// Fills a folder in one step: everything goes into a temporary folder beside it
/// first, and takes the real name only when complete. An interruption (app
/// closed, disk full, a file that fails) leaves no half folder that would count
/// as ready, and a folder chosen before stays as it was.
pub(crate) fn fill_folder(dir: &Path, fill: impl FnOnce(&Path) -> Result<(), String>) -> Result<(), String> {
    let part = beside(dir, ".cerberus-part");
    let _ = fs::remove_dir_all(&part);
    fs::create_dir_all(&part).map_err(|e| e.to_string())?;
    if let Err(e) = fill(&part) {
        let _ = fs::remove_dir_all(&part);
        return Err(e);
    }
    let old = beside(dir, ".cerberus-old");
    let _ = fs::remove_dir_all(&old);
    if dir.exists() {
        if let Err(e) = fs::rename(dir, &old) {
            let _ = fs::remove_dir_all(&part);
            return Err(e.to_string());
        }
    }
    if let Err(e) = fs::rename(&part, dir) {
        if old.exists() {
            let _ = fs::rename(&old, dir);
        }
        let _ = fs::remove_dir_all(&part);
        return Err(e.to_string());
    }
    let _ = fs::remove_dir_all(&old);
    Ok(())
}

fn put_in_folder(install_dir: &str, folder: &str, file_name: Option<String>, extract: bool, accepts_folder: bool, copy: &Path) -> Result<(), String> {
    // Folder and name come from the catalog: they must stay inside the game's
    // folder, so only plain names are taken (no "..", no drive, no root).
    let inside = |p: &str| Path::new(p).components().all(|c| matches!(c, std::path::Component::Normal(_) | std::path::Component::CurDir));
    if !inside(folder) {
        return Err(tr!("O catálogo pede um caminho inesperado; nada foi copiado.").into());
    }
    let dir = Path::new(install_dir).join(folder);
    // A folder of its own is filled in one step; the game's own folder (".") can't be swapped whole.
    let own_folder = Path::new(folder).components().any(|c| matches!(c, std::path::Component::Normal(_)));
    let fill = |work: &dyn Fn(&Path) -> Result<(), String>| -> Result<(), String> {
        if own_folder {
            if let Some(parent) = dir.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fill_folder(&dir, |part| work(part))
        } else {
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            work(&dir)
        }
    };
    if copy.is_dir() {
        if !accepts_folder {
            return Err(tr!("Esse jogo precisa de um arquivo, não de uma pasta.").into());
        }
        return fill(&|into: &Path| copy_tree(copy, into).map_err(|e| tr!("Não foi possível copiar os arquivos do jogo: {}", e)));
    }
    // A disc image is unpacked, whichever of the two formats the recomps ask for it is.
    if extract && crate::xdvdfs::is_xbox_disc(copy) {
        return fill(&|into: &Path| crate::xdvdfs::extract(copy, into).map(|_| ()));
    }
    if extract && crate::iso9660::is_iso9660(copy) {
        return fill(&|into: &Path| crate::iso9660::extract(copy, into).map(|_| ()));
    }
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let name = file_name.unwrap_or_else(|| copy.file_name().unwrap_or_default().to_string_lossy().into_owned());
    let plain_name = Path::new(&name).components().count() == 1 && inside(&name);
    if !plain_name {
        return Err(tr!("O catálogo pede um caminho inesperado; nada foi copiado.").into());
    }
    // A CHD or CSO where the recomp reads an ISO: unpacked into one.
    let wants_iso = Path::new(&name)
        .extension()
        .is_none_or(|e| ["iso", "chd", "cso"].iter().any(|x| e.eq_ignore_ascii_case(x)));
    if wants_iso && crate::disc_image::kind_of(copy) != crate::disc_image::Kind::Plain {
        crate::disc_image::write_iso(copy, &dir.join(Path::new(&name).with_extension("iso")))?;
        return Ok(());
    }
    copy_whole(copy, &dir.join(&name)).map_err(|e| tr!("Não foi possível copiar sua cópia do jogo: {}", e))?;
    Ok(())
}

/// Copies a folder's files and folders under `to`, keeping their layout.
pub(crate) fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
            continue;
        }
        fs::copy(entry.path(), target)?;
    }
    Ok(())
}

/// Every recomp in the catalog, with what this PC has: straight from the
/// catalog, with no question to GitHub, so 1900 entries list at once.
/// `owned` maps a recomp id to the player's copy of its game, found in their ROM folders.
pub fn entries(library: &LocalLibrary, owned: &HashMap<String, String>) -> Result<Vec<RecompEntry>, String> {
    let installed = installed_versions(library)?;
    let games = library.games()?;
    let all = catalog::all();
    Ok(all
        .iter()
        .filter(|entry| !entry.soon)
        .map(|entry| {
            // An installed game shows what its own file says.
            let game = games.iter().find(|g| matches!(&g.source, Source::Recomp { recomp_id, .. } if entry.is(recomp_id)));
            match game.and_then(entry_for) {
                Some(own) => to_entry(&own, &installed, owned),
                None => to_entry(entry, &installed, owned),
            }
        })
        .collect())
}

/// Why a recomp can't be installed here, if it can't.
fn problem_with(entry: &CatalogEntry) -> Option<String> {
    let for_this_pc = entry.systems.is_empty() || entry.systems.iter().any(|s| s == os_key());
    if !for_this_pc {
        return Some(tr!("Não tem versão para este sistema.").into());
    }
    // A file Cerberus can't unpack (a .rar, a .bat): said on the page, not
    // after downloading all of it.
    if let Some(file) = entry.files.get(os_key()) {
        if assets::kind(&file.name).is_none() && !assets::is_program(&file.name) {
            let format = file.name.rsplit('.').next().unwrap_or_default().to_uppercase();
            return Some(tr!("A versão para este PC vem em {} ({}), um formato que o Cerberus ainda não instala. Baixe pela página do projeto.", format, file.name));
        }
    }
    match entry.kind.as_str() {
        "source" => Some(tr!("Esse projeto só publica o código-fonte; precisa ser compilado. Veja a página do projeto.").into()),
        "installer" if !cfg!(windows) => Some(tr!("Esse recomp vem com um instalador de Windows.").into()),
        _ => None,
    }
}

fn to_entry(entry: &CatalogEntry, installed: &[(String, String)], owned: &HashMap<String, String>) -> RecompEntry {
    let version = installed.iter().find(|(id, _)| entry.is(id)).map(|(_, v)| v.clone());
    let problem = problem_with(entry);
    let direct = entry.files.get(os_key());
    let latest = match (&problem, &entry.tag) {
        (None, Some(tag)) => Some(Release {
            version: tag.clone(),
            file: direct.map(|f| f.name.clone()).unwrap_or_default(),
            size: catalog::bytes_of(&entry.size_text),
        }),
        _ => None,
    };
    let host = entry.forge().map(|f| f.name()).unwrap_or_default();
    let system_needs = entry.manifest.as_deref().map(manifest::system_needs).unwrap_or_default();
    let system_problem = entry.manifest.as_deref().and_then(|m| manifest::system_problem(m, this_windows_build()));
    // The developer's list once installed, the site's before.
    let own: Vec<String> = entry.manifest.as_ref().map(|m| m.games.iter().map(|g| g.name.clone()).collect()).unwrap_or_default();
    let parts = if own.len() >= 2 { own } else { entry.parts.iter().map(|p| p.name.clone()).collect() };
    let parts = if parts.len() >= 2 { parts } else { Vec::new() };
    let entry = entry.clone();
    RecompEntry {
        parts,
        system_needs,
        system_problem,
        // Found by its name, and not refused by the developer's checks.
        owned_copy: owned.get(&entry.id).cloned().filter(|f| {
            entry.manifest.as_ref().and_then(|m| m.main_input()).is_none_or(|m| manifest::still_fits(m, Path::new(f)))
        }),
        handoff: entry.handoff.clone(),
        accepts_folder: entry.accepts_folder || entry.copy_accepts == "folder",
        only_folder: entry.copy_accepts == "folder",
        settings_count: entry.settings.len(),
        player_note: entry.player_note.clone(),
        multiplayer: entry.multiplayer.clone(),
        id: entry.id,
        title: entry.title,
        name: entry.name,
        game: entry.game,
        console: entry.console,
        host,
        repo: entry.repo,
        from_source: entry.from_source,
        icon: entry.icon,
        cover: entry.cover,
        hero: entry.hero,
        logo: entry.logo,
        genre: entry.genre,
        original: entry.original,
        first_release: entry.first_release,
        releases: entry.releases,
        players: entry.players,
        about: entry.about,
        adds: entry.adds,
        needs: translate::shown_needs(&entry.needs),
        origin: entry.origin,
        author: entry.author,
        downloads: entry.downloads,
        stars: entry.stars,
        tags: entry.tags.iter().map(|t| translate::shown_tag(t)).collect(),
        features: entry.features,
        setup_steps: entry.setup_steps,
        recommended: entry.recommended,
        why: entry.why,
        released: entry.released,
        added: entry.added,
        latest,
        installed: version,
        problem,
    }
}

fn installed_versions(library: &LocalLibrary) -> Result<Vec<(String, String)>, String> {
    let recomps = library.games()?.into_iter().filter_map(|game| match game.source {
        Source::Recomp { recomp_id, version, .. } => Some((recomp_id, version)),
        _ => None,
    });
    Ok(recomps.collect())
}

/// Installs, or updates, a recomp to its newest release and puts it in the library.
pub async fn install(
    client: &Client,
    folders: &Folders,
    library: &LocalLibrary,
    installing: &Installing,
    sessions: &crate::sessions::Sessions,
    id: &str,
    report: impl Fn(InstallStep) + Send + Sync,
) -> Result<Game, String> {
    let entry = find(id).ok_or(tr!("Esse recomp não está no catálogo."))?;
    let cancel = Arc::new(install::Halt::default());
    {
        let mut busy = installing.0.lock().unwrap_or_else(|p| p.into_inner());
        if busy.contains_key(&entry.id) {
            return Err(tr!("{} já está sendo instalado.", entry.name));
        }
        let installed = installed_game(library, &entry)?;
        if installed.as_ref().is_some_and(|game| game.external) {
            return Err(tr!("{} foi instalado fora do Cerberus, então as atualizações dele ficam por sua conta. Para o Cerberus cuidar delas, remova o vínculo e instale por aqui.", entry.name));
        }
        if installed.is_some_and(|game| sessions.is_running_any(&game.id)) {
            return Err(tr!("Feche o jogo antes de atualizá-lo.").into());
        }
        busy.insert(entry.id.clone(), cancel.clone());
    }
    installing.1.lock().unwrap_or_else(|p| p.into_inner()).remove(&entry.id);
    let result = install_now(client, folders, library, &entry, &cancel, &report).await;
    installing.0.lock().unwrap_or_else(|p| p.into_inner()).remove(&entry.id);
    if result.as_ref().is_err_and(|e| e == install::PAUSED) {
        installing.1.lock().unwrap_or_else(|p| p.into_inner()).insert(entry.id.clone(), cancel);
    }
    result
}

async fn install_now(
    client: &Client,
    folders: &Folders,
    library: &LocalLibrary,
    entry: &CatalogEntry,
    cancel: &install::Halt,
    report: &(impl Fn(InstallStep) + Send + Sync),
) -> Result<Game, String> {
    let (source, version) = choose_download(client, folders, entry).await?;

    // A source's file name decides how it is installed, since the catalog
    // cannot say whether a hoster link is an archive, a game or its installer.
    let mut effective = entry.clone();
    if let FileSource::Direct { name, .. } = &source {
        effective.kind = if assets::looks_like_installer(name) {
            "installer"
        } else if assets::is_program(name) {
            "single-exe"
        } else {
            "portable"
        }.into();
    }

    // An update goes where the game already is, even if new installs now go
    // elsewhere, and keeps a copy of the saves first in case the new version
    // changes or breaks them.
    let installed = installed_game(library, entry)?;
    let target = match &installed {
        Some(game) => PathBuf::from(&game.install_dir),
        None => free_install_dir(folders, &entry.id),
    };
    if let Some(Source::Recomp { recomp_id, .. }) = installed.as_ref().map(|g| &g.source) {
        check_install_folder(folders, recomp_id, &target)?;
        backup_saves(folders, recomp_id, &target, tk!("Antes de atualizar"))?;
    }

    // Progress is sent at most once per percent; a 70 MB file has thousands of chunks.
    let last_percent = AtomicU64::new(u64::MAX);
    let (got, of) = (AtomicU64::new(0), AtomicU64::new(0));
    let on_bytes = |done: u64, total: u64| {
        got.store(done, Ordering::Relaxed);
        of.store(total, Ordering::Relaxed);
        let percent = if total == 0 { 0 } else { done * 100 / total };
        if percent != last_percent.swap(percent, Ordering::Relaxed) {
            report(InstallStep::Downloading { done, total });
        }
    };
    // The checksum is checked as the bytes arrive, so there is no separate step for it.
    // Each recomp in its own folder: two catalogs' "release.zip" never land on one name.
    let download_dir = folders.downloads.join(&entry.id);
    let downloaded = match &source {
        FileSource::Asset(asset) => install::download(client, asset, &download_dir, cancel, on_bytes).await,
        FileSource::Direct { name, url, headers, mega: Some(mega), .. } => {
            hoster::download_decrypted(client, mega, url, headers, &download_dir, name, cancel, on_bytes).await
        }
        FileSource::Direct { name, size, url, identity, headers, mega: None } => {
            let asset = Asset { name: name.clone(), size: *size, url: url.clone(), sha256: None };
            let options = install::DownloadOptions { url: url.clone(), identity: identity.clone(), headers: headers.clone() };
            install::download_with(client, &asset, &options, &download_dir, cancel, on_bytes).await
        }
    };
    let file = match downloaded {
        Err(e) if e == install::PAUSED => {
            report(InstallStep::Paused { done: got.load(Ordering::Relaxed), total: of.load(Ordering::Relaxed) });
            return Err(e);
        }
        other => other?,
    };

    report(InstallStep::Unpacking);
    let unpack_target = target.clone();
    let release = effective.clone();
    let previous = installed.clone();
    let prepared = tauri::async_runtime::spawn_blocking(move || {
        let result = prepare_release(&file, &unpack_target, &release, previous.as_ref());
        let _ = fs::remove_file(&file);
        result
    }).await.map_err(|e| e.to_string())?;
    let (program, transaction) = prepared?;
    fetch_repo_manifest(client, entry, &target).await;
    let result = register(library, entry, installed.as_ref(), &version, &program, &target, effective.kind == "installer");
    let game = match transaction {
        Some(tx) => tx.finish(result)?,
        None => result?,
    };
    report(InstallStep::Finished);
    Ok(game)
}

fn prepare_release(
    file: &Path,
    target: &Path,
    entry: &CatalogEntry,
    installed: Option<&Game>,
) -> Result<(PathBuf, Option<update::Update>), String> {
    let transaction = installed.map(update::Update::begin).transpose()?;
    let (destination, staging) = match &transaction {
        Some(tx) => (tx.candidate(), tx.candidate().with_file_name("extract")),
        None => (target.to_path_buf(), staging_for(target, &entry.id)),
    };
    let prepare = || {
        let program = install::unpack_over(file, &staging, &destination, entry)?;
        let relative = program.strip_prefix(&destination).map_err(|e| e.to_string())?;
        let marker = match installed.map(|g| &g.source) {
            Some(Source::Recomp { recomp_id, .. }) => recomp_id,
            _ => &entry.id,
        };
        let marker_path = destination.join(MARKER);
        // Replace an archive-supplied marker without following a link.
        if let Err(e) = fs::remove_file(&marker_path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(e.to_string());
            }
        }
        fs::write(marker_path, marker).map_err(|e| e.to_string())?;
        match &transaction {
            Some(tx) => tx.publish()?,
            // A first install: what the release shipped, so a later update knows what the player changed.
            None => update::write_release_files(&destination, &update::release_files(&destination)),
        }
        Ok(target.join(relative))
    };
    match prepare() {
        Ok(program) => Ok((program, transaction)),
        Err(error) => match transaction {
            Some(tx) => tx.finish(Err(error)),
            None => Err(error),
        },
    }
}

/// The game's program inside `dir`, picked by its name like after an install.
pub fn find_game_program(dir: &Path, hints: &[&str]) -> Option<PathBuf> {
    install::find_program(dir, hints)
}

/// The file to download: a release picked from the project's server, or a direct
/// link from a source the player added (a plain file, or a hoster page to resolve).
enum FileSource {
    Asset(Asset),
    Direct {
        name: String,
        size: u64,
        url: String,
        identity: String,
        headers: Vec<(String, String)>,
        mega: Option<hoster::Mega>,
    },
}

/// The file to download, and its version. When the catalog has the exact file
/// with its checksum, it comes straight from the release: no question to the
/// server's API (GitHub's hourly limit is easy to reach). Otherwise the newest
/// release is asked for and its file picked by the catalog's pattern.
async fn choose_download(client: &Client, folders: &Folders, entry: &CatalogEntry) -> Result<(FileSource, String), String> {
    if let Some(file) = entry.files.get(os_key()).filter(|f| !f.url.is_empty()) {
        let version = entry.tag.clone().unwrap_or_else(|| "1".into());
        if file.hoster.is_empty() {
            return Ok((FileSource::Direct {
                name: file.name.clone(),
                size: 0,
                url: file.url.clone(),
                identity: file.url.clone(),
                headers: Vec::new(),
                mega: None,
            }, version));
        }
        let resolved = hoster::resolve(client, &file.hoster, &file.url).await?;
        return Ok((FileSource::Direct {
            name: resolved.name,
            size: resolved.size,
            url: resolved.url,
            identity: resolved.identity,
            headers: resolved.headers,
            mega: resolved.mega,
        }, version));
    }
    if let Some(problem) = problem_with(entry) {
        return Err(problem);
    }
    let forge = entry.forge().ok_or(tr!("O endereço do projeto no catálogo não é válido."))?;
    let direct = entry.files.get(os_key()).filter(|f| !f.name.is_empty() && !f.sha256.is_empty());
    if let (Some(tag), Some(file)) = (&entry.tag, direct)
        && let Some(url) = forge.release_file(tag, &file.name)
    {
        // The size comes with the download itself.
        let asset = Asset { name: file.name.clone(), size: 0, url, sha256: Some(file.sha256.to_lowercase()) };
        return Ok((FileSource::Asset(asset), tag.clone()));
    }
    let release = releases::latest(client, &folders.cache, &forge, true).await?;
    // GitLab: the catalog's file among the release's links, which may leave out
    // the file's extension; the catalog's name and checksum stand.
    if let Some(file) = direct
        && let Some(link) = release.assets.iter().find(|a| assets::same_file(&a.name, &file.name))
    {
        let asset = Asset { name: file.name.clone(), sha256: Some(file.sha256.to_lowercase()), ..link.clone() };
        return Ok((FileSource::Asset(asset), release.version));
    }
    let pattern = entry.assets.get(os_key());
    let by_pattern = pattern.and_then(|p| assets::by_pattern(&release.assets, p).cloned().or_else(|| assets::by_bare_pattern(&release.assets, p)));
    let asset = by_pattern
        .or_else(|| assets::pick(&release.assets, Os::current(), is_arm()).cloned())
        .ok_or(tr!("Ainda não tem uma versão para este sistema."))?;
    Ok((FileSource::Asset(asset), release.version))
}

/// The library's game for this recomp, under its current id or an older one.
fn installed_game(library: &LocalLibrary, entry: &CatalogEntry) -> Result<Option<Game>, String> {
    Ok(library.games()?.into_iter().find(|game| match &game.source {
        Source::Recomp { recomp_id, .. } => entry.is(recomp_id),
        _ => false,
    }))
}

/// A project on its own Forgejo or Gitea server may keep `recomplauncher.json`
/// at the top of its repo instead of in the release: fetched once per install.
/// Not being able to fetch it only means playing by the catalog.
async fn fetch_repo_manifest(client: &Client, entry: &CatalogEntry, install: &Path) {
    let has_own = install.join(manifest::FILE).is_file();
    let Some(forge::Forge::Forgejo { server, path }) = entry.forge() else { return };
    if has_own {
        return;
    }
    let url = format!("https://{server}/api/v1/repos/{path}/raw/{}", manifest::FILE);
    let Ok(response) = crate::net::send(client.get(&url)).await else { return };
    if !response.status().is_success() {
        return;
    }
    let Ok(text) = response.text().await else { return };
    if text.len() > 1024 * 1024 || manifest::Manifest::parse(&text).is_none() {
        return;
    }
    let target = install.join(manifest::FROM_REPO);
    if let Some(dir) = target.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let _ = fs::write(target, text);
}

/// Adds the recomp to the library, or updates the entry already there while
/// keeping the name and cover the player may have changed, and the id its save
/// backups are kept under.
fn register(
    library: &LocalLibrary,
    entry: &CatalogEntry,
    installed: Option<&Game>,
    version: &str,
    program: &Path,
    dir: &Path,
    setup_pending: bool,
) -> Result<Game, String> {
    let exe = program.to_string_lossy().into_owned();
    if let Some(existing) = installed {
        let Source::Recomp { recomp_id, generation, .. } = &existing.source else {
            return Err(tr!("O registro do recomp mudou durante a atualização.").into());
        };
        let generation = generation.checked_add(1).ok_or(tr!("Limite de atualizações atingido."))?;
        let source = Source::Recomp { recomp_id: recomp_id.clone(), version: version.to_string(), exe, generation };
        let mut game = library.update(&existing.id, |game| {
            game.source = source;
            game.setup_pending = setup_pending;
        })?;
        apply_art(&mut game);
        return Ok(game);
    }
    let id = library_id(&entry.id);
    let source = Source::Recomp { recomp_id: entry.id.clone(), version: version.to_string(), exe, generation: 0 };
    let mut game = Game {
        id,
        name: entry.name.clone(),
        source,
        install_dir: dir.to_string_lossy().into_owned(),
        cover_path: None,
        cover_url: None,
        hero_path: None,
        hero_url: None,
        playtime_secs: 0,
        last_played: None,
        last_session_secs: None,
        launch_args: String::new(),
        tracking_exe: None,
        game_copy: None,
        copy_verified: None,
        inputs: Default::default(),
        setup_pending,
        next_step: Default::default(),
        setting_picks: Default::default(),
        external: false,
        logo_path: None,
        look: None,
        favorite: false,
        hidden: false,
        collections: Vec::new(),
        status: None,
        rating: 0,
        notes: String::new(),
        updates_paused: false,
        part_of: None,
        part_ids: Vec::new(),
        parts: Default::default(),
    };
    apply_art(&mut game);
    library.add(game.clone())?;
    Ok(game)
}

/// Deletes an installed recomp's folder, wherever it was installed. Only a folder
/// we made is ever deleted: it has our marker file naming this recomp, or (for
/// installs from before the marker existed) it is the recomp's folder inside the
/// default installs folder. Whatever the library file says, nothing else goes.
pub fn delete_files(folders: &Folders, recomp_id: &str, dir: &Path) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    check_install_folder(folders, recomp_id, dir)?;
    // Set aside in one rename first: a file in use (the game, an Explorer
    // preview, the antivirus) stops it here with nothing deleted, instead of
    // half-way through with the folder's marker already gone.
    let aside = removing_name(dir, recomp_id);
    if let Err(e) = throttle::rename_patiently(dir, &aside, 6) {
        return Err(tr!("Não foi possível apagar {0}: {1}. Feche o jogo e o que estiver usando a pasta (o Explorador de Arquivos, por exemplo) e tente de novo; nada foi apagado.", dir.display(), e));
    }
    // What can't go now goes the next time Cerberus starts; earlier leftovers beside it go too.
    let _ = fs::remove_dir_all(&aside);
    if let Some(parent) = dir.parent() {
        for entry in fs::read_dir(parent).into_iter().flatten().flatten() {
            if entry.file_name().to_string_lossy().starts_with(REMOVING) {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
    }
    Ok(())
}

/// The name a folder being removed gets beside its old place: new for each try,
/// so a leftover of an earlier one (still locked) never blocks this one.
fn removing_name(dir: &Path, recomp_id: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    dir.with_file_name(format!("{REMOVING}{recomp_id}-{stamp}"))
}

pub(super) const REMOVING: &str = ".cerberus-removing-";

/// Half-downloaded files nobody came back for in two weeks (a pause before a
/// new version, or Cerberus closed mid-download): gigabytes otherwise kept forever.
pub fn sweep_stale_downloads(folders: &Folders) {
    let two_weeks = std::time::Duration::from_secs(14 * 24 * 3600);
    let mut pending = vec![folders.downloads.clone()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                pending.push(path);
                continue;
            }
            let old = entry.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > two_weeks);
            if old && path.extension().is_some_and(|e| e == "part") {
                let _ = fs::remove_file(&path);
            }
        }
    }
}

/// Copies a file under a temporary name and renames it when complete: a copy
/// cut short (Cerberus closed, the disk full) never sits under the real name,
/// where the game would take it for the whole thing.
pub(crate) fn copy_whole(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut partial = to.as_os_str().to_owned();
    partial.push(".part");
    let partial = PathBuf::from(partial);
    if let Err(e) = fs::copy(from, &partial) {
        let _ = fs::remove_file(&partial);
        return Err(e);
    }
    let _ = fs::remove_file(to);
    fs::rename(&partial, to)
}

/// Folders an uninstall set aside but couldn't finish deleting.
pub fn sweep_removed(folders: &Folders) {
    for root in [folders.installs(), folders.default_installs.clone()] {
        let Ok(entries) = fs::read_dir(&root) else { continue };
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with(REMOVING) {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
    }
}

/// Where a new install goes: the recomp's own folder, unless a folder of that
/// name is already there and isn't ours (installed by hand, another program):
/// then the next free `<id>-2`, `<id>-3`... so nothing of the player's is ever
/// written over, or deleted on uninstall.
fn free_install_dir(folders: &Folders, recomp_id: &str) -> PathBuf {
    let first = folders.install_dir(recomp_id);
    // Only our own marker counts here (not the old "default folder" allowance).
    let ours = |dir: &Path| fs::read_to_string(dir.join(MARKER)).is_ok_and(|id| id.trim() == recomp_id);
    let usable = |dir: &Path| !dir.exists() || ours(dir) || fs::read_dir(dir).is_ok_and(|mut d| d.next().is_none());
    if usable(&first) {
        return first;
    }
    (2..1000).map(|n| first.with_file_name(format!("{recomp_id}-{n}"))).find(|dir| usable(dir)).unwrap_or(first)
}

/// Whether `dir` is a folder Cerberus installed this recomp in (see `check_install_folder`).
pub fn check_install_folder_pub(folders: &Folders, recomp_id: &str, dir: &Path) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    check_install_folder(folders, recomp_id, dir)
}

fn check_install_folder(folders: &Folders, recomp_id: &str, dir: &Path) -> Result<(), String> {
    let marked = fs::read_to_string(dir.join(MARKER)).is_ok_and(|id| id.trim() == recomp_id);
    let safe_id = recomp_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    let legacy = safe_id && dir == folders.default_installs.join(recomp_id);
    if !marked && !legacy {
        return Err(tr!("O Cerberus não reconheceu a pasta de instalação {}.", dir.display()));
    }
    Ok(())
}

fn is_arm() -> bool {
    std::env::consts::ARCH == "aarch64"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_interrupted_folder_fill_is_not_ready_and_the_old_one_stays() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("game").join("dvd");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("old.bin"), b"chosen before").unwrap();
        // Fails half-way: the folder chosen before is untouched, nothing half is left.
        let failed = fill_folder(&dir, |part| {
            fs::write(part.join("a.bin"), b"1").unwrap();
            Err("disk full".into())
        });
        assert!(failed.is_err());
        assert_eq!(fs::read(dir.join("old.bin")).unwrap(), b"chosen before");
        assert!(!dir.join("a.bin").exists());
        assert!(!root.path().join("game").join("dvd.cerberus-part").exists());
        // Succeeds: the new content replaces the old one whole.
        fill_folder(&dir, |part| fs::write(part.join("new.bin"), b"2").map_err(|e| e.to_string())).unwrap();
        assert!(dir.join("new.bin").exists() && !dir.join("old.bin").exists());
        // A first fill that fails leaves no folder at all (so it never counts as there).
        let fresh = root.path().join("game").join("other");
        assert!(fill_folder(&fresh, |_| Err("stopped".into())).is_err());
        assert!(!fresh.exists());
    }

    #[test]
    fn a_new_install_never_lands_in_someone_elses_folder() {
        let root = tempfile::tempdir().unwrap();
        let folders = Folders::new(root.path(), None);
        let theirs = folders.install_dir("zelda");
        fs::create_dir_all(&theirs).unwrap();
        fs::write(theirs.join("texture-pack.zip"), b"mine").unwrap();
        let picked = free_install_dir(&folders, "zelda");
        assert_ne!(picked, theirs);
        assert!(picked.ends_with("zelda-2"));
        // Ours (marked) or empty: used as is.
        fs::write(theirs.join(MARKER), "zelda").unwrap();
        assert_eq!(free_install_dir(&folders, "zelda"), theirs);
    }

    #[test]
    fn uninstalling_refuses_a_folder_cerberus_didnt_make_and_a_leftover_never_blocks() {
        let root = tempfile::tempdir().unwrap();
        let folders = Folders::new(root.path(), None);
        // A folder the player made: refused (the screen then offers to keep it).
        let theirs = root.path().join("MyGame");
        fs::create_dir_all(&theirs).unwrap();
        fs::write(theirs.join("save.dat"), b"mine").unwrap();
        assert!(check_install_folder_pub(&folders, "x", &theirs).is_err());
        assert!(delete_files(&folders, "x", &theirs).is_err());
        assert!(theirs.join("save.dat").exists());
        // Ours, with a leftover from an earlier try beside it: removed, leftover too.
        let ours = root.path().join("ours");
        fs::create_dir_all(&ours).unwrap();
        fs::write(ours.join(MARKER), "x").unwrap();
        let leftover = root.path().join(format!("{REMOVING}x"));
        fs::create_dir_all(&leftover).unwrap();
        delete_files(&folders, "x", &ours).unwrap();
        assert!(!ours.exists() && !leftover.exists());
        // A folder already gone is fine.
        assert!(check_install_folder_pub(&folders, "x", &ours).is_ok());
    }

    fn recomp_at(dir: &Path, copy: Option<&Path>) -> Game {
        let exe = dir.join("Game.exe");
        fs::write(&exe, b"MZ").unwrap();
        let mut game = crate::library::game_from_exe(&exe);
        game.source = Source::Recomp { recomp_id: "x".into(), version: "1".into(), exe: exe.to_string_lossy().into(), generation: Default::default() };
        game.game_copy = copy.map(|c| c.to_string_lossy().into_owned());
        game
    }

    #[test]
    fn an_uninstall_sets_the_folder_aside_before_deleting() {
        let root = tempfile::tempdir().unwrap();
        let folders = Folders::new(root.path(), None);
        let dir = folders.install_dir("x");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(MARKER), "x").unwrap();
        fs::write(dir.join("Game.exe"), b"MZ").unwrap();
        delete_files(&folders, "x", &dir).unwrap();
        assert!(!dir.exists());
        assert!(!removing_name(&dir, "x").exists());

        // A leftover from an interrupted removal is cleared at start.
        let left = removing_name(&dir, "y");
        fs::create_dir_all(&left).unwrap();
        sweep_removed(&folders);
        assert!(!left.exists());
    }

    #[test]
    fn next_step_asks_for_what_each_recomp_lacks() {
        let dir = tempfile::tempdir().unwrap();
        let flag = CopyHandoff::Flag { flag: "--rom".into() };
        let folder = CopyHandoff::Folder { folder: "game".into(), file_name: None, extract: false };
        let rom = dir.path().join("mk64.z64");

        // No copy yet: the one from the ROM folders is offered.
        let found = rom.to_string_lossy().into_owned();
        let game = recomp_at(dir.path(), None);
        assert_eq!(next_step_with(&game, Some(&flag), Some(&found)), NextStep::ChooseCopy { found: Some(found.clone()) });
        assert_eq!(next_step_with(&game, Some(&folder), None), NextStep::ChooseCopy { found: None });
        // A game that asks in its own menu, or has no handoff, just plays.
        assert_eq!(next_step_with(&game, Some(&CopyHandoff::AsksItself), None), NextStep::Play);
        assert_eq!(next_step_with(&game, None, None), NextStep::Play);

        // A copy passed on the command line must still exist; one copied into the folder needn't.
        let mut game = recomp_at(dir.path(), Some(&rom));
        game.install_dir = dir.path().to_string_lossy().into_owned();
        assert_eq!(next_step_with(&game, Some(&flag), None), NextStep::ChooseCopy { found: None });
        assert_eq!(next_step_with(&game, Some(&folder), None), NextStep::Play);
        fs::write(&rom, b"rom").unwrap();
        assert_eq!(next_step_with(&game, Some(&flag), None), NextStep::Play);

        // A fixed file name in the game's folder (SpaghettiKart's mk64.us.z64): its presence decides.
        let named = CopyHandoff::Folder { folder: ".".into(), file_name: Some("mk64.us.z64".into()), extract: false };
        assert_eq!(next_step_with(&game, Some(&named), None), NextStep::ChooseCopy { found: None });
        fs::write(dir.path().join("mk64.us.z64"), b"rom").unwrap();
        assert_eq!(next_step_with(&game, Some(&named), None), NextStep::Play);
        game.game_copy = None;
        assert_eq!(next_step_with(&game, Some(&named), None), NextStep::Play, "set up by hand, outside Cerberus");
    }

    #[test]
    fn a_copy_of_another_console_or_a_file_for_a_folder_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mhgu = CatalogEntry {
            id: "mhgurecomp".into(),
            name: "MHGU Recomp".into(),
            game: "Monster Hunter Generations Ultimate".into(),
            console: "Nintendo Switch".into(),
            origin: "switch".into(),
            copy_accepts: "folder".into(),
            copy_contains: vec!["exefs".into(), "romfs".into()],
            ..Default::default()
        };
        let three_ds = dir.path().join("Monster Hunter Generations (USA).3ds");
        fs::write(&three_ds, b"rom").unwrap();
        assert!(copy_fits(&mhgu, &three_ds).unwrap_err().contains("é de Nintendo 3DS"));
        let nsp = dir.path().join("MHGU.nsp");
        fs::write(&nsp, b"rom").unwrap();
        assert!(copy_fits(&mhgu, &nsp).unwrap_err().contains("precisa da pasta"));
        let dump = dir.path().join("dump");
        fs::create_dir_all(dump.join("ExeFS")).unwrap();
        assert!(copy_fits(&mhgu, &dump).unwrap_err().contains("não tem romfs"));
        fs::create_dir_all(dump.join("romfs")).unwrap();
        assert!(copy_fits(&mhgu, &dump).is_ok());
    }

    #[test]
    fn next_step_runs_the_installer_or_asks_where_the_program_went() {
        let dir = tempfile::tempdir().unwrap();
        let mut game = recomp_at(dir.path(), None);
        game.setup_pending = true;
        assert_eq!(next_step_with(&game, None, None), NextStep::RunInstaller);
        fs::remove_file(dir.path().join("Game.exe")).unwrap();
        assert!(matches!(next_step_with(&game, None, None), NextStep::Locate { reason } if reason.starts_with("O instalador")));
        game.setup_pending = false;
        assert!(matches!(next_step_with(&game, None, None), NextStep::Locate { reason } if reason.starts_with("O programa")));
    }

    #[test]
    fn bundled_catalog_is_valid() {
        let entries = catalog::bundled();
        assert!(!entries.is_empty());
        let mut ids: Vec<_> = entries.iter().map(|e| e.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), entries.len(), "catalog ids must be unique");
    }

    #[test]
    fn catalog_ids_are_safe_folder_names() {
        for entry in catalog::bundled() {
            let safe = entry.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
            assert!(safe, "{} is used as a folder name", entry.id);
        }
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cerberus-delete-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn deletes_a_marked_install_anywhere() {
        let data = temp("marked");
        let folders = Folders::new(&data, None);
        let game = data.join("D-drive/Recomps/zelda64recomp");
        fs::create_dir_all(&game).unwrap();
        fs::write(game.join(MARKER), "zelda64recomp").unwrap();
        delete_files(&folders, "zelda64recomp", &game).unwrap();
        assert!(!game.exists());
    }

    #[test]
    fn refuses_a_folder_without_our_marker() {
        let data = temp("unmarked");
        let folders = Folders::new(&data, None);
        let someone_elses = data.join("Documents");
        fs::create_dir_all(&someone_elses).unwrap();
        assert!(delete_files(&folders, "zelda64recomp", &someone_elses).is_err());
        assert!(someone_elses.exists());
    }

    #[test]
    fn refuses_a_marker_naming_another_recomp() {
        let data = temp("other-marker");
        let folders = Folders::new(&data, None);
        let game = data.join("elsewhere/banjorecomp");
        fs::create_dir_all(&game).unwrap();
        fs::write(game.join(MARKER), "banjorecomp").unwrap();
        assert!(delete_files(&folders, "zelda64recomp", &game).is_err());
    }

    #[test]
    fn still_deletes_installs_made_before_the_marker() {
        let data = temp("legacy");
        let folders = Folders::new(&data, None);
        let game = folders.default_installs.join("mariokart64recomp");
        fs::create_dir_all(&game).unwrap();
        delete_files(&folders, "mariokart64recomp", &game).unwrap();
        assert!(!game.exists());
    }

    /// A PC's view of one recomp: its install folder with a save, and its data folder.
    fn pc(root: &Path, cloud: &Path, save: &str) -> (Folders, PathBuf) {
        fs::create_dir_all(cloud).unwrap();
        let folders = Folders::new(&root.join("data"), None);
        folders.set_cloud(Some(cloud.to_path_buf()));
        let install = root.join("game");
        fs::create_dir_all(install.join("Save")).unwrap();
        fs::write(install.join("Save/file1.sav"), save).unwrap();
        (folders, install)
    }

    #[test]
    fn saves_changed_on_both_pcs_are_a_conflict_and_nothing_is_replaced() {
        let root = temp("cloud-conflict");
        let cloud = root.join("OneDrive");
        let (desktop, desktop_game) = pc(&root.join("desktop"), &cloud, "level 1");
        backup_saves(&desktop, "shipwright", &desktop_game, "Depois de jogar").unwrap();
        let (laptop, laptop_game) = pc(&root.join("laptop"), &cloud, "nothing");
        std::thread::sleep(std::time::Duration::from_millis(1100));
        // Both start from the same saves.
        assert!(pull_from_cloud(&laptop, "shipwright", &laptop_game).unwrap().is_some());
        // Each plays without syncing; the laptop sends first.
        fs::write(desktop_game.join("Save/file1.sav"), "desktop level 7").unwrap();
        fs::write(laptop_game.join("Save/file1.sav"), "laptop level 9").unwrap();
        backup_saves(&laptop, "shipwright", &laptop_game, "Depois de jogar").unwrap();
        // The desktop opens the game: changed here and there, so nothing is replaced...
        let told = pull_from_cloud(&desktop, "shipwright", &desktop_game).unwrap();
        assert!(told.is_some_and(|m| m.contains("Nada foi trocado")));
        assert_eq!(fs::read_to_string(desktop_game.join("Save/file1.sav")).unwrap(), "desktop level 7");
        // ...and the laptop's copy is here, ready to restore.
        let laptops = list_backups(&desktop, "shipwright").into_iter().find(|b| {
            let probe = root.join("probe");
            fs::create_dir_all(probe.join("Save")).unwrap();
            saves::restore(&desktop.backups_dir("shipwright"), &b.name, &probe).is_ok()
                && fs::read_to_string(probe.join("Save/file1.sav")).is_ok_and(|t| t == "laptop level 9")
        });
        assert!(laptops.is_some(), "the other PC's saves can be restored here");
    }

    #[test]
    fn a_safety_copy_outlives_twelve_sessions() {
        let root = tempfile::tempdir().unwrap();
        let folders = Folders::new(&root.path().join("data"), None);
        let install = root.path().join("game");
        fs::create_dir_all(install.join("Save")).unwrap();
        let slot = install.join("Save/file1.sav");
        fs::write(&slot, "before the cloud").unwrap();
        backup_saves_to(&folders, "shipwright", &install, saves::BEFORE_CLOUD, false).unwrap();
        for round in 0..12 {
            fs::write(&slot, format!("session {round}")).unwrap();
            backup_saves(&folders, "shipwright", &install, "Depois de jogar").unwrap();
        }
        let kept = list_backups(&folders, "shipwright");
        assert!(kept.iter().any(|b| b.reason == saves::BEFORE_CLOUD), "the safety copy is still there");
        assert_eq!(kept.iter().filter(|b| b.reason == "Depois de jogar").count(), 10);
    }

    #[test]
    fn can_restore_the_oldest_backup_while_saving_current_progress() {
        let root = tempfile::tempdir().unwrap();
        let folders = Folders::new(&root.path().join("data"), None);
        let install = root.path().join("game");
        fs::create_dir_all(install.join("Save")).unwrap();
        let slot = install.join("Save/file1.sav");
        for round in 0..10 {
            fs::write(&slot, round.to_string()).unwrap();
            backup_saves(&folders, "shipwright", &install, "Manual").unwrap().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(3));
        }
        let backups = list_backups(&folders, "shipwright");
        assert_eq!(backups.len(), 10);
        fs::write(&slot, "latest progress").unwrap();
        restore_backup(&folders, "shipwright", &install, &backups.last().unwrap().name).unwrap();
        assert_eq!(fs::read_to_string(&slot).unwrap(), "0");
        let current = list_backups(&folders, "shipwright");
        assert_eq!(current.len(), 11, "ten regular copies, plus the safety copy kept apart");
        let safety_copy = current.iter().find(|b| b.reason == "Antes de restaurar").unwrap();
        saves::restore(&folders.backups_dir("shipwright"), &safety_copy.name, &install).unwrap();
        assert_eq!(fs::read_to_string(&slot).unwrap(), "latest progress");
    }

    #[test]
    fn offline_uploads_survive_restart_and_stay_with_the_chosen_folder() {
        let root = tempfile::tempdir().unwrap();
        let sync = root.path().join("sync");
        let desktop_dir = root.path().join("desktop");
        let (desktop, game) = pc(&desktop_dir, &sync, "desktop progress");
        fs::remove_dir(&sync).unwrap();
        let made = backup_saves(&desktop, "shipwright", &game, "Depois de jogar").unwrap().unwrap();
        assert_eq!(cloud::pending_count(&desktop), 1);
        assert!(!sync.exists());
        drop(desktop);

        let reopened = Folders::new(&desktop_dir.join("data"), None);
        cloud::retry(&reopened).unwrap();
        let other = root.path().join("other");
        fs::create_dir(&other).unwrap();
        reopened.set_cloud(Some(other.clone()));
        cloud::retry(&reopened).unwrap();
        assert_eq!(fs::read_dir(other).unwrap().count(), 0);

        reopened.set_cloud(Some(sync.clone()));
        assert!(cloud::retry(&reopened).is_err());
        std::thread::sleep(std::time::Duration::from_millis(5));
        let (laptop, laptop_game) = pc(&root.path().join("laptop"), &sync, "newer on laptop");
        let newer = backup_saves(&laptop, "shipwright", &laptop_game, "Depois de jogar").unwrap().unwrap();
        cloud::retry(&reopened).unwrap();
        assert_eq!(cloud::pending_count(&reopened), 0);
        let destination = reopened.cloud_dir("shipwright").unwrap();
        assert_eq!(fs::read(destination.join(&made.name)).unwrap(),
            fs::read(reopened.backups_dir("shipwright").join(&made.name)).unwrap());
        assert_eq!(saves::newest_with_fingerprint(&destination).unwrap().0.name, newer.name);
        // Rechecking with identical saves must not publish a duplicate.
        assert!(backup_saves(&reopened, "shipwright", &game, "Manual").unwrap().is_none());
        assert_eq!(fs::read_dir(&destination).unwrap().count(), 2);
        pull_from_cloud(&reopened, "shipwright", &game).unwrap().unwrap();
        assert_eq!(fs::read_to_string(game.join("Save/file1.sav")).unwrap(), "newer on laptop");
    }

    #[test]
    fn remote_backup_with_another_save_layout_preserves_local_progress() {
        let root = tempfile::tempdir().unwrap();
        let sync = root.path().join("cloud");
        let (pc, install) = pc(&root.path().join("pc"), &sync, "current slot");
        let old = root.path().join("old-version");
        fs::create_dir_all(old.join("EarlierSlots")).unwrap();
        fs::write(old.join("EarlierSlots/slot.sav"), "legacy slot").unwrap();
        let cloud = pc.cloud_dir("shipwright").unwrap();
        saves::backup(&cloud, &["{install}/EarlierSlots".to_string()], &old, "Manual", None).unwrap().unwrap();
        assert!(pull_from_cloud(&pc, "shipwright", &install).is_err());
        assert_eq!(fs::read_to_string(install.join("Save/file1.sav")).unwrap(), "current slot");
        assert!(!install.join("EarlierSlots").exists());
    }

    #[test]
    fn manual_backup_can_send_an_existing_local_copy() {
        let root = tempfile::tempdir().unwrap();
        let sync = root.path().join("cloud");
        let (desktop, game) = pc(&root.path().join("desktop"), &sync, "existing save");
        desktop.set_cloud(None);
        let made = backup_saves(&desktop, "shipwright", &game, "Manual").unwrap().unwrap();
        desktop.set_cloud(Some(sync));
        assert!(backup_saves(&desktop, "shipwright", &game, "Manual").unwrap().is_none());
        assert!(desktop.cloud_dir("shipwright").unwrap().join(made.name).is_file());
        assert_eq!(list_backups(&desktop, "shipwright").len(), 1);
    }

    #[test]
    fn interrupted_cloud_restore_can_retry_after_the_archive_is_repaired() {
        let root = tempfile::tempdir().unwrap();
        let sync = root.path().join("sync");
        let (desktop, game) = pc(&root.path().join("desktop"), &sync, "remote progress");
        let (laptop, local_game) = pc(&root.path().join("laptop"), &sync, "local progress");
        let made = backup_saves(&desktop, "shipwright", &game, "Manual").unwrap().unwrap();
        let remote = desktop.cloud_dir("shipwright").unwrap().join(&made.name);
        let good = fs::read(&remote).unwrap();
        let offset = {
            let mut zip = zip::ZipArchive::new(fs::File::open(&remote).unwrap()).unwrap();
            zip.by_index(0).unwrap().data_start() as usize
        };
        let mut damaged = good.clone();
        damaged[offset] ^= 0xff;
        fs::write(&remote, damaged).unwrap();

        assert!(pull_from_cloud(&laptop, "shipwright", &local_game).is_err());
        assert!(!laptop.backups_dir("shipwright").join(&made.name).exists());
        assert_eq!(fs::read_to_string(local_game.join("Save/file1.sav")).unwrap(), "local progress");
        fs::write(&remote, good).unwrap();
        assert!(pull_from_cloud(&laptop, "shipwright", &local_game).unwrap().is_some());
        assert_eq!(fs::read_to_string(local_game.join("Save/file1.sav")).unwrap(), "remote progress");
        assert_eq!(cloud::pending_count(&laptop), 0);
        // Closing unchanged after a pull must not republish the remote save as newer.
        assert!(backup_saves(&laptop, "shipwright", &local_game, "Depois de jogar").unwrap().is_none());
        assert_eq!(fs::read_dir(desktop.cloud_dir("shipwright").unwrap()).unwrap().count(), 1);
    }

    #[test]
    fn saves_travel_between_pcs_through_the_cloud_folder() {
        let root = temp("cloud");
        let cloud = root.join("OneDrive");
        let (desktop, desktop_game) = pc(&root.join("desktop"), &cloud, "level 1");
        let (laptop, laptop_game) = pc(&root.join("laptop"), &cloud, "old");

        // The desktop plays to level 5; closing the game makes a backup, which goes to the cloud.
        fs::write(desktop_game.join("Save/file1.sav"), "level 5").unwrap();
        backup_saves(&desktop, "shipwright", &desktop_game, "Depois de jogar").unwrap();

        // The laptop opens the game: the newer saves come down, its own are kept as a backup.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let pulled = pull_from_cloud(&laptop, "shipwright", &laptop_game).unwrap();
        assert!(pulled.is_some());
        assert_eq!(fs::read_to_string(laptop_game.join("Save/file1.sav")).unwrap(), "level 5");
        let kept = list_backups(&laptop, "shipwright");
        assert!(kept.iter().any(|b| b.reason == "Antes de trazer da nuvem"));

        // Same saves on both: nothing more to do.
        assert!(pull_from_cloud(&laptop, "shipwright", &laptop_game).unwrap().is_none());
    }

    #[test]
    fn newer_saves_on_this_pc_are_not_overwritten() {
        let root = temp("cloud-newer-here");
        let cloud = root.join("Dropbox");
        let (desktop, desktop_game) = pc(&root.join("desktop"), &cloud, "desktop save");
        backup_saves(&desktop, "shipwright", &desktop_game, "Depois de jogar").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let (laptop, laptop_game) = pc(&root.join("laptop"), &root.join("elsewhere"), "laptop played later");
        backup_saves(&laptop, "shipwright", &laptop_game, "Depois de jogar").unwrap();
        laptop.set_cloud(Some(cloud));
        assert!(pull_from_cloud(&laptop, "shipwright", &laptop_game).unwrap().is_none());
        assert_eq!(fs::read_to_string(laptop_game.join("Save/file1.sav")).unwrap(), "laptop played later");
    }

    #[test]
    fn splits_launch_options_like_a_command_line() {
        assert_eq!(split_args(r#"-fullscreen "C:\My Games\x.iso"  --fps 60"#), vec!["-fullscreen", r"C:\My Games\x.iso", "--fps", "60"]);
        assert!(split_args("   ").is_empty());
        assert_eq!(split_args(r#"--name """#), vec!["--name", ""], "an empty quoted option is still one");
    }

    #[test]
    fn direct_source_download_does_not_need_a_project_or_checksum() {
        let entry = CatalogEntry {
            id: "hydra-game".into(),
            tag: Some("2026-11-25".into()),
            size_text: "10 MB".into(),
            files: std::collections::HashMap::from([("windows".into(), catalog::DirectFile {
                name: "Game.zip".into(),
                sha256: String::new(),
                url: "https://example.org/Game.zip".into(),
                hoster: String::new(),
            })]),
            ..Default::default()
        };
        let temp = tempfile::tempdir().unwrap();
        let folders = Folders::new(temp.path(), None);
        let client = Client::new();
        let (source, version) = tauri::async_runtime::block_on(choose_download(&client, &folders, &entry)).unwrap();
        let FileSource::Direct { name, url, size, identity, .. } = source else { panic!("a source link is not a release") };
        assert_eq!(url, "https://example.org/Game.zip");
        assert_eq!(name, "Game.zip");
        assert_eq!(identity, "https://example.org/Game.zip");
        assert_eq!(size, 0, "the source's rounded display size is not authoritative");
        assert_eq!(version, "2026-11-25");
        let shown = to_entry(&entry, &[], &HashMap::new());
        assert_eq!(shown.latest.map(|r| r.size), Some(catalog::bytes_of(&entry.size_text)));
    }

    #[test]
    fn installs_a_hydra_direct_link_end_to_end() {
        use std::io::Write;
        use std::net::TcpListener;

        // A zip holding a fake game, served over HTTP like a list's download link.
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            zip.start_file("HydraGame.exe", zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(b"MZ-fake-exe").unwrap();
            zip.finish().unwrap();
        }
        let bytes = buf.into_inner();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/game.zip", listener.local_addr().unwrap());
        let body = bytes.clone();
        let server = std::thread::spawn(move || {
            // Read the request first: closing a socket with unread bytes still in
            // its receive buffer makes Windows answer with RST, which discards the reply.
            let (socket, _) = listener.accept().unwrap();
            {
                use std::io::BufRead;
                let mut buf = std::io::BufReader::new(&socket);
                let mut line = String::new();
                while buf.read_line(&mut line).unwrap() > 0 && !line.trim().is_empty() {
                    line.clear();
                }
            }
            let mut socket = &socket;
            write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/zip\r\nContent-Length: {}\r\n\r\n", body.len()).unwrap();
            socket.write_all(&body).unwrap();
        });

        let data = tempfile::tempdir().unwrap();
        let folders = Folders::new(data.path(), None);
        let library = LocalLibrary::load(data.path());
        let entry = CatalogEntry {
            id: "hydra-e2e".into(),
            title: "hydra-e2e".into(),
            name: "Hydra End to End".into(),
            game: "Hydra End to End".into(),
            console: "PC".into(),
            origin: "pc".into(),
            tag: Some("1".into()),
            from_source: true,
            systems: vec!["windows".into()],
            kind: "portable".into(),
            files: std::collections::HashMap::from([("windows".into(), catalog::DirectFile {
                name: "game.zip".into(),
                sha256: String::new(),
                url: url.clone(),
                hoster: String::new(),
            })]),
            ..Default::default()
        };
        let client = Client::builder().no_proxy().build().unwrap();
        let cancel = install::Halt::default();
        let report = |_step: InstallStep| {};
        let game = tauri::async_runtime::block_on(install_now(&client, &folders, &library, &entry, &cancel, &report)).unwrap();
        server.join().unwrap();

        assert_eq!(game.id, "recomp:hydra-e2e");
        let installed_exe = Path::new(&game.install_dir).join("HydraGame.exe");
        assert!(installed_exe.is_file(), "the list's archive unpacks to the game's exe");
        assert!(Path::new(&game.install_dir).join(MARKER).is_file(), "the install folder stays ours");
        assert!(library.games().unwrap().iter().any(|g| g.id == game.id), "registered in the library");
    }

    #[test]
    fn staging_sits_beside_the_install() {
        let target = Path::new(r"D:\Recomps\zelda64recomp");
        assert_eq!(staging_for(target, "zelda64recomp"), Path::new(r"D:\Recomps\.staging-zelda64recomp"));
    }
}

#[cfg(test)]
#[test]
#[ignore = "asks GitHub, GitLab and Forgejo servers about catalog recomps downloaded on this PC"]
fn real_downloads_resolve_on_every_server() {
    let cache = std::path::PathBuf::from(std::env::var("APPDATA").unwrap()).join("app.cerberus.launcher/cache");
    catalog::load_copy(&cache);
    let temp = tempfile::tempdir().unwrap();
    let folders = Folders::new(temp.path(), None);
    let client = Client::builder().user_agent("CerberusLauncher").build().unwrap();
    tauri::async_runtime::block_on(async {
        for id in ["mariokart64recomp-2", "commander-genius", "fe3h-recomp-2", "longshot", "hookshot", "zelda64recomp"] {
            let entry = find(id).unwrap_or_else(|| panic!("{id} not in the catalog"));
            let (source, version) = choose_download(&client, &folders, &entry).await.unwrap_or_else(|e| panic!("{id}: {e}"));
            let FileSource::Asset(asset) = source else { panic!("{id} is not a release") };
            let reply = client.get(&asset.url).header("Range", "bytes=0-0").send().await.unwrap();
            println!("{id} {version} {} -> {} {}", asset.name, reply.status(), reply.url().host_str().unwrap_or_default());
            assert!(reply.status().is_success(), "{id}: {}", reply.status());
        }
    });
}
