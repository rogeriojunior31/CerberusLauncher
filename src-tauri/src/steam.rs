//! Finds games installed through Steam by reading its library files on disk.

use std::fs;
use std::path::{Path, PathBuf};

use cerberus_shared::{Game, Source};

/// Steamworks Common Redistributables: installed with many games, but not playable.
const NOT_GAMES: &[u32] = &[228980];

pub fn installed_games() -> Vec<Game> {
    let Some(root) = steam_root() else {
        return Vec::new();
    };
    let covers = cover_cache_dir(&root);
    let mut games: Vec<Game> = Vec::new();
    for library in library_folders(&root) {
        let Ok(entries) = fs::read_dir(library.join("steamapps")) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !is_app_manifest(&path) {
                continue;
            }
            let Some(game) = read_manifest(&path, &library, &covers) else {
                continue;
            };
            // A game copied between libraries leaves a manifest in both; keep one.
            if games.iter().any(|known| known.id == game.id) {
                continue;
            }
            games.push(game);
        }
    }
    games.sort_by_key(|game| game.name.to_lowercase());
    games
}

fn is_app_manifest(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    name.starts_with("appmanifest_") && name.ends_with(".acf")
}

/// Where Steam keeps the library art it already downloaded for every owned game.
pub fn cover_cache_dir(root: &Path) -> PathBuf {
    root.join("appcache").join("librarycache")
}

pub fn root() -> Option<PathBuf> {
    steam_root()
}

/// Art from Steam's local cache, the first of `names` found. Older games keep it
/// as `<appid>/library_600x900.jpg`; newer ones put it in a hash-named
/// subfolder, and name the tall cover `library_capsule.jpg`.
fn local_art(covers: &Path, app_id: u32, names: &[&str]) -> Option<String> {
    let dir = covers.join(app_id.to_string());
    for name in names {
        let direct = dir.join(name);
        if direct.is_file() {
            return clean_path(&direct);
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            return None;
        };
        for entry in entries.flatten() {
            let nested = entry.path().join(name);
            if nested.is_file() {
                return clean_path(&nested);
            }
        }
    }
    None
}

fn clean_path(path: &Path) -> Option<String> {
    let path = dunce::canonicalize(path).ok()?;
    Some(path.to_string_lossy().into_owned())
}

fn read_manifest(path: &Path, library: &Path, covers: &Path) -> Option<Game> {
    let text = fs::read_to_string(path).ok()?;
    let app_id: u32 = first_value(&text, "appid")?.parse().ok()?;
    if NOT_GAMES.contains(&app_id) {
        return None;
    }
    let name = first_value(&text, "name")?;
    let install_dir = first_value(&text, "installdir")?;
    let install_dir = library.join("steamapps").join("common").join(install_dir);
    let cover_url = format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{app_id}/library_600x900.jpg");
    let hero_url = format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{app_id}/library_hero.jpg");
    Some(Game {
        id: format!("steam:{app_id}"),
        name,
        source: Source::Steam { app_id },
        install_dir: install_dir.to_string_lossy().into_owned(),
        cover_path: local_art(covers, app_id, &["library_600x900.jpg", "library_capsule.jpg"]),
        cover_url: Some(cover_url),
        hero_path: local_art(covers, app_id, &["library_hero.jpg"]),
        hero_url: Some(hero_url),
        playtime_secs: 0,
        last_played: None,
        last_session_secs: None,
        launch_args: String::new(),
        tracking_exe: None,
        game_copy: None,
        setup_pending: false,
        next_step: Default::default(),
        setting_picks: Default::default(),
        external: false,
    })
}

/// The main Steam folder plus every extra library the user added (other drives etc).
fn library_folders(root: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![root.to_path_buf()];
    let vdf = root.join("steamapps").join("libraryfolders.vdf");
    if let Ok(text) = fs::read_to_string(vdf) {
        let listed = pairs(&text).filter(|(key, _)| key == "path");
        candidates.extend(listed.map(|(_, value)| PathBuf::from(value)));
    }
    // The registry spells the main folder "c:/program files (x86)/steam" while the
    // .vdf says "C:\Program Files (x86)\Steam"; compare the resolved paths so the
    // same folder is not scanned twice.
    let mut folders: Vec<PathBuf> = Vec::new();
    for candidate in candidates {
        let Ok(folder) = dunce::canonicalize(&candidate) else {
            continue;
        };
        if !folders.contains(&folder) {
            folders.push(folder);
        }
    }
    folders
}

fn first_value(text: &str, wanted: &str) -> Option<String> {
    pairs(text).find(|(key, _)| key == wanted).map(|(_, value)| value)
}

/// Steam's .vdf/.acf files are nested `"key" "value"` lines. We only need a few
/// flat keys, so scanning lines is enough and saves a parser dependency.
fn pairs(text: &str) -> impl Iterator<Item = (String, String)> + '_ {
    text.lines().filter_map(|line| {
        let mut quoted = line.split('"').skip(1).step_by(2);
        let key = quoted.next()?;
        let value = quoted.next()?;
        Some((key.to_lowercase(), value.replace("\\\\", "\\")))
    })
}

#[cfg(windows)]
fn steam_root() -> Option<PathBuf> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let from_registry = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Valve\Steam")
        .and_then(|key| key.get_value::<String, _>("SteamPath"))
        .ok()
        .map(PathBuf::from);
    let fallback = PathBuf::from(r"C:\Program Files (x86)\Steam");
    from_registry.into_iter().chain([fallback]).find(|p| p.is_dir())
}

#[cfg(not(windows))]
fn steam_root() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    let candidates = [
        home.join(".steam/steam"),
        home.join(".local/share/Steam"),
        home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
        home.join("Library/Application Support/Steam"),
    ];
    candidates.into_iter().find(|p| p.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_flat_keys_from_a_manifest() {
        let acf = "\"AppState\"\n{\n\t\"appid\"\t\t\"620\"\n\t\"name\"\t\t\"Portal 2\"\n\t\"installdir\"\t\t\"Portal 2\"\n}";
        assert_eq!(first_value(acf, "appid").as_deref(), Some("620"));
        assert_eq!(first_value(acf, "name").as_deref(), Some("Portal 2"));
    }

    #[test]
    fn unescapes_windows_library_paths() {
        let vdf = "\"libraryfolders\"\n{\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n\t}\n}";
        assert_eq!(first_value(vdf, "path").as_deref(), Some(r"D:\SteamLibrary"));
    }
}
