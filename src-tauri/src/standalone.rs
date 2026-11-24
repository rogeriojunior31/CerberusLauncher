//! Standalone emulators, beside RetroArch: DuckStation, PCSX2, RPCS3, PPSSPP,
//! Dolphin and Cemu. Each is installed from its own project's official
//! release (never from anywhere else, and never with BIOS or firmware), or
//! found where the player already has it. The folders each emulator already
//! lists games from are read (only read) so those games join the classics.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use cerberus_shared::{StandaloneView, tr};
use serde_json::Value;

/// Where an emulator's Windows build comes from.
pub enum Source {
    /// A GitHub release: the newest stable one, or a fixed tag (a rolling build).
    GitHub { repo: &'static str, tag: Option<&'static str>, asset: fn(&str) -> bool },
    /// Dolphin publishes its builds on its own site, named after its tags.
    Dolphin,
}

pub struct Emulator {
    pub id: &'static str,
    pub name: &'static str,
    /// The consoles it plays, as the ROM scan names them.
    pub consoles: &'static [&'static str],
    /// The program, by any of these file names.
    pub exe: &'static [&'static str],
    pub source: Source,
    /// The arguments that open a game straight away, in full screen.
    pub args: fn(&Path) -> Vec<String>,
    /// Where Windows installers usually put it, under Program Files or the user's programs.
    pub installed_in: &'static [&'static str],
}

fn path(rom: &Path) -> String {
    rom.to_string_lossy().into_owned()
}

pub const ALL: &[Emulator] = &[
    Emulator {
        id: "duckstation",
        name: "DuckStation",
        consoles: &["ps1"],
        exe: &["duckstation-qt-x64-ReleaseLTCG.exe", "duckstation-qt.exe"],
        source: Source::GitHub { repo: "stenzek/duckstation", tag: Some("latest"), asset: |n| n.eq_ignore_ascii_case("duckstation-windows-x64-release.zip") },
        args: |rom| vec!["-batch".into(), "-fullscreen".into(), "--".into(), path(rom)],
        installed_in: &["DuckStation"],
    },
    Emulator {
        id: "pcsx2",
        name: "PCSX2",
        consoles: &["ps2"],
        exe: &["pcsx2-qt.exe", "pcsx2-qtx64-avx2.exe", "pcsx2-qtx64.exe"],
        source: Source::GitHub { repo: "PCSX2/pcsx2", tag: None, asset: |n| n.starts_with("pcsx2-v") && n.ends_with("-windows-x64-Qt.7z") },
        args: |rom| vec!["-batch".into(), "-fullscreen".into(), "--".into(), path(rom)],
        installed_in: &["PCSX2"],
    },
    Emulator {
        id: "rpcs3",
        name: "RPCS3",
        consoles: &["ps3"],
        exe: &["rpcs3.exe"],
        source: Source::GitHub { repo: "RPCS3/rpcs3-binaries-win", tag: None, asset: |n| n.starts_with("rpcs3-") && (n.ends_with("_win64.7z") || n.ends_with("_win64_msvc.7z")) },
        args: |rom| vec!["--no-gui".into(), path(rom)],
        installed_in: &["RPCS3"],
    },
    Emulator {
        id: "ppsspp",
        name: "PPSSPP",
        consoles: &["psp"],
        exe: &["PPSSPPWindows64.exe", "PPSSPPWindows.exe"],
        source: Source::GitHub { repo: "hrydgard/PPSSPP", tag: None, asset: |n| n.starts_with("PPSSPP-v") && n.ends_with("-Windows-x64.zip") },
        args: |rom| vec!["--fullscreen".into(), path(rom)],
        installed_in: &["PPSSPP"],
    },
    Emulator {
        id: "dolphin",
        name: "Dolphin",
        consoles: &["gc", "wii"],
        exe: &["Dolphin.exe"],
        source: Source::Dolphin,
        args: |rom| vec!["-b".into(), "-e".into(), path(rom)],
        installed_in: &["Dolphin", "Dolphin-x64", "Dolphin Emulator"],
    },
    Emulator {
        id: "cemu",
        name: "Cemu",
        consoles: &["wiiu"],
        exe: &["Cemu.exe"],
        source: Source::GitHub { repo: "cemu-project/Cemu", tag: None, asset: |n| n.starts_with("cemu-") && n.ends_with("-windows-x64.zip") },
        args: |rom| vec!["-f".into(), "-g".into(), path(rom)],
        installed_in: &["Cemu"],
    },
];

pub fn find(id: &str) -> Option<&'static Emulator> {
    ALL.iter().find(|e| e.id == id)
}

/// The emulators that play a console.
pub fn for_console(console: &str) -> impl Iterator<Item = &'static Emulator> + '_ {
    ALL.iter().filter(move |e| e.consoles.contains(&console))
}

/// Where Cerberus keeps the emulators it installs.
pub fn managed_dir(data: &Path, id: &str) -> PathBuf {
    data.join("emulators").join(id)
}

/// The program inside a folder, a few levels down at most (archives often wrap it in one).
fn program_in(dir: &Path, names: &[&str], depth: u32) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_file() && p.file_name().is_some_and(|n| names.iter().any(|w| n.eq_ignore_ascii_case(w))) {
            return Some(p);
        }
        if p.is_dir() {
            subdirs.push(p);
        }
    }
    if depth == 0 {
        return None;
    }
    subdirs.into_iter().find_map(|d| program_in(&d, names, depth - 1))
}

/// The emulator's program: the one the player picked, Cerberus's own copy, or
/// one installed in the usual places. `bool`: it's Cerberus's own.
pub fn locate(data: &Path, picked: Option<&Path>, emulator: &Emulator) -> Option<(PathBuf, bool)> {
    if let Some(p) = picked.filter(|p| p.is_file()) {
        return Some((p.to_path_buf(), false));
    }
    if let Some(p) = program_in(&managed_dir(data, emulator.id), emulator.exe, 2) {
        return Some((p, true));
    }
    let roots: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
        .iter()
        .filter_map(|v| std::env::var_os(v).map(PathBuf::from))
        .chain(std::env::var_os("LOCALAPPDATA").map(|l| PathBuf::from(l).join("Programs")))
        .collect();
    roots.iter().flat_map(|r| emulator.installed_in.iter().map(move |d| r.join(d))).find_map(|d| program_in(&d, emulator.exe, 1)).map(|p| (p, false))
}

// ---------- the folders each emulator lists games from (read only) ----------

/// Values of `key` under `[section]` in an INI file (keys may repeat).
fn ini_values(text: &str, section: &str, key: &str) -> Vec<String> {
    let mut current = String::new();
    let mut found = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            current = name.trim().to_string();
            continue;
        }
        if !current.eq_ignore_ascii_case(section) {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            if k.eq_ignore_ascii_case(key) || (key.ends_with('*') && k.to_ascii_lowercase().starts_with(&key[..key.len() - 1].to_ascii_lowercase())) {
                let v = v.trim().trim_matches('"').trim();
                if !v.is_empty() {
                    found.push(v.to_string());
                }
            }
        }
    }
    found
}

/// Every `<Entry>` inside Cemu's `<GamePaths>`.
fn cemu_paths(xml: &str) -> Vec<String> {
    let Some(start) = xml.find("<GamePaths>") else { return Vec::new() };
    let end = xml[start..].find("</GamePaths>").map_or(xml.len(), |e| start + e);
    let mut out = Vec::new();
    let mut rest = &xml[start..end];
    while let Some(i) = rest.find("<Entry>") {
        rest = &rest[i + 7..];
        let Some(j) = rest.find("</Entry>") else { break };
        let value = rest[..j].trim();
        if !value.is_empty() {
            out.push(value.replace("&amp;", "&"));
        }
        rest = &rest[j..];
    }
    out
}

/// The game folders listed in RPCS3's `games.yml` ("SERIAL: folder").
fn rpcs3_paths(yml: &str) -> Vec<String> {
    yml.lines()
        .filter_map(|l| l.split_once(": "))
        .map(|(_, v)| v.trim().trim_matches(|c| c == '"' || c == '\'').to_string())
        .filter(|v| !v.is_empty())
        .collect()
}

fn documents() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("Documents"))
}

fn appdata() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(PathBuf::from)
}

/// The settings files an emulator may keep: beside the program when it's
/// portable, in the user's folders otherwise.
fn config_files(id: &str, exe: Option<&Path>) -> Vec<PathBuf> {
    let beside = exe.and_then(Path::parent).map(Path::to_path_buf);
    let mut files = Vec::new();
    let mut add = |base: Option<PathBuf>, rel: &str| {
        if let Some(b) = base {
            files.push(b.join(rel));
        }
    };
    match id {
        "duckstation" => {
            add(beside.clone(), "settings.ini");
            add(documents(), "DuckStation/settings.ini");
            add(std::env::var_os("LOCALAPPDATA").map(PathBuf::from), "DuckStation/settings.ini");
        }
        "pcsx2" => {
            add(beside.clone(), "inis/PCSX2.ini");
            add(documents(), "PCSX2/inis/PCSX2.ini");
        }
        "rpcs3" => {
            add(beside.clone(), "config/games.yml");
            add(beside.clone(), "games.yml");
        }
        "ppsspp" => {
            add(beside.clone(), "memstick/PSP/SYSTEM/ppsspp.ini");
            add(documents(), "PPSSPP/PSP/SYSTEM/ppsspp.ini");
        }
        "dolphin" => {
            add(beside.clone(), "User/Config/Dolphin.ini");
            add(appdata(), "Dolphin Emulator/Config/Dolphin.ini");
            add(documents(), "Dolphin Emulator/Config/Dolphin.ini");
        }
        "cemu" => {
            add(beside.clone(), "settings.xml");
            add(beside.clone(), "portable/settings.xml");
            add(appdata(), "Cemu/settings.xml");
        }
        _ => {}
    }
    files
}

/// The game folders one emulator's settings list.
pub fn folders_in(id: &str, text: &str) -> Vec<String> {
    match id {
        "duckstation" | "pcsx2" => [ini_values(text, "GameList", "RecursivePaths"), ini_values(text, "GameList", "Paths")].concat(),
        "ppsspp" => ini_values(text, "General", "CurrentDirectory"),
        "dolphin" => ini_values(text, "General", "ISOPath*").into_iter().filter(|v| !v.chars().all(|c| c.is_ascii_digit())).collect(),
        "cemu" => cemu_paths(text),
        "rpcs3" => rpcs3_paths(text),
        _ => Vec::new(),
    }
}

/// Every folder the emulators on this PC (RetroArch too) list games from, that exists.
pub fn game_folders(data: &Path, picked: &BTreeMap<String, PathBuf>, retroarch: Option<&Path>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for folder in retroarch.map(retroarch_folders).unwrap_or_default() {
        let folder = PathBuf::from(folder.replace('/', "\\"));
        if folder.is_dir() && !out.iter().any(|o| o == &folder) {
            out.push(folder);
        }
    }
    for emulator in ALL {
        let exe = locate(data, picked.get(emulator.id).map(PathBuf::as_path), emulator).map(|(p, _)| p);
        for file in config_files(emulator.id, exe.as_deref()) {
            let Ok(text) = fs::read_to_string(&file) else { continue };
            for folder in folders_in(emulator.id, &text) {
                let folder = PathBuf::from(folder.replace('/', "\\"));
                if folder.is_dir() && !out.iter().any(|o| o == &folder) {
                    out.push(folder);
                }
            }
        }
    }
    out
}

// ---------- BIOS and firmware (the player's own, never downloaded) ----------

/// A PlayStation BIOS is 512 KB; a PlayStation 2 BIOS, 4 MB.
const PS1_BIOS: u64 = 512 * 1024;
const PS2_BIOS: u64 = 4 * 1024 * 1024;

/// What an emulator needs from the player's own console before games start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Needs {
    Ps1Bios,
    Ps2Bios,
    /// PS3 system software, installed by RPCS3 from the player's PS3UPDAT.PUP.
    Ps3Firmware,
}

pub fn needs(id: &str) -> Option<Needs> {
    match id {
        "duckstation" => Some(Needs::Ps1Bios),
        "pcsx2" => Some(Needs::Ps2Bios),
        "rpcs3" => Some(Needs::Ps3Firmware),
        _ => None,
    }
}

fn portable(exe: &Path) -> bool {
    exe.parent().is_some_and(|d| d.join("portable.txt").is_file() || d.join("portable.ini").is_file())
}

/// Where the emulator looks for its BIOS (or firmware) on this PC.
pub fn bios_dir(id: &str, exe: &Path) -> Option<PathBuf> {
    let beside = exe.parent()?.to_path_buf();
    match id {
        "duckstation" if portable(exe) => Some(beside.join("bios")),
        "duckstation" => documents().map(|d| d.join("DuckStation").join("bios")),
        "pcsx2" if portable(exe) => Some(beside.join("bios")),
        "pcsx2" => documents().map(|d| d.join("PCSX2").join("bios")),
        "rpcs3" => Some(beside.join("dev_flash").join("sys").join("external")),
        _ => None,
    }
}

fn has_bin(dir: &Path, size: u64) -> bool {
    fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|e| {
            e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("bin")) && e.metadata().is_ok_and(|m| m.is_file() && m.len() == size)
        })
    })
}

/// Whether what the emulator needs is in place (None: it needs nothing).
pub fn bios_ready(id: &str, exe: &Path) -> Option<bool> {
    let dir = bios_dir(id, exe)?;
    Some(match needs(id)? {
        Needs::Ps1Bios => has_bin(&dir, PS1_BIOS),
        Needs::Ps2Bios => has_bin(&dir, PS2_BIOS),
        Needs::Ps3Firmware => fs::read_dir(&dir).is_ok_and(|mut d| d.next().is_some()),
    })
}

/// Copies the player's BIOS files from a folder into the emulator's BIOS
/// folder, telling them apart by size. Returns how many were copied.
pub fn import_bios(id: &str, exe: &Path, from: &Path) -> Result<usize, String> {
    let (kind, dir) = (needs(id), bios_dir(id, exe));
    let (Some(kind), Some(dir)) = (kind, dir) else { return Err(tr!("Esse emulador não usa BIOS.").into()) };
    let wanted = match kind {
        Needs::Ps1Bios => PS1_BIOS,
        Needs::Ps2Bios => PS2_BIOS,
        Needs::Ps3Firmware => return Err(tr!("O firmware do PS3 é instalado pelo próprio RPCS3, a partir do arquivo PS3UPDAT.PUP.").into()),
    };
    let entries = fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))?;
    let mut copied = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        let ext = path.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
        // PCSX2 also keeps the BIOS's NVRAM and MEC files beside it.
        let companion = kind == Needs::Ps2Bios && matches!(ext.as_str(), "mec" | "nvm");
        if !meta.is_file() || !((ext == "bin" && meta.len() == wanted) || companion) {
            continue;
        }
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let name = entry.file_name();
        fs::copy(&path, dir.join(&name)).map_err(|e| tr!("Não foi possível copiar {0}: {1}", name.to_string_lossy(), e))?;
        if ext == "bin" {
            copied += 1;
        }
    }
    if copied == 0 {
        return Err(match kind {
            Needs::Ps1Bios => tr!("Nenhuma BIOS de PlayStation (arquivo .bin de 512 KB) nessa pasta."),
            _ => tr!("Nenhuma BIOS de PlayStation 2 (arquivo .bin de 4 MB) nessa pasta."),
        }
        .into());
    }
    Ok(copied)
}

/// Has RPCS3 install the player's PS3 firmware file, in its own window.
pub fn install_firmware(exe: &Path, pup: &Path) -> Result<(), String> {
    let mut command = std::process::Command::new(exe);
    command.arg("--installfw").arg(pup);
    if let Some(dir) = exe.parent() {
        command.current_dir(dir);
    }
    command.spawn().map(|_| ()).map_err(|e| tr!("Não foi possível abrir o {}: {}", "RPCS3", e))
}

// ---------- RetroArch's own game folders ----------

/// The folders RetroArch knows games in: its file browser's start folder and
/// the folders of the games in its playlists.
pub fn retroarch_folders(exe: &Path) -> Vec<String> {
    let Some(dir) = exe.parent() else { return Vec::new() };
    let mut out: Vec<String> = Vec::new();
    if let Ok(cfg) = fs::read_to_string(dir.join("retroarch.cfg")) {
        out.extend(
            cfg.lines()
                .filter_map(|l| l.split_once('='))
                .filter(|(k, _)| k.trim() == "rgui_browser_directory")
                .map(|(_, v)| v.trim().trim_matches('"').to_string())
                .filter(|v| !v.is_empty() && v != "default"),
        );
    }
    if let Ok(lists) = fs::read_dir(dir.join("playlists")) {
        for list in lists.flatten().filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("lpl"))) {
            if let Ok(text) = fs::read_to_string(list.path()) {
                for folder in playlist_folders(&text) {
                    if !out.contains(&folder) {
                        out.push(folder);
                    }
                }
            }
        }
    }
    out
}

/// A RetroArch playlist (JSON): the folders its games are in. A game inside an
/// archive ("game.zip#game.sfc") counts as the archive.
fn playlist_folders(text: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<Value>(text) else { return Vec::new() };
    let mut out: Vec<String> = Vec::new();
    for item in v.get("items").and_then(Value::as_array).into_iter().flatten() {
        let Some(path) = item.get("path").and_then(Value::as_str) else { continue };
        let path = path.split('#').next().unwrap_or(path);
        if let Some(parent) = Path::new(path).parent().map(|p| p.to_string_lossy().to_string()).filter(|p| !p.is_empty()) {
            if !out.contains(&parent) {
                out.push(parent);
            }
        }
    }
    out
}

// ---------- installing ----------

/// The newest build's file: its address, name, and checksum when the host publishes one.
async fn latest_asset(http: &reqwest::Client, emulator: &Emulator) -> Result<crate::recomp::Asset, String> {
    match &emulator.source {
        Source::GitHub { repo, tag, asset } => {
            let path = match tag {
                Some(t) => format!("releases/tags/{t}"),
                None => "releases/latest".to_string(),
            };
            let request = http.get(format!("https://api.github.com/repos/{repo}/{path}")).header("Accept", "application/vnd.github+json").timeout(Duration::from_secs(20));
            let reply = crate::net::send(request).await.map_err(|_| tr!("Não foi possível falar com o GitHub.").to_string())?;
            if !reply.status().is_success() {
                return Err(tr!("O GitHub não respondeu como esperado ({}). Tente de novo em alguns minutos.", reply.status().as_u16()));
            }
            let release: Value = reply.json().await.map_err(|_| tr!("O GitHub respondeu algo inesperado.").to_string())?;
            pick_asset(&release, *asset).ok_or_else(|| tr!("A versão mais nova do {} não tem o arquivo para Windows.", emulator.name))
        }
        Source::Dolphin => {
            let request = http.get("https://api.github.com/repos/dolphin-emu/dolphin/tags?per_page=30").header("Accept", "application/vnd.github+json").timeout(Duration::from_secs(20));
            let reply = crate::net::send(request).await.map_err(|_| tr!("Não foi possível falar com o GitHub.").to_string())?;
            let tags: Value = reply.json().await.map_err(|_| tr!("O GitHub respondeu algo inesperado.").to_string())?;
            let names: Vec<String> = tags.as_array().into_iter().flatten().filter_map(|t| t.get("name").and_then(Value::as_str)).map(str::to_string).collect();
            let version = newest_dolphin(&names).ok_or(tr!("Não foi possível achar a versão mais nova do Dolphin."))?;
            Ok(crate::recomp::Asset {
                name: format!("dolphin-{version}-x64.7z"),
                size: 0,
                url: format!("https://dl.dolphin-emu.org/releases/{version}/dolphin-{version}-x64.7z"),
                sha256: None,
            })
        }
    }
}

/// The release's Windows file, with GitHub's own SHA-256 when it lists one.
fn pick_asset(release: &Value, wanted: fn(&str) -> bool) -> Option<crate::recomp::Asset> {
    let assets = release.get("assets")?.as_array()?;
    let a = assets.iter().find(|a| a.get("name").and_then(Value::as_str).is_some_and(wanted))?;
    let url = a.get("browser_download_url")?.as_str()?.to_string();
    if !url.starts_with("https://github.com/") {
        return None;
    }
    let sha256 = a.get("digest").and_then(Value::as_str).and_then(|d| d.strip_prefix("sha256:")).map(str::to_string);
    Some(crate::recomp::Asset { name: a.get("name")?.as_str()?.to_string(), size: a.get("size").and_then(Value::as_u64).unwrap_or(0), url, sha256 })
}

/// Dolphin's release tags are years and months ("2609"), with a letter for a fix ("2609a").
fn newest_dolphin(tags: &[String]) -> Option<String> {
    tags.iter()
        .filter(|t| {
            let digits = t.trim_end_matches(|c: char| c.is_ascii_lowercase());
            digits.len() == 4 && digits.chars().all(|c| c.is_ascii_digit()) && t.len() <= 5
        })
        .max_by_key(|t| (t[..4].parse::<u32>().unwrap_or(0), t[4..].to_string()))
        .cloned()
}

/// Downloads and unpacks an emulator into Cerberus's folder; returns its program.
pub async fn install(http: &reqwest::Client, data: &Path, emulator: &Emulator, report: impl Fn(u64, u64) + Send) -> Result<PathBuf, String> {
    let asset = latest_asset(http, emulator).await?;
    let downloads = data.join("downloads");
    let archive = crate::recomp::download_file(http, &asset, &downloads, &crate::recomp::Halt::default(), report).await?;
    let staging = data.join("emulators").join(format!(".{}-new", emulator.id));
    let _ = fs::remove_dir_all(&staging);
    let seven = asset.name.to_ascii_lowercase().ends_with(".7z");
    let unpacked = tokio::task::spawn_blocking({
        let (archive, staging) = (archive.clone(), staging.clone());
        move || if seven { crate::recomp::extract_7z(&archive, &staging) } else { crate::recomp::extract_zip(&archive, &staging) }
    })
    .await
    .map_err(|e| e.to_string())?;
    let _ = fs::remove_file(&archive);
    unpacked?;
    if program_in(&staging, emulator.exe, 2).is_none() {
        let _ = fs::remove_dir_all(&staging);
        return Err(tr!("O pacote do {} veio diferente do esperado.", emulator.name));
    }
    let target = managed_dir(data, emulator.id);
    let _ = fs::remove_dir_all(&target);
    fs::rename(&staging, &target).map_err(|e| tr!("Não foi possível instalar o {}: {}", emulator.name, e))?;
    // Portable mode keeps the emulator's settings and saves in its own folder.
    if matches!(emulator.id, "duckstation" | "pcsx2" | "dolphin") {
        if let Some(dir) = program_in(&target, emulator.exe, 2).and_then(|p| p.parent().map(Path::to_path_buf)) {
            let _ = fs::write(dir.join("portable.txt"), "");
        }
    }
    program_in(&target, emulator.exe, 2).ok_or_else(|| tr!("O pacote do {} veio diferente do esperado.", emulator.name))
}

/// What Configurações shows for each emulator.
pub fn status(data: &Path, picked: &BTreeMap<String, PathBuf>) -> Vec<StandaloneView> {
    ALL.iter()
        .map(|e| {
            let found = locate(data, picked.get(e.id).map(PathBuf::as_path), e);
            StandaloneView {
                id: e.id.to_string(),
                name: e.name.to_string(),
                consoles: e.consoles.iter().map(|c| c.to_string()).collect(),
                exe: found.as_ref().map(|(p, _)| p.display().to_string()),
                managed: found.as_ref().is_some_and(|(_, m)| *m),
                bios: found.as_ref().and_then(|(p, _)| bios_ready(e.id, p)),
                firmware: needs(e.id) == Some(Needs::Ps3Firmware),
            }
        })
        .collect()
}

/// Opens a game in a standalone emulator.
pub fn launch(exe: &Path, emulator: &Emulator, rom: &Path) -> Result<std::process::Child, String> {
    let mut command = std::process::Command::new(exe);
    command.args((emulator.args)(rom));
    if let Some(dir) = exe.parent() {
        command.current_dir(dir);
    }
    command.spawn().map_err(|e| tr!("Não foi possível abrir o {}: {}", emulator.name, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_console_has_its_emulator_and_names_are_unique() {
        for console in ["ps1", "ps2", "ps3", "psp", "gc", "wii", "wiiu"] {
            assert!(for_console(console).next().is_some(), "{console}");
        }
        let mut ids: Vec<&str> = ALL.iter().map(|e| e.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), ALL.len());
    }

    #[test]
    fn the_right_windows_file_is_picked_from_each_release() {
        let pick = |id: &str, name: &str| match &find(id).unwrap().source {
            Source::GitHub { asset, .. } => asset(name),
            Source::Dolphin => false,
        };
        assert!(pick("duckstation", "duckstation-windows-x64-release.zip"));
        assert!(!pick("duckstation", "duckstation-windows-x64-release-symbols.7z") && !pick("duckstation", "duckstation-windows-arm64-release.zip"));
        assert!(pick("pcsx2", "pcsx2-v2.8.2-windows-x64-Qt.7z") && !pick("pcsx2", "pcsx2-v2.8.2-windows-x64-Qt-symbols.7z"));
        assert!(pick("rpcs3", "rpcs3-v0.0.43-20654-8bb970d0_win64_msvc.7z") && !pick("rpcs3", "rpcs3-v0.0.43-20654-8bb970d0_win64_msvc.7z.sha256"));
        assert!(pick("ppsspp", "PPSSPP-v1.20.4-Windows-x64.zip") && !pick("ppsspp", "PPSSPP-v1.20.4-Windows-ARM64.zip"));
        assert!(pick("cemu", "cemu-2.6-windows-x64.zip") && !pick("cemu", "cemu-2.6-ubuntu-22.04-x64.zip"));
        let release = serde_json::json!({ "assets": [
            { "name": "cemu-2.6-ubuntu-22.04-x64.zip", "browser_download_url": "https://github.com/x/ubuntu.zip", "size": 1 },
            { "name": "cemu-2.6-windows-x64.zip", "browser_download_url": "https://github.com/cemu-project/Cemu/releases/download/v2.6/cemu-2.6-windows-x64.zip", "size": 9, "digest": "sha256:abc" }
        ] });
        let a = pick_asset(&release, |n| n.starts_with("cemu-") && n.ends_with("-windows-x64.zip")).unwrap();
        assert_eq!((a.size, a.sha256.as_deref()), (9, Some("abc")));
        let elsewhere = serde_json::json!({ "assets": [{ "name": "cemu-2.6-windows-x64.zip", "browser_download_url": "https://evil.example/c.zip" }] });
        assert!(pick_asset(&elsewhere, |_| true).is_none(), "only GitHub's own download links");
    }

    #[test]
    fn dolphins_newest_release_tag_wins() {
        let tags: Vec<String> = ["nJoy", "2606a", "2609", "2609b", "2603", "5.0", "2609a"].iter().map(|s| s.to_string()).collect();
        assert_eq!(newest_dolphin(&tags).as_deref(), Some("2609b"));
    }

    #[test]
    fn game_folders_are_read_from_each_emulators_settings() {
        let ini = "[Main]\nRecursivePaths = C:\\No\n[GameList]\nRecursivePaths = D:\\PS1\nRecursivePaths = E:\\More\nPaths = F:\\Flat\n[Other]\nPaths = G:\\No\n";
        assert_eq!(folders_in("duckstation", ini), ["D:\\PS1", "E:\\More", "F:\\Flat"]);
        let dolphin = "[General]\nISOPaths = 2\nISOPath0 = D:/GC\nISOPath1 = D:/Wii\n";
        assert_eq!(folders_in("dolphin", dolphin), ["D:/GC", "D:/Wii"]);
        assert_eq!(folders_in("ppsspp", "[General]\nCurrentDirectory = \"D:/PSP/\"\n"), ["D:/PSP/"]);
        let cemu = "<content><GamePaths><Entry>D:\\WiiU</Entry><Entry>E:\\A &amp; B</Entry></GamePaths><Entry>X</Entry></content>";
        assert_eq!(folders_in("cemu", cemu), ["D:\\WiiU", "E:\\A & B"]);
        assert_eq!(folders_in("rpcs3", "BLUS30443: D:/PS3/Demon/\nNPUB31154: \"D:/PS3/Other/\"\n"), ["D:/PS3/Demon/", "D:/PS3/Other/"]);
    }

    /// Against the real sites (reads only, downloads nothing): each emulator's newest Windows file is found.
    /// `cargo test -p cerberus-launcher real_emulator_sources -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_emulator_sources() {
        let http = reqwest::Client::builder().user_agent("CerberusLauncher").build().unwrap();
        for e in ALL {
            let asset = tauri::async_runtime::block_on(latest_asset(&http, e)).unwrap_or_else(|err| panic!("{}: {err}", e.id));
            let head = tauri::async_runtime::block_on(http.head(&asset.url).send()).unwrap();
            assert!(head.status().is_success() || head.status().is_redirection(), "{}: {} answered {}", e.id, asset.url, head.status());
            println!("{:<12} {} ({} MB, sha256: {})", e.id, asset.name, asset.size / 1_000_000, asset.sha256.is_some());
        }
    }

    #[test]
    fn bios_goes_where_each_emulator_looks_and_by_size() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("emu").join("pcsx2-qt.exe");
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::write(&exe, b"MZ").unwrap();
        fs::write(exe.parent().unwrap().join("portable.txt"), b"").unwrap();
        assert_eq!(bios_dir("pcsx2", &exe).unwrap(), dir.path().join("emu").join("bios"));
        assert_eq!(bios_ready("pcsx2", &exe), Some(false));
        assert_eq!(bios_ready("ppsspp", &exe), None, "PPSSPP needs no BIOS");
        let mine = dir.path().join("mine");
        fs::create_dir_all(&mine).unwrap();
        fs::write(mine.join("scph39001.bin"), vec![0u8; PS2_BIOS as usize]).unwrap();
        fs::write(mine.join("scph39001.nvm"), b"n").unwrap();
        fs::write(mine.join("scph1001.bin"), vec![0u8; PS1_BIOS as usize]).unwrap();
        fs::write(mine.join("notes.txt"), b"x").unwrap();
        assert_eq!(import_bios("pcsx2", &exe, &mine).unwrap(), 1);
        assert_eq!(bios_ready("pcsx2", &exe), Some(true));
        let bios = dir.path().join("emu").join("bios");
        assert!(bios.join("scph39001.nvm").is_file() && !bios.join("scph1001.bin").exists() && !bios.join("notes.txt").exists());
        let empty = dir.path().join("empty");
        fs::create_dir_all(&empty).unwrap();
        assert!(import_bios("pcsx2", &exe, &empty).is_err());
    }

    #[test]
    fn retroarch_playlists_give_their_games_folders() {
        let lpl = r#"{"version":"1.5","items":[{"path":"D:\\ROMS\\SNES\\Zelda.sfc"},{"path":"D:\\ROMS\\SNES\\Mario.zip#Mario.sfc"},{"path":"E:\\GBA\\Golden Sun.gba"}]}"#;
        assert_eq!(playlist_folders(lpl), [r"D:\ROMS\SNES", r"E:\GBA"]);
        assert!(playlist_folders("not json").is_empty());
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("retroarch.exe");
        fs::write(dir.path().join("retroarch.cfg"), "video_fullscreen = \"true\"\nrgui_browser_directory = \"D:\\ROMS\"\n").unwrap();
        fs::create_dir_all(dir.path().join("playlists")).unwrap();
        fs::write(dir.path().join("playlists").join("Nintendo - SNES.lpl"), lpl).unwrap();
        assert_eq!(retroarch_folders(&exe), [r"D:\ROMS", r"D:\ROMS\SNES", r"E:\GBA"]);
    }

    #[test]
    fn games_open_straight_away_in_each_emulator() {
        let rom = Path::new(r"D:\ROMs\game.iso");
        for e in ALL {
            let args = (e.args)(rom);
            assert_eq!(args.last().map(String::as_str), Some(r"D:\ROMs\game.iso"), "{}", e.id);
        }
    }
}
