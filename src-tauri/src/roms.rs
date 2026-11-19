//! The player's own games in their ROM and backup folders, and which recomps
//! they fit. Nothing is read beyond a file's first bytes, and nothing leaves the PC.
//!
//! N64 cartridges carry the game's name in their header, so those are matched
//! on it whatever the file is called; other cartridges and disc images are
//! matched on their file name.

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
        'D' => "alemã",
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
        return Some(format!("Essa ROM é de outro jogo (código {}, esperado {}).", code.id, wanted.id));
    }
    if !wanted.regions.is_empty() && !wanted.regions.contains(code.region) {
        let wanted_names: Vec<&str> = wanted.regions.chars().map(region_name).collect();
        return Some(format!(
            "Essa ROM é a versão {}; esse recomp precisa da versão {}.",
            region_name(code.region),
            wanted_names.join(" ou ")
        ));
    }
    if !wanted.revisions.is_empty() && !wanted.revisions.contains(&code.revision) {
        let list: Vec<String> = wanted.revisions.iter().map(|r| format!("1.{r}")).collect();
        return Some(format!("Essa ROM é a revisão 1.{}; esse recomp precisa da {}.", code.revision, list.join(" ou ")));
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
const DISC: &[&str] = &["iso", "chd", "cue", "gdi", "cdi", "rvz", "wbfs", "gcm", "cso", "wia", "xex", "pkg"];

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
        return Some(RomFile { path: path.to_path_buf(), system: "n64".into(), title: header.name, region, n64: Some(header.code) });
    }
    let system = match cartridge_system(&ext) {
        Some(system) => system.to_string(),
        None if DISC.contains(&ext.as_str()) => String::new(),
        None => return None,
    };
    Some(RomFile { path: path.to_path_buf(), system, region: region_from_name(&stem), title: stem, n64: None })
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
                return found;
            }
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if path.is_dir() {
                if !name.starts_with('.') && name != "windows" && name != "node_modules" {
                    pending.push(path);
                }
                continue;
            }
            if let Some(rom) = identify(&path) {
                found.push(rom);
            }
        }
    }
    found
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
        has_all && mostly_game
    };
    let from_file = rom.path.file_stem().map(|s| words(&s.to_string_lossy())).unwrap_or_default();
    covers(&words(&rom.title)) || covers(&from_file) || header_fits(game, rom)
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
    system == origin || game_boy || switch
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
        RomFile { path: PathBuf::from(path), system: system.into(), title: title.into(), region: String::new(), n64: None }
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
