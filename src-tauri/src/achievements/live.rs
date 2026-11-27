//! Steam achievements the moment they unlock. While a Steam game runs, the
//! Steam client rewrites its `UserGameStats_<account>_<app>.bin` when the game
//! stores an unlock; that file is checked every few seconds and what's new
//! becomes a notification, saying when one is rare and when the game is complete.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::SystemTime;

use cerberus_shared::{Achievement, UnlockKind, UnlockNotice, tk, tr};
use tauri::{AppHandle, Emitter, Manager};

use super::steam;

/// Below this share of players, an achievement is called rare.
const RARE: f32 = 10.0;

struct Watched {
    file: PathBuf,
    changed: Option<SystemTime>,
    unlocked: HashSet<String>,
    rarity: HashMap<String, f32>,
}

/// The Steam games being watched, by app id.
#[derive(Default)]
pub struct Live {
    games: HashMap<u32, Watched>,
}

fn changed(file: &PathBuf) -> Option<SystemTime> {
    std::fs::metadata(file).and_then(|m| m.modified()).ok()
}

fn unlocked_ids(list: &[Achievement]) -> HashSet<String> {
    list.iter().filter(|a| a.unlocked_at.is_some()).map(|a| a.id.clone()).collect()
}

/// What changed between two readings: the new unlocks, and whether the game is now complete.
pub fn news<'a>(before: &HashSet<String>, now: &'a [Achievement]) -> (Vec<&'a Achievement>, bool) {
    let fresh: Vec<&Achievement> = now.iter().filter(|a| a.unlocked_at.is_some() && !before.contains(&a.id)).collect();
    let complete = !fresh.is_empty() && !now.is_empty() && now.iter().all(|a| a.unlocked_at.is_some());
    (fresh, complete)
}

/// Tells new unlocks of any store (their rarity, when known, from the list
/// itself), and the 100% when the last one came; notes each one in the trail.
pub fn tell(app: &AppHandle, game_id: &str, game: &str, fresh: &[&Achievement], complete: bool) {
    for a in fresh {
        let rare = a.rarity.filter(|r| *r < RARE);
        let title = match rare {
            Some(r) => tr!("Conquista rara desbloqueada ({}% dos jogadores)", format!("{r:.1}")),
            None => tr!("Conquista desbloqueada").to_string(),
        };
        crate::unlock_toast::show(
            app,
            UnlockNotice { title, name: a.name.clone(), description: a.description.clone(), icon: a.icon.clone(), kind: if rare.is_some() { UnlockKind::Rare } else { UnlockKind::Normal } },
        );
        crate::trail::note(app, game_id, tk!("Conquista"), &Ok(a.name.clone()));
    }
    if complete {
        crate::unlock_toast::show(
            app,
            UnlockNotice { title: tr!("100% das conquistas!").to_string(), name: game.to_string(), description: tr!("Você completou {}.", game), icon: None, kind: UnlockKind::Complete },
        );
    }
}

impl Live {
    /// Starts watching a game that just opened: what's unlocked now is the baseline.
    pub fn start(&mut self, app: &AppHandle, app_id: u32) {
        let Some(account) = crate::steam::root().and_then(|root| steam::account(&root)) else { return };
        let file = account.stats_dir.join(format!("UserGameStats_{}_{app_id}.bin", account.id));
        let unlocked = steam::read_game(&account, app_id).map(|(_, list)| unlocked_ids(&list)).unwrap_or_default();
        // How rare each one is, from the cache or Steam (no key needed); a few KB once a week.
        let rarity = match app.path().app_data_dir() {
            Ok(data) => {
                let http = app.state::<reqwest::Client>().inner().clone();
                let dir = data.join("cache").join("achievements");
                tauri::async_runtime::block_on(async move { steam::rarity(&http, &dir, app_id).await })
            }
            Err(_) => HashMap::new(),
        };
        self.games.insert(app_id, Watched { changed: changed(&file), file, unlocked, rarity });
    }

    pub fn stop(&mut self, app_id: u32) {
        self.games.remove(&app_id);
    }

    /// Looks for new unlocks in every watched game.
    pub fn check(&mut self, app: &AppHandle) {
        if self.games.is_empty() {
            return;
        }
        let Some(account) = crate::steam::root().and_then(|root| steam::account(&root)) else { return };
        for (&app_id, w) in self.games.iter_mut() {
            let now = changed(&w.file);
            if now == w.changed {
                continue;
            }
            w.changed = now;
            let Some((name, list)) = steam::read_game(&account, app_id) else { continue };
            let (fresh, complete) = news(&w.unlocked, &list);
            let game = if name.trim().is_empty() { format!("App {app_id}") } else { name };
            for a in &fresh {
                let rare = w.rarity.get(&a.id).copied().filter(|r| *r < RARE);
                let title = match rare {
                    Some(r) => tr!("Conquista rara desbloqueada ({}% dos jogadores)", format!("{r:.1}")),
                    None => tr!("Conquista desbloqueada").to_string(),
                };
                crate::unlock_toast::show(
                    app,
                    UnlockNotice {
                        title,
                        name: a.name.clone(),
                        description: a.description.clone(),
                        icon: a.icon.clone(),
                        kind: if rare.is_some() { UnlockKind::Rare } else { UnlockKind::Normal },
                    },
                );
                crate::trail::note(app, &format!("steam:{app_id}"), tk!("Conquista"), &Ok(a.name.clone()));
            }
            if complete {
                crate::unlock_toast::show(
                    app,
                    UnlockNotice {
                        title: tr!("100% das conquistas!").to_string(),
                        name: game.clone(),
                        description: tr!("Você completou {}.", game),
                        icon: None,
                        kind: UnlockKind::Complete,
                    },
                );
            }
            if !fresh.is_empty() {
                let _ = app.emit("achievements-changed", app_id);
            }
            w.unlocked = unlocked_ids(&list);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ach(id: &str, unlocked: bool) -> Achievement {
        Achievement {
            id: id.into(),
            name: id.into(),
            description: String::new(),
            icon: None,
            icon_locked: None,
            hidden: false,
            unlocked_at: unlocked.then_some(100),
            hardcore_at: None,
            points: 0,
            rarity: None,
        }
    }

    #[test]
    fn only_new_unlocks_are_told_and_the_last_one_completes_the_game() {
        let before: HashSet<String> = ["a".to_string()].into();
        let now = vec![ach("a", true), ach("b", true), ach("c", false)];
        let (fresh, complete) = news(&before, &now);
        assert_eq!(fresh.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["b"]);
        assert!(!complete);

        let before: HashSet<String> = ["a".to_string(), "b".to_string()].into();
        let now = vec![ach("a", true), ach("b", true), ach("c", true)];
        let (fresh, complete) = news(&before, &now);
        assert_eq!(fresh.len(), 1);
        assert!(complete);

        // Nothing new: not "complete" again on every reading.
        let all: HashSet<String> = ["a", "b", "c"].map(String::from).into();
        assert_eq!(news(&all, &now), (vec![], false));
    }
}
