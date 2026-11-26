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
