//! The achievements hub: Steam (from the Steam client's files on this PC) and
//! RetroAchievements (from its Web API), side by side.

pub mod classic;
pub mod epic;
pub mod live;
pub mod retro;
pub mod steam;
pub mod steam_api;
pub mod store;
pub mod xbox;

use store::{Scope, Store};

/// The Xbox account's refresh token, renewed in the settings when Xbox Live
/// hands a new one (only while the account the renewal started from is still signed in).
fn save_xbox_refresh(data_dir: &Path, started_with: &str, refresh: &str) {
    if started_with != refresh {
        crate::settings::renew_token(data_dir, crate::settings::Account::Xbox, started_with, refresh);
    }
}

/// Forgets the Epic session kept in memory (signed out, or another account).
pub fn forget_epic_session() {
    if let Ok(mut kept) = EPIC_SESSION.lock() {
        *kept = None;
    }
}

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::SystemTime;

use cerberus_shared::{AchievementGame, AchievementSource, AchievementsHub, GameAchievements, RecentUnlock, tr};

/// How many recent unlocks the hub's timeline keeps.
const RECENT: usize = 40;

/// What Steam's files looked like when they were last read: how many, the newest change, and the play-time file's.
type Mark = (usize, SystemTime, Option<SystemTime>);
type SteamPart = (Vec<AchievementGame>, Vec<RecentUnlock>);

/// The last reading, reused while Steam's files stay the same.
static LAST: Mutex<Option<(Mark, SteamPart)>> = Mutex::new(None);

fn mark(account: &steam::Account) -> Mark {
    let mut count = 0;
    let mut newest = SystemTime::UNIX_EPOCH;
    for entry in std::fs::read_dir(&account.stats_dir).into_iter().flatten().flatten() {
        count += 1;
        if let Ok(t) = entry.metadata().and_then(|m| m.modified()) {
            newest = newest.max(t);
        }
    }
    let config = account.localconfig.as_ref().and_then(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
    (count, newest, config)
}

/// Steam's part of the hub: every game with achievements, and its unlocks.
fn steam_part() -> Option<SteamPart> {
    let root = crate::steam::root()?;
    let account = steam::account(&root)?;
    let now = mark(&account);
    if let Some((was, part)) = LAST.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
        if *was == now {
            return Some(part.clone());
        }
    }
    let played = account.localconfig.as_deref().map(steam::playtimes).unwrap_or_default();
    // Some caches lack the game's name: the installed game's, when there is one.
    let installed: HashMap<String, String> = crate::steam::installed_games().into_iter().map(|g| (g.id, g.name)).collect();
    let mut games = Vec::new();
    let mut recent = Vec::new();
    for app in steam::apps(&account) {
        let Some((mut name, list)) = steam::read_game(&account, app) else { continue };
        if list.is_empty() {
            continue;
        }
        if name.trim().is_empty() {
            name = installed.get(&format!("steam:{app}")).cloned().unwrap_or_default();
        }
        let entry = steam::game_entry(app, name, &list, played.get(&app));
        for a in list.into_iter().filter(|a| a.unlocked_at.is_some_and(|t| t > 0)) {
            recent.push(RecentUnlock { source: AchievementSource::Steam, game_key: entry.key.clone(), game_name: entry.name.clone(), achievement: a });
        }
        games.push(entry);
    }
    let part = (games, recent);
    *LAST.lock().unwrap_or_else(|p| p.into_inner()) = Some((now, part.clone()));
    Some(part)
}

/// Steam's own play time per game: this PC's (localconfig.vdf) and, with the
/// Web API on, the whole account's from the last answer kept on disk; the
/// larger wins. Unix seconds for when it was last played.
pub fn steam_playtimes(data_dir: &Path, steam_id: Option<&str>) -> HashMap<u32, (u64, Option<u64>)> {
    let mut all: HashMap<u32, (u64, Option<u64>)> = crate::steam::root()
        .and_then(|root| steam::account(&root))
        .and_then(|a| a.localconfig)
        .map(|p| steam::playtimes(&p))
        .unwrap_or_default();
    if let Some(id) = steam_id {
        let file = data_dir.join("cache").join("achievements").join("steam-api").join(format!("owned-{id}.json"));
        let owned: Vec<steam_api::Owned> = std::fs::read_to_string(file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        for o in owned {
            let e = all.entry(o.app).or_insert((0, None));
            e.0 = e.0.max(o.minutes * 60);
            if o.last_played > 0 {
                e.1 = Some(e.1.unwrap_or(0).max(o.last_played));
            }
        }
    }
    all
}

/// Names Steam's cache doesn't have, asked once of the Steam store and kept on disk.
async fn fill_names(http: &reqwest::Client, data_dir: &Path, games: &mut [AchievementGame], recent: &mut [RecentUnlock], online: bool) {
    let file = data_dir.join("cache").join("achievements").join("steam-names.json");
    let mut known: HashMap<String, String> = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let nameless = |g: &AchievementGame| g.source == AchievementSource::Steam && g.name == format!("App {}", g.key);
    let missing: Vec<String> = if online { games.iter().filter(|g| nameless(g) && !known.contains_key(&g.key)).map(|g| g.key.clone()).collect() } else { Vec::new() };
    let mut learned = false;
    for chunk in missing.chunks(6) {
        let asks = chunk.iter().map(|app| steam::store_name(http, app));
        for (app, name) in chunk.iter().zip(futures_util::future::join_all(asks).await) {
            if let Some(name) = name {
                known.insert(app.clone(), name);
                learned = true;
            }
        }
    }
    if learned {
        let _ = std::fs::create_dir_all(file.parent().unwrap_or(data_dir));
        let _ = crate::state_file::write_bytes(&file, serde_json::to_string(&known).unwrap_or_default().as_bytes());
    }
    for g in games.iter_mut().filter(|g| nameless(g)) {
        if let Some(name) = known.get(&g.key) {
            g.name = name.clone();
        }
    }
    for r in recent.iter_mut().filter(|r| r.source == AchievementSource::Steam && r.game_name == format!("App {}", r.game_key)) {
        if let Some(name) = known.get(&r.game_key) {
            r.game_name = name.clone();
        }
    }
}

/// Steam's edge servers turn the key away for a while when asked too fast:
/// one question at a time, with a pause after each one that reached Steam.
const AT_ONCE: usize = 1;
/// Games whose unlock dates are asked again per refresh (the most recently
/// played first); the rest use what's kept, and the counts are always fresh.
const DATES_PER_REFRESH: usize = 25;

/// Steam's part from the Web API: every owned game with achievements. The
/// counts come for all games in a few questions; unlock dates come per game,
/// a few each refresh. The local cache still fills icons, and keeps games the
/// API doesn't list.
async fn steam_api_part(
    http: &reqwest::Client,
    data_dir: &Path,
    key: &str,
    id: &str,
    local: Vec<AchievementGame>,
    local_recent: Vec<RecentUnlock>,
    online: bool,
) -> Result<(Vec<AchievementGame>, Vec<RecentUnlock>, Option<String>), String> {
    use futures_util::StreamExt;
    use std::sync::atomic::{AtomicBool, Ordering};
    let not_public = cerberus_shared::i18n::t(steam_api::NOT_PUBLIC);
    let owned = match steam_api::owned(http, data_dir, key, id, online).await {
        Ok(list) => list,
        // Steam busy: last hour's list on disk still beats nothing.
        Err(e) if online && e != not_public => match steam_api::owned(http, data_dir, key, id, false).await {
            Ok(list) => list,
            Err(_) => return Err(e),
        },
        Err(e) => return Err(e),
    };
    let local_keys: std::collections::HashSet<String> = local.iter().map(|g| g.key.clone()).collect();
    let mut wanted: Vec<steam_api::Owned> = owned.into_iter().filter(|o| o.stats && (o.minutes > 0 || local_keys.contains(&o.app.to_string()))).collect();
    // The games played last come first, so a stop halfway still shows what matters.
    wanted.sort_by(|a, b| b.last_played.cmp(&a.last_played).then(b.minutes.cmp(&a.minutes)));

    let mut first_error: Option<String> = None;
    let apps: Vec<(u32, u64)> = wanted.iter().map(|o| (o.app, o.last_played)).collect();
    let tops = match steam_api::tops(http, data_dir, key, id, &apps, online).await {
        Ok(t) => t,
        Err(e) if e == not_public => return Err(e),
        Err(e) => {
            if online && e != steam_api::OFFLINE {
                first_error = Some(e);
            }
            steam_api::tops(http, data_dir, key, id, &apps, false).await.unwrap_or_default()
        }
    };

    // Once Steam says no, the rest is read from the disk only.
    let blocked = AtomicBool::new(!online || first_error.is_some());
    let blocked = &blocked;
    // With the counts in hand the profile is public: a "not public" for one game
    // is about that game only, not a reason to stop.
    let public = !tops.is_empty();
    let mut asks = futures_util::stream::iter(wanted.into_iter().enumerate().map(|(i, o)| async move {
        let ask = i < DATES_PER_REFRESH && !blocked.load(Ordering::Relaxed);
        let mut list = steam_api::mine(http, data_dir, key, id, o.app, o.last_played, ask).await;
        if let Err(e) = &list {
            let one_game = public && *e == cerberus_shared::i18n::t(steam_api::NOT_PUBLIC);
            if ask && e != steam_api::OFFLINE && !one_game {
                blocked.store(true, Ordering::Relaxed);
                let refused = e.clone();
                list = steam_api::mine(http, data_dir, key, id, o.app, o.last_played, false).await.map_err(|_| refused);
            }
        }
        (o, list)
    }))
    .buffer_unordered(AT_ONCE);
    // Icons already known from the local cache, by game and achievement.
    let icons: HashMap<(String, String), (Option<String>, Option<String>)> = local_recent
        .iter()
        .map(|r| ((r.game_key.clone(), r.achievement.id.clone()), (r.achievement.icon.clone(), r.achievement.icon_locked.clone())))
        .collect();
    let mut games = Vec::new();
    let mut recent = Vec::new();
    let mut from_api = std::collections::HashSet::new();
    while let Some((o, list)) = asks.next().await {
        let list = match list {
            Ok(list) => list,
            Err(e) => {
                if e != steam_api::OFFLINE && !(public && e == not_public) {
                    first_error.get_or_insert(e);
                }
                Vec::new()
            }
        };
        let top = tops.get(&o.app).copied().filter(|t| t.total > 0);
        if list.is_empty() && top.is_none() {
            continue;
        }
        let app = o.app.to_string();
        from_api.insert(app.clone());
        let unlocked_list: Vec<&steam_api::Mine> = list.iter().filter(|m| m.achieved).collect();
        let (total, unlocked) = match top {
            Some(t) => (t.total, t.unlocked),
            None => (list.len() as u32, unlocked_list.len() as u32),
        };
        let all = total > 0 && unlocked == total;
        let last = unlocked_list.iter().map(|m| m.unlocked_at).filter(|t| *t > 0).max();
        let name = if o.name.trim().is_empty() { local.iter().find(|g| g.key == app).map(|g| g.name.clone()).unwrap_or_else(|| format!("App {app}")) } else { o.name.clone() };
        for m in &unlocked_list {
            if m.unlocked_at == 0 {
                continue;
            }
            let (icon, icon_locked) = icons.get(&(app.clone(), m.api.clone())).cloned().unwrap_or((None, None));
            recent.push(RecentUnlock {
                source: AchievementSource::Steam,
                game_key: app.clone(),
                game_name: name.clone(),
                achievement: cerberus_shared::Achievement {
                    id: m.api.clone(),
                    name: m.name.clone(),
                    description: m.description.clone(),
                    icon,
                    icon_locked,
                    hidden: false,
                    unlocked_at: Some(m.unlocked_at),
                    hardcore_at: None,
                    points: 0,
                    rarity: None,
                },
            });
        }
        games.push(AchievementGame {
            source: AchievementSource::Steam,
            key: app.clone(),
            name,
            art: steam::art(o.app),
            platform: "Steam".into(),
            total,
            unlocked,
            hardcore: 0,
            points: 0,
            points_total: 0,
            last_unlock: last.or_else(|| local.iter().find(|g| g.key == app).and_then(|g| g.last_unlock)),
            playtime_secs: (o.minutes > 0).then_some(o.minutes * 60),
            last_played: (o.last_played > 0).then_some(o.last_played),
            award: all.then_some(cerberus_shared::GameAward::Completed),
            award_at: if all { last } else { None },
        });
    }
    // Nothing came back at all: a private profile or a key problem; say so.
    if games.is_empty() {
        if let Some(e) = first_error.take() {
            return Err(e);
        }
        if !online {
            return Err(steam_api::OFFLINE.into());
        }
    }
    // Games only the local cache knows (family sharing, a game the API skipped).
    games.extend(local.into_iter().filter(|g| !from_api.contains(&g.key)));
    recent.extend(local_recent.into_iter().filter(|r| !from_api.contains(&r.game_key)));
    Ok((games, recent, first_error))
}

/// Icons for recent Steam unlocks the local cache didn't have, from the game's
/// schema (asked once per game and kept).
async fn fill_recent_icons(http: &reqwest::Client, data_dir: &Path, key: &str, recent: &mut [RecentUnlock], online: bool) {
    let games: std::collections::BTreeSet<String> =
        recent.iter().filter(|r| r.source == AchievementSource::Steam && r.achievement.icon.is_none()).map(|r| r.game_key.clone()).collect();
    for game in games {
        let Ok(app) = game.parse::<u32>() else { continue };
        let Ok(schema) = steam_api::schema(http, data_dir, key, app, online).await else { continue };
        for r in recent.iter_mut().filter(|r| r.source == AchievementSource::Steam && r.game_key == game) {
            if let Some(s) = schema.iter().find(|s| s.api == r.achievement.id) {
                r.achievement.icon = s.icon.clone();
                r.achievement.icon_locked = s.icon_locked.clone();
            }
        }
    }
}

/// What RetroAchievements answered last, kept to show at once next time.
#[derive(serde::Serialize, serde::Deserialize)]
struct RetroKept {
    games: Vec<AchievementGame>,
    recent: Vec<RecentUnlock>,
    points: (u64, u64),
}

fn retro_file(data_dir: &Path, user: &str) -> std::path::PathBuf {
    let safe: String = user.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    data_dir.join("cache").join("achievements").join(format!("retro-{safe}.json"))
}

/// Everything the hub shows. A source that can't be read leaves a note in `problems`.
/// With `online` off nothing is asked of the internet: the hub opens at once with
/// what's on disk, and the page asks again with `online` on to bring it up to date.
fn keep_retro(db: &mut Store, scope: &Scope, r: &RetroKept) {
    db.keep_games(scope, &r.games);
    let mut by_game: HashMap<&str, Vec<cerberus_shared::Achievement>> = HashMap::new();
    for u in &r.recent {
        by_game.entry(u.game_key.as_str()).or_default().push(u.achievement.clone());
    }
    for (game, list) in by_game {
        db.keep_unlocks(scope, game, &list);
    }
}

/// One account sync at a time.
static SYNCING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Games played since their list was read, asked again per sync (the rest wait for the next one).
const XBOX_LISTS_PER_SYNC: usize = 8;

/// The Xbox account into the store: every game's progress in one question; the
/// first time, every list (a page per thousand achievements); after that, only
/// the lists of games played since they were read.
/// Syncs the Xbox account when due, and stamps how it went (synced, or
/// refused with a longer wait) while still holding the lock: the hub and the
/// library may both ask at once, and the one that finds it not due leaves
/// the stamp alone (it used to write "synced" over a refusal, and the wait was lost).
async fn sync_xbox(http: &reqwest::Client, data_dir: &Path, db: &mut Store, scope: &Scope, refresh: &str) -> Result<(), String> {
    let _one = SYNCING.lock().await;
    if !db.due(scope) {
        return Ok(());
    }
    let result = sync_xbox_now(http, data_dir, db, scope, refresh).await;
    stamp(db, scope, &result);
    result
}

/// A game whose list failed (a title the store answers 404 or 403 for, a
/// namespace without achievements) waits this long before it's asked again,
/// so it doesn't stop the account's other games every time, first in line.
const GAME_FAIL_WAIT: u64 = 6 * 3600;

fn failed_lately(db: &Store, scope: &Scope, key: &str, now: u64) -> bool {
    db.stamp(scope, &format!("fail:{key}")).is_some_and(|at| now < at + GAME_FAIL_WAIT)
}

/// Failures in one round: one game failing is that game's; a second one in a
/// row is the account (signed out, the store busy), and the round stops.
#[derive(Default)]
struct Misses(u32);

impl Misses {
    /// Notes a game's failure; whether the round should stop.
    fn game_failed(&mut self, db: &Store, scope: &Scope, key: &str, now: u64) -> bool {
        db.set_stamp(scope, &format!("fail:{key}"), now);
        self.0 += 1;
        self.0 >= 2
    }

    fn game_answered(&mut self) {
        self.0 = 0;
    }
}

/// How a sync went, in the store: synced, or refused (the wait grows).
fn stamp(db: &mut Store, scope: &Scope, result: &Result<(), String>) {
    match result {
        Ok(()) => db.synced(scope),
        Err(_) => db.refused(scope),
    }
}

async fn sync_xbox_now(http: &reqwest::Client, data_dir: &Path, db: &mut Store, scope: &Scope, refresh: &str) -> Result<(), String> {
    let session = xbox::session(http, refresh, |r| save_xbox_refresh(data_dir, refresh, r)).await?;
    let (games, last_played) = xbox::games(http, &session).await?;
    // Minutes played too: a hundred titles per question.
    let titles: Vec<String> = last_played.iter().map(|(_, title, _)| title.clone()).collect();
    let minutes = xbox::minutes(http, &session, &titles).await;
    // The library knows a game by its package; achievements are by title.
    for (pfn, title, _) in &last_played {
        if let Ok(title) = title.parse::<u64>() {
            db.set_stamp(scope, &format!("pfn:{}", pfn.to_lowercase()), title);
        }
    }
    db.keep_played(
        scope,
        &last_played.into_iter().map(|(pfn, title, last)| (pfn.to_lowercase(), minutes.get(&title).copied().unwrap_or(0) * 60, last)).collect::<Vec<_>>(),
    );
    if db.stamp(scope, "all_lists_v4").is_none() {
        let lists = xbox::all_lists(http, &session).await?;
        for g in &games {
            if let Some(list) = lists.get(&g.key) {
                db.keep_list(scope, &g.key, g.last_played.unwrap_or(0), list);
            }
        }
        db.set_stamp(scope, "all_lists_v4", store::now());
    } else {
        let now = store::now();
        let mut stale: Vec<&AchievementGame> = games
            .iter()
            .filter(|g| db.list(scope, &g.key).is_none_or(|(played, _)| g.last_played.unwrap_or(0) > played))
            .filter(|g| g.unlocked > 0 || g.total == 0)
            .filter(|g| !failed_lately(db, scope, &g.key, now))
            .collect();
        stale.sort_by(|a, b| b.last_played.cmp(&a.last_played));
        let mut misses = Misses::default();
        let mut stopped = None;
        for g in stale.into_iter().take(XBOX_LISTS_PER_SYNC) {
            match xbox::title_list(http, &session, &g.key).await {
                Ok(list) => {
                    misses.game_answered();
                    db.keep_list(scope, &g.key, g.last_played.unwrap_or(0), &list);
                }
                Err(e) if misses.game_failed(db, scope, &g.key, now) => {
                    stopped = Some(e);
                    break;
                }
                Err(_) => {}
            }
        }
        // What came is kept even when the round stopped.
        if let Some(e) = stopped {
            keep_xbox_games(db, scope, games);
            return Err(e);
        }
    }
    keep_xbox_games(db, scope, games);
    Ok(())
}

/// The account's games with the lists kept for them.
fn keep_xbox_games(db: &mut Store, scope: &Scope, games: Vec<AchievementGame>) {
    let games: Vec<AchievementGame> = games
        .into_iter()
        .map(|g| match db.list(scope, &g.key) {
            Some((_, list)) => xbox::with_list(g, &list),
            None => g,
        })
        .collect();
    db.keep_games(scope, &games);
}

/// The Epic session, kept for half an hour (Epic hands a new refresh token each time: it's saved).
static EPIC_SESSION: std::sync::Mutex<Option<(std::time::Instant, String, String)>> = std::sync::Mutex::new(None);

async fn epic_session(http: &reqwest::Client, data_dir: &Path) -> Result<(String, String), String> {
    if let Ok(kept) = EPIC_SESSION.lock() {
        if let Some((at, access, account)) = kept.as_ref() {
            if at.elapsed() < std::time::Duration::from_secs(1800) {
                return Ok((access.clone(), account.clone()));
            }
        }
    }
    let refresh = crate::settings::load(data_dir).epic_refresh.ok_or(tr!("Entre na Epic em Configurações › Epic Games primeiro."))?;
    let s = crate::epic_account::token(http, crate::epic_account::Grant::Refresh(&refresh)).await?;
    crate::settings::renew_token(data_dir, crate::settings::Account::Epic, &refresh, &s.refresh);
    // Signed out while asking: the session isn't kept.
    if crate::settings::load(data_dir).epic_refresh.as_deref() != Some(s.refresh.as_str()) {
        return Err(tr!("Entre na Epic em Configurações › Epic Games primeiro.").into());
    }
    if let Ok(mut kept) = EPIC_SESSION.lock() {
        *kept = Some((std::time::Instant::now(), s.access.clone(), s.account.clone()));
    }
    Ok((s.access, s.account))
}

/// Epic games asked per sync (two questions each); the rest wait for the next sync.
const EPIC_GAMES_PER_SYNC: usize = 40;

/// The Epic account into the store: play time of every game in one question;
/// then each game new to the store, or played since its list was read.
/// The Epic account's sync, stamped as `sync_xbox` is.
async fn sync_epic(http: &reqwest::Client, data_dir: &Path, db: &mut Store, scope: &Scope) -> Result<(), String> {
    let _one = SYNCING.lock().await;
    if !db.due(scope) {
        return Ok(());
    }
    let result = sync_epic_now(http, data_dir, db, scope).await;
    stamp(db, scope, &result);
    result
}

async fn sync_epic_now(http: &reqwest::Client, data_dir: &Path, db: &mut Store, scope: &Scope) -> Result<(), String> {
    let (access, account) = epic_session(http, data_dir).await?;
    let owned = crate::epic_account::cached(data_dir);
    let played = epic::playtimes(http, &access, &account).await.unwrap_or_default();
    let mut asked = 0;
    let mut games = Vec::new();
    // Games played first: the others have nothing unlocked yet.
    let mut owned = owned;
    owned.sort_by_key(|o| std::cmp::Reverse(played.get(&o.app_name).map_or(0, |p| p.0)));
    db.keep_played(scope, &played.iter().map(|(k, (s, l))| (k.clone(), *s, *l)).collect::<Vec<_>>());
    let now = store::now();
    let mut misses = Misses::default();
    for o in &owned {
        let secs = played.get(&o.app_name).map_or(0, |p| p.0);
        let kept = db.list(scope, &o.namespace);
        let fresh = kept.as_ref().is_some_and(|(at, _)| *at >= secs.max(1));
        // One that failed lately keeps what it had and lets the others go first.
        let list = if fresh || asked >= EPIC_GAMES_PER_SYNC || failed_lately(db, scope, &o.namespace, now) {
            kept.map(|(_, l)| l)
        } else {
            asked += 1;
            match epic::game_list(http, &access, &account, &o.namespace).await {
                Ok(list) => {
                    misses.game_answered();
                    db.keep_list(scope, &o.namespace, secs.max(1), &list);
                    Some(list)
                }
                Err(e) if misses.game_failed(db, scope, &o.namespace, now) => {
                    // The account, not a game: what came is kept, and the round stops.
                    db.keep_games(scope, &games);
                    return Err(e);
                }
                Err(_) => kept.map(|(_, l)| l),
            }
        };
        if let Some(list) = list.filter(|l| !l.is_empty()) {
            games.push(epic::entry(&o.namespace, &o.title, o.cover.as_deref(), secs, &list));
        }
    }
    db.keep_games(scope, &games);
    Ok(())
}

/// The Xbox and Epic accounts' play time on the library's games: the larger
/// time and the latest day played win over what this PC saw.
pub fn account_times(data_dir: &Path, games: &mut [cerberus_shared::Game]) {
    let chosen = crate::settings::load(data_dir);
    let Ok(db) = Store::open(data_dir) else { return };
    let epic = chosen.epic_user.map(|u| db.played(&Scope::new(AchievementSource::Epic, &u))).unwrap_or_default();
    let xbox = chosen.xbox_user.map(|u| db.played(&Scope::new(AchievementSource::Xbox, &u))).unwrap_or_default();
    for game in games.iter_mut() {
        let found = match &game.source {
            cerberus_shared::Source::Epic { app_name, .. } => epic.get(app_name),
            cerberus_shared::Source::Xbox { family, .. } => xbox.get(&family.to_lowercase()),
            _ => None,
        };
        if let Some(&(seconds, last)) = found {
            game.playtime_secs = game.playtime_secs.max(seconds);
            if last > 0 {
                game.last_played = Some(game.last_played.unwrap_or(0).max(last));
            }
        }
    }
}

/// The Xbox and Epic accounts brought up to date, when due (the library calls
/// this in the background). True when a store was asked.
pub async fn sync_accounts(http: &reqwest::Client, data_dir: &Path) -> bool {
    let chosen = crate::settings::load(data_dir);
    let Ok(mut db) = Store::open(data_dir) else { return false };
    let mut asked = false;
    if let (Some(refresh), Some(user)) = (chosen.xbox_refresh, chosen.xbox_user) {
        let scope = Scope::new(AchievementSource::Xbox, &user);
        if db.due(&scope) {
            asked = true;
            let _ = sync_xbox(http, data_dir, &mut db, &scope, &refresh).await;
        }
    }
    if let (Some(_), Some(user)) = (chosen.epic_refresh, chosen.epic_user) {
        let scope = Scope::new(AchievementSource::Epic, &user);
        if db.due(&scope) {
            asked = true;
            let _ = sync_epic(http, data_dir, &mut db, &scope).await;
        }
    }
    asked
}

pub async fn hub(
    http: &reqwest::Client,
    data_dir: &Path,
    retro_account: Option<(String, String)>,
    steam_api_account: Option<(String, String)>,
    online: bool,
) -> AchievementsHub {
    let mut hub = AchievementsHub::default();
    let (mut games, mut recent) = match tauri::async_runtime::spawn_blocking(steam_part).await {
        Ok(Some(part)) => {
            hub.steam = true;
            part
        }
        _ => (Vec::new(), Vec::new()),
    };
    if let Some((key, id)) = &steam_api_account {
        hub.steam = true;
        match steam_api_part(http, data_dir, key, id, games.clone(), recent.clone(), online).await {
            Ok((g, r, warning)) => {
                games = g;
                recent = r;
                if let Some(e) = warning {
                    hub.retry_soon = e == cerberus_shared::i18n::t(steam_api::BUSY);
                    hub.problems.push(tr!("Steam: {}", e));
                }
            }
            Err(e) if e == steam_api::OFFLINE => {}
            Err(e) => hub.problems.push(tr!("Steam: {}", e)),
        }
    }
    fill_names(http, data_dir, &mut games, &mut recent, online).await;
    let mut db = match Store::open(data_dir) {
        Ok(db) => db,
        Err(e) => {
            hub.problems.push(e);
            return hub;
        }
    };
    let mut scopes = Vec::new();
    if hub.steam {
        let scope = Scope::new(AchievementSource::Steam, "");
        db.keep_games(&scope, &games);
        let mut by_game: HashMap<String, Vec<cerberus_shared::Achievement>> = HashMap::new();
        for r in recent {
            by_game.entry(r.game_key).or_default().push(r.achievement);
        }
        for (game, list) in by_game {
            db.keep_unlocks(&scope, &game, &list);
        }
        scopes.push(scope);
    }
    if let Some((user, key)) = retro_account {
        hub.retro = true;
        let scope = Scope::new(AchievementSource::Retro, &user);
        let file = retro_file(data_dir, &user);
        let kept: Option<RetroKept> = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok());
        if online && db.due(&scope) {
            let (games, recent, points) = futures_util::join!(retro::games(http, &user, &key), retro::recent(http, &user, &key), retro::points(http, &user, &key));
            match (games, points) {
                (Ok(games), Ok(points)) => {
                    let got = RetroKept { games, recent: recent.unwrap_or_default(), points };
                    let _ = std::fs::create_dir_all(file.parent().unwrap_or(data_dir));
                    let _ = crate::state_file::write_bytes(&file, serde_json::to_string(&got).unwrap_or_default().as_bytes());
                    db.synced(&scope);
                    keep_retro(&mut db, &scope, &got);
                    (hub.retro_points, hub.retro_softcore) = got.points;
                }
                (Err(e), _) | (_, Err(e)) => {
                    db.refused(&scope);
                    hub.problems.push(tr!("RetroAchievements: {}", e));
                    if let Some(k) = &kept {
                        (hub.retro_points, hub.retro_softcore) = k.points;
                    }
                }
            }
        } else if let Some(k) = &kept {
            // What was read before the store existed comes in too (merging again changes nothing).
            keep_retro(&mut db, &scope, k);
            (hub.retro_points, hub.retro_softcore) = k.points;
        }
        scopes.push(scope);
    }
    let chosen = crate::settings::load(data_dir);
    if let (Some(refresh), Some(user)) = (chosen.xbox_refresh, chosen.xbox_user) {
        hub.xbox = true;
        let scope = Scope::new(AchievementSource::Xbox, &user);
        if online && db.due(&scope) {
            if let Err(e) = sync_xbox(http, data_dir, &mut db, &scope, &refresh).await {
                hub.problems.push(tr!("Xbox: {}", e));
            }
        }
        scopes.push(scope);
    }
    let chosen = crate::settings::load(data_dir);
    if let (Some(_), Some(user)) = (chosen.epic_refresh, chosen.epic_user) {
        hub.epic = true;
        let scope = Scope::new(AchievementSource::Epic, &user);
        if online && db.due(&scope) {
            if let Err(e) = sync_epic(http, data_dir, &mut db, &scope).await {
                hub.problems.push(tr!("Epic: {}", e));
            }
        }
        scopes.push(scope);
    }
    for scope in &scopes {
        // A game whose total isn't known yet (its list not read) waits for the next sync.
        hub.games.extend(db.games(scope).into_iter().filter(|g| g.total > 0));
    }
    hub.recent = db.recent(&scopes, RECENT);
    (hub.next, hub.rarest) = db.highlights(&scopes, 12);
    if let Some((key, _)) = &steam_api_account {
        fill_recent_icons(http, data_dir, key, &mut hub.recent, online).await;
    }
    hub
}

/// A Steam game's achievements from the Web API: the player's unlocks with the
/// game's icons and secret flags.
async fn steam_api_game(http: &reqwest::Client, data_dir: &Path, key: &str, id: &str, app: u32) -> Result<Option<(String, Vec<cerberus_shared::Achievement>, Option<steam_api::Owned>)>, String> {
    let owned = steam_api::owned(http, data_dir, key, id, true).await.ok().and_then(|list| list.into_iter().find(|o| o.app == app));
    let last_played = owned.as_ref().map(|o| o.last_played).unwrap_or(u64::MAX);
    let (mine, schema) = futures_util::join!(steam_api::mine(http, data_dir, key, id, app, last_played, true), steam_api::schema(http, data_dir, key, app, true));
    let mine = match mine {
        Ok(list) => list,
        Err(_) => steam_api::mine(http, data_dir, key, id, app, last_played, false).await?,
    };
    let schema = schema.unwrap_or_default();
    if mine.is_empty() && schema.is_empty() {
        return Ok(None);
    }
    let by_api = steam_api::by_api(&mine);
    let list: Vec<cerberus_shared::Achievement> = if schema.is_empty() {
        mine.iter()
            .map(|m| cerberus_shared::Achievement {
                id: m.api.clone(),
                name: m.name.clone(),
                description: m.description.clone(),
                icon: None,
                icon_locked: None,
                hidden: false,
                unlocked_at: m.achieved.then_some(m.unlocked_at),
                hardcore_at: None,
                points: 0,
                rarity: None,
            })
            .collect()
    } else {
        schema
            .iter()
            .map(|s| {
                let m = by_api.get(s.api.as_str());
                cerberus_shared::Achievement {
                    id: s.api.clone(),
                    name: if s.name.is_empty() { m.map(|m| m.name.clone()).unwrap_or_default() } else { s.name.clone() },
                    description: if s.description.is_empty() { m.map(|m| m.description.clone()).unwrap_or_default() } else { s.description.clone() },
                    icon: s.icon.clone(),
                    icon_locked: s.icon_locked.clone(),
                    hidden: s.hidden,
                    unlocked_at: m.filter(|m| m.achieved).map(|m| m.unlocked_at),
                    hardcore_at: None,
                    points: 0,
                    rarity: None,
                }
            })
            .collect()
    };
    let name = owned.as_ref().map(|o| o.name.clone()).unwrap_or_default();
    Ok(Some((name, list, owned)))
}

/// Achievements without a picture get Steam's local one, or the Web API's game
/// list (asked again: it's the call that fails when Steam is busy).
async fn fill_missing_icons(
    http: &reqwest::Client,
    data_dir: &Path,
    app: u32,
    list: &mut [cerberus_shared::Achievement],
    local: &HashMap<String, (Option<String>, Option<String>)>,
    api: Option<&(String, String)>,
) {
    for a in list.iter_mut().filter(|a| a.icon.is_none()) {
        if let Some((icon, locked)) = local.get(&a.id) {
            a.icon = icon.clone();
            a.icon_locked = locked.clone();
        }
    }
    let Some((key, _)) = api else { return };
    if list.iter().all(|a| a.icon.is_some()) {
        return;
    }
    let Ok(schema) = steam_api::schema(http, data_dir, key, app, true).await else { return };
    for a in list.iter_mut().filter(|a| a.icon.is_none()) {
        if let Some(s) = schema.iter().find(|s| s.api == a.id) {
            a.icon = s.icon.clone();
            a.icon_locked = s.icon_locked.clone();
        }
    }
}

/// One game's achievements, with how rare each one is.
pub async fn game(
    http: &reqwest::Client,
    data_dir: &Path,
    source: AchievementSource,
    key: &str,
    retro_account: Option<(String, String)>,
    steam_api_account: Option<(String, String)>,
) -> Result<GameAchievements, String> {
    match source {
        AchievementSource::Steam => {
            let app: u32 = key.parse().map_err(|_| tr!("Jogo inválido.").to_string())?;
            let read = tauri::async_runtime::spawn_blocking(move || {
                let root = crate::steam::root()?;
                let account = steam::account(&root)?;
                let played = account.localconfig.as_deref().map(steam::playtimes).unwrap_or_default();
                let (name, list) = steam::read_game(&account, app)?;
                Some((name, list, played.get(&app).copied()))
            })
            .await
            .map_err(|e| e.to_string())?;
            // The Web API, when turned on, knows unlocks made anywhere.
            let from_api = match &steam_api_account {
                Some((k, id)) => steam_api_game(http, data_dir, k, id, app).await.ok().flatten(),
                None => None,
            };
            // Pictures Steam keeps on this PC, for when the Web API answered without them.
            let local_icons: HashMap<String, (Option<String>, Option<String>)> = read
                .as_ref()
                .map(|(_, l, _)| l.iter().map(|a| (a.id.clone(), (a.icon.clone(), a.icon_locked.clone()))).collect())
                .unwrap_or_default();
            let (name, mut list, played) = match (from_api, read) {
                (Some((api_name, list, owned)), local) => {
                    let name = if api_name.is_empty() { local.as_ref().map(|l| l.0.clone()).unwrap_or_default() } else { api_name };
                    let played = owned
                        .map(|o| (o.minutes * 60, (o.last_played > 0).then_some(o.last_played)))
                        .or_else(|| local.and_then(|l| l.2));
                    (name, list, played)
                }
                (None, Some(local)) => local,
                (None, None) => return Err(tr!("A Steam não tem as conquistas desse jogo neste PC.").into()),
            };
            fill_missing_icons(http, data_dir, app, &mut list, &local_icons, steam_api_account.as_ref()).await;
            let rarity = steam::rarity(http, &data_dir.join("cache").join("achievements"), app).await;
            for a in &mut list {
                a.rarity = rarity.get(&a.id).copied();
            }
            let game = steam::game_entry(app, name, &list, played.as_ref());
            // Kept: the hub's next and rarest achievements come from the lists read.
            if let Ok(mut db) = Store::open(data_dir) {
                db.keep_list(&Scope::new(AchievementSource::Steam, ""), key, game.last_played.unwrap_or(0), &list);
            }
            Ok(GameAchievements { game, achievements: list, by_title: false })
        }
        AchievementSource::Retro => {
            let (user, key_) = retro_account.ok_or(tr!("Conecte sua conta em Configurações › RetroAchievements primeiro."))?;
            let (game, achievements) = retro::game(http, &user, &key_, key).await?;
            if let Ok(mut db) = Store::open(data_dir) {
                db.keep_list(&Scope::new(AchievementSource::Retro, &user), key, game.last_played.unwrap_or(0), &achievements);
            }
            Ok(GameAchievements { game, achievements, by_title: false })
        }
        AchievementSource::Epic => {
            let user = crate::settings::load(data_dir).epic_user.ok_or(tr!("Entre na Epic em Configurações › Epic Games primeiro."))?;
            let scope = Scope::new(AchievementSource::Epic, &user);
            let db = Store::open(data_dir)?;
            let game = db.game(&scope, key).ok_or(tr!("Esse jogo não está nas conquistas da Epic."))?;
            let achievements = db.list(&scope, key).map(|(_, l)| l).unwrap_or_default();
            Ok(GameAchievements { game, achievements, by_title: false })
        }
        AchievementSource::Xbox => {
            let chosen = crate::settings::load(data_dir);
            let (Some(refresh), Some(user)) = (chosen.xbox_refresh, chosen.xbox_user) else {
                return Err(tr!("Entre com a conta Microsoft em Configurações › Xbox primeiro.").into());
            };
            let scope = Scope::new(AchievementSource::Xbox, &user);
            let mut db = Store::open(data_dir)?;
            // From the library: "pfn:<package family>".
            let title = match key.strip_prefix("pfn:") {
                Some(pfn) => db.stamp(&scope, &format!("pfn:{}", pfn.to_lowercase())).map(|t| t.to_string()).ok_or(tr!("Esse jogo não está nas conquistas do Xbox."))?,
                None => key.to_string(),
            };
            let key = title.as_str();
            let game = db.game(&scope, key).ok_or(tr!("Esse jogo não está nas conquistas do Xbox."))?;
            let played = game.last_played.unwrap_or(0);
            let list = match db.list(&scope, key) {
                Some((at, list)) if at >= played => list,
                kept => {
                    let fresh = async {
                        let session = xbox::session(http, &refresh, |r| save_xbox_refresh(data_dir, &refresh, r)).await?;
                        xbox::title_list(http, &session, key).await
                    }
                    .await;
                    match (fresh, kept) {
                        (Ok(list), _) => {
                            db.keep_list(&scope, key, played, &list);
                            list
                        }
                        (Err(_), Some((_, list))) => list,
                        (Err(e), None) => return Err(e),
                    }
                }
            };
            let game = xbox::with_list(game, &list);
            db.keep_games(&scope, std::slice::from_ref(&game));
            Ok(GameAchievements { game, achievements: list, by_title: false })
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_failing_game_waits_and_a_second_failure_in_a_row_stops_the_round() {
        use super::*;
        let dir = tempfile::tempdir().unwrap();
        let db = Store::open(dir.path()).unwrap();
        let scope = Scope::new(AchievementSource::Epic, "player");
        let now = 1_000_000;
        let mut misses = Misses::default();
        assert!(!misses.game_failed(&db, &scope, "broken", now), "one game failing doesn't stop the round");
        assert!(failed_lately(&db, &scope, "broken", now + 60), "it waits");
        assert!(!failed_lately(&db, &scope, "broken", now + GAME_FAIL_WAIT), "and is asked again later");
        assert!(!failed_lately(&db, &scope, "fine", now));
        misses.game_answered();
        assert!(!misses.game_failed(&db, &scope, "other", now), "an answer in between: still one");
        assert!(misses.game_failed(&db, &scope, "third", now), "two in a row: the account");
    }

    #[test]
    fn skipped_sync_keeps_the_wait() {
        use super::*;
        let dir = tempfile::tempdir().unwrap();
        let mut db = Store::open(dir.path()).unwrap();
        let scope = Scope::new(AchievementSource::Xbox, "player");
        // The hub's sync was just refused: it waits.
        db.refused(&scope);
        let (wait, failures) = (db.stamp(&scope, "wait_until"), db.stamp(&scope, "failures"));
        // The library asks meanwhile: not due, so it goes nowhere and stamps nothing.
        let http = reqwest::Client::new();
        assert!(tauri::async_runtime::block_on(sync_xbox(&http, dir.path(), &mut db, &scope, "token")).is_ok());
        assert_eq!((db.stamp(&scope, "wait_until"), db.stamp(&scope, "failures")), (wait, failures), "the wait is kept");
        assert!(db.stamp(&scope, "synced").is_none(), "not stamped as synced");
    }

    /// Reads the Steam of the PC running the tests (read only): `cargo test -- --ignored real_steam`.
    #[test]
    #[ignore = "reads this PC's Steam"]
    fn real_steam_hub_reads_this_pc() {
        let start = std::time::Instant::now();
        let (games, recent) = super::steam_part().expect("Steam with achievement data");
        let unlocked: u32 = games.iter().map(|g| g.unlocked).sum();
        let total: u32 = games.iter().map(|g| g.total).sum();
        let with_time = games.iter().filter(|g| g.playtime_secs.is_some()).count();
        println!("{} games, {unlocked}/{total} unlocked, {} with Steam play time, {} unlocks with a date, in {:?}", games.len(), with_time, recent.len(), start.elapsed());
        let mut best: Vec<_> = games.iter().filter(|g| g.unlocked > 0).collect();
        best.sort_by_key(|g| std::cmp::Reverse(g.percent()));
        for g in best.iter().take(5) {
            println!("  {} · {}/{} ({}%)", g.name, g.unlocked, g.total, g.percent());
        }
        assert!(!games.is_empty());
    }
}

