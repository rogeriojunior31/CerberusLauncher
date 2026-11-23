//! Moving an installed recomp to another folder or drive, as Hydra's "move
//! game": the files go, then the library points at the new place.

use cerberus_shared::tr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use cerberus_shared::{Game, Source};

use super::{Folders, check_install_folder};
use crate::library::LocalLibrary;

/// Moves the game's folder into `into` (keeping its folder name) and updates
/// the library. On the same drive it is a rename; across drives the files are
/// copied to a temporary folder first, so a failure leaves the game where it was.
pub fn move_install(folders: &Folders, library: &LocalLibrary, game_id: &str, into: &Path) -> Result<Game, String> {
    let game = library.get(game_id)?.ok_or(tr!("Esse jogo não está mais na biblioteca."))?;
    let Source::Recomp { recomp_id, .. } = &game.source else {
        return Err(tr!("Só recomps instalados pelo Cerberus podem ser movidos por aqui.").into());
    };
    if game.external {
        return Err(tr!("Essa pasta é sua: mova pelo Windows e use \"Já tenho instalado\" de novo.").into());
    }
    let from = PathBuf::from(&game.install_dir);
    check_install_folder(folders, recomp_id, &from)?;
    let name = from.file_name().ok_or(tr!("A pasta do jogo não tem nome."))?;
    let to = into.join(name);
    if same_path(&to, &from) {
        return Err(tr!("O jogo já está nessa pasta.").into());
    }
    if starts_with(into, &from) {
        return Err(tr!("Escolha uma pasta fora da pasta do jogo.").into());
    }
    if to.exists() {
        return Err(tr!("Já existe uma pasta {} lá. Escolha outro lugar.", to.display()));
    }
    fs::create_dir_all(into).map_err(|e| tr!("Não foi possível usar {0}: {1}", into.display(), e))?;

    if fs::rename(&from, &to).is_err() {
        // Another drive: copy, then delete the old folder.
        crate::disk::require(into, folder_size(&from))?;
        let staging = into.join(format!(".moving-{}", name.to_string_lossy()));
        let _ = fs::remove_dir_all(&staging);
        let copied = copy_dir(&from, &staging).and_then(|()| fs::rename(&staging, &to));
        if let Err(e) = copied {
            let _ = fs::remove_dir_all(&staging);
            return Err(tr!("Não foi possível copiar o jogo: {}. Ele continua onde estava.", e));
        }
        // The copy is complete; a leftover old folder only costs space.
        let _ = fs::remove_dir_all(&from);
    }

    let rebase = |path: &str| match Path::new(path).strip_prefix(&from) {
        Ok(rest) => to.join(rest).to_string_lossy().into_owned(),
        Err(_) => path.to_string(),
    };
    library.update(game_id, |g| {
        g.install_dir = to.to_string_lossy().into_owned();
        if let Source::Recomp { exe, .. } = &mut g.source {
            *exe = rebase(exe);
        }
        g.tracking_exe = g.tracking_exe.as_deref().map(rebase);
        g.game_copy = g.game_copy.as_deref().map(rebase);
    })
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

/// `path` is `dir` or inside it, ignoring case as Windows does.
fn starts_with(path: &Path, dir: &Path) -> bool {
    let (path, dir) = (path.to_string_lossy().to_lowercase(), dir.to_string_lossy().to_lowercase());
    let dir = dir.trim_end_matches(['\\', '/']);
    path == dir || path.starts_with(&format!("{dir}\\")) || path.starts_with(&format!("{dir}/"))
}

fn folder_size(dir: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(dir) else { return 0 };
    entries
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => folder_size(&e.path()),
            Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

/// Copies a folder's files and subfolders. Links are not followed.
fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed(root: &Path) -> (Folders, LocalLibrary, Game) {
        let folders = Folders::new(&root.join("data"), None);
        let dir = root.join("games").join("srb2");
        fs::create_dir_all(dir.join("addons")).unwrap();
        fs::write(dir.join("srb2win.exe"), b"MZ").unwrap();
        fs::write(dir.join("addons").join("mod.pk3"), b"mod").unwrap();
        fs::write(dir.join(super::super::MARKER), "srb2").unwrap();
        let mut game = crate::library::game_from_exe(&dir.join("srb2win.exe"));
        game.source = Source::Recomp { recomp_id: "srb2".into(), version: "1".into(), exe: dir.join("srb2win.exe").to_string_lossy().into(), generation: 0 };
        game.install_dir = dir.to_string_lossy().into();
        let library = LocalLibrary::load(&root.join("data"));
        library.add(game.clone()).unwrap();
        (folders, library, game)
    }

    #[test]
    fn moves_the_folder_and_points_the_library_there() {
        let root = tempfile::tempdir().unwrap();
        let (folders, library, game) = installed(root.path());
        let into = root.path().join("D").join("Jogos");
        let moved = move_install(&folders, &library, &game.id, &into).unwrap();
        let to = into.join("srb2");
        assert_eq!(moved.install_dir, to.to_string_lossy());
        assert!(matches!(&moved.source, Source::Recomp { exe, .. } if Path::new(exe) == to.join("srb2win.exe")));
        assert_eq!(fs::read(to.join("addons").join("mod.pk3")).unwrap(), b"mod");
        assert!(!Path::new(&game.install_dir).exists());
        assert_eq!(library.get(&game.id).unwrap().unwrap().install_dir, moved.install_dir);
    }

    #[test]
    fn refuses_its_own_folder_and_an_occupied_place() {
        let root = tempfile::tempdir().unwrap();
        let (folders, library, game) = installed(root.path());
        let inside = Path::new(&game.install_dir).join("addons");
        assert!(move_install(&folders, &library, &game.id, &inside).is_err());
        let busy = root.path().join("E");
        fs::create_dir_all(busy.join("srb2")).unwrap();
        assert!(move_install(&folders, &library, &game.id, &busy).is_err());
        // Nothing moved.
        assert!(Path::new(&game.install_dir).join("srb2win.exe").is_file());
    }

    #[test]
    fn copies_everything_when_a_rename_is_not_possible() {
        let root = tempfile::tempdir().unwrap();
        let (_, _, game) = installed(root.path());
        let copy = root.path().join("copy");
        copy_dir(Path::new(&game.install_dir), &copy).unwrap();
        assert_eq!(folder_size(&copy), folder_size(Path::new(&game.install_dir)));
        assert_eq!(fs::read(copy.join("addons").join("mod.pk3")).unwrap(), b"mod");
    }
}
