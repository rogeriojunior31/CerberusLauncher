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

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use cerberus_shared::Backup;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Newest copies kept per game.
const KEEP: usize = 10;
pub(super) const BEFORE_CLOUD: &str = "Antes de trazer da nuvem";
/// Nothing bigger than this is a save: it's a disc image, a video, an asset pack.
const MAX_FILE: u64 = 64 * 1024 * 1024;
/// The most a guess at the game's whole folder may add up to.
const GAME_FOLDER_MAX: u64 = 50 * 1024 * 1024;
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

#[derive(Serialize, Deserialize)]
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
    if rest.is_empty() {
        return Some(base);
    }
    Some(base.join(rest))
}

fn base_folder(key: &str, install: &Path) -> Option<PathBuf> {
    let env = |name: &str| std::env::var_os(name).map(PathBuf::from);
    let home = env("USERPROFILE").or_else(|| env("HOME"));
    // Off Windows these games keep their files in ~/.config, by their own code.
    let config = env("XDG_CONFIG_HOME").or_else(|| home.as_ref().map(|h| h.join(".config")));
    match key {
        "{install}" => Some(install.to_path_buf()),
        "{local}" if cfg!(windows) => env("LOCALAPPDATA"),
        "{appdata}" if cfg!(windows) => env("APPDATA"),
        "{local}" | "{appdata}" => config,
        "{config}" if cfg!(windows) => env("APPDATA"),
        "{config}" => config,
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

fn collect(templates: &[String], install: &Path) -> Vec<Found> {
    let mut found = Vec::new();
    for (place, template) in templates.iter().enumerate() {
        let Some((dir, here)) = files_of(template, install) else {
            continue;
        };
        let whole_game_folder = template == "{install}";
        let total: u64 = here.iter().map(|path| fs::metadata(path).map(|m| m.len()).unwrap_or(0)).sum();
        // A guess at the game folder that is this big is the game, not its saves.
        if whole_game_folder && total > GAME_FOLDER_MAX {
            continue;
        }
        for absolute in here {
            let Ok(relative) = absolute.strip_prefix(&dir) else {
                continue;
            };
            found.push(Found { place, relative: relative.to_path_buf(), absolute: absolute.clone() });
        }
    }
    found
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
    let found = collect(templates, install);
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
    let created = now();
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
    write_zip(temp.as_file(), &found, &manifest).map_err(|e| format!("Não foi possível criar o backup: {e}"))?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    let queued = cloud.map(|destination| super::cloud::queue(&target, destination)).transpose();
    temp.persist_noclobber(&target).map_err(|e| e.to_string())?;
    prune(backups);
    queued.map_err(|e| format!("Backup local guardado. Falha ao agendar o envio: {e}"))?;
    Ok(Some(Backup { name, created, reason: manifest.reason, files: manifest.files, bytes }))
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

fn describe(path: &Path) -> Option<Backup> {
    let manifest = read_manifest(path).ok()?;
    let name = path.file_name()?.to_string_lossy().into_owned();
    Some(Backup { name, created: manifest.created, reason: manifest.reason, files: manifest.files, bytes: manifest.bytes })
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

fn read_manifest(path: &Path) -> Result<Manifest, String> {
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

fn prune(backups: &Path) {
    for old in list(backups).into_iter().skip(KEEP) {
        let archive = backups.join(old.name);
        if fs::remove_file(&archive).is_ok() {
            let _ = fs::remove_file(archive.with_extension("upload"));
        }
    }
}

/// Puts a copy's files back where they belong for the game as installed now.
/// Returns how many files were written.
#[cfg(test)]
pub fn restore(backups: &Path, name: &str, install: &Path) -> Result<u32, String> {
    restore_open(open_backup(backups, name)?, install, None)
}

pub(super) fn open_backup(backups: &Path, name: &str) -> Result<zip::ZipArchive<fs::File>, String> {
    // Only a plain file name from our own list, never a path.
    let plain = Path::new(name).components().count() == 1 && name.ends_with(".zip");
    if !plain {
        return Err("Backup inválido.".into());
    }
    let path = backups.join(name);
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    zip::ZipArchive::new(file).map_err(|e| format!("Esse backup não pode ser lido: {e}"))
}

pub(super) fn restore_open(mut zip: zip::ZipArchive<fs::File>, install: &Path, allowed_places: Option<&[String]>) -> Result<u32, String> {
    let manifest = manifest_in(&mut zip)?;
    if allowed_places.is_some_and(|places| manifest.places.iter().any(|p| !places.contains(p))) {
        return Err("As pastas do backup na nuvem diferem das pastas de saves deste jogo.".into());
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
        let parent = target.parent().ok_or("O destino do save é inválido.")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let mut temp = tempfile::Builder::new().prefix(".cerberus-").suffix(".tmp").tempfile_in(parent)
            .map_err(|e| format!("Não foi possível preparar {}: {e}", target.display()))?;
        io::copy(&mut entry, temp.as_file_mut()).map_err(|e| format!("Falha ao ler o save {}: {e}", entry.name()))?;
        temp.as_file().sync_all().map_err(|e| e.to_string())?;
        staged.push((temp.into_temp_path(), target));
    }
    // Finish reading and checking every ZIP entry before replacing the first live save.
    let written = staged.len() as u32;
    for (temp, target) in staged {
        temp.persist(&target).map_err(|e| format!("Não foi possível restaurar {}: {e}", target.display()))?;
    }
    Ok(written)
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
