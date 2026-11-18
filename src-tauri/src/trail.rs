//! A short local history of what the launcher did for each game: sessions,
//! backups, cloud syncs, installs. Notices disappear; this stays, so the last
//! failure (and its cause) can be read after a restart.
//!
//! One JSON object per line in `trail.jsonl`. The file is cut back to the newest
//! `KEEP` lines once it passes `KEEP + SLACK`, so it never grows without limit.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use cerberus_shared::TrailEntry;

const KEEP: usize = 500;
const SLACK: usize = 100;

pub struct Trail {
    path: PathBuf,
    // Lines written since the file was last cut back; `None` until counted.
    written: Mutex<Option<usize>>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default()
}

fn read_all(path: &Path) -> Vec<TrailEntry> {
    let Ok(file) = fs::File::open(path) else { return Vec::new() };
    // A line cut short by a crash is skipped, not a reason to lose the rest.
    BufReader::new(file).lines().map_while(Result::ok).filter_map(|line| serde_json::from_str(&line).ok()).collect()
}

impl Trail {
    pub fn new(data_dir: &Path) -> Self {
        Self { path: data_dir.join("trail.jsonl"), written: Mutex::new(None) }
    }

    /// Records one step. Never fails the caller: the history only explains
    /// problems, so a problem writing it is printed and the step carries on.
    pub fn note(&self, game_id: &str, step: &str, outcome: &Result<String, String>) {
        let (ok, message) = match outcome {
            Ok(message) => (true, message.clone()),
            Err(message) => (false, message.clone()),
        };
        let entry = TrailEntry { at: now(), game_id: game_id.to_string(), step: step.to_string(), ok, message };
        if let Err(error) = self.append(&entry) {
            eprintln!("histórico: {error}");
        }
    }

    fn append(&self, entry: &TrailEntry) -> std::io::Result<()> {
        let mut written = self.written.lock().unwrap_or_else(|e| e.into_inner());
        let count = written.get_or_insert_with(|| read_all(&self.path).len());
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut file = OpenOptions::new().create(true).append(true).open(&self.path)?;
        writeln!(file, "{}", serde_json::to_string(entry)?)?;
        *count += 1;
        if *count > KEEP + SLACK {
            drop(file);
            let all = read_all(&self.path);
            let newest = &all[all.len().saturating_sub(KEEP)..];
            let temp = self.path.with_extension("jsonl.tmp");
            let mut text = String::new();
            for e in newest {
                text.push_str(&serde_json::to_string(e)?);
                text.push('\n');
            }
            fs::write(&temp, text)?;
            fs::rename(&temp, &self.path)?;
            *count = newest.len();
        }
        Ok(())
    }

    /// A game's steps, newest first, at most `limit`.
    pub fn for_game(&self, game_id: &str, limit: usize) -> Vec<TrailEntry> {
        let _guard = self.written.lock().unwrap_or_else(|e| e.into_inner());
        let mut mine: Vec<TrailEntry> = read_all(&self.path).into_iter().filter(|e| e.game_id == game_id).collect();
        mine.reverse();
        mine.truncate(limit);
        mine
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_failures_per_game_across_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let trail = Trail::new(dir.path());
        trail.note("a", "Backup", &Err("Acesso negado".into()));
        trail.note("b", "Partida", &Ok("10 min".into()));
        trail.note("a", "Partida", &Ok("5 min".into()));
        drop(trail);

        let reopened = Trail::new(dir.path());
        let a = reopened.for_game("a", 10);
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].step, "Partida");
        assert!(!a[1].ok);
        assert_eq!(a[1].message, "Acesso negado");
    }

    #[test]
    fn a_damaged_line_does_not_hide_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let trail = Trail::new(dir.path());
        trail.note("a", "Backup", &Ok("1".into()));
        let mut file = OpenOptions::new().append(true).open(dir.path().join("trail.jsonl")).unwrap();
        file.write_all(b"{\"at\":1,\"game_").unwrap();
        file.write_all(b"\n").unwrap();
        trail.note("a", "Backup", &Ok("2".into()));
        assert_eq!(trail.for_game("a", 10).len(), 2);
    }

    #[test]
    fn stays_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let trail = Trail::new(dir.path());
        for n in 0..(KEEP + SLACK + 5) {
            trail.note("a", "Backup", &Ok(n.to_string()));
        }
        let all = read_all(&dir.path().join("trail.jsonl"));
        assert!(all.len() <= KEEP + SLACK);
        assert_eq!(all.last().unwrap().message, (KEEP + SLACK + 4).to_string());
    }

    #[test]
    fn an_unwritable_place_does_not_fail_the_step() {
        let dir = tempfile::tempdir().unwrap();
        // A folder where the file should be: every write fails.
        fs::create_dir(dir.path().join("trail.jsonl")).unwrap();
        let trail = Trail::new(dir.path());
        trail.note("a", "Backup", &Err("x".into()));
        assert!(trail.for_game("a", 10).is_empty());
    }
}

/// Records a step through the app's shared history.
pub fn note(app: &tauri::AppHandle, game_id: &str, step: &str, outcome: &Result<String, String>) {
    use tauri::Manager;
    if let Some(trail) = app.try_state::<Trail>() {
        trail.note(game_id, step, outcome);
    }
}
