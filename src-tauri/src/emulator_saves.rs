//! The classics' saves: each emulator keeps them in its own folders (memory
//! cards, save states), shared by all its games. Cerberus copies those folders
//! after each classic closes, with the same backups the recomps get (the ten
//! newest kept), and can put one back.

use cerberus_shared::{tr, Backup};
use std::path::{Path, PathBuf};

use crate::recomp::saves;

/// Where an emulator keeps its saves on this PC: a root, and the folders in it.
#[derive(Debug, PartialEq)]
pub struct Places {
    pub root: PathBuf,
    pub folders: Vec<String>,
}

fn portable(exe: &Path) -> bool {
    exe.parent().is_some_and(|d| d.join("portable.txt").is_file() || d.join("portable.ini").is_file())
}

/// The first of these folders that exists, else the first.
fn existing_or_first(options: Vec<PathBuf>) -> Option<PathBuf> {
    options.iter().find(|p| p.is_dir()).cloned().or_else(|| options.into_iter().next())
}

/// The emulator's user folder: beside it when portable, else where it keeps
/// its data on Windows (Documents, possibly inside OneDrive, or AppData).
fn user_dir(id: &str, exe: &Path) -> Option<PathBuf> {
    let beside = exe.parent()?.to_path_buf();
    let documents = crate::standalone::documents();
    let env = |name: &str| std::env::var_os(name).map(PathBuf::from);
    match id {
        "duckstation" if portable(exe) => Some(beside),
        "duckstation" => existing_or_first([documents.map(|d| d.join("DuckStation")), env("LOCALAPPDATA").map(|d| d.join("DuckStation"))].into_iter().flatten().collect()),
        "pcsx2" if portable(exe) => Some(beside),
        "pcsx2" => documents.map(|d| d.join("PCSX2")),
        "dolphin" if beside.join("portable.txt").is_file() => Some(beside.join("User")),
        "dolphin" => existing_or_first([documents.map(|d| d.join("Dolphin Emulator")), env("APPDATA").map(|d| d.join("Dolphin Emulator"))].into_iter().flatten().collect()),
        "ppsspp" if beside.join("memstick").is_dir() => Some(beside.join("memstick").join("PSP")),
        "ppsspp" => documents.map(|d| d.join("PPSSPP").join("PSP")),
        _ => None,
    }
}

/// Where this emulator's saves are. RetroArch's come from its own settings.
pub fn places(id: &str, exe: &Path) -> Option<Places> {
    if id == "retroarch" {
        let saves = crate::emulation::retroarch_dir(exe, "savefile_directory", "saves");
        let states = crate::emulation::retroarch_dir(exe, "savestate_directory", "states");
        let root = saves.parent()?.to_path_buf();
        let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned());
        let mut folders = vec![name(&saves)?];
        // Save states too, when they sit beside the saves (the usual setup).
        if states.parent() == Some(root.as_path()) {
            folders.extend(name(&states));
        }
        return Some(Places { root, folders });
    }
    let folders: &[&str] = match id {
        "duckstation" => &["memcards", "savestates"],
        "pcsx2" => &["memcards", "sstates"],
        "dolphin" => &["GC", "StateSaves"],
        "ppsspp" => &["SAVEDATA", "PPSSPP_STATE"],
        _ => return None,
    };
    Some(Places { root: user_dir(id, exe)?, folders: folders.iter().map(|f| f.to_string()).collect() })
}

fn templates(places: &Places) -> Vec<String> {
    places.folders.iter().map(|f| format!("{{install}}/{f}")).collect()
}

/// This emulator's backups, kept with Cerberus's data.
pub fn backups_dir(data: &Path, id: &str) -> PathBuf {
    data.join("backups").join(format!("emulator-{id}"))
}

/// Copies the emulator's saves now. `Ok(None)` when nothing changed since the last copy.
pub fn backup(data: &Path, id: &str, exe: &Path, reason: &str) -> Result<Option<Backup>, String> {
    let places = places(id, exe).ok_or(tr!("O Cerberus não sabe onde esse emulador guarda os saves."))?;
    if !places.root.is_dir() {
        return Ok(None);
    }
    saves::backup(&backups_dir(data, id), &templates(&places), &places.root, reason, None)
}

/// The backups of this emulator, newest first.
pub fn list(data: &Path, id: &str) -> Vec<Backup> {
    saves::list(&backups_dir(data, id))
}

/// Puts a backup back, after a copy of the saves as they are now.
pub fn restore(data: &Path, id: &str, exe: &Path, name: &str) -> Result<u32, String> {
    let places = places(id, exe).ok_or(tr!("O Cerberus não sabe onde esse emulador guarda os saves."))?;
    let dir = backups_dir(data, id);
    let selected = saves::open_backup(&dir, name)?;
    saves::backup(&dir, &templates(&places), &places.root, cerberus_shared::tk!("Antes de restaurar"), None)?;
    saves::restore_open(selected, &places.root, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn a_portable_duckstation_keeps_its_cards_beside_it_and_they_come_back() {
        let root = tempfile::tempdir().unwrap();
        let emu = root.path().join("duckstation");
        fs::create_dir_all(emu.join("memcards")).unwrap();
        fs::write(emu.join("portable.txt"), b"").unwrap();
        let exe = emu.join("duckstation-qt-x64-ReleaseLTCG.exe");
        fs::write(&exe, b"MZ").unwrap();
        fs::write(emu.join("memcards").join("shared_card_1.mcd"), b"crash progress").unwrap();
        let data = root.path().join("data");

        assert_eq!(places("duckstation", &exe).unwrap().root, emu);
        let made = backup(&data, "duckstation", &exe, "Depois de jogar").unwrap().expect("a first copy");
        fs::write(emu.join("memcards").join("shared_card_1.mcd"), b"lost").unwrap();
        restore(&data, "duckstation", &exe, &made.name).unwrap();
        assert_eq!(fs::read(emu.join("memcards").join("shared_card_1.mcd")).unwrap(), b"crash progress");
        // The saves replaced were kept first.
        assert!(list(&data, "duckstation").len() >= 2);
    }

    #[test]
    fn retroarch_saves_come_from_its_settings() {
        let root = tempfile::tempdir().unwrap();
        let exe = root.path().join("retroarch.exe");
        assert_eq!(places("retroarch", &exe).unwrap(), Places { root: root.path().to_path_buf(), folders: vec!["saves".into(), "states".into()] });
    }
}
