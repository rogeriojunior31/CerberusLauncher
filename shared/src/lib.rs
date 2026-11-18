//! Types the backend and the UI both use. Keeping them in one crate means a field
//! renamed on one side fails to compile on the other, instead of breaking at runtime.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Game {
    /// Unique across sources, e.g. "steam:620".
    pub id: String,
    pub name: String,
    pub source: Source,
    pub install_dir: String,
    /// Cover already on disk; preferred because it works offline and is always there.
    pub cover_path: Option<String>,
    /// Remote fallback when there is no local cover.
    pub cover_url: Option<String>,
    /// Wide backdrop art for banners and the game's page, on disk.
    #[serde(default)]
    pub hero_path: Option<String>,
    /// Wide backdrop art, online.
    #[serde(default)]
    pub hero_url: Option<String>,
    /// Time spent playing, counted by the launcher (only for games it starts itself).
    #[serde(default)]
    pub playtime_secs: u64,
    /// Unix seconds when the game last closed.
    #[serde(default)]
    pub last_played: Option<u64>,
    /// Extra command-line options the player added, passed after the launcher's own.
    #[serde(default)]
    pub launch_args: String,
    /// The player's copy of the original game (ROM or disc image) a recomp is set up with.
    #[serde(default)]
    pub game_copy: Option<String>,
    /// The release was an installer that hasn't run yet: the first Play runs it,
    /// and `exe` points at the installer until then.
    #[serde(default)]
    pub setup_pending: bool,
}

/// How a recomp is given the player's copy of the game, as its catalog entry says.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CopyHandoff {
    /// The game asks for it in its own menu the first time.
    AsksItself,
    /// `--rom <path>` on its command line.
    Flag { flag: String },
    /// An environment variable holding the path.
    Env { name: String },
    /// The file copied into its folder, under a fixed name when the game expects one.
    /// With `extract`, the copy is an Xbox disc image (or its files) unpacked there.
    Folder {
        folder: String,
        file_name: Option<String>,
        #[serde(default)]
        extract: bool,
    },
}

impl Game {
    /// Store games start through their own launcher, so we cannot see them run.
    pub fn is_tracked(&self) -> bool {
        matches!(self.source, Source::Local { .. } | Source::Recomp { .. })
    }
}

/// A copy of a recomp's saves, kept by the launcher.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Backup {
    /// The file's name, used to ask for it back.
    pub name: String,
    /// Unix seconds.
    pub created: u64,
    pub reason: String,
    pub files: u32,
    pub bytes: u64,
}

/// A mod from Thunderstore, as the game's Mods tab shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModItem {
    /// `Owner-Name`, Thunderstore's id for it.
    pub full_name: String,
    pub name: String,
    pub owner: String,
    pub description: String,
    pub icon: String,
    pub version: String,
    pub page: String,
    pub downloads: u64,
    /// The version installed, if it is.
    pub installed: Option<String>,
}

/// A game's mods: where they come from, and the list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModList {
    pub community: String,
    pub folder: String,
    pub items: Vec<ModItem>,
}

/// What the Settings page shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingsView {
    /// Where new recomps are installed.
    pub recomps_dir: String,
    /// Whether that is the default folder.
    pub recomps_dir_is_default: bool,
    /// The app's own data folder (library, saves backups, caches).
    pub data_dir: String,
    /// Bytes taken by the copies of game art.
    pub art_cache_bytes: u64,
    pub version: String,
    /// Folders looked through for the player's ROMs and disc images.
    pub rom_dirs: Vec<String>,
    /// Game files found in them at the last look.
    pub roms_found: usize,
    /// Recomps whose game the player has.
    pub roms_matched: usize,
    /// How many recomps the catalog has, and where it comes from.
    pub catalog_count: usize,
    pub catalog_source: String,
    /// The synced folder save backups are copied to, if set.
    pub cloud_dir: Option<String>,
    pub sources: Vec<SourceView>,
}

/// A catalog source the player added.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceView {
    pub url: String,
    pub name: String,
    pub games: usize,
}

/// Sent when a game the launcher started opens or closes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionEvent {
    pub game_id: String,
    pub running: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    Steam { app_id: u32 },
    Epic { app_name: String, namespace: String, item_id: String },
    /// Any executable the user added by hand.
    Local { exe: String },
    /// A recomp installed from its developer's GitHub releases.
    Recomp { recomp_id: String, version: String, exe: String },
}

/// One recomp in Discover: what the catalog says about it, plus what we know right now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecompEntry {
    pub id: String,
    /// Which game this is; recomps sharing it are versions of the same game.
    pub title: String,
    pub name: String,
    /// The original game, e.g. "The Legend of Zelda: Majora's Mask".
    pub game: String,
    pub console: String,
    pub repo: String,
    pub icon: Option<String>,
    /// Box art of the original game.
    pub cover: Option<String>,
    /// Wide art (a screenshot of the original game) for banners and the page backdrop.
    pub hero: Option<String>,
    /// One or two sentences about the recomp.
    pub about: String,
    /// What this version brings, to tell it from the game's other versions.
    pub adds: String,
    /// What the player must bring: their own copy of the game.
    pub needs: String,
    /// The console's short id (`n64`, `x360`...), for filtering.
    pub origin: String,
    pub author: String,
    pub downloads: u64,
    pub stars: u64,
    pub tags: Vec<String>,
    pub features: Vec<String>,
    /// Steps to do by hand before playing; empty when the recomp just works.
    pub setup_steps: Vec<String>,
    /// The version the catalog recommends among the game's versions, and why.
    pub recommended: bool,
    pub why: String,
    /// When its newest release came out, `YYYY-MM-DD`.
    pub released: String,
    /// When it joined the catalog, `YYYY-MM-DD`.
    pub added: String,
    /// The player's own copy of the game, found in their ROM folders.
    pub owned_copy: Option<String>,
    /// How the recomp takes that copy; `None` when the catalog doesn't say.
    pub handoff: Option<CopyHandoff>,
    /// The copy can be a folder of extracted files, not just a file.
    pub accepts_folder: bool,
    /// The newest full release for this PC; `None` when unknown (offline, no build for this system).
    pub latest: Option<Release>,
    /// The version installed, if any.
    pub installed: Option<String>,
    /// Why `latest` is missing, in words the player can act on.
    pub problem: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Release {
    pub version: String,
    pub file: String,
    pub size: u64,
}

/// Sent from the backend while a recomp installs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstallProgress {
    pub recomp_id: String,
    pub step: InstallStep,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InstallStep {
    Downloading { done: u64, total: u64 },
    Unpacking,
    Finished,
    Failed { message: String },
}
