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
    /// Duration of the last observed session. Older libraries only have the total.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_session_secs: Option<u64>,
    /// Extra command-line options the player added, passed after the launcher's own.
    #[serde(default)]
    pub launch_args: String,
    /// Optional Windows child executable used as the session clock.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracking_exe: Option<String>,
    /// The player's copy of the original game (ROM or disc image) a recomp is set up with.
    #[serde(default)]
    pub game_copy: Option<String>,
    /// The release was an installer that hasn't run yet: the first Play runs it,
    /// and `exe` points at the installer until then.
    #[serde(default)]
    pub setup_pending: bool,
    /// What the main button does now. Worked out by the backend each time the
    /// library is listed, so every screen offers the same action.
    #[serde(default)]
    pub next_step: NextStep,
    /// The game's command-line settings the player picked: setting id -> choice.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub setting_picks: std::collections::BTreeMap<String, usize>,
    /// A recomp installed outside Cerberus and linked to the catalog: its files
    /// are the player's, so Cerberus never updates or deletes them.
    #[serde(default)]
    pub external: bool,
    /// The game's logo, when the player picked one; shown in place of its name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo_path: Option<String>,
    /// A picture filter applied while the game runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look: Option<Look>,
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

/// A game's own settings, as its Ajustes tab shows them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GameSettings {
    pub settings: Vec<GameSetting>,
    /// The settings files they are written to.
    pub files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameSetting {
    pub id: String,
    pub label: String,
    pub help: String,
    /// The heading it goes under; empty for none.
    pub group: String,
    pub advanced: bool,
    pub choices: Vec<String>,
    pub current: usize,
    /// False while another setting makes this one irrelevant (shown greyed out).
    pub applies: bool,
    /// A number on a slider instead of choices.
    #[serde(default)]
    pub slider: Option<SettingSlider>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingSlider {
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub value: f64,
    /// The value as the game writes it ("0.50", "80").
    pub text: String,
}

/// A picture filter for a game: which one (`sharp`, `vivid`, `crt`, `film`) and how strong, 0-100.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Look {
    pub id: String,
    pub strength: u8,
}

/// Whether a game can take a look, and the one it has.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LookStatus {
    /// Why it can't, in words for the player; `None` when it can.
    pub unsupported: Option<String>,
    pub current: Option<Look>,
}

/// Facts about the original game, from the catalog (IGDB).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OriginalGame {
    #[serde(default)]
    pub released: String,
    #[serde(default)]
    pub developer: String,
    #[serde(default)]
    pub publisher: String,
    #[serde(default)]
    pub series: String,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub modes: Vec<String>,
    #[serde(default)]
    pub platforms: Vec<String>,
    /// 0-100.
    #[serde(default)]
    pub rating: Option<f64>,
    #[serde(default)]
    pub summary: String,
}

/// A game on SteamGridDB.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtGame {
    pub id: u64,
    pub name: String,
    pub year: Option<u32>,
}

/// A picture on SteamGridDB: a small one to choose from, the full one to keep.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtPic {
    pub url: String,
    pub thumb: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ArtOptions {
    pub covers: Vec<ArtPic>,
    pub heroes: Vec<ArtPic>,
    pub logos: Vec<ArtPic>,
}

/// What a look through one of the player's folders found.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AdoptScan {
    pub folder: String,
    pub found: Vec<AdoptFound>,
}

/// A recomp found among the player's own programs, to bring into the library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdoptFound {
    pub recomp_id: String,
    pub name: String,
    pub game: String,
    pub console: String,
    pub cover: Option<String>,
    pub program: String,
    /// The program has the exact name the catalog gives; otherwise it only looks alike.
    pub sure: bool,
}

/// The one thing that stands between the player and the game, if anything.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NextStep {
    #[default]
    Play,
    /// A release that ships an installer: it runs on the first Play.
    RunInstaller,
    /// The recomp needs the player's copy of the game. `found` is one the ROM
    /// folders have, used with one click; otherwise the player picks a file.
    ChooseCopy { found: Option<String> },
    /// The program to open is gone (moved, deleted, or an installer put it
    /// somewhere unknown); the player points at it.
    Locate { reason: String },
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

/// One step the launcher took for a game (a session, a backup, an install), kept
/// on disk so a failure can still be looked up after the notice is gone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrailEntry {
    /// Unix seconds.
    pub at: u64,
    pub game_id: String,
    /// What was being done, as the player reads it: "Backup", "Partida"...
    pub step: String,
    pub ok: bool,
    pub message: String,
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
    pub install_free_bytes: Option<u64>,
    pub download_free_bytes: Option<u64>,
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
    #[serde(default)]
    pub pending_uploads: usize,
    pub sources: Vec<SourceView>,
    /// False until the first-run setup is finished.
    #[serde(default)]
    pub setup_done: bool,
    #[serde(default)]
    pub start_in_tv: bool,
    /// The player's own SteamGridDB key, for picking art.
    #[serde(default)]
    pub steamgriddb_key: Option<String>,
    /// "home", "library" or "discover".
    #[serde(default)]
    pub start_page: String,
    /// The linked RetroAchievements username; the key never comes back to the window.
    #[serde(default)]
    pub ra_user: Option<String>,
    /// Files found in the ROM folders, for the Clássicos view.
    #[serde(default)]
    pub roms: Vec<RomView>,
}

/// A game file found in the ROM folders.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RomView {
    pub path: String,
    pub title: String,
    /// "n64", "3ds", "ps2"...; empty for a disc image of an unknown console.
    pub system: String,
    pub region: String,
}

/// A linked RetroAchievements account, as the settings and the library show it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RaProfile {
    pub user: String,
    pub picture: Option<String>,
    /// Hardcore points.
    pub points: u64,
    pub softcore_points: u64,
    /// Points weighted by rarity.
    pub true_points: u64,
    pub member_since: String,
    pub playing_now: String,
    pub url: String,
    pub games: Vec<RaGame>,
}

/// A game the player played lately, with their progress.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RaGame {
    pub id: u64,
    pub title: String,
    pub console: String,
    pub icon: Option<String>,
    pub achieved: u64,
    pub total: u64,
    pub last_played: String,
}

/// A catalog source the player added.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceView {
    pub url: String,
    pub name: String,
    pub games: usize,
    pub hydra: bool,
    pub synced_at: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceListing {
    pub title: String,
    pub file_size: Option<String>,
    pub upload_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourcePage {
    pub items: Vec<SourceListing>,
    pub total: usize,
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
    Recomp {
        recomp_id: String,
        version: String,
        exe: String,
        /// Identifies the last committed file replacement, including a reinstall of the same version.
        #[serde(default)]
        generation: u64,
    },
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
    /// The game's logo from the catalog, shown in place of its name.
    #[serde(default)]
    pub logo: Option<String>,
    /// The recomp's kind of game: "action", "racing"...
    #[serde(default)]
    pub genre: String,
    /// What is known about the original game.
    #[serde(default)]
    pub original: Option<OriginalGame>,
    /// When the project put out its first release, and how many it has.
    #[serde(default)]
    pub first_release: String,
    #[serde(default)]
    pub releases: u64,
    /// PCs that played it lately, as the catalog counts them.
    #[serde(default)]
    pub players: u64,
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
    /// How many of the game's own settings the launcher can change.
    #[serde(default)]
    pub settings_count: usize,
    /// What the player should know before the first start, from the catalog (in English).
    #[serde(default)]
    pub player_note: String,
    /// "local", "online" or "both"; empty for single player.
    #[serde(default)]
    pub multiplayer: String,
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
