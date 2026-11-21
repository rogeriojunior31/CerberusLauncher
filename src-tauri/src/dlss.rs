//! Swapping a game's DLSS files for another version the player already has.
//!
//! Cerberus downloads nothing here: the player picks a DLL of the same name
//! (from another game, or one they downloaded themselves). The game's own file
//! is kept in Cerberus's data folder first, so "Voltar à original" puts it back.
//! A game update that brings its own DLL makes the kept copy pointless, so it
//! is dropped then.

use std::fs;
use std::path::{Component, Path, PathBuf};

use cerberus_shared::DlssFile;

/// The DLSS files a game may carry, and what each does.
const FILES: [(&str, &str); 3] = [
    ("nvngx_dlss.dll", "DLSS Super Resolution"),
    ("nvngx_dlssg.dll", "DLSS Frame Generation"),
    ("nvngx_dlssd.dll", "DLSS Ray Reconstruction"),
];
const MAX_DEPTH: usize = 8;
const MAX_ENTRIES: usize = 60_000;

fn label_of(name: &str) -> Option<&'static str> {
    FILES.iter().find(|(file, _)| file.eq_ignore_ascii_case(name)).map(|(_, label)| *label)
}

/// Every DLSS file inside the game's folder, by path from the folder.
pub fn find(game_dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut seen = 0;
    let mut stack = vec![(game_dir.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            seen += 1;
            if seen > MAX_ENTRIES {
                return found;
            }
            let Ok(kind) = entry.file_type() else { continue };
            let path = entry.path();
            if kind.is_dir() && depth < MAX_DEPTH {
                stack.push((path, depth + 1));
            } else if kind.is_file() && label_of(&entry.file_name().to_string_lossy()).is_some() {
                if let Ok(relative) = path.strip_prefix(game_dir) {
                    found.push(relative.to_path_buf());
                }
            }
        }
    }
    found.sort();
    found
}

/// Where the game's own copy of a file is kept.
fn kept(data: &Path, game_id: &str, relative: &Path) -> PathBuf {
    let folder: String = game_id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    data.join("dlss").join(folder).join(relative)
}

/// Only plain names going down from the game's folder.
fn inside(relative: &Path) -> bool {
    relative.components().count() > 0 && relative.components().all(|c| matches!(c, Component::Normal(_)))
}

/// The game's DLSS files with their versions, and the original's when swapped.
pub fn list(data: &Path, game_id: &str, game_dir: &Path) -> Vec<DlssFile> {
    find(game_dir)
        .into_iter()
        .map(|relative| {
            let here = game_dir.join(&relative);
            let version = version_of(&here).unwrap_or_else(|| "?".into());
            let backup = kept(data, game_id, &relative);
            let mut original = backup.is_file().then(|| version_of(&backup).unwrap_or_else(|| "?".into()));
            // The game brought its file back (an update or a file check): nothing to undo any more.
            if original.as_deref() == Some(version.as_str()) && same_size(&here, &backup) {
                let _ = fs::remove_file(&backup);
                original = None;
            }
            let name = relative.file_name().unwrap_or_default().to_string_lossy().into_owned();
            DlssFile {
                path: relative.to_string_lossy().into_owned(),
                label: label_of(&name).unwrap_or("DLSS").to_string(),
                version,
                original,
            }
        })
        .collect()
}

fn same_size(a: &Path, b: &Path) -> bool {
    matches!((fs::metadata(a), fs::metadata(b)), (Ok(x), Ok(y)) if x.len() == y.len())
}

/// The file to swap, checked: one of the game's DLSS files.
fn target(game_dir: &Path, relative: &str) -> Result<(PathBuf, PathBuf), String> {
    let relative = PathBuf::from(relative);
    if !inside(&relative) || !find(game_dir).contains(&relative) {
        return Err("Esse arquivo não é um DLSS do jogo.".into());
    }
    Ok((game_dir.join(&relative), relative))
}

fn locked(e: std::io::Error) -> String {
    if e.raw_os_error() == Some(32) {
        "O arquivo está em uso: feche o jogo e tente de novo.".into()
    } else {
        format!("Não foi possível trocar o arquivo: {e}")
    }
}

/// Puts `new_file` in place of the game's file, keeping the game's own first.
pub fn swap(data: &Path, game_id: &str, game_dir: &Path, relative: &str, new_file: &Path) -> Result<String, String> {
    swap_with(data, game_id, game_dir, relative, new_file, looks_like_dlss)
}

fn swap_with(data: &Path, game_id: &str, game_dir: &Path, relative: &str, new_file: &Path, is_dlss: fn(&Path) -> bool) -> Result<String, String> {
    let (here, relative) = target(game_dir, relative)?;
    let name = here.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let picked = new_file.file_name().unwrap_or_default().to_string_lossy().into_owned();
    if !picked.eq_ignore_ascii_case(&name) {
        return Err(format!("Escolha um arquivo {name} (o escolhido é {picked})."));
    }
    let new_version = version_of(new_file).ok_or("Esse arquivo não tem versão: não parece uma DLL do DLSS.")?;
    if !is_dlss(new_file) {
        return Err("Esse arquivo não é da NVIDIA: não parece uma DLL do DLSS.".into());
    }
    if dunce::canonicalize(new_file).ok() == dunce::canonicalize(&here).ok() {
        return Err("Esse já é o arquivo do jogo.".into());
    }
    let backup = kept(data, game_id, &relative);
    if !backup.is_file() {
        fs::create_dir_all(backup.parent().unwrap_or(data)).map_err(|e| e.to_string())?;
        fs::copy(&here, &backup).map_err(|e| format!("Não foi possível guardar o arquivo do jogo: {e}"))?;
    }
    // Copied beside it first, then swapped in one step.
    let fresh = here.with_extension("dll.cerberus-new");
    fs::copy(new_file, &fresh).map_err(locked)?;
    if let Err(e) = fs::rename(&fresh, &here) {
        let _ = fs::remove_file(&fresh);
        return Err(locked(e));
    }
    Ok(new_version)
}

/// Puts the game's own file back.
pub fn restore(data: &Path, game_id: &str, game_dir: &Path, relative: &str) -> Result<(), String> {
    let (here, relative) = target(game_dir, relative)?;
    let backup = kept(data, game_id, &relative);
    if !backup.is_file() {
        return Err("O arquivo original não está guardado.".into());
    }
    let fresh = here.with_extension("dll.cerberus-new");
    fs::copy(&backup, &fresh).map_err(locked)?;
    if let Err(e) = fs::rename(&fresh, &here) {
        let _ = fs::remove_file(&fresh);
        return Err(locked(e));
    }
    let _ = fs::remove_file(&backup);
    Ok(())
}

#[cfg(windows)]
fn version_of(file: &Path) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VS_FIXEDFILEINFO, VerQueryValueW};
    let wide: Vec<u16> = file.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: the buffer is sized by GetFileVersionInfoSizeW; VerQueryValueW
    // points inside it and its length is checked before reading.
    unsafe {
        let size = GetFileVersionInfoSizeW(wide.as_ptr(), std::ptr::null_mut());
        if size == 0 {
            return None;
        }
        let mut buffer = vec![0u8; size as usize];
        if GetFileVersionInfoW(wide.as_ptr(), 0, size, buffer.as_mut_ptr().cast()) == 0 {
            return None;
        }
        let mut info: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut len = 0u32;
        let root: Vec<u16> = "\\".encode_utf16().chain([0]).collect();
        if VerQueryValueW(buffer.as_ptr().cast(), root.as_ptr(), &mut info, &mut len) == 0 || (len as usize) < std::mem::size_of::<VS_FIXEDFILEINFO>() {
            return None;
        }
        let fixed = &*(info as *const VS_FIXEDFILEINFO);
        Some(format!(
            "{}.{}.{}.{}",
            fixed.dwFileVersionMS >> 16,
            fixed.dwFileVersionMS & 0xffff,
            fixed.dwFileVersionLS >> 16,
            fixed.dwFileVersionLS & 0xffff
        ))
    }
}

#[cfg(not(windows))]
fn version_of(_: &Path) -> Option<String> {
    None
}

/// A Windows DLL that names NVIDIA in its text (its version information does).
fn looks_like_dlss(file: &Path) -> bool {
    let Ok(bytes) = fs::read(file) else { return false };
    let nvidia: Vec<u8> = "NVIDIA".encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
    bytes.starts_with(b"MZ") && bytes.windows(nvidia.len()).any(|w| w == nvidia.as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in DLL: "MZ" and NVIDIA's name, but no version (so swaps are refused).
    fn fake(path: &Path, extra: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut bytes = b"MZ".to_vec();
        bytes.extend("NVIDIA".encode_utf16().flat_map(|c| c.to_le_bytes()));
        bytes.extend(extra);
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn finds_the_dlss_files_anywhere_in_the_game() {
        let dir = tempfile::tempdir().unwrap();
        fake(&dir.path().join("Engine/Plugins/DLSS/nvngx_dlss.dll"), b"");
        fake(&dir.path().join("bin/NVNGX_DLSSG.DLL"), b"");
        fs::write(dir.path().join("bin/other.dll"), b"MZ").unwrap();
        let found = find(dir.path());
        assert_eq!(found.len(), 2);
        assert!(found.contains(&PathBuf::from("Engine/Plugins/DLSS/nvngx_dlss.dll")));
        assert_eq!(label_of("NVNGX_DLSSG.DLL"), Some("DLSS Frame Generation"));
    }

    #[test]
    fn only_the_games_own_dlss_files_can_be_swapped() {
        let dir = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        fake(&dir.path().join("nvngx_dlss.dll"), b"");
        let elsewhere = data.path().join("x/nvngx_dlss.dll");
        fake(&elsewhere, b"new");
        assert!(swap(data.path(), "steam:1", dir.path(), "../nvngx_dlss.dll", &elsewhere).is_err());
        assert!(swap(data.path(), "steam:1", dir.path(), "other.dll", &elsewhere).is_err());
        let wrong_name = data.path().join("x/nvngx_dlssg.dll");
        fake(&wrong_name, b"");
        let refused = swap(data.path(), "steam:1", dir.path(), "nvngx_dlss.dll", &wrong_name).unwrap_err();
        assert!(refused.contains("nvngx_dlss.dll"), "{refused}");
        // Without version information the file isn't taken, and the game's file stays as it was.
        assert!(swap(data.path(), "steam:1", dir.path(), "nvngx_dlss.dll", &elsewhere).is_err());
        assert!(!fs::read(dir.path().join("nvngx_dlss.dll")).unwrap().ends_with(b"new"));
        assert!(restore(data.path(), "steam:1", dir.path(), "nvngx_dlss.dll").is_err(), "nothing kept yet");
    }

    /// With a real versioned DLL from Windows standing in for DLSS files.
    #[cfg(windows)]
    #[test]
    fn swapping_keeps_the_original_and_restoring_brings_it_back() {
        let system = PathBuf::from(std::env::var("SystemRoot").unwrap_or(r"C:\Windows".into())).join("System32");
        let (a, b) = (system.join("kernel32.dll"), system.join("user32.dll"));
        assert!(version_of(&a).is_some());
        let dir = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let game_file = dir.path().join("nvngx_dlss.dll");
        fs::copy(&a, &game_file).unwrap();
        let picked = data.path().join("pick/nvngx_dlss.dll");
        fs::create_dir_all(picked.parent().unwrap()).unwrap();
        fs::copy(&b, &picked).unwrap();
        // (Windows reports its own System32 files with an older version; copies tell the truth.)
        let (old, new) = (version_of(&game_file), version_of(&picked));
        // Windows' own DLLs don't name NVIDIA: that one check is left out here.
        assert!(swap(data.path(), "steam:1", dir.path(), "nvngx_dlss.dll", &picked).is_err());
        let swapped = swap_with(data.path(), "steam:1", dir.path(), "nvngx_dlss.dll", &picked, |_| true).unwrap();
        assert_eq!(Some(swapped), new);
        let backup = kept(data.path(), "steam:1", Path::new("nvngx_dlss.dll"));
        assert_eq!(fs::read(&backup).unwrap(), fs::read(&a).unwrap(), "the game's own file is kept");
        assert!(!dir.path().join("nvngx_dlss.dll.cerberus-new").exists());
        // Swapping again keeps the first original, not the swapped one.
        swap_with(data.path(), "steam:1", dir.path(), "nvngx_dlss.dll", &picked, |_| true).unwrap();
        assert_eq!(fs::read(&backup).unwrap(), fs::read(&a).unwrap());
        let listed = list(data.path(), "steam:1", dir.path());
        assert_eq!(Some(listed[0].version.clone()), new);
        assert_eq!(listed[0].original, old);
        restore(data.path(), "steam:1", dir.path(), "nvngx_dlss.dll").unwrap();
        assert_eq!(fs::read(&game_file).unwrap(), fs::read(&a).unwrap());
        assert!(!backup.exists());
        assert_eq!(list(data.path(), "steam:1", dir.path())[0].original, None);
    }
}
