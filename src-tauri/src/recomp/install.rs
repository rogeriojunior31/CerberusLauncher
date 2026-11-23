//! Download, checksum, unpack, and find the game's program.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::{Client, StatusCode, header};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use super::CatalogEntry;
use super::assets::{Asset, Kind, kind, words};

pub const CANCELLED: &str = "Instalação cancelada.";
pub use cerberus_shared::PAUSED;

/// Stops a download: cancelled (its bytes are thrown away) or paused (they are
/// kept, and the next download of the same file carries on from them).
#[derive(Default)]
pub struct Halt {
    stop: AtomicBool,
    keep: AtomicBool,
    part: Mutex<Option<PathBuf>>,
}

impl Halt {
    pub fn cancel(&self) {
        self.keep.store(false, Ordering::Relaxed);
        self.stop.store(true, Ordering::Relaxed);
    }

    pub fn pause(&self) {
        self.keep.store(true, Ordering::Relaxed);
        self.stop.store(true, Ordering::Relaxed);
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// Throws away what a paused download kept.
    pub fn discard(&self) {
        if let Some(part) = self.part.lock().unwrap_or_else(|p| p.into_inner()).take() {
            let _ = fs::remove_file(part);
        }
    }
}
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
    cancel: &Halt,
    report: impl Fn(u64, u64) + Send,
) -> Result<PathBuf, String> {
    tokio::fs::create_dir_all(dir).await.map_err(|e| e.to_string())?;
    let safe_name: String = asset.name.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '_' }).collect();
    let target = dir.join(&safe_name);
    let tag = &format!("{:x}", Sha256::digest(format!("{}|{}", asset.size, asset.url)))[..10];
    let part = dir.join(format!("{safe_name}.{tag}.part"));
    *cancel.part.lock().unwrap_or_else(|p| p.into_inner()) = Some(part.clone());

    // Re-reading a large paused file takes seconds: off the async workers.
    let (resume_part, size) = (part.clone(), asset.size);
    let (mut hash, mut done) = tauri::async_runtime::spawn_blocking(move || resume_state(&resume_part, size)).await.map_err(|e| e.to_string())?;
    // 0 until the server says, when the catalog didn't know the exact size.
    let mut total = asset.size;
    let mut tries = 0;
    loop {
        tries += 1;
        match fetch_into(client, asset, &part, &mut hash, &mut done, &mut total, cancel, &report).await {
            Ok(()) => break,
            Err(Stop::Cancelled) if cancel.keep.load(Ordering::Relaxed) => return Err(PAUSED.into()),
            Err(Stop::Cancelled) => {
                let _ = fs::remove_file(&part);
                return Err(CANCELLED.into());
            }
            Err(Stop::Fatal(message)) => return Err(message),
            Err(Stop::Retry(_)) if tries < TRIES => {
                let _ = until_cancelled(tokio::time::sleep(Duration::from_secs(2 * tries as u64)), cancel).await;
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

// Cancel a stalled connection or retry delay even if the server sends no further bytes.
async fn until_cancelled<F: std::future::Future>(task: F, cancel: &Halt) -> Result<F::Output, Stop> {
    use futures_util::future::{Either, select};
    if cancel.stopped() { return Err(Stop::Cancelled); }
    let cancelled = async {
        while !cancel.stopped() { tokio::time::sleep(Duration::from_millis(150)).await; }
    };
    futures_util::pin_mut!(task, cancelled);
    match select(task, cancelled).await {
        Either::Left((value, _)) => Ok(value),
        Either::Right(_) => Err(Stop::Cancelled),
    }
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
    cancel: &Halt,
    report: &(impl Fn(u64, u64) + Send),
) -> Result<(), Stop> {
    if cancel.stopped() { return Err(Stop::Cancelled); }
    if *done == *total && *total > 0 {
        return Ok(());
    }
    let request = || client.get(&asset.url).timeout(Duration::from_secs(6 * 60 * 60));
    let dropped = || Stop::Retry("A conexão caiu durante o download. Tente de novo; ele continua de onde parou.".into());
    let mut get = request();
    if *done > 0 {
        get = get.header(header::RANGE, format!("bytes={done}-"));
    }
    let mut reply = until_cancelled(get.send(), cancel).await?.map_err(|_| dropped())?;
    let range = reply.headers().get(header::CONTENT_RANGE).and_then(|h| h.to_str().ok());
    let partial = range.and_then(|r| partial_range(r, *done, asset.size));
    if reply.status() == StatusCode::RANGE_NOT_SATISFIABLE && *done > 0 {
        let length = range.and_then(|r| r.strip_prefix("bytes */")).and_then(|n| n.parse::<u64>().ok());
        if length == Some(*done) && (asset.size == 0 || asset.size == *done) {
            *total = *done;
            return Ok(());
        }
    }
    let restart = reply.status() == StatusCode::RANGE_NOT_SATISFIABLE
        || (reply.status() == StatusCode::PARTIAL_CONTENT && partial.is_none());
    if restart {
        // Keep the existing part until the server actually supplies a full replacement.
        reply = until_cancelled(request().send(), cancel).await?.map_err(|_| dropped())?;
    }
    let status = reply.status();
    if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
        return Err(Stop::Retry(format!("O GitHub está ocupado (resposta {}). Tente de novo em um minuto.", status.as_u16())));
    }
    let continuing = !restart && status == StatusCode::PARTIAL_CONTENT;
    let response_end = if continuing {
        let (end, size) = partial.ok_or_else(|| Stop::Fatal("O servidor enviou um intervalo inválido.".into()))?;
        *total = size;
        end
    } else if status == StatusCode::OK {
        let length = reply.content_length();
        if asset.size > 0 && length.is_some_and(|n| n != asset.size) {
            return Err(Stop::Fatal("O tamanho da release mudou. Atualize o catálogo e tente de novo.".into()));
        }
        *total = length.unwrap_or(asset.size);
        *total
    } else {
        return Err(Stop::Fatal(format!("O GitHub respondeu {} ao baixar {}.", status.as_u16(), asset.name)));
    };
    crate::disk::require(part.parent().unwrap_or(part), total.saturating_sub(*done)).map_err(Stop::Fatal)?;
    if !continuing {
        *hash = Sha256::new();
        *done = 0;
    }
    let total = *total;
    let mut options = tokio::fs::OpenOptions::new();
    options.create(true);
    if continuing { options.append(true) } else { options.write(true).truncate(true) };
    let mut out = options.open(part).await.map_err(|e| Stop::Fatal(e.to_string()))?;

    let mut stream = reply.bytes_stream();
    while let Some(chunk) = until_cancelled(stream.next(), cancel).await? {
        let chunk = chunk.map_err(|_| dropped())?;
        if response_end > 0 && chunk.len() as u64 > response_end.saturating_sub(*done) {
            return Err(Stop::Fatal("A resposta excede o tamanho anunciado pela release.".into()));
        }
        out.write_all(&chunk).await.map_err(|e| Stop::Fatal(e.to_string()))?;
        hash.update(&chunk);
        *done += chunk.len() as u64;
        report(*done, total);
    }
    out.flush().await.map_err(|e| Stop::Fatal(e.to_string()))?;
    if total > 0 && *done != total { return Err(dropped()); }
    Ok(())
}

/// End (exclusive) and size of the file. Append only if the bytes continue our exact offset.
fn partial_range(value: &str, offset: u64, expected: u64) -> Option<(u64, u64)> {
    let (span, size) = value.strip_prefix("bytes ")?.split_once('/')?;
    let (first, last) = span.split_once('-')?;
    let start = first.parse::<u64>().ok()?;
    let end = last.parse::<u64>().ok()?.checked_add(1)?;
    let size = size.parse::<u64>().ok()?;
    (start == offset && start < end && end <= size && (expected == 0 || size == expected)).then_some((end, size))
}

/// Unpacks into `staging`, then moves the files over `target`. Files already in
/// `target` that the new release doesn't have (saves, settings) are kept.
pub fn unpack_over(archive: &Path, staging: &Path, target: &Path, entry: &CatalogEntry) -> Result<PathBuf, String> {
    if let Err(e) = fs::remove_dir_all(staging) {
        if e.kind() != io::ErrorKind::NotFound {
            return Err(format!("Não foi possível limpar a pasta temporária: {e}"));
        }
    }
    fs::create_dir_all(staging).map_err(|e| e.to_string())?;
    let result = install_staged(archive, staging, target, entry);
    let _ = fs::remove_dir_all(staging);
    result
}

fn install_staged(archive: &Path, staging: &Path, target: &Path, entry: &CatalogEntry) -> Result<PathBuf, String> {
    let name = archive.file_name().unwrap_or_default().to_string_lossy();
    match kind(&name) {
        Some(Kind::Zip) => unzip(archive, staging)?,
        Some(Kind::SevenZip) => un7z(archive, staging)?,
        Some(Kind::TarGz) => untar_gz(archive, staging)?,
        // A release that is a program (the game, or its installer) goes in as it is.
        None if super::assets::is_program(&name) => {
            crate::disk::require(staging, fs::metadata(archive).map_err(|e| e.to_string())?.len())?;
            fs::copy(archive, staging.join(name.as_ref())).map_err(|e| e.to_string())?;
        }
        None => return Err(format!("{name} não é um formato que o Cerberus saiba instalar.")),
    }
    let root = single_top_folder(staging);
    let program = if entry.kind == "installer" {
        installer_in(&root).ok_or("O instalador não foi encontrado na release.")?
    } else {
        let named = entry.start.as_deref().map(Path::new)
            .filter(|path| path.components().all(|c| matches!(c, std::path::Component::Normal(_) | std::path::Component::CurDir)))
            .map(|path| root.join(path)).filter(|path| path.is_file());
        named.or_else(|| find_program(&root, &[&entry.name, &entry.repo]))
            .ok_or("A release não contém o programa do jogo.")?
    };
    let resolved = program.canonicalize().map_err(|e| e.to_string())?;
    if !resolved.starts_with(staging.canonicalize().map_err(|e| e.to_string())?) {
        return Err("O programa escolhido está fora da release.".into());
    }
    let relative = program.strip_prefix(&root).map_err(|e| e.to_string())?;
    overlay(&root, target).map_err(|e| format!("Não foi possível instalar os arquivos: {e}"))?;
    Ok(target.join(relative))
}

/// The installer a release brought: the program at the top of its folder.
fn installer_in(dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    let mut programs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_file() && super::assets::is_program(&p.to_string_lossy())).collect();
    programs.sort();
    programs.into_iter().next()
}

pub(super) fn unzip(file: &Path, to: &Path) -> Result<(), String> {
    let broken = |e: zip::result::ZipError| format!("O arquivo baixado está corrompido: {e}");
    let reader = fs::File::open(file).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(reader).map_err(broken)?;
    if let Some(size) = archive.decompressed_size() {
        crate::disk::require(to, u64::try_from(size).unwrap_or(u64::MAX))?;
    }
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
pub(super) fn un7z(file: &Path, to: &Path) -> Result<(), String> {
    let archive = sevenz_rust2::Archive::open(file).map_err(|e| e.to_string())?;
    let size = archive.files.iter().fold(0u64, |sum, item| sum.saturating_add(item.size()));
    crate::disk::require(to, size)?;
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
pub(super) mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn resumptions_validate_the_server_range_before_publishing() {
        use std::{io::{BufRead, BufReader}, net::TcpListener};
        for (first, fallback) in [
            ("206 Partial Content\r\nContent-Range: bytes 3-6/7\r\nContent-Length: 4\r\n\r\nDEFG", false),
            ("416 Range Not Satisfiable\r\nContent-Range: bytes */2\r\nContent-Length: 0\r\n\r\n", true),
            ("206 Partial Content\r\nContent-Range: bytes 1-4/7\r\nContent-Length: 4\r\n\r\nBCDE", true),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/game.zip", listener.local_addr().unwrap());
            let server = std::thread::spawn(move || {
                let mut replies = vec![first];
                if fallback { replies.push("200 OK\r\nContent-Length: 7\r\n\r\nABCDEFG"); }
                for (i, response) in replies.into_iter().enumerate() {
                    let (mut socket, _) = listener.accept().unwrap();
                    let head: Vec<_> = BufReader::new(&socket).lines().map(Result::unwrap)
                        .take_while(|line| !line.is_empty()).collect();
                    assert_eq!(head.iter().any(|s| s.eq_ignore_ascii_case("range: bytes=3-")), i == 0);
                    write!(socket, "HTTP/1.1 {response}").unwrap();
                }
            });
            let dir = tempfile::tempdir().unwrap();
            let key = format!("{:x}", Sha256::digest(format!("0|{url}")));
            fs::write(dir.path().join(format!("game.zip.{}.part", &key[..10])), b"ABC").unwrap();
            let asset = Asset { name: "game.zip".into(), url, size: 0, sha256: None };
            let client = Client::builder().no_proxy().build().unwrap();
            let path = tauri::async_runtime::block_on(download(&client, &asset, dir.path(), &Halt::default(), |_, _| {})).unwrap();
            assert_eq!(fs::read(path).unwrap(), b"ABCDEFG");
            server.join().unwrap();
        }
    }

    #[test]
    fn cancel_works_even_when_the_server_has_not_sent_headers() {
        use std::{net::TcpListener, sync::{Arc, mpsc}};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/stalled.zip", listener.local_addr().unwrap());
        let cancel = Arc::new(Halt::default());
        let flag = cancel.clone();
        let (finished, wait) = mpsc::channel::<()>();
        let server = std::thread::spawn(move || {
            let (_socket, _) = listener.accept().unwrap();
            flag.cancel();
            assert!(wait.recv_timeout(Duration::from_secs(5)).is_ok(), "download waited for server headers");
        });
        let dir = tempfile::tempdir().unwrap();
        let hash = format!("{:x}", Sha256::digest(format!("0|{url}")));
        let part = dir.path().join(format!("stalled.zip.{}.part", &hash[..10]));
        fs::write(&part, b"ABC").unwrap();
        let asset = Asset { name: "stalled.zip".into(), url, size: 0, sha256: None };
        let client = Client::builder().no_proxy().build().unwrap();
        let result = tauri::async_runtime::block_on(download(&client, &asset, dir.path(), &cancel, |_, _| {}));
        finished.send(()).unwrap();
        server.join().unwrap();
        assert_eq!(result.unwrap_err(), CANCELLED);
        assert!(!part.exists());
    }

    #[test]
    fn a_paused_download_keeps_its_bytes_for_next_time() {
        use std::{net::TcpListener, sync::{Arc, mpsc}};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/paused.zip", listener.local_addr().unwrap());
        let halt = Arc::new(Halt::default());
        let flag = halt.clone();
        let (finished, wait) = mpsc::channel::<()>();
        let server = std::thread::spawn(move || {
            let (_socket, _) = listener.accept().unwrap();
            flag.pause();
            let _ = wait.recv_timeout(Duration::from_secs(5));
        });
        let dir = tempfile::tempdir().unwrap();
        let hash = format!("{:x}", Sha256::digest(format!("0|{url}")));
        let part = dir.path().join(format!("paused.zip.{}.part", &hash[..10]));
        fs::write(&part, b"ABC").unwrap();
        let asset = Asset { name: "paused.zip".into(), url, size: 0, sha256: None };
        let client = Client::builder().no_proxy().build().unwrap();
        let result = tauri::async_runtime::block_on(download(&client, &asset, dir.path(), &halt, |_, _| {}));
        finished.send(()).unwrap();
        server.join().unwrap();
        assert_eq!(result.unwrap_err(), PAUSED);
        assert_eq!(fs::read(&part).unwrap(), b"ABC");
        // Giving up on it afterwards throws the bytes away.
        halt.discard();
        assert!(!part.exists());
    }

    #[test]
    fn insufficient_space_keeps_the_existing_part() {
        use std::{io::{BufRead, BufReader}, net::TcpListener};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/large.zip", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            for line in BufReader::new(&socket).lines() { if line.unwrap().is_empty() { break; } }
            write!(socket, "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 3-3/{}\r\nContent-Length: 1\r\n\r\nD", u64::MAX).unwrap();
        });
        let dir = tempfile::tempdir().unwrap();
        let hash = format!("{:x}", Sha256::digest(format!("0|{url}")));
        let part = dir.path().join(format!("large.zip.{}.part", &hash[..10]));
        fs::write(&part, b"ABC").unwrap();
        let asset = Asset { name: "large.zip".into(), url, size: 0, sha256: None };
        let client = Client::builder().no_proxy().build().unwrap();
        let err = tauri::async_runtime::block_on(download(&client, &asset, dir.path(), &Halt::default(), |_, _| {})).unwrap_err();
        server.join().unwrap();
        assert!(err.contains("Espaço insuficiente"));
        assert_eq!(fs::read(part).unwrap(), b"ABC");
        assert!(!dir.path().join("large.zip").exists());
    }

    #[test]
    fn resumed_archive_installs_on_the_selected_volume() {
        use std::{io::{BufRead, BufReader}, net::TcpListener};
        let cache = tempfile::tempdir().unwrap();
        let install = match std::env::var_os("CERBERUS_TEST_INSTALL_VOLUME") {
            Some(root) => tempfile::tempdir_in(root).unwrap(),
            None => tempfile::tempdir().unwrap(),
        };
        let release = cache.path().join("origin.zip");
        make_zip(&release, &[("Game.exe", "playable"), ("data.bin", "data")]);
        let bytes = fs::read(release).unwrap();
        let digest = format!("{:x}", Sha256::digest(&bytes));
        let size = bytes.len();
        let offset = size / 2;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/sample.zip", listener.local_addr().unwrap());
        let asset = Asset { name: "sample.zip".into(), url: url.clone(), size: size as u64, sha256: Some(digest) };
        let hash = format!("{:x}", Sha256::digest(format!("{size}|{url}")));
        fs::write(cache.path().join(format!("sample.zip.{}.part", &hash[..10])), &bytes[..offset]).unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let head: Vec<_> = BufReader::new(&socket).lines().map(Result::unwrap).take_while(|s| !s.is_empty()).collect();
            assert!(head.iter().any(|s| s.eq_ignore_ascii_case(&format!("range: bytes={offset}-"))));
            write!(socket, "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {offset}-{}/{size}\r\nContent-Length: {}\r\n\r\n", size - 1, size - offset).unwrap();
            socket.write_all(&bytes[offset..]).unwrap();
        });
        let client = Client::builder().no_proxy().build().unwrap();
        let archive = tauri::async_runtime::block_on(download(&client, &asset, cache.path(), &Halt::default(), |_, _| {})).unwrap();
        server.join().unwrap();
        let target = install.path().join("game");
        let exe = unpack_over(&archive, &install.path().join("stage"), &target, &portable("Game.exe")).unwrap();
        assert_eq!(fs::read(exe).unwrap(), b"playable");
        assert_eq!(fs::read(target.join("data.bin")).unwrap(), b"data");
    }

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

    pub(in crate::recomp) fn make_zip(path: &Path, files: &[(&str, &str)]) {
        let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        for (name, body) in files {
            zip.start_file(*name, options).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    fn portable(start: &str) -> CatalogEntry {
        CatalogEntry { start: Some(start.into()), ..Default::default() }
    }

    #[test]
    fn missing_program_in_update_leaves_the_installed_release_untouched() {
        let dir = empty_dir("preflight-update");
        let staging = dir.join("staging");
        let target = dir.join("installed");
        let zip = dir.join("game.zip");
        let entry = portable("Game.exe");
        make_zip(&zip, &[("Game.exe", "v1"), ("data.bin", "old")]);
        assert_eq!(unpack_over(&zip, &staging, &target, &entry).unwrap(), target.join("Game.exe"));
        touch(&target.join("slot.sav"));
        make_zip(&zip, &[("data.bin", "different"), ("new.txt", "extra")]);
        assert!(unpack_over(&zip, &staging, &target, &entry).is_err());
        assert_eq!(fs::read(target.join("Game.exe")).unwrap(), b"v1");
        assert_eq!(fs::read(target.join("data.bin")).unwrap(), b"old");
        assert_eq!(fs::read(target.join("slot.sav")).unwrap(), b"x");
        assert!(!target.join("new.txt").exists());
        assert!(!staging.exists());
    }

    #[test]
    fn installer_is_selected_from_the_new_release() {
        let dir = empty_dir("preflight-installer");
        let staging = dir.join("staging");
        let target = dir.join("installed");
        let zip = dir.join("game.zip");
        touch(&target.join("A-old-setup.exe"));
        let entry = CatalogEntry { kind: "installer".into(), ..Default::default() };
        make_zip(&zip, &[("v2/NewSetup.msi", "installer")]);
        assert_eq!(unpack_over(&zip, &staging, &target, &entry).unwrap(), target.join("NewSetup.msi"));
        make_zip(&zip, &[("readme.txt", "missing setup")]);
        assert!(unpack_over(&zip, &staging, &target, &entry).is_err());
        assert!(!target.join("readme.txt").exists());
    }

    #[test]
    fn unwraps_a_single_top_folder() {
        let dir = empty_dir("unwrap");
        let zip = dir.join("game.zip");
        make_zip(&zip, &[("Game-v1/Game.exe", "new"), ("Game-v1/data/a.bin", "a")]);
        let target = dir.join("installed");
        unpack_over(&zip, &dir.join("staging"), &target, &portable("Game.exe")).unwrap();
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
        unpack_over(&zip, &dir.join("staging"), &target, &portable("Game.exe")).unwrap();
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
        unpack_over(&archive, &dir.join("staging"), &target, &portable("Game.exe")).unwrap();
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
        unpack_over(&archive, &dir.join("staging"), &target, &portable("Game")).unwrap();
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
        let _ = unpack_over(&archive, &dir.join("staging"), &dir.join("installed"), &portable("Game.exe"));
        assert!(!dir.join("escape.txt").exists());
    }

    #[test]
    fn refuses_paths_that_escape_the_folder() {
        let dir = empty_dir("escape");
        let zip = dir.join("evil.zip");
        make_zip(&zip, &[("../../evil.exe", "x")]);
        assert!(unpack_over(&zip, &dir.join("staging"), &dir.join("installed"), &portable("Game.exe")).is_err());
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
