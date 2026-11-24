//! Types the backend and the UI both use. Keeping them in one crate means a field
//! renamed on one side fails to compile on the other, instead of breaking at runtime.

pub mod i18n;

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
    /// The other files a recomp's `recomplauncher.json` asks for: input id -> path.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub inputs: std::collections::BTreeMap<String, String>,
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
    /// The player marked it as a favorite (kept in favorites.json, not here).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub favorite: bool,
    /// The player hid it from the library (kept in collections.json, not here).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
    /// The player's collections it is in (kept in collections.json, not here).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collections: Vec<String>,
    /// Where the player is with it (kept in game-notes.json, not here).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<GameStatus>,
    /// The player's rating, 1 to 5 stars; 0 when not rated.
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub rating: u8,
    /// The player's own notes about it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// A recomp whose updates the player paused: it stays on its version.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub updates_paused: bool,
}

fn is_zero_u8(n: &u8) -> bool {
    *n == 0
}

/// Where the player is with a game, as they mark it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GameStatus {
    Playing,
    PlanToPlay,
    Beaten,
    Completed,
    OnHold,
    Abandoned,
}

impl GameStatus {
    pub const ALL: [GameStatus; 6] = [GameStatus::Playing, GameStatus::PlanToPlay, GameStatus::Beaten, GameStatus::Completed, GameStatus::OnHold, GameStatus::Abandoned];
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
    /// The developer's `recomplauncher.json` says which files the game takes
    /// (the copy, each disc, DLC, an update) and how each is passed.
    Inputs,
}

/// A file a recomp's `recomplauncher.json` asks the player for, besides the copy.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SetupInput {
    pub id: String,
    pub label: String,
    /// disc, dlc, update or key.
    pub role: String,
    pub optional: bool,
    /// Takes a folder (an extracted game) rather than a file.
    pub folder: bool,
    /// What the player picked, when they did.
    pub chosen: Option<String>,
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
    /// A store game the account owns that this PC hasn't installed: the store installs it.
    Install,
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
    /// Likes on GameBanana, which doesn't tell downloads in its list.
    #[serde(default)]
    pub likes: u64,
}

/// A game's mods: where they come from, and the list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModList {
    /// Where the mods come from, for the player: "thunderstore.io/c/..." or "gamebanana.com".
    #[serde(default)]
    pub from: String,
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
    /// Recomp download limit in MB/s; 0 for none.
    #[serde(default)]
    pub download_limit: u32,
    #[serde(default)]
    pub slow_while_playing: bool,
    #[serde(default)]
    pub in_game: InGame,
    #[serde(default)]
    pub notices: Notices,
    /// The player's own SteamGridDB key, for picking art.
    #[serde(default)]
    pub steamgriddb_key: Option<String>,
    /// "home", "library" or "discover".
    #[serde(default)]
    pub start_page: String,
    /// recomplauncher.com's catalog is in use.
    #[serde(default)]
    pub site_catalog: bool,
    /// Closing the window keeps Cerberus in the tray.
    #[serde(default)]
    pub close_to_tray: bool,
    /// Cerberus opens with Windows.
    #[serde(default)]
    pub autostart: bool,
    /// Opened with Windows, it starts in the tray.
    #[serde(default)]
    pub start_hidden: bool,
    /// The linked RetroAchievements username; the key never comes back to the window.
    #[serde(default)]
    pub ra_user: Option<String>,
    /// The RetroAchievements password was given once: RetroArch gets connected by itself.
    #[serde(default)]
    pub ra_retroarch: bool,
    /// Files found in the ROM folders, for the Clássicos view.
    #[serde(default)]
    pub roms: Vec<RomView>,
    /// The monitor Cerberus opens on; none: where Windows puts it.
    #[serde(default)]
    pub monitor: Option<String>,
    /// The language picked in the settings ("pt-BR", "en"); none: the system's.
    #[serde(default)]
    pub language: Option<String>,
}

/// A DLSS file inside a game's folder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DlssFile {
    /// From the game's folder, as given back to swap it.
    pub path: String,
    /// "DLSS Super Resolution", "DLSS Frame Generation"...
    pub label: String,
    pub version: String,
    /// The game's own version, kept by Cerberus, when another was put in place.
    pub original: Option<String>,
}

/// What the menu over a game (Ctrl+F12) shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlayView {
    pub game: Game,
    /// How long it's been played this time.
    pub session_secs: u64,
    /// The latest pictures of it, newest first.
    pub shots: Vec<Screenshot>,
}

/// The player's Steam account, when they turned on Steam's Web API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SteamAccount {
    pub steamid: String,
    pub name: String,
    pub avatar: Option<String>,
    pub url: String,
    /// The profile itself is public.
    pub profile_public: bool,
    /// Game details (owned games, play time, achievements) are public: needed to read them.
    pub games_public: bool,
    /// Owned games, when they can be read.
    pub games: u32,
}

/// A newer Cerberus on GitHub (or this version's notes, for "What's new").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppUpdate {
    pub version: String,
    /// The release notes, as written on GitHub (Markdown).
    pub notes: String,
    pub size: u64,
    pub sha256: Option<String>,
    pub url: String,
    pub published: String,
}

/// One finished play session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaySession {
    pub game_id: String,
    /// Unix seconds when it ended.
    pub ended: u64,
    pub seconds: u64,
}

/// A piece of a store description: "h" heading, "p" paragraph, "li" list item, "img" picture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoreBlock {
    pub kind: String,
    pub text: String,
    pub src: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoreShot {
    pub thumb: String,
    pub full: String,
}

/// A trailer: an MP4 when Steam still has one, otherwise its HLS stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoreMovie {
    pub name: String,
    pub thumb: String,
    pub mp4: Option<String>,
    pub hls: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoreLanguage {
    pub name: String,
    /// Full audio in this language, not just text.
    pub audio: bool,
}

/// The original game's Wikipedia article: its opening paragraph, as plain text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WikiAbout {
    pub title: String,
    pub extract: String,
    pub url: String,
}

/// A post by a Steam game's developers, for Novidades. Plain text only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewsItem {
    pub app_id: u32,
    pub game: String,
    pub title: String,
    /// The start of the post.
    pub summary: String,
    /// The post on Steam.
    pub url: String,
    /// Unix seconds.
    pub date: u64,
}

/// A game's page on the Steam store, as Cerberus shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoreDetails {
    pub app: u32,
    pub name: String,
    pub short: String,
    pub about: Vec<StoreBlock>,
    pub developers: Vec<String>,
    pub publishers: Vec<String>,
    pub released: String,
    pub coming_soon: bool,
    pub genres: Vec<String>,
    /// Steam's categories: single-player, co-op, achievements, cloud...
    pub features: Vec<String>,
    pub metacritic: Option<u32>,
    pub recommendations: Option<u64>,
    /// "full", "partial" or empty.
    pub controller: String,
    pub languages: Vec<StoreLanguage>,
    pub minimum: Vec<(String, String)>,
    pub recommended: Vec<(String, String)>,
    pub screenshots: Vec<StoreShot>,
    pub movies: Vec<StoreMovie>,
    pub website: Option<String>,
    pub background: Option<String>,
    pub is_free: bool,
}

/// What kind of achievement notice to draw over the game.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnlockKind {
    Normal,
    /// Few players have it.
    Rare,
    /// The last one: the game is at 100%.
    Complete,
}

/// A notice over the game when an achievement unlocks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnlockNotice {
    pub title: String,
    pub name: String,
    pub description: String,
    pub icon: Option<String>,
    pub kind: UnlockKind,
}

/// Where a game's achievements come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AchievementSource {
    Steam,
    Retro,
}

impl AchievementSource {
    /// The id used in routes: "steam", "retro".
    pub fn code(self) -> &'static str {
        match self {
            AchievementSource::Steam => "steam",
            AchievementSource::Retro => "retro",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "steam" => Some(AchievementSource::Steam),
            "retro" => Some(AchievementSource::Retro),
            _ => None,
        }
    }
}

/// The highest award a game earned: beaten (normal or hardcore), completed
/// (every achievement) or mastered (every achievement in hardcore). Steam games
/// at 100% count as completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GameAward {
    BeatenSoftcore,
    BeatenHardcore,
    Completed,
    Mastered,
}

impl GameAward {
    /// RetroAchievements' `HighestAwardKind`: "beaten-softcore", "beaten-hardcore", "completed", "mastered".
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "beaten-softcore" => Some(GameAward::BeatenSoftcore),
            "beaten-hardcore" => Some(GameAward::BeatenHardcore),
            "completed" => Some(GameAward::Completed),
            "mastered" => Some(GameAward::Mastered),
            _ => None,
        }
    }

    pub fn hardcore(self) -> bool {
        matches!(self, GameAward::BeatenHardcore | GameAward::Mastered)
    }

    /// Every achievement, not just beaten.
    pub fn full(self) -> bool {
        matches!(self, GameAward::Completed | GameAward::Mastered)
    }
}

/// A game with achievements, as the hub lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AchievementGame {
    pub source: AchievementSource,
    /// Steam's app id or RetroAchievements' game id.
    pub key: String,
    pub name: String,
    /// Pictures to try in order: a tall cover (Steam: the copy on disk, then
    /// online) or the game's square icon (RetroAchievements).
    pub art: Vec<String>,
    /// "Steam", or the console for RetroAchievements.
    pub platform: String,
    pub total: u32,
    pub unlocked: u32,
    /// RetroAchievements' hardcore unlocks.
    #[serde(default)]
    pub hardcore: u32,
    /// RetroAchievements' points: earned and possible (0 when unknown).
    #[serde(default)]
    pub points: u32,
    #[serde(default)]
    pub points_total: u32,
    /// The latest unlock, in Unix seconds.
    pub last_unlock: Option<u64>,
    /// Steam's own play time and last played, from the client's files.
    pub playtime_secs: Option<u64>,
    pub last_played: Option<u64>,
    /// The highest award, and when it was earned.
    #[serde(default)]
    pub award: Option<GameAward>,
    #[serde(default)]
    pub award_at: Option<u64>,
}

impl AchievementGame {
    pub fn percent(&self) -> u32 {
        if self.total == 0 { 0 } else { self.unlocked * 100 / self.total }
    }
}

/// One achievement of a game.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Achievement {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: Option<String>,
    /// The grey picture shown while it's locked.
    pub icon_locked: Option<String>,
    /// Secret until unlocked.
    pub hidden: bool,
    pub unlocked_at: Option<u64>,
    /// RetroAchievements' hardcore unlock.
    #[serde(default)]
    pub hardcore_at: Option<u64>,
    /// RetroAchievements' points (0 for Steam).
    #[serde(default)]
    pub points: u32,
    /// How many players have it, in percent, when known.
    pub rarity: Option<f32>,
}

/// An achievement unlocked lately, for the hub's timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecentUnlock {
    pub source: AchievementSource,
    pub game_key: String,
    pub game_name: String,
    pub achievement: Achievement,
}

/// Everything the hub shows.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AchievementsHub {
    pub games: Vec<AchievementGame>,
    /// Newest first.
    pub recent: Vec<RecentUnlock>,
    /// Steam is installed and has achievement data on this PC.
    pub steam: bool,
    /// A RetroAchievements account is connected.
    pub retro: bool,
    /// RetroAchievements' points, when connected: hardcore and softcore.
    pub retro_points: u64,
    #[serde(default)]
    pub retro_softcore: u64,
    /// Why a source couldn't be read this time (offline, key refused...).
    pub problems: Vec<String>,
    /// Steam turned questions away before every game was read: ask again in a few minutes.
    #[serde(default)]
    pub retry_soon: bool,
}

/// One game's achievements, for its page in the hub.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameAchievements {
    pub game: AchievementGame,
    pub achievements: Vec<Achievement>,
}

/// A picture taken of a game (F12 while it runs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Screenshot {
    pub path: String,
    /// When, in Unix seconds.
    pub taken: u64,
}

/// A monitor plugged in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MonitorView {
    /// Windows' name for it, kept in the settings.
    pub name: String,
    /// "Monitor 2 · 2560×1440 (principal)".
    pub label: String,
    pub primary: bool,
}

/// A standalone emulator (DuckStation, PCSX2...) as Configurações shows it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StandaloneView {
    pub id: String,
    pub name: String,
    /// The consoles it plays, as the ROM scan names them ("ps2").
    pub consoles: Vec<String>,
    /// Its program, when found.
    pub exe: Option<String>,
    /// Cerberus installed it (and may remove it).
    pub managed: bool,
    /// Its BIOS (or firmware) from the player's console is in place; None when it needs none.
    #[serde(default)]
    pub bios: Option<bool>,
    /// What it needs is the PS3 firmware (installed by RPCS3), not BIOS files.
    #[serde(default)]
    pub firmware: bool,
}

/// Every standalone emulator, which one plays each console, and the folders they list games from.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StandaloneStatus {
    pub emulators: Vec<StandaloneView>,
    /// Console → "retroarch" or an emulator id: who opens its games now.
    pub play_with: Vec<(String, String)>,
    pub use_folders: bool,
    pub folders: Vec<String>,
}

/// Emulation as Configurações shows it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EmulationView {
    /// The RetroArch program in use, if any.
    pub retroarch: Option<String>,
    /// It is the one Cerberus installed.
    pub managed: bool,
    pub cores: Vec<CoreView>,
    /// The RetroAchievements user RetroArch is logged in as.
    pub ra_user: Option<String>,
    /// The player's BIOS files are in place.
    #[serde(default)]
    pub ps1_bios: bool,
    #[serde(default)]
    pub ps2_bios: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoreView {
    pub console: String,
    pub core: String,
    /// The consoles it plays, as players say them.
    pub consoles: String,
    pub installed: bool,
}

/// A game file found in the ROM folders.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RomView {
    pub path: String,
    pub title: String,
    /// "n64", "3ds", "ps2"...; empty for a disc image of an unknown console.
    pub system: String,
    pub region: String,
    /// Seconds played through Cerberus, and when last (Unix seconds).
    #[serde(default)]
    pub playtime_secs: u64,
    #[serde(default)]
    pub last_played: Option<u64>,
    /// A game on several discs: each disc's file, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub discs: Vec<String>,
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

/// What the PC has that games need (Configurações › Seu PC).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PcStatus {
    /// The Visual C++ 2015-2022 x64 runtime's version, when installed.
    pub vc_version: Option<String>,
    /// Installed and recent enough for today's recomps.
    pub vc_ok: bool,
    /// DirectX's older parts (D3DX9, XInput 1.3...) the PC lacks.
    #[serde(default)]
    pub dx_missing: Vec<String>,
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
    /// Where the project lives, for the player: "GitHub", "GitLab" or its server's name.
    #[serde(default)]
    pub host: String,
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
    /// Only a folder of extracted files will do (a disc or cartridge file won't).
    #[serde(default)]
    pub only_folder: bool,
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

/// The error an install ends with when the player pauses its download.
pub const PAUSED: &str = "Download pausado.";
/// What a cancelled install answers (in Portuguese, the text's key).
pub const CANCELLED_INSTALL: &str = "Instalação cancelada.";

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
    /// Stopped by the player with its bytes kept; installing again carries on.
    Paused { done: u64, total: u64 },
    Unpacking,
    Finished,
    Failed { message: String },
}

/// What Cerberus does while a game is open (Settings › While playing).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct InGame {
    /// Ctrl+F12 (or View+Menu on a controller) opens the menu over the game.
    pub menu: bool,
    /// F12 saves a picture of the game.
    pub screenshots: bool,
    /// A small notice over the game when an achievement unlocks.
    pub unlock_notice: bool,
    /// Where that notice shows: "br", "bl", "tr" or "tl".
    pub corner: String,
    /// Cerberus minimizes itself while a game runs and comes back when it closes.
    pub minimize: bool,
}

impl Default for InGame {
    fn default() -> Self {
        Self { menu: true, screenshots: true, unlock_notice: true, corner: "br".into(), minimize: false }
    }
}

/// Which notices Cerberus gives, and whether Windows shows them (Settings › Notifications).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Notices {
    /// A recomp or game finished installing.
    pub installed: bool,
    /// A game closed with an error.
    pub crashed: bool,
    /// A picture was saved with F12.
    pub screenshots: bool,
    /// When Cerberus isn't in front, Windows shows the notice; off: only inside Cerberus.
    pub windows: bool,
}

impl Default for Notices {
    fn default() -> Self {
        Self { installed: true, crashed: true, screenshots: true, windows: true }
    }
}
