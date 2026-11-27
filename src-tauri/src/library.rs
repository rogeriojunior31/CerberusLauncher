//! Games the user added by hand, saved as JSON in the app's data folder.

mod recovery;

use cerberus_shared::tr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cerberus_shared::{Game, Source};

pub struct LocalLibrary {
    file: PathBuf,
    games: Mutex<Option<Vec<Game>>>,
    /// Every notice from opening the library (both a recovered file and a stuck update).
    recovery_notice: Mutex<Vec<String>>,
}

impl LocalLibrary {
    pub fn load(dir: &Path) -> Self {
        Self { file: dir.join("library.json"), games: Mutex::new(None), recovery_notice: Mutex::new(Vec::new()) }
    }

    // A failed read stays retryable. Every operation loads a valid starting point before it can write.
    fn access<T>(&self, action: impl FnOnce(&mut Vec<Game>) -> Result<T, String>) -> Result<T, String> {
        let mut cache = self.games.lock().unwrap_or_else(|p| p.into_inner());
        let games = match &mut *cache {
            Some(games) => games,
            None => {
                let (games, notice) = recovery::read(&self.file)?;
                if let Some(text) = notice {
                    self.recovery_notice.lock().unwrap_or_else(|p| p.into_inner()).push(text);
                }
                // One game's interrupted update that can't be put right yet (a
                // file held by the antivirus) mustn't keep the whole library from
                // opening: it is said, and tried again on the next start.
                let stuck: Vec<String> = games
                    .iter()
                    .filter_map(|game| crate::recomp::update::recover(game).err().map(|e| format!("{}: {}", game.name, e)))
                    .collect();
                if !stuck.is_empty() {
                    self.recovery_notice.lock().unwrap_or_else(|p| p.into_inner()).push(tr!("Uma atualização interrompida não pôde ser desfeita agora; o Cerberus tenta de novo na próxima vez que abrir. {}", stuck.join(" · ")));
                }
                cache.insert(games)
            }
        };
        action(games)
    }

    pub fn recovery_notice(&self) -> Option<String> {
        let notices = self.recovery_notice.lock().unwrap_or_else(|p| p.into_inner());
        (!notices.is_empty()).then(|| notices.join(" "))
    }

    pub fn games(&self) -> Result<Vec<Game>, String> {
        self.access(|games| Ok(games.clone()))
    }

    pub fn get(&self, id: &str) -> Result<Option<Game>, String> {
        self.access(|games| {
            if let Some(found) = games.iter().find(|game| game.id == id) {
                return Ok(Some(found.clone()));
            }
            Ok(part_in(games, id).map(|(release, part)| crate::recomp::part_view(&games[release], &part)))
        })
    }

    /// The library as players see it: a release that carries several games
    /// lists each as a game of its own, after the release (marked with their ids).
    pub fn games_with_parts(&self) -> Result<Vec<Game>, String> {
        let games = self.games()?;
        let mut all = Vec::with_capacity(games.len());
        for mut game in games {
            let parts = crate::recomp::parts_of(&game);
            game.part_ids = parts.iter().map(|p| crate::recomp::part_id(&game.id, &p.id)).collect();
            let views: Vec<Game> = parts.iter().map(|p| crate::recomp::part_view(&game, p)).collect();
            all.push(game);
            all.extend(views);
        }
        Ok(all)
    }

    pub fn add(&self, mut game: Game) -> Result<(), String> {
        self.access(|games| {
            if games.iter().any(|known| known.id == game.id) {
                return Ok(());
            }
            // Played before and removed: its time comes back.
            let kept: std::collections::BTreeMap<String, (u64, Option<u64>)> = crate::state_file::load_or_default(&self.kept_time());
            if let Some((seconds, last)) = kept.get(&game.id) {
                game.playtime_secs = game.playtime_secs.max(*seconds);
                game.last_played = game.last_played.max(*last);
            }
            let mut next = games.clone();
            next.push(game);
            self.commit(games, next)
        })
    }

    /// Where the time of removed games waits, for when they come back.
    fn kept_time(&self) -> PathBuf {
        self.file.with_file_name("played-kept.json")
    }

    pub fn update(&self, id: &str, change: impl FnOnce(&mut Game)) -> Result<Game, String> {
        self.access(|games| {
            let mut next = games.clone();
            if let Some(game) = next.iter_mut().find(|game| game.id == id) {
                change(game);
                let updated = game.clone();
                self.commit(games, next)?;
                return Ok(updated);
            }
            // One game of a release: its play time is its own, the copy and setup the release's.
            let (index, part) = part_in(games, id).ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
            let mut view = crate::recomp::part_view(&next[index], &part);
            change(&mut view);
            let release = &mut next[index];
            release.parts.insert(
                part.id.clone(),
                cerberus_shared::PartState { playtime_secs: view.playtime_secs, last_played: view.last_played, last_session_secs: view.last_session_secs, launch_args: view.launch_args.clone() },
            );
            release.game_copy = view.game_copy.clone();
            release.copy_verified = view.copy_verified.clone();
            release.inputs = view.inputs.clone();
            release.setting_picks = view.setting_picks.clone();
            release.look = view.look.clone();
            let updated = crate::recomp::part_view(release, &part);
            self.commit(games, next)?;
            Ok(updated)
        })
    }

    pub fn record_session(&self, id: &str, credited: u64, elapsed: u64, closed_at: Option<u64>) -> Result<Game, String> {
        self.update(id, |game| {
            game.playtime_secs = game.playtime_secs.saturating_sub(credited).saturating_add(elapsed);
            if let Some(end) = closed_at {
                game.last_played = Some(end);
                game.last_session_secs = Some(elapsed);
            }
        })
    }

    pub fn remove(&self, id: &str) -> Result<Option<Game>, String> {
        self.access(|games| {
            // Removing one game of a release removes the release: its files are one.
            let index = games.iter().position(|game| game.id == id).or_else(|| part_in(games, id).map(|(i, _)| i));
            let Some(index) = index else { return Ok(None) };
            let mut next = games.clone();
            let removed = next.remove(index);
            // The time played stays, for a reinstall (saves stay too).
            if removed.playtime_secs > 0 || removed.last_played.is_some() {
                let (id, seconds, last) = (removed.id.clone(), removed.playtime_secs, removed.last_played);
                crate::state_file::update(&self.kept_time(), |kept: &mut std::collections::BTreeMap<String, (u64, Option<u64>)>| {
                    let entry = kept.entry(id).or_insert((0, None));
                    entry.0 = entry.0.max(seconds);
                    entry.1 = entry.1.max(last);
                })?;
            }
            self.commit(games, next)?;
            Ok(Some(removed))
        })
    }

    fn commit(&self, current: &mut Vec<Game>, next: Vec<Game>) -> Result<(), String> {
        save(&self.file, &next)?;
        *current = next;
        Ok(())
    }
}

/// The release a part's id points into, and the part: `recomp:cogbox--gow2`.
fn part_in(games: &[Game], id: &str) -> Option<(usize, crate::recomp::catalog::CatalogPart)> {
    let (release, part) = id.rsplit_once("--")?;
    let index = games.iter().position(|g| g.id == release)?;
    let found = crate::recomp::parts_of(&games[index]).into_iter().find(|p| p.id == part)?;
    Some((index, found))
}

/// A game with nothing filled in yet, for the stores' importers to complete.
pub fn blank_game() -> Game {
    let mut game = game_from_exe(Path::new(""));
    game.id = String::new();
    game.name = String::new();
    game.install_dir = String::new();
    game
}

pub fn game_from_exe(exe: &Path) -> Game {
    let install_dir = exe.parent().unwrap_or(exe).to_string_lossy().into_owned();
    let exe_text = exe.to_string_lossy().into_owned();
    Game {
        id: format!("local:{exe_text}"),
        name: name_from_stem(exe),
        source: Source::Local { exe: exe_text },
        install_dir,
        cover_path: None,
        cover_url: None,
        hero_path: None,
        hero_url: None,
        playtime_secs: 0,
        last_played: None,
        last_session_secs: None,
        launch_args: String::new(),
        tracking_exe: None,
        game_copy: None,
        copy_verified: None,
        inputs: Default::default(),
        setup_pending: false,
        next_step: Default::default(),
        setting_picks: Default::default(),
        external: false,
        logo_path: None,
        look: None,
        favorite: false,
        hidden: false,
        collections: Vec::new(),
        status: None,
        rating: 0,
        notes: String::new(),
        updates_paused: false,
        part_of: None,
        part_ids: Vec::new(),
        parts: Default::default(),
    }
}

/// Names worth looking up for this executable, best guess first: the cleaned-up
/// file name, then the folder it sits in (unless that folder is a generic one).
pub fn name_candidates(exe: &Path) -> Vec<String> {
    let mut names = vec![name_from_stem(exe)];
    let folders = exe.ancestors().skip(1).filter_map(|dir| dir.file_name());
    for folder in folders {
        let folder = folder.to_string_lossy();
        if GENERIC_FOLDERS.contains(&folder.to_lowercase().as_str()) {
            continue;
        }
        let name = readable(&folder);
        if !names.contains(&name) {
            names.push(name);
        }
        break;
    }
    names
}

/// Folders that say where a build lives, not which game it is.
const GENERIC_FOLDERS: &[&str] = &[
    "bin", "binaries", "win64", "win32", "x64", "x86", "build", "release", "game", "games",
    "common", "downloads", "desktop", "program files", "program files (x86)", "engine",
];

/// Tails engines add to the executable name, e.g. "Game-Win64-Shipping.exe".
const BUILD_SUFFIXES: &[&str] = &["-win64-shipping", "-wingdk-shipping", "-shipping", "_x64", "-x64", "_win64", "-win64"];

fn name_from_stem(exe: &Path) -> String {
    let stem = exe.file_stem().unwrap_or_default().to_string_lossy();
    let lower = stem.to_lowercase();
    let suffix = BUILD_SUFFIXES.iter().find(|suffix| lower.ends_with(*suffix));
    let trimmed = match suffix {
        Some(suffix) => &stem[..stem.len() - suffix.len()],
        None => &stem,
    };
    readable(trimmed)
}

/// "DarkSoulsIII" -> "Dark Souls III", "hollow_knight" -> "Hollow Knight".
fn readable(raw: &str) -> String {
    let mut spaced = String::new();
    let mut previous: Option<char> = None;
    for c in raw.chars() {
        let separator = matches!(c, '_' | '-' | '.');
        let starts_word = c.is_uppercase() && previous.is_some_and(char::is_lowercase);
        if separator {
            spaced.push(' ');
        } else {
            if starts_word {
                spaced.push(' ');
            }
            spaced.push(c);
        }
        previous = Some(c);
    }
    let words = spaced.split_whitespace().map(capitalize);
    words.collect::<Vec<_>>().join(" ")
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    first.to_uppercase().chain(chars).collect()
}

fn save(file: &Path, games: &[Game]) -> Result<(), String> {
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(games).map_err(|e| e.to_string())?;
    // Write a temp file and rename it over the old one, so a crash mid-write
    // cannot leave a half-written library behind.
    // Flushed to the disk before the rename: a power cut can't leave an empty library.
    crate::state_file::write_bytes_through(file, &file.with_extension("json.tmp"), json.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_game_removed_and_added_again_keeps_its_time() {
        let dir = tempfile::tempdir().unwrap();
        let library = LocalLibrary::load(dir.path());
        let mut game = blank_game();
        game.id = "recomp:zelda".into();
        game.name = "Zelda".into();
        game.playtime_secs = 36_000;
        game.last_played = Some(1_700_000_000);
        library.add(game.clone()).unwrap();
        library.remove(&game.id).unwrap();
        let fresh = Game { playtime_secs: 0, last_played: None, ..game.clone() };
        library.add(fresh).unwrap();
        let back = library.get(&game.id).unwrap().unwrap();
        assert_eq!((back.playtime_secs, back.last_played), (36_000, Some(1_700_000_000)));
    }

    fn empty_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cerberus-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn celeste() -> Game {
        game_from_exe(Path::new(r"C:\Games\Celeste\Celeste.exe"))
    }

    #[test]
    fn each_game_of_a_release_keeps_its_own_play_time() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("srb2");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("srb2win.exe"), b"MZ").unwrap();
        fs::write(dir.join("recomplauncher.json"), r#"{"format":1,"start":{"program":["srb2win.exe"]},
            "games":[{"id":"one","name":"Game One","args":["--game=1"]},{"id":"two","name":"Game Two"}]}"#).unwrap();
        let mut release = game_from_exe(&dir.join("srb2win.exe"));
        // Any recomp the built-in catalog knows; its own file names the games.
        release.source = Source::Recomp { recomp_id: "zelda64recomp".into(), version: "1".into(), exe: dir.join("srb2win.exe").to_string_lossy().into(), generation: 0 };
        release.id = "recomp:zelda64recomp".into();
        release.install_dir = dir.to_string_lossy().into();
        let library = LocalLibrary::load(&root.path().join("data"));
        library.add(release.clone()).unwrap();
        let all = library.games_with_parts().unwrap();
        let ids: Vec<&str> = all.iter().map(|g| g.id.as_str()).collect();
        let one = crate::recomp::part_id(&release.id, "one");
        assert_eq!(ids, [release.id.as_str(), one.as_str(), &crate::recomp::part_id(&release.id, "two")]);
        assert_eq!(all[0].part_ids.len(), 2);
        library.record_session(&one, 0, 90, Some(500)).unwrap();
        let part = library.get(&one).unwrap().unwrap();
        assert_eq!((part.name.as_str(), part.playtime_secs, part.last_played), ("Game One", 90, Some(500)));
        let release_now = library.get(&release.id).unwrap().unwrap();
        assert_eq!(release_now.playtime_secs, 0, "the release's own time is apart");
        // The copy picked from a game's page is the release's.
        library.update(&one, |g| g.game_copy = Some("D:/disc.iso".into())).unwrap();
        assert_eq!(library.get(&release.id).unwrap().unwrap().game_copy.as_deref(), Some("D:/disc.iso"));
        assert_eq!(crate::recomp::launch_plan(&library.get(&one).unwrap().unwrap()).args.first().map(String::as_str), Some("--game=1"));
        library.remove(&one).unwrap();
        assert!(library.games().unwrap().is_empty(), "removing a game of a release removes the release");
    }

    #[test]
    fn latest_session_and_total_survive_reopening_an_older_library() {
        let dir = tempfile::tempdir().unwrap();
        let mut old = celeste();
        old.playtime_secs = 1336;
        let json = serde_json::to_string(&[&old]).unwrap();
        assert!(!json.contains("last_session_secs"));
        fs::write(dir.path().join("library.json"), json).unwrap();
        let library = LocalLibrary::load(dir.path());
        assert_eq!(library.get(&old.id).unwrap().unwrap().last_session_secs, None);
        library.record_session(&old.id, 0, 40, None).unwrap();
        let reopened = LocalLibrary::load(dir.path()).get(&old.id).unwrap().unwrap();
        assert_eq!((reopened.playtime_secs, reopened.last_session_secs), (1376, None));
        library.record_session(&old.id, 40, 62, Some(100)).unwrap();
        library.record_session(&old.id, 0, 17, Some(200)).unwrap();
        let game = LocalLibrary::load(dir.path()).get(&old.id).unwrap().unwrap();
        assert_eq!(game.playtime_secs, 1415);
        assert_eq!(game.last_session_secs, Some(17));
        assert_eq!(game.last_played, Some(200));
    }

    #[test]
    fn added_games_survive_a_reload() {
        let dir = empty_dir("reload");
        LocalLibrary::load(&dir).add(celeste()).unwrap();
        assert_eq!(LocalLibrary::load(&dir).games().unwrap(), vec![celeste()]);
    }

    #[test]
    fn removed_games_stay_removed() {
        let dir = empty_dir("remove");
        let library = LocalLibrary::load(&dir);
        library.add(celeste()).unwrap();
        library.remove(&celeste().id).unwrap();
        assert!(LocalLibrary::load(&dir).games().unwrap().is_empty());
    }

    #[test]
    fn renames_are_saved() {
        let dir = empty_dir("rename");
        let library = LocalLibrary::load(&dir);
        library.add(celeste()).unwrap();
        library.update(&celeste().id, |game| game.name = "Celeste 64".into()).unwrap();
        assert_eq!(LocalLibrary::load(&dir).games().unwrap()[0].name, "Celeste 64");
    }

    #[test]
    fn failed_writes_keep_the_last_committed_library_and_can_be_retried() {
        let dir = empty_dir("write-failure");
        let library = LocalLibrary::load(&dir);
        let original = celeste();
        let other = game_from_exe(Path::new("OtherGame.exe"));
        library.add(original.clone()).unwrap();

        // A directory where save needs to create its temporary file fails on every OS.
        let blocker = dir.join("library.json.tmp");
        fs::create_dir(&blocker).unwrap();
        assert!(library.add(other.clone()).is_err());
        assert!(library.get(&other.id).unwrap().is_none());
        assert!(library.update(&original.id, |game| game.name = "Renamed".into()).is_err());
        assert_eq!(library.get(&original.id).unwrap(), Some(original.clone()));
        assert!(library.remove(&original.id).is_err());
        assert_eq!(library.games().unwrap(), vec![original.clone()]);
        assert_eq!(LocalLibrary::load(&dir).games().unwrap(), library.games().unwrap());

        fs::remove_dir(blocker).unwrap();
        library.add(other.clone()).unwrap();
        library.update(&other.id, |game| game.name = "Renamed".into()).unwrap();
        assert_eq!(library.remove(&original.id).unwrap(), Some(original));
        let reloaded = LocalLibrary::load(&dir);
        assert_eq!(reloaded.games().unwrap(), library.games().unwrap());
        assert_eq!(reloaded.get(&other.id).unwrap().unwrap().name, "Renamed");
    }

    #[test]
    fn recovery_keeps_valid_neighbors_and_preserves_the_original_bytes() {
        let dir = empty_dir("recover-neighbors");
        fs::create_dir_all(&dir).unwrap();
        let first = celeste();
        let second = game_from_exe(Path::new("Other.exe"));
        let mut legacy = serde_json::to_value(&first).unwrap();
        legacy.as_object_mut().unwrap().remove("setup_pending");
        let bytes = serde_json::to_vec_pretty(&serde_json::json!([legacy, {"id": 99}, second])).unwrap();
        fs::write(dir.join("library.json"), &bytes).unwrap();
        fs::write(dir.join("library.original-1.json"), b"older backup").unwrap();
        let library = LocalLibrary::load(&dir);
        assert_eq!(library.games().unwrap(), [first, second]);
        let backup = dir.join("library.original-2.json");
        assert_eq!(fs::read(&backup).unwrap(), bytes);
        assert!(library.recovery_notice().unwrap().contains(backup.to_str().unwrap()));
        assert_eq!(fs::read(dir.join("library.original-1.json")).unwrap(), b"older backup");
        let reopened = LocalLibrary::load(&dir);
        assert_eq!(reopened.games().unwrap(), library.games().unwrap());
        assert!(reopened.recovery_notice().is_none());
    }

    #[test]
    fn truncated_last_record_recovers_only_complete_games() {
        let dir = empty_dir("recover-truncated");
        fs::create_dir_all(&dir).unwrap();
        let bytes = format!(r#"[{},{{"id":"unfinished"#, serde_json::to_string(&celeste()).unwrap());
        fs::write(dir.join("library.json"), &bytes).unwrap();
        let library = LocalLibrary::load(&dir);
        assert_eq!(library.games().unwrap(), [celeste()]);
        assert_eq!(fs::read_to_string(dir.join("library.original-1.json")).unwrap(), bytes);
    }

    #[test]
    fn unrepairable_input_blocks_writes_and_can_be_replaced_then_retried() {
        let dir = empty_dir("recover-unsupported");
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("library.json");
        let library = LocalLibrary::load(&dir);
        let syntax_error = format!("[{}, garbage]", serde_json::to_string(&celeste()).unwrap());
        for text in ["", "{", r#"[{"future":5}]"#, &syntax_error] {
            fs::write(&file, text).unwrap();
            assert!(library.add(celeste()).is_err());
            assert!(library.remove(&celeste().id).is_err());
            assert!(library.games().is_err());
            assert_eq!(fs::read_to_string(&file).unwrap(), text);
        }
        fs::write(&file, "[]").unwrap();
        library.add(celeste()).unwrap();
        assert_eq!(LocalLibrary::load(&dir).games().unwrap(), [celeste()]);
    }

    #[test]
    fn failed_recovery_is_retryable_without_overwriting_or_duplicating_backups() {
        let dir = empty_dir("recover-retry");
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("library.json");
        let original = serde_json::to_vec(&serde_json::json!([celeste(), null])).unwrap();
        fs::write(&file, &original).unwrap();
        let library = LocalLibrary::load(&dir);
        let backup = dir.join("library.original-1.json");
        fs::create_dir(&backup).unwrap();
        assert!(library.games().is_err());
        assert_eq!(fs::read(&file).unwrap(), original);
        fs::remove_dir(&backup).unwrap();
        let blocker = dir.join("library.json.tmp");
        fs::create_dir(&blocker).unwrap();
        assert!(library.add(celeste()).is_err());
        assert_eq!(fs::read(&file).unwrap(), original);
        assert_eq!(fs::read(&backup).unwrap(), original);
        fs::remove_dir(blocker).unwrap();
        assert_eq!(library.games().unwrap(), [celeste()]);
        assert!(!dir.join("library.original-2.json").exists());
    }

    #[test]
    fn missing_library_stays_empty_until_first_write() {
        let dir = empty_dir("missing-library");
        let library = LocalLibrary::load(&dir);
        assert!(library.games().unwrap().is_empty());
        assert!(!dir.exists());
    }

    #[cfg(windows)]
    #[test]
    fn locked_library_is_never_treated_as_empty() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = empty_dir("locked-library");
        LocalLibrary::load(&dir).add(celeste()).unwrap();
        let file = dir.join("library.json");
        let locked = fs::OpenOptions::new().read(true).share_mode(0).open(&file).unwrap();
        let library = LocalLibrary::load(&dir);
        assert!(library.games().is_err());
        assert!(library.add(game_from_exe(Path::new("Other.exe"))).is_err());
        drop(locked);
        assert_eq!(library.games().unwrap(), [celeste()]);
    }

    #[test]
    fn splits_camel_case_names() {
        let game = game_from_exe(Path::new(r"C:\Games\DS3\DarkSoulsIII.exe"));
        assert_eq!(game.name, "Dark Souls III");
    }

    #[test]
    fn turns_underscores_into_words() {
        let game = game_from_exe(Path::new(r"C:\Games\hollow_knight.exe"));
        assert_eq!(game.name, "Hollow Knight");
    }

    #[test]
    fn drops_unreal_build_suffixes() {
        let game = game_from_exe(Path::new(r"C:\Games\Grounded-Win64-Shipping.exe"));
        assert_eq!(game.name, "Grounded");
    }

    #[test]
    fn skips_generic_folders_when_guessing() {
        let exe = Path::new(r"D:\Satisfactory\Engine\Binaries\Win64\FactoryGame.exe");
        assert_eq!(name_candidates(exe), vec!["Factory Game", "Satisfactory"]);
    }
}
