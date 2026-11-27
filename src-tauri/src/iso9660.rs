//! Reading ISO 9660 disc images (PlayStation 2 and PSP discs, most CDs and DVDs),
//! to put a game's files where a recomp expects them.
//!
//! The primary volume descriptor sits in sector 16 and holds the root
//! directory's record. A directory is a run of records: each gives its length,
//! the first sector and size of the file or folder, whether it's a folder, and
//! its name (`NAME.EXT;1`, the `;1` being a version number).

use cerberus_shared::tr;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, SeekFrom, Write};
use crate::disc_image::Disc;
use std::path::{Component, Path};

const SECTOR: u64 = 2048;
const MAX_DEPTH: usize = 32;

fn u32_le(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// The root directory's first sector and size, if this is an ISO 9660 image.
fn root(file: &mut dyn Disc) -> Option<(u64, u64)> {
    let mut descriptor = [0u8; 2048];
    file.seek(SeekFrom::Start(16 * SECTOR)).ok()?;
    file.read_exact(&mut descriptor).ok()?;
    // Type 1 is the primary volume descriptor; "CD001" marks ISO 9660.
    if descriptor[0] != 1 || &descriptor[1..6] != b"CD001" {
        return None;
    }
    let record = &descriptor[156..156 + 34];
    Some((u32_le(record, 2)? as u64, u32_le(record, 10)? as u64))
}

pub fn is_iso9660(path: &Path) -> bool {
    crate::disc_image::open(path).ok().as_mut().and_then(|f| root(&mut **f)).is_some()
}

/// Unpacks every file of the image into `to`. Returns how many were written.
pub fn extract(image: &Path, to: &Path) -> Result<usize, String> {
    let mut file = crate::disc_image::open(image).map_err(|e| e.to_string())?;
    let (sector, size) = root(&mut *file).ok_or(tr!("Esse arquivo não é uma imagem de disco ISO."))?;
    fs::create_dir_all(to).map_err(|e| e.to_string())?;
    let mut written = 0;
    let mut seen = HashSet::new();
    walk(&mut *file, sector, size, to, 0, &mut seen, &mut written).map_err(|e| tr!("Não foi possível ler a imagem de disco: {}", e))?;
    Ok(written)
}

fn walk(file: &mut dyn Disc, sector: u64, size: u64, to: &Path, depth: usize, seen: &mut HashSet<u64>, written: &mut usize) -> io::Result<()> {
    let bad = |what: &str| io::Error::new(io::ErrorKind::InvalidData, what.to_string());
    if depth > MAX_DEPTH || size > 64 * 1024 * 1024 || !seen.insert(sector) {
        return Err(bad(tr!("estrutura de pastas inválida")));
    }
    let mut table = vec![0u8; size as usize];
    file.seek(SeekFrom::Start(sector * SECTOR))?;
    file.read_exact(&mut table)?;

    let mut at = 0usize;
    while at < table.len() {
        let length = table[at] as usize;
        // A zero length pads to the next sector: records never cross one.
        if length == 0 {
            at = (at / SECTOR as usize + 1) * SECTOR as usize;
            continue;
        }
        let record = table.get(at..at + length).ok_or_else(|| bad("registro cortado"))?;
        at += length;
        let extent = u32_le(record, 2).ok_or_else(|| bad("registro cortado"))? as u64;
        let data_length = u32_le(record, 10).ok_or_else(|| bad("registro cortado"))? as u64;
        let is_dir = record.get(25).is_some_and(|flags| flags & 0x02 != 0);
        let name_len = *record.get(32).ok_or_else(|| bad("registro cortado"))? as usize;
        let raw_name = record.get(33..33 + name_len).ok_or_else(|| bad("nome cortado"))?;
        // The first two records of every directory are itself (0x00) and its parent (0x01).
        if raw_name == [0] || raw_name == [1] {
            continue;
        }
        let name = String::from_utf8_lossy(raw_name);
        // "SLUS_206.51;1" -> "SLUS_206.51"; a name with no extension keeps no trailing dot.
        let name = name.split(';').next().unwrap_or_default().trim_end_matches('.').to_string();
        let plain = Path::new(&name).components().count() == 1 && matches!(Path::new(&name).components().next(), Some(Component::Normal(_)));
        if !plain || !crate::disk::plain_name(&name) {
            return Err(bad(tr!("nome de arquivo inseguro")));
        }
        let target = to.join(&name);
        if is_dir {
            fs::create_dir_all(&target)?;
            walk(file, extent, data_length, &target, depth + 1, seen, written)?;
        } else {
            copy_out(file, extent * SECTOR, data_length, &target)?;
            *written += 1;
        }
    }
    Ok(())
}

fn copy_out(file: &mut dyn Disc, at: u64, size: u64, target: &Path) -> io::Result<()> {
    file.seek(SeekFrom::Start(at))?;
    let mut out = File::create(target)?;
    let mut left = size;
    let mut buffer = vec![0u8; 1 << 20];
    while left > 0 {
        let take = left.min(buffer.len() as u64) as usize;
        file.read_exact(&mut buffer[..take])?;
        out.write_all(&buffer[..take])?;
        left -= take as u64;
        crate::copying::tick(take as u64)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(extent: u32, size: u32, dir: bool, name: &[u8]) -> Vec<u8> {
        let length = 33 + name.len() + (name.len() + 1) % 2;
        let mut r = vec![0u8; length];
        r[0] = length as u8;
        r[2..6].copy_from_slice(&extent.to_le_bytes());
        r[10..14].copy_from_slice(&size.to_le_bytes());
        r[25] = if dir { 0x02 } else { 0 };
        r[32] = name.len() as u8;
        r[33..33 + name.len()].copy_from_slice(name);
        r
    }

    /// SYSTEM.CNF at the root and DATA/LEVEL1.BIN below it.
    fn tiny_image(path: &Path, evil: bool) {
        let mut image = vec![0u8; 30 * SECTOR as usize];
        let pvd = 16 * SECTOR as usize;
        image[pvd] = 1;
        image[pvd + 1..pvd + 6].copy_from_slice(b"CD001");
        let mut root = record(20, 0, true, &[0]);
        root.extend(record(20, 0, true, &[1]));
        let file_name: &[u8] = if evil { b"..;1" } else { b"SYSTEM.CNF;1" };
        root.extend(record(22, 5, false, file_name));
        let mut data = record(21, 0, true, &[0]);
        data.extend(record(20, 0, true, &[1]));
        data.extend(record(23, 4, false, b"LEVEL1.BIN;1"));
        root.extend(record(21, data.len() as u32, true, b"DATA"));
        let root_record = record(20, root.len() as u32, true, &[0]);
        image[pvd + 156..pvd + 156 + root_record.len()].copy_from_slice(&root_record);
        image[20 * SECTOR as usize..20 * SECTOR as usize + root.len()].copy_from_slice(&root);
        image[21 * SECTOR as usize..21 * SECTOR as usize + data.len()].copy_from_slice(&data);
        image[22 * SECTOR as usize..22 * SECTOR as usize + 5].copy_from_slice(b"BOOT2");
        image[23 * SECTOR as usize..23 * SECTOR as usize + 4].copy_from_slice(b"LVL1");
        fs::write(path, image).unwrap();
    }

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("cerberus-iso-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn extracts_files_and_folders() {
        let dir = temp("extract");
        let iso = dir.join("game.iso");
        tiny_image(&iso, false);
        assert!(is_iso9660(&iso));
        let out = dir.join("out");
        assert_eq!(extract(&iso, &out).unwrap(), 2);
        assert_eq!(fs::read(out.join("SYSTEM.CNF")).unwrap(), b"BOOT2");
        assert_eq!(fs::read(out.join("DATA/LEVEL1.BIN")).unwrap(), b"LVL1");
    }

    #[test]
    fn extracts_from_a_cso_and_a_chd_too() {
        let dir = temp("packed");
        let iso = dir.join("game.iso");
        tiny_image(&iso, false);
        let plain = fs::read(&iso).unwrap();
        let cso = dir.join("game.cso");
        fs::write(&cso, crate::disc_image::tests::cso(&plain)).unwrap();
        let chd = dir.join("game.chd");
        crate::disc_image::tests::write_chd(&chd, chd::write::ChdWriteOptions::dvd(), &plain, None);
        for packed in [cso, chd] {
            assert!(is_iso9660(&packed));
            let out = dir.join(format!("out-{}", packed.extension().unwrap().to_string_lossy()));
            assert_eq!(extract(&packed, &out).unwrap(), 2);
            assert_eq!(fs::read(out.join("DATA/LEVEL1.BIN")).unwrap(), b"LVL1");
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_names_that_climb_out() {
        let dir = temp("evil");
        let iso = dir.join("evil.iso");
        tiny_image(&iso, true);
        assert!(extract(&iso, &dir.join("out")).is_err());
    }

    #[test]
    fn knows_an_iso_from_other_files() {
        let dir = temp("not");
        let file = dir.join("x.bin");
        fs::write(&file, vec![0u8; 40 * 2048]).unwrap();
        assert!(!is_iso9660(&file));
    }
}
