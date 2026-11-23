//! How fast recomp downloads may go: the player's limit, and a slower one while
//! a game is open so the download doesn't take the connection from it. The
//! download loop asks after each piece and waits when it got ahead.
//!
//! Also: moving files Windows (or an antivirus scanning them) holds for a
//! moment is tried again for a few seconds before giving up.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use cerberus_shared::tr;

/// The player's limit in bytes per second; 0 for none.
static LIMIT: AtomicU64 = AtomicU64::new(0);
/// Slow down while a game is open.
static WHEN_PLAYING: AtomicBool = AtomicBool::new(true);
/// A game is open right now.
static PLAYING: AtomicBool = AtomicBool::new(false);

/// The speed while a game is open, when that's turned on.
pub const PLAYING_LIMIT: u64 = 3 * 1024 * 1024;

pub fn set_limit(mb_per_second: u32, slow_when_playing: bool) {
    LIMIT.store(u64::from(mb_per_second) * 1024 * 1024, Ordering::Relaxed);
    WHEN_PLAYING.store(slow_when_playing, Ordering::Relaxed);
}

pub fn set_playing(on: bool) {
    PLAYING.store(on, Ordering::Relaxed);
}

/// The limit in force now, in bytes per second (0 = none).
pub fn current() -> u64 {
    let chosen = LIMIT.load(Ordering::Relaxed);
    if PLAYING.load(Ordering::Relaxed) && WHEN_PLAYING.load(Ordering::Relaxed) {
        if chosen == 0 { PLAYING_LIMIT } else { chosen.min(PLAYING_LIMIT) }
    } else {
        chosen
    }
}

/// Keeps a download under the limit: call after each piece with its size.
pub struct Pace {
    since: Instant,
    bytes: u64,
    limit: u64,
}

impl Default for Pace {
    fn default() -> Self {
        Self { since: Instant::now(), bytes: 0, limit: current() }
    }
}

impl Pace {
    /// How long to wait after `got` more bytes so the average stays under the limit.
    pub fn after(&mut self, got: u64) -> Option<Duration> {
        let limit = current();
        // The limit changed (a game opened, a setting moved): count from now.
        if limit != self.limit {
            *self = Self { since: Instant::now(), bytes: 0, limit };
        }
        if limit == 0 {
            return None;
        }
        self.bytes += got;
        let should_take = Duration::from_secs_f64(self.bytes as f64 / limit as f64);
        let took = self.since.elapsed();
        // A fresh window every few seconds keeps the average honest after a pause.
        if took > Duration::from_secs(5) {
            *self = Self { since: Instant::now(), bytes: 0, limit };
        }
        should_take.checked_sub(took).filter(|d| *d > Duration::from_millis(5))
    }
}

/// Windows answers "in use" (32) or "access denied" (5) while another program,
/// often an antivirus checking a new file, holds it.
fn held(e: &std::io::Error) -> bool {
    matches!(e.raw_os_error(), Some(5 | 32 | 33)) || e.kind() == std::io::ErrorKind::PermissionDenied
}

/// `fs::rename`, tried again for up to `tries` × 500 ms while the file is held.
pub fn rename_patiently(from: &Path, to: &Path, tries: u32) -> Result<(), String> {
    let mut left = tries;
    loop {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) if held(&e) && left > 0 => {
                left -= 1;
                std::thread::sleep(Duration::from_millis(500));
            }
            Err(e) if held(&e) => {
                return Err(tr!("O Windows ou o antivírus está segurando um arquivo do jogo ({}). Feche o jogo e programas que usem a pasta, e tente de novo.", to.display()));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_playing_limit_applies_only_while_playing_and_when_wanted() {
        set_limit(0, true);
        set_playing(false);
        assert_eq!(current(), 0);
        set_playing(true);
        assert_eq!(current(), PLAYING_LIMIT);
        set_limit(1, true);
        assert_eq!(current(), 1024 * 1024, "a lower limit of the player's still wins");
        // 2 MB right away at 1 MB/s: about two seconds to wait.
        let mut pace = Pace::default();
        let wait = pace.after(2 * 1024 * 1024).unwrap();
        assert!(wait > Duration::from_millis(1900) && wait <= Duration::from_secs(2), "{wait:?}");
        set_limit(0, false);
        assert_eq!(current(), 0);
        set_playing(false);
    }

    #[test]
    fn a_held_file_is_tried_again_and_a_free_one_moves_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.txt");
        std::fs::write(&a, b"x").unwrap();
        rename_patiently(&a, &dir.path().join("b.txt"), 3).unwrap();
        assert!(dir.path().join("b.txt").exists());
        // Missing file: not "held", fails at once.
        assert!(rename_patiently(&a, &dir.path().join("c.txt"), 3).is_err());
    }
}
