//! "Adicionar à Steam", as RecompLauncher and Hydra offer: the game becomes a
//! non-Steam game in the player's Steam library, with its art, so it starts
//! from Steam's Big Picture or a Steam Deck. Steam starts Cerberus with
//! `--play <id>`, as the desktop shortcut does, so the session is tracked as usual.
//!
//! Steam keeps these games in `userdata/<account>/config/shortcuts.vdf`, a
//! binary key-value file, and their pictures in `config/grid`, named after a
//! number made from the program and the name. Steam rewrites the file when it
//! closes, so it must be closed while Cerberus changes it; the file is backed
//! up first, and any part Cerberus doesn't understand stops the change.

use cerberus_shared::tr;
use std::fs;
use std::path::{Path, PathBuf};

/// A value in Steam's binary key-value format.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Map(Vec<(String, Node)>),
    Text(String),
    /// A text that isn't UTF-8 (another program's name, written by hand or an
    /// old tool): written back byte for byte, never "fixed".
    Raw(Vec<u8>),
    Int(u32),
}

const MAP: u8 = 0x00;
const TEXT: u8 = 0x01;
const INT: u8 = 0x02;
const END: u8 = 0x08;

/// The tag Cerberus's games carry in Steam (it shows as a collection there).
pub const TAG: &str = "Cerberus";

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn byte(&mut self) -> Result<u8, String> {
        let b = *self.bytes.get(self.at).ok_or(tr!("shortcuts.vdf terminou no meio."))?;
        self.at += 1;
        Ok(b)
    }

    fn text(&mut self) -> Result<String, String> {
        Ok(String::from_utf8_lossy(&self.raw()?).into_owned())
    }

    fn raw(&mut self) -> Result<Vec<u8>, String> {
        let rest = &self.bytes[self.at..];
        let end = rest.iter().position(|&b| b == 0).ok_or(tr!("shortcuts.vdf tem um texto sem fim."))?;
        self.at += end + 1;
        Ok(rest[..end].to_vec())
    }

    fn map(&mut self) -> Result<Vec<(String, Node)>, String> {
        let mut items = Vec::new();
        loop {
            let kind = self.byte()?;
            if kind == END {
                return Ok(items);
            }
            let key = self.text()?;
            let value = match kind {
                MAP => Node::Map(self.map()?),
                TEXT => {
                    let raw = self.raw()?;
                    match String::from_utf8(raw) {
                        Ok(text) => Node::Text(text),
                        Err(e) => Node::Raw(e.into_bytes()),
                    }
                }
                INT => {
                    let end = self.at + 4;
                    let raw = self.bytes.get(self.at..end).ok_or(tr!("shortcuts.vdf terminou no meio."))?;
                    self.at = end;
                    Node::Int(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
                }
                // Rewriting a part not understood could lose it: leave the file alone.
                other => return Err(tr!("shortcuts.vdf tem um tipo de valor desconhecido ({}); nada foi mudado.", other)),
            };
            items.push((key, value));
        }
    }
}

/// The file's top level: `shortcuts` -> entries "0", "1"...
pub fn parse(bytes: &[u8]) -> Result<Vec<(String, Node)>, String> {
    let mut reader = Reader { bytes, at: 0 };
    let top = reader.map()?;
    Ok(top)
}

fn write_map(items: &[(String, Node)], out: &mut Vec<u8>) {
    for (key, value) in items {
        let kind = match value {
            Node::Map(_) => MAP,
            Node::Text(_) | Node::Raw(_) => TEXT,
            Node::Int(_) => INT,
        };
        out.push(kind);
        out.extend_from_slice(key.as_bytes());
        out.push(0);
        match value {
            Node::Map(inner) => write_map(inner, out),
            Node::Text(text) => {
                out.extend(text.bytes().filter(|&b| b != 0));
                out.push(0);
            }
            Node::Raw(bytes) => {
                out.extend(bytes.iter().filter(|&&b| b != 0));
                out.push(0);
            }
            Node::Int(n) => out.extend_from_slice(&n.to_le_bytes()),
        }
    }
    out.push(END);
}

pub fn write(top: &[(String, Node)]) -> Vec<u8> {
    let mut out = Vec::new();
    write_map(top, &mut out);
    out
}

/// The number Steam gives a non-Steam game, from its program and name; its
/// pictures in `config/grid` are named after it.
pub fn app_id(exe: &str, name: &str) -> u32 {
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(exe.as_bytes());
    hasher.update(name.as_bytes());
    hasher.finalize() | 0x8000_0000
}

/// What Cerberus puts in Steam for one game.
#[derive(Clone)]
pub struct Shortcut {
    pub name: String,
    /// Cerberus's own program.
    pub launcher: PathBuf,
    pub game_id: String,
    pub icon: Option<PathBuf>,
}

impl Shortcut {
    fn exe(&self) -> String {
        format!("\"{}\"", self.launcher.display())
    }

    fn options(&self) -> String {
        format!("--play \"{}\"", self.game_id.replace('"', ""))
    }

    pub fn app_id(&self) -> u32 {
        app_id(&self.exe(), &self.name)
    }

    fn entry(&self) -> Node {
        let text = |k: &str, v: String| (k.to_string(), Node::Text(v));
        let int = |k: &str, v: u32| (k.to_string(), Node::Int(v));
        let start_dir = self.launcher.parent().map(|d| format!("\"{}\"", d.display())).unwrap_or_default();
        Node::Map(vec![
            int("appid", self.app_id()),
            text("AppName", self.name.clone()),
            text("Exe", self.exe()),
            text("StartDir", start_dir),
            text("icon", self.icon.as_ref().map(|i| i.display().to_string()).unwrap_or_default()),
            text("ShortcutPath", String::new()),
            text("LaunchOptions", self.options()),
            int("IsHidden", 0),
            int("AllowDesktopConfig", 1),
            int("AllowOverlay", 1),
            int("OpenVR", 0),
            int("Devkit", 0),
            text("DevkitGameID", String::new()),
            int("DevkitOverrideAppID", 0),
            int("LastPlayTime", 0),
            text("FlatpakAppID", String::new()),
            ("tags".to_string(), Node::Map(vec![("0".to_string(), Node::Text(TAG.to_string()))])),
        ])
    }
}

fn field<'a>(entry: &'a Node, key: &str) -> Option<&'a Node> {
    let Node::Map(items) = entry else { return None };
    items.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v)
}

fn text_of<'a>(entry: &'a Node, key: &str) -> &'a str {
    match field(entry, key) {
        Some(Node::Text(t)) => t,
        _ => "",
    }
}

/// An entry Cerberus made for this game: its options start this game.
fn is_ours(entry: &Node, game_id: &str) -> bool {
    let options = text_of(entry, "LaunchOptions");
    options == format!("--play \"{}\"", game_id.replace('"', "")) && text_of(entry, "Exe").to_lowercase().contains("cerberus")
}

fn entries_mut(top: &mut Vec<(String, Node)>) -> &mut Vec<(String, Node)> {
    if !top.iter().any(|(k, _)| k.eq_ignore_ascii_case("shortcuts")) {
        top.push(("shortcuts".into(), Node::Map(Vec::new())));
    }
    let (_, node) = top.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case("shortcuts")).expect("added above");
    if !matches!(node, Node::Map(_)) {
        *node = Node::Map(Vec::new());
    }
    let Node::Map(entries) = node else { unreachable!() };
    entries
}

/// Steam numbers the entries "0", "1"... in order.
fn renumber(entries: &mut [(String, Node)]) {
    for (index, (key, _)) in entries.iter_mut().enumerate() {
        *key = index.to_string();
    }
}

/// Adds the game, or replaces Cerberus's earlier entry for it. Returns the
/// numbers the earlier entry had that the new one doesn't (renamed: Steam's
/// number changes), whose pictures would be left behind.
pub fn add(top: &mut Vec<(String, Node)>, shortcut: &Shortcut) -> Vec<u32> {
    let gone: Vec<u32> = remove(top, &shortcut.game_id).into_iter().filter(|n| *n != shortcut.app_id()).collect();
    let entries = entries_mut(top);
    entries.push((String::new(), shortcut.entry()));
    renumber(entries);
    gone
}

/// Takes the game out; returns the numbers of the entries removed.
pub fn remove(top: &mut Vec<(String, Node)>, game_id: &str) -> Vec<u32> {
    let entries = entries_mut(top);
    let gone: Vec<u32> = entries
        .iter()
        .filter(|(_, e)| is_ours(e, game_id))
        .filter_map(|(_, e)| match field(e, "appid") {
            Some(Node::Int(n)) => Some(*n),
            _ => None,
        })
        .collect();
    entries.retain(|(_, e)| !is_ours(e, game_id));
    renumber(entries);
    gone
}

pub fn contains(top: &[(String, Node)], game_id: &str) -> bool {
    let Some((_, Node::Map(entries))) = top.iter().find(|(k, _)| k.eq_ignore_ascii_case("shortcuts")) else {
        return false;
    };
    entries.iter().any(|(_, e)| is_ours(e, game_id))
}

/// The Steam accounts' config folders: the one last signed in, or every one
/// when Steam doesn't say.
pub fn config_dirs(steam: &Path) -> Vec<PathBuf> {
    let users: Vec<PathBuf> = fs::read_dir(steam.join("userdata"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().chars().all(|c| c.is_ascii_digit()) && e.file_name() != "0")
        .map(|e| e.path().join("config"))
        .collect();
    let recent = fs::read_to_string(steam.join("config").join("loginusers.vdf")).ok().and_then(|text| most_recent_account(&text));
    match recent.map(|account| steam.join("userdata").join(account.to_string()).join("config")) {
        Some(dir) if users.contains(&dir) => vec![dir],
        _ => users,
    }
}

/// The account number (SteamID64 minus its base) marked `"MostRecent" "1"` in loginusers.vdf.
fn most_recent_account(text: &str) -> Option<u64> {
    let mut current: Option<u64> = None;
    for line in text.lines() {
        let parts: Vec<&str> = line.split('"').filter(|p| !p.trim().is_empty()).collect();
        match parts.as_slice() {
            [id] if id.len() == 17 && id.chars().all(|c| c.is_ascii_digit()) => current = id.parse().ok(),
            [key, value] if key.eq_ignore_ascii_case("MostRecent") && *value == "1" => {
                return current.and_then(|id| id.checked_sub(76_561_197_960_265_728));
            }
            _ => {}
        }
    }
    None
}

/// Reads an account's list (empty when it has none yet).
pub fn load(config: &Path) -> Result<Vec<(String, Node)>, String> {
    match fs::read(config.join("shortcuts.vdf")) {
        Ok(bytes) if !bytes.is_empty() => parse(&bytes),
        Ok(_) => Ok(Vec::new()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(tr!("Não foi possível ler a lista da Steam: {}", e)),
    }
}

/// Writes the list back, keeping the previous file as `shortcuts.vdf.cerberus.bak`.
pub fn save(config: &Path, top: &[(String, Node)]) -> Result<(), String> {
    let file = config.join("shortcuts.vdf");
    fs::create_dir_all(config).map_err(|e| e.to_string())?;
    // The list from before a batch ("Adicionar todos" saves once per game):
    // a copy made in the last ten minutes already holds it, and is kept.
    // The list as it was before Cerberus ever changed it, made once and kept.
    let original = config.join("shortcuts.vdf.cerberus-original.bak");
    if file.is_file() && !original.exists() {
        fs::copy(&file, &original).map_err(|e| tr!("Não foi possível guardar uma cópia da lista da Steam: {}", e))?;
    }
    let backup = config.join("shortcuts.vdf.cerberus.bak");
    let recent = fs::metadata(&backup).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age < std::time::Duration::from_secs(600));
    if file.is_file() && !recent {
        fs::copy(&file, &backup).map_err(|e| tr!("Não foi possível guardar uma cópia da lista da Steam: {}", e))?;
    }
    let temp = config.join("shortcuts.vdf.cerberus.tmp");
    fs::write(&temp, write(top)).map_err(|e| e.to_string())?;
    fs::rename(&temp, &file).map_err(|e| e.to_string())
}

/// The picture's kind from its first bytes: Steam reads PNG and JPEG.
fn picture_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG") {
        Some("png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else {
        None
    }
}

/// Copies the game's art into `config/grid`: the cover as the library's
/// tall picture, the backdrop as its hero; returns how many were copied.
pub fn place_art(config: &Path, app_id: u32, cover: Option<&[u8]>, hero: Option<&[u8]>) -> usize {
    let grid = config.join("grid");
    let mut placed = 0;
    for (source, suffix) in [(cover, "p"), (hero, "_hero")] {
        let Some(bytes) = source else { continue };
        let Some(ext) = picture_extension(bytes) else { continue };
        if fs::create_dir_all(&grid).is_ok() && fs::write(grid.join(format!("{app_id}{suffix}.{ext}")), &bytes).is_ok() {
            placed += 1;
        }
    }
    placed
}

/// Takes the game's pictures out of `config/grid`.
pub fn remove_art(config: &Path, app_id: u32) {
    for suffix in ["p", "_hero", "_logo", ""] {
        for ext in ["png", "jpg"] {
            let _ = fs::remove_file(config.join("grid").join(format!("{app_id}{suffix}.{ext}")));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This PC's real lists, read only: read and written again, the same bytes.
    /// `cargo test -p cerberus-launcher real_shortcuts_round_trip -- --ignored --nocapture`
    #[test]
    #[ignore = "reads this PC's Steam"]
    fn real_shortcuts_round_trip() {
        let steam = PathBuf::from(r"C:\Program Files (x86)\Steam");
        for config in config_dirs(&steam) {
            let Ok(bytes) = fs::read(config.join("shortcuts.vdf")) else { continue };
            let top = parse(&bytes).unwrap();
            assert_eq!(write(&top), bytes, "{}", config.display());
            println!("{}: {} bytes, the same after", config.display(), bytes.len());
        }
    }

    #[test]
    fn another_programs_name_outside_utf8_keeps_its_bytes() {
        // An entry named "Café" in Latin-1 (0xE9), as an old tool writes it.
        let mut bytes = vec![MAP];
        bytes.extend_from_slice(b"shortcuts\0");
        bytes.push(MAP);
        bytes.extend_from_slice(b"0\0");
        bytes.push(TEXT);
        bytes.extend_from_slice(b"AppName\0Caf\xe9\0");
        bytes.extend_from_slice(&[END, END, END]);
        let mut top = parse(&bytes).unwrap();
        assert_eq!(write(&top), bytes, "read and written unchanged");
        add(&mut top, &shortcut("local:x"));
        let after = write(&top);
        assert!(after.windows(5).any(|w| w == b"Caf\xe9\0"), "the other name's bytes stay");
    }

    #[test]
    fn readding_a_renamed_game_says_which_old_number_to_clear() {
        let mut top = Vec::new();
        let first = shortcut("local:x");
        assert!(add(&mut top, &first).is_empty());
        let renamed = Shortcut { name: "Another name".into(), ..first.clone() };
        assert_eq!(add(&mut top, &renamed), vec![first.app_id()]);
        assert!(add(&mut top, &renamed).is_empty(), "same name again: same number");
    }

    fn shortcut(id: &str) -> Shortcut {
        Shortcut { name: "Zelda 64: Recompiled".into(), launcher: PathBuf::from(r"C:\Apps\Cerberus\cerberus-launcher.exe"), game_id: id.into(), icon: None }
    }

    #[test]
    fn reads_back_what_it_writes_and_keeps_other_games() {
        // Someone else's game, as Steam writes it.
        let mut top = vec![(
            "shortcuts".to_string(),
            Node::Map(vec![("0".into(), Node::Map(vec![("appid".into(), Node::Int(123)), ("AppName".into(), Node::Text("Other".into())), ("Exe".into(), Node::Text("\"C:\\other.exe\"".into()))]))]),
        )];
        add(&mut top, &shortcut("recomp-zelda"));
        add(&mut top, &shortcut("recomp-zelda"));
        let bytes = write(&top);
        let read = parse(&bytes).unwrap();
        assert_eq!(read, top);
        let Node::Map(entries) = &read[0].1 else { panic!() };
        assert_eq!(entries.len(), 2, "added once, the other game kept");
        assert_eq!(entries[1].0, "1");
        assert!(contains(&read, "recomp-zelda"));
        assert_eq!(text_of(&entries[1].1, "LaunchOptions"), "--play \"recomp-zelda\"");

        let gone = remove(&mut top, "recomp-zelda");
        assert_eq!(gone, vec![shortcut("recomp-zelda").app_id()]);
        assert!(!contains(&top, "recomp-zelda"));
        let Node::Map(entries) = &top[0].1 else { panic!() };
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn makes_steams_number_for_a_non_steam_game() {
        // crc32("\"C:\\a.exe\"" + "A") with the top bit set, as Steam computes it.
        let id = app_id("\"C:\\a.exe\"", "A");
        assert!(id & 0x8000_0000 != 0);
        assert_eq!(id, crc32fast::hash(b"\"C:\\a.exe\"A") | 0x8000_0000);
    }

    #[test]
    fn leaves_a_file_it_does_not_understand_alone() {
        let mut bytes = vec![MAP];
        bytes.extend_from_slice(b"shortcuts\0");
        bytes.push(0x07);
        bytes.extend_from_slice(b"x\0");
        assert!(parse(&bytes).is_err());
        assert!(parse(b"\x00shortcuts\0\x01name").is_err(), "cut short");
    }

    #[test]
    fn finds_the_account_last_signed_in() {
        let text = r#""users"
{
	"76561198000000001"
	{
		"AccountName"		"a"
		"MostRecent"		"0"
	}
	"76561198176472402"
	{
		"AccountName"		"b"
		"MostRecent"		"1"
	}
}"#;
        assert_eq!(most_recent_account(text), Some(76_561_198_176_472_402 - 76_561_197_960_265_728));
    }

    #[test]
    fn copies_png_and_jpeg_art_only() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("config");
        assert_eq!(place_art(&config, 42, Some(b"\x89PNG...."), Some(b"RIFF....WEBP")), 1);
        assert!(config.join("grid").join("42p.png").is_file());
        remove_art(&config, 42);
        assert!(!config.join("grid").join("42p.png").exists());
    }
}
