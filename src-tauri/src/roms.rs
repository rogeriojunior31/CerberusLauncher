//! The player's own games in their ROM and backup folders, and which recomps
//! they fit. Nothing is read beyond a file's first bytes, and nothing leaves the PC.
//!
//! N64 cartridges carry the game's name in their header, so those are matched
//! on it whatever the file is called; other cartridges and disc images are
//! matched on their file name.

use cerberus_shared::tr;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::recomp::CatalogEntry;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RomFile {
    pub path: PathBuf,
    /// `n64`, `snes`, `gba`... for cartridges; empty for disc images, whose
    /// console the file type doesn't say.
    pub system: String,
    /// The name in the N64 header, else the file's name.
    pub title: String,
    /// US, Europe, Japan, or empty.
    pub region: String,
    /// For N64 cartridges, the exact version the header names.
    #[serde(default)]
    pub n64: Option<N64Code>,
    /// A game on several discs: every disc, in order (`path` is the first, or
    /// the player's own .m3u list).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub discs: Vec<PathBuf>,
}

/// Which N64 game and version a cartridge is: its two-letter id (`KT` for
/// Mario Kart 64), its region letter (`E` USA, `P` Europe, `J` Japan...) and
/// its revision (0 for the first release, 1 for "Rev 1"...).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct N64Code {
    pub id: String,
    pub region: char,
    pub revision: u8,
}

/// What a recomp's catalog entry says it was built against.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct N64Wanted {
    pub id: String,
    /// The region letters it takes, e.g. `"EP"`; empty for any.
    pub regions: String,
    /// The revisions it takes; empty for any.
    pub revisions: Vec<u8>,
}

/// The region letter as people say it.
fn region_name(code: char) -> &'static str {
    match code {
        'E' | 'N' => "americana (EUA)",
        'J' => "japonesa",
        'P' | 'X' | 'Y' => "europeia",
        'D' => tr!("alemã"),
        'F' => "francesa",
        'I' => "italiana",
        'S' => "espanhola",
        'U' => "australiana",
        _ => "desconhecida",
    }
}

/// Why this cartridge isn't the version the recomp needs, if it isn't.
pub fn version_problem(code: &N64Code, wanted: &N64Wanted) -> Option<String> {
    if !wanted.id.is_empty() && !code.id.eq_ignore_ascii_case(&wanted.id) {
        return Some(tr!("Essa ROM é de outro jogo (código {}, esperado {}).", code.id, wanted.id));
    }
    if !wanted.regions.is_empty() && !wanted.regions.contains(code.region) {
        let wanted_names: Vec<&str> = wanted.regions.chars().map(region_name).collect();
        return Some(tr!("Essa ROM é a versão {}; esse recomp precisa da versão {}.", region_name(code.region), wanted_names.join(tr!(" ou "))));
    }
    if !wanted.revisions.is_empty() && !wanted.revisions.contains(&code.revision) {
        let list: Vec<String> = wanted.revisions.iter().map(|r| format!("1.{r}")).collect();
        return Some(tr!("Essa ROM é a revisão 1.{}; esse recomp precisa da {}.", code.revision, list.join(tr!(" ou "))));
    }
    None
}

/// The N64 version of a file, if it's an N64 cartridge.
pub fn n64_code_of(path: &Path) -> Option<N64Code> {
    n64_header(path).map(|header| header.code)
}

struct N64Header {
    name: String,
    region: String,
    code: N64Code,
}

/// Entries looked at, at most, in one scan, so a whole drive picked by mistake
/// still finishes.
const MOST_ENTRIES: usize = 300_000;
const N64: &[&str] = &["z64", "n64", "v64"];
const DISC: &[&str] = &["iso", "chd", "cue", "gdi", "cdi", "rvz", "wbfs", "gcm", "cso", "wia", "xex", "pkg", "m3u"];

fn cartridge_system(ext: &str) -> Option<&'static str> {
    let system = match ext {
        "nes" | "fds" | "unf" => "nes",
        "sfc" | "smc" => "snes",
        "gb" => "gb",
        "gbc" => "gbc",
        "gba" => "gba",
        "nds" => "ds",
        "3ds" | "cci" | "cia" => "3ds",
        "md" | "gen" | "smd" => "genesis",
        "32x" => "32x",
        "gg" => "gg",
        "sms" => "sms",
        "xci" | "nsp" => "switch",
        _ => return None,
    };
    Some(system)
}

/// Consoles whose games come on discs; a disc image may be any of them.
const DISC_CONSOLES: &[&str] = &[
    "ps1", "ps2", "ps3", "ps4", "psp", "gc", "wii", "wiiu", "xbox", "x360", "dc", "saturn", "segacd", "pce", "3do", "cdi",
];

/// The name, region and version in an N64 cartridge's header, in any of the
/// three byte orders dumps come in.
fn n64_header(path: &Path) -> Option<N64Header> {
    let mut file = fs::File::open(path).ok()?;
    let mut head = [0u8; 64];
    file.read_exact(&mut head).ok()?;
    match head[..4] {
        [0x80, 0x37, 0x12, 0x40] => {}
        // Byte-swapped (.v64): every pair is reversed.
        [0x37, 0x80, 0x40, 0x12] => head.chunks_mut(2).for_each(|pair| pair.swap(0, 1)),
        // Little-endian (.n64): every word of four is reversed.
        [0x40, 0x12, 0x37, 0x80] => head.chunks_mut(4).for_each(<[u8]>::reverse),
        _ => return None,
    }
    let name: String = head[0x20..0x34].iter().map(|&b| if b.is_ascii_graphic() || b == b' ' { b as char } else { ' ' }).collect();
    let region = match head[0x3E] {
        b'E' | b'N' => "US",
        b'J' => "Japan",
        b'P' | b'D' | b'F' | b'I' | b'S' | b'U' | b'X' | b'Y' => "Europe",
        _ => "",
    };
    // 0x3B is the media letter, then the game's two-letter id, its region and revision.
    let id: String = head[0x3C..0x3E].iter().map(|&b| b as char).collect();
    let code = N64Code { id, region: head[0x3E] as char, revision: head[0x3F] };
    Some(N64Header { name: name.trim().to_string(), region: region.to_string(), code })
}

fn region_from_name(name: &str) -> String {
    let lower = name.to_lowercase();
    let region = if lower.contains("(usa") || lower.contains("(us)") || lower.contains("(u)") {
        "US"
    } else if lower.contains("(europe") || lower.contains("(e)") {
        "Europe"
    } else if lower.contains("(japan") || lower.contains("(j)") {
        "Japan"
    } else {
        ""
    };
    region.to_string()
}

fn identify(path: &Path) -> Option<RomFile> {
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    if N64.contains(&ext.as_str()) {
        let header = n64_header(path)?;
        let region = if header.region.is_empty() { region_from_name(&stem) } else { header.region };
        return Some(RomFile { path: path.to_path_buf(), system: "n64".into(), title: header.name, region, n64: Some(header.code), discs: Vec::new() });
    }
    let system = match cartridge_system(&ext) {
        Some(system) => system.to_string(),
        // A list of discs is its first disc's console.
        None if ext == "m3u" => listed_discs(path).first().and_then(|disc| crate::disc_console::console_of(disc)).or_else(|| crate::disc_console::console_of(path)).unwrap_or_default().to_string(),
        // A disc says which console it's from (or its folder does).
        None if DISC.contains(&ext.as_str()) => crate::disc_console::console_of(path).unwrap_or_default().to_string(),
        None => return None,
    };
    Some(RomFile { path: path.to_path_buf(), system, region: region_from_name(&stem), title: stem, n64: None, discs: Vec::new() })
}

/// "(Disc 2)", "(Disk 2 of 3)", "(CD2)": the disc's number, and the name without the tag.
fn disc_tag(stem: &str) -> Option<(u32, String)> {
    // ASCII-only lowercase keeps every byte where it was, so the positions found
    // here cut `stem` at the same places ("İ" would not, and could panic).
    let lower = stem.to_ascii_lowercase();
    for word in ["(disc", "(disk", "(cd"] {
        let Some(start) = lower.find(word) else { continue };
        let rest = &lower[start + word.len()..];
        let end = rest.find(')')?;
        let inside = rest[..end].trim();
        let number: String = inside.chars().take_while(char::is_ascii_digit).collect();
        let tail = inside[number.len()..].trim();
        if number.is_empty() || !(tail.is_empty() || tail.starts_with("of ")) {
            continue;
        }
        let cut = start + word.len() + end + 1;
        let base = format!("{}{}", &stem[..start], &stem[cut..]).split_whitespace().collect::<Vec<_>>().join(" ");
        return Some((number.parse().ok()?, base));
    }
    None
}

fn is_list(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("m3u"))
}

/// The discs an .m3u lists, beside it or where it says; only files that exist.
fn listed_discs(list: &Path) -> Vec<PathBuf> {
    let Ok(file) = fs::File::open(list) else { return Vec::new() };
    let mut text = String::new();
    // A disc list is a few lines; a bigger file isn't one.
    if file.take(64 * 1024).read_to_string(&mut text).is_err() {
        return Vec::new();
    }
    let dir = list.parent().unwrap_or(Path::new("."));
    text.trim_start_matches('\u{feff}')
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| dir.join(l))
        .filter(|p| p.is_file() && !is_list(p))
        .take(16)
        .collect()
}

fn same_file(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().replace('/', "\\").eq_ignore_ascii_case(&b.to_string_lossy().replace('/', "\\"))
}

/// A game on several discs shows once: the player's .m3u list when there is
/// one (its discs no longer show apart), else its "(Disc 1)" file, holding
/// the others from the same folder.
pub fn join_discs(found: Vec<RomFile>) -> Vec<RomFile> {
    let mut lists: Vec<RomFile> = Vec::new();
    let mut rest: Vec<RomFile> = Vec::new();
    for mut rom in found {
        if is_list(&rom.path) {
            rom.discs = listed_discs(&rom.path);
            if !rom.discs.is_empty() {
                lists.push(rom);
            }
        } else {
            rest.push(rom);
        }
    }
    rest.retain(|rom| !lists.iter().any(|l| l.discs.iter().any(|d| same_file(d, &rom.path))));
    // Discs of one game: same folder, same name without the tag, same file type.
    let key = |rom: &RomFile| {
        let stem = rom.path.file_stem()?.to_string_lossy().into_owned();
        let (number, base) = disc_tag(&stem)?;
        let ext = rom.path.extension()?.to_string_lossy().to_lowercase();
        let dir = rom.path.parent()?.to_string_lossy().to_lowercase();
        Some(((dir, base.to_lowercase(), ext), number))
    };
    let mut groups: HashMap<(String, String, String), Vec<(u32, PathBuf)>> = HashMap::new();
    for rom in &rest {
        if let Some((k, number)) = key(rom) {
            groups.entry(k).or_default().push((number, rom.path.clone()));
        }
    }
    let mut out = Vec::new();
    for mut rom in rest {
        let Some((k, _)) = key(&rom) else {
            out.push(rom);
            continue;
        };
        let Some(group) = groups.get_mut(&k) else { continue };
        if group.len() < 2 {
            out.push(rom);
            continue;
        }
        group.sort();
        if !same_file(&group[0].1, &rom.path) {
            continue;
        }
        rom.discs = group.iter().map(|(_, p)| p.clone()).collect();
        out.push(rom);
    }
    out.extend(lists);
    out
}

/// Folders that never hold the player's games.
const SKIPPED_DIRS: [&str; 6] = ["windows", "node_modules", "$recycle.bin", "system volume information", "program files", "program files (x86)"];

/// A folder too wide to look through as a ROM folder when it comes from an
/// emulator's settings (the last folder browsed can be a whole drive): a drive's
/// root or the user's own folder.
pub fn too_wide(dir: &Path) -> bool {
    let parts = dir.components().filter(|c| matches!(c, std::path::Component::Normal(_))).count();
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from);
    parts == 0 || home.is_some_and(|h| dunce::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf()) == dunce::canonicalize(&h).unwrap_or(h))
}

/// Every game file under `dirs`, folders included.
pub fn scan(dirs: &[PathBuf]) -> Vec<RomFile> {
    let mut found = Vec::new();
    let mut seen_dirs = HashSet::new();
    let mut looked = 0usize;
    let mut pending: Vec<PathBuf> = dirs.iter().rev().cloned().collect();
    while let Some(dir) = pending.pop() {
        // A link back up the tree would otherwise be walked forever.
        if !seen_dirs.insert(dunce::canonicalize(&dir).unwrap_or_else(|_| dir.clone())) {
            continue;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            looked += 1;
            if looked > MOST_ENTRIES {
                // Stop looking, but what was found still gets its discs put together.
                pending.clear();
                break;
            }
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                if !name.starts_with('.') && !SKIPPED_DIRS.contains(&name.as_str()) {
                    pending.push(path);
                }
                continue;
            }
            if let Some(rom) = identify(&path) {
                found.push(rom);
            }
        }
    }
    join_discs(found)
}

/// The words of a name that say which game it is: lower case, without
/// apostrophes, bracketed tags ("(USA)", "[!]") or filler words.
/// "Legend of Zelda, The - Majora's Mask (USA)" gives legend, zelda, majoras, mask.
pub fn words(name: &str) -> Vec<String> {
    let lower = name.to_lowercase().replace(['\'', '’'], "");
    let mut outside = String::new();
    let mut depth = 0i32;
    for c in lower.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth == 0 => outside.push(c),
            _ => {}
        }
    }
    const FILLER: &[&str] = &["the", "of", "a", "and", "usa", "us", "europe", "rev", "v1"];
    outside
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty() && !FILLER.contains(w))
        .map(str::to_string)
        .collect()
}

/// Every word of the game's name is in the file's, and most of the file's words
/// are the game's: "Tony Hawk's Pro Skater 3" isn't "Skate 3" though it has both words.
fn same_game(game: &str, rom: &RomFile) -> bool {
    let wanted = words(game);
    if wanted.is_empty() {
        return false;
    }
    // A plural or a small suffix still counts: "kart" in "karts".
    let hit = |w: &String, h: &String| h == w || (w.len() > 4 && h.starts_with(w.as_str()) && h.len() - w.len() <= 2);
    let covers = |pool: &[String]| {
        let has_all = !pool.is_empty() && wanted.iter().all(|w| pool.iter().any(|h| hit(w, h)));
        let mostly_game = pool.iter().filter(|h| wanted.iter().any(|w| hit(w, h))).count() * 2 >= pool.len();
        // A number the game's name doesn't have is another game of the series:
        // "God of War II" isn't "God of War", "Crackdown 2" isn't "Crackdown".
        let another_number = pool.iter().any(|h| is_number_word(h) && !wanted.contains(h));
        has_all && mostly_game && !another_number
    };
    let from_file = rom.path.file_stem().map(|s| words(&s.to_string_lossy())).unwrap_or_default();
    covers(&words(&rom.title)) || covers(&from_file) || header_fits(game, rom)
}

/// "2", "64", "ii", "2nd": a word that numbers a game in its series.
fn is_number_word(word: &str) -> bool {
    const ROMAN: [&str; 11] = ["ii", "iii", "iv", "v", "vi", "vii", "viii", "ix", "x", "xi", "xii"];
    let digits = word.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let suffix = &word[digits.len()..];
    (!digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) && matches!(suffix, "" | "st" | "nd" | "rd" | "th"))
        || ROMAN.contains(&word)
}

/// N64 headers squeeze names into 20 letters, often without spaces
/// ("MARIOKART64"), so those are also compared with the spaces taken out. Long
/// enough to be a name, not a word that's in many games.
fn header_fits(game: &str, rom: &RomFile) -> bool {
    if rom.system != "n64" {
        return false;
    }
    let squash = |s: &str| words(s).concat();
    let header = squash(&rom.title);
    header.len() >= 8 && squash(game).contains(&header)
}

/// Could this file be a game of the console `origin`?
fn fits(rom: &RomFile, origin: &str) -> bool {
    if rom.system.is_empty() {
        return origin.is_empty() || origin == "other" || origin == "arcade" || DISC_CONSOLES.contains(&origin);
    }
    console_fits(&rom.system, origin)
}

/// A cartridge of `system` can be a game of `origin`.
pub fn console_fits(system: &str, origin: &str) -> bool {
    if origin.is_empty() || origin == "other" || origin == "arcade" {
        return true;
    }
    let game_boy = matches!((system, origin), ("gb", "gbc") | ("gbc", "gb"));
    let switch = system == "switch" && origin == "switch2";
    let sega_32x = system == "32x" && origin == "s32x";
    system == origin || game_boy || switch || sega_32x
}

/// The console a cartridge file is for, by its extension; `None` for disc
/// images and unknown files, which could be many consoles.
pub fn cartridge_of(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    if N64.contains(&ext.as_str()) {
        return Some("n64");
    }
    cartridge_system(&ext)
}

/// A console's name as players say it.
pub fn console_name(system: &str) -> &str {
    match system {
        "nes" => "NES",
        "snes" => "Super Nintendo",
        "n64" => "Nintendo 64",
        "gb" => "Game Boy",
        "gbc" => "Game Boy Color",
        "gba" => "Game Boy Advance",
        "ds" => "Nintendo DS",
        "3ds" => "Nintendo 3DS",
        "genesis" => "Mega Drive",
        "switch" => "Nintendo Switch",
        other => other,
    }
}

/// For each recomp, the player's copy of its game, if they have one. A US copy
/// first, as most recomps are built against it.
pub fn match_catalog(entries: &[CatalogEntry], roms: &[RomFile]) -> HashMap<String, String> {
    let mut owned = HashMap::new();
    for entry in entries {
        if entry.game.is_empty() {
            continue;
        }
        let wanted = entry.n64.as_ref();
        // A cartridge whose header names another game isn't this one, whatever its file is called.
        let same_id = |rom: &&RomFile| match (wanted, &rom.n64) {
            (Some(w), Some(code)) if !w.id.is_empty() => code.id.eq_ignore_ascii_case(&w.id),
            _ => true,
        };
        let right_version = |rom: &RomFile| match (wanted, &rom.n64) {
            (Some(w), Some(code)) => version_problem(code, w).is_none(),
            _ => false,
        };
        let candidates = roms.iter().filter(|rom| fits(rom, &entry.origin) && same_game(&entry.game, rom)).filter(same_id);
        // The exact version the recomp was built against first, then a US copy.
        let best = candidates.max_by_key(|rom| (right_version(rom), rom.region == "US"));
        if let Some(rom) = best {
            owned.insert(entry.id.clone(), rom.path.to_string_lossy().into_owned());
        }
    }
    owned
}

/// The last scan, kept on disk so the matches show at start without walking the folders again.
pub fn load_index(file: &Path) -> Vec<RomFile> {
    let text = fs::read_to_string(file).unwrap_or_default();
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn save_index(file: &Path, roms: &[RomFile]) {
    if let Some(dir) = file.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string(roms) {
        let _ = fs::write(file, json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sequel_is_not_the_first_game() {
        let disc = |name: &str| RomFile { path: PathBuf::from(format!(r"D:\ps2\{name}.iso")), system: "ps2".into(), title: name.into(), region: "US".into(), n64: None, discs: Vec::new() };
        assert!(same_game("God of War", &disc("God of War (USA)")));
        assert!(!same_game("God of War", &disc("God of War II (USA)")));
        assert!(!same_game("Crackdown", &disc("Crackdown 2 (USA)")));
        assert!(!same_game("Zone of the Enders", &disc("Zone of the Enders - The 2nd Runner (USA)")));
        assert!(same_game("Mario Kart 64", &disc("Mario Kart 64 (USA)")));
        assert!(same_game("Final Fantasy VII", &disc("Final Fantasy VII (USA) (Disc 1)")));
    }

    #[test]
    fn reads_the_disc_tag() {
        assert_eq!(disc_tag("Final Fantasy VII (USA) (Disc 2)"), Some((2, "Final Fantasy VII (USA)".into())));
        assert_eq!(disc_tag("Metal Gear Solid (Europe) (Disk 1 of 2) (En)"), Some((1, "Metal Gear Solid (Europe) (En)".into())));
        assert_eq!(disc_tag("Game (CD3)"), Some((3, "Game".into())));
        assert_eq!(disc_tag("Discworld (USA)"), None);
        assert_eq!(disc_tag("Game (Disc Edition)"), None);
        assert_eq!(disc_tag("İstanbul (Disc 2)"), Some((2, "İstanbul".into())), "letters that change size in lowercase");
    }

    #[test]
    fn a_game_on_several_discs_shows_once() {
        let dir = tempfile::tempdir().unwrap();
        let ff = dir.path().join("ps1");
        let mgs = dir.path().join("mgs");
        fs::create_dir_all(&ff).unwrap();
        fs::create_dir_all(&mgs).unwrap();
        for name in ["Final Fantasy VII (USA) (Disc 2).chd", "Final Fantasy VII (USA) (Disc 1).chd", "Final Fantasy VII (USA) (Disc 3).chd", "Crash (USA).chd", "Lone (USA) (Disc 1).chd"] {
            fs::write(ff.join(name), b"disc").unwrap();
        }
        for name in ["MGS (Disc 1).cue", "MGS (Disc 2).cue"] {
            fs::write(mgs.join(name), b"disc").unwrap();
        }
        // The player's own list wins over the names.
        fs::write(mgs.join("MGS.m3u"), "# discs\nMGS (Disc 1).cue\r\nMGS (Disc 2).cue\nmissing.cue\n").unwrap();
        let mut found = scan(&[dir.path().to_path_buf()]);
        found.sort_by(|a, b| a.path.cmp(&b.path));
        let names: Vec<String> = found.iter().map(|r| r.path.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["MGS.m3u", "Crash (USA).chd", "Final Fantasy VII (USA) (Disc 1).chd", "Lone (USA) (Disc 1).chd"]);
        let ff7 = &found[2];
        assert_eq!(ff7.discs.iter().map(|d| d.file_name().unwrap().to_string_lossy().into_owned()).collect::<Vec<_>>(),
            ["Final Fantasy VII (USA) (Disc 1).chd", "Final Fantasy VII (USA) (Disc 2).chd", "Final Fantasy VII (USA) (Disc 3).chd"]);
        assert_eq!(found[0].discs.len(), 2);
        assert!(found[1].discs.is_empty() && found[3].discs.is_empty());
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cerberus-roms-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A 64-byte N64 header in big-endian order, with this name and region.
    fn n64_rom(name: &str, region: u8) -> Vec<u8> {
        let mut head = vec![0u8; 64];
        head[..4].copy_from_slice(&[0x80, 0x37, 0x12, 0x40]);
        head[0x20..0x20 + name.len()].copy_from_slice(name.as_bytes());
        head[0x3E] = region;
        head
    }

    fn rom(path: &str, system: &str, title: &str) -> RomFile {
        RomFile { path: PathBuf::from(path), system: system.into(), title: title.into(), region: String::new(), n64: None, discs: Vec::new() }
    }

    fn code(id: &str, region: char, revision: u8) -> N64Code {
        N64Code { id: id.into(), region, revision }
    }

    #[test]
    fn reads_the_version_from_the_header() {
        let dir = temp("code");
        let mut head = n64_rom("MARIOKART64", b'E');
        head[0x3B..0x40].copy_from_slice(b"NKTE\x01");
        fs::write(dir.join("mk.z64"), head).unwrap();
        assert_eq!(n64_code_of(&dir.join("mk.z64")), Some(code("KT", 'E', 1)));
    }

    #[test]
    fn says_what_is_wrong_with_a_version() {
        let wanted = N64Wanted { id: "KT".into(), regions: "E".into(), revisions: vec![] };
        assert!(version_problem(&code("KT", 'E', 0), &wanted).is_none());
        let europe = version_problem(&code("KT", 'P', 0), &wanted).unwrap();
        assert!(europe.contains("europeia") && europe.contains("americana"));
        assert!(version_problem(&code("ZS", 'E', 0), &wanted).unwrap().contains("outro jogo"));
        let rev = N64Wanted { id: String::new(), regions: String::new(), revisions: vec![1] };
        assert!(version_problem(&code("SF", 'E', 0), &rev).unwrap().contains("1.1"));
    }

    #[test]
    fn prefers_the_version_the_recomp_needs() {
        let mut us = rom(r"C:\roms\Mario Kart 64 (USA).z64", "n64", "MARIOKART64");
        us.n64 = Some(code("KT", 'E', 0));
        let mut eu = rom(r"C:\roms\Mario Kart 64 (Europe).z64", "n64", "MARIOKART64");
        eu.n64 = Some(code("KT", 'P', 0));
        let mut needs_eu = entry("mk-pal", "Mario Kart 64", "n64");
        needs_eu.n64 = Some(N64Wanted { id: "KT".into(), regions: "P".into(), revisions: vec![] });
        let owned = match_catalog(&[needs_eu], &[us, eu]);
        assert!(owned["mk-pal"].contains("Europe"));
    }

    #[test]
    fn a_header_for_another_game_is_not_a_match() {
        let mut odd = rom(r"C:\roms\Mario Kart 64.z64", "n64", "MARIOKART64");
        odd.n64 = Some(code("ZS", 'E', 0));
        let mut mk = entry("mk", "Mario Kart 64", "n64");
        mk.n64 = Some(N64Wanted { id: "KT".into(), ..N64Wanted::default() });
        assert!(match_catalog(&[mk], &[odd]).is_empty());
    }

    fn entry(id: &str, game: &str, origin: &str) -> CatalogEntry {
        CatalogEntry { id: id.into(), game: game.into(), origin: origin.into(), ..CatalogEntry::default() }
    }

    #[test]
    fn reads_an_n64_header_in_every_byte_order() {
        let dir = temp("n64");
        let big = n64_rom("ZELDA MAJORA'S MASK", b'E');
        let swapped: Vec<u8> = big.chunks(2).flat_map(|p| [p[1], p[0]]).collect();
        let little: Vec<u8> = big.chunks(4).flat_map(|w| [w[3], w[2], w[1], w[0]]).collect();
        for (name, bytes) in [("a.z64", big), ("b.v64", swapped), ("c.n64", little)] {
            fs::write(dir.join(name), bytes).unwrap();
            let found = identify(&dir.join(name)).unwrap();
            assert_eq!(found.title, "ZELDA MAJORA'S MASK", "{name}");
            assert_eq!(found.region, "US", "{name}");
        }
    }

    #[test]
    fn finds_games_in_subfolders_and_skips_other_files() {
        let dir = temp("scan");
        fs::create_dir_all(dir.join("N64/USA")).unwrap();
        fs::write(dir.join("N64/USA/mm.z64"), n64_rom("ZELDA MAJORA'S MASK", b'E')).unwrap();
        fs::write(dir.join("Sonic Unleashed (USA).iso"), b"disc").unwrap();
        fs::write(dir.join("notes.txt"), b"text").unwrap();
        let found = scan(&[dir]);
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn matches_by_the_n64_header_whatever_the_file_is_called() {
        let roms = vec![rom(r"C:\roms\mk.z64", "n64", "MARIOKART64")];
        let entries = vec![entry("mk64", "Mario Kart 64", "n64")];
        assert_eq!(match_catalog(&entries, &roms).len(), 1);
        let roms = vec![rom(r"C:\roms\mm.z64", "n64", "ZELDA MAJORA'S MASK")];
        let entries = vec![entry("mm", "The Legend of Zelda: Majora's Mask", "n64")];
        assert_eq!(match_catalog(&entries, &roms).len(), 1);
    }

    #[test]
    fn a_short_header_is_not_enough() {
        let roms = vec![rom(r"C:\roms\x.z64", "n64", "MARIO")];
        let entries = vec![entry("mk64", "Mario Kart 64", "n64")];
        assert!(match_catalog(&entries, &roms).is_empty());
    }

    #[test]
    fn matches_a_no_intro_style_name() {
        let roms = vec![rom(r"C:\roms\Legend of Zelda, The - Majora's Mask (USA).z64", "n64", "ZELDA MAJORA'S MASK")];
        let entries = vec![entry("zelda64recomp", "The Legend of Zelda: Majora's Mask", "n64")];
        assert_eq!(match_catalog(&entries, &roms)["zelda64recomp"], r"C:\roms\Legend of Zelda, The - Majora's Mask (USA).z64");
    }

    #[test]
    fn does_not_match_a_different_game_with_the_same_words() {
        let roms = vec![rom(r"C:\roms\Tony Hawk's Pro Skater 3.iso", "", "Tony Hawk's Pro Skater 3")];
        let entries = vec![entry("skate3", "Skate 3", "x360")];
        assert!(match_catalog(&entries, &roms).is_empty());
    }

    #[test]
    fn does_not_match_across_consoles() {
        let roms = vec![rom(r"C:\roms\Mario Kart 64.z64", "n64", "MARIO KART 64")];
        let entries = vec![entry("mkds", "Mario Kart 64", "ds")];
        assert!(match_catalog(&entries, &roms).is_empty());
    }

    #[test]
    fn prefers_the_us_copy() {
        let mut eu = rom(r"C:\roms\Banjo-Kazooie (Europe).z64", "n64", "Banjo-Kazooie");
        eu.region = "Europe".into();
        let mut us = rom(r"C:\roms\Banjo-Kazooie (USA).z64", "n64", "Banjo-Kazooie");
        us.region = "US".into();
        let entries = vec![entry("banjo", "Banjo-Kazooie", "n64")];
        let owned = match_catalog(&entries, &[eu, us]);
        assert!(owned["banjo"].contains("(USA)"));
    }

    #[test]
    fn a_disc_image_can_be_any_disc_console() {
        let roms = vec![rom(r"D:\iso\Sonic Unleashed (USA).iso", "", "Sonic Unleashed (USA)")];
        let entries = vec![entry("unleashed", "Sonic Unleashed", "x360"), entry("cart", "Sonic Unleashed", "gba")];
        let owned = match_catalog(&entries, &roms);
        assert!(owned.contains_key("unleashed"));
        assert!(!owned.contains_key("cart"));
    }
}
