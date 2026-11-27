//! The achievements kept on this PC for good (`achievements.db`, SQLite, in the
//! app's data folder, not the cache): every game seen, every full list read and
//! every unlock, with its date. Each sync merges into it, so an unlock once seen
//! is never lost (even when a store stops listing it), and the hub opens from it
//! at once. It also remembers when each source was last asked and how long to
//! wait after a refusal, across restarts, so stores aren't asked too often.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use cerberus_shared::{Achievement, AchievementGame, AchievementSource, RecentUnlock};
use rusqlite::{params, Connection, OptionalExtension};

/// Between two syncs of the same source, at least this long.
pub const SYNC_EVERY: u64 = 300;

/// Who the data belongs to: the store and the account in it.
#[derive(Clone, Debug, PartialEq)]
pub struct Scope {
    pub source: AchievementSource,
    pub account: String,
}

impl Scope {
    pub fn new(source: AchievementSource, account: &str) -> Self {
        Scope { source, account: account.trim().to_lowercase() }
    }
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub struct Store(Connection);

impl Store {
    pub fn open(data: &Path) -> Result<Store, String> {
        let _ = std::fs::create_dir_all(data);
        let file = data.join("achievements.db");
        let c = Connection::open(&file).map_err(|e| e.to_string())?;
        // The schema is brought up to date once per run for each file, not on every open.
        static UP_TO_DATE: std::sync::Mutex<Vec<std::path::PathBuf>> = std::sync::Mutex::new(Vec::new());
        let mut done = UP_TO_DATE.lock().unwrap_or_else(|p| p.into_inner());
        let store = if done.contains(&file) { Self::connected(c)? } else { Self::with(c)? };
        if !done.contains(&file) {
            done.push(file);
        }
        Ok(store)
    }

    #[cfg(test)]
    pub fn memory() -> Store {
        Self::with(Connection::open_in_memory().unwrap()).unwrap()
    }

    /// A connection to a database already up to date.
    fn connected(c: Connection) -> Result<Store, String> {
        c.busy_timeout(std::time::Duration::from_secs(5)).map_err(|e| e.to_string())?;
        Ok(Store(c))
    }

    /// The same, applying the migrations the database hasn't had yet
    /// (`PRAGMA user_version` counts them), each in its own transaction.
    fn with(mut c: Connection) -> Result<Store, String> {
        c.busy_timeout(std::time::Duration::from_secs(5)).map_err(|e| e.to_string())?;
        c.execute_batch("PRAGMA journal_mode=WAL;").map_err(|e| e.to_string())?;
        let at: usize = c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)).map_err(|e| e.to_string())? as usize;
        for (i, step) in MIGRATIONS.iter().enumerate().skip(at) {
            let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(|e| e.to_string())?;
            tx.execute_batch(step).map_err(|e| e.to_string())?;
            tx.execute_batch(&format!("PRAGMA user_version = {}", i + 1)).map_err(|e| e.to_string())?;
            tx.commit().map_err(|e| e.to_string())?;
        }
        Ok(Store(c))
    }
}

/// The schema, one step per version. A new column is a new step at the end;
/// a step already shipped never changes. Version 1 is the schema from before
/// versions were counted (IF NOT EXISTS: an old database only gets its number).
const MIGRATIONS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS games(source TEXT NOT NULL, account TEXT NOT NULL, key TEXT NOT NULL, data TEXT NOT NULL,
                PRIMARY KEY(source, account, key));
             CREATE TABLE IF NOT EXISTS lists(source TEXT NOT NULL, account TEXT NOT NULL, game TEXT NOT NULL, played INTEGER NOT NULL, data TEXT NOT NULL,
                PRIMARY KEY(source, account, game));
             CREATE TABLE IF NOT EXISTS unlocks(source TEXT NOT NULL, account TEXT NOT NULL, game TEXT NOT NULL, id TEXT NOT NULL,
                unlocked_at INTEGER NOT NULL, data TEXT NOT NULL, PRIMARY KEY(source, account, game, id));
             CREATE INDEX IF NOT EXISTS unlocks_when ON unlocks(unlocked_at DESC);
             CREATE TABLE IF NOT EXISTS played(source TEXT NOT NULL, account TEXT NOT NULL, key TEXT NOT NULL,
                seconds INTEGER NOT NULL, last INTEGER NOT NULL, PRIMARY KEY(source, account, key));
             CREATE TABLE IF NOT EXISTS stamps(source TEXT NOT NULL, account TEXT NOT NULL, name TEXT NOT NULL, value INTEGER NOT NULL,
                PRIMARY KEY(source, account, name));",
];

impl Store {

    /// Merges games read now: the store's names and art win, progress only grows
    /// (a smaller count read from one place never hides what another showed).
    pub fn keep_games(&mut self, scope: &Scope, games: &[AchievementGame]) {
        // Immediate: the write lock is taken at the start, so another connection
        // writing meanwhile waits (busy_timeout) instead of failing halfway.
        let tx = match self.0.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate) {
            Ok(tx) => tx,
            Err(e) => {
                eprintln!("conquistas: {e}");
                return;
            }
        };
        for g in games {
            let old: Option<AchievementGame> = tx
                .query_row("SELECT data FROM games WHERE source=?1 AND account=?2 AND key=?3", params![scope.source.code(), scope.account, g.key], |r| r.get::<_, String>(0))
                .optional()
                .ok()
                .flatten()
                .and_then(|t| serde_json::from_str(&t).ok());
            let merged = match old {
                Some(old) => merge(old, g.clone()),
                None => g.clone(),
            };
            let _ = tx.execute(
                "INSERT INTO games(source, account, key, data) VALUES(?1, ?2, ?3, ?4)
                 ON CONFLICT(source, account, key) DO UPDATE SET data=excluded.data",
                params![scope.source.code(), scope.account, g.key, serde_json::to_string(&merged).unwrap_or_default()],
            );
        }
        if let Err(e) = tx.commit() {
            eprintln!("conquistas: {e}");
        }
    }

    /// Unlocks seen now; a date already kept stays (the earliest real one wins).
    pub fn keep_unlocks(&mut self, scope: &Scope, game: &str, list: &[Achievement]) {
        // Immediate: the write lock is taken at the start, so another connection
        // writing meanwhile waits (busy_timeout) instead of failing halfway.
        let tx = match self.0.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate) {
            Ok(tx) => tx,
            Err(e) => {
                eprintln!("conquistas: {e}");
                return;
            }
        };
        for a in list {
            let Some(at) = a.unlocked_at else { continue };
            let _ = tx.execute(
                "INSERT INTO unlocks(source, account, game, id, unlocked_at, data) VALUES(?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(source, account, game, id) DO UPDATE SET data=excluded.data,
                   unlocked_at = CASE WHEN unlocked_at <= 1 THEN excluded.unlocked_at
                                      WHEN excluded.unlocked_at <= 1 THEN unlocked_at
                                      ELSE MIN(unlocked_at, excluded.unlocked_at) END",
                params![scope.source.code(), scope.account, game, a.id, at as i64, serde_json::to_string(a).unwrap_or_default()],
            );
        }
        if let Err(e) = tx.commit() {
            eprintln!("conquistas: {e}");
        }
    }

    /// A game's full list, as read when the game had last been played at `played`.
    pub fn keep_list(&mut self, scope: &Scope, game: &str, played: u64, list: &[Achievement]) {
        let _ = self.0.execute(
            "INSERT INTO lists(source, account, game, played, data) VALUES(?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(source, account, game) DO UPDATE SET played=excluded.played, data=excluded.data",
            params![scope.source.code(), scope.account, game, played as i64, serde_json::to_string(list).unwrap_or_default()],
        );
        self.keep_unlocks(scope, game, list);
    }

    /// The kept full list and when the game had last been played when it was read.
    pub fn list(&self, scope: &Scope, game: &str) -> Option<(u64, Vec<Achievement>)> {
        self.0
            .query_row("SELECT played, data FROM lists WHERE source=?1 AND account=?2 AND game=?3", params![scope.source.code(), scope.account, game], |r| {
                Ok((r.get::<_, i64>(0)? as u64, r.get::<_, String>(1)?))
            })
            .optional()
            .ok()
            .flatten()
            .and_then(|(p, t)| serde_json::from_str(&t).ok().map(|l| (p, l)))
    }

    pub fn game(&self, scope: &Scope, key: &str) -> Option<AchievementGame> {
        self.0
            .query_row("SELECT data FROM games WHERE source=?1 AND account=?2 AND key=?3", params![scope.source.code(), scope.account, key], |r| r.get::<_, String>(0))
            .optional()
            .ok()
            .flatten()
            .and_then(|t| serde_json::from_str(&t).ok())
    }

    pub fn games(&self, scope: &Scope) -> Vec<AchievementGame> {
        let Ok(mut q) = self.0.prepare("SELECT data FROM games WHERE source=?1 AND account=?2") else { return Vec::new() };
        q.query_map(params![scope.source.code(), scope.account], |r| r.get::<_, String>(0))
            .map(|rows| rows.flatten().filter_map(|t| serde_json::from_str(&t).ok()).collect())
            .unwrap_or_default()
    }

    /// The newest unlocks with a real date, across the scopes given.
    pub fn recent(&self, scopes: &[Scope], limit: usize) -> Vec<RecentUnlock> {
        let mut all = Vec::new();
        for scope in scopes {
            let Ok(mut q) = self.0.prepare(
                "SELECT u.game, u.data, g.data, u.unlocked_at FROM unlocks u LEFT JOIN games g ON g.source=u.source AND g.account=u.account AND g.key=u.game
                 WHERE u.source=?1 AND u.account=?2 AND u.unlocked_at > 1 ORDER BY u.unlocked_at DESC LIMIT ?3",
            ) else {
                continue;
            };
            let rows = q.query_map(params![scope.source.code(), scope.account, limit as i64], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, i64>(3)?)));
            for (game, a, g, at) in rows.into_iter().flatten().flatten() {
                let Ok(mut achievement) = serde_json::from_str::<Achievement>(&a) else { continue };
                achievement.unlocked_at = Some(at as u64);
                let name = g.and_then(|t| serde_json::from_str::<AchievementGame>(&t).ok()).map(|g| g.name).unwrap_or_default();
                all.push(RecentUnlock { source: scope.source, game_key: game, game_name: name, achievement });
            }
        }
        all.sort_by(|a, b| b.achievement.unlocked_at.cmp(&a.achievement.unlocked_at));
        all.truncate(limit);
        all
    }

    /// From the kept full lists: the easiest locked achievements of games in
    /// progress (the most common first), and the rarest ones unlocked.
    pub fn highlights(&self, scopes: &[Scope], limit: usize) -> (Vec<RecentUnlock>, Vec<RecentUnlock>) {
        let mut next = Vec::new();
        let mut rarest = Vec::new();
        for scope in scopes {
            let Ok(mut q) = self.0.prepare(
                "SELECT l.game, l.data, g.data FROM lists l JOIN games g ON g.source=l.source AND g.account=l.account AND g.key=l.game
                 WHERE l.source=?1 AND l.account=?2",
            ) else {
                continue;
            };
            let rows = q.query_map(params![scope.source.code(), scope.account], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)));
            for (game, list, g) in rows.into_iter().flatten().flatten() {
                let (Ok(list), Ok(g)) = (serde_json::from_str::<Vec<Achievement>>(&list), serde_json::from_str::<AchievementGame>(&g)) else { continue };
                let started = g.unlocked > 0 && g.unlocked < g.total;
                for a in list {
                    if a.rarity.is_none() {
                        continue;
                    }
                    let item = |a: Achievement| RecentUnlock { source: scope.source, game_key: game.clone(), game_name: g.name.clone(), achievement: a };
                    match a.unlocked_at {
                        None if started && !a.hidden => next.push(item(a)),
                        Some(_) => rarest.push(item(a)),
                        None => {}
                    }
                }
            }
        }
        let rarity = |r: &RecentUnlock| r.achievement.rarity.unwrap_or(0.0);
        next.sort_by(|a, b| rarity(b).total_cmp(&rarity(a)));
        rarest.sort_by(|a, b| rarity(a).total_cmp(&rarity(b)));
        // One per game, so a single game doesn't fill the row.
        let one_each = |list: Vec<RecentUnlock>| {
            let mut seen = std::collections::HashSet::new();
            list.into_iter().filter(|r| seen.insert((r.source, r.game_key.clone()))).take(limit).collect::<Vec<_>>()
        };
        (one_each(next), one_each(rarest))
    }

    /// The account's play time per game (seconds, and when last played; 0 when unknown).
    /// Only grows: a smaller answer never takes time away.
    pub fn keep_played(&mut self, scope: &Scope, times: &[(String, u64, u64)]) {
        // Immediate: the write lock is taken at the start, so another connection
        // writing meanwhile waits (busy_timeout) instead of failing halfway.
        let tx = match self.0.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate) {
            Ok(tx) => tx,
            Err(e) => {
                eprintln!("conquistas: {e}");
                return;
            }
        };
        for (key, seconds, last) in times {
            let _ = tx.execute(
                "INSERT INTO played(source, account, key, seconds, last) VALUES(?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(source, account, key) DO UPDATE SET seconds=MAX(seconds, excluded.seconds), last=MAX(last, excluded.last)",
                params![scope.source.code(), scope.account, key, *seconds as i64, *last as i64],
            );
        }
        if let Err(e) = tx.commit() {
            eprintln!("conquistas: {e}");
        }
    }

    /// Play time per game key, for a store's account.
    pub fn played(&self, scope: &Scope) -> std::collections::HashMap<String, (u64, u64)> {
        let Ok(mut q) = self.0.prepare("SELECT key, seconds, last FROM played WHERE source=?1 AND account=?2") else { return Default::default() };
        q.query_map(params![scope.source.code(), scope.account], |r| Ok((r.get::<_, String>(0)?, (r.get::<_, i64>(1)? as u64, r.get::<_, i64>(2)? as u64))))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default()
    }

    pub fn stamp(&self, scope: &Scope, name: &str) -> Option<u64> {
        self.0
            .query_row("SELECT value FROM stamps WHERE source=?1 AND account=?2 AND name=?3", params![scope.source.code(), scope.account, name], |r| r.get::<_, i64>(0))
            .optional()
            .ok()
            .flatten()
            .map(|v| v as u64)
    }

    pub fn set_stamp(&self, scope: &Scope, name: &str, value: u64) {
        let _ = self.0.execute(
            "INSERT INTO stamps(source, account, name, value) VALUES(?1, ?2, ?3, ?4)
             ON CONFLICT(source, account, name) DO UPDATE SET value=excluded.value",
            params![scope.source.code(), scope.account, name, value as i64],
        );
    }

    /// Not waiting after a refusal.
    pub fn ready(&self, scope: &Scope) -> bool {
        self.stamp(scope, "wait_until").is_none_or(|w| now() >= w)
    }

    /// Ready, and the last sync is old enough to ask again.
    pub fn due(&self, scope: &Scope) -> bool {
        self.ready(scope) && self.stamp(scope, "synced").is_none_or(|s| now() >= s + SYNC_EVERY)
    }

    /// The sync went through: ask again after `SYNC_EVERY`.
    pub fn synced(&self, scope: &Scope) {
        self.set_stamp(scope, "synced", now());
        self.set_stamp(scope, "failures", 0);
        self.set_stamp(scope, "wait_until", 0);
    }

    /// The store said no (busy, offline): wait longer each time in a row, 2 min up to 6 h.
    pub fn refused(&self, scope: &Scope) {
        let failures = self.stamp(scope, "failures").unwrap_or(0) + 1;
        self.set_stamp(scope, "failures", failures);
        let wait = (120u64 << (failures - 1).min(10)).min(6 * 3600);
        self.set_stamp(scope, "wait_until", now() + wait);
    }

    /// Everything of an account (signed out).
    pub fn forget(&self, scope: &Scope) {
        for table in ["games", "lists", "unlocks", "stamps", "played"] {
            let _ = self.0.execute(&format!("DELETE FROM {table} WHERE source=?1 AND account=?2"), params![scope.source.code(), scope.account]);
        }
    }
}

/// The newer reading, with progress that never goes back.
fn merge(old: AchievementGame, mut new: AchievementGame) -> AchievementGame {
    if new.name.trim().is_empty() {
        new.name = old.name;
    }
    if new.art.is_empty() {
        new.art = old.art;
    }
    new.total = new.total.max(if new.total == 0 { old.total } else { 0 });
    new.unlocked = new.unlocked.max(old.unlocked);
    if new.total > 0 {
        new.unlocked = new.unlocked.min(new.total);
    }
    new.hardcore = new.hardcore.max(old.hardcore);
    new.points = new.points.max(old.points);
    new.points_total = new.points_total.max(old.points_total);
    new.last_unlock = new.last_unlock.max(old.last_unlock);
    new.last_played = new.last_played.max(old.last_played);
    new.playtime_secs = new.playtime_secs.max(old.playtime_secs);
    if new.award.is_none() {
        new.award = old.award;
        new.award_at = old.award_at;
    }
    new
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_a_v1_database() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("achievements.db");
        {
            // A database made before versions were counted, with a row in it.
            let c = Connection::open(&file).unwrap();
            c.execute_batch(MIGRATIONS[0]).unwrap();
            c.execute("INSERT INTO stamps(source, account, name, value) VALUES('epic', 'p', 'synced', 7)", []).unwrap();
            assert_eq!(c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
        }
        let db = Store::open(dir.path()).unwrap();
        assert_eq!(db.stamp(&Scope::new(AchievementSource::Epic, "p"), "synced"), Some(7), "no row lost");
        let version: i64 = db.0.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version as usize, MIGRATIONS.len());
        drop(db);
        // Opened again: nothing to apply.
        assert!(Store::open(dir.path()).is_ok());
    }

    #[test]
    fn parallel_writers_lose_nothing() {
        let dir = tempfile::tempdir().unwrap();
        Store::open(dir.path()).unwrap();
        let writers: Vec<_> = ["a", "b"]
            .into_iter()
            .map(|who| {
                let path = dir.path().to_path_buf();
                std::thread::spawn(move || {
                    let mut db = Store::open(&path).unwrap();
                    let scope = Scope::new(AchievementSource::Epic, "player");
                    for round in 0..20 {
                        let times: Vec<(String, u64, u64)> = (0..50).map(|i| (format!("{who}-{round}-{i}"), 60, 1)).collect();
                        db.keep_played(&scope, &times);
                    }
                })
            })
            .collect();
        for w in writers {
            w.join().unwrap();
        }
        let db = Store::open(dir.path()).unwrap();
        assert_eq!(db.played(&Scope::new(AchievementSource::Epic, "player")).len(), 2000);
    }

    fn game(key: &str, unlocked: u32, total: u32) -> AchievementGame {
        AchievementGame {
            source: AchievementSource::Xbox,
            key: key.into(),
            name: format!("Game {key}"),
            art: Vec::new(),
            platform: "Xbox".into(),
            total,
            unlocked,
            hardcore: 0,
            points: 0,
            points_total: 0,
            last_unlock: None,
            playtime_secs: None,
            last_played: None,
            award: None,
            award_at: None,
        }
    }

    fn unlock(id: &str, at: Option<u64>) -> Achievement {
        Achievement { id: id.into(), name: id.into(), description: String::new(), icon: None, icon_locked: None, hidden: false, unlocked_at: at, hardcore_at: None, points: 0, rarity: None }
    }

    #[test]
    fn progress_and_unlocks_are_never_lost() {
        let mut s = Store::memory();
        let me = Scope::new(AchievementSource::Xbox, "Me");
        s.keep_games(&me, &[game("1", 10, 50)]);
        // A later reading that knows less (no total, fewer unlocks) doesn't erase it.
        s.keep_games(&me, &[game("1", 3, 0)]);
        let g = s.game(&me, "1").unwrap();
        assert_eq!((g.unlocked, g.total), (10, 50));
        s.keep_unlocks(&me, "1", &[unlock("a", Some(500)), unlock("b", Some(1))]);
        s.keep_unlocks(&me, "1", &[unlock("a", Some(900)), unlock("b", Some(700))]);
        let recent = s.recent(std::slice::from_ref(&me), 10);
        let dates: Vec<_> = recent.iter().map(|r| (r.achievement.id.as_str(), r.achievement.unlocked_at)).collect();
        assert_eq!(dates, vec![("b", Some(700)), ("a", Some(500))], "earliest real date kept, unknown replaced");
        assert_eq!(recent[0].game_name, "Game 1");
        // Another account sees nothing of it.
        assert!(s.recent(&[Scope::new(AchievementSource::Xbox, "other")], 10).is_empty());
    }

    #[test]
    fn waits_after_refusals_and_between_syncs() {
        let s = Store::memory();
        let me = Scope::new(AchievementSource::Retro, "me");
        assert!(s.due(&me));
        s.synced(&me);
        assert!(!s.due(&me) && s.ready(&me));
        s.refused(&me);
        assert!(!s.ready(&me));
        let first = s.stamp(&me, "wait_until").unwrap();
        s.refused(&me);
        assert!(s.stamp(&me, "wait_until").unwrap() > first, "waits longer each time");
    }
}
