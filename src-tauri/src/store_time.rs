//! Playtime for Steam and Epic games, which their own launchers start. As in
//! Hydra, Cerberus looks every few seconds for a running program inside a
//! game's install folder; while one runs the game counts as played, and its
//! time is kept in `store-playtime.json` (those games are read from the stores
//! each time, so the library can't hold it).

use cerberus_shared::tk;
use cerberus_shared::tr;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use cerberus_shared::{Game, Source};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Played {
    pub seconds: u64,
    pub last: u64,
    #[serde(default)]
    pub last_session: u64,
}

fn file(data: &Path) -> PathBuf {
    data.join("store-playtime.json")
}

pub fn load(data: &Path) -> HashMap<String, Played> {
    fs::read_to_string(file(data)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn add(data: &Path, id: &str, seconds: u64, now: u64) -> Result<(), String> {
    let mut all = load(data);
    let entry = all.entry(id.to_string()).or_default();
    entry.seconds += seconds;
    entry.last = now;
    entry.last_session = seconds;
    let text = serde_json::to_string_pretty(&all).map_err(|e| e.to_string())?;
    let target = file(data);
    let temp = target.with_extension("json.tmp");
    fs::write(&temp, text).map_err(|e| e.to_string())?;
    fs::rename(&temp, &target).map_err(|e| e.to_string())
}

/// Puts the kept time on Steam and Epic games as they are listed.
pub fn apply(data: &Path, games: &mut [Game]) {
    let all = load(data);
    for game in games.iter_mut().filter(|g| is_store(g)) {
        if let Some(p) = all.get(&game.id) {
            game.playtime_secs = p.seconds;
            game.last_played = (p.last > 0).then_some(p.last);
            game.last_session_secs = (p.last_session > 0).then_some(p.last_session);
        }
    }
}

fn is_store(game: &Game) -> bool {
    matches!(game.source, Source::Steam { .. } | Source::Epic { .. })
}

/// A folder as compared: lower case, backslashes, ending in one.
fn folder_key(path: &str) -> String {
    let mut key = path.replace('/', "\\").to_lowercase();
    while key.ends_with('\\') {
        key.pop();
    }
    key.push('\\');
    key
}

/// The games (by id) that have a running program inside their folder.
fn running(programs: &[String], games: &[(String, String)]) -> HashSet<String> {
    let programs: Vec<String> = programs.iter().map(|p| p.replace('/', "\\").to_lowercase()).collect();
    games
        .iter()
        .filter(|(_, dir)| !dir.trim().is_empty())
        .filter(|(_, dir)| {
            let key = folder_key(dir);
            programs.iter().any(|p| p.starts_with(&key))
        })
        .map(|(id, _)| id.clone())
        .collect()
}

#[cfg(windows)]
pub fn running_programs() -> Vec<String> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::ProcessStatus::EnumProcesses;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW};

    let mut pids = vec![0u32; 4096];
    let mut used = 0u32;
    if unsafe { EnumProcesses(pids.as_mut_ptr(), (pids.len() * 4) as u32, &mut used) } == 0 {
        return Vec::new();
    }
    pids.truncate(used as usize / 4);
    let mut found = Vec::new();
    let mut buffer = vec![0u16; 32768];
    for pid in pids.into_iter().filter(|&p| p != 0) {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            continue;
        }
        let mut len = buffer.len() as u32;
        if unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut len) } != 0 {
            found.push(OsString::from_wide(&buffer[..len as usize]).to_string_lossy().into_owned());
        }
        unsafe { CloseHandle(handle) };
    }
    found
}

#[cfg(not(windows))]
pub fn running_programs() -> Vec<String> {
    Vec::new()
}

/// Watches for Steam and Epic games while Cerberus runs (even in the tray).
pub fn watch(app: AppHandle) {
    std::thread::spawn(move || {
        let mut games: Vec<(String, String, String)> = Vec::new();
        let mut scanned = Instant::now() - Duration::from_secs(3600);
        let mut playing: HashMap<String, Instant> = HashMap::new();
        let mut live = crate::achievements::live::Live::default();
        let steam_app = |id: &str| id.strip_prefix("steam:").and_then(|n| n.parse::<u32>().ok());
        loop {
            // The stores' lists change rarely; reading them each minute is plenty.
            if scanned.elapsed() >= Duration::from_secs(60) {
                let mut found = crate::steam::installed_games();
                found.extend(crate::epic::installed_games());
                let before: HashSet<String> = games.iter().map(|(id, _, _)| id.clone()).collect();
                games = found.into_iter().map(|g| (g.id, g.install_dir, g.name)).collect();
                // A game finished installing (or was removed) in its store: the library shows it.
                let now: HashSet<String> = games.iter().map(|(id, _, _)| id.clone()).collect();
                if !before.is_empty() && before != now {
                    let _ = app.emit("catalog-changed", ());
                }
                scanned = Instant::now();
            }
            let dirs: Vec<(String, String)> = games.iter().map(|(id, dir, _)| (id.clone(), dir.clone())).collect();
            let now_running = if dirs.is_empty() { HashSet::new() } else { running(&running_programs(), &dirs) };
            for id in &now_running {
                if !playing.contains_key(id) {
                    playing.insert(id.clone(), Instant::now());
                    crate::in_game::session(&app, true);
                    let _ = app.emit(crate::sessions::EVENT, cerberus_shared::SessionEvent { game_id: id.clone(), running: true });
                    if let Some(n) = steam_app(id) {
                        live.start(&app, n);
                    }
                }
            }
            let ended: Vec<String> = playing.keys().filter(|id| !now_running.contains(*id)).cloned().collect();
            for id in ended {
                let Some(started) = playing.remove(&id) else { continue };
                if let Some(n) = steam_app(&id) {
                    // Steam may store the last unlock as the game closes.
                    live.check(&app);
                    live.stop(n);
                }
                let seconds = started.elapsed().as_secs();
                crate::in_game::session(&app, false);
                let _ = app.emit(crate::sessions::EVENT, cerberus_shared::SessionEvent { game_id: id.clone(), running: false });
                // A blink (a launcher checking for updates) isn't a session.
                if seconds < 30 {
                    continue;
                }
                let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default();
                let Ok(data) = app.path().app_data_dir() else { continue };
                let saved = add(&data, &id, seconds, now);
                crate::play_log::record(&data, &id, seconds, now);
                let minutes = if seconds < 60 { tr!("Menos de 1 min jogado").to_string() } else { crate::sessions::played_text(seconds / 60 * 60) };
                crate::trail::note(&app, &id, tk!("Partida"), &saved.map(|_| minutes));
                let _ = app.emit("playtime-updated", &id);
            }
            live.check(&app);
            // Downloads slow down while any game is open (store games or ones Cerberus started).
            let ours = !app.state::<crate::sessions::Sessions>().running().is_empty();
            crate::recomp::throttle::set_playing(ours || !playing.is_empty());
            std::thread::sleep(Duration::from_secs(5));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_game_runs_when_a_program_inside_its_folder_does() {
        let games = vec![
            ("steam:1".to_string(), r"D:\SteamLibrary\steamapps\common\Balatro".to_string()),
            ("steam:2".to_string(), r"D:\SteamLibrary\steamapps\common\Balatro 2\".to_string()),
            ("epic:x".to_string(), String::new()),
        ];
        let programs = vec![r"d:\steamlibrary\steamapps\common\balatro\Balatro.exe".to_string(), r"C:\Windows\explorer.exe".to_string()];
        let found = running(&programs, &games);
        // "Balatro 2" isn't running just because "Balatro" is: folders end at a separator.
        assert_eq!(found.into_iter().collect::<Vec<_>>(), ["steam:1"]);
    }

    #[cfg(windows)]
    #[test]
    fn the_running_programs_include_this_one_with_its_full_path() {
        let me = std::env::current_exe().unwrap().to_string_lossy().to_lowercase();
        let all = running_programs();
        assert!(all.len() > 5, "only {} programs listed", all.len());
        assert!(all.iter().any(|p| p.to_lowercase() == me));
        // So a game in this folder would be seen running.
        let dir = std::env::current_exe().unwrap().parent().unwrap().to_string_lossy().into_owned();
        assert_eq!(running(&all, &[("me".into(), dir)]).len(), 1);
    }

    #[test]
    fn kept_time_adds_up_and_reaches_the_listed_game() {
        let dir = tempfile::tempdir().unwrap();
        add(dir.path(), "steam:1", 600, 100).unwrap();
        add(dir.path(), "steam:1", 120, 200).unwrap();
        let mut games: Vec<Game> = vec![serde_json::from_value(serde_json::json!({
            "id": "steam:1", "name": "Balatro", "source": {"kind": "steam", "app_id": 1}, "install_dir": "x"
        }))
        .unwrap()];
        apply(dir.path(), &mut games);
        assert_eq!((games[0].playtime_secs, games[0].last_played, games[0].last_session_secs), (720, Some(200), Some(120)));
    }
}
