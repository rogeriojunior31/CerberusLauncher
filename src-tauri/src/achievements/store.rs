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
/// Pressing Update asks again this long after the last sync, not before.
const FORCED_FLOOR: u64 = 60;

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

/// A consistent copy of the database at `to` (the file itself can't be copied
/// while it's open with its write-ahead log).
pub fn copy_to(data: &Path, to: &Path) -> Result<(), String> {
    let file = data.join("achievements.db");
    if !file.exists() {
        return Err("no achievements database".into());
    }
    let _ = std::fs::remove_file(to);
    let c = Connection::open(&file).map_err(|e| e.to_string())?;
    c.execute("VACUUM INTO ?1", params![to.to_string_lossy()]).map_err(|e| e.to_string())?;
    Ok(())
}

/// Brings in what another copy of the database saw: unlocks and games this
/// one doesn't have, and the larger play time. Nothing here is taken away,
/// and this PC's sync times (stamps) stay its own.
pub fn merge_from(data: &Path, other: &Path) -> Result<(), String> {
    let mut db = Store::open(data)?;
    db.0.execute("ATTACH DATABASE ?1 AS other", params![other.to_string_lossy()]).map_err(|e| e.to_string())?;
    let result = (|| {
        let tx = db.0.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(|e| e.to_string())?;
        tx.execute_batch(
            "INSERT OR IGNORE INTO games(source, account, key, data) SELECT source, account, key, data FROM other.games;
             INSERT OR IGNORE INTO lists(source, account, game, played, data) SELECT source, account, game, played, data FROM other.lists;
             INSERT OR IGNORE INTO unlocks(source, account, game, id, unlocked_at, data) SELECT source, account, game, id, unlocked_at, data FROM other.unlocks;
             INSERT INTO played(source, account, key, seconds, last) SELECT source, account, key, seconds, last FROM other.played WHERE true
                ON CONFLICT(source, account, key) DO UPDATE SET seconds=MAX(seconds, excluded.seconds), last=MAX(last, excluded.last);",
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    })();
    let _ = db.0.execute("DETACH DATABASE other", []);
    result
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
    // 2: the language a list was read in, so one read in another is read again.
    "ALTER TABLE lists ADD COLUMN lang TEXT NOT NULL DEFAULT '';",
    // 3: when an unlock seen once stopped being in the store's own list (0: still there).
    "ALTER TABLE unlocks ADD COLUMN revoked_at INTEGER NOT NULL DEFAULT 0;",
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
            "INSERT INTO lists(source, account, game, played, data, lang) VALUES(?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(source, account, game) DO UPDATE SET played=excluded.played, data=excluded.data, lang=excluded.lang",
            params![scope.source.code(), scope.account, game, played as i64, serde_json::to_string(list).unwrap_or_default(), cerberus_shared::i18n::lang().code()],
        );
        self.keep_unlocks(scope, game, list);
    }

    /// A whole list from a source that says what the player has now (Xbox,
    /// Epic, RetroAchievements, Steam's Web API): what was unlocked once and
    /// isn't now is marked revoked (kept, not counted), what came back is not,
    /// and the game's counts become the list's instead of the highest seen.
    /// Steam's files on this PC are not one: a game not opened here may be behind.
    pub fn keep_current_list(&mut self, scope: &Scope, game: &str, played: u64, list: &[Achievement]) {
        self.keep_list(scope, game, played, list);
        if list.is_empty() {
            return;
        }
        let now = now() as i64;
        let tx = match self.0.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate) {
            Ok(tx) => tx,
            Err(e) => {
                eprintln!("conquistas: {e}");
                return;
            }
        };
        let unlocked: std::collections::HashSet<&str> = list.iter().filter(|a| a.unlocked_at.is_some()).map(|a| a.id.as_str()).collect();
        let kept: Vec<String> = tx
            .prepare("SELECT id FROM unlocks WHERE source=?1 AND account=?2 AND game=?3")
            .and_then(|mut q| q.query_map(params![scope.source.code(), scope.account, game], |r| r.get::<_, String>(0))?.collect())
            .unwrap_or_default();
        for id in kept {
            let _ = if unlocked.contains(id.as_str()) {
                tx.execute("UPDATE unlocks SET revoked_at=0 WHERE source=?1 AND account=?2 AND game=?3 AND id=?4", params![scope.source.code(), scope.account, game, id])
            } else {
                tx.execute(
                    "UPDATE unlocks SET revoked_at=?5 WHERE source=?1 AND account=?2 AND game=?3 AND id=?4 AND revoked_at=0",
                    params![scope.source.code(), scope.account, game, id, now],
                )
            };
        }
        let row: Option<AchievementGame> = tx
            .query_row("SELECT data FROM games WHERE source=?1 AND account=?2 AND key=?3", params![scope.source.code(), scope.account, game], |r| r.get::<_, String>(0))
            .optional()
            .ok()
            .flatten()
            .and_then(|t| serde_json::from_str(&t).ok());
        if let Some(mut g) = row {
            let have = list.iter().filter(|a| a.unlocked_at.is_some());
            g.total = list.len() as u32;
            g.unlocked = have.clone().count() as u32;
            g.hardcore = list.iter().filter(|a| a.hardcore_at.is_some()).count() as u32;
            g.points = have.map(|a| a.points).sum();
            g.points_total = g.points_total.max(list.iter().map(|a| a.points).sum());
            let _ = tx.execute(
                "UPDATE games SET data=?4 WHERE source=?1 AND account=?2 AND key=?3",
                params![scope.source.code(), scope.account, game, serde_json::to_string(&g).unwrap_or_default()],
            );
        }
        if let Err(e) = tx.commit() {
            eprintln!("conquistas: {e}");
        }
    }

    /// Unlocks the store took back (seen once, not in its list now), the latest first.
    pub fn revoked(&self, scopes: &[Scope], limit: usize) -> Vec<RecentUnlock> {
        let mut all = Vec::new();
        for scope in scopes {
            let Ok(mut q) = self.0.prepare(
                "SELECT u.game, u.data, g.data, u.unlocked_at, u.revoked_at FROM unlocks u LEFT JOIN games g ON g.source=u.source AND g.account=u.account AND g.key=u.game
                 WHERE u.source=?1 AND u.account=?2 AND u.revoked_at > 0 ORDER BY u.revoked_at DESC LIMIT ?3",
            ) else {
                continue;
            };
            let rows = q.query_map(params![scope.source.code(), scope.account, limit as i64], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?))
            });
            for (game, a, g, at, revoked) in rows.into_iter().flatten().flatten() {
                let Ok(mut achievement) = serde_json::from_str::<Achievement>(&a) else { continue };
                achievement.unlocked_at = Some(at as u64);
                let name = g.and_then(|t| serde_json::from_str::<AchievementGame>(&t).ok()).map(|g| g.name).unwrap_or_default();
                all.push((revoked, RecentUnlock { source: scope.source, game_key: game, game_name: name, achievement }));
            }
        }
        all.sort_by(|a, b| b.0.cmp(&a.0));
        all.into_iter().take(limit).map(|(_, r)| r).collect()
    }

    /// The kept full list and when the game had last been played when it was
    /// read. One read in another language than the app's now says 0 for when,
    /// so it's read again like a game played since (a few per sync, not all).
    pub fn list(&self, scope: &Scope, game: &str) -> Option<(u64, Vec<Achievement>)> {
        let now = cerberus_shared::i18n::lang().code();
        self.0
            .query_row("SELECT played, data, lang FROM lists WHERE source=?1 AND account=?2 AND game=?3", params![scope.source.code(), scope.account, game], |r| {
                Ok((r.get::<_, i64>(0)? as u64, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
            })
            .optional()
            .ok()
            .flatten()
            .and_then(|(p, t, lang)| {
                // Lists from before languages were kept count as the app's own.
                let played = if lang.is_empty() || lang == now { p } else { 0 };
                serde_json::from_str(&t).ok().map(|l| (played, l))
            })
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
                 WHERE u.source=?1 AND u.account=?2 AND u.unlocked_at > 1 AND u.revoked_at = 0 ORDER BY u.unlocked_at DESC LIMIT ?3",
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

    /// Due, or with `force` (the player pressed Update) due a minute after the
    /// last sync; a store's wait is kept either way.
    pub fn due_now(&self, scope: &Scope, force: bool) -> bool {
        if !force {
            return self.due(scope);
        }
        self.ready(scope) && self.stamp(scope, "synced").is_none_or(|s| now() >= s + FORCED_FLOOR)
    }

    /// When the account last synced, and until when the store asked to wait.
    pub fn status(&self, scope: &Scope) -> cerberus_shared::SourceStatus {
        let now = now();
        cerberus_shared::SourceStatus {
            source: scope.source,
            synced_at: self.stamp(scope, "synced").filter(|s| *s > 0),
            wait_until: self.stamp(scope, "wait_until").filter(|w| *w > now),
            syncing: false,
        }
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

    /// A copy of this PC's real database (the original isn't touched) opens
    /// on the newest schema with nothing lost.
    /// `cargo test -p cerberus-launcher real_database_migrates -- --ignored --nocapture`
    #[test]
    #[ignore = "copies this PC's achievements database"]
    fn real_database_migrates() {
        let real = std::path::PathBuf::from(std::env::var("APPDATA").unwrap()).join("app.cerberus.launcher");
        let dir = tempfile::tempdir().unwrap();
        copy_to(&real, &dir.path().join("achievements.db")).unwrap();
        let count = |c: &Connection, t: &str| c.query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get::<_, i64>(0)).unwrap();
        let before = { let c = Connection::open(dir.path().join("achievements.db")).unwrap(); (count(&c, "unlocks"), count(&c, "lists"), count(&c, "games")) };
        let db = Store::open(dir.path()).unwrap();
        let after = (count(&db.0, "unlocks"), count(&db.0, "lists"), count(&db.0, "games"));
        let version: i64 = db.0.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        println!("before {before:?} after {after:?} version {version}");
        assert_eq!(before, after);
        assert_eq!(version as usize, MIGRATIONS.len());
    }

    #[test]
    fn list_in_other_language_is_stale() {
        let mut db = Store::memory();
        let scope = Scope::new(AchievementSource::Xbox, "p");
        db.keep_list(&scope, "game", 500, &[]);
        assert_eq!(db.list(&scope, "game").map(|(p, _)| p), Some(500), "read in the app's language: as it was");
        let other = if cerberus_shared::i18n::lang().code() == "en" { "pt-BR" } else { "en" };
        db.0.execute("UPDATE lists SET lang=?1", params![other]).unwrap();
        assert_eq!(db.list(&scope, "game").map(|(p, _)| p), Some(0), "another language: read again");
        db.0.execute("UPDATE lists SET lang=''", []).unwrap();
        assert_eq!(db.list(&scope, "game").map(|(p, _)| p), Some(500), "from before languages were kept: left alone");
    }

    #[test]
    fn source_status_reads_stamps_and_update_asks_a_minute_later() {
        let dir = tempfile::tempdir().unwrap();
        let db = Store::open(dir.path()).unwrap();
        let scope = Scope::new(AchievementSource::Retro, "player");
        assert_eq!((db.status(&scope).synced_at, db.status(&scope).wait_until), (None, None), "never synced");
        db.synced(&scope);
        let at = db.status(&scope).synced_at.unwrap();
        assert!(!db.due_now(&scope, false) && !db.due_now(&scope, true), "just synced: not even Update asks again");
        db.set_stamp(&scope, "synced", at - FORCED_FLOOR);
        assert!(!db.due_now(&scope, false), "a minute later: not due on its own");
        assert!(db.due_now(&scope, true), "but Update asks");
        // The store said no: the wait shows, and Update keeps it.
        db.refused(&scope);
        assert!(db.status(&scope).wait_until.is_some_and(|w| w > now()));
        assert!(!db.due_now(&scope, true));
        db.set_stamp(&scope, "wait_until", now() - 1);
        assert_eq!(db.status(&scope).wait_until, None, "a wait that's over isn't shown");
    }

    #[test]
    fn taken_back_unlocks_are_kept_apart_and_stop_counting() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = Store::open(dir.path()).unwrap();
        let scope = Scope::new(AchievementSource::Xbox, "player");
        db.keep_games(&scope, &[game("g", 3, 3)]);
        db.keep_current_list(&scope, "g", 1, &[unlock("a", Some(100)), unlock("b", Some(200)), unlock("c", Some(300))]);
        assert!(db.revoked(&[scope.clone()], 10).is_empty());
        // The store took "c" back and the developer removed "b".
        db.keep_current_list(&scope, "g", 2, &[unlock("a", Some(100)), unlock("c", None)]);
        let revoked: Vec<String> = db.revoked(&[scope.clone()], 10).into_iter().map(|r| r.achievement.id).collect();
        assert_eq!(revoked.len(), 2, "both kept apart: {revoked:?}");
        let recent: Vec<String> = db.recent(&[scope.clone()], 10).into_iter().map(|r| r.achievement.id).collect();
        assert_eq!(recent, vec!["a"], "only what the store still shows is recent");
        let g = db.game(&scope, "g").unwrap();
        assert_eq!((g.unlocked, g.total), (1, 2), "the game counts what the store says now, not the highest seen");
        // Unlocked again: no longer revoked, and the first date stays.
        db.keep_current_list(&scope, "g", 3, &[unlock("a", Some(100)), unlock("c", Some(900))]);
        let revoked: Vec<String> = db.revoked(&[scope.clone()], 10).into_iter().map(|r| r.achievement.id).collect();
        assert_eq!(revoked, vec!["b"]);
        assert!(db.recent(&[scope.clone()], 10).iter().any(|r| r.achievement.id == "c" && r.achievement.unlocked_at == Some(300)));
        // A list from Steam's files on this PC changes nothing (may be behind).
        db.keep_list(&scope, "g", 4, &[unlock("a", None), unlock("c", None)]);
        assert_eq!(db.revoked(&[scope], 10).len(), 1);
    }

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
