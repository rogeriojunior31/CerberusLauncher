//! The player's state files (settings, favorites, collections, notes, art,
//! play time...): read and written so that a file that couldn't be read is
//! never replaced by an empty one.
//!
//! - Reading tells "there is no file yet" (a new player: defaults) apart from
//!   "there is a file I couldn't read" (in use, damaged). A file in use is tried
//!   again for a moment; a damaged one falls back to the last good copy kept
//!   beside it (`<name>.bak`); with neither, the damaged file is kept aside
//!   (`<name>.unreadable-<time>`) before the defaults are used, so nothing the
//!   player had is lost even then.
//! - Writing goes to a temporary file of its own, is flushed to disk, keeps the
//!   copy being replaced as `<name>.bak`, and only then takes the real name.
//!   Writers of the same file wait for each other.
//! - `update` reads, changes and writes under that same wait, so two tasks
//!   changing one file never undo each other.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde::Serialize;

/// One lock per file, for writers and for read-change-write.
fn lock_for(path: &Path) -> Arc<Mutex<()>> {
    static LOCKS: Mutex<Option<HashMap<PathBuf, Arc<Mutex<()>>>>> = Mutex::new(None);
    let mut all = LOCKS.lock().unwrap_or_else(|p| p.into_inner());
    all.get_or_insert_with(HashMap::new).entry(path.to_path_buf()).or_default().clone()
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

/// The copy of the previous good version, kept beside the file.
pub fn backup_of(path: &Path) -> PathBuf {
    with_suffix(path, ".bak")
}

/// What reading a state file gave.
#[derive(Debug, PartialEq)]
pub enum Read<T> {
    /// No file: nothing was ever saved.
    Missing,
    /// The file (or, when it was damaged, its last good copy).
    Found(T),
    /// A file exists but couldn't be read, nor its copy.
    Unreadable(String),
}

/// The file's text; a file held by another program (antivirus, cloud sync) is
/// tried again for about a third of a second.
fn text_of(path: &Path) -> std::io::Result<String> {
    let mut last = None;
    for attempt in 0..4 {
        match fs::read_to_string(path) {
            Ok(text) => return Ok(text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(e),
            Err(e) => {
                last = Some(e);
                if attempt < 3 {
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("unreadable")))
}

fn parse<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    match text_of(path) {
        Ok(text) => serde_json::from_str(text.trim_start_matches('\u{feff}')).map(Some).map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

pub fn read<T: DeserializeOwned>(path: &Path) -> Read<T> {
    match parse(path) {
        Ok(Some(value)) => Read::Found(value),
        Ok(None) => match parse(&backup_of(path)) {
            // The file was lost between its copy and the rename: the copy is the latest.
            Ok(Some(value)) => Read::Found(value),
            _ => Read::Missing,
        },
        Err(e) => match parse(&backup_of(path)) {
            Ok(Some(value)) => {
                crate::state_file::note(path, &e, true);
                Read::Found(value)
            }
            _ => Read::Unreadable(e),
        },
    }
}

/// Writes down why a file couldn't be read (for the history and the log).
fn note(path: &Path, why: &str, recovered: bool) {
    eprintln!(
        "state file {}: {why}{}",
        path.display(),
        if recovered { " (used the last good copy)" } else { "" }
    );
}

/// The file's value, the default when there is no file, and also the default
/// when it can't be read (after putting the unreadable file aside, so it's
/// still there for the player and is never saved over).
pub fn load_or_default<T: DeserializeOwned + Default>(path: &Path) -> T {
    match read(path) {
        Read::Found(value) => value,
        Read::Missing => T::default(),
        Read::Unreadable(why) => {
            note(path, &why, false);
            set_aside(path);
            T::default()
        }
    }
}

/// Keeps an unreadable file under another name, once per minute at most.
fn set_aside(path: &Path) {
    let lock = lock_for(path);
    let _held = lock.lock().unwrap_or_else(|p| p.into_inner());
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() / 60).unwrap_or(0);
    let aside = with_suffix(path, &format!(".unreadable-{stamp}"));
    if !aside.exists() {
        let _ = fs::copy(path, &aside);
    }
}

/// Writes the value: temporary file, flushed, previous copy kept, renamed.
pub fn save<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<(), String> {
    let lock = lock_for(path);
    let _held = lock.lock().unwrap_or_else(|p| p.into_inner());
    write_locked(path, value)
}

fn write_locked<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<(), String> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    let temp = with_suffix(path, &format!(".{}-{}.tmp", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed)));
    let written = (|| -> std::io::Result<()> {
        let mut file = fs::File::create(&temp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()
    })();
    if let Err(e) = written {
        let _ = fs::remove_file(&temp);
        return Err(e.to_string());
    }
    // The copy being replaced stays as the last good one, if it was good.
    if parse::<serde_json::Value>(path).is_ok_and(|v| v.is_some()) {
        let _ = fs::copy(path, backup_of(path));
    }
    let renamed = rename_retrying(&temp, path);
    if renamed.is_err() {
        let _ = fs::remove_file(&temp);
    }
    renamed.map_err(|e| e.to_string())
}

/// A rename over a file another program holds for an instant is tried again.
fn rename_retrying(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut last = None;
    for _ in 0..5 {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(Duration::from_millis(80));
            }
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("rename")))
}

/// Reads, changes and writes the file with no other writer in between. A file
/// that exists but can't be read is refused: changing it would erase it.
pub fn update<T, R>(path: &Path, change: impl FnOnce(&mut T) -> R) -> Result<R, String>
where
    T: Serialize + DeserializeOwned + Default,
{
    let lock = lock_for(path);
    let _held = lock.lock().unwrap_or_else(|p| p.into_inner());
    let mut value = match read(path) {
        Read::Found(value) => value,
        Read::Missing => T::default(),
        Read::Unreadable(why) => {
            note(path, &why, false);
            return Err(cerberus_shared::tr!("Não foi possível ler {}: o arquivo ficou como estava. Feche outros programas que o usem e tente de novo.", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
        }
    };
    let out = change(&mut value);
    write_locked(path, &value)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    type Map = BTreeMap<String, u32>;

    #[test]
    fn no_file_is_a_new_start_and_a_damaged_one_comes_back_from_its_copy() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("favorites.json");
        assert_eq!(read::<Map>(&file), Read::Missing);
        save(&file, &Map::from([("a".into(), 1)])).unwrap();
        save(&file, &Map::from([("a".into(), 1), ("b".into(), 2)])).unwrap();
        fs::write(&file, "{ broken").unwrap();
        // The copy holds the version before the last save: everything but the newest change survives.
        assert_eq!(read::<Map>(&file), Read::Found(Map::from([("a".into(), 1)])));
    }

    #[test]
    fn an_unreadable_file_is_never_saved_over_by_update_and_is_kept_aside_by_load() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("notes.json");
        fs::write(&file, "not json and no copy").unwrap();
        assert!(update::<Map, _>(&file, |m| m.insert("x".into(), 1)).is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), "not json and no copy", "untouched");
        let loaded: Map = load_or_default(&file);
        assert!(loaded.is_empty());
        let aside: Vec<_> = fs::read_dir(dir.path()).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().contains(".unreadable-")).collect();
        assert_eq!(aside.len(), 1, "the damaged file is kept for the player");
        assert_eq!(fs::read_to_string(aside[0].path()).unwrap(), "not json and no copy");
    }

    #[test]
    fn many_writers_at_once_lose_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let file = Arc::new(dir.path().join("store-playtime.json"));
        let threads: Vec<_> = (0..16)
            .map(|i| {
                let file = file.clone();
                std::thread::spawn(move || {
                    for j in 0..10 {
                        update::<Map, _>(&file, |m| m.insert(format!("{i}-{j}"), j)).unwrap();
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let all: Map = load_or_default(&file);
        assert_eq!(all.len(), 160, "every change of every task is there");
        let leftovers = fs::read_dir(dir.path()).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".tmp")).count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn a_file_saved_with_a_bom_still_reads() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        fs::write(&file, "\u{feff}{\"a\": 1}").unwrap();
        assert_eq!(read::<Map>(&file), Read::Found(Map::from([("a".into(), 1)])));
    }
}
