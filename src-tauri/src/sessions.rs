//! Games the launcher starts itself (added by hand, recomps): whether each is
//! running, how long it ran, and a save backup when it closes.
//!
//! Steam and Epic games start through their own launcher, so they never show up
//! here. On Windows a session can include a launcher and its playable child.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use cerberus_shared::tk;
use cerberus_shared::tr;
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
    /// The game's exit code, reported just before `Closed` when it is known.
    Exited(i32),
    /// Its windows were gone well before the process ended: whatever went
    /// wrong happened while closing, after the play was over.
    #[cfg_attr(not(windows), allow(dead_code))]
    WindowsClosedFirst,
    Closed(Duration),
}

impl Sessions {
    pub fn running(&self) -> Vec<String> {
        self.lock().keys().cloned().collect()
    }

    pub fn is_running(&self, id: &str) -> bool {
        self.lock().contains_key(id)
    }

    /// How long the game has been played this time, in seconds.
    pub fn playing_for(&self, id: &str) -> Option<u64> {
        self.lock().get(id)?.as_ref().map(|round| round.started.elapsed().as_secs())
    }

    fn reserve(&self, game: &Game) -> Result<(), String> {
        let mut active = self.lock();
        if active.contains_key(&game.id) { return Err(tr!("{} já está aberto.", game.name)); }
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
        let slot = active.get_mut(id).ok_or(tr!("A sessão já foi encerrada."))?;
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
                || tr!("Automático · jogo e processos iniciados por ele").into(),
                |name| tr!("Automático · {}", name.display())),
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
    // A setup step the release runs once with the player's copy (unpacking the
    // disc): it runs first, and the game starts when it's done.
    match recomp::preparation(game) {
        Ok(Some(step)) => {
            notify(app, &game.id, true);
            return prepare_then_start(app, game.clone(), Some(exe.to_path_buf()), step);
        }
        Ok(None) => {}
        Err(problem) => {
            release(app, &game.id);
            return Err(problem);
        }
    }
    // Newer saves from another PC go in place before the game reads them.
    if let Source::Recomp { recomp_id, .. } = &game.source {
        let folders = app.state::<recomp::Folders>();
        match recomp::pull_from_cloud(&folders, recomp_id, Path::new(&game.install_dir)) {
            Ok(Some(message)) => {
                crate::trail::note(app, &game.id, tk!("Saves da nuvem"), &Ok(message.clone()));
                let _ = app.emit("toast", message);
            }
            Ok(None) => {}
            // Playing matters more than syncing; say so and carry on.
            Err(error) => {
                crate::trail::note(app, &game.id, tk!("Saves da nuvem"), &Err(error.clone()));
                let _ = app.emit("toast", tr!("Não foi possível conferir os saves da nuvem: {}", error));
            }
        }
    }
    // Many games load their data relative to where they start, so start them in their own folder.
    let plan = recomp::launch_plan(game);
    // The program on record is gone, but the developer's file names one that's there.
    let exe = match &plan.program {
        Some(named) if !exe.is_file() => named.as_path(),
        _ => exe,
    };
    let dir = exe.parent().map(Path::to_path_buf).unwrap_or_else(PathBuf::new);
    // The settings files the developer's file keeps up; a file that can't be
    // written is noted, and the game starts anyway.
    if let Err(error) = recomp::prepare_start(game) {
        crate::trail::note(app, &game.id, tk!("Ajustes do jogo"), &Err(error));
    }
    let mut command = Command::new(exe);
    command.current_dir(dir).args(plan.args).envs(plan.env);
    // The picture filter: ReShade beside the game until it closes. A look that
    // can't go on never stops the game from starting.
    // An older PC than the game asks for: said, and the game starts anyway.
    if let Some(problem) = recomp::entry_for(game).and_then(|e| e.manifest).and_then(|m| recomp::manifest::system_problem(&m, recomp::this_windows_build())) {
        let _ = app.emit("toast", problem);
    }
    let look = match recomp::look_rule(game) {
        Some(Err(why)) => {
            if game.look.is_some() {
                crate::trail::note(app, &game.id, tk!("Filtro"), &Err(why));
            }
            None
        }
        _ => game.look.as_ref(),
    };
    if let Some(look) = look {
        let data = app.path().app_data_dir().map_err(|e| e.to_string())?;
        match crate::looks::prepare(&data, &game.id, exe, look, recomp::look_lines(game)) {
            Ok(prepared) => {
                command.envs(prepared.env);
            }
            Err(error) => {
                crate::trail::note(app, &game.id, tk!("Filtro"), &Err(error.clone()));
                let _ = app.emit("toast", tr!("Filtro desligado nesta partida: {}", error));
            }
        }
    }
    let child = spawn_game(command, game.tracking_exe.as_deref().map(Path::new)).map_err(|e| {
        sessions.lock().remove(&game.id);
        let message = blocked_by_windows(e.raw_os_error()).map(str::to_string).unwrap_or_else(|| tr!("Não foi possível abrir {0}: {1}", exe.display(), e));
        crate::trail::note(app, &game.id, tk!("Abrir o jogo"), &Err(message.clone()));
        message
    })?;
    notify(app, &game.id, true);

    let app = app.clone();
    let game = game.clone();
    std::thread::spawn(move || {
        let mut exit_code = None;
        let mut closing = false;
        let result = child.observe(|event| match event {
            Observation::Started(started) => {
                app.state::<Sessions>().lock().insert(game.id.clone(), Some(Round { started, credited: 0 }));
            }
            Observation::Exited(code) => exit_code = Some(code),
            Observation::WindowsClosedFirst => closing = true,
            Observation::Closed(duration) => {
                record(&app, &game, duration.as_secs());
                if let Some(code) = exit_code.take() {
                    report_crash(&app, &game, code, duration, std::mem::take(&mut closing));
                }
            }
        });
        if let Err(e) = result {
            let message = tr!("Não foi possível acompanhar a sessão de {0}: {1}", game.name, e);
            crate::trail::note(&app, &game.id, tk!("Partida"), &Err(message.clone()));
            let _ = app.emit("toast", message);
        }
        release(&app, &game.id);
    });
    Ok(())
}

#[cfg(not(windows))]
struct GameProcess(std::process::Child, Instant);

/// Why Windows itself refused to start a game, in words the player can act on.
fn blocked_by_windows(code: Option<i32>) -> Option<&'static str> {
    Some(match code? {
        // ERROR_SYSTEM_INTEGRITY_POLICY_VIOLATION and its reputation variants: Smart App Control.
        4551 | 4553 | 4580 => tr!("O Smart App Control do Windows bloqueou o jogo, porque o programa não é assinado. Ele não abre exceções: para jogar, é preciso desligar o Smart App Control em Segurança do Windows › Controle de aplicativos e navegador."),
        // ERROR_VIRUS_INFECTED / ERROR_VIRUS_DELETED
        225 | 226 => tr!("O antivírus bloqueou o jogo. Se você confia no recomp, libere a pasta dele no antivírus e use Reparar instalação na aba Gerenciar."),
        // ERROR_ACCESS_DISABLED_BY_POLICY
        1260 => tr!("Uma política do Windows (da empresa ou do AppLocker) bloqueia programas nessa pasta. Instale os recomps numa pasta permitida, em Configurações › Instalação."),
        // ERROR_ELEVATION_REQUIRED
        740 => tr!("Esse jogo pede para abrir como administrador. O Cerberus não abre jogos como administrador; veja se o projeto tem uma versão que não precise disso."),
        _ => return None,
    })
}

#[cfg(not(windows))]
fn spawn_game(mut command: Command, _selected: Option<&Path>) -> std::io::Result<GameProcess> {
    let started = Instant::now();
    Ok(GameProcess(command.spawn()?, started))
}

#[cfg(not(windows))]
impl GameProcess {
    fn observe(mut self, mut report: impl FnMut(Observation)) -> std::io::Result<()> {
        report(Observation::Started(self.1));
        let status = self.0.wait()?;
        if let Some(code) = status.code() {
            report(Observation::Exited(code));
        }
        report(Observation::Closed(self.1.elapsed()));
        Ok(())
    }
}

/// The setup step on its own, from the game's page: the game doesn't start after.
/// `false` when there is nothing to prepare.
pub fn prepare_now(app: &AppHandle, game: &Game) -> Result<bool, String> {
    let sessions = app.state::<Sessions>();
    sessions.reserve(game)?;
    match recomp::preparation(game) {
        Ok(Some(step)) => {
            notify(app, &game.id, true);
            prepare_then_start(app, game.clone(), None, step).map(|()| true)
        }
        other => {
            release(app, &game.id);
            other.map(|_| false)
        }
    }
}

/// `exe`: the game to start once the step is done; `None` to only prepare.
fn prepare_then_start(app: &AppHandle, game: Game, exe: Option<PathBuf>, step: recomp::Preparation) -> Result<(), String> {
    let fail = |message: String| {
        release(app, &game.id);
        message
    };
    let install = Path::new(&game.install_dir);
    // A build needs room: at least 1 GB on the game's drive.
    crate::disk::require(install, 1 << 30).map_err(fail)?;
    if let Some(temp) = &step.temp {
        let _ = std::fs::remove_dir_all(temp);
        std::fs::create_dir_all(temp).map_err(|e| fail(e.to_string()))?;
    }
    let dir = step.program.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut command = Command::new(&step.program);
    command.args(&step.args).envs(step.env.clone()).current_dir(dir).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No window: the step never asks anything.
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().map_err(|e| fail(tr!("Não foi possível abrir a preparação do jogo: {}", e)))?;
    let _ = app.emit("toast", tr!("{0}: {1}. Isso acontece uma vez só e pode levar alguns minutos.", game.name, step.label));
    let say = |app: &AppHandle, fraction: Option<f32>, text: String, finished: bool| {
        let _ = app.emit("prepare-progress", cerberus_shared::PrepareProgress { game_id: game.id.clone(), fraction, text, finished });
    };
    say(app, Some(0.0), step.label.clone(), false);

    // What it prints: progress for the player, the last lines for an error.
    let heard = std::sync::Arc::new(Mutex::new((Instant::now(), Vec::<String>::new())));
    for stream in [child.stdout.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>), child.stderr.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>)].into_iter().flatten() {
        let (heard, app, label) = (heard.clone(), app.clone(), step.label.clone());
        let game_id = game.id.clone();
        std::thread::spawn(move || {
            use std::io::BufRead;
            for line in std::io::BufReader::new(stream).lines().map_while(Result::ok) {
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }
                if let Some((fraction, text)) = progress_line(&line) {
                    let text = if text.is_empty() { label.clone() } else { text };
                    let _ = app.emit("prepare-progress", cerberus_shared::PrepareProgress { game_id: game_id.clone(), fraction, text, finished: false });
                }
                if let Ok(mut h) = heard.lock() {
                    h.0 = Instant::now();
                    h.1.push(line);
                    let len = h.1.len();
                    if len > 40 {
                        h.1.drain(..len - 40);
                    }
                }
            }
        });
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let started = Instant::now();
        let outcome = loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => break Ok(()),
                Ok(Some(status)) => {
                    let said = heard.lock().map(|h| h.1.join("\n")).unwrap_or_default();
                    let long_path = said.contains("PathTooLongException") || said.to_lowercase().contains("filename or extension is too long");
                    break Err(if long_path {
                        tr!("O caminho da pasta do jogo é longo demais para o Windows. Instale numa pasta com caminho mais curto ou ligue o suporte a caminhos longos do Windows.").to_string()
                    } else {
                        tr!("A preparação do jogo terminou com o código {}.", status.code().unwrap_or(-1)).to_string()
                    });
                }
                Ok(None) => {
                    let silent = heard.lock().map(|h| h.0.elapsed()).unwrap_or_default();
                    // No limit for a long build, but an hour without a word is a stuck one.
                    if step.timeout.is_some_and(|t| started.elapsed() > t) || silent > Duration::from_secs(3600) {
                        let _ = child.kill();
                        break Err(tr!("A preparação do jogo parou de responder e foi interrompida.").to_string());
                    }
                    std::thread::sleep(Duration::from_millis(500));
                }
                Err(e) => break Err(e.to_string()),
            }
        };
        if let Some(temp) = &step.temp {
            let _ = std::fs::remove_dir_all(temp);
        }
        crate::trail::note(&app, &game.id, tk!("Preparação do jogo"), &outcome.clone().map(|()| step.label.clone()));
        let _ = app.emit("prepare-progress", cerberus_shared::PrepareProgress { game_id: game.id.clone(), fraction: Some(1.0), text: String::new(), finished: true });
        release(&app, &game.id);
        match outcome {
            Ok(()) => {
                let _ = std::fs::write(&step.marker, &step.done_with);
                match &exe {
                    Some(exe) => {
                        if let Err(error) = start(&app, &game, exe) {
                            let _ = app.emit("toast", error);
                        }
                    }
                    None => {
                        let _ = app.emit("toast", tr!("{}: pronto para jogar.", game.name));
                    }
                }
            }
            Err(message) => {
                let _ = app.emit("toast", tr!("{0}: {1}", game.name, message));
            }
        }
    });
    Ok(())
}

/// "progress 0.4 Unpacking menu art" (the SDK's way), or "@note 89%, 68606 files"
/// and "@step 2 3 Unpacking" (some setups' own): how far, and what.
fn progress_line(line: &str) -> Option<(Option<f32>, String)> {
    if let Some(rest) = line.strip_prefix("progress ") {
        let (number, text) = rest.split_once(' ').unwrap_or((rest, ""));
        let fraction = number.trim().parse::<f32>().ok()?.clamp(0.0, 1.0);
        return Some((Some(fraction), text.trim().to_string()));
    }
    if let Some(rest) = line.strip_prefix("@note ") {
        let percent = rest.split('%').next()?.trim().parse::<f32>().ok()?;
        return Some((Some((percent / 100.0).clamp(0.0, 1.0)), String::new()));
    }
    if let Some(rest) = line.strip_prefix("@step ") {
        let mut parts = rest.splitn(3, ' ');
        let (_, _, text) = (parts.next()?, parts.next()?, parts.next().unwrap_or_default());
        return Some((None, text.trim().to_string()));
    }
    None
}

/// A game that ended badly: why, in the history and as a notice, with its log.
fn report_crash(app: &AppHandle, game: &Game, code: i32, played: Duration, while_closing: bool) {
    // The developer's own words for this code, or "ok": the game's normal close.
    let own = recomp::entry_for(game).and_then(|e| e.manifest).and_then(|m| m.exit_message(code));
    let why = match own {
        Some(None) => return,
        Some(Some(message)) => Some(message),
        None => crate::crash::reason(code, played.as_secs()),
    };
    let Some(mut why) = why else { return };
    // Fell over after its window had closed: the play itself went fine. Kept in
    // the history, without the alarm (a notice, "Precisa de atenção").
    if while_closing {
        crate::trail::note(app, &game.id, tk!("Fechamento do jogo"), &Ok(tr!("O jogo deu erro ao fechar, depois que a janela já tinha fechado. A partida não foi afetada. ({})", why)));
        return;
    }
    // "A DLL is too old": find which one, when something stands out.
    if code as u32 == crate::crash::OLD_DLL {
        let program_dir = match &game.source {
            Source::Recomp { exe, .. } | Source::Local { exe } => Path::new(exe).parent().map(Path::to_path_buf),
            _ => None,
        }
        .unwrap_or_else(|| PathBuf::from(&game.install_dir));
        if let Some(cause) = crate::crash::old_dll_cause(&crate::crash::old_dll_clues(&program_dir)) {
            why = cause;
        }
    }
    let since = SystemTime::now() - played - Duration::from_secs(5);
    // The developer's `logs` folder first, then the game's own.
    let install = Path::new(&game.install_dir);
    let logs = recomp::entry_for(game).and_then(|e| e.manifest).map(|m| m.folder("logs", install)).unwrap_or_default();
    let log = logs.iter().find_map(|dir| crate::crash::newest_log(dir, since)).or_else(|| crate::crash::newest_log(install, since));
    let detail = match &log {
        Some(path) => tr!("{1} Log do jogo: {0}", path.display(), why),
        None => why.clone(),
    };
    crate::trail::note(app, &game.id, tk!("O jogo fechou com erro"), &Err(detail));
    crate::notify::tell(app, crate::notify::Kind::Crashed, &tr!("{} fechou com erro", game.name), &why);
}

fn record(app: &AppHandle, game: &Game, seconds: u64) {
    let closed_at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default();
    let trail = app.state::<crate::trail::Trail>();
    let notices = close_session(&app.state::<LocalLibrary>(), &app.state::<Sessions>(), &app.state::<recomp::Folders>(), &trail, game, seconds, closed_at);
    if let Ok(data) = app.path().app_data_dir() {
        crate::play_log::record(&data, &game.id, seconds, closed_at);
    }
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
    let played = played_text(seconds);
    trail.note(&game.id, tk!("Partida"), &saved.as_ref().map(|_| played).map_err(|e| tr!("Tempo não gravado: {}", e)));
    if let Err(e) = saved {
        notices.push(tr!("Não foi possível gravar o tempo de {0}: {1}", game.name, e));
    }
    if let Source::Recomp { recomp_id, .. } = &game.source {
        let result = recomp::backup_saves(folders, recomp_id, Path::new(&game.install_dir), tk!("Depois de jogar"));
        trail.note(&game.id, tk!("Backup depois de jogar"), &backup_outcome(&result));
        if let Err(e) = result {
            notices.push(tr!("Falha no backup de {0}: {1}", game.name, e));
        }
    }
    notices
}

/// "1 h 5 min", "3 min", "40 s".
/// "1 min jogado", "5 min jogados": what a session's history line says.
pub fn played_text(seconds: u64) -> String {
    let time = duration_text(seconds);
    if time == "1 min" || time == "1 s" { format!("{time} jogado") } else { format!("{time} jogados") }
}

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
        Ok(Some(backup)) if backup.files == 1 => Ok("1 arquivo copiado".into()),
        Ok(Some(backup)) => Ok(format!("{} arquivos copiados", backup.files)),
        Ok(None) => Ok(tr!("Nada novo para copiar").into()),
        Err(e) => Err(e.to_string()),
    }
}

pub fn release(app: &AppHandle, id: &str) {
    if let Ok(data) = app.path().app_data_dir() {
        crate::looks::take_back(&data, id);
    }
    app.state::<Sessions>().lock().remove(id);
    crate::overlay::game_closed(app, id);
    notify(app, id, false);
    crate::background::game_ended(app);
}

fn notify(app: &AppHandle, game_id: &str, running: bool) {
    crate::in_game::session(app, running);
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

    #[test]
    fn windows_refusals_are_explained() {
        assert!(blocked_by_windows(Some(4551)).unwrap().contains("Smart App Control"));
        assert!(blocked_by_windows(Some(225)).is_some() && blocked_by_windows(Some(740)).is_some() && blocked_by_windows(Some(1260)).is_some());
        assert!(blocked_by_windows(Some(2)).is_none() && blocked_by_windows(None).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn crash_reporters_and_updaters_are_not_the_game() {
        for helper in ["UnityCrashHandler64.exe", "crashpad_handler.exe", "CrashReportClient.exe", "Game-Updater.exe", "unins000.exe", "WerFault.exe", "BsSndRpt.exe-bugsplat.exe"] {
            assert!(windows::is_helper(helper), "{helper}");
        }
        for game in ["Zelda64Recompiled.exe", "SpaghettiKart.exe", "Hollow Knight.exe", "ReStreet.exe", "UpdateGame.exe"] {
            assert!(!windows::is_helper(game), "{game}");
        }
    }
    use std::os::windows::process::CommandExt;
    use std::process::Stdio;
    use std::time::Instant;

    #[test]
    fn setup_steps_say_how_far_they_are() {
        assert_eq!(progress_line("progress 0.4 Unpacking menu art"), Some((Some(0.4), "Unpacking menu art".into())));
        assert_eq!(progress_line("@note 89%, 68606 files"), Some((Some(0.89), String::new())));
        assert_eq!(progress_line("@step 2 3 Unpacking the disc"), Some((None, "Unpacking the disc".into())));
        assert_eq!(progress_line("some log line"), None);
    }

    /// A window shown for a moment, then the program's own exit code, from PowerShell.
    #[cfg(windows)]
    fn window_then_exit(close_first: bool) -> Vec<String> {
        let script = if close_first {
            "Add-Type -AssemblyName System.Windows.Forms; $f=New-Object Windows.Forms.Form; $f.Show(); [Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 900; $f.Close(); Start-Sleep -Milliseconds 1500; exit -1073741819"
        } else {
            "Add-Type -AssemblyName System.Windows.Forms; $f=New-Object Windows.Forms.Form; $f.Show(); [Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 1500; exit -1073741819"
        };
        let mut game = Command::new("powershell.exe");
        game.args(["-NoProfile", "-NonInteractive", "-Command", script]).stdout(Stdio::null()).creation_flags(0x0800_0000);
        let process = spawn_game(game, None).unwrap();
        let mut seen = Vec::new();
        process
            .observe(|event| match event {
                Observation::WindowsClosedFirst => seen.push("closed first".to_string()),
                Observation::Exited(code) => seen.push(format!("exit {code:#X}")),
                _ => {}
            })
            .unwrap();
        seen
    }

    #[test]
    #[cfg(windows)]
    fn a_crash_after_the_window_closed_is_told_apart() {
        assert_eq!(window_then_exit(true), ["closed first", "exit 0xC0000005"]);
        assert_eq!(window_then_exit(false), ["exit 0xC0000005"], "the window was there when it fell: a real crash");
    }

    #[test]
    fn a_game_that_crashes_reports_its_windows_exit_code() {
        // cmd ends with the "missing DLL" status a broken game would.
        let mut game = Command::new("cmd.exe");
        game.args(["/c", "exit", "-1073741515"]).stdout(Stdio::null()).creation_flags(0x0800_0000);
        let process = spawn_game(game, None).unwrap();
        let mut seen = Vec::new();
        process
            .observe(|event| match event {
                Observation::Exited(code) => seen.push(format!("exit {code:#X}")),
                Observation::Closed(_) => seen.push("closed".into()),
                Observation::Started(_) | Observation::WindowsClosedFirst => {}
            })
            .unwrap();
        assert_eq!(seen, ["exit 0xC0000135", "closed"]);
        assert!(crate::crash::reason(0xC000_0135_u32 as i32, 1).unwrap().contains("DLL"));
    }

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
