//! An update is prepared beside the live installation. The generation in library.json is the
//! commit record. Until it advances, the previous directory always wins on recovery.
use cerberus_shared::tr;
use cerberus_shared::{Game, Source};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
struct Intent {
    game: String,
    target: PathBuf,
    generation: u64,
}

pub(super) struct Update {
    target: PathBuf,
    work: PathBuf,
    owner: String,
    // Outside the work directory so cleanup also works with Windows file locking.
    _lock: fs::File,
}

fn paths(game: &Game) -> Result<(PathBuf, PathBuf), String> {
    let target = PathBuf::from(&game.install_dir);
    let parent = target
        .parent()
        .filter(|_| target.is_absolute() && target.file_name().is_some())
        .ok_or(tr!("Pasta de instalação inválida para atualizar."))?;
    let key = format!("{:x}", Sha256::digest(game.id.as_bytes()));
    let work = parent.join(format!(".cerberus-update-{key}"));
    Ok((target, work))
}

fn lock(target: PathBuf, work: PathBuf, game: &Game) -> Result<Update, String> {
    let Source::Recomp { recomp_id, .. } = &game.source else {
        return Err(tr!("Esperado um recomp.").into());
    };
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(work.with_extension("lock"))
        .map_err(|e| e.to_string())?;
    file.try_lock().map_err(|e| {
        tr!("Não foi possível reservar a atualização de {0}: {1}", target.display(), e)
    })?;
    Ok(Update {
        target,
        work,
        owner: recomp_id.clone(),
        _lock: file,
    })
}

fn exists(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

impl Update {
    pub(super) fn begin(game: &Game) -> Result<Self, String> {
        let (target, work) = paths(game)?;
        let tx = lock(target, work, game)?;
        tx.reconcile(game)?;
        if tx.work.exists() {
            return Err(tr!("Não foi possível limpar a atualização anterior em {}.", tx.work.display()));
        }
        let Source::Recomp { generation, .. } = game.source else {
            return Err(tr!("Esperado um recomp.").into());
        };
        let parent = tx.work.parent().unwrap();
        let initial = tempfile::Builder::new()
            .prefix(".cerberus-init-")
            .tempdir_in(parent)
            .map_err(|e| e.to_string())?;
        let intent = Intent {
            game: game.id.clone(),
            target: tx.target.clone(),
            generation,
        };
        let bytes = serde_json::to_vec(&intent).map_err(|e| e.to_string())?;
        let mut record =
            fs::File::create(initial.path().join("intent.json")).map_err(|e| e.to_string())?;
        record
            .write_all(&bytes)
            .and_then(|()| record.sync_all())
            .map_err(|e| e.to_string())?;
        drop(record);
        fs::rename(initial.path(), &tx.work).map_err(|e| e.to_string())?;
        Ok(tx)
    }

    pub(super) fn candidate(&self) -> PathBuf {
        self.work.join("next")
    }

    pub(super) fn publish(&self) -> Result<(), String> {
        // The developer's file describes one release: the old one isn't carried
        // into a new release that dropped or moved it.
        let own_file = self.candidate().join(super::manifest::FILE);
        let new_has_it = own_file.exists();
        // What this release ships, and what the one installed shipped: a file the
        // player changed since (a setting, a mod, a swapped DLL) is kept.
        let shipped = release_files(&self.candidate());
        let installed = read_release_files(&self.target);
        preserve(&self.target, &self.candidate(), Path::new(""), installed.as_ref())
            .map_err(|e| tr!("Falha ao preparar a atualização: {}", e))?;
        write_release_files(&self.candidate(), &shipped);
        if !new_has_it {
            let _ = fs::remove_file(&own_file);
        }
        // An antivirus checking the new files (or the old ones) may hold them a moment.
        super::throttle::rename_patiently(&self.target, &self.work.join("previous"), 20)
            .map_err(|e| tr!("Falha ao reservar a versão anterior: {}", e))?;
        super::throttle::rename_patiently(&self.candidate(), &self.target, 20)
            .map_err(|e| tr!("Falha ao ativar a atualização: {}", e))
    }

    // Remove the intent last. An interrupted cleanup is safe to resume.
    fn cleanup(&self) -> io::Result<()> {
        for name in ["next", "previous", "extract"] {
            let path = self.work.join(name);
            if exists(&path)? {
                fs::remove_dir_all(path)?;
            }
        }
        fs::remove_file(self.work.join("intent.json"))?;
        fs::remove_dir(&self.work)
    }

    fn rollback(&self) -> io::Result<()> {
        let previous = self.work.join("previous");
        if exists(&previous)? {
            if exists(&self.target)? {
                ordinary_file_type(&self.target)?;
                let owned = fs::read_to_string(self.target.join(super::MARKER))
                    .is_ok_and(|id| id == self.owner);
                if exists(&self.candidate())? || !owned {
                    return Err(io::Error::other(
                        tr!("a pasta do jogo foi recriada durante a atualização"),
                    ));
                }
                fs::rename(&self.target, self.candidate())?;
            }
            fs::rename(previous, &self.target)?;
        }
        self.cleanup()
    }

    pub(super) fn finish<T>(self, result: Result<T, String>) -> Result<T, String> {
        match result {
            Ok(value) => {
                // Registration already committed. A leftover backup can be cleaned on restart.
                let _ = self.cleanup();
                Ok(value)
            }
            Err(error) => match self.rollback() {
                Ok(()) => Err(error),
                Err(e) => Err(tr!("{1} · Recuperação pendente em {0}: {2}", self.work.display(), error, e)),
            },
        }
    }

    fn reconcile(&self, game: &Game) -> Result<(), String> {
        if !exists(&self.work).map_err(|e| e.to_string())? {
            return Ok(());
        }
        ordinary_file_type(&self.work).map_err(|e| e.to_string())?;
        let path = self.work.join("intent.json");
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                // Cleanup may have stopped between removing the record and its empty directory.
                return fs::remove_dir(&self.work).map_err(|e| {
                    tr!("Pasta de recuperação inesperada em {0}: {1}", self.work.display(), e)
                });
            }
            Err(e) => return Err(tr!("Falha ao ler {0}: {1}", path.display(), e)),
        };
        let intent: Intent = serde_json::from_slice(&bytes).map_err(|e| {
            tr!("Registro de atualização inválido em {0}: {1}", path.display(), e)
        })?;
        if intent.game != game.id || intent.target != self.target {
            return Err(tr!("A atualização em {} pertence a outra instalação.", self.work.display()));
        }
        let Source::Recomp { generation, .. } = game.source else {
            return Err(tr!("Esperado um recomp.").into());
        };
        let result = if intent.generation == generation {
            self.rollback()
        } else if intent.generation.checked_add(1) == Some(generation) && self.target.is_dir() {
            // A locked old file can delay cleanup without blocking use of the committed game.
            let _ = self.cleanup();
            Ok(())
        } else {
            return Err(tr!("Confira a versão instalada e a cópia em {}.", self.work.display()));
        };
        result.map_err(|e| tr!("Falha ao recuperar {0}: {1}", self.target.display(), e))
    }
}

pub(crate) fn recover(game: &Game) -> Result<(), String> {
    if !matches!(game.source, Source::Recomp { .. }) {
        return Ok(());
    }
    let Ok((target, work)) = paths(game) else {
        return Ok(());
    };
    if !work.try_exists().map_err(|e| e.to_string())? {
        return Ok(());
    }
    lock(target, work, game)?.reconcile(game)
}

fn ordinary_file_type(path: &Path) -> io::Result<fs::FileType> {
    let metadata = fs::symlink_metadata(path)?;
    // Links and junctions only. Other reparse points are ordinary files to us:
    // OneDrive keeps every file of a synced folder as one (Files On-Demand), and
    // refusing them made every update of a game installed there fail.
    let linked = metadata.file_type().is_symlink();
    if linked || !(metadata.is_dir() || metadata.is_file()) {
        return Err(io::Error::other(tr!("Arquivo especial na instalação: {}", path.display())));
    }
    Ok(metadata.file_type())
}

/// Files a release ships, by path inside the game's folder, with their hash
/// (files up to this size: settings and DLLs are small; ROMs and packs aren't hashed).
const RELEASE_FILES: &str = ".cerberus-files.json";
const HASHED_UP_TO: u64 = 16 * 1024 * 1024;

fn hash_of(file: &Path) -> Option<String> {
    if fs::metadata(file).ok()?.len() > HASHED_UP_TO {
        return None;
    }
    Some(format!("{:x}", Sha256::digest(fs::read(file).ok()?)))
}

/// What's in a freshly unpacked release.
pub(super) fn release_files(dir: &Path) -> std::collections::BTreeMap<String, String> {
    let mut found = std::collections::BTreeMap::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(here) = pending.pop() {
        for entry in fs::read_dir(&here).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if let (Ok(rel), Some(hash)) = (path.strip_prefix(dir), hash_of(&path)) {
                let rel = rel.to_string_lossy().replace('\\', "/");
                if !rel.starts_with(".cerberus") {
                    found.insert(rel, hash);
                }
            }
        }
    }
    found
}

pub(super) fn write_release_files(dir: &Path, files: &std::collections::BTreeMap<String, String>) {
    let _ = crate::state_file::save(&dir.join(RELEASE_FILES), files);
}

fn read_release_files(dir: &Path) -> Option<std::collections::BTreeMap<String, String>> {
    match crate::state_file::read(&dir.join(RELEASE_FILES)) {
        crate::state_file::Read::Found(files) => Some(files),
        _ => None,
    }
}

// The new release supplies replacements. Carry everything else forward without writing into
// the live directory. A hard link avoids duplicating large ROMs; copying also works on exFAT.
// A file both have that the player changed since the installed release shipped it stays the
// player's; the new release's copy is kept beside it as `<name>.cerberus-new`.
fn preserve(from: &Path, to: &Path, rel: &Path, installed: Option<&std::collections::BTreeMap<String, String>>) -> io::Result<()> {
    ordinary_file_type(from)?;
    ordinary_file_type(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let src = entry.path();
        let dest = to.join(entry.file_name());
        let here = rel.join(entry.file_name());
        let ty = ordinary_file_type(&src)?;
        if exists(&dest)? {
            let new = ordinary_file_type(&dest)?;
            if ty.is_dir() && new.is_dir() {
                preserve(&src, &dest, &here, installed)?;
            } else if !(ty.is_file() && new.is_file()) {
                return Err(io::Error::other(tr!("Tipo de arquivo incompatível: {}", src.display())));
            } else {
                let key = here.to_string_lossy().replace('\\', "/");
                let changed = match installed.and_then(|files| files.get(&key)) {
                    Some(shipped) => hash_of(&src).is_some_and(|now| &now != shipped),
                    // Not known what the installed release shipped (installed by an
                    // older Cerberus, or too big to hash): a settings file that
                    // differs from the new one is taken as the player's.
                    None => is_settings_file(&here) && hash_of(&src).is_some_and(|now| Some(now) != hash_of(&dest)),
                };
                if changed {
                    let mut aside = dest.file_name().map(|n| n.to_os_string()).unwrap_or_default();
                    aside.push(".cerberus-new");
                    fs::rename(&dest, dest.with_file_name(aside))?;
                    fs::copy(&src, &dest)?;
                }
            }
        } else if ty.is_dir() {
            fs::create_dir(&dest)?;
            preserve(&src, &dest, &here, installed)?;
        } else if fs::hard_link(&src, &dest).is_err() {
            fs::copy(src, dest)?;
        }
    }
    Ok(())
}

/// A file players edit to change settings (kept on update when in doubt).
fn is_settings_file(path: &Path) -> bool {
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    matches!(ext.as_str(), "ini" | "cfg" | "conf" | "toml" | "json" | "yaml" | "yml" | "xml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::LocalLibrary;
    use crate::recomp::{CatalogEntry, install::tests::make_zip, prepare_release, register};

    struct Installation {
        root: tempfile::TempDir,
        target: PathBuf,
        release: PathBuf,
        entry: CatalogEntry,
        library: LocalLibrary,
        original: Game,
    }

    impl Installation {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let target = root.path().join("game");
            let release = root.path().join("release.zip");
            let library = LocalLibrary::load(root.path());
            let entry = CatalogEntry {
                id: "demo".into(),
                start: Some("Game.exe".into()),
                ..Default::default()
            };
            make_zip(&release, &[("Game.exe", "old")]);
            let (exe, _) = prepare_release(&release, &target, &entry, None, &entry.id).unwrap();
            let original = register(&library, &entry, None, "v1", &exe, &target, false).unwrap();
            fs::write(target.join("slot.sav"), "level 3").unwrap();
            fs::write(target.join("settings.ini"), "language=pt").unwrap();
            make_zip(
                &release,
                &[("Game.exe", "new"), ("assets/new.dat", "new content")],
            );
            Self {
                root,
                target,
                release,
                entry,
                library,
                original,
            }
        }

        fn stage(&self) -> (PathBuf, Update) {
            let (exe, tx) = prepare_release(
                &self.release,
                &self.target,
                &self.entry,
                Some(&self.original),
                &self.entry.id,
            )
            .unwrap();
            (exe, tx.unwrap())
        }

        fn record(&self, exe: &Path, version: &str) -> Result<Game, String> {
            register(
                &self.library,
                &self.entry,
                Some(&self.original),
                version,
                exe,
                &self.target,
                false,
            )
        }

        fn check_reopened(&self, updated: bool, tag: &str) {
            let library = LocalLibrary::load(self.root.path());
            let game = library.get(&self.original.id).unwrap().unwrap();
            let Source::Recomp {
                version,
                generation,
                exe,
                ..
            } = game.source
            else {
                panic!()
            };
            assert_eq!(version, tag);
            assert_eq!(generation, u64::from(updated));
            assert_eq!(
                fs::read_to_string(exe).unwrap(),
                if updated { "new" } else { "old" }
            );
            assert_eq!(self.target.join("assets/new.dat").exists(), updated);
            assert_eq!(
                fs::read_to_string(self.target.join("slot.sav")).unwrap(),
                "level 3"
            );
            assert_eq!(
                fs::read_to_string(self.target.join("settings.ini")).unwrap(),
                "language=pt"
            );
            assert_eq!(
                fs::read_to_string(self.target.join(crate::recomp::MARKER)).unwrap(),
                "demo"
            );
            assert!(!paths(&self.original).unwrap().1.exists());
        }
    }

    #[test]
    fn interrupt_during_preparation_or_between_directory_renames() {
        for moved_previous in [false, true] {
            let install = Installation::new();
            let tx = Update::begin(&install.original).unwrap();
            fs::create_dir(tx.candidate()).unwrap();
            fs::write(tx.candidate().join("partial"), "data").unwrap();
            if moved_previous {
                fs::rename(&install.target, tx.work.join("previous")).unwrap();
            }
            drop(tx);
            install.check_reopened(false, "v1");
        }
    }

    #[test]
    fn interrupt_after_publish_before_registration() {
        let install = Installation::new();
        let (_, tx) = install.stage();
        drop(tx);
        install.check_reopened(false, "v1");
        install.check_reopened(false, "v1");
    }

    #[test]
    fn interrupt_after_registration_of_same_version() {
        let install = Installation::new();
        let (exe, tx) = install.stage();
        install.record(&exe, "v1").unwrap();
        drop(tx);
        install.check_reopened(true, "v1");
    }

    #[test]
    fn failed_registration_rolls_back_immediately_and_retry_succeeds() {
        let install = Installation::new();
        let (exe, tx) = install.stage();
        let blocked = install.root.path().join("library.json.tmp");
        fs::create_dir(&blocked).unwrap();
        assert!(tx.finish(install.record(&exe, "v2")).is_err());
        assert_eq!(fs::read_to_string(&exe).unwrap(), "old");
        install.check_reopened(false, "v1");
        fs::remove_dir(blocked).unwrap();
        let (exe, tx) = install.stage();
        tx.finish(install.record(&exe, "v2")).unwrap();
        install.check_reopened(true, "v2");
    }

    #[test]
    fn an_update_keeps_settings_the_player_changed_and_updates_the_rest() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("game");
        let release = root.path().join("release.zip");
        let library = LocalLibrary::load(root.path());
        let entry = CatalogEntry { id: "demo".into(), start: Some("Game.exe".into()), ..Default::default() };
        make_zip(&release, &[("Game.exe", "v1"), ("config.ini", "A"), ("readme.txt", "r1")]);
        let (exe, _) = prepare_release(&release, &target, &entry, None, &entry.id).unwrap();
        let original = register(&library, &entry, None, "v1", &exe, &target, false).unwrap();
        // The player changes a setting the release ships.
        fs::write(target.join("config.ini"), "B").unwrap();
        make_zip(&release, &[("Game.exe", "v2"), ("config.ini", "A2"), ("readme.txt", "r2")]);
        let (exe, tx) = prepare_release(&release, &target, &entry, Some(&original), &entry.id).unwrap();
        tx.unwrap().finish(register(&library, &entry, Some(&original), "v2", &exe, &target, false)).unwrap();
        assert_eq!(fs::read_to_string(target.join("config.ini")).unwrap(), "B", "the player's setting stays");
        assert_eq!(fs::read_to_string(target.join("config.ini.cerberus-new")).unwrap(), "A2", "the new default is beside it");
        assert_eq!(fs::read_to_string(target.join("readme.txt")).unwrap(), "r2", "untouched files take the new version");
        assert_eq!(fs::read_to_string(target.join("Game.exe")).unwrap(), "v2");
    }

    #[test]
    fn an_install_from_an_older_cerberus_keeps_edited_settings_too() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("game");
        let release = root.path().join("release.zip");
        let library = LocalLibrary::load(root.path());
        let entry = CatalogEntry { id: "demo".into(), start: Some("Game.exe".into()), ..Default::default() };
        make_zip(&release, &[("Game.exe", "v1"), ("config.ini", "A"), ("data.pak", "d1")]);
        let (exe, _) = prepare_release(&release, &target, &entry, None, &entry.id).unwrap();
        let original = register(&library, &entry, None, "v1", &exe, &target, false).unwrap();
        // Installed before the record of shipped files existed.
        fs::remove_file(target.join(RELEASE_FILES)).unwrap();
        fs::write(target.join("config.ini"), "B").unwrap();
        make_zip(&release, &[("Game.exe", "v2"), ("config.ini", "A2"), ("data.pak", "d2")]);
        let (exe, tx) = prepare_release(&release, &target, &entry, Some(&original), &entry.id).unwrap();
        tx.unwrap().finish(register(&library, &entry, Some(&original), "v2", &exe, &target, false)).unwrap();
        assert_eq!(fs::read_to_string(target.join("config.ini")).unwrap(), "B", "the player's setting stays");
        assert_eq!(fs::read_to_string(target.join("config.ini.cerberus-new")).unwrap(), "A2");
        assert_eq!(fs::read_to_string(target.join("data.pak")).unwrap(), "d2", "game data takes the new version");
    }

    #[test]
    fn conflicting_file_types_abort_before_switch() {
        let install = Installation::new();
        fs::write(install.target.join("assets"), "custom data").unwrap();
        assert!(
            prepare_release(
                &install.release,
                &install.target,
                &install.entry,
                Some(&install.original),
                &install.entry.id,
            )
            .is_err()
        );
        assert_eq!(
            fs::read_to_string(install.target.join("assets")).unwrap(),
            "custom data"
        );
        install.check_reopened(false, "v1");
    }
}
