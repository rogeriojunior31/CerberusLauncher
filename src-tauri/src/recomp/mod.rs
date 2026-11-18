//! Recomps: native PC versions of console games, installed from each
//! developer's own GitHub releases. The player brings their own copy of the
//! game; every recomp in the catalog asks for it the first time it opens.

mod assets;
pub mod catalog;
mod github;
mod install;
mod saves;
pub mod sources;
mod translate;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use cerberus_shared::{Backup, CopyHandoff, Game, InstallStep, Release, RecompEntry, Source};
use reqwest::Client;

use crate::library::LocalLibrary;
use assets::{Asset, Os};
pub use catalog::{CatalogEntry, find};

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
#[derive(Default)]
pub struct Installing(Mutex<HashMap<String, Arc<AtomicBool>>>);

impl Installing {
    pub fn cancel(&self, id: &str) {
        let busy = self.0.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(flag) = busy.get(id) {
            flag.store(true, Ordering::Relaxed);
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
    let entry = find(recomp_id).ok_or("Esse recomp não está no catálogo.")?;
    let places = saves::places(&entry.saves);
    let made = saves::backup(&folders.backups_dir(recomp_id), &places, install_dir, reason)?;
    let cloud = folders.cloud_dir(recomp_id).filter(|_| to_cloud);
    if let (Some(backup), Some(cloud)) = (&made, cloud) {
        // A cloud folder that can't be written now (offline drive) mustn't stop the local copy.
        let _ = fs::create_dir_all(&cloud);
        let _ = fs::copy(folders.backups_dir(recomp_id).join(&backup.name), cloud.join(&backup.name));
    }
    Ok(made)
}

/// Before playing: when another PC left newer saves in the cloud folder, they
/// are put in place. The saves here are backed up first (and that copy goes to
/// the cloud too), so whichever was right is never lost.
/// Returns what happened, for a message, or `None` when nothing changed.
pub fn pull_from_cloud(folders: &Folders, recomp_id: &str, install_dir: &Path) -> Result<Option<String>, String> {
    let Some(cloud) = folders.cloud_dir(recomp_id) else {
        return Ok(None);
    };
    let Some((cloud_newest, cloud_print)) = saves::newest_with_fingerprint(&cloud) else {
        return Ok(None);
    };
    let entry = find(recomp_id).ok_or("Esse recomp não está no catálogo.")?;
    let places = saves::places(&entry.saves);
    if saves::current_fingerprint(&places, install_dir).as_deref() == Some(cloud_print.as_str()) {
        return Ok(None);
    }
    let local_dir = folders.backups_dir(recomp_id);
    let local_newest = saves::newest_with_fingerprint(&local_dir);
    // This PC played after the cloud copy was made: its saves are the newer ones.
    // A local copy of that very cloud backup counts as "already here".
    let local_is_newer = local_newest.as_ref().is_some_and(|(b, _)| b.created > cloud_newest.created || b.name == cloud_newest.name);
    if local_is_newer {
        return Ok(None);
    }
    backup_saves_to(folders, recomp_id, install_dir, "Antes de trazer da nuvem", false)?;
    fs::create_dir_all(&local_dir).map_err(|e| e.to_string())?;
    fs::copy(cloud.join(&cloud_newest.name), local_dir.join(&cloud_newest.name)).map_err(|e| e.to_string())?;
    saves::restore(&local_dir, &cloud_newest.name, install_dir)?;
    Ok(Some("Saves mais novos de outro PC foram trazidos da nuvem; os deste PC ficaram num backup.".into()))
}

pub fn list_backups(folders: &Folders, recomp_id: &str) -> Vec<Backup> {
    saves::list(&folders.backups_dir(recomp_id))
}

/// Puts a backup back, after keeping a copy of the saves as they are now, so
/// restoring the wrong one can be undone.
pub fn restore_backup(folders: &Folders, recomp_id: &str, install_dir: &Path, name: &str) -> Result<u32, String> {
    backup_saves(folders, recomp_id, install_dir, "Antes de restaurar")?;
    saves::restore(&folders.backups_dir(recomp_id), name, install_dir)
}

pub fn library_id(recomp_id: &str) -> String {
    format!("recomp:{recomp_id}")
}

/// Gives an installed recomp the catalog's current art, unless the player picked
/// a cover of their own. Done on every listing, so art added to the catalog later
/// reaches games installed before it.
pub fn apply_art(game: &mut Game) {
    let Source::Recomp { recomp_id, .. } = &game.source else {
        return;
    };
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
}

pub fn launch_plan(game: &Game) -> LaunchPlan {
    let mut args = Vec::new();
    let mut env = Vec::new();
    let handoff = match &game.source {
        Source::Recomp { recomp_id, .. } => find(recomp_id).and_then(|e| e.handoff),
        _ => None,
    };
    if let (Some(copy), Some(handoff)) = (&game.game_copy, handoff) {
        match handoff {
            CopyHandoff::Flag { flag } => args.extend([flag, copy.clone()]),
            CopyHandoff::Env { name } => env.push((name, copy.clone())),
            // Already in the game's folder, or asked for by the game itself.
            CopyHandoff::Folder { .. } | CopyHandoff::AsksItself => {}
        }
    }
    args.extend(split_args(&game.launch_args));
    LaunchPlan { args, env }
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
pub fn use_game_copy(library: &LocalLibrary, game_id: &str, copy: &Path) -> Result<Game, String> {
    if !copy.exists() {
        return Err(format!("{} não existe mais.", copy.display()));
    }
    let game = library.get(game_id)?.ok_or("Esse jogo não está mais na biblioteca.")?;
    let Source::Recomp { recomp_id, .. } = &game.source else {
        return Err("Só recomps usam uma cópia do jogo.".into());
    };
    let entry = find(recomp_id);
    // The wrong region or revision of an N64 game opens to a black screen; say so now instead.
    let wanted = entry.as_ref().and_then(|e| e.n64.clone());
    if let (Some(wanted), Some(code)) = (wanted, crate::roms::n64_code_of(copy)) {
        if let Some(problem) = crate::roms::version_problem(&code, &wanted) {
            return Err(problem);
        }
    }
    let accepts_folder = entry.as_ref().is_some_and(|e| e.accepts_folder);
    match entry.and_then(|e| e.handoff) {
        Some(CopyHandoff::Folder { folder, file_name, extract }) => {
            put_in_folder(&game.install_dir, &folder, file_name, extract, accepts_folder, copy)?;
        }
        _ if copy.is_dir() => return Err("Esse jogo precisa de um arquivo, não de uma pasta.".into()),
        _ => {}
    }
    let path = copy.to_string_lossy().into_owned();
    library.update(game_id, |game| game.game_copy = Some(path))
}

fn put_in_folder(install_dir: &str, folder: &str, file_name: Option<String>, extract: bool, accepts_folder: bool, copy: &Path) -> Result<(), String> {
    // Folder and name come from the catalog: they must stay inside the game's
    // folder, so only plain names are taken (no "..", no drive, no root).
    let inside = |p: &str| Path::new(p).components().all(|c| matches!(c, std::path::Component::Normal(_) | std::path::Component::CurDir));
    if !inside(folder) {
        return Err("O catálogo pede um caminho inesperado; nada foi copiado.".into());
    }
    let dir = Path::new(install_dir).join(folder);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if copy.is_dir() {
        if !accepts_folder {
            return Err("Esse jogo precisa de um arquivo, não de uma pasta.".into());
        }
        return copy_tree(copy, &dir).map_err(|e| format!("Não foi possível copiar os arquivos do jogo: {e}"));
    }
    // A disc image is unpacked, whichever of the two formats the recomps ask for it is.
    if extract && crate::xdvdfs::is_xbox_disc(copy) {
        crate::xdvdfs::extract(copy, &dir)?;
        return Ok(());
    }
    if extract && crate::iso9660::is_iso9660(copy) {
        crate::iso9660::extract(copy, &dir)?;
        return Ok(());
    }
    let name = file_name.unwrap_or_else(|| copy.file_name().unwrap_or_default().to_string_lossy().into_owned());
    let plain_name = Path::new(&name).components().count() == 1 && inside(&name);
    if !plain_name {
        return Err("O catálogo pede um caminho inesperado; nada foi copiado.".into());
    }
    fs::copy(copy, dir.join(&name)).map_err(|e| format!("Não foi possível copiar sua cópia do jogo: {e}"))?;
    Ok(())
}

/// Copies a folder's files and folders under `to`, keeping their layout.
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
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
    let all = catalog::all();
    Ok(all.iter().filter(|entry| !entry.soon).map(|entry| to_entry(entry, &installed, owned)).collect())
}

/// Why a recomp can't be installed here, if it can't.
fn problem_with(entry: &CatalogEntry) -> Option<String> {
    let for_this_pc = entry.systems.is_empty() || entry.systems.iter().any(|s| s == os_key());
    if !for_this_pc {
        return Some("Não tem versão para este sistema.".into());
    }
    match entry.kind.as_str() {
        "source" => Some("Esse projeto só publica o código-fonte; precisa ser compilado. Veja a página do projeto.".into()),
        "installer" if !cfg!(windows) => Some("Esse recomp vem com um instalador de Windows.".into()),
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
    let entry = entry.clone();
    RecompEntry {
        owned_copy: owned.get(&entry.id).cloned(),
        handoff: entry.handoff.clone(),
        accepts_folder: entry.accepts_folder,
        id: entry.id,
        title: entry.title,
        name: entry.name,
        game: entry.game,
        console: entry.console,
        repo: entry.repo,
        icon: entry.icon,
        cover: entry.cover,
        hero: entry.hero,
        about: entry.about,
        adds: entry.adds,
        needs: entry.needs,
        origin: entry.origin,
        author: entry.author,
        downloads: entry.downloads,
        stars: entry.stars,
        tags: entry.tags,
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
    id: &str,
    report: impl Fn(InstallStep) + Send + Sync,
) -> Result<Game, String> {
    let entry = find(id).ok_or("Esse recomp não está no catálogo.")?;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut busy = installing.0.lock().unwrap_or_else(|p| p.into_inner());
        if busy.contains_key(id) {
            return Err(format!("{} já está sendo instalado.", entry.name));
        }
        busy.insert(id.to_string(), cancel.clone());
    }
    let result = install_now(client, folders, library, &entry, &cancel, &report).await;
    installing.0.lock().unwrap_or_else(|p| p.into_inner()).remove(id);
    result
}

async fn install_now(
    client: &Client,
    folders: &Folders,
    library: &LocalLibrary,
    entry: &CatalogEntry,
    cancel: &AtomicBool,
    report: &(impl Fn(InstallStep) + Send + Sync),
) -> Result<Game, String> {
    let (asset, version) = choose_download(client, folders, entry).await?;

    // An update goes where the game already is, even if new installs now go
    // elsewhere, and keeps a copy of the saves first in case the new version
    // changes or breaks them.
    let installed = installed_game(library, entry)?;
    let target = match &installed {
        Some(game) => PathBuf::from(&game.install_dir),
        None => folders.install_dir(&entry.id),
    };
    if let Some(Source::Recomp { recomp_id, .. }) = installed.as_ref().map(|g| &g.source) {
        backup_saves(folders, recomp_id, &target, "Antes de atualizar")?;
    }

    // Progress is sent at most once per percent; a 70 MB file has thousands of chunks.
    let last_percent = AtomicU64::new(u64::MAX);
    let on_bytes = |done: u64, total: u64| {
        let percent = if total == 0 { 0 } else { done * 100 / total };
        if percent != last_percent.swap(percent, Ordering::Relaxed) {
            report(InstallStep::Downloading { done, total });
        }
    };
    // The checksum is checked as the bytes arrive, so there is no separate step for it.
    let file = install::download(client, &asset, &folders.downloads, cancel, on_bytes).await?;

    report(InstallStep::Unpacking);
    let staging = staging_for(&target, &entry.id);
    let unpack_target = target.clone();
    let release = entry.clone();
    let unpack = tauri::async_runtime::spawn_blocking(move || {
        let result = install::unpack_over(&file, &staging, &unpack_target, &release);
        // The archive is not needed once its files are in place.
        let _ = fs::remove_file(&file);
        result
    });
    let program = unpack.await.map_err(|e| e.to_string())??;
    fs::write(target.join(MARKER), entry.id.as_bytes()).map_err(|e| e.to_string())?;

    // An installer runs on the first Play; until then, it's what Play starts.
    let is_installer = entry.kind == "installer";
    let game = register(library, entry, installed.as_ref(), &version, &program, &target, is_installer)?;
    report(InstallStep::Finished);
    Ok(game)
}

/// The game's program inside `dir`, picked by its name like after an install.
pub fn find_game_program(dir: &Path, hints: &[&str]) -> Option<PathBuf> {
    install::find_program(dir, hints)
}

/// The file to download, and its version. When the catalog has the exact file
/// with GitHub's checksum, it comes straight from the release: no question to
/// GitHub's API, whose hourly limit is easy to reach. Otherwise the newest
/// release is asked for and its file picked by the catalog's pattern.
async fn choose_download(client: &Client, folders: &Folders, entry: &CatalogEntry) -> Result<(Asset, String), String> {
    if let Some(problem) = problem_with(entry) {
        return Err(problem);
    }
    let direct = entry.files.get(os_key()).filter(|f| !f.name.is_empty() && !f.sha256.is_empty());
    if let (Some(tag), Some(file)) = (&entry.tag, direct) {
        let name = file.name.replace(' ', "%20");
        let url = format!("https://github.com/{}/releases/download/{tag}/{name}", entry.repo);
        // The size comes with the download itself.
        let asset = Asset { name: file.name.clone(), size: 0, url, sha256: Some(file.sha256.to_lowercase()) };
        return Ok((asset, tag.clone()));
    }
    let release = github::latest(client, &folders.cache, &entry.repo, true).await?;
    let by_pattern = entry.assets.get(os_key()).and_then(|pattern| assets::by_pattern(&release.assets, pattern));
    let asset = by_pattern
        .or_else(|| assets::pick(&release.assets, Os::current(), is_arm()))
        .ok_or("Ainda não tem uma versão para este sistema.")?;
    Ok((asset.clone(), release.version))
}

/// The library's game for this recomp, under its current id or an older one.
fn installed_game(library: &LocalLibrary, entry: &CatalogEntry) -> Result<Option<Game>, String> {
    Ok(library.games()?.into_iter().find(|game| match &game.source {
        Source::Recomp { recomp_id, .. } => entry.is(recomp_id),
        _ => false,
    }))
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
        let recomp_id = match &existing.source {
            Source::Recomp { recomp_id, .. } => recomp_id.clone(),
            _ => entry.id.clone(),
        };
        let source = Source::Recomp { recomp_id, version: version.to_string(), exe };
        let mut game = library.update(&existing.id, |game| {
            game.source = source;
            game.setup_pending = setup_pending;
        })?;
        apply_art(&mut game);
        return Ok(game);
    }
    let id = library_id(&entry.id);
    let source = Source::Recomp { recomp_id: entry.id.clone(), version: version.to_string(), exe };
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
        launch_args: String::new(),
        game_copy: None,
        setup_pending,
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
    let marked = fs::read_to_string(dir.join(MARKER)).is_ok_and(|id| id.trim() == recomp_id);
    let safe_id = recomp_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    let legacy = safe_id && dir == folders.default_installs.join(recomp_id);
    if !marked && !legacy {
        return Err(format!("{} não parece uma pasta instalada pelo Cerberus; nada foi apagado.", dir.display()));
    }
    match fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Não foi possível apagar {}: {e}. O jogo está aberto?", dir.display())),
    }
}

fn is_arm() -> bool {
    std::env::consts::ARCH == "aarch64"
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let folders = Folders::new(&root.join("data"), None);
        folders.set_cloud(Some(cloud.to_path_buf()));
        let install = root.join("game");
        fs::create_dir_all(install.join("Save")).unwrap();
        fs::write(install.join("Save/file1.sav"), save).unwrap();
        (folders, install)
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
    fn staging_sits_beside_the_install() {
        let target = Path::new(r"D:\Recomps\zelda64recomp");
        assert_eq!(staging_for(target, "zelda64recomp"), Path::new(r"D:\Recomps\.staging-zelda64recomp"));
    }
}
