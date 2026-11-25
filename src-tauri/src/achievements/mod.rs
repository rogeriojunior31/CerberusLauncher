//! The achievements hub: Steam (from the Steam client's files on this PC) and
//! RetroAchievements (from its Web API), side by side.

pub mod classic;
pub mod live;
pub mod retro;
pub mod steam;
pub mod steam_api;
pub mod xbox;

/// Between two syncs of the same source, at least this: the page opened again
/// (or the library listed) shows what's on disk instead of asking again.
const SYNC_EVERY: std::time::Duration = std::time::Duration::from_secs(300);

/// The file was written within `SYNC_EVERY`.
fn recent_file(file: &Path) -> bool {
    std::fs::metadata(file).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age < SYNC_EVERY)
}

/// The Xbox account's refresh token, renewed in the settings when Xbox Live hands a new one.
fn save_xbox_refresh(data_dir: &Path, refresh: &str) {
    let mut chosen = crate::settings::load(data_dir);
    if chosen.xbox_refresh.as_deref() != Some(refresh) {
        chosen.xbox_refresh = Some(refresh.to_string());
        let _ = crate::settings::save(data_dir, &chosen);
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
        let _ = std::fs::write(&file, serde_json::to_string(&known).unwrap_or_default());
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
    hub.games.extend(games);
    hub.recent.extend(recent);
    if let Some((user, key)) = retro_account {
        hub.retro = true;
        let file = retro_file(data_dir, &user);
        let kept: Option<RetroKept> = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok());
        // Read a few minutes ago: no need to ask RetroAchievements again yet.
        let fresh = if online && !(kept.is_some() && recent_file(&file)) {
            let (games, recent, points) = futures_util::join!(retro::games(http, &user, &key), retro::recent(http, &user, &key), retro::points(http, &user, &key));
            match (games, points) {
                (Ok(games), Ok(points)) => {
                    let got = RetroKept { games, recent: recent.unwrap_or_default(), points };
                    let _ = std::fs::create_dir_all(file.parent().unwrap_or(data_dir));
                    let _ = std::fs::write(&file, serde_json::to_string(&got).unwrap_or_default());
                    Some(got)
                }
                (Err(e), _) | (_, Err(e)) => {
                    hub.problems.push(tr!("RetroAchievements: {}", e));
                    None
                }
            }
        } else {
            None
        };
        if let Some(r) = fresh.or(kept) {
            hub.games.extend(r.games);
            hub.recent.extend(r.recent);
            (hub.retro_points, hub.retro_softcore) = r.points;
        }
    }
    if let Some(refresh) = crate::settings::load(data_dir).xbox_refresh {
        hub.xbox = true;
        let file = xbox::hub_file(data_dir);
        let kept: Option<xbox::HubKept> = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok());
        let fresh = if online && !(kept.is_some() && recent_file(&file)) {
            let got = async {
                let session = xbox::session(http, &refresh, |r| save_xbox_refresh(data_dir, r)).await?;
                let games = xbox::games(http, &session).await?;
                let recent = match xbox::recent(http, &session, &games).await {
                    Ok(list) => list,
                    Err(e) => {
                        eprintln!("xbox recent: {e}");
                        Vec::new()
                    }
                };
                Ok::<_, String>(xbox::HubKept { games, recent })
            }
            .await;
            match got {
                Ok(got) => {
                    let _ = std::fs::create_dir_all(file.parent().unwrap_or(data_dir));
                    let _ = std::fs::write(&file, serde_json::to_string(&got).unwrap_or_default());
                    Some(got)
                }
                Err(e) => {
                    hub.problems.push(tr!("Xbox: {}", e));
                    None
                }
            }
        } else {
            None
        };
        if let Some(x) = fresh.or(kept) {
            hub.games.extend(x.games);
            hub.recent.extend(x.recent);
        }
    }
    hub.recent.sort_by(|a, b| b.achievement.unlocked_at.cmp(&a.achievement.unlocked_at));
    hub.recent.truncate(RECENT);
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
            Ok(GameAchievements { game, achievements: list })
        }
        AchievementSource::Retro => {
            let (user, key_) = retro_account.ok_or(tr!("Conecte sua conta em Configurações › RetroAchievements primeiro."))?;
            let (game, achievements) = retro::game(http, &user, &key_, key).await?;
            Ok(GameAchievements { game, achievements })
        }
        AchievementSource::Xbox => {
            let refresh = crate::settings::load(data_dir).xbox_refresh.ok_or(tr!("Entre com a conta Microsoft em Configurações › Xbox primeiro."))?;
            // The game as the hub last read it: its progress, and when it was played.
            let kept: Option<xbox::HubKept> = std::fs::read_to_string(xbox::hub_file(data_dir)).ok().and_then(|t| serde_json::from_str(&t).ok());
            let mut game = kept.and_then(|k| k.games.into_iter().find(|g| g.key == key)).ok_or(tr!("Esse jogo não está nas conquistas do Xbox."))?;
            let session = xbox::session(http, &refresh, |r| save_xbox_refresh(data_dir, r)).await?;
            let achievements = xbox::game_list(http, data_dir, &session, key, game.last_played).await?;
            game.unlocked = achievements.iter().filter(|a| a.unlocked_at.is_some()).count() as u32;
            game.total = game.total.max(achievements.len() as u32);
            game.last_unlock = achievements.iter().filter_map(|a| a.unlocked_at).max();
            Ok(GameAchievements { game, achievements })
        }
    }
}

#[cfg(test)]
mod tests {
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
