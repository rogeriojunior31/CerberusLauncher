//! Save backups for recomps.
//!
//! Each catalog entry says where its recomp saves, as templates like
//! `{local}/Zelda64Recompiled/saves` (checked against each project's source).
//! When it says nothing, the player's own files in the game's folder are taken
//! instead, leaving out programs, ROMs and other things that are not saves.
//!
//! A backup is a zip holding `<n>/<path>` for the n-th place, plus
//! `cerberus-backup.json` naming the places as templates, so a copy goes back to
//! the right folder even if the game was reinstalled somewhere else.

use cerberus_shared::tk;
use cerberus_shared::tr;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use cerberus_shared::Backup;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Newest copies kept per game.
const KEEP: usize = 10;
pub(super) const BEFORE_CLOUD: &str = tk!("Antes de trazer da nuvem");
/// Nothing bigger than this is a save: it's a disc image, a video, an asset pack.
const MAX_FILE: u64 = 64 * 1024 * 1024;
/// The most a guess at the game's whole folder may add up to.
const GAME_FOLDER_MAX: u64 = 50 * 1024 * 1024;
/// In a game folder too big to copy whole, the files this small are kept:
/// saves are small, the game's own data mostly isn't.
const SMALL_FILE: u64 = 2 * 1024 * 1024;
const MANIFEST: &str = "cerberus-backup.json";

/// Never saves: game files, ROMs, programs, archives, logs.
const NOT_SAVES: &[&str] = &[
    "exe", "dll", "so", "dylib", "pdb", "msi", "zip", "7z", "rar", "appimage", "z64", "n64", "v64", "iso", "xex",
    "o2r", "otr", "log", "tmp", "dmp", "mp4", "mkv",
];
/// Folders that never hold saves.
const NOT_SAVE_DIRS: &[&str] = &[
    "mods", "shadercache", "shader_cache", "shaders", "cache", "caches", "logs", "log", "crashdumps", "dumps",
    "screenshots", "texture_packs", "textures", "packs", "temp", "assets", "yamls", "meta",
];

#[derive(Serialize, Deserialize, Clone)]
struct Manifest {
    places: Vec<String>,
    created: u64,
    reason: String,
    files: u32,
    bytes: u64,
    /// Same files, same contents: same fingerprint. Used to skip copies that would change nothing.
    fingerprint: String,
}

/// One file to copy: which place it is under, and its path inside that place.
struct Found {
    place: usize,
    relative: PathBuf,
    absolute: PathBuf,
}

/// The templates to use: the catalog's, else the whole game folder.
pub fn places(catalog: &[String]) -> Vec<String> {
    if catalog.is_empty() {
        return vec!["{install}".to_string()];
    }
    catalog.to_vec()
}

/// A template on this PC. `None` when it names a folder this system lacks.
pub fn resolve(template: &str, install: &Path) -> Option<PathBuf> {
    let (base, rest) = match template.split_once('}') {
        Some((key, rest)) => (base_folder(&format!("{key}}}"), install)?, rest),
        None => return None,
    };
    let rest = rest.trim_start_matches(['/', '\\']);
    // A place stays inside its folder: no "..", no drive or root of its own.
    if !Path::new(rest).components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir)) {
        return None;
    }
    if rest.is_empty() {
        // The game's own folders may be whole; the user's, Documents or AppData never
        // (a backup would copy all of it, a restore write anywhere in it).
        return matches!(key_of(template), "{install}" | "{data}").then_some(base);
    }
    Some(base.join(rest))
}

fn key_of(template: &str) -> &str {
    template.split_once('}').map(|(k, _)| &template[..k.len() + 1]).unwrap_or(template)
}

fn base_folder(key: &str, install: &Path) -> Option<PathBuf> {
    let env = |name: &str| std::env::var_os(name).map(PathBuf::from);
    let home = env("USERPROFILE").or_else(|| env("HOME"));
    // Off Windows these games keep their files in ~/.config, by their own code.
    let config = env("XDG_CONFIG_HOME").or_else(|| home.as_ref().map(|h| h.join(".config")));
    match key {
        "{install}" => Some(install.to_path_buf()),
        "{data}" => Some(install.join("recompdata")),
        "{local}" if cfg!(windows) => env("LOCALAPPDATA"),
        "{appdata}" if cfg!(windows) => env("APPDATA"),
        "{local}" | "{appdata}" => config,
        "{config}" if cfg!(windows) => env("APPDATA"),
        "{config}" => config,
        // Where Windows really keeps Documents (often inside OneDrive).
        "{documents}" if cfg!(windows) => crate::standalone::documents(),
        "{documents}" => home.map(|h| h.join("Documents")),
        "{home}" => home,
        _ => None,
    }
}

/// A template whose last part has a `*` (`{install}/controllerPak*.sav`) names
/// files in a folder; returns that folder and the pattern.
fn split_pattern(template: &str) -> Option<(&str, &str)> {
    let (folder, last) = template.rsplit_once('/')?;
    last.contains('*').then_some((folder, last))
}

/// `*` matches any run of characters; letter case doesn't matter.
pub fn matches_pattern(pattern: &str, name: &str) -> bool {
    let pattern = pattern.to_lowercase();
    let name = name.to_lowercase();
    let parts: Vec<&str> = pattern.split('*').collect();
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !name.starts_with(first) || !name[first.len()..].ends_with(last) {
        return false;
    }
    let mut rest = &name[first.len()..name.len() - last.len()];
    for middle in &parts[1..parts.len() - 1] {
        let Some(at) = rest.find(middle) else {
            return false;
        };
        rest = &rest[at + middle.len()..];
    }
    true
}

/// The files a template names, with the folder their paths are relative to.
fn files_of(template: &str, install: &Path) -> Option<(PathBuf, Vec<PathBuf>)> {
    let Some((folder, pattern)) = split_pattern(template) else {
        let dir = resolve(template, install)?;
        let mut here = Vec::new();
        walk(&dir, &dir, 0, &mut here);
        return Some((dir, here));
    };
    let dir = resolve(folder, install)?;
    let entries = fs::read_dir(&dir).ok()?;
    let mut here: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect();
    here.retain(|p| matches_pattern(pattern, &p.file_name().unwrap_or_default().to_string_lossy()));
    here.sort();
    Some((dir, here))
}

/// The files to copy, and whether a guess at the game's folder was too big to
/// tell its saves apart (then nothing of it is in the list).
fn collect_checked(templates: &[String], install: &Path) -> (Vec<Found>, bool) {
    let mut found = Vec::new();
    let mut unsure = false;
    for (place, template) in templates.iter().enumerate() {
        let Some((dir, mut here)) = files_of(template, install) else {
            continue;
        };
        let whole_game_folder = template == "{install}";
        let size = |path: &PathBuf| fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        // A guess at the game folder that is this big holds the game too: its
        // small files are the saves' likely place. Still too much: not a guess to trust.
        if whole_game_folder && here.iter().map(size).sum::<u64>() > GAME_FOLDER_MAX {
            here.retain(|path| size(path) <= SMALL_FILE);
            if here.iter().map(size).sum::<u64>() > GAME_FOLDER_MAX {
                unsure = true;
                continue;
            }
        }
        for absolute in here {
            let Ok(relative) = absolute.strip_prefix(&dir) else {
                continue;
            };
            found.push(Found { place, relative: relative.to_path_buf(), absolute: absolute.clone() });
        }
    }
    (found, unsure)
}

fn collect(templates: &[String], install: &Path) -> Vec<Found> {
    collect_checked(templates, install).0
}

fn walk(root: &Path, dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 8 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
        if path.is_dir() {
            if !NOT_SAVE_DIRS.contains(&name.as_str()) {
                walk(root, &path, depth + 1, out);
            }
            continue;
        }
        let ext = path.extension().unwrap_or_default().to_string_lossy().to_lowercase();
        let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(u64::MAX);
        if NOT_SAVES.contains(&ext.as_str()) || size > MAX_FILE {
            continue;
        }
        out.push(path);
    }
}

/// Copies the saves into `backups`. `Ok(None)` when there is nothing to copy or
/// nothing changed since the newest copy.
pub fn backup(backups: &Path, templates: &[String], install: &Path, reason: &str, cloud: Option<&Path>) -> Result<Option<Backup>, String> {
    let (found, unsure) = collect_checked(templates, install);
    // "Nothing to copy" must mean that: before an uninstall or an update, a
    // folder whose saves can't be told apart stops it instead of losing them.
    if found.is_empty() && unsure {
        return Err(tr!("Não deu para separar os saves deste jogo dos arquivos dele (a pasta é grande demais para copiar inteira). Copie os saves por conta própria antes; a pasta é {}.", install.display()));
    }
    if found.is_empty() {
        return Ok(None);
    }
    let fingerprint = fingerprint(&found).map_err(|e| e.to_string())?;
    let newest = list(backups).into_iter().find(|b| reason == BEFORE_CLOUD || b.reason != BEFORE_CLOUD);
    if let Some(existing) = newest {
        let archive = backups.join(&existing.name);
        if read_manifest(&archive).is_ok_and(|m| m.fingerprint == fingerprint) {
            // A manual backup can also send a matching local copy from before the retry queue existed.
            if reason == "Manual" && let Some(destination) = cloud {
                super::cloud::queue(&archive, destination).map_err(|e| e.to_string())?;
            }
            return Ok(None);
        }
    }

    fs::create_dir_all(backups).map_err(|e| e.to_string())?;
    // Always after the newest one, even with the clock set back (a dead CMOS
    // battery, a dual boot): a new copy is never taken for an old one.
    let created = now().max(list(backups).first().map_or(0, |b| b.created + 1));
    // Two PCs can finish in the same millisecond. Keep both backups in the shared folder.
    let temp = tempfile::Builder::new().prefix(&format!("{}-", now_millis())).suffix(".part")
        .tempfile_in(backups).map_err(|e| e.to_string())?;
    let target = temp.path().with_extension("zip");
    let name = target.file_name().unwrap().to_string_lossy().into_owned();
    let bytes: u64 = found.iter().map(|f| fs::metadata(&f.absolute).map(|m| m.len()).unwrap_or(0)).sum();
    let manifest = Manifest {
        places: templates.to_vec(),
        created,
        reason: reason.to_string(),
        files: found.len() as u32,
        bytes,
        fingerprint,
    };
    write_zip(temp.as_file(), &found, &manifest).map_err(|e| tr!("Não foi possível criar o backup: {}", e))?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    let queued = cloud.map(|destination| super::cloud::queue(&target, destination)).transpose();
    temp.persist_noclobber(&target).map_err(|e| e.to_string())?;
    prune(backups, &name);
    queued.map_err(|e| tr!("Backup local guardado. Falha ao agendar o envio: {}", e))?;
    Ok(Some(Backup { name, created, reason: manifest.reason, files: manifest.files, bytes, in_cloud: None, waiting: false, damaged: false }))
}

fn write_zip(file: &fs::File, found: &[Found], manifest: &Manifest) -> io::Result<()> {
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    for item in found {
        let inner = format!("{}/{}", item.place, item.relative.to_string_lossy().replace('\\', "/"));
        zip.start_file(inner, options)?;
        let mut source = fs::File::open(&item.absolute)?;
        io::copy(&mut source, &mut zip)?;
    }
    zip.start_file(MANIFEST, options)?;
    let json = serde_json::to_vec_pretty(manifest).map_err(io::Error::other)?;
    zip.write_all(&json)?;
    zip.finish()?;
    Ok(())
}

fn fingerprint(found: &[Found]) -> io::Result<String> {
    let mut hash = Sha256::new();
    for item in found {
        hash.update(item.place.to_le_bytes());
        hash.update(item.relative.to_string_lossy().as_bytes());
        let mut file = fs::File::open(&item.absolute)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        hash.update(Sha256::digest(&buffer));
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// Newest first.
pub fn list(backups: &Path) -> Vec<Backup> {
    let Ok(entries) = fs::read_dir(backups) else {
        return Vec::new();
    };
    let zips = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "zip"));
    let mut found: Vec<Backup> = zips.filter_map(|path| describe(&path)).collect();
    found.sort_by(|a, b| (b.created, &b.name).cmp(&(a.created, &a.name)));
    found
}

/// The same, with the copies that can't be read listed too (by the file's own
/// date), marked damaged: for the player's list only, never to restore or sync from.
pub fn list_with_damaged(backups: &Path) -> Vec<Backup> {
    let mut found = list(backups);
    let Ok(entries) = fs::read_dir(backups) else { return found };
    for path in entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "zip")) {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if found.iter().any(|b| b.name == name) {
            continue;
        }
        let meta = fs::metadata(&path).ok();
        let created = meta.as_ref().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
        found.push(Backup { name, created, reason: String::new(), files: 0, bytes: meta.map(|m| m.len()).unwrap_or(0), in_cloud: None, waiting: false, damaged: true });
    }
    found.sort_by(|a, b| (b.created, &b.name).cmp(&(a.created, &a.name)));
    found
}

fn describe(path: &Path) -> Option<Backup> {
    let manifest = read_manifest(path).ok()?;
    let name = path.file_name()?.to_string_lossy().into_owned();
    Some(Backup { name, created: manifest.created, reason: manifest.reason, files: manifest.files, bytes: manifest.bytes, in_cloud: None, waiting: false, damaged: false })
}

/// The fingerprint a backup made now would have; `None` when there are no saves yet.
pub fn current_fingerprint(templates: &[String], install: &Path) -> Option<String> {
    let found = collect(templates, install);
    if found.is_empty() {
        return None;
    }
    fingerprint(&found).ok()
}

/// The newest backup in `dir`, with the fingerprint of the saves it holds.
pub fn newest_with_fingerprint(dir: &Path) -> Option<(Backup, String)> {
    let newest = list(dir).into_iter().find(|b| b.reason != BEFORE_CLOUD)?;
    let manifest = read_manifest(&dir.join(&newest.name)).ok()?;
    Some((newest, manifest.fingerprint))
}

/// What each backup's manifest says, by file, size and date. A backup is
/// written once under a name of its own and never changed, so its manifest is
/// read once: listing a cloud folder of 20 copies on every Play doesn't open
/// (and, with OneDrive's "free up space", download) all 20 again. Kept on this
/// PC only, never in the shared folder.
type ManifestKey = (PathBuf, u64, u64);
static MANIFESTS: std::sync::Mutex<Option<HashMap<ManifestKey, Manifest>>> = std::sync::Mutex::new(None);
static MANIFEST_FILE: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

/// Where the manifests already read are kept between runs.
pub fn keep_manifests_in(file: PathBuf) {
    *MANIFEST_FILE.lock().unwrap_or_else(|p| p.into_inner()) = Some(file);
}

#[cfg(test)]
thread_local! {
    static OPENED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn read_manifest(path: &Path) -> Result<Manifest, String> {
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    let modified = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
    let key = (path.to_path_buf(), meta.len(), modified);
    let file = MANIFEST_FILE.lock().unwrap_or_else(|p| p.into_inner()).clone();
    {
        let mut known = MANIFESTS.lock().unwrap_or_else(|p| p.into_inner());
        let known = known.get_or_insert_with(|| {
            let kept: Vec<(ManifestKey, Manifest)> = file.as_deref().and_then(|f| fs::read(f).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
            kept.into_iter().collect()
        });
        if let Some(manifest) = known.get(&key) {
            return Ok(manifest.clone());
        }
    }
    let manifest = read_manifest_from_zip(path)?;
    let mut known = MANIFESTS.lock().unwrap_or_else(|p| p.into_inner());
    let known = known.get_or_insert_with(HashMap::new);
    // Backups that are gone leave the index too.
    known.retain(|(p, _, _), _| p.exists());
    known.insert(key, manifest.clone());
    if let Some(file) = file {
        let list: Vec<(&ManifestKey, &Manifest)> = known.iter().collect();
        if let Ok(bytes) = serde_json::to_vec(&list) {
            let _ = crate::state_file::write_bytes(&file, &bytes);
        }
    }
    Ok(manifest)
}

fn read_manifest_from_zip(path: &Path) -> Result<Manifest, String> {
    #[cfg(test)]
    OPENED.with(|n| n.set(n.get() + 1));
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    manifest_in(&mut zip)
}

fn manifest_in(zip: &mut zip::ZipArchive<fs::File>) -> Result<Manifest, String> {
    let mut entry = zip.by_name(MANIFEST).map_err(|e| e.to_string())?;
    let mut text = String::new();
    entry.read_to_string(&mut text).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// The copies a game's cloud folder keeps (all PCs together): the newest ones.
/// Without a limit it grew by one per session forever, and each Play read more.
const CLOUD_KEEP: usize = 20;

pub(super) fn prune_cloud(folder: &Path) {
    for old in list(folder).into_iter().skip(CLOUD_KEEP) {
        let _ = fs::remove_file(folder.join(old.name));
    }
}

/// Safety copies (made before saves are replaced: from the cloud, a restore,
/// an uninstall) kept per reason, apart from the regular ones.
const KEEP_SAFETY: usize = 3;

fn is_safety(reason: &str) -> bool {
    reason.starts_with("Antes de")
}

/// The newest `KEEP` regular copies, and the newest `KEEP_SAFETY` of each
/// safety reason; `just_made` always stays.
fn prune(backups: &Path, just_made: &str) {
    let mut regular = 0;
    let mut safety: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for old in list(backups) {
        let within = if is_safety(&old.reason) {
            let n = safety.entry(old.reason.clone()).or_default();
            *n += 1;
            *n <= KEEP_SAFETY
        } else {
            regular += 1;
            regular <= KEEP
        };
        let keep = within || old.name == just_made;
        if !keep {
            let archive = backups.join(old.name);
            if fs::remove_file(&archive).is_ok() {
                let _ = fs::remove_file(archive.with_extension("upload"));
            }
        }
    }
}

/// The saves a backup holds, by their fingerprint.
pub fn fingerprint_in(archive: &Path) -> Option<String> {
    read_manifest(archive).ok().map(|m| m.fingerprint)
}

#[derive(Serialize, Deserialize, Default)]
struct Anchor {
    fingerprint: String,
    /// The cloud folder it was agreed with: another folder starts over.
    #[serde(default)]
    cloud: String,
}

fn folder_key(cloud: &Path) -> String {
    cloud.to_string_lossy().to_lowercase()
}

/// The saves this PC and that cloud folder last agreed on (pulled from it or sent to it).
pub fn anchor_of(backups: &Path, cloud: &Path) -> Option<String> {
    let anchor: Anchor = crate::state_file::load_or_default(&backups.join("sync.json"));
    Some(anchor.fingerprint).filter(|f| !f.is_empty() && anchor.cloud == folder_key(cloud))
}

pub fn set_anchor(backups: &Path, cloud: &Path, fingerprint: &str) {
    let _ = crate::state_file::save(&backups.join("sync.json"), &Anchor { fingerprint: fingerprint.to_string(), cloud: folder_key(cloud) });
}

/// Puts a copy's files back where they belong for the game as installed now.
/// Returns how many files were written.
#[cfg(test)]
pub fn restore(backups: &Path, name: &str, install: &Path) -> Result<u32, String> {
    restore_open(open_backup(backups, name)?, install, None)
}

pub(crate) fn open_backup(backups: &Path, name: &str) -> Result<zip::ZipArchive<fs::File>, String> {
    // Only a plain file name from our own list, never a path.
    let plain = Path::new(name).components().count() == 1 && name.ends_with(".zip");
    if !plain {
        return Err(tr!("Backup inválido.").into());
    }
    let path = backups.join(name);
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    zip::ZipArchive::new(file).map_err(|e| tr!("Esse backup não pode ser lido: {}", e))
}

pub(crate) fn restore_open(mut zip: zip::ZipArchive<fs::File>, install: &Path, allowed_places: Option<&[String]>) -> Result<u32, String> {
    let manifest = manifest_in(&mut zip)?;
    if allowed_places.is_some_and(|places| manifest.places.iter().any(|p| !places.contains(p))) {
        return Err(tr!("As pastas do backup na nuvem diferem das pastas de saves deste jogo.").into());
    }
    let mut staged = Vec::new();
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|e| e.to_string())?;
        if entry.is_dir() || entry.name() == MANIFEST {
            continue;
        }
        let Some(target) = target_for(entry.name(), &manifest.places, install) else {
            continue;
        };
        let parent = target.parent().ok_or(tr!("O destino do save é inválido."))?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let mut temp = tempfile::Builder::new().prefix(".cerberus-").suffix(".tmp").tempfile_in(parent)
            .map_err(|e| tr!("Não foi possível preparar {}: {}", target.display(), e))?;
        io::copy(&mut entry, temp.as_file_mut()).map_err(|e| tr!("Falha ao ler o save {}: {}", entry.name(), e))?;
        temp.as_file().sync_all().map_err(|e| e.to_string())?;
        staged.push((temp.into_temp_path(), target));
    }
    // Finish reading and checking every ZIP entry before replacing the first live save.
    let written = staged.len() as u32;
    swap_all(staged)?;
    Ok(written)
}

/// Puts every staged file in place, or none: each live save is set aside
/// first, and when one can't be replaced (another program holds it) the ones
/// already replaced go back. Saves are never left half from the backup and
/// half from before.
fn swap_all(staged: Vec<(tempfile::TempPath, PathBuf)>) -> Result<(), String> {
    let mut done: Vec<(PathBuf, Option<PathBuf>)> = Vec::new();
    let undo = |done: &[(PathBuf, Option<PathBuf>)]| {
        for (target, old) in done.iter().rev() {
            match old {
                Some(old) => {
                    let _ = fs::rename(old, target);
                }
                None => {
                    let _ = fs::remove_file(target);
                }
            }
        }
    };
    let nothing_changed = |target: &Path, e: &dyn std::fmt::Display| tr!("Não foi possível restaurar {0}: {1}. Nada foi mudado: os saves continuam como estavam. Feche o jogo e o que estiver usando os saves e tente de novo.", target.display(), e);
    for (index, (temp, target)) in staged.into_iter().enumerate() {
        let old = if target.exists() {
            let old = target.with_file_name(format!(".{}.cerberus-before-{index}", target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
            let _ = fs::remove_file(&old);
            if let Err(e) = rename_patiently(&target, &old) {
                undo(&done);
                return Err(nothing_changed(&target, &e));
            }
            Some(old)
        } else {
            None
        };
        // A file held for a moment (the antivirus, a sync app) gets a few tries.
        let mut temp = temp;
        let mut placed = Err(String::new());
        for attempt in 0..3 {
            match temp.persist(&target) {
                Ok(()) => {
                    placed = Ok(());
                    break;
                }
                Err(e) => {
                    placed = Err(e.error.to_string());
                    temp = e.path;
                    if attempt < 2 {
                        std::thread::sleep(std::time::Duration::from_millis(700));
                    }
                }
            }
        }
        if let Err(e) = placed {
            if let Some(old) = &old {
                let _ = fs::rename(old, &target);
            }
            undo(&done);
            return Err(nothing_changed(&target, &e));
        }
        done.push((target, old));
    }
    for (_, old) in done {
        if let Some(old) = old {
            let _ = fs::remove_file(old);
        }
    }
    Ok(())
}

/// A rename tried a few times: a save held for a moment by another program.
fn rename_patiently(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut last = None;
    for attempt in 0..3 {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                if attempt < 2 {
                    std::thread::sleep(std::time::Duration::from_millis(700));
                }
            }
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("rename")))
}

/// `"0/slot1.bin"` -> the file in the first place. Anything climbing out of its place is refused.
fn target_for(inner: &str, places: &[String], install: &Path) -> Option<PathBuf> {
    let (index, relative) = inner.split_once('/')?;
    let template = places.get(index.parse::<usize>().ok()?)?;
    let relative = Path::new(relative);
    let safe = relative.components().all(|c| matches!(c, Component::Normal(_)));
    if !safe {
        return None;
    }
    // A pattern's files were stored relative to the pattern's folder.
    let folder = split_pattern(template).map_or(template.as_str(), |(folder, _)| folder);
    Some(resolve(folder, install)?.join(relative))
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default()
}

fn now_millis() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or_default()
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_damaged_copy_is_listed_for_the_player_but_never_used() {
        let dir = tempfile::tempdir().unwrap();
        let install = dir.path().join("game");
        let backups = dir.path().join("backups");
        std::fs::create_dir_all(install.join("Save")).unwrap();
        std::fs::write(install.join("Save/slot.sav"), "progress").unwrap();
        super::backup(&backups, &["{install}/Save".to_string()], &install, "Manual", None).unwrap().unwrap();
        std::fs::write(backups.join("1-broken.zip"), b"not a zip").unwrap();
        assert_eq!(super::list(&backups).len(), 1, "the sync and restore logic never sees it");
        let shown = super::list_with_damaged(&backups);
        assert_eq!(shown.len(), 2);
        assert!(shown.iter().any(|b| b.damaged && b.name == "1-broken.zip"));
    }

    #[test]
    fn a_save_place_never_climbs_out_nor_takes_a_whole_user_folder() {
        use std::path::Path;
        let install = Path::new(r"C:\Games\x");
        assert!(super::resolve("{home}/../x", install).is_none());
        assert!(super::resolve("{documents}/../../Windows", install).is_none());
        assert!(super::resolve("{home}", install).is_none());
        assert!(super::resolve("{appdata}/", install).is_none());
        assert_eq!(super::resolve("{install}", install).as_deref(), Some(install));
        assert_eq!(super::resolve("{install}/Save", install), Some(install.join("Save")));
        assert!(super::resolve("{install}/C:/Windows", install).is_none());
    }

    #[test]
    fn a_big_game_folder_keeps_its_small_files_or_refuses() {
        let backups = tempfile::tempdir().unwrap();
        let install = tempfile::tempdir().unwrap();
        // The game's data: over the folder cap, file by file over the small size.
        for n in 0..9 {
            fs::write(install.path().join(format!("data{n}.bin")), vec![0u8; 6 * 1024 * 1024]).unwrap();
        }
        fs::write(install.path().join("slot1.sav"), b"save").unwrap();
        let made = backup(backups.path(), &places(&[]), install.path(), "Antes de desinstalar", None).unwrap();
        assert!(made.is_some(), "the save beside a big game is copied");

        // Thousands of small game files too: the saves can't be told apart, so it says so.
        let crowded = tempfile::tempdir().unwrap();
        for n in 0..40 {
            fs::write(crowded.path().join(format!("tex{n}.bin")), vec![0u8; 1536 * 1024]).unwrap();
        }
        assert!(backup(backups.path(), &places(&[]), crowded.path(), "Antes de desinstalar", None).is_err());
    }

    use super::*;

    fn empty_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cerberus-saves-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, body: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }

    fn install_saves() -> Vec<String> {
        vec!["{install}/Save".to_string()]
    }

    #[test]
    fn each_backup_is_opened_once_to_be_listed() {
        let dir = tempfile::tempdir().unwrap();
        let install = dir.path().join("game");
        let backups = dir.path().join("backups");
        for n in 0..5 {
            write(&install.join("Save/slot.sav"), &format!("progress {n}"));
            backup(&backups, &install_saves(), &install, "Manual", None).unwrap();
        }
        let before = OPENED.with(std::cell::Cell::get);
        assert_eq!(list(&backups).len(), 5);
        assert!(newest_with_fingerprint(&backups).is_some());
        let first = OPENED.with(std::cell::Cell::get) - before;
        assert_eq!(list(&backups).len(), 5);
        assert!(newest_with_fingerprint(&backups).is_some());
        assert_eq!(OPENED.with(std::cell::Cell::get) - before, first, "a second listing opens no zip");
    }

    #[test]
    fn backs_up_and_restores_a_save() {
        let dir = empty_dir("roundtrip");
        let install = dir.join("game");
        write(&install.join("Save/file1.sav"), "progress");
        let backups = dir.join("backups");
        let made = backup(&backups, &install_saves(), &install, "Manual", None).unwrap().unwrap();
        write(&install.join("Save/file1.sav"), "lost");
        restore(&backups, &made.name, &install).unwrap();
        assert_eq!(fs::read_to_string(install.join("Save/file1.sav")).unwrap(), "progress");
    }

    #[cfg(windows)]
    #[test]
    fn a_save_held_by_another_program_leaves_every_save_as_it_was() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempfile::tempdir().unwrap();
        let install = dir.path().join("game");
        let backups = dir.path().join("backups");
        let slots = ["a.sav", "b.sav", "c.sav"].map(|n| install.join("Save").join(n));
        for slot in &slots {
            write(slot, "from the backup");
        }
        let made = backup(&backups, &install_saves(), &install, "Manual", None).unwrap().unwrap();
        for slot in &slots {
            write(slot, "played since");
        }
        // The second slot open in another program, sharing nothing.
        let held = fs::OpenOptions::new().read(true).share_mode(0).open(&slots[1]).unwrap();
        let error = restore(&backups, &made.name, &install).unwrap_err();
        drop(held);
        assert!(error.contains("Nada foi mudado"), "{error}");
        for slot in &slots {
            assert_eq!(fs::read_to_string(slot).unwrap(), "played since", "{}", slot.display());
        }
        let left: Vec<_> = fs::read_dir(install.join("Save")).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(left.len(), 3, "nothing set aside is left behind: {left:?}");
        // Closed, it goes through.
        restore(&backups, &made.name, &install).unwrap();
        assert_eq!(fs::read_to_string(&slots[1]).unwrap(), "from the backup");
    }

    #[test]
    fn damaged_second_slot_leaves_both_live_saves_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let install = dir.path().join("game");
        let backups = dir.path().join("backups");
        let slot1 = install.join("Save/a.sav");
        let slot2 = install.join("Save/b.sav");
        write(&slot1, "first slot");
        write(&slot2, "second slot");
        let made = backup(&backups, &install_saves(), &install, "Manual", None).unwrap().unwrap();

        let archive = backups.join(&made.name);
        let offset = {
            let mut zip = open_backup(&backups, &made.name).unwrap();
            zip.by_name("0/b.sav").unwrap().data_start()
        };
        let mut bytes = fs::read(&archive).unwrap();
        bytes[offset as usize] ^= 0xff;
        fs::write(archive, bytes).unwrap();
        write(&slot1, "current A");
        write(&slot2, "current B");
        assert!(restore(&backups, &made.name, &install).is_err());
        assert_eq!(fs::read_to_string(slot1).unwrap(), "current A");
        assert_eq!(fs::read_to_string(slot2).unwrap(), "current B");
        assert_eq!(fs::read_dir(install.join("Save")).unwrap().count(), 2);
    }

    #[test]
    fn skips_a_copy_when_nothing_changed() {
        let dir = empty_dir("unchanged");
        let install = dir.join("game");
        write(&install.join("Save/file1.sav"), "progress");
        let backups = dir.join("backups");
        assert!(backup(&backups, &install_saves(), &install, "Manual", None).unwrap().is_some());
        assert!(backup(&backups, &install_saves(), &install, "Manual", None).unwrap().is_none());
    }

    #[test]
    fn has_nothing_to_copy_before_the_first_save() {
        let dir = empty_dir("nothing");
        let made = backup(&dir.join("backups"), &install_saves(), &dir.join("game"), "Manual", None).unwrap();
        assert!(made.is_none());
    }

    #[test]
    fn guessing_the_game_folder_leaves_out_game_files() {
        let dir = empty_dir("guess");
        let install = dir.join("game");
        write(&install.join("Spaghettify.exe"), "program");
        write(&install.join("mk64.o2r"), "assets");
        write(&install.join("logs/run.log"), "log");
        write(&install.join("default.sav"), "save");
        let found = collect(&places(&[]), &install);
        let names: Vec<_> = found.iter().map(|f| f.relative.to_string_lossy().into_owned()).collect();
        assert_eq!(names, vec!["default.sav"]);
    }

    #[test]
    fn keeps_only_the_newest_ten() {
        let dir = empty_dir("prune");
        let install = dir.join("game");
        let backups = dir.join("backups");
        for round in 0..12 {
            write(&install.join("Save/file1.sav"), &format!("round {round}"));
            backup(&backups, &install_saves(), &install, "Manual", None).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(list(&backups).len(), KEEP);
    }

    #[test]
    fn refuses_entries_that_climb_out_of_their_place() {
        let places = install_saves();
        assert!(target_for("0/../../evil.dll", &places, Path::new(r"C:\game")).is_none());
        assert!(target_for("7/file.sav", &places, Path::new(r"C:\game")).is_none());
    }

    #[test]
    fn patterns_take_only_matching_files() {
        let dir = empty_dir("pattern");
        let install = dir.join("game");
        write(&install.join("controllerPak_header.sav"), "pak");
        write(&install.join("controllerPak_file_1.sav"), "ghost");
        write(&install.join("recompcontrollerdb.txt"), "not a save");
        let found = collect(&["{install}/controllerPak*.sav".to_string()], &install);
        let names: Vec<_> = found.iter().map(|f| f.relative.to_string_lossy().into_owned()).collect();
        assert_eq!(names, vec!["controllerPak_file_1.sav", "controllerPak_header.sav"]);
    }

    #[test]
    fn pattern_files_go_back_to_their_folder() {
        let dir = empty_dir("pattern-restore");
        let install = dir.join("game");
        write(&install.join("controllerPak_header.sav"), "pak");
        let backups = dir.join("backups");
        let templates = vec!["{install}/controllerPak*.sav".to_string()];
        let made = backup(&backups, &templates, &install, "Manual", None).unwrap().unwrap();
        fs::remove_file(install.join("controllerPak_header.sav")).unwrap();
        restore(&backups, &made.name, &install).unwrap();
        assert_eq!(fs::read_to_string(install.join("controllerPak_header.sav")).unwrap(), "pak");
    }

    #[test]
    fn wildcard_matching() {
        assert!(matches_pattern("controllerPak*.sav", "ControllerPak_header.SAV"));
        assert!(matches_pattern("*.sav", "slot1.sav"));
        assert!(matches_pattern("a*b*c", "a-x-b-y-c"));
        assert!(!matches_pattern("controllerPak*.sav", "controllerPak_header.sav.bak"));
        assert!(!matches_pattern("a*b*c", "a-c-b"));
    }

    #[test]
    fn refuses_backup_names_that_are_paths() {
        let dir = empty_dir("names");
        assert!(restore(&dir, "../other/1.zip", &dir).is_err());
    }
}
