//! Games the user added by hand, saved as JSON in the app's data folder.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use cerberus_shared::{Game, Source};

pub struct LocalLibrary {
    file: PathBuf,
    games: Mutex<Vec<Game>>,
}

impl LocalLibrary {
    pub fn load(dir: &Path) -> Self {
        let file = dir.join("library.json");
        // A missing or unreadable file means an empty library; never block startup on it.
        let text = fs::read_to_string(&file).unwrap_or_default();
        let games = serde_json::from_str(&text).unwrap_or_default();
        Self { file, games: Mutex::new(games) }
    }

    pub fn games(&self) -> Vec<Game> {
        self.lock().clone()
    }

    pub fn get(&self, id: &str) -> Option<Game> {
        self.lock().iter().find(|game| game.id == id).cloned()
    }

    pub fn add(&self, game: Game) -> Result<(), String> {
        let mut games = self.lock();
        if games.iter().any(|known| known.id == game.id) {
            return Ok(());
        }
        games.push(game);
        save(&self.file, &games)
    }

    /// Applies `change` to one game, saves, and returns the game as it is now.
    pub fn update(&self, id: &str, change: impl FnOnce(&mut Game)) -> Result<Game, String> {
        let mut games = self.lock();
        let Some(game) = games.iter_mut().find(|game| game.id == id) else {
            return Err("Esse jogo não está mais na biblioteca.".into());
        };
        change(game);
        let updated = game.clone();
        save(&self.file, &games)?;
        Ok(updated)
    }

    pub fn remove(&self, id: &str) -> Result<Option<Game>, String> {
        let mut games = self.lock();
        let Some(index) = games.iter().position(|game| game.id == id) else {
            return Ok(None);
        };
        let removed = games.remove(index);
        save(&self.file, &games)?;
        Ok(Some(removed))
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Game>> {
        // A panic while holding the lock leaves the list itself intact, so keep using it.
        self.games.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
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
        launch_args: String::new(),
        game_copy: None,
        setup_pending: false,
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
    let temp = file.with_extension("json.tmp");
    fs::write(&temp, json).map_err(|e| e.to_string())?;
    fs::rename(&temp, file).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cerberus-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn celeste() -> Game {
        game_from_exe(Path::new(r"C:\Games\Celeste\Celeste.exe"))
    }

    #[test]
    fn added_games_survive_a_reload() {
        let dir = empty_dir("reload");
        LocalLibrary::load(&dir).add(celeste()).unwrap();
        assert_eq!(LocalLibrary::load(&dir).games(), vec![celeste()]);
    }

    #[test]
    fn removed_games_stay_removed() {
        let dir = empty_dir("remove");
        let library = LocalLibrary::load(&dir);
        library.add(celeste()).unwrap();
        library.remove(&celeste().id).unwrap();
        assert!(LocalLibrary::load(&dir).games().is_empty());
    }

    #[test]
    fn renames_are_saved() {
        let dir = empty_dir("rename");
        let library = LocalLibrary::load(&dir);
        library.add(celeste()).unwrap();
        library.update(&celeste().id, |game| game.name = "Celeste 64".into()).unwrap();
        assert_eq!(LocalLibrary::load(&dir).games()[0].name, "Celeste 64");
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
