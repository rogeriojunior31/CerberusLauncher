//! Games the launcher starts itself (added by hand, recomps): whether each is
//! running, how long it ran, and a save backup when it closes.
//!
//! Steam and Epic games start through their own launcher, so they never show up
//! here. On Windows a session can include a launcher and its playable child.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows::spawn_game;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(not(windows))]
use std::time::{Duration, Instant};

use cerberus_shared::{Game, SessionEvent, Source};
use tauri::{AppHandle, Emitter, Manager};

use crate::library::LocalLibrary;
use crate::recomp;

pub const EVENT: &str = "game-session";

#[derive(Default)]
pub struct Sessions(Mutex<HashSet<String>>);

impl Sessions {
    pub fn running(&self) -> Vec<String> {
        self.lock().iter().cloned().collect()
    }

    pub fn is_running(&self, id: &str) -> bool {
        self.lock().contains(id)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashSet<String>> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }
}

pub fn start(app: &AppHandle, game: &Game, exe: &Path) -> Result<(), String> {
    let sessions = app.state::<Sessions>();
    if sessions.is_running(&game.id) {
        return Err(format!("{} já está aberto.", game.name));
    }
    // An installer release runs its setup on the first Play instead of a session.
    if game.setup_pending {
        return crate::setup_runner::run(app, game, exe);
    }
    // Newer saves from another PC go in place before the game reads them.
    if let Source::Recomp { recomp_id, .. } = &game.source {
        let folders = app.state::<recomp::Folders>();
        match recomp::pull_from_cloud(&folders, recomp_id, Path::new(&game.install_dir)) {
            Ok(Some(message)) => {
                let _ = app.emit("toast", message);
            }
            Ok(None) => {}
            // Playing matters more than syncing; say so and carry on.
            Err(error) => {
                let _ = app.emit("toast", format!("Não foi possível conferir os saves da nuvem: {error}"));
            }
        }
    }
    // Many games load their data relative to where they start, so start them in their own folder.
    let dir = exe.parent().map(Path::to_path_buf).unwrap_or_else(PathBuf::new);
    let plan = recomp::launch_plan(game);
    let mut command = Command::new(exe);
    command.current_dir(dir).args(plan.args).envs(plan.env);
    let child = spawn_game(command)
        .map_err(|e| format!("Não foi possível abrir {}: {e}", exe.display()))?;
    sessions.lock().insert(game.id.clone());
    notify(app, &game.id, true);

    let app = app.clone();
    let game = game.clone();
    std::thread::spawn(move || {
        let result = child.observe(|duration| record(&app, &game, duration.as_secs()));
        if let Err(e) = result {
            let _ = app.emit("toast", format!("Não foi possível acompanhar a sessão de {}: {e}", game.name));
        }
        app.state::<Sessions>().lock().remove(&game.id);
        notify(&app, &game.id, false);
    });
    Ok(())
}

#[cfg(not(windows))]
struct GameProcess(std::process::Child, Instant);

#[cfg(not(windows))]
fn spawn_game(mut command: Command) -> std::io::Result<GameProcess> {
    let started = Instant::now();
    Ok(GameProcess(command.spawn()?, started))
}

#[cfg(not(windows))]
impl GameProcess {
    fn observe(mut self, mut played: impl FnMut(Duration)) -> std::io::Result<()> {
        self.0.wait()?;
        played(self.1.elapsed());
        Ok(())
    }
}

fn record(app: &AppHandle, game: &Game, seconds: u64) {
    let closed_at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default();
    let library = app.state::<LocalLibrary>();
    if let Err(e) = library.record_session(&game.id, seconds, closed_at) {
        let _ = app.emit("toast", format!("Não foi possível gravar o tempo de {}: {e}", game.name));
    }
    if let Source::Recomp { recomp_id, .. } = &game.source {
        let folders = app.state::<recomp::Folders>();
        // A failed copy must not hide that the game closed; the next session tries again.
        let _ = recomp::backup_saves(&folders, recomp_id, Path::new(&game.install_dir), "Depois de jogar");
    }
    let _ = app.emit("playtime-updated", &game.id);
}

fn notify(app: &AppHandle, game_id: &str, running: bool) {
    let event = SessionEvent { game_id: game_id.to_string(), running };
    let _ = app.emit(EVENT, event);
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
        let game = spawn_game(launcher).unwrap();
        let mut recorded = std::time::Duration::ZERO;
        game.observe(|time| recorded += time).unwrap();
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
                for n in 1..=2 {
                    let runtime = Path::new("Binaries/Pitch.exe").canonicalize().unwrap();
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
        let dir = tempfile::tempdir().unwrap();
        let menu = dir.path().join("PitchLauncher.exe");
        let binaries = dir.path().join("Binaries");
        std::fs::create_dir(&binaries).unwrap();
        let fixture = std::env::current_exe().unwrap();
        for exe in [&menu, &binaries.join("Pitch.exe")] {
            if std::fs::hard_link(&fixture, exe).is_err() { std::fs::copy(&fixture, exe).unwrap(); }
        }
        let mut cmd = command(&menu, "menu");
        cmd.current_dir(dir.path());
        let mut rounds = 0;
        spawn_game(cmd).unwrap().observe(|duration| {
            rounds += 1;
            assert!(duration >= std::time::Duration::from_millis(1100));
            assert!(!dir.path().join("menu-closed").exists());
            std::fs::write(dir.path().join(format!("round-{rounds}")), "").unwrap();
        }).unwrap();
        assert_eq!(rounds, 2);
        assert!(dir.path().join("menu-closed").exists());
    }
}
