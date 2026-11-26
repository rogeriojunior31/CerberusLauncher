//! Playtime for Steam and Epic games, which their own launchers start. As in
//! Hydra, Cerberus looks every few seconds for a running program inside a
//! game's install folder; while one runs the game counts as played, and its
//! time is kept in `store-playtime.json` (those games are read from the stores
//! each time, so the library can't hold it).

use cerberus_shared::tk;
use cerberus_shared::tr;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
// Hours the PC slept with the game open aren't played.
use crate::awake_time::AwakeInstant as Instant;

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

/// Store games (Steam, Epic, Xbox) seen running right now.
static STORE_PLAYING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// A store game is open now (Cerberus counts its time, so it must stay open too).
pub fn store_playing() -> bool {
    STORE_PLAYING.load(std::sync::atomic::Ordering::SeqCst) > 0
}

pub fn load(data: &Path) -> HashMap<String, Played> {
    crate::state_file::load_or_default(&file(data))
}

/// Adds `seconds` to a game's time; `session` is how long the session that
/// brought them has lasted so far (the minute-by-minute credit isn't one).
fn add(data: &Path, id: &str, seconds: u64, now: u64, session: u64) -> Result<(), String> {
    // A file that can't be read is left alone (the minute is lost, never the total).
    crate::state_file::update(&file(data), |all: &mut HashMap<String, Played>| {
        let entry = all.entry(id.to_string()).or_default();
        entry.seconds += seconds;
        entry.last = now;
        entry.last_session = session;
    })
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
    matches!(game.source, Source::Steam { .. } | Source::Epic { .. } | Source::Xbox { .. })
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

/// The programs that can say a store game is being played: not a crash
/// reporter or updater left behind, and not one inside a folder Cerberus's own
/// session already follows (a store game also added by hand, opened from it).
fn counted(programs: &[String], tracked: &[String]) -> Vec<String> {
    let tracked: Vec<String> = tracked.iter().map(|d| folder_key(d)).collect();
    programs
        .iter()
        .filter(|p| {
            let name = p.rsplit(['\\', '/']).next().unwrap_or(p);
            !crate::sessions::is_helper_program(&name.to_lowercase())
        })
        .filter(|p| {
            let p = p.replace('/', "\\").to_lowercase();
            !tracked.iter().any(|t| p.starts_with(t))
        })
        .cloned()
        .collect()
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

    // Asked again with room to spare when the list came back full.
    let mut pids = vec![0u32; 4096];
    loop {
        let mut used = 0u32;
        if unsafe { EnumProcesses(pids.as_mut_ptr(), (pids.len() * 4) as u32, &mut used) } == 0 {
            return Vec::new();
        }
        if (used as usize) < pids.len() * 4 || pids.len() >= 1 << 16 {
            pids.truncate(used as usize / 4);
            break;
        }
        pids = vec![0u32; pids.len() * 2];
    }
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
        // Seconds already written for each open game, and when: every minute, so
        // closing Cerberus (or Windows) mid-game keeps all but the last minute.
        let mut credited: HashMap<String, (u64, Instant)> = HashMap::new();
        let mut live = crate::achievements::live::Live::default();
        let steam_app = |id: &str| id.strip_prefix("steam:").and_then(|n| n.parse::<u32>().ok());
        loop {
            // The stores' lists change rarely; reading them each minute is plenty.
            if scanned.elapsed() >= Duration::from_secs(60) {
                let mut found = crate::steam::installed_games();
                found.extend(crate::epic::installed_games());
                found.extend(crate::xbox::installed_games());
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
            let now_running = if dirs.is_empty() { HashSet::new() } else { running(&counted(&running_programs(), &crate::sessions::tracked_folders(&app)), &dirs) };
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
            if let Ok(data) = app.path().app_data_dir() {
                for (id, started) in &playing {
                    let so_far = started.elapsed().as_secs();
                    let (done, at) = credited.get(id).copied().unwrap_or((0, *started));
                    if so_far >= 30 && at.elapsed() >= Duration::from_secs(60) {
                        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default();
                        if add(&data, id, so_far - done, now, so_far).is_ok() {
                            credited.insert(id.clone(), (so_far, Instant::now()));
                        }
                        crate::play_log::still_going(&data, id, so_far, now);
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
                let already = credited.remove(&id).map_or(0, |(done, _)| done);
                crate::in_game::session(&app, false);
                let _ = app.emit(crate::sessions::EVENT, cerberus_shared::SessionEvent { game_id: id.clone(), running: false });
                // A blink (a launcher checking for updates) isn't a session.
                if seconds < 30 {
                    continue;
                }
                let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default();
                let Ok(data) = app.path().app_data_dir() else { continue };
                let saved = add(&data, &id, seconds.saturating_sub(already), now, seconds);
                crate::play_log::ended(&data, &id);
                crate::play_log::record(&data, &id, seconds, now);
                let minutes = if seconds < 60 { tr!("Menos de 1 min jogado").to_string() } else { crate::sessions::played_text(seconds / 60 * 60) };
                crate::trail::note(&app, &id, tk!("Partida"), &saved.map(|_| minutes));
                let _ = app.emit("playtime-updated", &id);
            }
            live.check(&app);
            // Downloads slow down while any game is open (store games or ones Cerberus started).
            let ours = !app.state::<crate::sessions::Sessions>().running().is_empty();
            crate::recomp::throttle::set_playing(ours || !playing.is_empty());
            STORE_PLAYING.store(playing.len(), std::sync::atomic::Ordering::SeqCst);
            std::thread::sleep(Duration::from_secs(5));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers_and_games_cerberus_follows_dont_count_for_the_store() {
        let games = [("steam:1".to_string(), r"D:\Steam\common\Game".to_string())];
        let crash = vec![r"D:\Steam\common\Game\CrashReporter.exe".to_string()];
        assert!(running(&counted(&crash, &[]), &games).is_empty(), "a crash handler left running");
        let game = vec![r"D:\Steam\common\Game\Game.exe".to_string()];
        assert_eq!(running(&counted(&game, &[]), &games).len(), 1);
        // Opened from its hand-added copy: Cerberus's own session counts it.
        assert!(running(&counted(&game, &[r"D:\Steam\common\Game".into()]), &games).is_empty());
    }

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
        add(dir.path(), "steam:1", 600, 100, 600).unwrap();
        add(dir.path(), "steam:1", 120, 200, 120).unwrap();
        let mut games: Vec<Game> = vec![serde_json::from_value(serde_json::json!({
            "id": "steam:1", "name": "Balatro", "source": {"kind": "steam", "app_id": 1}, "install_dir": "x"
        }))
        .unwrap()];
        apply(dir.path(), &mut games);
        assert_eq!((games[0].playtime_secs, games[0].last_played, games[0].last_session_secs), (720, Some(200), Some(120)));
    }

    #[test]
    fn the_last_session_is_the_whole_session_not_its_last_minute() {
        let dir = tempfile::tempdir().unwrap();
        // An hour, credited a minute at a time, then the close.
        for minute in 1..=59u64 {
            add(dir.path(), "steam:1", 60, minute * 60, minute * 60).unwrap();
        }
        add(dir.path(), "steam:1", 10, 3550, 3550).unwrap();
        let kept = load(dir.path());
        assert_eq!((kept["steam:1"].seconds, kept["steam:1"].last_session), (59 * 60 + 10, 3550));
    }
}
