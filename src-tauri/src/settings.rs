//! The player's choices, kept in settings.json in the app's data folder.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// The first-run setup was finished (or skipped). A settings file from before
    /// the setup existed counts as done: that player already set things up.
    #[serde(default = "yes")]
    pub setup_done: bool,
    /// Opens in the TV screen (Big Picture Mode) instead of the desktop window.
    #[serde(default)]
    pub start_in_tv: bool,
    /// The player's own SteamGridDB API key (free, from their account there).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steamgriddb_key: Option<String>,
    /// The page the desktop window opens on: "home", "library" or "discover".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub start_page: String,
    /// The player's RetroAchievements account: username and their own Web API key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ra_user: Option<String>,
    /// The Epic Games account signed in: its name, and the refresh token (never a password).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epic_user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epic_refresh: Option<String>,
    /// The Microsoft account behind the Xbox app: its gamertag and Live refresh token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xbox_user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xbox_refresh: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ra_key: Option<String>,
    /// RetroAchievements' connect token, traded once for the password (which is
    /// never kept), written into RetroArch whenever it lacks it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ra_token: Option<String>,
    /// The version whose "What's new" was shown; a newer running version shows its notes once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seen_version: Option<String>,
    /// Recomp download limit in MB/s; 0 for none.
    #[serde(default)]
    pub download_limit: u32,
    /// Downloads go slower while a game is open (on unless turned off).
    #[serde(default = "yes")]
    pub slow_while_playing: bool,
    /// Recomps whose updates the player paused, by game id.
    #[serde(default, skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    pub paused_updates: std::collections::BTreeSet<String>,
    /// Steam's Web API, turned on by the player: their own key and their SteamID64.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steam_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steam_id: Option<String>,
    /// recomplauncher.com's catalog is used besides Cerberus's own (on unless turned off).
    #[serde(default = "yes")]
    pub site_catalog: bool,
    /// The recomp catalog is shown at all (on unless turned off). Off, only the
    /// recomps already installed stay, in the library, with their updates.
    #[serde(default = "yes")]
    pub recomp_catalog: bool,
    /// Closing the window keeps Cerberus in the notification area (tray).
    #[serde(default)]
    pub close_to_tray: bool,
    /// The app's language ("pt-BR", "en"); none: Windows' language.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// The monitor Cerberus opens on, by Windows' name for it; none: where Windows puts it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monitor: Option<String>,
    /// Opened with Windows, Cerberus starts in the tray instead of showing its window.
    #[serde(default)]
    pub start_hidden: bool,
    /// A RetroArch the player picked, instead of Cerberus's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retroarch: Option<PathBuf>,
    /// Standalone emulators the player pointed at, by id ("pcsx2" → its program).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub emulators: std::collections::BTreeMap<String, PathBuf>,
    /// Which emulator plays each console ("retroarch" or a standalone id); unset: the usual one.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub play_with: std::collections::BTreeMap<String, String>,
    /// The folders the emulators already list games from are looked through too.
    #[serde(default = "yes")]
    pub emulator_folders: bool,
    /// PC Game Pass's games listed in the library (not installed until the Xbox
    /// app installs them). Off until the player says they subscribe: a player
    /// without it would see hundreds of games they can't play.
    #[serde(default)]
    pub game_pass: bool,
    /// What happens while a game is open.
    #[serde(default)]
    pub in_game: cerberus_shared::InGame,
    /// Which notices to give, and where.
    #[serde(default)]
    pub notices: cerberus_shared::Notices,
}

fn yes() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        // Every field's own default, except that a brand-new player hasn't done the setup yet.
        let all: Settings = serde_json::from_str("{}").expect("every field has a default");
        Settings { setup_done: false, ..all }
    }
}

pub fn load(data_dir: &Path) -> Settings {
    match crate::state_file::read(&file(data_dir)) {
        crate::state_file::Read::Found(settings) => settings,
        // No file yet: a new player, who gets the first-run setup.
        crate::state_file::Read::Missing => Settings::default(),
        // A damaged file with no good copy: kept aside, and the app starts with
        // the defaults (never refusing to start over it, never asking the setup again).
        crate::state_file::Read::Unreadable(_) => Settings { setup_done: true, ..crate::state_file::load_or_default(&file(data_dir)) },
    }
}

pub fn save(data_dir: &Path, settings: &Settings) -> Result<(), String> {
    crate::state_file::save(&file(data_dir), settings)
}

/// Which store account a renewed token belongs to.
#[derive(Clone, Copy)]
pub enum Account {
    Epic,
    Xbox,
}

/// Keeps a store's renewed refresh token, but only if the account is still the
/// one the renewal started from: a sign-out (or another sign-in) in the
/// meantime wins, and the account doesn't come back.
pub fn renew_token(data_dir: &Path, account: Account, started_with: &str, renewed: &str) {
    let _ = update(data_dir, |s| {
        let slot = match account {
            Account::Epic => &mut s.epic_refresh,
            Account::Xbox => &mut s.xbox_refresh,
        };
        if slot.as_deref() == Some(started_with) {
            *slot = Some(renewed.to_string());
        }
    });
}

/// Reads, changes and saves the settings with no other change in between (two
/// tasks saving at once can't undo each other).
pub fn update<R>(data_dir: &Path, change: impl FnOnce(&mut Settings) -> R) -> Result<R, String> {
    crate::state_file::update(&file(data_dir), change)
}

fn file(data_dir: &Path) -> PathBuf {
    data_dir.join("settings.json")
}

/// Total size of the files directly inside `dir`.
/// The same, kept with the folder's time: summed again only when the folder
/// changed (a picture added, the cache cleared), not on every Settings view.
pub fn folder_size_kept(dir: &Path) -> u64 {
    static KEPT: std::sync::Mutex<Option<(std::path::PathBuf, std::time::SystemTime, u64)>> = std::sync::Mutex::new(None);
    let changed = fs::metadata(dir).and_then(|m| m.modified()).ok();
    let mut kept = KEPT.lock().unwrap_or_else(|p| p.into_inner());
    if let (Some((path, at, bytes)), Some(changed)) = (kept.as_ref(), changed) {
        if path == dir && *at == changed {
            return *bytes;
        }
    }
    let bytes = folder_size(dir);
    if let Some(at) = changed {
        *kept = Some((dir.to_path_buf(), at, bytes));
    }
    bytes
}

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
    fn game_pass_is_off_until_the_player_turns_it_on_and_a_saved_choice_stays() {
        assert!(!Settings::default().game_pass, "a new player: off");
        let chose: Settings = serde_json::from_str(r#"{"game_pass": true}"#).unwrap();
        assert!(chose.game_pass, "one who turned it on keeps it");
    }

    #[test]
    fn the_site_catalog_is_on_unless_turned_off() {
        assert!(Settings::default().site_catalog);
        let old: Settings = serde_json::from_str(r#"{"rom_dirs": []}"#).unwrap();
        assert!(old.site_catalog);
        let off: Settings = serde_json::from_str(r#"{"site_catalog": false}"#).unwrap();
        assert!(!off.site_catalog);
    }

    #[test]
    fn the_recomp_catalog_is_on_unless_turned_off() {
        assert!(Settings::default().recomp_catalog);
        let old: Settings = serde_json::from_str(r#"{"rom_dirs": []}"#).unwrap();
        assert!(old.recomp_catalog, "settings saved before the switch existed: on");
        let off: Settings = serde_json::from_str(r#"{"recomp_catalog": false}"#).unwrap();
        assert!(!off.recomp_catalog);
    }

    #[test]
    fn a_renewal_that_finishes_after_sign_out_doesnt_bring_the_account_back() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Settings::default();
        s.epic_user = Some("Me".into());
        s.epic_refresh = Some("OLD".into());
        save(dir.path(), &s).unwrap();
        // Signed in: the renewal keeps the new token.
        renew_token(dir.path(), Account::Epic, "OLD", "NEW");
        assert_eq!(load(dir.path()).epic_refresh.as_deref(), Some("NEW"));
        // Signed out while a renewal (started with NEW) was asking.
        update(dir.path(), |s| {
            s.epic_user = None;
            s.epic_refresh = None;
        })
        .unwrap();
        renew_token(dir.path(), Account::Epic, "NEW", "NEWER");
        assert!(load(dir.path()).epic_refresh.is_none(), "the account stays signed out");
    }

    #[test]
    fn a_damaged_settings_file_is_kept_and_its_last_good_copy_used() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Settings::default();
        s.rom_dirs = vec!["D:/ROMS".into()];
        s.setup_done = true;
        save(dir.path(), &s).unwrap();
        s.start_page = "library".into();
        save(dir.path(), &s).unwrap();
        fs::write(file(dir.path()), "{ cut in hal").unwrap();
        let back = load(dir.path());
        assert_eq!(back.rom_dirs, vec![std::path::PathBuf::from("D:/ROMS")], "the folders survive");
        assert!(back.setup_done);
    }

    #[test]
    fn only_a_new_player_gets_the_first_run_setup() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!load(dir.path()).setup_done, "no settings file: a new player");
        fs::write(file(dir.path()), r#"{"rom_dirs": ["D:/ROMS"]}"#).unwrap();
        assert!(load(dir.path()).setup_done, "a file from before the setup existed");
        let mut s = load(dir.path());
        s.setup_done = false;
        save(dir.path(), &s).unwrap();
        assert!(!load(dir.path()).setup_done);
    }

    #[test]
    fn saved_settings_come_back() {
        let dir = std::env::temp_dir().join(format!("cerberus-settings-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let chosen = Settings {
            recomps_dir: Some(PathBuf::from(r"D:\Recomps")),
            rom_dirs: vec![PathBuf::from(r"E:\ROMs")],
            cloud_dir: None,
            sources: vec!["https://example.com/games.json".into()],
            ..Settings::default()
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
