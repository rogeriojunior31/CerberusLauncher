//! The achievements hub: Steam (from the Steam client's files on this PC) and
//! RetroAchievements (from its Web API), side by side.

pub mod retro;
pub mod steam;
pub mod steam_api;

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

/// Names Steam's cache doesn't have, asked once of the Steam store and kept on disk.
async fn fill_names(http: &reqwest::Client, data_dir: &Path, games: &mut [AchievementGame], recent: &mut [RecentUnlock]) {
    let file = data_dir.join("cache").join("achievements").join("steam-names.json");
    let mut known: HashMap<String, String> = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let nameless = |g: &AchievementGame| g.source == AchievementSource::Steam && g.name == format!("App {}", g.key);
    let missing: Vec<String> = games.iter().filter(|g| nameless(g) && !known.contains_key(&g.key)).map(|g| g.key.clone()).collect();
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

/// How many games' achievements are asked of Steam's API at once.
const AT_ONCE: usize = 8;

/// Steam's part from the Web API: every owned game with achievements. Games the
/// API has are taken from it (it knows unlocks made on other PCs); the local
/// cache still fills icons, and keeps games the API doesn't list.
async fn steam_api_part(
    http: &reqwest::Client,
    data_dir: &Path,
    key: &str,
    id: &str,
    local: Vec<AchievementGame>,
    local_recent: Vec<RecentUnlock>,
) -> Result<(Vec<AchievementGame>, Vec<RecentUnlock>), String> {
    use futures_util::StreamExt;
    let owned = steam_api::owned(http, data_dir, key, id).await?;
    let local_keys: std::collections::HashSet<String> = local.iter().map(|g| g.key.clone()).collect();
    let wanted: Vec<steam_api::Owned> = owned.into_iter().filter(|o| o.stats && (o.minutes > 0 || local_keys.contains(&o.app.to_string()))).collect();
    let mut asks = futures_util::stream::iter(wanted.into_iter().map(|o| async move {
        let list = steam_api::mine(http, data_dir, key, id, o.app, o.last_played).await;
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
    let mut first_error = None;
    let mut from_api = std::collections::HashSet::new();
    while let Some((o, list)) = asks.next().await {
        let list = match list {
            Ok(list) => list,
            Err(e) => {
                first_error.get_or_insert(e);
                continue;
            }
        };
        if list.is_empty() {
            continue;
        }
        let app = o.app.to_string();
        from_api.insert(app.clone());
        let unlocked: Vec<&steam_api::Mine> = list.iter().filter(|m| m.achieved).collect();
        let all = unlocked.len() == list.len();
        let last = unlocked.iter().map(|m| m.unlocked_at).filter(|t| *t > 0).max();
        let name = if o.name.trim().is_empty() { local.iter().find(|g| g.key == app).map(|g| g.name.clone()).unwrap_or_else(|| format!("App {app}")) } else { o.name.clone() };
        for m in &unlocked {
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
            total: list.len() as u32,
            unlocked: unlocked.len() as u32,
            hardcore: 0,
            points: 0,
            points_total: 0,
            last_unlock: last,
            playtime_secs: (o.minutes > 0).then_some(o.minutes * 60),
            last_played: (o.last_played > 0).then_some(o.last_played),
            award: all.then_some(cerberus_shared::GameAward::Completed),
            award_at: if all { last } else { None },
        });
    }
    // Nothing came back at all: a private profile or a key problem; say so.
    if games.is_empty() {
        if let Some(e) = first_error {
            return Err(e);
        }
    }
    // Games only the local cache knows (family sharing, a game the API skipped).
    games.extend(local.into_iter().filter(|g| !from_api.contains(&g.key)));
    recent.extend(local_recent.into_iter().filter(|r| !from_api.contains(&r.game_key)));
    Ok((games, recent))
}

/// Icons for recent Steam unlocks the local cache didn't have, from the game's
/// schema (asked once per game and kept).
async fn fill_recent_icons(http: &reqwest::Client, data_dir: &Path, key: &str, recent: &mut [RecentUnlock]) {
    let games: std::collections::BTreeSet<String> =
        recent.iter().filter(|r| r.source == AchievementSource::Steam && r.achievement.icon.is_none()).map(|r| r.game_key.clone()).collect();
    for game in games {
        let Ok(app) = game.parse::<u32>() else { continue };
        let Ok(schema) = steam_api::schema(http, data_dir, key, app).await else { continue };
        for r in recent.iter_mut().filter(|r| r.source == AchievementSource::Steam && r.game_key == game) {
            if let Some(s) = schema.iter().find(|s| s.api == r.achievement.id) {
                r.achievement.icon = s.icon.clone();
                r.achievement.icon_locked = s.icon_locked.clone();
            }
        }
    }
}

/// Everything the hub shows. A source that can't be read leaves a note in `problems`.
pub async fn hub(
    http: &reqwest::Client,
    data_dir: &Path,
    retro_account: Option<(String, String)>,
    steam_api_account: Option<(String, String)>,
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
        match steam_api_part(http, data_dir, key, id, games.clone(), recent.clone()).await {
            Ok((g, r)) => {
                games = g;
                recent = r;
            }
            Err(e) => hub.problems.push(tr!("Steam: {}", e)),
        }
    }
    fill_names(http, data_dir, &mut games, &mut recent).await;
    hub.games.extend(games);
    hub.recent.extend(recent);
    if let Some((user, key)) = retro_account {
        hub.retro = true;
        let (games, recent, points) = futures_util::join!(retro::games(http, &user, &key), retro::recent(http, &user, &key), retro::points(http, &user, &key));
        match games {
            Ok(list) => hub.games.extend(list),
            Err(e) => hub.problems.push(tr!("RetroAchievements: {}", e)),
        }
        if let Ok(list) = recent {
            hub.recent.extend(list);
        }
        match points {
            Ok((hardcore, softcore)) => {
                hub.retro_points = hardcore;
                hub.retro_softcore = softcore;
            }
            Err(e) => hub.problems.push(tr!("RetroAchievements: {}", e)),
        }
    }
    hub.recent.sort_by(|a, b| b.achievement.unlocked_at.cmp(&a.achievement.unlocked_at));
    hub.recent.truncate(RECENT);
    if let Some((key, _)) = &steam_api_account {
        fill_recent_icons(http, data_dir, key, &mut hub.recent).await;
    }
    hub
}

/// A Steam game's achievements from the Web API: the player's unlocks with the
/// game's icons and secret flags.
async fn steam_api_game(http: &reqwest::Client, data_dir: &Path, key: &str, id: &str, app: u32) -> Result<Option<(String, Vec<cerberus_shared::Achievement>, Option<steam_api::Owned>)>, String> {
    let owned = steam_api::owned(http, data_dir, key, id).await.ok().and_then(|list| list.into_iter().find(|o| o.app == app));
    let last_played = owned.as_ref().map(|o| o.last_played).unwrap_or(u64::MAX);
    let (mine, schema) = futures_util::join!(steam_api::mine(http, data_dir, key, id, app, last_played), steam_api::schema(http, data_dir, key, app));
    let mine = mine?;
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
