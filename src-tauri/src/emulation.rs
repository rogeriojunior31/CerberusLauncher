//! Emulation for the classics, through RetroArch: the player's own RetroArch,
//! or one Cerberus installs from libretro's own site into its data folder.
//! Each console has a libretro core, fetched one by one from libretro's
//! buildbot. A classic starts as `retroarch -L <core> <rom>`.
//!
//! RetroAchievements: RetroArch logs in with a connect token, not the Web API
//! key. The player types their password once; Cerberus trades it for the token
//! at retroachievements.org and writes the token into RetroArch's settings, so
//! the password is never kept. The settings file is backed up first.

use cerberus_shared::tr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
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
    ("ps2", "pcsx2", "PlayStation 2"),
];

/// A PlayStation BIOS is 512 KB; a PlayStation 2 BIOS, 4 MB.
const PS1_BIOS: u64 = 512 * 1024;
const PS2_BIOS: u64 = 4 * 1024 * 1024;

/// Where RetroArch reads BIOS files: its own `system` folder, or the one the
/// player set in its settings (`:` meaning beside the program).
fn system_dir(exe: &Path) -> PathBuf {
    let base = exe.parent().unwrap_or(Path::new("."));
    let chosen = fs::read_to_string(config_file(exe))
        .ok()
        .and_then(|text| config_value(&text, "system_directory"))
        .filter(|v| !v.is_empty() && v != "default");
    match chosen {
        Some(v) if v.starts_with(':') => base.join(v.trim_start_matches(':').trim_start_matches(['\\', '/'])),
        Some(v) => PathBuf::from(v),
        None => base.join("system"),
    }
}

fn ps2_bios_dir(exe: &Path) -> PathBuf {
    system_dir(exe).join("pcsx2").join("bios")
}

fn has_file(dir: &Path, wanted: impl Fn(&Path, u64) -> bool) -> bool {
    fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|e| e.metadata().is_ok_and(|m| m.is_file() && wanted(&e.path(), m.len())))
    })
}

fn is_bin(path: &Path) -> bool {
    path.extension().is_some_and(|x| x.eq_ignore_ascii_case("bin"))
}

/// Which BIOS files RetroArch has: (PlayStation, PlayStation 2).
pub fn bios_status(exe: &Path) -> (bool, bool) {
    let ps1 = has_file(&system_dir(exe), |p, size| is_bin(p) && size == PS1_BIOS);
    let ps2 = has_file(&ps2_bios_dir(exe), |p, size| is_bin(p) && size == PS2_BIOS);
    (ps1, ps2)
}

/// Copies the player's BIOS files (from their own consoles) where the cores
/// look for them, telling PlayStation from PlayStation 2 by size. Returns how
/// many of each were copied.
pub fn import_bios(exe: &Path, from: &Path) -> Result<(usize, usize), String> {
    let entries = fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))?;
    let (mut ps1, mut ps2) = (0, 0);
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let name = entry.file_name();
        let ext = path.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
        let lower = name.to_string_lossy().to_lowercase();
        let target = if ext == "bin" && meta.len() == PS1_BIOS {
            ps1 += 1;
            system_dir(exe)
        } else if (ext == "bin" && meta.len() == PS2_BIOS) || matches!(ext.as_str(), "mec" | "nvm") || lower == "erom.bin" || lower.starts_with("rom1") || lower.starts_with("rom2") {
            if ext == "bin" && meta.len() == PS2_BIOS {
                ps2 += 1;
            }
            ps2_bios_dir(exe)
        } else {
            continue;
        };
        fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        fs::copy(&path, target.join(&name)).map_err(|e| tr!("Não foi possível copiar {0}: {1}", name.to_string_lossy(), e))?;
    }
    if ps1 == 0 && ps2 == 0 {
        return Err(tr!("Nenhuma BIOS de PlayStation (512 KB) ou PlayStation 2 (4 MB) nessa pasta.").into());
    }
    Ok((ps1, ps2))
}

/// Files some cores need in RetroArch's `system` folder, from libretro's site.
fn system_files(core: &str) -> Option<&'static str> {
    match core {
        "ppsspp" => Some("PPSSPP"),
        "dolphin" => Some("Dolphin"),
        _ => None,
    }
}

const SYSTEM_FILES: &str = "https://buildbot.libretro.com/assets/system";

/// This RetroArch has the core for the console.
pub fn has_core(exe: &Path, console: &str) -> bool {
    core_for(console).is_some_and(|core| core_file(exe, core).is_file())
}

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
    let (ps1_bios, ps2_bios) = exe.as_deref().map(bios_status).unwrap_or_default();
    EmulationView {
        ps1_bios,
        ps2_bios,
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
        .map_err(|e| tr!("Não foi possível ver a versão do RetroArch: {}", e))?
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
    versions.pop().map(|(_, v)| v).ok_or_else(|| tr!("Não foi possível ver a versão do RetroArch.").to_string())
}

/// Installs RetroArch from libretro's site into Cerberus's data folder.
pub async fn install(http: &reqwest::Client, data: &Path, report: impl Fn(u64, u64) + Send) -> Result<PathBuf, String> {
    let version = latest_version(http).await?;
    let asset = crate::recomp::Asset {
        name: format!("RetroArch-{}.7z", version),
        size: 0,
        url: format!("{STABLE}/{version}/windows/x86_64/RetroArch.7z"),
        sha256: None,
    };
    let downloads = data.join("downloads");
    let cancel = crate::recomp::Halt::default();
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
    fs::create_dir_all(target.parent().unwrap_or(data)).map_err(|e| e.to_string())?;
    swap_keeping_player_files(&target, &inner)?;
    let _ = fs::remove_dir_all(&staging);
    let _ = fs::remove_file(&archive);
    let exe = target.join("retroarch.exe");
    if !exe.is_file() {
        return Err(tr!("O pacote do RetroArch veio diferente do esperado.").into());
    }
    Ok(exe)
}

/// Fetches one core into RetroArch's cores folder.
pub async fn install_core(http: &reqwest::Client, exe: &Path, core: &str) -> Result<(), String> {
    if !CORES.iter().any(|(_, c, _)| *c == core) {
        return Err(tr!("Núcleo desconhecido.").into());
    }
    let url = format!("{NIGHTLY_CORES}/{core}_libretro.dll.zip");
    let bytes = http
        .get(&url)
        .timeout(Duration::from_secs(120))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| tr!("Não foi possível baixar o núcleo: {}", e))?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| tr!("O núcleo baixado veio corrompido.").to_string())?;
    let name = format!("{core}_libretro.dll");
    let mut file = zip.by_name(&name).map_err(|_| tr!("O núcleo baixado veio diferente do esperado.").to_string())?;
    let mut dll = Vec::new();
    std::io::Read::read_to_end(&mut file, &mut dll).map_err(|e| e.to_string())?;
    if !dll.starts_with(b"MZ") {
        return Err(tr!("O núcleo baixado veio diferente do esperado.").into());
    }
    let target = core_file(exe, core);
    fs::create_dir_all(target.parent().unwrap_or(Path::new("."))).map_err(|e| e.to_string())?;
    fs::write(&target, dll).map_err(|e| tr!("Não foi possível guardar o núcleo: {}", e))?;
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
        .map_err(|e| tr!("Não foi possível baixar os arquivos do núcleo: {}", e))?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;
    let system = exe.parent().unwrap_or(Path::new(".")).join("system");
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| tr!("Os arquivos do núcleo vieram corrompidos.").to_string())?;
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

/// Consoles whose RetroArch core reads an .m3u list of discs and swaps them
/// from its menu (Disc Control).
const DISC_LISTS: &[&str] = &["ps1", "gc"];

/// What RetroArch opens for a game on several discs: an .m3u of them, written
/// in Cerberus's own folder (never beside the player's files); the first disc
/// for a console whose core can't swap discs; the file itself otherwise.
pub fn disc_list(data: &Path, rom: &Path, discs: &[PathBuf], console: &str) -> Result<PathBuf, String> {
    let own_list = rom.extension().is_some_and(|e| e.eq_ignore_ascii_case("m3u"));
    if discs.len() < 2 || own_list {
        return Ok(rom.to_path_buf());
    }
    if !DISC_LISTS.contains(&console) {
        return Ok(discs[0].clone());
    }
    let dir = data.join("emulators").join("playlists");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // Named after the game (not a number): emulators name saves and memory
    // cards after the list, so "Final Fantasy VII (USA).srm" is found again.
    let stem = discs[0].file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let name: String = crate::roms::without_disc_tag(&stem).chars().filter(|c| !r#"<>:"/\|?*"#.contains(*c)).collect();
    let name = if name.trim().is_empty() { format!("{:08x}", crc32fast::hash(rom.to_string_lossy().as_bytes())) } else { name };
    let list = dir.join(format!("{name}.m3u"));
    let text: String = discs.iter().map(|d| format!("{}\n", d.display())).collect();
    fs::write(&list, text).map_err(|e| tr!("Não foi possível preparar a lista de discos: {}", e))?;
    Ok(list)
}

/// Starts a classic in RetroArch with its console's core. With `log`, RetroArch
/// writes what happens (achievements unlocked among it) to that file.
pub fn launch(exe: &Path, rom: &Path, console: &str, log: Option<&Path>) -> Result<std::process::Child, String> {
    let core = core_for(console).ok_or(tr!("Esse console ainda não tem emulação no Cerberus."))?;
    if console == "ps2" && !bios_status(exe).1 {
        return Err(tr!("O PlayStation 2 precisa da BIOS do seu console. Adicione em Configurações › Emulação › BIOS.").into());
    }
    let core_path = core_file(exe, core);
    if !core_path.is_file() {
        return Err(tr!("O núcleo desse console não está instalado. Instale em Configurações › Emulação.").into());
    }
    if !rom.is_file() {
        return Err(tr!("{} não existe mais.", rom.display()));
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
        .map_err(|e| tr!("Não foi possível abrir o RetroArch: {}", e))
}

/// Trades the player's password for RetroAchievements' connect token: the
/// account's exact name and the token. The password is not kept.
pub async fn connect_token(http: &reqwest::Client, user: &str, password: &str) -> Result<(String, String), String> {
    let reply = http
        .post("https://retroachievements.org/dorequest.php")
        .header("User-Agent", format!("CerberusLauncher/{}", env!("CARGO_PKG_VERSION")))
        .form(&[("r", "login2"), ("u", user), ("p", password)])
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|_| tr!("Não foi possível falar com o RetroAchievements.").to_string())?;
    let body: serde_json::Value = reply.json().await.map_err(|_| tr!("O RetroAchievements respondeu algo inesperado.").to_string())?;
    if body.get("Success").and_then(serde_json::Value::as_bool) != Some(true) {
        let why = body.get("Error").and_then(serde_json::Value::as_str).unwrap_or(tr!("usuário ou senha incorretos"));
        return Err(tr!("O RetroAchievements recusou: {}.", why));
    }
    let token = body.get("Token").and_then(serde_json::Value::as_str).filter(|t| !t.is_empty()).ok_or(tr!("O RetroAchievements não mandou o código de acesso."))?;
    let name = body.get("User").and_then(serde_json::Value::as_str).unwrap_or(user).to_string();
    Ok((name, token.to_string()))
}

/// Puts the account in RetroArch's settings when they don't have it already
/// (backed up first). Nothing is written when it's already there.
pub fn write_token(exe: &Path, name: &str, token: &str) -> Result<(), String> {
    let cfg = config_file(exe);
    let text = fs::read_to_string(&cfg).unwrap_or_default();
    let same = config_value(&text, "cheevos_enable").as_deref() == Some("true")
        && config_value(&text, "cheevos_username").as_deref() == Some(name)
        && config_value(&text, "cheevos_token").as_deref() == Some(token);
    if same {
        return Ok(());
    }
    // The player's own settings, kept once: a second link mustn't replace them with ours.
    let backup = cfg.with_extension("cfg.cerberus-bak");
    if !text.is_empty() && !backup.exists() {
        let _ = fs::copy(&cfg, &backup);
    }
    let updated = with_values(&text, &[("cheevos_enable", "true"), ("cheevos_username", name), ("cheevos_token", token), ("cheevos_password", "")]);
    fs::write(&cfg, updated).map_err(|e| tr!("Não foi possível gravar as configurações do RetroArch: {}", e))
}

/// Takes the account Cerberus put in RetroArch's settings back out (when it is
/// still that one: a token the player set there themselves stays).
pub fn forget_token(exe: &Path, token: &str) {
    let cfg = config_file(exe);
    let Ok(text) = fs::read_to_string(&cfg) else { return };
    if config_value(&text, "cheevos_token").as_deref() != Some(token) {
        return;
    }
    let updated = with_values(&text, &[("cheevos_token", ""), ("cheevos_username", "")]);
    let _ = fs::write(&cfg, updated);
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
    fn several_discs_open_from_a_list_in_cerberus_folder() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let discs = vec![PathBuf::from(r"D:\ps1\FF7 (Disc 1).chd"), PathBuf::from(r"D:\ps1\FF7 (Disc 2).chd")];
        let list = disc_list(&data, &discs[0], &discs, "ps1").unwrap();
        assert!(list.starts_with(data.join("emulators").join("playlists")));
        assert_eq!(list.file_name().unwrap(), "FF7.m3u", "named after the game, so its saves are found by name");
        assert_eq!(fs::read_to_string(&list).unwrap(), "D:\\ps1\\FF7 (Disc 1).chd\nD:\\ps1\\FF7 (Disc 2).chd\n");
        assert_eq!(disc_list(&data, &discs[0], &discs, "ps2").unwrap(), discs[0], "a core that can't swap: disc 1");
        let own = PathBuf::from(r"D:\ps1\FF7.m3u");
        assert_eq!(disc_list(&data, &own, &discs, "ps1").unwrap(), own, "the player's own list as it is");
        assert_eq!(disc_list(&data, &discs[0], &[], "ps1").unwrap(), discs[0]);
    }

    #[test]
    fn bios_files_go_where_each_core_looks_for_them() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("RetroArch").join("retroarch.exe");
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::write(&exe, b"MZ").unwrap();
        let from = dir.path().join("bios");
        fs::create_dir_all(&from).unwrap();
        fs::write(from.join("scph5501.bin"), vec![0u8; PS1_BIOS as usize]).unwrap();
        fs::write(from.join("SCPH-70012.bin"), vec![0u8; PS2_BIOS as usize]).unwrap();
        fs::write(from.join("SCPH-70012.nvm"), b"x").unwrap();
        fs::write(from.join("notes.txt"), b"x").unwrap();
        assert_eq!(bios_status(&exe), (false, false));
        assert_eq!(import_bios(&exe, &from).unwrap(), (1, 1));
        assert!(system_dir(&exe).join("scph5501.bin").is_file());
        assert!(ps2_bios_dir(&exe).join("SCPH-70012.bin").is_file());
        assert!(ps2_bios_dir(&exe).join("SCPH-70012.nvm").is_file());
        assert!(!system_dir(&exe).join("notes.txt").exists());
        assert_eq!(bios_status(&exe), (true, true));
        let empty = dir.path().join("empty");
        fs::create_dir_all(&empty).unwrap();
        assert!(import_bios(&exe, &empty).is_err());
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
        assert_eq!(core_for("ps2"), Some("pcsx2"));
        assert_eq!(core_for("x360"), None);
        let dir = tempfile::tempdir().unwrap();
        let view = status(dir.path(), None);
        let mut names: Vec<&str> = view.cores.iter().map(|c| c.core.as_str()).collect();
        names.dedup();
        assert_eq!(names.len(), view.cores.len());
    }

    #[test]
    fn the_bios_is_looked_for_where_retroarch_is_set_to_read_it() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("retroarch.exe");
        assert_eq!(system_dir(&exe), dir.path().join("system"));
        fs::write(config_file(&exe), "system_directory = \":\\bios\"\n").unwrap();
        assert_eq!(system_dir(&exe), dir.path().join("bios"));
        fs::write(config_file(&exe), "system_directory = \"D:\\BIOS\"\n").unwrap();
        assert_eq!(system_dir(&exe), PathBuf::from(r"D:\BIOS"));
        fs::write(config_file(&exe), "system_directory = \"default\"\n").unwrap();
        assert_eq!(system_dir(&exe), dir.path().join("system"));
    }

    #[test]
    fn forgetting_the_account_takes_only_our_token_out_of_retroarch() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("retroarch.exe");
        fs::write(config_file(&exe), "video_fullscreen = \"true\"
").unwrap();
        write_token(&exe, "me", "abc").unwrap();
        write_token(&exe, "me", "def").unwrap();
        // The player's original settings are the backup, not our first edit.
        assert!(!fs::read_to_string(config_file(&exe).with_extension("cfg.cerberus-bak")).unwrap().contains("cheevos_token"));
        forget_token(&exe, "other");
        assert_eq!(config_value(&fs::read_to_string(config_file(&exe)).unwrap(), "cheevos_token").as_deref(), Some("def"), "not ours: kept");
        forget_token(&exe, "def");
        let text = fs::read_to_string(config_file(&exe)).unwrap();
        assert_eq!(config_value(&text, "cheevos_token").as_deref(), Some(""));
        assert_eq!(config_value(&text, "video_fullscreen").as_deref(), Some("true"));
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


/// What a reinstall of RetroArch keeps from the copy before: the player's
/// saves, save states, BIOS (`system`), playlists, settings and cores.
const KEPT_ON_REINSTALL: [&str; 10] =
    ["saves", "states", "system", "playlists", "config", "cores", "thumbnails", "screenshots", "retroarch.cfg", "retroarch-core-options.cfg"];

/// Puts the new RetroArch (`fresh`) in place of the one at `target`, moving the
/// player's files over. The old copy is set aside first, in one rename, so a
/// RetroArch that is open stops this before anything changes; any later
/// failure puts the old copy back.
fn swap_keeping_player_files(target: &Path, fresh: &Path) -> Result<(), String> {
    if !target.exists() {
        return fs::rename(fresh, target).map_err(|e| tr!("Não foi possível instalar o RetroArch: {}", e));
    }
    let old = target.with_file_name(".retroarch-old");
    let _ = fs::remove_dir_all(&old);
    fs::rename(target, &old).map_err(|_| tr!("Feche o RetroArch antes de reinstalar: ele está aberto ou com arquivos em uso."))?;
    let moved = (|| -> std::io::Result<()> {
        for name in KEPT_ON_REINSTALL {
            let from = old.join(name);
            if !from.exists() {
                continue;
            }
            let to = fresh.join(name);
            if to.is_dir() {
                fs::remove_dir_all(&to)?;
            } else if to.exists() {
                fs::remove_file(&to)?;
            }
            fs::rename(&from, &to)?;
        }
        fs::rename(fresh, target)
    })();
    match moved {
        Ok(()) => {
            let _ = fs::remove_dir_all(&old);
            Ok(())
        }
        Err(e) => {
            // Whatever moved goes back, then the old copy returns whole.
            for name in KEPT_ON_REINSTALL {
                let (from, to) = (fresh.join(name), old.join(name));
                if from.exists() && !to.exists() {
                    let _ = fs::rename(&from, &to);
                }
            }
            let _ = fs::remove_dir_all(target);
            let _ = fs::rename(&old, target);
            Err(tr!("Não foi possível reinstalar o RetroArch: {}. A instalação anterior continua como estava.", e))
        }
    }
}

#[cfg(test)]
mod reinstall_tests {
    use super::*;

    #[test]
    fn a_reinstall_keeps_saves_bios_and_settings() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("retroarch");
        let fresh = root.path().join("new");
        for dir in [&target.join("saves"), &target.join("system"), &fresh.join("system")] {
            fs::create_dir_all(dir).unwrap();
        }
        fs::write(target.join("saves/ff7.srm"), b"mine").unwrap();
        fs::write(target.join("system/scph5501.bin"), b"bios").unwrap();
        fs::write(target.join("retroarch.cfg"), b"video_fullscreen = \"true\"").unwrap();
        fs::write(target.join("retroarch.exe"), b"old").unwrap();
        fs::write(fresh.join("retroarch.exe"), b"new").unwrap();
        fs::write(fresh.join("retroarch.cfg"), b"defaults").unwrap();

        swap_keeping_player_files(&target, &fresh).unwrap();
        assert_eq!(fs::read(target.join("retroarch.exe")).unwrap(), b"new");
        assert_eq!(fs::read(target.join("saves/ff7.srm")).unwrap(), b"mine");
        assert_eq!(fs::read(target.join("system/scph5501.bin")).unwrap(), b"bios");
        assert_eq!(fs::read(target.join("retroarch.cfg")).unwrap(), b"video_fullscreen = \"true\"");
        assert!(!root.path().join(".retroarch-old").exists());
    }
}
