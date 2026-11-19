//! Games the launcher starts itself (added by hand, recomps): whether each is
//! running, how long it ran, and a save backup when it closes.
//!
//! Steam and Epic games start through their own launcher, so they never show up
//! here. On Windows a session can include a launcher and its playable child.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows::spawn_game;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use std::time::{Duration, Instant};

use cerberus_shared::{Game, SessionEvent, Source};
use tauri::{AppHandle, Emitter, Manager};

use crate::library::LocalLibrary;
use crate::recomp;

pub const EVENT: &str = "game-session";

#[derive(Default)]
pub struct Sessions(Mutex<HashMap<String, Option<Round>>>);

struct Round {
    started: Instant,
    credited: u64,
}

enum Observation {
    Started(Instant),
    Closed(Duration),
}

impl Sessions {
    pub fn running(&self) -> Vec<String> {
        self.lock().keys().cloned().collect()
    }

    pub fn is_running(&self, id: &str) -> bool {
        self.lock().contains_key(id)
    }

    fn reserve(&self, game: &Game) -> Result<(), String> {
        let mut active = self.lock();
        if active.contains_key(&game.id) { return Err(format!("{} já está aberto.", game.name)); }
        active.insert(game.id.clone(), None);
        Ok(())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Option<Round>>> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }

    // An open installer or launcher menu has no measured round.
    fn checkpoint(&self, library: &LocalLibrary, now: Instant) {
        for (id, round) in self.lock().iter_mut() {
            let Some(round) = round else { continue };
            let elapsed = now.saturating_duration_since(round.started).as_secs();
            if elapsed > round.credited && library.record_session(id, round.credited, elapsed, None).is_ok() {
                round.credited = elapsed;
            }
        }
    }

    fn close_round(&self, library: &LocalLibrary, id: &str, seconds: u64, closed_at: u64) -> Result<Game, String> {
        let mut active = self.lock();
        let slot = active.get_mut(id).ok_or("A sessão já foi encerrada.")?;
        let credited = slot.take().map_or(0, |round| round.credited);
        library.record_session(id, credited, seconds, Some(closed_at))
    }
}

pub fn checkpoint(app: &AppHandle) {
    app.state::<Sessions>().checkpoint(&app.state::<LocalLibrary>(), Instant::now());
}

pub fn tracking_mode(game: &Game) -> Option<String> {
    #[cfg(windows)]
    {
        let exe = match &game.source {
            Source::Recomp { exe, .. } | Source::Local { exe } if !game.setup_pending => exe,
            _ => return None,
        };
        Some(match &game.tracking_exe {
            Some(path) => path.clone(),
            None => windows::runtime_for(Path::new(exe)).map_or_else(
                || "Automático · jogo e processos iniciados por ele".into(),
                |name| format!("Automático · {}", name.display())),
        })
    }
    #[cfg(not(windows))]
    { let _ = game; None }
}

pub fn start(app: &AppHandle, game: &Game, exe: &Path) -> Result<(), String> {
    let sessions = app.state::<Sessions>();
    sessions.reserve(game)?;
    // An installer release runs its setup on the first Play instead of a session,
    // unless an earlier run already installed the game.
    let found;
    let (game, exe) = match game.setup_pending.then(|| crate::setup_runner::already_installed(app, game)).flatten() {
        Some(g) => {
            found = g;
            let program = match &found.source {
                Source::Recomp { exe, .. } | Source::Local { exe } => PathBuf::from(exe),
                _ => exe.to_path_buf(),
            };
            (&found, program)
        }
        None => (game, exe.to_path_buf()),
    };
    let exe = exe.as_path();
    if game.setup_pending {
        notify(app, &game.id, true);
        let result = crate::setup_runner::run(app, game, exe);
        if result.is_err() { release(app, &game.id); }
        return result;
    }
    // Newer saves from another PC go in place before the game reads them.
    if let Source::Recomp { recomp_id, .. } = &game.source {
        let folders = app.state::<recomp::Folders>();
        match recomp::pull_from_cloud(&folders, recomp_id, Path::new(&game.install_dir)) {
            Ok(Some(message)) => {
                crate::trail::note(app, &game.id, "Saves da nuvem", &Ok(message.clone()));
                let _ = app.emit("toast", message);
            }
            Ok(None) => {}
            // Playing matters more than syncing; say so and carry on.
            Err(error) => {
                crate::trail::note(app, &game.id, "Saves da nuvem", &Err(error.clone()));
                let _ = app.emit("toast", format!("Não foi possível conferir os saves da nuvem: {error}"));
            }
        }
    }
    // Many games load their data relative to where they start, so start them in their own folder.
    let dir = exe.parent().map(Path::to_path_buf).unwrap_or_else(PathBuf::new);
    let plan = recomp::launch_plan(game);
    let mut command = Command::new(exe);
    command.current_dir(dir).args(plan.args).envs(plan.env);
    // The picture filter: ReShade beside the game until it closes. A look that
    // can't go on never stops the game from starting.
    if let Some(look) = &game.look {
        let data = app.path().app_data_dir().map_err(|e| e.to_string())?;
        match crate::looks::prepare(&data, &game.id, exe, look) {
            Ok(prepared) => {
                command.envs(prepared.env);
            }
            Err(error) => {
                crate::trail::note(app, &game.id, "Filtro", &Err(error.clone()));
                let _ = app.emit("toast", format!("Filtro desligado nesta partida: {error}"));
            }
        }
    }
    let child = spawn_game(command, game.tracking_exe.as_deref().map(Path::new)).map_err(|e| {
        sessions.lock().remove(&game.id);
        let message = format!("Não foi possível abrir {}: {e}", exe.display());
        crate::trail::note(app, &game.id, "Abrir o jogo", &Err(message.clone()));
        message
    })?;
    notify(app, &game.id, true);

    let app = app.clone();
    let game = game.clone();
    std::thread::spawn(move || {
        let result = child.observe(|event| match event {
            Observation::Started(started) => {
                app.state::<Sessions>().lock().insert(game.id.clone(), Some(Round { started, credited: 0 }));
            }
            Observation::Closed(duration) => record(&app, &game, duration.as_secs()),
        });
        if let Err(e) = result {
            let message = format!("Não foi possível acompanhar a sessão de {}: {e}", game.name);
            crate::trail::note(&app, &game.id, "Partida", &Err(message.clone()));
            let _ = app.emit("toast", message);
        }
        release(&app, &game.id);
    });
    Ok(())
}

#[cfg(not(windows))]
struct GameProcess(std::process::Child, Instant);

#[cfg(not(windows))]
fn spawn_game(mut command: Command, _selected: Option<&Path>) -> std::io::Result<GameProcess> {
    let started = Instant::now();
    Ok(GameProcess(command.spawn()?, started))
}

#[cfg(not(windows))]
impl GameProcess {
    fn observe(mut self, mut report: impl FnMut(Observation)) -> std::io::Result<()> {
        report(Observation::Started(self.1));
        self.0.wait()?;
        report(Observation::Closed(self.1.elapsed()));
        Ok(())
    }
}

fn record(app: &AppHandle, game: &Game, seconds: u64) {
    let closed_at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default();
    let trail = app.state::<crate::trail::Trail>();
    let notices = close_session(&app.state::<LocalLibrary>(), &app.state::<Sessions>(), &app.state::<recomp::Folders>(), &trail, game, seconds, closed_at);
    for notice in notices {
        let _ = app.emit("toast", notice);
    }
    let _ = app.emit("playtime-updated", &game.id);
}

/// Everything that happens when a round ends: its time goes into the library,
/// a recomp's saves are backed up, and both results go into the history. A
/// failure never stops the next step; it comes back as a notice for the player.
fn close_session(
    library: &LocalLibrary,
    sessions: &Sessions,
    folders: &recomp::Folders,
    trail: &crate::trail::Trail,
    game: &Game,
    seconds: u64,
    closed_at: u64,
) -> Vec<String> {
    let mut notices = Vec::new();
    let saved = sessions.close_round(library, &game.id, seconds, closed_at);
    let played = format!("{} jogados", duration_text(seconds));
    trail.note(&game.id, "Partida", &saved.as_ref().map(|_| played).map_err(|e| format!("Tempo não gravado: {e}")));
    if let Err(e) = saved {
        notices.push(format!("Não foi possível gravar o tempo de {}: {e}", game.name));
    }
    if let Source::Recomp { recomp_id, .. } = &game.source {
        let result = recomp::backup_saves(folders, recomp_id, Path::new(&game.install_dir), "Depois de jogar");
        trail.note(&game.id, "Backup depois de jogar", &backup_outcome(&result));
        if let Err(e) = result {
            notices.push(format!("Falha no backup de {}: {e}", game.name));
        }
    }
    notices
}

/// "1 h 5 min", "3 min", "40 s".
fn duration_text(seconds: u64) -> String {
    match seconds {
        0..60 => format!("{seconds} s"),
        60..3600 => format!("{} min", seconds / 60),
        _ => format!("{} h {} min", seconds / 3600, seconds % 3600 / 60),
    }
}

/// A backup's result as the history shows it.
pub fn backup_outcome<E: std::fmt::Display>(result: &Result<Option<cerberus_shared::Backup>, E>) -> Result<String, String> {
    match result {
        Ok(Some(backup)) => Ok(format!("{} arquivos copiados", backup.files)),
        Ok(None) => Ok("Nada novo para copiar".into()),
        Err(e) => Err(e.to_string()),
    }
}

pub fn release(app: &AppHandle, id: &str) {
    if let Ok(data) = app.path().app_data_dir() {
        crate::looks::take_back(&data, id);
    }
    app.state::<Sessions>().lock().remove(id);
    notify(app, id, false);
}

fn notify(app: &AppHandle, game_id: &str, running: bool) {
    let event = SessionEvent { game_id: game_id.to_string(), running };
    let _ = app.emit(EVENT, event);
}

#[cfg(test)]
#[test]
fn checkpoints_survive_restart_and_a_failed_write_without_counting_the_menu() {
    let dir = tempfile::tempdir().unwrap();
    let library = LocalLibrary::load(dir.path());
    let game = crate::library::game_from_exe(&dir.path().join("Game.exe"));
    library.add(game.clone()).unwrap();
    let clock = Sessions::default();
    let start = Instant::now();
    clock.lock().insert(game.id.clone(), Some(Round { started: start, credited: 0 }));
    let tick = |seconds| clock.checkpoint(&library, start + Duration::from_secs(seconds));
    let persisted = || LocalLibrary::load(dir.path()).get(&game.id).unwrap().unwrap();
    tick(15);
    assert_eq!(persisted().playtime_secs, 15);
    let blocker = dir.path().join("library.json.tmp");
    std::fs::create_dir(&blocker).unwrap();
    tick(30);
    assert_eq!(persisted().playtime_secs, 15);
    std::fs::remove_dir(blocker).unwrap();
    tick(43);
    tick(43);
    assert_eq!((persisted().playtime_secs, persisted().last_session_secs), (43, None));
    clock.close_round(&library, &game.id, 50, 100).unwrap();
    tick(90);
    assert_eq!((persisted().playtime_secs, persisted().last_session_secs), (50, Some(50)));
}

#[cfg(test)]
#[test]
fn a_failed_write_still_ends_the_session_and_its_cause_survives_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let library = LocalLibrary::load(dir.path());
    let game = crate::library::game_from_exe(&dir.path().join("Game.exe"));
    library.add(game.clone()).unwrap();
    let sessions = Sessions::default();
    sessions.lock().insert(game.id.clone(), Some(Round { started: Instant::now(), credited: 0 }));
    let folders = recomp::Folders::new(dir.path(), None);
    let trail = crate::trail::Trail::new(dir.path());
    // The library can't be written while a folder sits where its temporary file goes.
    std::fs::create_dir(dir.path().join("library.json.tmp")).unwrap();

    let notices = close_session(&library, &sessions, &folders, &trail, &game, 125, 100);
    assert_eq!(notices.len(), 1);
    // The round is over (a launcher menu may stay open), even though its time wasn't saved.
    assert!(sessions.lock()[&game.id].is_none());

    let reopened = crate::trail::Trail::new(dir.path());
    let last = &reopened.for_game(&game.id, 1)[0];
    assert_eq!(last.step, "Partida");
    assert!(!last.ok);
    assert!(last.message.starts_with("Tempo não gravado"));
}

#[cfg(test)]
#[test]
fn durations_read_as_people_say_them() {
    assert_eq!(duration_text(40), "40 s");
    assert_eq!(duration_text(125), "2 min");
    assert_eq!(duration_text(3900), "1 h 5 min");
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::os::windows::process::CommandExt;
    use std::process::Stdio;
    use std::time::Instant;

    #[test]
    fn session_waits_for_game_after_launcher_exits() {
        // A launcher hands off to a game that keeps playing independently.
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("game-finished");
        let mut launcher = Command::new("cmd.exe");
        launcher.args(["/d", "/c"]).raw_arg(
            r#"start "" /b powershell.exe -NoProfile -NonInteractive -Command "Start-Sleep -Milliseconds 900; [IO.File]::WriteAllText($env:CERBERUS_TEST_MARKER, 'done')""#,
        );
        launcher.env("CERBERUS_TEST_MARKER", &marker)
            .stdout(Stdio::null()).stderr(Stdio::null());
        let started = Instant::now();
        let game = spawn_game(launcher, None).unwrap();
        let mut recorded = std::time::Duration::ZERO;
        game.observe(|event| { if let Observation::Closed(time) = event { recorded += time; } }).unwrap();
        let elapsed = started.elapsed();
        // Keep the fixture available long enough for the detached child to finish on failure.
        let finished_with_session = marker.is_file();
        if !finished_with_session {
            for _ in 0..50 {
                if marker.is_file() { break; }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        assert!(marker.is_file(), "the fixture game must have run");
        assert!(finished_with_session, "session closed after {elapsed:?} while game was still running");
        assert!(recorded >= std::time::Duration::from_millis(900));
    }

    #[test]
    fn records_each_game_while_its_launcher_menu_stays_open() {
        const TEST: &str = "sessions::tests::records_each_game_while_its_launcher_menu_stays_open";
        const ROLE: &str = "CERBERUS_SESSION_FIXTURE";
        let command = |exe: &Path, role: &str| {
            let mut cmd = Command::new(exe);
            cmd.args(["--exact", TEST]).env(ROLE, role)
                .stdout(Stdio::null()).stderr(Stdio::null());
            cmd
        };
        match std::env::var(ROLE).as_deref() {
            Ok("game") => {
                std::thread::sleep(std::time::Duration::from_millis(1100));
                return;
            }
            Ok("menu") => {
                if let Some(decoy) = std::env::var_os("CERBERUS_FIXTURE_DECOY") {
                    assert!(command(Path::new(&decoy), "game").status().unwrap().success());
                }
                for n in 1..=2 {
                    let runtime = PathBuf::from(std::env::var_os("CERBERUS_FIXTURE_GAME").unwrap());
                    assert!(command(&runtime, "game").status().unwrap().success());
                    // The menu remains open. Cerberus should have credited this round before the next one.
                    let ack = PathBuf::from(format!("round-{n}"));
                    for _ in 0..50 {
                        if ack.exists() { break; }
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                    assert!(ack.exists());
                }
                std::fs::write("menu-closed", "").unwrap();
                return;
            }
            _ => {}
        }
        let fixture = std::env::current_exe().unwrap();
        for explicit in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let menu = dir.path().join("PitchLauncher.exe");
            let separate = tempfile::tempdir().unwrap();
            let runtime = if explicit { separate.path().join("Match.exe") } else { dir.path().join("Binaries/Pitch.exe") };
            let link_fixture = |path: &Path| {
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                if std::fs::hard_link(&fixture, path).is_err() { std::fs::copy(&fixture, path).unwrap(); }
            };
            link_fixture(&menu);
            link_fixture(&runtime);
            let mut cmd = command(&menu, "menu");
            cmd.current_dir(dir.path()).env("CERBERUS_FIXTURE_GAME", &runtime);
            if explicit {
                let decoy = dir.path().join("Match.exe");
                link_fixture(&decoy);
                cmd.env("CERBERUS_FIXTURE_DECOY", decoy);
            }
            let mut rounds = 0;
            spawn_game(cmd, explicit.then_some(runtime.as_path())).unwrap().observe(|event| {
                let Observation::Closed(duration) = event else { return };
                rounds += 1;
                assert!(duration >= Duration::from_millis(1100));
                assert!(!dir.path().join("menu-closed").exists());
                std::fs::write(dir.path().join(format!("round-{rounds}")), "").unwrap();
            }).unwrap();
            assert_eq!(rounds, 2);
            assert!(dir.path().join("menu-closed").exists());
        }
    }
}
