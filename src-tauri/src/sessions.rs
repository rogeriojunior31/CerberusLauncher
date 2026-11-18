//! Games the launcher starts itself (added by hand, recomps): whether each is
//! running, how long it ran, and a save backup when it closes.
//!
//! Steam and Epic games start through their own launcher, so they never show up
//! here. A game that hands off to another process and exits at once (a small
//! launcher stub) counts as a very short session; that is the cost of only
//! watching the process we started.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

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
    let mut child = Command::new(exe)
        .current_dir(dir)
        .args(&plan.args)
        .envs(plan.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .spawn()
        .map_err(|e| format!("Não foi possível abrir {}: {e}", exe.display()))?;
    sessions.lock().insert(game.id.clone());
    notify(app, &game.id, true);

    let app = app.clone();
    let game = game.clone();
    let started = Instant::now();
    // One thread per open game, asleep in `wait` until it closes.
    std::thread::spawn(move || {
        let _ = child.wait();
        finish(&app, &game, started.elapsed().as_secs());
    });
    Ok(())
}

fn finish(app: &AppHandle, game: &Game, seconds: u64) {
    app.state::<Sessions>().lock().remove(&game.id);
    let closed_at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default();
    let library = app.state::<LocalLibrary>();
    let _ = library.update(&game.id, |g| {
        g.playtime_secs += seconds;
        g.last_played = Some(closed_at);
    });
    if let Source::Recomp { recomp_id, .. } = &game.source {
        let folders = app.state::<recomp::Folders>();
        // A failed copy must not hide that the game closed; the next session tries again.
        let _ = recomp::backup_saves(&folders, recomp_id, Path::new(&game.install_dir), "Depois de jogar");
    }
    notify(app, &game.id, false);
}

fn notify(app: &AppHandle, game_id: &str, running: bool) {
    let event = SessionEvent { game_id: game_id.to_string(), running };
    let _ = app.emit(EVENT, event);
}
