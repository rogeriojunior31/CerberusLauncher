//! Compressed disc images (CHD and CSO) read as the plain disc they hold, so
//! a recomp's game files can come out of them as out of an ISO.
//!
//! What comes out is the disc's data as 2048-byte sectors, the way an ISO
//! holds it:
//! - a DVD CHD (`chdman createdvd`, PS2 DVDs) already is that;
//! - a CD CHD keeps whole 2352-byte frames plus 96 bytes of subcode, so each
//!   sector's data is cut out of its frame, by the first track's type;
//! - a CSO (PSP, PS2) is the ISO cut in blocks, each deflated or stored.

use cerberus_shared::tr;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

/// A disc's data, readable and seekable like an ISO file.
pub trait Disc: Read + Seek {}
impl<T: Read + Seek> Disc for T {}

const SECTOR: u64 = 2048;
/// A CD frame in a CHD: 2352 bytes of sector and 96 of subcode.
const CD_FRAME: u64 = 2448;

fn bad(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why.to_string())
}

/// What kind of file this is, by its first bytes.
#[derive(Debug, PartialEq)]
pub enum Kind {
    Chd,
    Cso,
    Plain,
}

pub fn kind_of(path: &Path) -> Kind {
    let mut magic = [0u8; 8];
    let read = File::open(path).and_then(|mut f| f.read_exact(&mut magic));
    match (read, &magic) {
        (Ok(()), b"MComprHD") => Kind::Chd,
        (Ok(()), [b'C', b'I', b'S', b'O', ..]) => Kind::Cso,
        _ => Kind::Plain,
    }
}

/// Opens an image as its disc data: CHD and CSO decompressed as they are read,
/// anything else as the file itself.
pub fn open(path: &Path) -> io::Result<Box<dyn Disc>> {
    match kind_of(path) {
        Kind::Chd => chd(path),
        Kind::Cso => Ok(Box::new(Cso::open(BufReader::new(File::open(path)?))?)),
        Kind::Plain => Ok(Box::new(BufReader::new(File::open(path)?))),
    }
}

fn chd(path: &Path) -> io::Result<Box<dyn Disc>> {
    let fail = |e: chd::Error| bad(&tr!("CHD ilegível: {}", e));
    let mut chd = chd::Chd::open(BufReader::new(File::open(path)?), None).map_err(fail)?;
    if chd.header().has_parent() {
        return Err(bad(tr!("Esse CHD depende de outro (é um CHD \"filho\"); use o CHD completo.")));
    }
    let refs: Vec<chd::metadata::MetadataRef> = chd.metadata_refs().collect::<Result<_, _>>().map_err(fail)?;
    let mut first_track = None;
    for r in refs {
        let meta = r.read(chd.inner()).map_err(fail)?;
        if meta.metatag == chd::metadata::KnownMetadata::GdRomTrack as u32 {
            return Err(bad(tr!("Imagens de GD-ROM (Dreamcast) não têm como ser abertas assim.")));
        }
        let cd = [chd::metadata::KnownMetadata::CdRomTrack2 as u32, chd::metadata::KnownMetadata::CdRomTrack as u32];
        if cd.contains(&meta.metatag) {
            let text = String::from_utf8_lossy(&meta.value).into_owned();
            if track_number(&text) == Some(1) {
                first_track = Some(text);
            }
        }
    }
    let reader = chd::read::ChdReader::new(chd);
    let Some(track) = first_track else {
        // No CD tracks: a DVD (or hard disk) image, already plain sectors.
        return Ok(Box::new(reader));
    };
    let start = data_start(&track).ok_or_else(|| bad(tr!("A primeira faixa desse CD não é de dados.")))?;
    let frames = field(&track, "FRAMES").and_then(|f| f.parse::<u64>().ok()).unwrap_or(0);
    Ok(Box::new(CdSectors { inner: reader, start, len: frames * SECTOR, position: 0 }))
}

/// `TRACK:1 TYPE:MODE2_RAW SUBTYPE:NONE FRAMES:12345 ...`: one field's value.
fn field<'a>(track: &'a str, name: &str) -> Option<&'a str> {
    track.trim_end_matches('\0').split_whitespace().find_map(|part| part.strip_prefix(name)?.strip_prefix(':'))
}

fn track_number(track: &str) -> Option<u32> {
    field(track, "TRACK")?.parse().ok()
}

/// Where a sector's 2048 bytes of data sit inside its frame, by track type.
fn data_start(track: &str) -> Option<u64> {
    match field(track, "TYPE")? {
        // Stored "cooked": the data is at the frame's start.
        "MODE1" | "MODE2_FORM1" => Some(0),
        // 2336 bytes: an 8-byte subheader, then the data.
        "MODE2" | "MODE2_FORM_MIX" => Some(8),
        // Whole sectors: 12 sync + 4 header (Mode 1), plus 8 of subheader (Mode 2, the PlayStation's).
        "MODE1_RAW" => Some(16),
        "MODE2_RAW" => Some(24),
        _ => None,
    }
}

/// A CD CHD's first track as 2048-byte sectors.
struct CdSectors<R> {
    inner: R,
    start: u64,
    len: u64,
    position: u64,
}

impl<R: Read + Seek> Read for CdSectors<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() || self.position >= self.len {
            return Ok(0);
        }
        let sector = self.position / SECTOR;
        let within = self.position % SECTOR;
        let count = (buf.len() as u64).min(SECTOR - within).min(self.len - self.position) as usize;
        self.inner.seek(SeekFrom::Start(sector * CD_FRAME + self.start + within))?;
        self.inner.read_exact(&mut buf[..count])?;
        self.position += count as u64;
        Ok(count)
    }
}

impl<R> Seek for CdSectors<R> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.position = seek_to(pos, self.position, self.len)?;
        Ok(self.position)
    }
}

fn seek_to(pos: SeekFrom, current: u64, len: u64) -> io::Result<u64> {
    match pos {
        SeekFrom::Start(at) => Some(at),
        SeekFrom::End(by) => len.checked_add_signed(by),
        SeekFrom::Current(by) => current.checked_add_signed(by),
    }
    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, tr!("posição inválida")))
}

/// A CSO image: a header, a table of where each block starts, then the blocks.
struct Cso<R> {
    inner: R,
    block: u64,
    align: u32,
    version: u8,
    len: u64,
    index: Vec<u32>,
    position: u64,
    /// The block now in `data`.
    loaded: Option<u64>,
    data: Vec<u8>,
    packed: Vec<u8>,
}

impl<R: Read + Seek> Cso<R> {
    fn open(mut inner: R) -> io::Result<Self> {
        let mut header = [0u8; 24];
        inner.seek(SeekFrom::Start(0))?;
        inner.read_exact(&mut header)?;
        let len = u64::from_le_bytes(header[8..16].try_into().unwrap());
        let block = u32::from_le_bytes(header[16..20].try_into().unwrap()) as u64;
        let version = header[20];
        let align = header[21] as u32;
        let sane = block.is_power_of_two() && (512..=1 << 20).contains(&block) && len <= 64 << 30 && align < 32 && version <= 2;
        if !sane {
            return Err(bad(tr!("Esse CSO tem um cabeçalho que o Cerberus não entende.")));
        }
        let blocks = len.div_ceil(block) as usize;
        let mut table = vec![0u8; (blocks + 1) * 4];
        inner.read_exact(&mut table)?;
        let index = table.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
        Ok(Cso { inner, block, align, version, len, index, position: 0, loaded: None, data: Vec::new(), packed: Vec::new() })
    }

    fn load(&mut self, n: u64) -> io::Result<()> {
        if self.loaded == Some(n) {
            return Ok(());
        }
        self.loaded = None;
        let (Some(&here), Some(&next)) = (self.index.get(n as usize), self.index.get(n as usize + 1)) else {
            return Err(bad(tr!("Bloco fora do CSO.")));
        };
        let at = ((here & 0x7fff_ffff) as u64) << self.align;
        let end = ((next & 0x7fff_ffff) as u64) << self.align;
        let size = end.checked_sub(at).ok_or_else(|| bad(tr!("CSO corrompido.")))?;
        if size > self.block * 2 + 64 {
            return Err(bad(tr!("CSO corrompido.")));
        }
        let flagged = here & 0x8000_0000 != 0;
        self.packed.resize(size as usize, 0);
        self.inner.seek(SeekFrom::Start(at))?;
        self.inner.read_exact(&mut self.packed)?;
        // Version 1 flags stored blocks; version 2 stores a block whole when
        // packing didn't help, and flags LZ4 ones.
        let stored = if self.version >= 2 { size >= self.block } else { flagged };
        self.data.clear();
        if stored {
            self.data.extend_from_slice(&self.packed[..(self.block as usize).min(self.packed.len())]);
        } else if self.version >= 2 && flagged {
            return Err(bad(tr!("Esse CSO usa LZ4, que o Cerberus ainda não abre.")));
        } else {
            flate2::read::DeflateDecoder::new(&self.packed[..]).take(self.block).read_to_end(&mut self.data)?;
        }
        self.data.resize(self.block as usize, 0);
        self.loaded = Some(n);
        Ok(())
    }
}

impl<R: Read + Seek> Read for Cso<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() || self.position >= self.len {
            return Ok(0);
        }
        let n = self.position / self.block;
        self.load(n)?;
        let within = (self.position % self.block) as usize;
        let count = buf.len().min(self.data.len() - within).min((self.len - self.position) as usize);
        buf[..count].copy_from_slice(&self.data[within..within + count]);
        self.position += count as u64;
        Ok(count)
    }
}

impl<R> Seek for Cso<R> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.position = seek_to(pos, self.position, self.len)?;
        Ok(self.position)
    }
}

/// Writes the disc's plain data to `to` (an ISO), for a recomp that wants the
/// image itself. Returns the bytes written.
pub fn write_iso(image: &Path, to: &Path) -> Result<u64, String> {
    let mut disc = open(image).map_err(|e| tr!("Não foi possível abrir a imagem de disco: {}", e))?;
    let part = to.with_extension("iso.part");
    let result = File::create(&part).and_then(|mut out| io::copy(&mut disc, &mut out));
    match result {
        Ok(bytes) => {
            std::fs::rename(&part, to).map_err(|e| e.to_string())?;
            Ok(bytes)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            Err(tr!("Não foi possível converter a imagem de disco: {}", e))
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("cerberus-disc-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A made-up disc: each sector filled with its own number.
    fn disc(sectors: u64) -> Vec<u8> {
        (0..sectors).flat_map(|n| vec![n as u8; SECTOR as usize]).collect()
    }

    /// Packs `data` as a version 1 CSO: blocks deflated, except odd ones stored.
    pub(crate) fn cso(data: &[u8]) -> Vec<u8> {
        let blocks: Vec<&[u8]> = data.chunks(SECTOR as usize).collect();
        let header_and_index = 24 + (blocks.len() + 1) * 4;
        let mut body = Vec::new();
        let mut index = Vec::new();
        for (n, block) in blocks.iter().enumerate() {
            let at = (header_and_index + body.len()) as u32;
            if n % 2 == 1 {
                index.push(at | 0x8000_0000);
                body.extend_from_slice(block);
            } else {
                index.push(at);
                let mut z = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
                z.write_all(block).unwrap();
                body.extend(z.finish().unwrap());
            }
        }
        index.push((header_and_index + body.len()) as u32);
        let mut out = b"CISO".to_vec();
        out.extend(24u32.to_le_bytes());
        out.extend((data.len() as u64).to_le_bytes());
        out.extend((SECTOR as u32).to_le_bytes());
        out.extend([1, 0, 0, 0]);
        for i in index {
            out.extend(i.to_le_bytes());
        }
        out.extend(body);
        out
    }

    #[test]
    fn a_cso_reads_as_the_disc_it_packs() {
        let dir = temp("cso");
        let plain = disc(7);
        let path = dir.join("game.cso");
        std::fs::write(&path, cso(&plain)).unwrap();
        assert_eq!(kind_of(&path), Kind::Cso);
        let mut all = Vec::new();
        open(&path).unwrap().read_to_end(&mut all).unwrap();
        assert_eq!(all, plain);
        // Reading from the middle, across blocks.
        let mut image = open(&path).unwrap();
        image.seek(SeekFrom::Start(3 * SECTOR - 2)).unwrap();
        let mut four = [0u8; 4];
        image.read_exact(&mut four).unwrap();
        assert_eq!(four, [2, 2, 3, 3]);
        let iso = dir.join("game.iso");
        assert_eq!(write_iso(&path, &iso).unwrap(), plain.len() as u64);
        assert_eq!(std::fs::read(&iso).unwrap(), plain);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_broken_cso_is_refused() {
        let dir = temp("badcso");
        let path = dir.join("bad.cso");
        let mut bytes = cso(&disc(2));
        bytes[16..20].copy_from_slice(&3u32.to_le_bytes());
        std::fs::write(&path, bytes).unwrap();
        assert!(open(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cd_frames_give_their_data() {
        // Three Mode 2 frames: sync/header/subheader, then the sector's data, then the rest and subcode.
        let mut frames = Vec::new();
        for n in 0..3u8 {
            frames.extend(vec![0xEE; 24]);
            frames.extend(vec![n + 1; SECTOR as usize]);
            frames.extend(vec![0xEE; (CD_FRAME - 24 - SECTOR) as usize]);
        }
        let mut cd = CdSectors { inner: io::Cursor::new(frames), start: 24, len: 3 * SECTOR, position: 0 };
        let mut all = Vec::new();
        cd.read_to_end(&mut all).unwrap();
        assert_eq!(all, disc(3).iter().map(|b| b + 1).collect::<Vec<u8>>());
    }

    #[test]
    fn track_types_say_where_the_data_is() {
        let track = "TRACK:1 TYPE:MODE2_RAW SUBTYPE:NONE FRAMES:1234 PREGAP:0 PGTYPE:MODE1 PGSUB:RW POSTGAP:0\0";
        assert_eq!(track_number(track), Some(1));
        assert_eq!(data_start(track), Some(24));
        assert_eq!(field(track, "FRAMES"), Some("1234"));
        assert_eq!(data_start("TRACK:1 TYPE:MODE1_RAW"), Some(16));
        assert_eq!(data_start("TRACK:1 TYPE:MODE1"), Some(0));
        assert_eq!(data_start("TRACK:1 TYPE:AUDIO"), None);
    }

    pub(crate) fn write_chd(path: &Path, options: chd::write::ChdWriteOptions, data: &[u8], track: Option<&str>) {
        let mut writer = chd::write::ChdWriter::new(File::create(path).unwrap(), options).unwrap();
        if let Some(track) = track {
            writer.set_metadata(chd::metadata::KnownMetadata::CdRomTrack2, 0, track.as_bytes(), true).unwrap();
        }
        writer.write_all(data).unwrap();
        writer.finish().unwrap();
    }

    #[test]
    fn a_dvd_chd_reads_as_its_iso() {
        let dir = temp("dvdchd");
        let plain = disc(9);
        let path = dir.join("game.chd");
        write_chd(&path, chd::write::ChdWriteOptions::dvd(), &plain, None);
        assert_eq!(kind_of(&path), Kind::Chd);
        let mut all = Vec::new();
        open(&path).unwrap().read_to_end(&mut all).unwrap();
        assert_eq!(all, plain);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_cd_chd_reads_as_its_data_sectors() {
        let dir = temp("cdchd");
        let mut frames = Vec::new();
        for n in 0..5u8 {
            frames.extend(vec![0xEE; 24]);
            frames.extend(vec![n; SECTOR as usize]);
            frames.extend(vec![0xEE; (CD_FRAME - 24 - SECTOR) as usize]);
        }
        let path = dir.join("game.chd");
        let track = "TRACK:1 TYPE:MODE2_RAW SUBTYPE:NONE FRAMES:5 PREGAP:0 PGTYPE:MODE1 PGSUB:RW POSTGAP:0\0";
        write_chd(&path, chd::write::ChdWriteOptions::cd(), &frames, Some(track));
        let mut all = Vec::new();
        open(&path).unwrap().read_to_end(&mut all).unwrap();
        assert_eq!(all, disc(5));
        let audio = dir.join("audio.chd");
        write_chd(&audio, chd::write::ChdWriteOptions::cd(), &frames, Some("TRACK:1 TYPE:AUDIO SUBTYPE:NONE FRAMES:5\0"));
        assert!(open(&audio).is_err(), "a music CD has no files");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_plain_iso_is_the_file_itself() {
        let dir = temp("plain");
        let path = dir.join("game.iso");
        std::fs::write(&path, disc(2)).unwrap();
        assert_eq!(kind_of(&path), Kind::Plain);
        let mut all = Vec::new();
        open(&path).unwrap().read_to_end(&mut all).unwrap();
        assert_eq!(all, disc(2));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
