//! What the player says about each game: where they are with it (playing,
//! beaten, want to play...), a rating from 1 to 5 stars and notes. Kept in
//! `game-notes.json` by game id, apart from the library, because Steam and Epic
//! games are read from their stores each time and this must survive that.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cerberus_shared::{Game, GameStatus};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Note {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<GameStatus>,
    /// 0 when not rated, else 1 to 5.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rating: u8,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

fn is_zero(n: &u8) -> bool {
    *n == 0
}

fn file(data: &Path) -> PathBuf {
    data.join("game-notes.json")
}

pub fn load(data: &Path) -> BTreeMap<String, Note> {
    crate::state_file::load_or_default(&file(data))
}

/// A game's note under another id (a classic whose file moved).
pub fn rename(data: &Path, old: &str, new: &str) {
    let _ = crate::state_file::update(&file(data), |all: &mut BTreeMap<String, Note>| {
        if let Some(note) = all.remove(old) {
            all.entry(new.to_string()).or_insert(note);
        }
    });
}

/// Saves one game's note; an empty one is removed.
pub fn set(data: &Path, id: &str, note: Note) -> Result<(), String> {
    let note = Note { rating: note.rating.min(5), notes: note.notes.chars().take(4000).collect(), ..note };
    crate::state_file::update(&file(data), |all: &mut BTreeMap<String, Note>| {
        if note == Note::default() {
            all.remove(id);
        } else {
            all.insert(id.to_string(), note);
        }
    })
}

/// Puts each game's note on it as listed.
pub fn apply(data: &Path, games: &mut [Game]) {
    let all = load(data);
    for game in games.iter_mut() {
        if let Some(n) = all.get(&game.id) {
            game.status = n.status;
            game.rating = n.rating;
            game.notes = n.notes.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_survive_a_restart_and_an_empty_one_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        set(dir.path(), "steam:620", Note { status: Some(GameStatus::Beaten), rating: 9, notes: "Final secreto".into() }).unwrap();
        set(dir.path(), "steam:400", Note { status: Some(GameStatus::Playing), ..Note::default() }).unwrap();
        let all = load(dir.path());
        assert_eq!(all["steam:620"].rating, 5, "ratings stop at 5 stars");
        assert_eq!(all["steam:400"].status, Some(GameStatus::Playing));
        set(dir.path(), "steam:400", Note::default()).unwrap();
        assert!(!load(dir.path()).contains_key("steam:400"));
    }
}
