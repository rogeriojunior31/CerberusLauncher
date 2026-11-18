//! The player's choices, kept in settings.json in the app's data folder.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Where new recomps are installed; `None` for the default inside the data folder.
    #[serde(default)]
    pub recomps_dir: Option<PathBuf>,
    /// Folders the player keeps ROMs and disc images in, looked through for games
    /// that have a recomp.
    #[serde(default)]
    pub rom_dirs: Vec<PathBuf>,
    /// A folder another program syncs between PCs, where save backups are copied.
    #[serde(default)]
    pub cloud_dir: Option<PathBuf>,
    /// Catalog sources the player added, by URL.
    #[serde(default)]
    pub sources: Vec<String>,
}

pub fn load(data_dir: &Path) -> Settings {
    let text = fs::read_to_string(file(data_dir)).unwrap_or_default();
    // A broken file just means the defaults; never refuse to start over it.
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn save(data_dir: &Path, settings: &Settings) -> Result<(), String> {
    fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    let target = file(data_dir);
    let temp = target.with_extension("json.tmp");
    fs::write(&temp, json).map_err(|e| e.to_string())?;
    fs::rename(&temp, &target).map_err(|e| e.to_string())
}

fn file(data_dir: &Path) -> PathBuf {
    data_dir.join("settings.json")
}

/// Total size of the files directly inside `dir`.
pub fn folder_size(dir: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries.flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_settings_come_back() {
        let dir = std::env::temp_dir().join(format!("cerberus-settings-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let chosen = Settings {
            recomps_dir: Some(PathBuf::from(r"D:\Recomps")),
            rom_dirs: vec![PathBuf::from(r"E:\ROMs")],
            cloud_dir: None,
            sources: vec!["https://example.com/games.json".into()],
        };
        save(&dir, &chosen).unwrap();
        assert_eq!(load(&dir).recomps_dir, chosen.recomps_dir);
    }

    #[test]
    fn a_broken_file_means_defaults() {
        let dir = std::env::temp_dir().join(format!("cerberus-settings-broken-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("settings.json"), "{ not json").unwrap();
        assert!(load(&dir).recomps_dir.is_none());
    }
}
