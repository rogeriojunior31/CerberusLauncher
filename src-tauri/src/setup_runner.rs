//! Recomps whose release is an installer: the first Play runs it, and when it
//! closes the launcher looks for the game it installed. First in the recomp's
//! own folder (some installers unpack beside themselves), then where Windows
//! says the installer put it (Settings > Apps). If neither finds it, the window
//! asks the player to point at the game's program.

use std::path::{Path, PathBuf};
use std::process::Command;

use cerberus_shared::{Game, Source};
use tauri::{AppHandle, Emitter, Manager};

use crate::library::LocalLibrary;
use crate::recomp;

pub const NEEDS_LOCATION: &str = "setup-needs-location";

pub fn run(app: &AppHandle, game: &Game, installer: &Path) -> Result<(), String> {
    let lower = installer.to_string_lossy().to_lowercase();
    let mut command = if lower.ends_with(".msi") {
        let mut msi = Command::new("msiexec");
        msi.arg("/i").arg(installer);
        msi
    } else {
        Command::new(installer)
    };
    let mut child = command
        .current_dir(installer.parent().unwrap_or(Path::new(".")))
        .spawn()
        .map_err(|e| format!("Não foi possível abrir o instalador: {e}"))?;
    let _ = app.emit("toast", format!("Instalador de {} aberto. Siga os passos dele; quando ele fechar, o Cerberus procura o jogo.", game.name));

    let app = app.clone();
    let game = game.clone();
    std::thread::spawn(move || {
        let _ = child.wait();
        finish(&app, &game);
    });
    Ok(())
}

fn finish(app: &AppHandle, game: &Game) {
    let Some(program) = locate(game) else {
        let _ = app.emit(NEEDS_LOCATION, game.id.clone());
        return;
    };
    match use_program(app, &game.id, &program) {
        Ok(_) => {
            let _ = app.emit("toast", format!("{} foi instalado e está pronto para jogar.", game.name));
        }
        Err(error) => {
            let _ = app.emit("toast", error);
        }
    }
}

/// Points the game at its program, now that the installer has run.
pub fn use_program(app: &AppHandle, game_id: &str, program: &Path) -> Result<Game, String> {
    if !program.is_file() {
        return Err(format!("{} não existe.", program.display()));
    }
    let exe = program.to_string_lossy().into_owned();
    let library = app.state::<LocalLibrary>();
    let game = library.update(game_id, |game| {
        if let Source::Recomp { exe: current, .. } = &mut game.source {
            *current = exe;
        }
        game.setup_pending = false;
    })?;
    let _ = app.emit("catalog-changed", ());
    Ok(game)
}

/// Where the installer put the game, if it can be found.
fn locate(game: &Game) -> Option<PathBuf> {
    let Source::Recomp { recomp_id, exe, .. } = &game.source else {
        return None;
    };
    let installer = PathBuf::from(exe);
    let name = recomp::find(recomp_id).map(|e| e.name).unwrap_or_else(|| game.name.clone());
    let hints = [name.as_str(), game.name.as_str()];
    // Beside the installer, but never the installer itself.
    let here = recomp::find_game_program(Path::new(&game.install_dir), &hints).filter(|p| *p != installer);
    here.or_else(|| installed_location(&name).and_then(|dir| recomp::find_game_program(&dir, &hints)))
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

/// Most of the recomp's name words are in the installed app's name.
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
    shared * 2 > wanted.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_installed_apps_by_name() {
        assert!(same_name("Burnout Revenge Recompiled", "Burnout Revenge (Recompiled) 1.2"));
        assert!(same_name("ReStreet", "ReStreet"));
        assert!(!same_name("Burnout Revenge Recompiled", "Burnout Paradise"));
        assert!(!same_name("FIFA Street PC Port", "Steam"));
    }
}
