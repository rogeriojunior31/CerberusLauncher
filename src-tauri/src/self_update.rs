//! Cerberus updating itself from its own GitHub releases.
//!
//! The newest release is asked of GitHub 20 seconds after start and every six
//! hours. When it's newer, the app says so; the player chooses when. The
//! installer is downloaded to the data folder, checked against the size and
//! SHA-256 GitHub publishes for it, and only then started silently; Cerberus
//! closes so the installer can replace it, and opens again when it's done.
//!
//! After an update, "What's new" shows that version's notes once.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use cerberus_shared::{AppUpdate, tr};
use futures_util::StreamExt;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

const REPO: &str = "rogeriojunior31/CerberusLauncher";

pub fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// "v0.11.2" or "0.11.2" as numbers; anything after a dash is a pre-release and not offered.
pub fn version(text: &str) -> Option<(u64, u64, u64)> {
    let text = text.trim().trim_start_matches('v');
    if text.contains('-') {
        return None;
    }
    let mut parts = text.split('.').map(|p| p.parse::<u64>().ok());
    Some((parts.next()??, parts.next().flatten().unwrap_or(0), parts.next().flatten().unwrap_or(0)))
}

/// Whether `candidate` (a release, never a pre-release) is newer than `than`,
/// which may be a test build: "0.12.0" is newer than "0.12.0-beta.1".
pub fn is_newer(candidate: &str, than: &str) -> bool {
    let Some(new) = version(candidate) else { return false };
    let than = than.trim();
    let (numbers, test_build) = match than.split_once('-') {
        Some((n, _)) => (n, true),
        None => (than, false),
    };
    match version(numbers) {
        Some(have) => new > have || (new == have && test_build),
        None => false,
    }
}

/// A release as Cerberus needs it.
pub fn from_release(v: &Value) -> Option<AppUpdate> {
    let tag = v.get("tag_name")?.as_str()?;
    let asset = v.get("assets")?.as_array()?.iter().find(|a| a.get("name").and_then(Value::as_str).is_some_and(|n| n.ends_with("_x64-setup.exe")))?;
    let url = asset.get("browser_download_url")?.as_str()?.to_string();
    // Only GitHub's own download host, never an address from somewhere else.
    if !url.starts_with(&format!("https://github.com/{REPO}/releases/download/")) {
        return None;
    }
    Some(AppUpdate {
        version: tag.trim_start_matches('v').to_string(),
        notes: v.get("body").and_then(Value::as_str).unwrap_or_default().to_string(),
        size: asset.get("size").and_then(Value::as_u64).unwrap_or(0),
        sha256: asset.get("digest").and_then(Value::as_str).and_then(|d| d.strip_prefix("sha256:")).map(str::to_lowercase),
        url,
        published: v.get("published_at").and_then(Value::as_str).unwrap_or_default().to_string(),
    })
}

async fn github(http: &reqwest::Client, path: &str) -> Result<Value, String> {
    let request = http.get(format!("https://api.github.com/repos/{REPO}/{path}")).header("Accept", "application/vnd.github+json").timeout(Duration::from_secs(20));
    crate::net::send(request)
        .await
        .ok()
        .and_then(|r| r.error_for_status().ok())
        .ok_or_else(|| tr!("Não foi possível falar com o GitHub.").to_string())?
        .json()
        .await
        .map_err(|_| tr!("O GitHub respondeu algo inesperado.").to_string())
}

/// The newest release when it's newer than this copy.
pub async fn check(http: &reqwest::Client) -> Result<Option<AppUpdate>, String> {
    let release = from_release(&github(http, "releases/latest").await?);
    // Development only: CERBERUS_PRETEND_UPDATE offers the latest release even when
    // it's this version, to see the update window without publishing one.
    #[cfg(debug_assertions)]
    if std::env::var_os("CERBERUS_PRETEND_UPDATE").is_some() {
        return Ok(release);
    }
    Ok(release.filter(|r| is_newer(&r.version, current())))
}

/// This version's notes, for "What's new".
pub async fn notes_of(http: &reqwest::Client, version: &str) -> Result<Option<AppUpdate>, String> {
    Ok(from_release(&github(http, &format!("releases/tags/v{version}")).await?))
}

fn installer_path(data_dir: &Path, update: &AppUpdate) -> PathBuf {
    data_dir.join("downloads").join(format!("Cerberus-Launcher_{}_x64-setup.exe", update.version))
}

/// Downloads the installer to `target`, checking it against the size and
/// SHA-256 GitHub publishes. A file that doesn't match is deleted.
pub async fn fetch_checked(http: &reqwest::Client, target: &Path, update: &AppUpdate, mut progress: impl FnMut(u64, u64)) -> Result<(), String> {
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let reply = http
        .get(&update.url)
        .timeout(Duration::from_secs(600))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|_| tr!("Não foi possível baixar a atualização.").to_string())?;
    let total = reply.content_length().unwrap_or(update.size);
    // Downloaded under another name: only a checked file ever has the installer's name.
    let part = target.with_extension("exe.part");
    let mut file = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    let mut stream = reply.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| tr!("A conexão caiu durante o download da atualização.").to_string())?;
        hasher.update(&chunk);
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        done += chunk.len() as u64;
        progress(done, total);
    }
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    let digest = format!("{:x}", hasher.finalize());
    let size_ok = update.size == 0 || done == update.size;
    // No published checksum, no install: a file that can't be checked is never run.
    let hash_ok = update.sha256.as_deref().is_some_and(|want| want == digest);
    if !size_ok || !hash_ok {
        let _ = std::fs::remove_file(&part);
        return Err(tr!("O arquivo baixado não confere com o publicado no GitHub. Nada foi instalado.").into());
    }
    let _ = std::fs::remove_file(target);
    std::fs::rename(&part, target).map_err(|e| e.to_string())
}

/// Downloads and checks the installer (progress as `update-progress`), starts
/// it silently and closes Cerberus. A wrong file is deleted, never run.
pub async fn install(app: &AppHandle, http: &reqwest::Client, data_dir: &Path, update: AppUpdate) -> Result<(), String> {
    if update.sha256.is_none() {
        return Err(tr!("O GitHub não informou o checksum dessa versão; por segurança ela não é instalada sozinha. Baixe pelo site do projeto.").into());
    }
    let target = installer_path(data_dir, &update);
    fetch_checked(http, &target, &update, |done, total| {
        let _ = app.emit("update-progress", (done, total));
    })
    .await?;
    run_and_close(app, &target)
}

/// Starts `installer` silently once process `pid` (Cerberus) is gone, then
/// `reopen`. PowerShell does the waiting, so nothing depends on a console.
#[cfg(windows)]
pub fn spawn_after_exit(pid: u32, installer: &Path, reopen: &Path) -> Result<(), String> {
    spawn_with("powershell.exe", pid, installer, reopen, std::time::Duration::from_secs(5))
}

/// The same with the shell named, and how long to wait for its sign that it
/// runs: a PowerShell blocked on this PC (AppLocker, a restricted mode) starts
/// but never runs the script, and Cerberus, already closed, would never come back.
#[cfg(windows)]
fn spawn_with(shell: &str, pid: u32, installer: &Path, reopen: &Path, wait: std::time::Duration) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    // Single quotes in PowerShell strings are written twice.
    let quote = |p: &Path| p.display().to_string().replace('\'', "''");
    let started = installer.with_extension("started");
    let _ = std::fs::remove_file(&started);
    let script = format!(
        "Set-Content -LiteralPath '{sign}' -Value ok; Wait-Process -Id {pid} -ErrorAction SilentlyContinue; Start-Process -FilePath '{inst}' -ArgumentList '/S' -Wait; Start-Process -FilePath '{me}'",
        sign = quote(&started),
        pid = pid,
        inst = quote(installer),
        me = quote(reopen)
    );
    const NO_WINDOW: u32 = 0x0800_0000;
    let mut child = std::process::Command::new(shell)
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &script])
        .creation_flags(NO_WINDOW)
        .spawn()
        .map_err(|e| tr!("Não foi possível abrir o instalador: {}", e))?;
    // Cerberus closes only once the script is seen running.
    let asked = std::time::Instant::now();
    while asked.elapsed() < wait {
        if started.is_file() {
            let _ = std::fs::remove_file(&started);
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let _ = child.kill();
    Err(tr!("O PowerShell não rodou neste PC (pode estar bloqueado). O Cerberus continua aberto; instale a versão nova pelo instalador em {}.", installer.display()))
}

#[cfg(windows)]
fn run_and_close(app: &AppHandle, installer: &Path) -> Result<(), String> {
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    spawn_after_exit(std::process::id(), installer, &me)?;
    app.exit(0);
    Ok(())
}

#[cfg(not(windows))]
fn run_and_close(_: &AppHandle, _: &Path) -> Result<(), String> {
    Err(tr!("A atualização automática é só para Windows.").into())
}

/// Installers of this version or older, left in the downloads folder by an update.
pub fn clean_old_installers(data_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(data_dir.join("downloads")) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(v) = name.strip_prefix("Cerberus-Launcher_").and_then(|r| r.strip_suffix("_x64-setup.exe")) else { continue };
        if !is_newer(v, current()) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Asks GitHub 20 seconds after start and then every six hours.
pub fn watch(app: AppHandle) {
    if let Ok(data) = app.path().app_data_dir() {
        clean_old_installers(&data);
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(20)).await;
        loop {
            let http = app.state::<reqwest::Client>().inner().clone();
            if let Ok(Some(update)) = check(&http).await {
                *app.state::<Found>().0.lock().unwrap_or_else(|p| p.into_inner()) = Some(update.clone());
                let _ = app.emit("update-available", update);
            }
            tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
        }
    });
}

/// The update found last, for windows opened after the event.
#[derive(Default)]
pub struct Found(pub std::sync::Mutex<Option<AppUpdate>>);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_as_numbers_and_skip_pre_releases() {
        assert!(is_newer("v0.11.0", "0.10.0"));
        assert!(is_newer("0.10.10", "0.10.9"));
        assert!(!is_newer("0.10.0", "0.10.0"));
        assert!(!is_newer("v0.9.9", "0.10.0"));
        assert!(!is_newer("0.11.0-beta.1", "0.10.0"));
        // A test build installed gets the release it was testing, and later ones.
        assert!(is_newer("0.12.0", "0.12.0-beta.1"));
        assert!(is_newer("0.12.1", "v0.12.0-rc.2"));
        assert!(!is_newer("0.11.9", "0.12.0-beta.1"));
    }

    #[test]
    fn installers_left_from_an_update_are_cleaned() {
        let dir = tempfile::tempdir().unwrap();
        let downloads = dir.path().join("downloads");
        std::fs::create_dir_all(&downloads).unwrap();
        let old = downloads.join(format!("Cerberus-Launcher_{}_x64-setup.exe", current()));
        let newer = downloads.join("Cerberus-Launcher_99.0.0_x64-setup.exe");
        let other = downloads.join("something-else.exe");
        for f in [&old, &newer, &other] {
            std::fs::write(f, b"x").unwrap();
        }
        clean_old_installers(dir.path());
        assert!(!old.exists() && newer.exists() && other.exists());
    }

    #[test]
    fn a_release_needs_the_windows_installer_from_github_itself() {
        let ok = serde_json::json!({
            "tag_name": "v0.11.0", "body": "Notas", "published_at": "2026-12-01T10:00:00Z",
            "assets": [{"name": "Cerberus-Launcher_0.11.0_x64-setup.exe", "size": 10, "digest": "sha256:ABC",
                "browser_download_url": "https://github.com/rogeriojunior31/CerberusLauncher/releases/download/v0.11.0/Cerberus-Launcher_0.11.0_x64-setup.exe"}]
        });
        let u = from_release(&ok).unwrap();
        assert_eq!((u.version.as_str(), u.size, u.sha256.as_deref()), ("0.11.0", 10, Some("abc")));
        let elsewhere = serde_json::json!({
            "tag_name": "v0.11.0",
            "assets": [{"name": "x_x64-setup.exe", "browser_download_url": "https://evil.example/x_x64-setup.exe"}]
        });
        assert!(from_release(&elsewhere).is_none());
    }
}

#[cfg(all(test, windows))]
mod handoff {
    /// The step after the download: waits for the app to end, runs the installer,
    /// then reopens. Here with a stand-in installer and app that only leave a mark.
    #[test]
    fn the_installer_runs_after_exit_and_the_app_reopens() {
        let dir = tempfile::tempdir().unwrap();
        let installed = dir.path().join("installed.txt");
        let reopened = dir.path().join("reopened.txt");
        let installer = dir.path().join("fake setup.cmd");
        let app = dir.path().join("fake app.cmd");
        std::fs::write(&installer, format!("@echo %1> \"{}\"\r\n", installed.display())).unwrap();
        std::fs::write(&app, format!("@echo yes> \"{}\"\r\n", reopened.display())).unwrap();
        // A stand-in for Cerberus that lives 3 seconds.
        let mut running = std::process::Command::new("powershell.exe").args(["-NoProfile", "-Command", "Start-Sleep -Seconds 3"]).spawn().unwrap();
        super::spawn_after_exit(running.id(), &installer, &app).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1500));
        assert!(!installed.exists(), "the installer must wait for the app to close");
        running.wait().unwrap();
        for _ in 0..60 {
            if reopened.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        assert_eq!(std::fs::read_to_string(&installed).unwrap().trim(), "/S");
        assert!(reopened.exists(), "the app opens again after installing");
    }

    /// A shell that starts but doesn't run the script (PowerShell blocked):
    /// refused in time, so Cerberus stays open instead of closing for nothing.
    #[test]
    fn a_shell_that_never_runs_the_script_keeps_cerberus_open() {
        let dir = tempfile::tempdir().unwrap();
        let installer = dir.path().join("setup.exe");
        let app = dir.path().join("app.exe");
        let error = super::spawn_with("where.exe", std::process::id(), &installer, &app, std::time::Duration::from_secs(1)).unwrap_err();
        assert!(error.contains("PowerShell"), "{error}");
    }
}

#[cfg(test)]
mod real {
    /// Asks the real GitHub: `cargo test -p cerberus-launcher real_github -- --ignored --nocapture`.
    #[test]
    #[ignore = "asks GitHub"]
    fn real_github_release_reads() {
        let http = reqwest::Client::builder().user_agent("CerberusLauncher").build().unwrap();
        let newer = tauri::async_runtime::block_on(super::check(&http)).unwrap();
        let notes = tauri::async_runtime::block_on(super::notes_of(&http, "0.10.0")).unwrap().unwrap();
        println!("newer than {}: {:?}; 0.10.0 notes {} chars, {} bytes, sha256 {:?}", super::current(), newer.map(|u| u.version), notes.notes.len(), notes.size, notes.sha256);
        assert!(notes.sha256.is_some());
        // The real installer comes down and matches; a wrong fingerprint is refused and deleted.
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("setup.exe");
        tauri::async_runtime::block_on(super::fetch_checked(&http, &target, &notes, |_, _| {})).unwrap();
        assert_eq!(std::fs::metadata(&target).unwrap().len(), notes.size);
        let mut wrong = notes.clone();
        wrong.sha256 = Some("00".repeat(32));
        let good = std::fs::read(&target).unwrap();
        assert!(tauri::async_runtime::block_on(super::fetch_checked(&http, &target, &wrong, |_, _| {})).is_err());
        assert!(!target.with_extension("exe.part").exists(), "the bad download is deleted");
        assert_eq!(std::fs::read(&target).unwrap(), good, "the checked installer isn't touched");
        // No published checksum: refused.
        let mut unknown = notes.clone();
        unknown.sha256 = None;
        let other = dir.path().join("other.exe");
        assert!(tauri::async_runtime::block_on(super::fetch_checked(&http, &other, &unknown, |_, _| {})).is_err());
        assert!(!other.exists());
    }
}
