//! Recomps whose release is an installer: the first Play runs it, and when it
//! closes the launcher looks for the game it installed. First in the recomp's
//! own folder (some installers unpack beside themselves), then where Windows
//! says the installer put it (Settings > Apps). If neither finds it, the window
//! asks the player to point at the game's program.

use cerberus_shared::tk;
use cerberus_shared::tr;
use std::path::{Path, PathBuf};
use std::process::Command;

use cerberus_shared::{Game, Source};
use tauri::{AppHandle, Emitter, Manager};

use crate::library::LocalLibrary;
use crate::recomp;

pub const NEEDS_LOCATION: &str = "setup-needs-location";

pub fn run(app: &AppHandle, game: &Game, installer: &Path) -> Result<(), String> {
    let lower = installer.to_string_lossy().to_lowercase();
    let msi_package = lower.ends_with(".msi");
    let mut command = if msi_package {
        let mut msi = Command::new("msiexec");
        msi.arg("/i").arg(installer);
        msi
    } else {
        Command::new(installer)
    };
    let dir = installer.parent().unwrap_or(Path::new(".")).to_path_buf();
    // An installer that asks for administrator rights goes through Windows'
    // permission prompt; its end is waited for all the same.
    let wait: Box<dyn FnOnce() -> Result<Option<i32>, String> + Send> = match command.current_dir(&dir).spawn() {
        Ok(mut child) => Box::new(move || child.wait().map(|s| s.code()).map_err(|e| tr!("Falha ao aguardar o instalador: {}", e))),
        Err(e) if crate::elevated::needs_admin(&e) => {
            let installer = installer.to_path_buf();
            Box::new(move || crate::elevated::run_and_wait(&installer, "", &dir).map_err(crate::elevated::failure))
        }
        Err(e) => return Err(tr!("Não foi possível abrir o instalador: {}", e)),
    };
    let _ = app.emit("toast", tr!("Instalador de {} aberto. Siga os passos dele; quando ele fechar, o Cerberus procura o jogo.", game.name));

    let app = app.clone();
    let game = game.clone();
    std::thread::spawn(move || {
        let outcome = wait().and_then(|code| installer_result(code, msi_package));
        crate::trail::note(&app, &game.id, tk!("Instalador"), &outcome.clone().map(|()| tr!("Instalador concluído").to_string()));
        match outcome {
            Ok(()) => finish(&app, &game),
            Err(message) => { let _ = app.emit("toast", message); }
        }
        crate::sessions::release(&app, &game.id);
    });
    Ok(())
}

/// Games whose installer ran while Cerberus was closed (or closed before it
/// finished): if the game is already installed, it's ready without asking.
pub fn recover(app: &AppHandle) {
    let Ok(games) = app.state::<LocalLibrary>().games() else { return };
    for game in games.into_iter().filter(|g| g.setup_pending) {
        if let Some(program) = found_program(app, &game) {
            if use_program(app, &game.id, &program).is_ok() {
                crate::trail::note(app, &game.id, tk!("Localizar o jogo"), &Ok(tr!("Encontrado depois de reiniciar: {}", program.display())));
            }
        }
    }
}

/// The game, when its installer already put it somewhere, so Play doesn't run
/// the installer a second time.
pub fn already_installed(app: &AppHandle, game: &Game) -> Option<Game> {
    let program = found_program(app, game)?;
    let found = use_program(app, &game.id, &program).ok()?;
    crate::trail::note(app, &game.id, tk!("Localizar o jogo"), &Ok(tr!("Já instalado: {}", program.display())));
    Some(found)
}

fn installer_result(code: Option<i32>, msi_package: bool) -> Result<(), String> {
    match code {
        Some(0) => Ok(()),
        Some(1641 | 3010) if msi_package => Ok(()),
        Some(1602) if msi_package => Err(tr!("Instalação cancelada. Você pode tentar de novo em Jogar.").into()),
        Some(code) => Err(tr!("O instalador encerrou com código {}. Você pode tentar de novo em Jogar.", code)),
        None => Err(tr!("O instalador foi interrompido. Você pode tentar de novo em Jogar.").into()),
    }
}

fn finish(app: &AppHandle, game: &Game) {
    let Some(program) = found_program(app, game) else {
        crate::trail::note(app, &game.id, tk!("Localizar o jogo"), &Err(tr!("O instalador terminou, mas o jogo instalado não foi encontrado. Escolha o programa dele.").into()));
        let _ = app.emit(NEEDS_LOCATION, game.id.clone());
        return;
    };
    match use_program(app, &game.id, &program) {
        Ok(_) => {
            crate::trail::note(app, &game.id, tk!("Localizar o jogo"), &Ok(program.display().to_string()));
            crate::notify::tell(app, crate::notify::Kind::Installed, &tr!("{} está pronto", game.name), tr!("O instalador terminou e o jogo foi encontrado."));
        }
        Err(error) => {
            crate::trail::note(app, &game.id, tk!("Localizar o jogo"), &Err(error.clone()));
            let _ = app.emit("toast", error);
        }
    }
}

/// Points the game at its program, now that the installer has run.
pub fn use_program(app: &AppHandle, game_id: &str, program: &Path) -> Result<Game, String> {
    if !program.is_file() {
        return Err(tr!("{} não existe.", program.display()));
    }
    let exe = program.to_string_lossy().into_owned();
    let library = app.state::<LocalLibrary>();
    let game = library.update(game_id, |game| {
        if let Source::Recomp { exe: current, .. } | Source::Local { exe: current } = &mut game.source {
            *current = exe;
        }
        game.setup_pending = false;
    })?;
    let _ = app.emit("catalog-changed", ());
    Ok(game)
}

/// Where the installer put the game, if it can be found.
/// The game's program, found by itself, unless it is another library game's:
/// then the player is asked to point at it.
fn found_program(app: &AppHandle, game: &Game) -> Option<PathBuf> {
    let program = locate(game)?;
    let others = app.state::<LocalLibrary>().games().unwrap_or_default();
    (!claimed_by_another(&others, &game.id, &program)).then_some(program)
}

/// Whether `program` sits in the folder of another game in the library.
fn claimed_by_another(games: &[Game], me: &str, program: &Path) -> bool {
    games.iter().filter(|g| g.id != me && !g.install_dir.is_empty()).any(|g| program.starts_with(&g.install_dir))
}

/// A store's own library folder: the game there is the store's, not the recomp's.
fn store_folder(dir: &Path) -> bool {
    let text = format!("{}\\", dir.to_string_lossy().to_lowercase().replace('/', "\\"));
    ["\\steamapps\\", "\\epic games\\", "\\xboxgames\\", "\\gog galaxy\\", "\\windowsapps\\"].iter().any(|s| text.contains(s))
}

fn locate(game: &Game) -> Option<PathBuf> {
    let Source::Recomp { recomp_id, exe, .. } = &game.source else {
        return None;
    };
    let installer = PathBuf::from(exe);
    let name = recomp::find(recomp_id).map(|e| e.name).unwrap_or_else(|| game.name.clone());
    let hints = [name.as_str(), game.name.as_str()];
    // Beside the installer, but never the installer itself.
    let here = recomp::find_game_program(Path::new(&game.install_dir), &hints).filter(|p| *p != installer);
    here.or_else(|| installed_location(&name).filter(|dir| !store_folder(dir)).and_then(|dir| recomp::find_game_program(&dir, &hints)))
}

#[cfg(windows)]
fn installed_location(name: &str) -> Option<PathBuf> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
    const UNINSTALL_32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";
    let places = [(HKEY_CURRENT_USER, UNINSTALL), (HKEY_LOCAL_MACHINE, UNINSTALL), (HKEY_LOCAL_MACHINE, UNINSTALL_32)];
    for (root, path) in places {
        let Ok(list) = RegKey::predef(root).open_subkey(path) else {
            continue;
        };
        for app in list.enum_keys().flatten() {
            let Ok(entry) = list.open_subkey(&app) else {
                continue;
            };
            let shown: String = entry.get_value("DisplayName").unwrap_or_default();
            if !same_name(name, &shown) {
                continue;
            }
            let location: String = entry.get_value("InstallLocation").unwrap_or_default();
            let location = location.trim().trim_matches('"').to_string();
            if !location.is_empty() && Path::new(&location).is_dir() {
                return Some(PathBuf::from(location));
            }
            // No folder recorded: the icon is often the game's own program.
            let icon: String = entry.get_value("DisplayIcon").unwrap_or_default();
            let icon = icon.split(',').next().unwrap_or_default().trim_matches('"').to_string();
            if let Some(dir) = Path::new(&icon).parent().filter(|d| d.is_dir()) {
                return Some(dir.to_path_buf());
            }
        }
    }
    None
}

#[cfg(not(windows))]
fn installed_location(_name: &str) -> Option<PathBuf> {
    None
}

/// Every word of the recomp's name is in the installed app's name: "Sonic
/// Unleashed" (the original game) is not "Sonic Unleashed Recompiled".
fn same_name(wanted: &str, shown: &str) -> bool {
    let words = |s: &str| -> Vec<String> {
        s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() > 1 && *w != "the").map(str::to_string).collect()
    };
    let wanted = words(wanted);
    let shown = words(shown);
    if wanted.is_empty() || shown.is_empty() {
        return false;
    }
    let shared = wanted.iter().filter(|w| shown.contains(w)).count();
    shared == wanted.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn another_game_is_not_taken_for_the_recomp() {
        assert!(!same_name("Sonic Unleashed Recompiled", "Sonic Unleashed"));
        assert!(same_name("Sonic Unleashed Recompiled", "Sonic Unleashed Recompiled 1.0.3"));
        assert!(store_folder(Path::new(r"D:\SteamLibrary\steamapps\common\Sonic")));
        assert!(!store_folder(Path::new(r"C:\Program Files\UnleashedRecomp")));
        let other: Game = serde_json::from_value(serde_json::json!({
            "id": "steam:1", "name": "Sonic", "source": {"kind": "local", "exe": "x"}, "install_dir": r"D:\Games\Sonic"
        }))
        .unwrap();
        assert!(claimed_by_another(&[other.clone()], "recomp:x", Path::new(r"D:\Games\Sonic\sonic.exe")));
        assert!(!claimed_by_another(&[other], "steam:1", Path::new(r"D:\Games\Sonic\sonic.exe")));
    }

    #[test]
    fn only_success_can_trigger_game_detection() {
        assert!(installer_result(Some(0), false).is_ok());
        assert!(installer_result(Some(1602), true).unwrap_err().contains("cancelada"));
        assert!(installer_result(Some(1), false).is_err());
        assert!(installer_result(None, false).is_err());
        for code in [1641, 3010] {
            assert!(installer_result(Some(code), true).is_ok());
            assert!(installer_result(Some(code), false).is_err());
        }
    }

    #[test]
    fn a_game_its_installer_already_unpacked_is_found_but_never_the_installer() {
        let dir = tempfile::tempdir().unwrap();
        let installer = dir.path().join("ReStreet-Setup.exe");
        std::fs::write(&installer, b"MZ").unwrap();
        let game: Game = serde_json::from_value(serde_json::json!({
            "id": "recomp:zz-test", "name": "ReStreet",
            "source": {"kind": "recomp", "recomp_id": "zz-test", "version": "1", "exe": installer.to_string_lossy(), "generation": 0},
            "install_dir": dir.path().to_string_lossy(), "setup_pending": true
        }))
        .unwrap();
        // Only the installer: nothing installed yet, so Play runs it.
        assert_eq!(locate(&game), None);
        let program = dir.path().join("ReStreet").join("ReStreet.exe");
        std::fs::create_dir_all(program.parent().unwrap()).unwrap();
        std::fs::write(&program, b"MZ").unwrap();
        assert_eq!(locate(&game), Some(program));
    }

    #[test]
    fn matches_installed_apps_by_name() {
        assert!(same_name("Burnout Revenge Recompiled", "Burnout Revenge (Recompiled) 1.2"));
        assert!(same_name("ReStreet", "ReStreet"));
        assert!(!same_name("Burnout Revenge Recompiled", "Burnout Paradise"));
        assert!(!same_name("FIFA Street PC Port", "Steam"));
    }
}
