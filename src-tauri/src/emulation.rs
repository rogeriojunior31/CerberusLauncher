//! Emulation for the classics, through RetroArch: the player's own RetroArch,
//! or one Cerberus installs from libretro's own site into its data folder.
//! Each console has a libretro core, fetched one by one from libretro's
//! buildbot. A classic starts as `retroarch -L <core> <rom>`.
//!
//! RetroAchievements: RetroArch logs in with a connect token, not the Web API
//! key. The player types their password once; Cerberus trades it for the token
//! at retroachievements.org and writes the token into RetroArch's settings, so
//! the password is never kept. The settings file is backed up first.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use cerberus_shared::{CoreView, EmulationView};

const STABLE: &str = "https://buildbot.libretro.com/stable";
const NIGHTLY_CORES: &str = "https://buildbot.libretro.com/nightly/windows/x86_64/latest";

/// The core for each console: (console, core file stem, name players know).
pub const CORES: &[(&str, &str, &str)] = &[
    ("nes", "fceumm", "NES"),
    ("snes", "snes9x", "Super Nintendo"),
    ("n64", "mupen64plus_next", "Nintendo 64"),
    ("gb", "gambatte", "Game Boy"),
    ("gbc", "gambatte", "Game Boy Color"),
    ("gba", "mgba", "Game Boy Advance"),
    ("genesis", "genesis_plus_gx", "Mega Drive"),
    ("sms", "genesis_plus_gx", "Master System"),
    ("gg", "genesis_plus_gx", "Game Gear"),
    ("ps1", "pcsx_rearmed", "PlayStation"),
    ("psp", "ppsspp", "PSP"),
    ("gc", "dolphin", "GameCube"),
    ("wii", "dolphin", "Wii"),
];

/// Files some cores need in RetroArch's `system` folder, from libretro's site.
fn system_files(core: &str) -> Option<&'static str> {
    match core {
        "ppsspp" => Some("PPSSPP"),
        "dolphin" => Some("Dolphin"),
        _ => None,
    }
}

const SYSTEM_FILES: &str = "https://buildbot.libretro.com/assets/system";

pub fn core_for(console: &str) -> Option<&'static str> {
    CORES.iter().find(|(c, _, _)| *c == console).map(|(_, core, _)| *core)
}

/// Where Cerberus keeps the RetroArch it installs.
pub fn managed_dir(data: &Path) -> PathBuf {
    data.join("emulators").join("RetroArch")
}

/// The RetroArch to use: the one the player picked, else Cerberus's own, else
/// one in a usual place.
pub fn retroarch(data: &Path, picked: Option<&Path>) -> Option<PathBuf> {
    if let Some(exe) = picked.filter(|p| p.is_file()) {
        return Some(exe.to_path_buf());
    }
    let mut places = vec![managed_dir(data).join("retroarch.exe")];
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    places.push(PathBuf::from(r"C:\RetroArch-Win64\retroarch.exe"));
    places.push(PathBuf::from(r"C:\RetroArch\retroarch.exe"));
    if let Some(appdata) = env("APPDATA") {
        places.push(appdata.join("RetroArch").join("retroarch.exe"));
    }
    if let Some(pf) = env("ProgramFiles") {
        places.push(pf.join("RetroArch").join("retroarch.exe"));
    }
    if let Some(steam) = crate::steam::root() {
        places.push(steam.join("steamapps").join("common").join("RetroArch").join("retroarch.exe"));
    }
    places.into_iter().find(|p| p.is_file())
}

fn core_file(exe: &Path, core: &str) -> PathBuf {
    exe.parent().unwrap_or(Path::new(".")).join("cores").join(format!("{core}_libretro.dll"))
}

fn config_file(exe: &Path) -> PathBuf {
    exe.parent().unwrap_or(Path::new(".")).join("retroarch.cfg")
}

/// A value in RetroArch's settings file (`key = "value"`).
fn config_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (k, v) = line.split_once('=')?;
        (k.trim() == key).then(|| v.trim().trim_matches('"').to_string())
    })
}

/// The settings file with these keys set, every other line kept as it was.
fn with_values(text: &str, values: &[(&str, &str)]) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    for (key, value) in values {
        let line = format!("{key} = \"{value}\"");
        match lines.iter().position(|l| l.split_once('=').is_some_and(|(k, _)| k.trim() == *key)) {
            Some(i) => lines[i] = line,
            None => lines.push(line),
        }
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

pub fn status(data: &Path, picked: Option<&Path>) -> EmulationView {
    let exe = retroarch(data, picked);
    let mut seen = Vec::new();
    let cores = CORES
        .iter()
        .filter(|(_, core, _)| {
            let first = !seen.contains(core);
            seen.push(core);
            first
        })
        .map(|(console, core, _)| {
            let consoles: Vec<String> = CORES.iter().filter(|(_, c, _)| c == core).map(|(_, _, n)| n.to_string()).collect();
            CoreView {
                console: console.to_string(),
                core: core.to_string(),
                consoles: consoles.join(", "),
                installed: exe.as_ref().is_some_and(|e| core_file(e, core).is_file()),
            }
        })
        .collect();
    let ra_user = exe.as_ref().and_then(|e| fs::read_to_string(config_file(e)).ok()).and_then(|t| {
        let enabled = config_value(&t, "cheevos_enable").is_some_and(|v| v == "true");
        let token = config_value(&t, "cheevos_token").filter(|v| !v.is_empty());
        config_value(&t, "cheevos_username").filter(|u| enabled && token.is_some() && !u.is_empty())
    });
    EmulationView {
        managed: exe.as_ref().is_some_and(|e| e.starts_with(managed_dir(data))),
        retroarch: exe.map(|e| e.display().to_string()),
        cores,
        ra_user,
    }
}

/// The newest stable RetroArch version on libretro's site.
async fn latest_version(http: &reqwest::Client) -> Result<String, String> {
    let page = http
        .get(format!("{STABLE}/"))
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| format!("Não foi possível ver a versão do RetroArch: {e}"))?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let mut versions: Vec<(Vec<u32>, String)> = page
        .split("/stable/")
        .skip(1)
        .filter_map(|rest| {
            let v = rest.split('/').next()?;
            let parts: Option<Vec<u32>> = v.split('.').map(|p| p.parse().ok()).collect();
            parts.filter(|p| p.len() == 3).map(|p| (p, v.to_string()))
        })
        .collect();
    versions.sort();
    versions.pop().map(|(_, v)| v).ok_or_else(|| "Não foi possível ver a versão do RetroArch.".to_string())
}

/// Installs RetroArch from libretro's site into Cerberus's data folder.
pub async fn install(http: &reqwest::Client, data: &Path, report: impl Fn(u64, u64) + Send) -> Result<PathBuf, String> {
    let version = latest_version(http).await?;
    let asset = crate::recomp::Asset {
        name: format!("RetroArch-{version}.7z"),
        size: 0,
        url: format!("{STABLE}/{version}/windows/x86_64/RetroArch.7z"),
        sha256: None,
    };
    let downloads = data.join("downloads");
    let cancel = AtomicBool::new(false);
    let archive = crate::recomp::download_file(http, &asset, &downloads, &cancel, report).await?;
    let staging = data.join("emulators").join(".retroarch-new");
    let _ = fs::remove_dir_all(&staging);
    let unpacked = tokio::task::spawn_blocking({
        let (archive, staging) = (archive.clone(), staging.clone());
        move || crate::recomp::extract_7z(&archive, &staging)
    })
    .await
    .map_err(|e| e.to_string())?;
    unpacked?;
    // The archive holds a "RetroArch-Win64" folder.
    let inner = fs::read_dir(&staging)
        .ok()
        .and_then(|mut d| d.find_map(|e| e.ok().map(|e| e.path()).filter(|p| p.join("retroarch.exe").is_file())))
        .unwrap_or_else(|| staging.clone());
    let target = managed_dir(data);
    let _ = fs::remove_dir_all(&target);
    fs::create_dir_all(target.parent().unwrap_or(data)).map_err(|e| e.to_string())?;
    fs::rename(&inner, &target).map_err(|e| format!("Não foi possível instalar o RetroArch: {e}"))?;
    let _ = fs::remove_dir_all(&staging);
    let _ = fs::remove_file(&archive);
    let exe = target.join("retroarch.exe");
    if !exe.is_file() {
        return Err("O pacote do RetroArch veio diferente do esperado.".into());
    }
    Ok(exe)
}

/// Fetches one core into RetroArch's cores folder.
pub async fn install_core(http: &reqwest::Client, exe: &Path, core: &str) -> Result<(), String> {
    if !CORES.iter().any(|(_, c, _)| *c == core) {
        return Err("Núcleo desconhecido.".into());
    }
    let url = format!("{NIGHTLY_CORES}/{core}_libretro.dll.zip");
    let bytes = http
        .get(&url)
        .timeout(Duration::from_secs(120))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| format!("Não foi possível baixar o núcleo: {e}"))?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| "O núcleo baixado veio corrompido.".to_string())?;
    let name = format!("{core}_libretro.dll");
    let mut file = zip.by_name(&name).map_err(|_| "O núcleo baixado veio diferente do esperado.".to_string())?;
    let mut dll = Vec::new();
    std::io::Read::read_to_end(&mut file, &mut dll).map_err(|e| e.to_string())?;
    if !dll.starts_with(b"MZ") {
        return Err("O núcleo baixado veio diferente do esperado.".into());
    }
    let target = core_file(exe, core);
    fs::create_dir_all(target.parent().unwrap_or(Path::new("."))).map_err(|e| e.to_string())?;
    fs::write(&target, dll).map_err(|e| format!("Não foi possível guardar o núcleo: {e}"))?;
    if let Some(pack) = system_files(core) {
        install_system_files(http, exe, pack).await?;
    }
    Ok(())
}

/// Unpacks a core's support files into RetroArch's `system` folder.
async fn install_system_files(http: &reqwest::Client, exe: &Path, pack: &str) -> Result<(), String> {
    let bytes = http
        .get(format!("{SYSTEM_FILES}/{pack}.zip"))
        .timeout(Duration::from_secs(300))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| format!("Não foi possível baixar os arquivos do núcleo: {e}"))?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;
    let system = exe.parent().unwrap_or(Path::new(".")).join("system");
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| "Os arquivos do núcleo vieram corrompidos.".to_string())?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        // Only names that stay inside the system folder.
        let Some(name) = entry.enclosed_name() else { continue };
        let out = system.join(name);
        if entry.is_dir() {
            fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut file = fs::File::create(&out).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut file).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Starts a classic in RetroArch with its console's core. With `log`, RetroArch
/// writes what happens (achievements unlocked among it) to that file.
pub fn launch(exe: &Path, rom: &Path, console: &str, log: Option<&Path>) -> Result<std::process::Child, String> {
    let core = core_for(console).ok_or("Esse console ainda não tem emulação no Cerberus.")?;
    let core_path = core_file(exe, core);
    if !core_path.is_file() {
        return Err("O núcleo desse console não está instalado. Instale em Configurações › Emulação.".into());
    }
    if !rom.is_file() {
        return Err(format!("{} não existe mais.", rom.display()));
    }
    let mut command = Command::new(exe);
    command.current_dir(exe.parent().unwrap_or(Path::new("."))).arg("-L").arg(&core_path);
    if let Some(log) = log {
        let _ = fs::remove_file(log);
        command.arg("--verbose").arg(format!("--log-file={}", log.display()));
    }
    command
        .arg(rom)
        .spawn()
        .map_err(|e| format!("Não foi possível abrir o RetroArch: {e}"))
}

/// Trades the player's password for RetroAchievements' connect token and
/// writes it into RetroArch's settings (backed up first). The password is not kept.
pub async fn link_retroachievements(http: &reqwest::Client, exe: &Path, user: &str, password: &str) -> Result<String, String> {
    let reply = http
        .post("https://retroachievements.org/dorequest.php")
        .header("User-Agent", format!("CerberusLauncher/{}", env!("CARGO_PKG_VERSION")))
        .form(&[("r", "login2"), ("u", user), ("p", password)])
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|_| "Não foi possível falar com o RetroAchievements.".to_string())?;
    let body: serde_json::Value = reply.json().await.map_err(|_| "O RetroAchievements respondeu algo inesperado.".to_string())?;
    if body.get("Success").and_then(serde_json::Value::as_bool) != Some(true) {
        let why = body.get("Error").and_then(serde_json::Value::as_str).unwrap_or("usuário ou senha incorretos");
        return Err(format!("O RetroAchievements recusou: {why}."));
    }
    let token = body.get("Token").and_then(serde_json::Value::as_str).filter(|t| !t.is_empty()).ok_or("O RetroAchievements não mandou o código de acesso.")?;
    let name = body.get("User").and_then(serde_json::Value::as_str).unwrap_or(user).to_string();
    let cfg = config_file(exe);
    let text = fs::read_to_string(&cfg).unwrap_or_default();
    if !text.is_empty() {
        let _ = fs::copy(&cfg, cfg.with_extension("cfg.cerberus-bak"));
    }
    let updated = with_values(&text, &[("cheevos_enable", "true"), ("cheevos_username", &name), ("cheevos_token", token), ("cheevos_password", "")]);
    fs::write(&cfg, updated).map_err(|e| format!("Não foi possível gravar as configurações do RetroArch: {e}"))?;
    Ok(name)
}

/// An achievement RetroArch unlocked, from a line of its log:
/// `[INFO] [RCHEEVOS]: Awarding achievement 1234: Title (description)`.
pub fn unlocked(line: &str) -> Option<String> {
    let at = line.find("chievement ")?;
    let before = line[..at].to_lowercase();
    if !(before.ends_with("awarding a") || before.ends_with("awarded a")) || before.contains("error") {
        return None;
    }
    let rest = &line[at + "chievement ".len()..];
    let (id, text) = rest.split_once(':')?;
    if !id.trim().chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let text = text.trim();
    let title = match text.rfind(" (") {
        Some(i) if text.ends_with(')') => &text[..i],
        _ => text,
    };
    (!title.is_empty()).then(|| title.to_string())
}

/// Time played and when, per classic (by its file), kept in `classics.json`.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Played {
    pub seconds: u64,
    pub last: u64,
}

pub fn played(data: &Path) -> std::collections::HashMap<String, Played> {
    fs::read_to_string(data.join("classics.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn add_played(data: &Path, rom: &str, seconds: u64, now: u64) -> Result<(), String> {
    let mut all = played(data);
    let entry = all.entry(rom.to_string()).or_default();
    entry.seconds += seconds;
    entry.last = now;
    let text = serde_json::to_string_pretty(&all).map_err(|e| e.to_string())?;
    let file = data.join("classics.json");
    let temp = file.with_extension("json.tmp");
    fs::write(&temp, text).map_err(|e| e.to_string())?;
    fs::rename(&temp, &file).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn achievements_are_read_from_the_log() {
        assert_eq!(unlocked("[INFO] [RCHEEVOS]: Awarding achievement 12345: First Blood (Win a race)").as_deref(), Some("First Blood"));
        assert_eq!(unlocked("[RCHEEVOS] Awarded achievement 9: Speedy").as_deref(), Some("Speedy"));
        assert_eq!(unlocked("[INFO] [RCHEEVOS]: Achievement 12 primed"), None);
        assert_eq!(unlocked("[INFO] Loading achievement list"), None);
        // A failed award isn't an unlock.
        assert_eq!(unlocked("[RCHEEVOS]: Error awarding achievement 12: Speedy, retrying in 5 seconds"), None);
    }

    #[test]
    fn play_time_adds_up_per_classic_and_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        add_played(dir.path(), "D:/a.z64", 60, 10).unwrap();
        add_played(dir.path(), "D:/a.z64", 30, 20).unwrap();
        let all = played(dir.path());
        assert_eq!((all["D:/a.z64"].seconds, all["D:/a.z64"].last), (90, 20));
    }

    #[test]
    fn settings_lines_are_set_in_place_and_others_kept() {
        let text = "video_fullscreen = \"false\"\ncheevos_enable = \"false\"\n";
        let out = with_values(text, &[("cheevos_enable", "true"), ("cheevos_username", "me")]);
        assert_eq!(out, "video_fullscreen = \"false\"\ncheevos_enable = \"true\"\ncheevos_username = \"me\"\n");
        assert_eq!(config_value(&out, "cheevos_username").as_deref(), Some("me"));
    }

    #[test]
    fn every_supported_console_has_a_core_and_status_lists_each_core_once() {
        assert_eq!(core_for("gba"), Some("mgba"));
        assert_eq!(core_for("ps2"), None);
        let dir = tempfile::tempdir().unwrap();
        let view = status(dir.path(), None);
        let mut names: Vec<&str> = view.cores.iter().map(|c| c.core.as_str()).collect();
        names.dedup();
        assert_eq!(names.len(), view.cores.len());
    }

    #[test]
    fn the_retroachievements_user_shows_only_with_a_token() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("retroarch.exe");
        fs::write(&exe, b"MZ").unwrap();
        fs::write(config_file(&exe), "cheevos_enable = \"true\"\ncheevos_username = \"me\"\ncheevos_token = \"\"\n").unwrap();
        assert_eq!(status(dir.path(), Some(&exe)).ra_user, None);
        fs::write(config_file(&exe), "cheevos_enable = \"true\"\ncheevos_username = \"me\"\ncheevos_token = \"abc\"\n").unwrap();
        assert_eq!(status(dir.path(), Some(&exe)).ra_user.as_deref(), Some("me"));
    }
}

#[cfg(test)]
#[test]
#[ignore = "starts the RetroArch installed on this PC with a ROM given in CERBERUS_ROM"]
fn launch_real_rom() {
    let data = PathBuf::from(std::env::var("APPDATA").unwrap()).join("app.cerberus.launcher");
    let exe = retroarch(&data, None).expect("RetroArch installed");
    let rom = PathBuf::from(std::env::var("CERBERUS_ROM").unwrap());
    let mut child = launch(&exe, &rom, "n64", None).expect("started");
    std::thread::sleep(Duration::from_secs(12));
    println!("ainda aberto: {}", child.try_wait().unwrap().is_none());
}
