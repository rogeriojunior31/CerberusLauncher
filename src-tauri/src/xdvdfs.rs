//! Reading Xbox and Xbox 360 disc images (XDVDFS), to put a game's files where
//! a recomp expects them when the player has the disc image, not the files.
//!
//! The volume header (`MICROSOFT*XBOX*MEDIA`) sits in sector 32 of the game
//! partition, which starts at a different place depending on the disc type; a
//! trimmed image ("xiso") starts at 0. Each directory is a binary tree of
//! entries: left and right child offsets (in 4-byte units), the entry's first
//! sector and size, its attributes, and its name.

use cerberus_shared::tr;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use crate::disc_image::Disc;
use std::path::{Component, Path};

const SECTOR: u64 = 2048;
const MAGIC: &[u8; 20] = b"MICROSOFT*XBOX*MEDIA";
/// Where the game partition starts: a trimmed image, a full original Xbox disc,
/// and Xbox 360 XGD2 and XGD3 discs.
const PARTITIONS: [u64; 4] = [0, 0x1830_0000, 0x0FD9_0000, 0x0208_0000];
const DIRECTORY: u8 = 0x10;
/// Deeper than any real disc; stops a broken image from looping.
const MAX_DEPTH: usize = 32;

/// Where the game partition starts in this image, if it is an Xbox disc at all.
fn partition(file: &mut dyn Disc) -> Option<u64> {
    for start in PARTITIONS {
        let mut magic = [0u8; 20];
        let at = start + 32 * SECTOR;
        if file.seek(SeekFrom::Start(at)).is_err() || file.read_exact(&mut magic).is_err() {
            continue;
        }
        if &magic == MAGIC {
            return Some(start);
        }
    }
    None
}

/// "xbox" or "x360" by where the game partition sits; a trimmed image (at 0)
/// could be either, so it says nothing.
pub fn xbox_kind(path: &Path) -> Option<&'static str> {
    match crate::disc_image::open(path).ok().as_mut().and_then(|f| partition(&mut **f))? {
        0x1830_0000 => Some("xbox"),
        0x0FD9_0000 | 0x0208_0000 => Some("x360"),
        _ => None,
    }
}

pub fn is_xbox_disc(path: &Path) -> bool {
    crate::disc_image::open(path).ok().as_mut().and_then(|f| partition(&mut **f)).is_some()
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// The names in an Xbox disc image's top folder.
pub fn root_files(path: &Path) -> Option<Vec<String>> {
    let mut file = crate::disc_image::open(path).ok()?;
    let start = partition(&mut *file)?;
    let mut header = [0u8; 28];
    file.seek(SeekFrom::Start(start + 32 * SECTOR)).ok()?;
    file.read_exact(&mut header).ok()?;
    let (sector, size) = (u32_at(&header, 20)? as u64, u32_at(&header, 24)? as u64);
    if size == 0 || size > 16 * 1024 * 1024 {
        return None;
    }
    let mut table = vec![0u8; size as usize];
    file.seek(SeekFrom::Start(start + sector * SECTOR)).ok()?;
    file.read_exact(&mut table).ok()?;
    let mut names = Vec::new();
    let (mut pending, mut visited) = (vec![0usize], HashSet::new());
    while let Some(at) = pending.pop() {
        if !visited.insert(at) || at + 14 > table.len() {
            continue;
        }
        let left = u16_at(&table, at)?;
        if left == 0xFFFF {
            continue;
        }
        let right = u16_at(&table, at + 2)?;
        let len = table[at + 13] as usize;
        names.push(String::from_utf8_lossy(table.get(at + 14..at + 14 + len)?).into_owned());
        for next in [left, right] {
            if next != 0 {
                pending.push(next as usize * 4);
            }
        }
    }
    Some(names)
}

/// Unpacks every file of the disc image into `to`. Returns how many were written.
pub fn extract(image: &Path, to: &Path) -> Result<usize, String> {
    let mut file = crate::disc_image::open(image).map_err(|e| e.to_string())?;
    let start = partition(&mut *file).ok_or(tr!("Esse arquivo não é uma imagem de disco de Xbox ou Xbox 360."))?;
    let mut header = [0u8; 28];
    file.seek(SeekFrom::Start(start + 32 * SECTOR)).map_err(|e| e.to_string())?;
    file.read_exact(&mut header).map_err(|e| e.to_string())?;
    let root_sector = u32_at(&header, 20).unwrap_or(0) as u64;
    let root_size = u32_at(&header, 24).unwrap_or(0) as u64;
    fs::create_dir_all(to).map_err(|e| e.to_string())?;
    let mut written = 0;
    let mut seen = HashSet::new();
    walk(&mut *file, start, root_sector, root_size, to, 0, &mut seen, &mut written).map_err(|e| tr!("Não foi possível ler a imagem de disco: {}", e))?;
    Ok(written)
}

#[allow(clippy::too_many_arguments)]
fn walk(file: &mut dyn Disc, start: u64, sector: u64, size: u64, to: &Path, depth: usize, seen: &mut HashSet<u64>, written: &mut usize) -> io::Result<()> {
    let bad = |what: &str| io::Error::new(io::ErrorKind::InvalidData, what.to_string());
    // A directory read twice means the image points in a circle.
    if depth > MAX_DEPTH || size == 0 || size > 64 * 1024 * 1024 || !seen.insert(sector) {
        return if size == 0 { Ok(()) } else { Err(bad(tr!("estrutura de pastas inválida"))) };
    }
    let mut table = vec![0u8; size as usize];
    file.seek(SeekFrom::Start(start + sector * SECTOR))?;
    file.read_exact(&mut table)?;

    // The tree's nodes, visited without recursion so a deep tree can't overflow the stack.
    let mut pending = vec![0usize];
    let mut visited = HashSet::new();
    while let Some(at) = pending.pop() {
        if !visited.insert(at) || at + 14 > table.len() {
            continue;
        }
        let left = u16_at(&table, at).ok_or_else(|| bad("entrada cortada"))?;
        // An empty directory is a single entry filled with 0xFF.
        if left == 0xFFFF {
            continue;
        }
        let right = u16_at(&table, at + 2).ok_or_else(|| bad("entrada cortada"))?;
        let entry_sector = u32_at(&table, at + 4).ok_or_else(|| bad("entrada cortada"))? as u64;
        let entry_size = u32_at(&table, at + 8).ok_or_else(|| bad("entrada cortada"))? as u64;
        let attributes = table[at + 12];
        let name_len = table[at + 13] as usize;
        let name_bytes = table.get(at + 14..at + 14 + name_len).ok_or_else(|| bad("nome cortado"))?;
        let name = String::from_utf8_lossy(name_bytes).into_owned();
        // A name is one plain part: nothing that climbs out of the folder.
        let plain = Path::new(&name).components().count() == 1 && matches!(Path::new(&name).components().next(), Some(Component::Normal(_)));
        if !plain {
            return Err(bad(tr!("nome de arquivo inseguro")));
        }
        let target = to.join(&name);
        if attributes & DIRECTORY != 0 {
            fs::create_dir_all(&target)?;
            walk(file, start, entry_sector, entry_size, &target, depth + 1, seen, written)?;
        } else {
            copy_out(file, start + entry_sector * SECTOR, entry_size, &target)?;
            *written += 1;
        }
        if left != 0 {
            pending.push(left as usize * 4);
        }
        if right != 0 {
            pending.push(right as usize * 4);
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
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One directory-table entry; `left`/`right` are byte offsets in the table.
    fn entry(left: usize, right: usize, sector: u32, size: u32, dir: bool, name: &str) -> Vec<u8> {
        let mut e = Vec::new();
        e.extend_from_slice(&((left / 4) as u16).to_le_bytes());
        e.extend_from_slice(&((right / 4) as u16).to_le_bytes());
        e.extend_from_slice(&sector.to_le_bytes());
        e.extend_from_slice(&size.to_le_bytes());
        e.push(if dir { DIRECTORY } else { 0 });
        e.push(name.len() as u8);
        e.extend_from_slice(name.as_bytes());
        while e.len() % 4 != 0 {
            e.push(0xFF);
        }
        e
    }

    /// A tiny trimmed image: default.xex and media/intro.bik at the root.
    fn tiny_image(path: &Path) {
        let mut image = vec![0u8; 40 * SECTOR as usize];
        let header_at = 32 * SECTOR as usize;
        image[header_at..header_at + 20].copy_from_slice(MAGIC);
        // Root table in sector 33: "default.xex", with "media" as its right child.
        let media_table = entry(0, 0, 37, 4, false, "intro.bik");
        let xex_len = entry(0, 0, 36, 5, false, "default.xex").len();
        let mut root_table = entry(0, xex_len, 36, 5, false, "default.xex");
        root_table.extend_from_slice(&entry(0, 0, 34, media_table.len() as u32, true, "media"));
        image[header_at + 20..header_at + 24].copy_from_slice(&33u32.to_le_bytes());
        image[header_at + 24..header_at + 28].copy_from_slice(&(root_table.len() as u32).to_le_bytes());
        let root_at = 33 * SECTOR as usize;
        image[root_at..root_at + root_table.len()].copy_from_slice(&root_table);
        // "media" directory in sector 34, holding intro.bik.
        let media_dir_at = 34 * SECTOR as usize;
        image[media_dir_at..media_dir_at + media_table.len()].copy_from_slice(&media_table);
        image[36 * SECTOR as usize..36 * SECTOR as usize + 5].copy_from_slice(b"XEX2!");
        image[37 * SECTOR as usize..37 * SECTOR as usize + 4].copy_from_slice(b"BIKi");
        fs::write(path, image).unwrap();
    }

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("cerberus-xdvdfs-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn extracts_files_and_folders() {
        let dir = temp("extract");
        let iso = dir.join("game.iso");
        tiny_image(&iso);
        assert!(is_xbox_disc(&iso));
        let out = dir.join("out");
        assert_eq!(extract(&iso, &out).unwrap(), 2);
        assert_eq!(fs::read(out.join("default.xex")).unwrap(), b"XEX2!");
        assert_eq!(fs::read(out.join("media/intro.bik")).unwrap(), b"BIKi");
    }

    #[test]
    fn refuses_what_is_not_an_xbox_disc() {
        let dir = temp("not");
        let file = dir.join("ps2.iso");
        fs::write(&file, vec![0u8; 100 * 1024]).unwrap();
        assert!(!is_xbox_disc(&file));
        assert!(extract(&file, &dir.join("out")).is_err());
    }

    #[test]
    fn refuses_names_that_climb_out() {
        let dir = temp("evil");
        let iso = dir.join("evil.iso");
        let mut image = vec![0u8; 40 * SECTOR as usize];
        let header_at = 32 * SECTOR as usize;
        image[header_at..header_at + 20].copy_from_slice(MAGIC);
        let root = entry(0, 0, 36, 1, false, "..");
        image[header_at + 20..header_at + 24].copy_from_slice(&33u32.to_le_bytes());
        image[header_at + 24..header_at + 28].copy_from_slice(&(root.len() as u32).to_le_bytes());
        let root_at = 33 * SECTOR as usize;
        image[root_at..root_at + root.len()].copy_from_slice(&root);
        fs::write(&iso, image).unwrap();
        assert!(extract(&iso, &dir.join("out")).is_err());
    }
}
