//! Recomps: native PC versions of console games, installed from each
//! developer's own GitHub releases. The player brings their own copy of the
//! game; every recomp in the catalog asks for it the first time it opens.

pub mod adopt;
mod assets;
pub mod catalog;
pub(crate) mod cloud;
mod forge;
mod releases;
mod install;
pub(crate) mod saves;
pub mod sources;
mod translate;
pub(crate) mod update;
pub mod relocate;

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
            return Err("Aguarde a atualização do jogo terminar.".into());
        }
        action()
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
    let entry = find(recomp_id).ok_or("Esse recomp não está no catálogo.")?;
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
    let entry = find(recomp_id).ok_or("Esse recomp não está no catálogo.")?;
    let places = saves::places(&entry.saves);
    if saves::current_fingerprint(&places, install_dir).as_deref() == Some(cloud_print.as_str()) {
        return Ok(None);
    }
    let local_dir = folders.backups_dir(recomp_id);
    let local_newest = saves::newest_with_fingerprint(&local_dir);
    // This PC played after the cloud copy was made: its saves are the newer ones.
    // A local copy of that very cloud backup counts as "already here".
    let local_is_newer = local_newest.as_ref().is_some_and(|(b, _)| (b.created, &b.name) >= (cloud_newest.created, &cloud_newest.name));
    if local_is_newer {
        return Ok(None);
    }
    let source = fs::File::open(cloud.join(&cloud_newest.name)).map_err(|e| e.to_string())?;
    let temp = cloud::stage(source, &local_dir).map_err(|e| e.to_string())?;
    let zip = zip::ZipArchive::new(temp.reopen().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    backup_saves_to(folders, recomp_id, install_dir, saves::BEFORE_CLOUD, false)?;
    if saves::restore_open(zip, install_dir, Some(&places))? == 0 {
        return Err("O backup na nuvem não contém saves que este PC possa restaurar.".into());
    }
    cloud::publish(temp, &local_dir.join(&cloud_newest.name)).map_err(|e| e.to_string())?;
    Ok(Some("Saves mais novos de outro PC foram trazidos da nuvem; os deste PC ficaram num backup.".into()))
}

pub fn list_backups(folders: &Folders, recomp_id: &str) -> Vec<Backup> {
    saves::list(&folders.backups_dir(recomp_id))
}

/// Puts a backup back, after keeping a copy of the saves as they are now, so
/// restoring the wrong one can be undone.
pub fn restore_backup(folders: &Folders, recomp_id: &str, install_dir: &Path, name: &str) -> Result<u32, String> {
    // Creating the safety copy can rotate the selected backup out of the ten kept on disk.
    let selected = saves::open_backup(&folders.backups_dir(recomp_id), name)?;
    backup_saves(folders, recomp_id, install_dir, "Antes de restaurar")?;
    saves::restore_open(selected, install_dir, None)
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

/// What stands between the player and the game. One answer for every screen
/// and for Play itself, so a button never offers what Play would refuse.
/// `found` is the player's copy from the ROM folders, if the scan saw one.
pub fn next_step(game: &Game, found: Option<&String>) -> NextStep {
    let entry = match &game.source {
        Source::Recomp { recomp_id, .. } => find(recomp_id),
        _ => None,
    };
    let handoff = entry.as_ref().and_then(|e| e.handoff.clone());
    let step = next_step_with(game, handoff.as_ref(), found);
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
        let what = if game.setup_pending { "O instalador" } else { "O programa do jogo" };
        return NextStep::Locate { reason: format!("{what} não está mais em {}.", exe.display()) };
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
    // The game's own command-line settings, picked in its Ajustes tab.
    if let Source::Recomp { recomp_id, .. } = &game.source {
        if let Some(entry) = find(recomp_id) {
            args.extend(entry.settings.args(&game.setting_picks));
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
    if let Some(entry) = &entry {
        copy_fits(entry, copy)?;
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

/// Whether a copy the player picked can be this recomp's game: a cartridge of
/// the right console, a folder when only a folder will do, and a folder that
/// holds what the recomp reads from it.
fn copy_fits(entry: &CatalogEntry, copy: &Path) -> Result<(), String> {
    let wants = if entry.copy_contains.is_empty() { String::new() } else { format!(" (com {})", entry.copy_contains.join(" e ")) };
    if copy.is_dir() {
        let missing: Vec<&String> = entry.copy_contains.iter().filter(|name| !holds(copy, name)).collect();
        if !missing.is_empty() {
            let names: Vec<&str> = missing.iter().map(|s| s.as_str()).collect();
            return Err(format!("Essa pasta não tem {}. Escolha a pasta do jogo{wants}.", names.join(" nem ")));
        }
        return Ok(());
    }
    if let Some(system) = crate::roms::cartridge_of(copy) {
        if !crate::roms::console_fits(system, &entry.origin) {
            return Err(format!(
                "Esse arquivo é de {}, mas {} é de {}. Escolha a sua cópia de {}.",
                crate::roms::console_name(system),
                entry.name,
                entry.console,
                entry.game
            ));
        }
    }
    if entry.copy_accepts == "folder" {
        return Err(format!("{} precisa da pasta com os arquivos do jogo{wants}, não de um arquivo.", entry.name));
    }
    Ok(())
}

/// The folder has an entry of that name, in any letter case.
fn holds(dir: &Path, name: &str) -> bool {
    fs::read_dir(dir).is_ok_and(|entries| entries.flatten().any(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name)))
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
    let host = entry.forge().map(|f| f.name()).unwrap_or_default();
    let entry = entry.clone();
    RecompEntry {
        owned_copy: owned.get(&entry.id).cloned(),
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
    sessions: &crate::sessions::Sessions,
    id: &str,
    report: impl Fn(InstallStep) + Send + Sync,
) -> Result<Game, String> {
    let entry = find(id).ok_or("Esse recomp não está no catálogo.")?;
    let cancel = Arc::new(install::Halt::default());
    {
        let mut busy = installing.0.lock().unwrap_or_else(|p| p.into_inner());
        if busy.contains_key(&entry.id) {
            return Err(format!("{} já está sendo instalado.", entry.name));
        }
        let installed = installed_game(library, &entry)?;
        if installed.as_ref().is_some_and(|game| game.external) {
            return Err(format!(
                "{} foi instalado fora do Cerberus, então as atualizações dele ficam por sua conta. Para o Cerberus cuidar delas, remova o vínculo e instale por aqui.",
                entry.name
            ));
        }
        if installed.is_some_and(|game| sessions.is_running(&game.id)) {
            return Err("Feche o jogo antes de atualizá-lo.".into());
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
        check_install_folder(folders, recomp_id, &target)?;
        backup_saves(folders, recomp_id, &target, "Antes de atualizar")?;
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
    let file = match install::download(client, &asset, &folders.downloads, cancel, on_bytes).await {
        Err(e) if e == install::PAUSED => {
            report(InstallStep::Paused { done: got.load(Ordering::Relaxed), total: of.load(Ordering::Relaxed) });
            return Err(e);
        }
        other => other?,
    };

    report(InstallStep::Unpacking);
    let unpack_target = target.clone();
    let release = entry.clone();
    let previous = installed.clone();
    let prepared = tauri::async_runtime::spawn_blocking(move || {
        let result = prepare_release(&file, &unpack_target, &release, previous.as_ref());
        let _ = fs::remove_file(&file);
        result
    }).await.map_err(|e| e.to_string())?;
    let (program, transaction) = prepared?;
    let result = register(library, entry, installed.as_ref(), &version, &program, &target, entry.kind == "installer");
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
        if let Some(tx) = &transaction {
            tx.publish()?;
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

/// The file to download, and its version. When the catalog has the exact file
/// with its checksum, it comes straight from the release: no question to the
/// server's API (GitHub's hourly limit is easy to reach). Otherwise the newest
/// release is asked for and its file picked by the catalog's pattern.
async fn choose_download(client: &Client, folders: &Folders, entry: &CatalogEntry) -> Result<(Asset, String), String> {
    if let Some(problem) = problem_with(entry) {
        return Err(problem);
    }
    let forge = entry.forge().ok_or("O endereço do projeto no catálogo não é válido.")?;
    let direct = entry.files.get(os_key()).filter(|f| !f.name.is_empty() && !f.sha256.is_empty());
    if let (Some(tag), Some(file)) = (&entry.tag, direct)
        && let Some(url) = forge.release_file(tag, &file.name)
    {
        // The size comes with the download itself.
        let asset = Asset { name: file.name.clone(), size: 0, url, sha256: Some(file.sha256.to_lowercase()) };
        return Ok((asset, tag.clone()));
    }
    let release = releases::latest(client, &folders.cache, &forge, true).await?;
    // GitLab: the catalog's file among the release's links, which may leave out
    // the file's extension; the catalog's name and checksum stand.
    if let Some(file) = direct
        && let Some(link) = release.assets.iter().find(|a| assets::same_file(&a.name, &file.name))
    {
        let asset = Asset { name: file.name.clone(), sha256: Some(file.sha256.to_lowercase()), ..link.clone() };
        return Ok((asset, release.version));
    }
    let pattern = entry.assets.get(os_key());
    let by_pattern = pattern.and_then(|p| assets::by_pattern(&release.assets, p).cloned().or_else(|| assets::by_bare_pattern(&release.assets, p)));
    let asset = by_pattern
        .or_else(|| assets::pick(&release.assets, Os::current(), is_arm()).cloned())
        .ok_or("Ainda não tem uma versão para este sistema.")?;
    Ok((asset, release.version))
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
        let Source::Recomp { recomp_id, generation, .. } = &existing.source else {
            return Err("O registro do recomp mudou durante a atualização.".into());
        };
        let generation = generation.checked_add(1).ok_or("Limite de atualizações atingido.")?;
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
        setup_pending,
        next_step: Default::default(),
        setting_picks: Default::default(),
        external: false,
        logo_path: None,
        look: None,
        favorite: false,
        hidden: false,
        collections: Vec::new(),
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
    match fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Não foi possível apagar {}: {e}. O jogo está aberto?", dir.display())),
    }
}

fn check_install_folder(folders: &Folders, recomp_id: &str, dir: &Path) -> Result<(), String> {
    let marked = fs::read_to_string(dir.join(MARKER)).is_ok_and(|id| id.trim() == recomp_id);
    let safe_id = recomp_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    let legacy = safe_id && dir == folders.default_installs.join(recomp_id);
    if !marked && !legacy {
        return Err(format!("O Cerberus não reconheceu a pasta de instalação {}.", dir.display()));
    }
    Ok(())
}

fn is_arm() -> bool {
    std::env::consts::ARCH == "aarch64"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recomp_at(dir: &Path, copy: Option<&Path>) -> Game {
        let exe = dir.join("Game.exe");
        fs::write(&exe, b"MZ").unwrap();
        let mut game = crate::library::game_from_exe(&exe);
        game.source = Source::Recomp { recomp_id: "x".into(), version: "1".into(), exe: exe.to_string_lossy().into(), generation: Default::default() };
        game.game_copy = copy.map(|c| c.to_string_lossy().into_owned());
        game
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
        assert_eq!(current.len(), 10);
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
            let (asset, version) = choose_download(&client, &folders, &entry).await.unwrap_or_else(|e| panic!("{id}: {e}"));
            let reply = client.get(&asset.url).header("Range", "bytes=0-0").send().await.unwrap();
            println!("{id} {version} {} -> {} {}", asset.name, reply.status(), reply.url().host_str().unwrap_or_default());
            assert!(reply.status().is_success(), "{id}: {}", reply.status());
        }
    });
}
