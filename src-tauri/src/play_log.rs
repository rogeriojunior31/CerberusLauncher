//! Every play session, one per line in `sessions.jsonl`: when it ended and how
//! long it lasted. The game page draws the last two weeks from it, and counts
//! sessions, the longest one, this week and this month.
//!
//! The first time it's read, it is filled from the sessions the history
//! (`trail.jsonl`) already described in words, so the chart isn't empty.

#[cfg(test)]
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cerberus_shared::{PlaySession, TrailEntry};

static WRITE: Mutex<()> = Mutex::new(());

fn file(data_dir: &Path) -> PathBuf {
    data_dir.join("sessions.jsonl")
}

fn open_file(data_dir: &Path) -> PathBuf {
    data_dir.join("sessions-open.json")
}

/// A session still going: written down every minute, so Cerberus closing in
/// the middle of it (an update, Quit) doesn't leave it out of the log.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Default)]
struct Open {
    /// Unix seconds of the last minute written.
    at: u64,
    seconds: u64,
}

/// A game's sessions under another id (a classic whose file moved).
pub fn rename(data_dir: &Path, old: &str, new: &str) {
    let _guard = WRITE.lock().unwrap_or_else(|p| p.into_inner());
    let all: Vec<PlaySession> = crate::state_file::json_lines(&file(data_dir));
    if !all.iter().any(|s| s.game_id == old) {
        return;
    }
    let mut text = String::new();
    for mut s in all {
        if s.game_id == old {
            s.game_id = new.to_string();
        }
        if let Ok(line) = serde_json::to_string(&s) {
            text.push_str(&line);
            text.push('\n');
        }
    }
    let _ = crate::state_file::write_bytes(&file(data_dir), text.as_bytes());
}

/// Notes how long a session that is still going has lasted.
pub fn still_going(data_dir: &Path, game_id: &str, seconds: u64, now: u64) {
    let _ = crate::state_file::update(&open_file(data_dir), |open: &mut std::collections::HashMap<String, Open>| {
        open.insert(game_id.to_string(), Open { at: now, seconds });
    });
}

/// The session ended normally: no longer open.
pub fn ended(data_dir: &Path, game_id: &str) {
    let _ = crate::state_file::update(&open_file(data_dir), |open: &mut std::collections::HashMap<String, Open>| {
        open.remove(game_id);
    });
}

/// At start: sessions Cerberus closed in the middle of go in the log with the
/// time written down for them.
pub fn close_left_open(data_dir: &Path) {
    let left: std::collections::HashMap<String, Open> = crate::state_file::load_or_default(&open_file(data_dir));
    if left.is_empty() {
        return;
    }
    for (game_id, open) in &left {
        record(data_dir, game_id, open.seconds, open.at);
    }
    let _ = crate::state_file::update(&open_file(data_dir), |open: &mut std::collections::HashMap<String, Open>| {
        open.retain(|id, _| !left.contains_key(id));
    });
}

/// Adds a finished session. Sessions under 30 seconds (a launcher blinking) aren't kept.
pub fn record(data_dir: &Path, game_id: &str, seconds: u64, ended: u64) {
    if seconds < 30 {
        return;
    }
    let _guard = WRITE.lock().unwrap_or_else(|p| p.into_inner());
    fill_from_trail(data_dir);
    let line = PlaySession { game_id: game_id.to_string(), ended, seconds };
    let Ok(text) = serde_json::to_string(&line) else { return };
    if let Err(e) = crate::state_file::append_line(&file(data_dir), &text) {
        eprintln!("partidas: {e}");
    }
    trim_if_big(&file(data_dir), TRIM_ABOVE, PER_GAME);
}

/// The log is read whole for a game's page: past this size it's trimmed.
const TRIM_ABOVE: u64 = 1 << 20;
/// Sessions kept per game when trimming (the chart shows two weeks).
const PER_GAME: usize = 1000;

/// Keeps each game's newest `per_game` sessions once the file passes `above` bytes.
fn trim_if_big(file: &Path, above: u64, per_game: usize) {
    if std::fs::metadata(file).map(|m| m.len()).unwrap_or(0) <= above {
        return;
    }
    let all: Vec<PlaySession> = crate::state_file::json_lines(file);
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    let mut keep = vec![false; all.len()];
    for (i, s) in all.iter().enumerate().rev() {
        let n = seen.entry(s.game_id.as_str()).or_default();
        if *n < per_game {
            *n += 1;
            keep[i] = true;
        }
    }
    let mut text = String::new();
    for (s, kept) in all.iter().zip(keep) {
        if let (true, Ok(line)) = (kept, serde_json::to_string(s)) {
            text.push_str(&line);
            text.push('\n');
        }
    }
    let _ = crate::state_file::write_bytes(file, text.as_bytes());
}

/// A game's sessions, oldest first.
pub fn of_game(data_dir: &Path, game_id: &str) -> Vec<PlaySession> {
    let _guard = WRITE.lock().unwrap_or_else(|p| p.into_inner());
    fill_from_trail(data_dir);
    crate::state_file::json_lines::<PlaySession>(&file(data_dir)).into_iter().filter(|s| s.game_id == game_id).collect()
}

/// "2 min jogados", "1 h 5 min jogados", "45 s jogados" as seconds.
pub fn seconds_in(text: &str) -> Option<u64> {
    // "Menos de 1 min jogado" is under a minute: too short to keep.
    if text.starts_with("Menos") {
        return None;
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut total = 0;
    let mut found = false;
    for pair in words.windows(2) {
        let Ok(n) = pair[0].parse::<u64>() else { continue };
        let unit = match pair[1] {
            "h" => 3600,
            "min" => 60,
            "s" => 1,
            _ => continue,
        };
        total += n * unit;
        found = true;
    }
    found.then_some(total)
}

/// Once: the sessions the history already wrote down in words.
fn fill_from_trail(data_dir: &Path) {
    let target = file(data_dir);
    if target.exists() {
        return;
    }
    let mut text = String::new();
    {
        for entry in crate::state_file::json_lines::<TrailEntry>(&data_dir.join("trail.jsonl")) {
            if entry.step != "Partida" || !entry.ok {
                continue;
            }
            let Some(seconds) = seconds_in(&entry.message).filter(|s| *s >= 30) else { continue };
            let s = PlaySession { game_id: entry.game_id, ended: entry.at, seconds };
            text.push_str(&serde_json::to_string(&s).unwrap_or_default());
            text.push('\n');
        }
    }
    let _ = crate::state_file::write_bytes(&target, text.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_big_log_keeps_each_games_newest_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("sessions.jsonl");
        let mut text = String::new();
        for i in 0..10u64 {
            for game in ["a", "b"] {
                text.push_str(&serde_json::to_string(&PlaySession { game_id: game.into(), ended: i, seconds: 60 }).unwrap());
                text.push('\n');
            }
        }
        fs::write(&log, &text).unwrap();
        trim_if_big(&log, 1 << 20, 3);
        assert_eq!(crate::state_file::json_lines::<PlaySession>(&log).len(), 20, "small: left alone");
        trim_if_big(&log, 10, 3);
        let left: Vec<PlaySession> = crate::state_file::json_lines(&log);
        assert_eq!(left.len(), 6);
        assert!(left.iter().all(|s| s.ended >= 7), "the newest three of each");
    }

    #[test]
    fn a_session_cerberus_closed_in_the_middle_of_still_reaches_the_log() {
        let dir = tempfile::tempdir().unwrap();
        still_going(dir.path(), "steam:1", 50 * 60, 1000);
        // Cerberus closed here; the next start:
        close_left_open(dir.path());
        let sessions = of_game(dir.path(), "steam:1");
        assert_eq!(sessions.iter().map(|s| s.seconds).sum::<u64>(), 50 * 60);
        // Not twice.
        close_left_open(dir.path());
        assert_eq!(of_game(dir.path(), "steam:1").len(), 1);
        // A session that ends normally leaves nothing open.
        still_going(dir.path(), "steam:2", 120, 2000);
        ended(dir.path(), "steam:2");
        close_left_open(dir.path());
        assert!(of_game(dir.path(), "steam:2").is_empty());
    }

    #[test]
    fn durations_in_words_are_read() {
        assert_eq!(seconds_in("2 min jogados"), Some(120));
        assert_eq!(seconds_in("1 h 5 min jogados"), Some(3900));
        assert_eq!(seconds_in("45 s jogados"), Some(45));
        assert_eq!(seconds_in("Menos de 1 min jogado"), None);
    }

    #[test]
    fn the_history_fills_the_log_once_and_new_sessions_add_up() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("trail.jsonl"),
            "{\"at\":100,\"game_id\":\"a\",\"step\":\"Partida\",\"ok\":true,\"message\":\"2 min jogados\"}\n\
             {\"at\":200,\"game_id\":\"a\",\"step\":\"Partida\",\"ok\":true,\"message\":\"10 s jogados\"}\n\
             {\"at\":300,\"game_id\":\"b\",\"step\":\"Backup\",\"ok\":true,\"message\":\"3 min\"}\n",
        )
        .unwrap();
        record(dir.path(), "a", 600, 400);
        record(dir.path(), "a", 5, 500);
        let a = of_game(dir.path(), "a");
        assert_eq!(a.iter().map(|s| (s.ended, s.seconds)).collect::<Vec<_>>(), [(100, 120), (400, 600)]);
        assert!(of_game(dir.path(), "b").is_empty());
    }
}
