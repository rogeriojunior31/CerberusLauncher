//! Download, checksum, unpack, and find the game's program.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::{Client, StatusCode, header};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use super::assets::{Asset, Kind, kind, words};

pub const CANCELLED: &str = "Instalação cancelada.";
/// Tries per download. Each try carries on from the bytes already saved.
const TRIES: u32 = 4;

/// Downloads to a `.part` file, renamed once complete and checked, so a
/// half-finished file is never mistaken for a good one.
///
/// A dropped connection is tried again from where it stopped, and a download
/// interrupted earlier (the app closed, the PC slept) carries on next time: the
/// `.part` name includes the file's size and address, so it is only ever
/// continued with the same file.
pub async fn download(
    client: &Client,
    asset: &Asset,
    dir: &Path,
    cancel: &AtomicBool,
    report: impl Fn(u64, u64) + Send,
) -> Result<PathBuf, String> {
    tokio::fs::create_dir_all(dir).await.map_err(|e| e.to_string())?;
    let safe_name: String = asset.name.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '_' }).collect();
    let target = dir.join(&safe_name);
    let tag = &format!("{:x}", Sha256::digest(format!("{}|{}", asset.size, asset.url)))[..10];
    let part = dir.join(format!("{safe_name}.{tag}.part"));

    let (mut hash, mut done) = resume_state(&part, asset.size);
    // 0 until the server says, when the catalog didn't know the exact size.
    let mut total = asset.size;
    let mut tries = 0;
    loop {
        tries += 1;
        match fetch_into(client, asset, &part, &mut hash, &mut done, &mut total, cancel, &report).await {
            Ok(()) => break,
            Err(Stop::Cancelled) => {
                let _ = fs::remove_file(&part);
                return Err(CANCELLED.into());
            }
            Err(Stop::Fatal(message)) => return Err(message),
            Err(Stop::Retry(_)) if tries < TRIES => {
                tokio::time::sleep(Duration::from_secs(2 * tries as u64)).await;
            }
            Err(Stop::Retry(message)) => return Err(message),
        }
    }
    report(done, total);

    let got = format!("{:x}", hash.finalize());
    // Uploads from before GitHub kept checksums have nothing to compare against.
    let wrong = asset.sha256.as_ref().is_some_and(|expected| *expected != got);
    if wrong {
        let _ = fs::remove_file(&part);
        return Err("O arquivo baixado não bate com o checksum do GitHub, então foi descartado. Tente de novo.".into());
    }
    fs::rename(&part, &target).map_err(|e| e.to_string())?;
    Ok(target)
}

enum Stop {
    Cancelled,
    /// The connection failed; worth another try.
    Retry(String),
    Fatal(String),
}

/// What an earlier, interrupted download left: its bytes hashed so far.
fn resume_state(part: &Path, size: u64) -> (Sha256, u64) {
    let mut hash = Sha256::new();
    let Ok(mut file) = fs::File::open(part) else {
        return (hash, 0);
    };
    let mut buffer = vec![0u8; 1 << 20];
    let mut done = 0u64;
    loop {
        let Ok(read) = file.read(&mut buffer) else {
            return (Sha256::new(), 0);
        };
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
        done += read as u64;
    }
    // More than the whole file means it is not this file; start over.
    if size > 0 && done > size {
        let _ = fs::remove_file(part);
        return (Sha256::new(), 0);
    }
    (hash, done)
}

async fn fetch_into(
    client: &Client,
    asset: &Asset,
    part: &Path,
    hash: &mut Sha256,
    done: &mut u64,
    total: &mut u64,
    cancel: &AtomicBool,
    report: &(impl Fn(u64, u64) + Send),
) -> Result<(), Stop> {
    if *done == *total && *total > 0 {
        return Ok(());
    }
    // The client's short timeout is for quick questions; a big file on a slow line takes longer.
    let mut request = client.get(&asset.url).timeout(Duration::from_secs(6 * 60 * 60));
    if *done > 0 {
        request = request.header(header::RANGE, format!("bytes={done}-"));
    }
    let dropped = || Stop::Retry("A conexão caiu durante o download. Tente de novo; ele continua de onde parou.".into());
    let reply = request.send().await.map_err(|_| dropped())?;
    let status = reply.status();
    // Asked for the bytes after the end: the copy is already whole (the checksum says if it's right).
    if status == StatusCode::RANGE_NOT_SATISFIABLE && *done > 0 {
        return Ok(());
    }
    if *total == 0 {
        let rest = reply.content_length().unwrap_or(0);
        *total = if status == StatusCode::PARTIAL_CONTENT { *done + rest } else { rest };
    }
    let total = *total;
    let busy = status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS;
    if busy {
        return Err(Stop::Retry(format!("O GitHub está ocupado (resposta {}). Tente de novo em um minuto.", status.as_u16())));
    }
    if !status.is_success() {
        return Err(Stop::Fatal(format!("O GitHub respondeu {} ao baixar {}.", status.as_u16(), asset.name)));
    }
    // A plain 200 to a ranged request sends the whole file again: start the copy over.
    let continuing = status == StatusCode::PARTIAL_CONTENT;
    if !continuing {
        *hash = Sha256::new();
        *done = 0;
    }
    let mut options = tokio::fs::OpenOptions::new();
    options.create(true);
    if continuing { options.append(true) } else { options.write(true).truncate(true) };
    let mut out = options.open(part).await.map_err(|e| Stop::Fatal(e.to_string()))?;

    let mut stream = reply.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            return Err(Stop::Cancelled);
        }
        let chunk = chunk.map_err(|_| dropped())?;
        out.write_all(&chunk).await.map_err(|e| Stop::Fatal(e.to_string()))?;
        hash.update(&chunk);
        *done += chunk.len() as u64;
        report(*done, total);
    }
    out.flush().await.map_err(|e| Stop::Fatal(e.to_string()))?;
    Ok(())
}

/// Unpacks into `staging`, then moves the files over `target`. Files already in
/// `target` that the new release doesn't have (saves, settings) are kept.
pub fn unpack_over(archive: &Path, staging: &Path, target: &Path) -> Result<(), String> {
    let _ = fs::remove_dir_all(staging);
    fs::create_dir_all(staging).map_err(|e| e.to_string())?;
    let name = archive.file_name().unwrap_or_default().to_string_lossy();
    match kind(&name) {
        Some(Kind::Zip) => unzip(archive, staging)?,
        Some(Kind::SevenZip) => un7z(archive, staging)?,
        Some(Kind::TarGz) => untar_gz(archive, staging)?,
        // A release that is a program (the game, or its installer) goes in as it is.
        None if super::assets::is_program(&name) => {
            fs::copy(archive, staging.join(name.as_ref())).map_err(|e| e.to_string())?;
        }
        None => return Err(format!("{name} não é um formato que o Cerberus saiba instalar.")),
    }
    let root = single_top_folder(staging);
    overlay(&root, target).map_err(|e| format!("Não foi possível instalar os arquivos: {e}"))?;
    let _ = fs::remove_dir_all(staging);
    Ok(())
}

fn unzip(file: &Path, to: &Path) -> Result<(), String> {
    let broken = |e: zip::result::ZipError| format!("O arquivo baixado está corrompido: {e}");
    let reader = fs::File::open(file).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(reader).map_err(broken)?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(broken)?;
        // A name like "../../x" would write outside the game's folder.
        let Some(relative) = entry.enclosed_name() else {
            return Err(format!("O arquivo tem um caminho inseguro: {}", entry.name()));
        };
        let path = to.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = fs::File::create(&path).map_err(|e| e.to_string())?;
        io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(mode));
        }
    }
    Ok(())
}

/// Entries naming `..`, a root or a drive are refused by the library itself.
fn un7z(file: &Path, to: &Path) -> Result<(), String> {
    sevenz_rust2::decompress_file(file, to).map_err(|e| format!("O arquivo baixado está corrompido: {e}"))
}

/// `tar` refuses entries that would land outside `to`, and keeps Unix permissions.
fn untar_gz(file: &Path, to: &Path) -> Result<(), String> {
    let reader = fs::File::open(file).map_err(|e| e.to_string())?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(reader));
    archive.unpack(to).map_err(|e| format!("O arquivo baixado está corrompido: {e}"))
}

/// Many archives hold everything inside one `Game-v1.2/` folder; the game's files
/// should land straight in its install folder either way.
fn single_top_folder(dir: &Path) -> PathBuf {
    let Ok(entries) = fs::read_dir(dir) else {
        return dir.to_path_buf();
    };
    let entries: Vec<_> = entries.flatten().collect();
    match entries.as_slice() {
        [only] if only.path().is_dir() => only.path(),
        _ => dir.to_path_buf(),
    }
}

fn overlay(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            overlay(&source, &dest)?;
            continue;
        }
        // Rename is instant on the same drive (staging sits next to the target);
        // copy covers the rare case it is not.
        if fs::rename(&source, &dest).is_err() {
            fs::copy(&source, &dest)?;
        }
    }
    Ok(())
}

/// Programs that come with a game but are not the game.
const NOT_THE_GAME: &[&str] = &[
    "unins", "uninstall", "uninstaller", "setup", "install", "installer", "crash", "crashpad", "crashhandler",
    "redist", "vcredist", "vc_redist", "helper", "updater", "update", "server", "dedicated", "elevate", "report",
];

/// The game's program inside `dir`: the one whose name shares the most words with
/// `hints` (the recomp's and repo's names), then the one nearest the top.
pub fn find_program(dir: &Path, hints: &[&str]) -> Option<PathBuf> {
    let hint_words: Vec<String> = hints.iter().flat_map(|h| name_words(h)).collect();
    let mut found = Vec::new();
    collect_programs(dir, 0, &mut found);
    let mut best: Option<(i32, PathBuf)> = None;
    for (path, depth) in found {
        let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
        let stem_words = name_words(&stem);
        if stem_words.iter().any(|w| NOT_THE_GAME.contains(&w.as_str())) {
            continue;
        }
        let shared = stem_words.iter().filter(|w| hint_words.contains(w)).count() as i32;
        let score = shared * 10 - depth as i32 * 5;
        if best.as_ref().is_none_or(|(top, _)| score > *top) {
            best = Some((score, path));
        }
    }
    best.map(|(_, path)| path)
}

fn collect_programs(dir: &Path, depth: usize, found: &mut Vec<(PathBuf, usize)>) {
    // Deep folders hold libraries and tools, not the game's own program.
    if depth > 3 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_programs(&path, depth + 1, found);
            continue;
        }
        if is_program(&path) {
            found.push((path, depth));
        }
    }
}

#[cfg(windows)]
fn is_program(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
}

#[cfg(not(windows))]
fn is_program(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let lower = path.to_string_lossy().to_lowercase();
    if lower.ends_with(".appimage") {
        return true;
    }
    let runnable = fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0);
    // Libraries are runnable too on Linux; a program has no extension.
    runnable && path.extension().is_none()
}

/// Words of a program or project name, with CamelCase split:
/// "Zelda64Recompiled" gives zelda64 and recompiled.
fn name_words(name: &str) -> Vec<String> {
    let mut spaced = String::new();
    let mut previous: Option<char> = None;
    for c in name.chars() {
        if c.is_uppercase() && previous.is_some_and(|p| p.is_lowercase() || p.is_ascii_digit()) {
            spaced.push(' ');
        }
        spaced.push(c);
        previous = Some(c);
    }
    words(&spaced)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn empty_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cerberus-install-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"x").unwrap();
    }

    fn make_zip(path: &Path, files: &[(&str, &str)]) {
        let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        for (name, body) in files {
            zip.start_file(*name, options).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn unwraps_a_single_top_folder() {
        let dir = empty_dir("unwrap");
        let zip = dir.join("game.zip");
        make_zip(&zip, &[("Game-v1/Game.exe", "new"), ("Game-v1/data/a.bin", "a")]);
        let target = dir.join("installed");
        unpack_over(&zip, &dir.join("staging"), &target).unwrap();
        assert!(target.join("Game.exe").is_file());
        assert!(target.join("data/a.bin").is_file());
    }

    #[test]
    fn updates_keep_files_the_game_made() {
        let dir = empty_dir("update");
        let target = dir.join("installed");
        touch(&target.join("saves/slot1.sav"));
        let zip = dir.join("game.zip");
        make_zip(&zip, &[("Game.exe", "v2")]);
        unpack_over(&zip, &dir.join("staging"), &target).unwrap();
        assert!(target.join("saves/slot1.sav").is_file());
        assert_eq!(fs::read_to_string(target.join("Game.exe")).unwrap(), "v2");
    }

    #[test]
    fn installs_a_7z() {
        let dir = empty_dir("sevenz");
        let source = dir.join("source/Game-v1");
        touch(&source.join("Game.exe"));
        touch(&source.join("data/a.bin"));
        let archive = dir.join("Game-v1-Windows.7z");
        sevenz_rust2::compress_to_path(dir.join("source"), &archive).unwrap();
        let target = dir.join("installed");
        unpack_over(&archive, &dir.join("staging"), &target).unwrap();
        assert!(target.join("Game.exe").is_file());
        assert!(target.join("data/a.bin").is_file());
    }

    fn make_tar_gz(path: &Path, files: &[(&str, &str)]) {
        let gz = flate2::write::GzEncoder::new(fs::File::create(path).unwrap(), flate2::Compression::fast());
        let mut tar = tar::Builder::new(gz);
        for (name, body) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(body.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            tar.append_data(&mut header, name, body.as_bytes()).unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap();
    }

    #[test]
    fn installs_a_tar_gz() {
        let dir = empty_dir("targz");
        let archive = dir.join("Game-v1-Linux-X64.tar.gz");
        make_tar_gz(&archive, &[("Game-v1/Game", "elf"), ("Game-v1/data/a.bin", "a")]);
        let target = dir.join("installed");
        unpack_over(&archive, &dir.join("staging"), &target).unwrap();
        assert!(target.join("Game").is_file());
        assert!(target.join("data/a.bin").is_file());
    }

    #[test]
    fn tar_gz_cannot_write_outside_the_folder() {
        let dir = empty_dir("targz-escape");
        let archive = dir.join("evil.tar.gz");
        // `append_data` refuses ".." itself, so write the raw name into the header.
        let gz = flate2::write::GzEncoder::new(fs::File::create(&archive).unwrap(), flate2::Compression::fast());
        let mut tar = tar::Builder::new(gz);
        let mut header = tar::Header::new_gnu();
        header.as_gnu_mut().unwrap().name[..13].copy_from_slice(b"../escape.txt");
        header.set_size(1);
        header.set_cksum();
        tar.append(&header, &b"x"[..]).unwrap();
        tar.into_inner().unwrap().finish().unwrap();
        let _ = unpack_over(&archive, &dir.join("staging"), &dir.join("installed"));
        assert!(!dir.join("escape.txt").exists());
    }

    #[test]
    fn refuses_paths_that_escape_the_folder() {
        let dir = empty_dir("escape");
        let zip = dir.join("evil.zip");
        make_zip(&zip, &[("../../evil.exe", "x")]);
        assert!(unpack_over(&zip, &dir.join("staging"), &dir.join("installed")).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn finds_the_game_not_its_helpers() {
        let dir = empty_dir("program");
        touch(&dir.join("crashpad_handler.exe"));
        touch(&dir.join("unins000.exe"));
        touch(&dir.join("Zelda64Recompiled.exe"));
        touch(&dir.join("tools/n64recomp.exe"));
        let found = find_program(&dir, &["Zelda 64: Recompiled", "Zelda64Recomp"]).unwrap();
        assert_eq!(found.file_name().unwrap(), "Zelda64Recompiled.exe");
    }

    #[cfg(windows)]
    #[test]
    fn takes_the_only_program_even_with_a_short_name() {
        let dir = empty_dir("short");
        touch(&dir.join("soh.exe"));
        touch(&dir.join("lib/crash_reporter.exe"));
        let found = find_program(&dir, &["Ship of Harkinian", "Shipwright"]).unwrap();
        assert_eq!(found.file_name().unwrap(), "soh.exe");
    }
}
