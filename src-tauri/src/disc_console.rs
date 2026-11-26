//! Which console a disc image is from, so it opens in the right emulator:
//! read from the disc itself when possible, from its folder's name otherwise.
//!
//! - PlayStation and PlayStation 2: the `SYSTEM.CNF` in the disc's root says
//!   `BOOT` (PS1) or `BOOT2` (PS2). A PSP disc has `PSP_GAME` or `UMD_DATA.BIN`.
//! - GameCube and Wii: a fixed number in the disc header (0x1C and 0x18).
//! - Xbox and Xbox 360: where the game partition sits (see `xdvdfs`).
//!
//! `.cue` images point at a `.bin` of raw CD sectors (2352 bytes, the data
//! inside each); those are read through the cue.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const SECTOR: u64 = 2048;
const GAMECUBE: [u8; 4] = [0xC2, 0x33, 0x9F, 0x3D];
const WII: [u8; 4] = [0x5D, 0x1C, 0x9E, 0xA3];

/// A disc read 2048-byte sector at a time, whatever the file's layout.
trait Sectors {
    fn sector(&mut self, lba: u64) -> io::Result<[u8; 2048]>;
}

/// Plain images (and CHD/CSO, decompressed as read): sectors back to back.
struct Cooked(Box<dyn crate::disc_image::Disc>);

impl Sectors for Cooked {
    fn sector(&mut self, lba: u64) -> io::Result<[u8; 2048]> {
        let mut buf = [0u8; 2048];
        self.0.seek(SeekFrom::Start(lba * SECTOR))?;
        self.0.read_exact(&mut buf)?;
        Ok(buf)
    }
}

/// Raw CD sectors: 2352 bytes each, the 2048 of data starting at `offset`.
struct Raw {
    file: File,
    offset: u64,
}

impl Sectors for Raw {
    fn sector(&mut self, lba: u64) -> io::Result<[u8; 2048]> {
        let mut buf = [0u8; 2048];
        self.file.seek(SeekFrom::Start(lba * 2352 + self.offset))?;
        self.file.read_exact(&mut buf)?;
        Ok(buf)
    }
}

/// The data file a cue sheet's first track is in, and where its data sits in each sector.
fn cue_track(cue: &Path) -> Option<(PathBuf, Option<u64>)> {
    let mut text = String::new();
    File::open(cue).ok()?.take(64 * 1024).read_to_string(&mut text).ok()?;
    let mut file = None;
    for line in text.lines() {
        let line = line.trim();
        let upper = line.to_ascii_uppercase();
        if upper.starts_with("FILE ") && file.is_none() {
            let name = line[5..].trim();
            let name = match name.strip_prefix('"') {
                Some(rest) => rest.split('"').next()?,
                None => name.split_whitespace().next()?,
            };
            // Only a file beside the cue, never somewhere else.
            let name = Path::new(name).file_name()?;
            file = Some(cue.parent()?.join(name));
        }
        if upper.starts_with("TRACK ") && file.is_some() {
            let offset = if upper.contains("MODE2/2352") {
                Some(24)
            } else if upper.contains("MODE1/2352") {
                Some(16)
            } else if upper.contains("MODE1/2048") {
                None
            } else {
                return None;
            };
            return Some((file?, offset));
        }
    }
    None
}

/// The names in the disc's root folder (ISO 9660), upper case, without `;1`.
fn root_names(disc: &mut dyn Sectors) -> Option<Vec<(String, u64, u64)>> {
    let pvd = disc.sector(16).ok()?;
    if pvd[0] != 1 || &pvd[1..6] != b"CD001" {
        return None;
    }
    let le = |b: &[u8], at: usize| -> Option<u64> { Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?) as u64) };
    let record = &pvd[156..190];
    let (extent, size) = (le(record, 2)?, le(record, 10)?.min(16 * SECTOR));
    let mut names = Vec::new();
    for i in 0..size.div_ceil(SECTOR) {
        let block = disc.sector(extent + i).ok()?;
        let mut at = 0usize;
        while at < block.len() {
            let len = block[at] as usize;
            if len == 0 {
                break;
            }
            let Some(rec) = block.get(at..at + len) else { break };
            at += len;
            let name_len = *rec.get(32)? as usize;
            let raw = rec.get(33..33 + name_len)?;
            if raw == [0] || raw == [1] {
                continue;
            }
            let name = String::from_utf8_lossy(raw).split(';').next().unwrap_or_default().trim_end_matches('.').to_ascii_uppercase();
            names.push((name, le(rec, 2)?, le(rec, 10)?));
        }
    }
    Some(names)
}

/// PS1, PS2 or PSP, from the root folder of an ISO 9660 disc.
fn playstation(disc: &mut dyn Sectors) -> Option<&'static str> {
    let names = root_names(disc)?;
    if names.iter().any(|(n, ..)| n == "PSP_GAME" || n == "UMD_DATA.BIN") {
        return Some("psp");
    }
    let (_, extent, size) = names.iter().find(|(n, ..)| n == "SYSTEM.CNF")?;
    let block = disc.sector(*extent).ok()?;
    let text = String::from_utf8_lossy(&block[..(*size as usize).min(2048)]).to_ascii_uppercase();
    if text.contains("BOOT2") {
        Some("ps2")
    } else if text.contains("BOOT") {
        Some("ps1")
    } else {
        None
    }
}

/// GameCube or Wii, from the disc header's fixed numbers.
fn nintendo(disc: &mut dyn Sectors) -> Option<&'static str> {
    let head = disc.sector(0).ok()?;
    if head[0x1C..0x20] == GAMECUBE {
        Some("gc")
    } else if head[0x18..0x1C] == WII {
        Some("wii")
    } else {
        None
    }
}

/// The console from the disc's contents, when they can be read.
fn from_contents(path: &Path, ext: &str) -> Option<&'static str> {
    match ext {
        "cue" => {
            let (bin, offset) = cue_track(path)?;
            let mut disc: Box<dyn Sectors> = match offset {
                Some(offset) => Box::new(Raw { file: File::open(&bin).ok()?, offset }),
                None => Box::new(Cooked(crate::disc_image::open(&bin).ok()?)),
            };
            playstation(&mut *disc)
        }
        "wbfs" => Some("wii"),
        "gcm" => Some("gc"),
        "iso" | "chd" | "cso" => {
            if let Some(kind) = crate::xdvdfs::xbox_kind(path) {
                return Some(kind);
            }
            let mut disc = Cooked(crate::disc_image::open(path).ok()?);
            nintendo(&mut disc).or_else(|| playstation(&mut disc))
        }
        _ => None,
    }
}

/// The console a folder's name says ("Playstation 2", "PS2", "Xbox 360"...).
fn from_folders(path: &Path) -> Option<&'static str> {
    const NAMES: &[(&str, &str)] = &[
        ("playstation3", "ps3"),
        ("ps3", "ps3"),
        ("playstation2", "ps2"),
        ("ps2", "ps2"),
        ("playstationportable", "psp"),
        ("psp", "psp"),
        ("playstation1", "ps1"),
        ("playstation", "ps1"),
        ("psx", "ps1"),
        ("ps1", "ps1"),
        ("xbox360", "x360"),
        ("x360", "x360"),
        ("xbox", "xbox"),
        ("gamecube", "gc"),
        ("ngc", "gc"),
        ("wiiu", "wiiu"),
        ("wii", "wii"),
        ("dreamcast", "dc"),
        ("saturn", "saturn"),
        ("segacd", "segacd"),
        ("megacd", "segacd"),
    ];
    // The nearest folder first: "ROMS\Playstation 2\Game (USA)\game.iso".
    for folder in path.ancestors().skip(1).take(4) {
        let name: String = folder.file_name()?.to_string_lossy().to_lowercase().chars().filter(char::is_ascii_alphanumeric).collect();
        if let Some((_, console)) = NAMES.iter().find(|(n, _)| name == *n) {
            return Some(console);
        }
    }
    None
}

/// The console of a disc image, or none when neither the disc nor its folder says.
/// The console a disc is from by its folders' names only, never opening it.
pub fn console_by_folder(path: &Path) -> Option<&'static str> {
    from_folders(path)
}

pub fn console_of(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    from_contents(path, &ext).or_else(|| from_folders(path))
}

/// A disc's sectors, however it is kept: a cue's raw track, a raw `.bin`, or a
/// plain (or compressed) image.
fn sectors_of(path: &Path) -> Option<Box<dyn Sectors>> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if ext == "cue" {
        let (bin, offset) = cue_track(path)?;
        return Some(match offset {
            Some(offset) => Box::new(Raw { file: File::open(&bin).ok()?, offset }),
            None => Box::new(Cooked(crate::disc_image::open(&bin).ok()?)),
        });
    }
    let mut cooked = Cooked(crate::disc_image::open(path).ok()?);
    let is_iso = |d: &mut dyn Sectors| d.sector(16).is_ok_and(|s| s[0] == 1 && &s[1..6] == b"CD001");
    if is_iso(&mut cooked) || ext != "bin" {
        return Some(Box::new(cooked));
    }
    // A raw `.bin` of 2352-byte sectors: Mode 2 (PlayStation) or Mode 1.
    for offset in [24, 16] {
        let mut raw = Raw { file: File::open(path).ok()?, offset };
        if is_iso(&mut raw) {
            return Some(Box::new(raw));
        }
    }
    Some(Box::new(cooked))
}

/// The names in an ISO 9660 disc's top folder, upper case and without `;1`.
pub fn root_files(path: &Path) -> Option<Vec<String>> {
    let mut disc = sectors_of(path)?;
    Some(root_names(&mut *disc)?.into_iter().map(|(name, ..)| name).collect())
}

/// Letters and digits only, upper case: `SLUS_215.27` and `SLUS-21527` read the same.
pub fn plain_id(id: &str) -> String {
    id.chars().filter(char::is_ascii_alphanumeric).map(|c| c.to_ascii_uppercase()).collect()
}

/// The IDs a disc carries, as plain ids: a PlayStation or PS2 serial (from
/// `SYSTEM.CNF`), a PSP's (from `UMD_DATA.BIN`), a GameCube or Wii disc's.
pub fn disc_ids(path: &Path) -> Vec<String> {
    let Some(mut disc) = sectors_of(path) else { return Vec::new() };
    let mut ids = Vec::new();
    if nintendo(&mut *disc).is_some() {
        if let Ok(head) = disc.sector(0) {
            ids.push(plain_id(&String::from_utf8_lossy(&head[..6])));
        }
        return ids;
    }
    let Some(names) = root_names(&mut *disc) else { return ids };
    let text_of = |disc: &mut dyn Sectors, name: &str| -> Option<String> {
        let (_, extent, size) = names.iter().find(|(n, ..)| n == name)?;
        let block = disc.sector(*extent).ok()?;
        Some(String::from_utf8_lossy(&block[..(*size as usize).min(2048)]).into_owned())
    };
    if let Some(cnf) = text_of(&mut *disc, "SYSTEM.CNF") {
        // BOOT2 = cdrom0:\SLUS_215.27;1
        let boot = cnf.lines().find(|l| l.trim_start().to_ascii_uppercase().starts_with("BOOT"));
        if let Some(name) = boot.and_then(|l| l.rsplit(['\\', ':', '/']).next()) {
            let id = plain_id(name.split(';').next().unwrap_or_default());
            if !id.is_empty() {
                ids.push(id);
            }
        }
    }
    if let Some(umd) = text_of(&mut *disc, "UMD_DATA.BIN") {
        ids.push(plain_id(umd.split('|').next().unwrap_or_default()));
    }
    ids.retain(|id| !id.is_empty());
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A tiny ISO 9660 image: sector 16 the volume descriptor, sector 20 the
    /// root folder holding one file, sector 21 that file.
    fn iso(name: &str, content: &[u8]) -> Vec<u8> {
        let mut img = vec![0u8; 22 * 2048];
        let pvd = 16 * 2048;
        img[pvd] = 1;
        img[pvd + 1..pvd + 6].copy_from_slice(b"CD001");
        img[pvd + 156 + 2..pvd + 156 + 6].copy_from_slice(&20u32.to_le_bytes());
        img[pvd + 156 + 10..pvd + 156 + 14].copy_from_slice(&2048u32.to_le_bytes());
        let mut at = 20 * 2048;
        for raw in [vec![0u8], vec![1u8], format!("{name};1").into_bytes()] {
            let len = 33 + raw.len() + (raw.len() + 1) % 2;
            img[at] = len as u8;
            img[at + 2..at + 6].copy_from_slice(&21u32.to_le_bytes());
            img[at + 10..at + 14].copy_from_slice(&(content.len() as u32).to_le_bytes());
            img[at + 32] = raw.len() as u8;
            img[at + 33..at + 33 + raw.len()].copy_from_slice(&raw);
            at += len;
        }
        img[21 * 2048..21 * 2048 + content.len()].copy_from_slice(content);
        img
    }

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        std::fs::File::create(&p).unwrap().write_all(bytes).unwrap();
        p
    }

    #[test]
    fn playstation_discs_tell_ps1_ps2_and_psp_apart() {
        let dir = tempfile::tempdir().unwrap();
        let ps2 = write(dir.path(), "a.iso", &iso("SYSTEM.CNF", b"BOOT2 = cdrom0:\\SLUS_215.27;1\r\nVER = 1.00\r\n"));
        assert_eq!(console_of(&ps2), Some("ps2"));
        let ps1 = write(dir.path(), "b.iso", &iso("SYSTEM.CNF", b"BOOT = cdrom:\\SCUS_949.00;1\r\nTCB = 4\r\n"));
        assert_eq!(console_of(&ps1), Some("ps1"));
        let psp = write(dir.path(), "c.iso", &iso("UMD_DATA.BIN", b"ULUS-10336|"));
        assert_eq!(console_of(&psp), Some("psp"));
    }

    #[test]
    fn a_cue_is_read_through_its_raw_bin() {
        let dir = tempfile::tempdir().unwrap();
        // The same tiny disc, as raw 2352-byte Mode 2 sectors (data at byte 24 of each).
        let cooked = iso("SYSTEM.CNF", b"BOOT = cdrom:\\SCUS_949.00;1\r\n");
        let mut raw = Vec::new();
        for chunk in cooked.chunks(2048) {
            let mut sector = vec![0u8; 2352];
            sector[24..24 + 2048].copy_from_slice(chunk);
            raw.extend_from_slice(&sector);
        }
        write(dir.path(), "Crash Bandicoot (USA).bin", &raw);
        let cue = write(dir.path(), "Crash Bandicoot (USA).cue", b"FILE \"Crash Bandicoot (USA).bin\" BINARY\r\n  TRACK 01 MODE2/2352\r\n    INDEX 01 00:00:00\r\n");
        assert_eq!(console_of(&cue), Some("ps1"));
        // A cue pointing outside its folder isn't followed there.
        let sly = write(dir.path(), "sly.cue", b"FILE \"..\\..\\other.bin\" BINARY\r\n  TRACK 01 MODE2/2352\r\n");
        assert_eq!(cue_track(&sly).unwrap().0, dir.path().join("other.bin"));
    }

    #[test]
    fn a_disc_tells_its_serial_and_top_files() {
        let dir = tempfile::tempdir().unwrap();
        let ps2 = write(dir.path(), "a.iso", &iso("SYSTEM.CNF", b"BOOT2 = cdrom0:\\SLUS_206.51;1\r\nVER = 1.00\r\n"));
        assert_eq!(disc_ids(&ps2), ["SLUS20651"]);
        assert_eq!(root_files(&ps2).unwrap(), ["SYSTEM.CNF"]);
        let psp = write(dir.path(), "c.iso", &iso("UMD_DATA.BIN", b"ULUS-10336|0001"));
        assert_eq!(disc_ids(&psp), ["ULUS10336"]);
        assert_eq!(plain_id("SLUS-21501"), plain_id("SLUS_215.01"));
    }

    #[test]
    fn nintendo_discs_by_their_header() {
        let dir = tempfile::tempdir().unwrap();
        let mut gc = vec![0u8; 40 * 2048];
        gc[0x1C..0x20].copy_from_slice(&GAMECUBE);
        assert_eq!(console_of(&write(dir.path(), "g.iso", &gc)), Some("gc"));
        let mut wii = vec![0u8; 40 * 2048];
        wii[0x18..0x1C].copy_from_slice(&WII);
        assert_eq!(console_of(&write(dir.path(), "w.iso", &wii)), Some("wii"));
    }

    /// The player's real discs (read only), from the classics already found.
    /// `cargo test -p cerberus-launcher real_discs -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_discs() {
        let index = PathBuf::from(std::env::var("APPDATA").unwrap()).join("app.cerberus.launcher").join("cache").join("roms.json");
        let list: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(index).unwrap()).unwrap();
        for rom in list.as_array().unwrap() {
            let path = PathBuf::from(rom["path"].as_str().unwrap());
            let ext = path.extension().unwrap().to_string_lossy().to_lowercase();
            println!("{:<6} contents={:<6} folder={:<6} {}", console_of(&path).unwrap_or("-"), from_contents(&path, &ext).unwrap_or("-"), from_folders(&path).unwrap_or("-"), path.display());
        }
    }

    #[test]
    fn otherwise_the_folder_name_says() {
        assert_eq!(from_folders(Path::new(r"D:\ROMS\Xbox 360\FIFA Street (USA, Europe)\FIFA Street.iso")), Some("x360"));
        assert_eq!(from_folders(Path::new(r"D:\ROMS\Playstation 2\Game (USA)\game.iso")), Some("ps2"));
        assert_eq!(from_folders(Path::new(r"D:\ROMS\Wii U\Game\game.wux")), Some("wiiu"));
        assert_eq!(from_folders(Path::new(r"D:\ROMS\PlayStation\Game.cue")), Some("ps1"));
        assert_eq!(from_folders(Path::new(r"D:\Games\Random\game.iso")), None);
    }
}
