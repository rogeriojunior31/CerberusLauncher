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
    retroarch_dir(exe, "system_directory", "system")
}

/// A folder RetroArch's settings name (`key`), else its usual one beside the program.
pub fn retroarch_dir(exe: &Path, key: &str, usual: &str) -> PathBuf {
    let base = exe.parent().unwrap_or(Path::new("."));
    let chosen = fs::read_to_string(config_file(exe))
        .ok()
        .and_then(|text| config_value(&text, key))
        .filter(|v| !v.is_empty() && v != "default");
    match chosen {
        Some(v) if v.starts_with(':') => base.join(v.trim_start_matches(':').trim_start_matches(['\\', '/'])),
        Some(v) => PathBuf::from(v),
        None => base.join(usual),
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

/// Where RetroArch loads a core from: the cores folder its settings name
/// (`libretro_directory`), else `cores` beside it.
fn core_file(exe: &Path, core: &str) -> PathBuf {
    retroarch_dir(exe, "libretro_directory", "cores").join(format!("{core}_libretro.dll"))
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
    let reply = http
        .get(&url)
        .timeout(Duration::from_secs(120))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| tr!("Não foi possível baixar o núcleo: {}", e.without_url()))?;
    // A core is a few MB; a server that doesn't stop can't fill the memory.
    let bytes = crate::net::read_limited(reply, 256 << 20).await.map_err(|e| tr!("Não foi possível baixar o núcleo: {}", e))?;
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
    // Whole or not at all: a cut download never replaces the core that was there.
    crate::state_file::write_bytes(&target, &dll).map_err(|e| tr!("Não foi possível guardar o núcleo: {}", e))?;
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
    // Where RetroArch reads them, the same folder as the BIOS.
    let system = system_dir(exe);
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

/// RetroArch names a game's save after the file it opened. Someone who played
/// "FF7 (USA) (Disc 1).cue" there has "FF7 (USA) (Disc 1).srm"; opened through
/// the disc list "FF7 (USA).m3u" the game would start on an empty card. The
/// old save is copied to the list's name (never moved, never over one that's
/// there). Returns the copies made.
pub fn carry_disc_saves(exe: &Path, first_disc: &Path, list: &Path) -> Vec<PathBuf> {
    let (Some(old), Some(new)) = (first_disc.file_stem(), list.file_stem()) else { return Vec::new() };
    if old == new {
        return Vec::new();
    }
    let saves = retroarch_dir(exe, "savefile_directory", "saves");
    // With "sort saves by core" on, each core has its own folder inside.
    let mut dirs = vec![saves.clone()];
    if let Ok(entries) = fs::read_dir(&saves) {
        dirs.extend(entries.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.path()));
    }
    let mut copied = Vec::new();
    for dir in dirs {
        for ext in ["srm", "mcd"] {
            let from = dir.join(format!("{}.{ext}", old.to_string_lossy()));
            let to = dir.join(format!("{}.{ext}", new.to_string_lossy()));
            if from.is_file() && !to.exists() && fs::copy(&from, &to).is_ok() {
                copied.push(to);
            }
        }
    }
    copied
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
/// `asked`: the player linked it just now in Cerberus. Otherwise (before a
/// game) RetroArch signed in to another account, or with achievements turned
/// off there, is left as the player set it.
pub fn write_token(exe: &Path, name: &str, token: &str, asked: bool) -> Result<(), String> {
    let cfg = config_file(exe);
    // A settings file that's there but can't be read is left alone: writing
    // ours in its place would wipe the player's RetroArch settings.
    let text = match fs::read_to_string(&cfg) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(tr!("Não foi possível ler as configurações do RetroArch ({}); nada foi mudado.", e)),
    };
    let same = config_value(&text, "cheevos_enable").as_deref() == Some("true")
        && config_value(&text, "cheevos_username").as_deref() == Some(name)
        && config_value(&text, "cheevos_token").as_deref() == Some(token);
    if same {
        return Ok(());
    }
    let theirs = config_value(&text, "cheevos_username").filter(|u| !u.is_empty());
    let another_account = theirs.as_deref().is_some_and(|u| !u.eq_ignore_ascii_case(name));
    let turned_off = config_value(&text, "cheevos_enable").as_deref() == Some("false") && theirs.is_some();
    if !asked && (another_account || turned_off) {
        return Ok(());
    }
    // The player's own settings, kept once: a second link mustn't replace them with ours.
    let backup = cfg.with_extension("cfg.cerberus-bak");
    if !text.is_empty() && !backup.exists() {
        let _ = fs::copy(&cfg, &backup);
    }
    let updated = with_values(&text, &[("cheevos_enable", "true"), ("cheevos_username", name), ("cheevos_token", token), ("cheevos_password", "")]);
    crate::state_file::write_bytes(&cfg, updated.as_bytes()).map_err(|e| tr!("Não foi possível gravar as configurações do RetroArch: {}", e))
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
    let _ = crate::state_file::write_bytes(&cfg, updated.as_bytes());
}

/// Time played and when, per classic (by its file), kept in `classics.json`.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Played {
    pub seconds: u64,
    pub last: u64,
}

pub fn played(data: &Path) -> std::collections::HashMap<String, Played> {
    crate::state_file::load_or_default(&data.join("classics.json"))
}

/// A classic's file moved: its play time, sessions, note and favorite follow it.
pub fn follow_moved(data: &Path, old: &Path, new: &Path) {
    let (old, new) = (old.to_string_lossy().into_owned(), new.to_string_lossy().into_owned());
    let _ = crate::state_file::update(&data.join("classics.json"), |all: &mut std::collections::HashMap<String, Played>| {
        if let Some(time) = all.remove(&old) {
            let entry = all.entry(new.clone()).or_default();
            entry.seconds += time.seconds;
            entry.last = entry.last.max(time.last);
        }
    });
    let (old_id, new_id) = (format!("classic:{old}"), format!("classic:{new}"));
    crate::play_log::rename(data, &old_id, &new_id);
    crate::game_notes::rename(data, &old_id, &new_id);
    crate::favorites::rename(data, &old_id, &new_id);
}

/// Adds a session; two classics closing at once both count, and a file that
/// can't be read is never replaced by this one session.
pub fn add_played(data: &Path, rom: &str, seconds: u64, now: u64) -> Result<(), String> {
    crate::state_file::update(&data.join("classics.json"), |all: &mut std::collections::HashMap<String, Played>| {
        let entry = all.entry(rom.to_string()).or_default();
        entry.seconds += seconds;
        entry.last = now;
    })
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
    fn a_save_made_from_disc_one_follows_the_game_to_its_disc_list() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("retroarch.exe");
        let by_core = dir.path().join("saves").join("PCSX-ReARMed");
        fs::create_dir_all(&by_core).unwrap();
        fs::write(by_core.join("FF7 (USA) (Disc 1).srm"), b"card").unwrap();
        let copied = carry_disc_saves(&exe, Path::new(r"D:\ps1\FF7 (USA) (Disc 1).cue"), Path::new(r"C:\x\FF7 (USA).m3u"));
        assert_eq!(copied, vec![by_core.join("FF7 (USA).srm")]);
        assert_eq!(fs::read(by_core.join("FF7 (USA).srm")).unwrap(), b"card");
        assert!(by_core.join("FF7 (USA) (Disc 1).srm").exists(), "the old one stays");
        // One already there is never replaced.
        fs::write(by_core.join("FF7 (USA).srm"), b"newer").unwrap();
        assert!(carry_disc_saves(&exe, Path::new(r"D:\ps1\FF7 (USA) (Disc 1).cue"), Path::new(r"C:\x\FF7 (USA).m3u")).is_empty());
        assert_eq!(fs::read(by_core.join("FF7 (USA).srm")).unwrap(), b"newer");
    }

    #[test]
    fn a_moved_classic_keeps_its_time_sessions_and_note() {
        let data = tempfile::tempdir().unwrap();
        let (old, new) = (Path::new(r"D:\ROMS\Crash.cue"), Path::new(r"E:\ROMS\Crash.cue"));
        add_played(data.path(), &old.to_string_lossy(), 600, 100).unwrap();
        crate::play_log::record(data.path(), &format!("classic:{}", old.display()), 600, 100);
        let note = crate::game_notes::Note { rating: 4, ..Default::default() };
        crate::game_notes::set(data.path(), &format!("classic:{}", old.display()), note).unwrap();
        follow_moved(data.path(), old, new);
        assert_eq!(played(data.path())[&new.to_string_lossy().into_owned()].seconds, 600);
        assert_eq!(crate::play_log::of_game(data.path(), &format!("classic:{}", new.display())).len(), 1);
        assert_eq!(crate::game_notes::load(data.path())[&format!("classic:{}", new.display())].rating, 4);
    }

    #[test]
    fn cores_go_where_retroarch_loads_them_from() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("retroarch.exe");
        assert_eq!(core_file(&exe, "snes9x"), dir.path().join("cores").join("snes9x_libretro.dll"));
        fs::write(config_file(&exe), "libretro_directory = \"D:\\RA\\cores\"\n").unwrap();
        assert_eq!(core_file(&exe, "snes9x"), PathBuf::from(r"D:\RA\cores").join("snes9x_libretro.dll"));
    }

    #[test]
    fn forgetting_the_account_takes_only_our_token_out_of_retroarch() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("retroarch.exe");
        fs::write(config_file(&exe), "video_fullscreen = \"true\"
").unwrap();
        write_token(&exe, "me", "abc", true).unwrap();
        write_token(&exe, "me", "def", true).unwrap();
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
    fn before_a_game_retroarch_keeps_the_account_and_switch_the_player_set() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("retroarch.exe");
        fs::write(config_file(&exe), "cheevos_enable = \"true\"\ncheevos_username = \"Outro\"\ncheevos_token = \"theirs\"\n").unwrap();
        write_token(&exe, "me", "ours", false).unwrap();
        assert_eq!(config_value(&fs::read_to_string(config_file(&exe)).unwrap(), "cheevos_username").as_deref(), Some("Outro"));
        // Turned off for a hack, with our own account: stays off.
        fs::write(config_file(&exe), "cheevos_enable = \"false\"\ncheevos_username = \"me\"\ncheevos_token = \"ours\"\n").unwrap();
        write_token(&exe, "me", "ours", false).unwrap();
        assert_eq!(config_value(&fs::read_to_string(config_file(&exe)).unwrap(), "cheevos_enable").as_deref(), Some("false"));
        // Linked by the player in Cerberus: written.
        write_token(&exe, "me", "new", true).unwrap();
        assert_eq!(config_value(&fs::read_to_string(config_file(&exe)).unwrap(), "cheevos_token").as_deref(), Some("new"));
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
/// player's files over.
fn swap_keeping_player_files(target: &Path, fresh: &Path) -> Result<(), String> {
    swap_keeping(target, fresh, &KEPT_ON_REINSTALL, &|dir: &Path| dir.to_path_buf(), "RetroArch")
}

/// The names in `base` that `keep` asks to keep: exact names, or "*.ext" for
/// every file with that extension.
pub fn kept_names(base: &Path, keep: &[&str]) -> Vec<String> {
    let mut names: Vec<String> = keep.iter().filter(|k| !k.starts_with("*.")).filter(|k| base.join(k).exists()).map(|k| k.to_string()).collect();
    let extensions: Vec<String> = keep.iter().filter_map(|k| k.strip_prefix("*.")).map(str::to_ascii_lowercase).collect();
    if !extensions.is_empty() {
        for entry in fs::read_dir(base).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let ext = Path::new(&name).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
            if entry.path().is_file() && extensions.contains(&ext) && !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

fn old_copy_of(target: &Path) -> PathBuf {
    target.with_file_name(format!(".{}-old", target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()))
}

/// Moves the player's files from `from_base` into `to_base`, replacing what the new copy brought.
fn move_kept(from_base: &Path, to_base: &Path, keep: &[&str]) -> std::io::Result<()> {
    for item in kept_names(from_base, keep) {
        let (from, to) = (from_base.join(&item), to_base.join(&item));
        if to.is_dir() {
            fs::remove_dir_all(&to)?;
        } else if to.exists() {
            fs::remove_file(&to)?;
        }
        fs::rename(&from, &to)?;
    }
    Ok(())
}

/// Finishes a swap that was interrupted (app closed, power cut): the old copy
/// set aside is never deleted while it may still hold the player's files.
/// - No program at `target`: the old copy goes back whole.
/// - The new copy in place: the player's files still in the old one move over.
pub fn recover_swap(target: &Path, keep: &[&str], base: &dyn Fn(&Path) -> PathBuf) -> Result<(), String> {
    let old = old_copy_of(target);
    if !old.exists() {
        return Ok(());
    }
    if !target.exists() {
        return fs::rename(&old, target).map_err(|e| e.to_string());
    }
    move_kept(&base(&old), &base(target), keep).map_err(|e| e.to_string())?;
    fs::remove_dir_all(&old).map_err(|e| e.to_string())
}

/// Puts a new copy of a program (`fresh`) in place of the one at `target`,
/// keeping the player's files (`keep`, found in `base` of each copy: the folder
/// with the program). The player's files never pass through `fresh` (a temporary
/// folder, wiped on the next try): the old copy is set aside in one rename (a
/// program that is open stops it there), the new one takes its place, and only
/// then do the player's files move across. An interruption at any point is
/// finished by `recover_swap` on the next install, with nothing lost.
pub fn swap_keeping(target: &Path, fresh: &Path, keep: &[&str], base: &dyn Fn(&Path) -> PathBuf, name: &str) -> Result<(), String> {
    recover_swap(target, keep, base).map_err(|e| tr!("Não foi possível reinstalar o {}: {}. A instalação anterior continua como estava.", name, e))?;
    if !target.exists() {
        return fs::rename(fresh, target).map_err(|e| tr!("Não foi possível instalar o {}: {}", name, e));
    }
    let old = old_copy_of(target);
    fs::rename(target, &old).map_err(|_| tr!("Feche o {} antes de reinstalar: ele está aberto ou com arquivos em uso.", name))?;
    if let Err(e) = fs::rename(fresh, target) {
        let _ = fs::rename(&old, target);
        return Err(tr!("Não foi possível reinstalar o {}: {}. A instalação anterior continua como estava.", name, e));
    }
    match move_kept(&base(&old), &base(target), keep) {
        Ok(()) => {
            let _ = fs::remove_dir_all(&old);
            Ok(())
        }
        // Whatever didn't move is still in the old copy, which stays: the next try finishes it.
        Err(e) => Err(tr!("Não foi possível terminar de reinstalar o {}: {}. Seus arquivos estão guardados e o Cerberus termina na próxima vez.", name, e)),
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

    #[test]
    fn a_failed_swap_puts_the_old_copy_back() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("emu");
        fs::create_dir_all(target.join("memcards")).unwrap();
        fs::write(target.join("memcards/card1.mcd"), b"mine").unwrap();
        fs::write(target.join("emu.exe"), b"old").unwrap();
        // The new copy can't be moved in: it doesn't exist.
        let missing = root.path().join("not-there");
        assert!(swap_keeping(&target, &missing, &["memcards"], &|d: &Path| d.to_path_buf(), "Emu").is_err());
        assert_eq!(fs::read(target.join("memcards/card1.mcd")).unwrap(), b"mine");
        assert_eq!(fs::read(target.join("emu.exe")).unwrap(), b"old");
    }

    #[test]
    fn an_interrupted_reinstall_loses_nothing_and_is_finished_next_time() {
        let id = |d: &Path| d.to_path_buf();
        // 1. Stopped right after setting the old copy aside: it comes back whole.
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("emu");
        let old = root.path().join(".emu-old");
        fs::create_dir_all(old.join("memcards")).unwrap();
        fs::write(old.join("memcards/card.mcd"), b"mine").unwrap();
        recover_swap(&target, &["memcards"], &id).unwrap();
        assert_eq!(fs::read(target.join("memcards/card.mcd")).unwrap(), b"mine");
        assert!(!old.exists());

        // 2. Stopped after the new copy took its place, before the files moved:
        //    a new reinstall (with yet another fresh copy) finishes the move first.
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("emu");
        let old = root.path().join(".emu-old");
        fs::create_dir_all(old.join("memcards")).unwrap();
        fs::write(old.join("memcards/card.mcd"), b"mine").unwrap();
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("emu.exe"), b"new").unwrap();
        let fresh = root.path().join("fresher");
        fs::create_dir_all(&fresh).unwrap();
        fs::write(fresh.join("emu.exe"), b"newer").unwrap();
        swap_keeping(&target, &fresh, &["memcards"], &id, "Emu").unwrap();
        assert_eq!(fs::read(target.join("memcards/card.mcd")).unwrap(), b"mine");
        assert_eq!(fs::read(target.join("emu.exe")).unwrap(), b"newer");
        assert!(!old.exists());
    }

    #[test]
    fn extensions_keep_every_matching_file() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("hdd.qcow2"), b"x").unwrap();
        fs::write(root.path().join("xemu.exe"), b"x").unwrap();
        let mut names = kept_names(root.path(), &["xemu.toml", "*.qcow2"]);
        names.sort();
        assert_eq!(names, vec!["hdd.qcow2"]);
    }
}
