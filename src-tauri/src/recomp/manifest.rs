//! `recomplauncher.json`: what a recomp's developer says about their game, in
//! the RecompLauncher SDK's format (format 1, MIT: github.com/elripalda/RecompLauncher-SDK).
//! Read from the release, beside the game, it wins over the catalog: which
//! program starts, with which arguments, which files the player gives it (the
//! game, each disc, DLC, an update), how to tell they're the right ones, the
//! settings files, where it saves, and what its exit codes mean.
//!
//! Anything the file says that Cerberus can't do yet is left alone (`prepare`,
//! `dumps`, `mods`, `looks`, `games`, `system`): the game still plays.

use cerberus_shared::tr;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

pub const FILE: &str = "recomplauncher.json";

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Format {
    #[serde(rename = "type")]
    pub kind: String,
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Check {
    /// A file a folder copy holds (`default.xex`, `eboot.bin`).
    pub file: Option<String>,
    /// A file on an Xbox disc image.
    pub xdvdfs: Option<String>,
    /// A file in the top folder of an ISO 9660 disc (PS1, PS2...).
    pub iso9660: Option<String>,
    /// The IDs the game reads: a disc's serial, a GameCube/Wii ID, a title ID.
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Input {
    pub id: String,
    pub label: String,
    /// game, disc, dlc, update, key or runtime.
    pub role: String,
    pub optional: bool,
    pub formats: Vec<Format>,
    pub check: Check,
    /// A folder inside the game's where it is put, and the name it gets there.
    pub into: Option<String>,
    pub name: Option<String>,
    pub sha256: Option<String>,
    /// A key or runtime shared by every game (switch.prod-keys, nvidia.dlss).
    pub shared: Option<String>,
}

impl Input {
    fn takes(&self, kind: &str) -> bool {
        self.formats.is_empty() || self.formats.iter().any(|f| f.kind == kind)
    }

    pub fn takes_file(&self) -> bool {
        self.takes("file")
    }

    pub fn takes_folder(&self) -> bool {
        self.takes("folder")
    }

    /// Something the launcher fetches itself (NVIDIA DLSS): not asked of the player.
    pub fn fetched(&self) -> bool {
        self.role == "runtime"
    }
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Start {
    pub program: Vec<String>,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

/// The developer's own setup step, run once before the first start.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Prepare {
    pub program: Vec<String>,
    pub args: Vec<String>,
    pub label: String,
    /// Seconds it may take.
    pub timeout: u64,
}

/// A button under the settings: opens one of the game's folders.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Action {
    pub id: String,
    pub label: String,
    pub help: String,
    pub open: String,
}

#[derive(Debug, Clone, Default)]
pub struct Manifest {
    pub inputs: Vec<Input>,
    pub start: Option<Start>,
    pub prepare: Option<Prepare>,
    /// Exit code -> message, or "ok" for a normal close.
    pub exit_codes: BTreeMap<String, String>,
    pub saves: Vec<String>,
    /// Named places: `saves` and `logs` mean something to the launcher.
    pub folders: BTreeMap<String, Vec<String>>,
    pub actions: Vec<Action>,
    /// The whole file, for the settings (`config`, `settings`, `groups`).
    pub raw: Value,
}

/// `"x"`, `["x", "y"]`, `[{ "path": "x", "if": "..." }]` or `{ "path": "x" }`:
/// the places it names, in order.
fn places(value: &Value) -> Vec<String> {
    match value {
        Value::String(s) => vec![s.clone()],
        Value::Array(list) => list.iter().flat_map(places).collect(),
        Value::Object(o) => o.get("path").and_then(Value::as_str).map(|p| vec![p.to_string()]).unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

impl Manifest {
    /// Reads a manifest; `None` when it isn't one (not JSON, or another format).
    pub fn parse(text: &str) -> Option<Manifest> {
        let raw: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
        let object = raw.as_object()?;
        if object.get("format").and_then(Value::as_u64).is_some_and(|f| f != 1) {
            return None;
        }
        // One bad input mustn't hide the others; ids must be usable and unique.
        let mut inputs: Vec<Input> = Vec::new();
        for value in object.get("inputs").and_then(Value::as_array).into_iter().flatten().take(32) {
            let Ok(mut input) = serde_json::from_value::<Input>(value.clone()) else { continue };
            if input.role.is_empty() {
                input.role = "game".into();
            }
            if valid_id(&input.id) && !input.label.trim().is_empty() && !inputs.iter().any(|i| i.id == input.id) {
                inputs.push(input);
            }
        }
        // Before named inputs, the copy was a top-level `game` (still read, as the input `game`).
        if let Some(old) = object.get("game").filter(|g| g.is_object()) {
            if !inputs.iter().any(|i| i.id == "game") {
                let kind = old.get("kind").and_then(Value::as_str).unwrap_or_default();
                let formats = match kind {
                    "file" | "folder" => vec![Format { kind: kind.into(), extensions: Vec::new() }],
                    _ => Vec::new(),
                };
                let check = old.get("check").and_then(|c| serde_json::from_value(c.clone()).ok()).unwrap_or_default();
                let label = old.get("label").and_then(Value::as_str).unwrap_or("Game").to_string();
                inputs.insert(0, Input { id: "game".into(), label, role: "game".into(), formats, check, ..Input::default() });
            }
        }
        let start = object.get("start").and_then(|s| serde_json::from_value::<Start>(s.clone()).ok());
        let prepare = object.get("prepare").and_then(|s| serde_json::from_value::<Prepare>(s.clone()).ok()).filter(|p| !p.program.is_empty());
        let exit_codes = object
            .get("exit_codes")
            .and_then(Value::as_object)
            .map(|m| m.iter().filter_map(|(k, v)| Some((k.trim().to_string(), v.as_str()?.trim().to_string()))).collect())
            .unwrap_or_default();
        let folders: BTreeMap<String, Vec<String>> = object
            .get("folders")
            .and_then(Value::as_object)
            .map(|m| m.iter().map(|(k, v)| (k.clone(), places(v))).collect())
            .unwrap_or_default();
        let mut saves: Vec<String> = object.get("saves").map(places).unwrap_or_default();
        if saves.is_empty() {
            saves = folders.get("saves").cloned().unwrap_or_default();
        }
        let actions = object
            .get("actions")
            .and_then(Value::as_array)
            .map(|l| l.iter().filter_map(|a| serde_json::from_value::<Action>(a.clone()).ok()).filter(|a| !a.open.is_empty()).collect())
            .unwrap_or_default();
        Some(Manifest { inputs, start, prepare, exit_codes, saves, folders, actions, raw })
    }

    /// The player's copy: the input with role `game`, else the first disc.
    pub fn main_input(&self) -> Option<&Input> {
        self.inputs.iter().find(|i| i.role == "game").or_else(|| self.inputs.iter().find(|i| i.role == "disc"))
    }

    /// The files besides the copy the player gives: other discs, DLC, an update, keys.
    pub fn other_inputs(&self) -> impl Iterator<Item = &Input> {
        let main = self.main_input().map(|i| i.id.clone());
        self.inputs.iter().filter(move |i| Some(&i.id) != main.as_ref() && !i.fetched())
    }

    pub fn input(&self, id: &str) -> Option<&Input> {
        self.inputs.iter().find(|i| i.id == id)
    }

    /// The folders a named place points at, filled in for this game.
    pub fn folder(&self, name: &str, install: &Path) -> Vec<PathBuf> {
        let none = BTreeMap::new();
        let fill = Fill { install, inputs: &none };
        self.folders.get(name).into_iter().flatten().filter_map(|t| fill.text(t)).map(PathBuf::from).filter(|p| p.is_absolute()).collect()
    }

    /// The settings, read by the same rules as the catalog's.
    pub fn has_settings(&self) -> bool {
        self.raw.get("settings").and_then(Value::as_array).is_some_and(|s| !s.is_empty())
    }

    /// The message for an exit code: `Some(None)` for a normal close ("ok").
    pub fn exit_message(&self, code: i32) -> Option<Option<String>> {
        let text = self.exit_codes.get(&code.to_string()).or_else(|| self.exit_codes.get(&(code as u32).to_string()))?;
        Some((!text.eq_ignore_ascii_case("ok")).then(|| text.clone()))
    }
}

/// Where the files a recomp writes go: `{install}/recompdata`.
pub fn data_dir(install: &Path) -> PathBuf {
    install.join("recompdata")
}

/// The manifest beside a game: at the top of its folder, or beside its program.
pub fn read(install: &Path, exe: Option<&Path>) -> Option<Arc<Manifest>> {
    let mut places = vec![install.join(FILE)];
    if let Some(dir) = exe.and_then(Path::parent).filter(|d| *d != install) {
        places.push(dir.join(FILE));
    }
    places.into_iter().find_map(|p| cached(&p))
}

/// Read again only when the file changes: a library listing asks for every game's.
fn cached(path: &Path) -> Option<Arc<Manifest>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, (SystemTime, Option<Arc<Manifest>>)>>> = OnceLock::new();
    let modified = fs::metadata(path).and_then(|m| m.modified()).ok()?;
    let cache = CACHE.get_or_init(Default::default);
    if let Some((when, manifest)) = cache.lock().ok()?.get(path) {
        if *when == modified {
            return manifest.clone();
        }
    }
    // A manifest is a few KB; anything far bigger isn't one.
    let text = fs::read(path).ok().filter(|b| b.len() < 1024 * 1024).and_then(|b| String::from_utf8(b).ok());
    let manifest = text.as_deref().and_then(Manifest::parse).map(Arc::new);
    if let Ok(mut c) = cache.lock() {
        c.insert(path.to_path_buf(), (modified, manifest.clone()));
    }
    manifest
}

/// A path inside the game's folder, written relative to it: no `..`, no drive.
pub fn inside(path: &str) -> Option<PathBuf> {
    let path = path.trim().replace('\\', "/");
    let path = path.strip_prefix("{install}/").unwrap_or(&path);
    let p = Path::new(path);
    (!path.is_empty() && p.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir))).then(|| p.to_path_buf())
}

/// What the places in a manifest fill in with.
pub struct Fill<'a> {
    pub install: &'a Path,
    /// Input id -> the file the player picked.
    pub inputs: &'a BTreeMap<String, String>,
}

impl Fill<'_> {
    /// `{install}`, `{data}`, `{input:id}`, `{documents}`... filled in anywhere in
    /// the text. `None` when it names a file the player didn't pick: that whole
    /// argument is left out.
    pub fn text(&self, text: &str) -> Option<String> {
        let mut out = String::new();
        let mut rest = text;
        while let Some(open) = rest.find('{') {
            out.push_str(&rest[..open]);
            let after = &rest[open..];
            let Some(close) = after.find('}') else {
                out.push_str(after);
                return Some(out);
            };
            let token = &after[1..close];
            let value = match token {
                "install" => Some(self.install.display().to_string()),
                "data" => Some(data_dir(self.install).display().to_string()),
                "pad" => Some("xbox".to_string()),
                // Before named inputs, `{game}` was the player's copy.
                "game" => Some(self.inputs.get("game").filter(|p| !p.is_empty())?.clone()),
                t if t.starts_with("input:") => Some(self.inputs.get(&t[6..]).filter(|p| !p.is_empty())?.clone()),
                t => super::saves::resolve(&format!("{{{t}}}"), self.install).map(|p| p.display().to_string()),
            };
            match value {
                Some(v) => out.push_str(&v),
                None => out.push_str(&after[..=close]),
            }
            rest = &after[close + 1..];
        }
        out.push_str(rest);
        Some(out)
    }
}

/// Names the game may not set: the system's own, and ones that change which code loads.
fn settable(name: &str) -> bool {
    const SYSTEM: &[&str] = &[
        "PATH", "PATHEXT", "SYSTEMROOT", "SYSTEMDRIVE", "WINDIR", "COMSPEC", "TEMP", "TMP", "USERPROFILE", "APPDATA", "LOCALAPPDATA",
        "PROGRAMDATA", "PROGRAMFILES", "HOME", "USER", "SHELL", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "WINEPREFIX",
    ];
    let upper = name.to_ascii_uppercase();
    !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !SYSTEM.contains(&upper.as_str())
        && !upper.starts_with("LD_")
        && !upper.starts_with("DYLD_")
        && !upper.starts_with("RECOMPLAUNCHER")
}

/// How the game starts, as its manifest says.
#[derive(Debug, Default, PartialEq)]
pub struct Launch {
    /// The first of `start.program` that exists in the game's folder.
    pub program: Option<PathBuf>,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

pub fn launch(manifest: &Manifest, fill: &Fill) -> Launch {
    let Some(start) = &manifest.start else { return Launch::default() };
    let program = start.program.iter().filter_map(|p| inside(p)).map(|p| fill.install.join(p)).find(|p| p.is_file());
    let args = start.args.iter().filter_map(|a| fill.text(a)).collect();
    let mut env: Vec<(String, String)> = start.env.iter().filter(|(k, _)| settable(k)).filter_map(|(k, v)| Some((k.clone(), fill.text(v)?))).collect();
    // What every game started by a manifest-reading launcher is told.
    env.push(("RECOMPLAUNCHER".into(), "1".into()));
    env.push(("RECOMPLAUNCHER_MODE".into(), "desktop".into()));
    env.push(("RECOMPLAUNCHER_DATA".into(), data_dir(fill.install).display().to_string()));
    Launch { program, args, env }
}

/// The required files the player hasn't given yet (or that are gone since).
pub fn missing<'a>(manifest: &'a Manifest, install: &Path, inputs: &BTreeMap<String, String>) -> Vec<&'a Input> {
    manifest
        .inputs
        .iter()
        .filter(|i| !i.optional && !i.fetched())
        .filter(|i| {
            if let Some(target) = placed_at(i, install, None) {
                return !target.exists();
            }
            !inputs.get(&i.id).is_some_and(|p| Path::new(p).exists())
        })
        .collect()
}

/// Where an input with `into` goes in the game's folder (with `name`, or the picked file's own).
pub fn placed_at(input: &Input, install: &Path, picked: Option<&Path>) -> Option<PathBuf> {
    let dir = install.join(inside(input.into.as_deref()?).unwrap_or_default());
    let name = input.name.clone().or_else(|| picked?.file_name().map(|n| n.to_string_lossy().into_owned()));
    Some(match name.filter(|n| Path::new(n).components().count() == 1) {
        Some(n) => dir.join(n),
        None => dir,
    })
}

/// The folder has this relative path, in any letter case.
fn holds(dir: &Path, path: &str) -> bool {
    let mut at = dir.to_path_buf();
    for part in path.replace('\\', "/").split('/').filter(|p| !p.is_empty()) {
        let Some(found) = fs::read_dir(&at).ok().and_then(|e| e.flatten().find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))) else {
            return false;
        };
        at = found.path();
    }
    true
}

/// The title ID in a PlayStation `PARAM.SFO`.
fn sfo_title(bytes: &[u8]) -> Option<String> {
    let u32_at = |at: usize| -> Option<usize> { Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize) };
    let u16_at = |at: usize| -> Option<usize> { Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?) as usize) };
    if bytes.get(..4)? != b"\0PSF" {
        return None;
    }
    let (keys, data, count) = (u32_at(8)?, u32_at(12)?, u32_at(16)?);
    for i in 0..count.min(256) {
        let entry = 20 + i * 16;
        let key_at = keys + u16_at(entry)?;
        let key_end = bytes.get(key_at..)?.iter().position(|b| *b == 0)? + key_at;
        if &bytes[key_at..key_end] == b"TITLE_ID" {
            let (len, at) = (u32_at(entry + 4)?, data + u32_at(entry + 12)?);
            let raw = bytes.get(at..at + len)?;
            return Some(String::from_utf8_lossy(raw).trim_end_matches('\0').to_string());
        }
    }
    None
}

/// The IDs a copy carries, as plain ids (letters and digits).
pub fn ids_of(copy: &Path) -> Vec<String> {
    use crate::disc_console::plain_id;
    if copy.is_dir() {
        let sfo = ["PS3_GAME/PARAM.SFO", "PARAM.SFO", "sce_sys/param.sfo"].iter().find_map(|p| {
            let mut at = copy.to_path_buf();
            for part in p.split('/') {
                at = fs::read_dir(&at).ok()?.flatten().find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?.path();
            }
            sfo_title(&fs::read(at).ok()?)
        });
        return sfo.map(|t| vec![plain_id(&t)]).unwrap_or_default();
    }
    let mut ids = crate::disc_console::disc_ids(copy);
    if let Some(code) = crate::roms::n64_code_of(copy) {
        ids.push(plain_id(&format!("N{}{}", code.id, code.region)));
        ids.push(plain_id(&format!("{}{}", code.id, code.region)));
    }
    ids
}

fn sha256_of(path: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buffer).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Some(format!("{:x}", hasher.finalize()))
}

/// Whether a file or folder the player picked can be this input: the kind it
/// takes, its extension, and the checks the developer gave. A check Cerberus
/// can't read on this copy isn't held against it.
pub fn check(input: &Input, copy: &Path) -> Result<(), String> {
    if !copy.exists() {
        return Err(tr!("{} não existe mais.", copy.display()));
    }
    let label = input.label.trim().trim_end_matches('.');
    let is_dir = copy.is_dir();
    if is_dir && !input.takes_folder() {
        return Err(tr!("Esse item precisa de um arquivo, não de uma pasta: {}.", label));
    }
    if !is_dir && !input.takes_file() {
        return Err(tr!("Esse item precisa de uma pasta, não de um arquivo: {}.", label));
    }
    let ext = copy.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let allowed: Vec<String> = input.formats.iter().filter(|f| f.kind == "file").flat_map(|f| f.extensions.iter().map(|e| e.trim_start_matches('.').to_lowercase())).collect();
    // A CD kept as a cue sheet (or gdi/m3u) with its tracks counts as its disc image.
    let sheet = matches!(ext.as_str(), "cue" | "gdi" | "m3u") && allowed.iter().any(|a| a == "bin" || a == "iso");
    if !is_dir && !allowed.is_empty() && !allowed.contains(&ext) && !sheet {
        let wanted: Vec<String> = allowed.iter().map(|a| format!(".{a}")).collect();
        return Err(tr!("Esse arquivo não é {0}. Escolha: {1}.", wanted.join(" / "), label));
    }
    let checks = &input.check;
    if let Some(file) = checks.file.as_deref().filter(|_| is_dir) {
        if !holds(copy, file) {
            return Err(tr!("Essa pasta não tem {0}. Escolha: {1}.", file, label));
        }
    }
    if let Some(file) = &checks.iso9660 {
        let found = if is_dir { holds(copy, file) } else {
            let wanted = file.split(';').next().unwrap_or_default().to_ascii_uppercase();
            match crate::disc_console::root_files(copy) {
                Some(names) => names.iter().any(|n| *n == wanted),
                None => return Err(tr!("Esse arquivo não é uma imagem de disco. Escolha: {}.", label)),
            }
        };
        if !found {
            return Err(tr!("Esse disco não é o jogo certo (não tem {0}). Escolha: {1}.", file, label));
        }
    }
    if let Some(file) = &checks.xdvdfs {
        let found = if is_dir { holds(copy, file) } else {
            match crate::xdvdfs::root_files(copy) {
                Some(names) => names.iter().any(|n| n.eq_ignore_ascii_case(file)),
                None => return Err(tr!("Esse arquivo não é uma imagem de disco de Xbox. Escolha: {}.", label)),
            }
        };
        if !found {
            return Err(tr!("Esse disco não é o jogo certo (não tem {0}). Escolha: {1}.", file, label));
        }
    }
    if !checks.ids.is_empty() {
        let found = ids_of(copy);
        let wanted: Vec<String> = checks.ids.iter().map(|i| crate::disc_console::plain_id(i)).filter(|i| !i.is_empty()).collect();
        if !found.is_empty() && !found.iter().any(|f| wanted.iter().any(|w| f.starts_with(w.as_str()))) {
            return Err(tr!("Essa cópia é {0}, mas o jogo precisa de {1}.", found[0], checks.ids.join(tr!(" ou "))));
        }
    }
    if let (Some(name), false) = (&input.name, is_dir) {
        let picked = copy.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if input.into.is_some() && !picked.eq_ignore_ascii_case(name) {
            return Err(tr!("Esse arquivo precisa se chamar {0}. Escolha: {1}.", name, label));
        }
    }
    if let (Some(wanted), false) = (&input.sha256, is_dir) {
        if sha256_of(copy).is_none_or(|h| !h.eq_ignore_ascii_case(wanted.trim())) {
            return Err(tr!("Esse arquivo não é o esperado (o conteúdo é diferente). Escolha: {}.", label));
        }
    }
    Ok(())
}

/// Puts an input with `into` in the game's folder. Others stay where they are
/// and are passed by their path.
pub fn place(input: &Input, install: &Path, copy: &Path) -> Result<(), String> {
    let Some(target) = placed_at(input, install, Some(copy)) else { return Ok(()) };
    if copy.is_dir() {
        return super::copy_tree(copy, &target).map_err(|e| tr!("Não foi possível copiar os arquivos: {}", e));
    }
    if let Some(dir) = target.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    crate::disk::require(target.parent().unwrap_or(install), fs::metadata(copy).map_err(|e| e.to_string())?.len())?;
    fs::copy(copy, &target).map(|_| ()).map_err(|e| tr!("Não foi possível copiar o arquivo: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    // The SDK's own examples (MIT, RecompLauncher-SDK/examples).
    const STREETBALL: &str = include_str!("../../tests/sdk/streetball.json");
    const FIFA: &str = include_str!("../../tests/sdk/fifawc14.json");
    const MULTIDISC: &str = include_str!("../../tests/sdk/multidisc-inputs.json");

    #[test]
    fn the_sdk_examples_read() {
        let street = Manifest::parse(STREETBALL).unwrap();
        let game = street.main_input().unwrap();
        assert_eq!((game.id.as_str(), game.check.iso9660.as_deref()), ("game", Some("SLUS_206.51")));
        assert_eq!(street.start.as_ref().unwrap().args, ["--iso", "{input:game}"]);
        assert_eq!(street.saves, ["{documents}/Streetball Revival"], "from folders.saves");
        assert!(street.has_settings());
        assert_eq!(street.actions.len(), 1);

        let fifa = Manifest::parse(FIFA).unwrap();
        assert!(fifa.has_settings());
        assert!(fifa.main_input().is_some());

        let discs = Manifest::parse(MULTIDISC).unwrap();
        assert_eq!(discs.main_input().unwrap().id, "disc1", "no role game: the first disc is the copy");
        let others: Vec<&str> = discs.other_inputs().map(|i| i.id.as_str()).collect();
        assert_eq!(others, ["disc2", "disc3", "disc4", "dlc1", "dlc2", "dlc3"]);
    }

    #[test]
    fn arguments_fill_in_and_unpicked_files_drop_their_argument() {
        let manifest = Manifest::parse(r#"{"format":1,"start":{"program":["bin/game.exe","game.exe"],
            "args":["--disc1={input:disc1}","--dlc1={input:dlc1}","--data","{data}"],
            "env":{"MY_DATA":"{install}/x","PATH":"C:/evil","LD_PRELOAD":"x.so"}}}"#).unwrap();
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("game.exe"), b"MZ").unwrap();
        let inputs = BTreeMap::from([("disc1".to_string(), "D:/a.iso".to_string())]);
        let launch = launch(&manifest, &Fill { install: dir.path(), inputs: &inputs });
        assert_eq!(launch.program, Some(dir.path().join("game.exe")), "the first program that exists");
        assert_eq!(launch.args[0], "--disc1=D:/a.iso");
        assert_eq!(launch.args.len(), 3, "the DLC the player skipped leaves its argument out");
        assert_eq!(launch.args[2], data_dir(dir.path()).display().to_string());
        let names: Vec<&str> = launch.env.iter().map(|(k, _)| k.as_str()).collect();
        assert!(names.contains(&"MY_DATA") && !names.contains(&"PATH") && !names.contains(&"LD_PRELOAD"));
        assert!(names.contains(&"RECOMPLAUNCHER_DATA"));
    }

    #[test]
    fn programs_outside_the_game_folder_never_run() {
        assert_eq!(inside("../x.exe"), None);
        assert_eq!(inside("C:/Windows/x.exe"), None);
        assert_eq!(inside("{install}/bin/x.exe"), Some(PathBuf::from("bin/x.exe")));
    }

    #[test]
    fn required_files_missing_and_optional_ones_not() {
        let discs = Manifest::parse(MULTIDISC).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let iso = dir.path().join("d.iso");
        fs::write(&iso, b"x").unwrap();
        let mut picked = BTreeMap::new();
        assert_eq!(missing(&discs, dir.path(), &picked).len(), 4, "four discs, no DLC");
        for d in ["disc1", "disc2", "disc3", "disc4"] {
            picked.insert(d.to_string(), iso.display().to_string());
        }
        assert!(missing(&discs, dir.path(), &picked).is_empty());
    }

    #[test]
    fn a_copy_is_checked_as_the_developer_says() {
        let dir = tempfile::tempdir().unwrap();
        let input = |json: &str| serde_json::from_str::<Input>(json).unwrap();
        let folder = dir.path().join("game");
        fs::create_dir_all(folder.join("Media")).unwrap();
        fs::write(folder.join("Media/Default.XEX"), b"x").unwrap();
        let by_file = input(r#"{"id":"game","label":"Your folder","formats":[{"type":"folder"}],"check":{"file":"media/default.xex"}}"#);
        assert!(check(&by_file, &folder).is_ok(), "any letter case");
        let other = dir.path().join("other");
        fs::create_dir_all(&other).unwrap();
        assert!(check(&by_file, &other).is_err());
        let file = dir.path().join("a.zip");
        fs::write(&file, b"x").unwrap();
        assert!(check(&by_file, &file).is_err(), "a folder is wanted");
        let iso_only = input(r#"{"id":"game","label":"Disc","formats":[{"type":"file","extensions":["iso"]}]}"#);
        assert!(check(&iso_only, &file).is_err(), "the wrong extension");
        let named = input(r#"{"id":"update","label":"TU","role":"update","into":"update","name":"tu.bin"}"#);
        assert!(check(&named, &file).is_err(), "an update under another name");
        let target = placed_at(&named, dir.path(), Some(&file)).unwrap();
        assert_eq!(target, dir.path().join("update").join("tu.bin"));
    }

    #[test]
    fn a_ps3_folder_tells_its_title_id() {
        // PSF header: 1 entry, key table at 36, data at 48.
        let mut sfo = b"\0PSF\x01\x01\0\0".to_vec();
        sfo.extend(36u32.to_le_bytes());
        sfo.extend(48u32.to_le_bytes());
        sfo.extend(1u32.to_le_bytes());
        sfo.extend([0, 0, 4, 2]);
        sfo.extend(10u32.to_le_bytes());
        sfo.extend(16u32.to_le_bytes());
        sfo.extend(0u32.to_le_bytes());
        sfo.extend(b"TITLE_ID\0\0\0\0");
        sfo.extend(b"BLUS30443\0");
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("PS3_GAME")).unwrap();
        fs::write(dir.path().join("PS3_GAME/PARAM.SFO"), &sfo).unwrap();
        assert_eq!(ids_of(dir.path()), ["BLUS30443"]);
        let input: Input = serde_json::from_str(r#"{"id":"game","label":"x","formats":[{"type":"folder"}],"check":{"ids":["BLUS-30443"]}}"#).unwrap();
        assert!(check(&input, dir.path()).is_ok());
        let wrong: Input = serde_json::from_str(r#"{"id":"game","label":"x","formats":[{"type":"folder"}],"check":{"ids":["BLES00932"]}}"#).unwrap();
        assert!(check(&wrong, dir.path()).unwrap_err().contains("BLUS30443"));
    }

    #[test]
    fn exit_codes_and_the_old_game_field() {
        let m = Manifest::parse(r#"{"format":1,"exit_codes":{"1":"ok","252968961":"Pick your copy again."},
            "game":{"kind":"file","label":"Your ISO","check":{"iso9660":"SLUS_200.00"}}}"#).unwrap();
        assert_eq!(m.exit_message(1), Some(None));
        assert_eq!(m.exit_message(252968961), Some(Some("Pick your copy again.".into())));
        assert_eq!(m.exit_message(2), None);
        assert_eq!(m.main_input().unwrap().check.iso9660.as_deref(), Some("SLUS_200.00"));
        assert!(Manifest::parse(r#"{"format":2}"#).is_none(), "a format this launcher doesn't know");
    }
}
