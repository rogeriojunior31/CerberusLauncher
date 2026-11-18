//! An update is prepared beside the live installation. The generation in library.json is the
//! commit record. Until it advances, the previous directory always wins on recovery.
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
        .ok_or("Pasta de instalação inválida para atualizar.")?;
    let key = format!("{:x}", Sha256::digest(game.id.as_bytes()));
    let work = parent.join(format!(".cerberus-update-{key}"));
    Ok((target, work))
}

fn lock(target: PathBuf, work: PathBuf, game: &Game) -> Result<Update, String> {
    let Source::Recomp { recomp_id, .. } = &game.source else {
        return Err("Esperado um recomp.".into());
    };
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(work.with_extension("lock"))
        .map_err(|e| e.to_string())?;
    file.try_lock().map_err(|e| {
        format!(
            "Não foi possível reservar a atualização de {}: {e}",
            target.display()
        )
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
            return Err(format!(
                "Não foi possível limpar a atualização anterior em {}.",
                tx.work.display()
            ));
        }
        let Source::Recomp { generation, .. } = game.source else {
            return Err("Esperado um recomp.".into());
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
        preserve(&self.target, &self.candidate())
            .map_err(|e| format!("Falha ao preparar a atualização: {e}"))?;
        fs::rename(&self.target, self.work.join("previous"))
            .map_err(|e| format!("Falha ao reservar a versão anterior: {e}"))?;
        fs::rename(self.candidate(), &self.target)
            .map_err(|e| format!("Falha ao ativar a atualização: {e}"))
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
                        "a pasta do jogo foi recriada durante a atualização",
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
                Err(e) => Err(format!(
                    "{error} · Recuperação pendente em {}: {e}",
                    self.work.display()
                )),
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
                    format!(
                        "Pasta de recuperação inesperada em {}: {e}",
                        self.work.display()
                    )
                });
            }
            Err(e) => return Err(format!("Falha ao ler {}: {e}", path.display())),
        };
        let intent: Intent = serde_json::from_slice(&bytes).map_err(|e| {
            format!(
                "Registro de atualização inválido em {}: {e}",
                path.display()
            )
        })?;
        if intent.game != game.id || intent.target != self.target {
            return Err(format!(
                "A atualização em {} pertence a outra instalação.",
                self.work.display()
            ));
        }
        let Source::Recomp { generation, .. } = game.source else {
            return Err("Esperado um recomp.".into());
        };
        let result = if intent.generation == generation {
            self.rollback()
        } else if intent.generation.checked_add(1) == Some(generation) && self.target.is_dir() {
            // A locked old file can delay cleanup without blocking use of the committed game.
            let _ = self.cleanup();
            Ok(())
        } else {
            return Err(format!(
                "Confira a versão instalada e a cópia em {}.",
                self.work.display()
            ));
        };
        result.map_err(|e| format!("Falha ao recuperar {}: {e}", self.target.display()))
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
    #[cfg(windows)]
    let linked = std::os::windows::fs::MetadataExt::file_attributes(&metadata) & 0x400 != 0;
    #[cfg(not(windows))]
    let linked = metadata.file_type().is_symlink();
    if linked || !(metadata.is_dir() || metadata.is_file()) {
        return Err(io::Error::other(format!(
            "Arquivo especial na instalação: {}",
            path.display()
        )));
    }
    Ok(metadata.file_type())
}

// The new release supplies replacements. Carry everything else forward without writing into
// the live directory. A hard link avoids duplicating large ROMs; copying also works on exFAT.
fn preserve(from: &Path, to: &Path) -> io::Result<()> {
    ordinary_file_type(from)?;
    ordinary_file_type(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let src = entry.path();
        let dest = to.join(entry.file_name());
        let ty = ordinary_file_type(&src)?;
        if exists(&dest)? {
            let new = ordinary_file_type(&dest)?;
            if ty.is_dir() && new.is_dir() {
                preserve(&src, &dest)?;
            } else if !(ty.is_file() && new.is_file()) {
                return Err(io::Error::other(format!(
                    "Tipo de arquivo incompatível: {}",
                    src.display()
                )));
            }
        } else if ty.is_dir() {
            fs::create_dir(&dest)?;
            preserve(&src, &dest)?;
        } else if fs::hard_link(&src, &dest).is_err() {
            fs::copy(src, dest)?;
        }
    }
    Ok(())
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
            let (exe, _) = prepare_release(&release, &target, &entry, None).unwrap();
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
    fn conflicting_file_types_abort_before_switch() {
        let install = Installation::new();
        fs::write(install.target.join("assets"), "custom data").unwrap();
        assert!(
            prepare_release(
                &install.release,
                &install.target,
                &install.entry,
                Some(&install.original)
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
