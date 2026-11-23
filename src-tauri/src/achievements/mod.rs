//! The achievements hub: Steam (from the Steam client's files on this PC) and
//! RetroAchievements (from its Web API), side by side.

pub mod retro;
pub mod steam;

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

/// Everything the hub shows. A source that can't be read leaves a note in `problems`.
pub async fn hub(http: &reqwest::Client, data_dir: &Path, retro_account: Option<(String, String)>) -> AchievementsHub {
    let mut hub = AchievementsHub::default();
    if let Ok(Some((mut games, mut recent))) = tauri::async_runtime::spawn_blocking(steam_part).await {
        fill_names(http, data_dir, &mut games, &mut recent).await;
        hub.steam = true;
        hub.games.extend(games);
        hub.recent.extend(recent);
    }
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
    hub
}

/// One game's achievements, with how rare each one is.
pub async fn game(
    http: &reqwest::Client,
    data_dir: &Path,
    source: AchievementSource,
    key: &str,
    retro_account: Option<(String, String)>,
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
            let (name, mut list, played) = read.ok_or(tr!("A Steam não tem as conquistas desse jogo neste PC."))?;
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
