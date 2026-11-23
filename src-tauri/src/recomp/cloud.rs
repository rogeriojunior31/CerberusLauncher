//! A local marker records which synced folder a backup was made for.
use cerberus_shared::tr;
use std::{fs, io, path::{Path, PathBuf}};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

use super::Folders;

pub(super) fn queue(archive: &Path, destination: &Path) -> io::Result<()> {
    let parent = archive.parent().ok_or(io::ErrorKind::InvalidInput)?;
    let marker = NamedTempFile::new_in(parent)?;
    serde_json::to_writer(marker.as_file(), destination)?;
    marker.as_file().sync_all()?;
    marker.persist(archive.with_extension("upload"))?;
    Ok(())
}

fn entries(dir: &Path) -> io::Result<impl Iterator<Item = io::Result<fs::DirEntry>>> {
    let list = match fs::read_dir(dir) {
        Ok(entries) => Some(entries),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    Ok(list.into_iter().flatten())
}

pub(super) fn stage(mut source: impl io::Read, dir: &Path) -> io::Result<NamedTempFile> {
    fs::create_dir_all(dir)?;
    let mut temp = NamedTempFile::new_in(dir)?;
    io::copy(&mut source, temp.as_file_mut())?;
    temp.as_file().sync_all()?;
    Ok(temp)
}

fn digest(file: &Path) -> io::Result<[u8; 32]> {
    let mut hash = Sha256::new();
    io::copy(&mut fs::File::open(file)?, &mut hash)?;
    Ok(hash.finalize().into())
}

pub(super) fn publish(temp: NamedTempFile, target: &Path) -> io::Result<()> {
    match temp.persist_noclobber(target) {
        Ok(_) => Ok(()),
        // A restart after publishing, or another retry in flight.
        Err(e) if e.error.kind() == io::ErrorKind::AlreadyExists => {
            if digest(target)? == digest(e.file.path())? { Ok(()) } else { Err(e.error) }
        }
        Err(e) => Err(e.error),
    }
}

fn send(archive: &Path, root: &Path, destination: &Path, folders: &Folders) -> io::Result<()> {
    // A missing sync root can be an unmounted drive.
    if !root.is_dir() { return Err(io::ErrorKind::NotFound.into()); }
    let name = archive.file_name().ok_or(io::ErrorKind::InvalidInput)?;
    let temp = stage(fs::File::open(archive)?, destination)?;
    // Disabling or switching sync while the copy ran cancels its publication.
    let current = folders.cloud.read().unwrap_or_else(|p| p.into_inner());
    if current.as_deref() == Some(root) {
        publish(temp, &destination.join(name))?;
        match fs::remove_file(archive.with_extension("upload")) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            result => result?,
        }
    }
    Ok(())
}

fn pending(backups: &Path, destination: &Path) -> io::Result<Vec<PathBuf>> {
    let mut pending = Vec::new();
    for file in entries(backups)? {
        let marker = file?.path();
        if marker.extension().is_none_or(|ext| ext != "upload") { continue; }
        let receipt = match fs::File::open(&marker) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue, // Another retry finished.
            Err(e) => return Err(e),
        };
        let queued: PathBuf = serde_json::from_reader(receipt)?;
        let archive = marker.with_extension("zip");
        if queued == destination && archive.try_exists()? { pending.push(archive); }
    }
    Ok(pending)
}

pub(super) fn retry_game(folders: &Folders, id: &str) -> Result<(), String> {
    let root = folders.cloud.read().unwrap_or_else(|p| p.into_inner()).clone();
    let Some(root) = root else { return Ok(()) };
    let destination = root.join("Cerberus Saves").join(id);
    let mut result = Ok(());
    let archives = pending(&folders.backups_dir(id), &destination)
        .map_err(|e| tr!("Não foi possível ler os envios pendentes de {}: {}", id, e))?;
    for archive in archives {
        if let Err(e) = send(&archive, &root, &destination, folders) {
            result = Err(tr!("Backup local guardado. Falha ao enviar para {0}: {1}", destination.display(), e));
        }
    }
    result
}

fn games(folders: &Folders) -> io::Result<Vec<String>> {
    let mut games = Vec::new();
    for entry in entries(&folders.saves)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() && let Ok(id) = entry.file_name().into_string() {
            games.push(id);
        }
    }
    Ok(games)
}

pub(crate) fn pending_count(folders: &Folders) -> usize {
    let root = folders.cloud.read().unwrap_or_else(|p| p.into_inner()).clone();
    let Some(root) = root else { return 0 };
    let Ok(games) = games(folders) else { return 0 };
    games.iter().filter_map(|id| pending(&folders.backups_dir(id), &root.join("Cerberus Saves").join(id)).ok())
        .map(|archives| archives.len()).sum()
}

pub(crate) fn retry(folders: &Folders) -> Result<(), String> {
    if folders.cloud.read().unwrap_or_else(|p| p.into_inner()).is_none() { return Ok(()); }
    let mut result = Ok(());
    for id in games(folders).map_err(|e| tr!("Não foi possível consultar a pasta de backups: {}", e))? {
        if let Err(e) = retry_game(folders, &id) { result = Err(e); }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn partial_copies_are_hidden_and_name_collisions_preserve_both_versions() {
        struct Disconnected;
        impl Read for Disconnected {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::ErrorKind::ConnectionReset.into())
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let interrupted = (&b"partial"[..]).chain(Disconnected);
        assert!(stage(interrupted, dir.path()).is_err());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
        let target = dir.path().join("test.zip");
        publish(stage(&b"complete"[..], dir.path()).unwrap(), &target).unwrap();
        publish(stage(&b"complete"[..], dir.path()).unwrap(), &target).unwrap();
        assert!(publish(stage(&b"other"[..], dir.path()).unwrap(), &target).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"complete");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
