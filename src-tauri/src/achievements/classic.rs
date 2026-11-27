//! Which RetroAchievements game a ROM from the player's folders is, so its page
//! can show the player's achievements.
//!
//! Cartridge ROMs are matched by their MD5, the way RetroAchievements hashes
//! them (no copier/iNES header; Nintendo 64 put in big-endian order). Disc
//! images and anything without a hash match fall back to the title. The game
//! list per console is kept for a month, and each ROM's match is remembered.

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// RetroAchievements' console number for Cerberus's console names.
pub fn console_id(console: &str) -> Option<u32> {
    Some(match console {
        "genesis" => 1,
        "n64" => 2,
        "snes" => 3,
        "gb" => 4,
        "gba" => 5,
        "gbc" => 6,
        "nes" => 7,
        "32x" => 10,
        "sms" => 11,
        "ps1" => 12,
        "gg" => 15,
        "gc" => 16,
        "ds" => 18,
        "ps2" => 21,
        "saturn" => 39,
        "dc" => 40,
        "psp" => 41,
        _ => return None,
    })
}

/// Consoles whose ROM is hashed whole (after the fixes below), as RetroAchievements does.
fn hashed_whole(console: &str) -> bool {
    matches!(console, "nes" | "snes" | "n64" | "gb" | "gbc" | "gba" | "genesis" | "32x" | "sms" | "gg")
}

/// The ROM's bytes as RetroAchievements hashes them.
pub fn hash_bytes(console: &str, mut data: Vec<u8>) -> String {
    match console {
        // iNES header: "NES\x1A" and 16 bytes.
        "nes" if data.starts_with(b"NES\x1a") && data.len() > 16 => {
            data.drain(..16);
        }
        // A copier header: 512 extra bytes.
        "snes" if data.len() % 1024 == 512 => {
            data.drain(..512);
        }
        "n64" if data.len() >= 4 => match data[..4] {
            // .v64: byte-swapped in pairs.
            [0x37, 0x80, 0x40, 0x12] => data.chunks_exact_mut(2).for_each(|c| c.swap(0, 1)),
            // .n64: little-endian words.
            [0x40, 0x12, 0x37, 0x80] => data.chunks_exact_mut(4).for_each(|c| c.reverse()),
            _ => {}
        },
        _ => {}
    }
    format!("{:x}", Md5::digest(&data))
}

/// A title reduced to compare names: lower case, no "(USA)" or "[!]", no
/// punctuation, "the" and articles kept out.
pub fn plain_title(name: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in name.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth > 0 => {}
            c if c.is_alphanumeric() => out.extend(c.to_lowercase()),
            _ => out.push(' '),
        }
    }
    let words: Vec<&str> = out.split_whitespace().filter(|w| !matches!(*w, "the" | "a" | "an")).collect();
    words.join(" ")
}

#[derive(Serialize, Deserialize, Clone)]
struct ListGame {
    id: u64,
    title: String,
    hashes: Vec<String>,
}

fn cache(data_dir: &Path) -> PathBuf {
    data_dir.join("cache").join("achievements")
}

async fn game_list(http: &reqwest::Client, data_dir: &Path, user: &str, key: &str, console: u32) -> Result<Vec<ListGame>, String> {
    let file = cache(data_dir).join(format!("ra-list-{console}.json"));
    let fresh = fs::metadata(&file).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|a| a < Duration::from_secs(30 * 24 * 3600));
    if fresh {
        if let Some(list) = fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok()) {
            return Ok(list);
        }
    }
    let v = crate::retroachievements::get(http, "API_GetGameList.php", user, key, &format!("&i={console}&h=1&f=1")).await?;
    let list: Vec<ListGame> = v
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|g| {
            Some(ListGame {
                id: g.get("ID")?.as_u64()?,
                title: g.get("Title").and_then(Value::as_str).unwrap_or_default().to_string(),
                hashes: g.get("Hashes").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_lowercase).collect(),
            })
        })
        .collect();
    let _ = fs::create_dir_all(cache(data_dir));
    let _ = crate::state_file::write_bytes(&file, serde_json::to_string(&list).unwrap_or_default().as_bytes());
    Ok(list)
}

/// The best game for a ROM: its hash, or else its title (hacks and demos last).
fn pick(list: &[ListGame], hash: Option<&str>, title: &str) -> Option<u64> {
    if let Some(h) = hash {
        if let Some(g) = list.iter().find(|g| g.hashes.iter().any(|x| x == h)) {
            return Some(g.id);
        }
    }
    let want = plain_title(title);
    if want.is_empty() {
        return None;
    }
    let mut best: Option<(u32, u64)> = None;
    for g in list {
        let have = plain_title(&g.title);
        let score = if have == want {
            3
        } else if have.split(" | ").any(|part| part == want) || want.starts_with(&format!("{have} ")) {
            2
        } else {
            continue;
        };
        // "~Hack~", "~Demo~" and friends only when nothing else fits.
        let score = if g.title.starts_with('~') { score - 1 } else { score };
        if best.is_none_or(|(s, _)| score > s) {
            best = Some((score, g.id));
        }
    }
    best.filter(|(s, _)| *s >= 2).map(|(_, id)| id)
}

/// What was found for a ROM, remembered with the file it was found for.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy)]
struct Match {
    id: Option<u64>,
    /// By the file's hash (sure), else by its title (probably).
    by_hash: bool,
    size: u64,
    modified: u64,
    /// Unix seconds when it was looked up.
    at: u64,
}

/// "Not found" is asked again after this long: RetroAchievements adds games and hashes.
const NOT_FOUND_FOR: u64 = 30 * 24 * 3600;

/// The RetroAchievements game id for a ROM, and whether it was found by the
/// file's hash; remembered for that file (a changed file is looked up again,
/// and "not found" only for 30 days).
pub async fn game_for(http: &reqwest::Client, data_dir: &Path, user: &str, key: &str, path: &Path, console: &str, titles: &[String]) -> Result<Option<(u64, bool)>, String> {
    let memo_file = cache(data_dir).join("ra-matches-v2.json");
    let mut memo: HashMap<String, Match> = fs::read_to_string(&memo_file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let memo_key = path.to_string_lossy().to_string();
    let meta = fs::metadata(path).ok();
    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
    let modified = meta.and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    if let Some(known) = memo.get(&memo_key) {
        let same_file = known.size == size && known.modified == modified;
        let still_good = known.id.is_some() || now.saturating_sub(known.at) < NOT_FOUND_FOR;
        if same_file && still_good {
            return Ok(known.id.map(|id| (id, known.by_hash)));
        }
    }
    let Some(ra_console) = console_id(console) else { return Ok(None) };
    let list = game_list(http, data_dir, user, key, ra_console).await?;
    let hash = if hashed_whole(console) && fs::metadata(path).map(|m| m.len()).unwrap_or(u64::MAX) <= 64 * 1024 * 1024 {
        let path = path.to_path_buf();
        let console = console.to_string();
        tauri::async_runtime::spawn_blocking(move || {
            let mut data = Vec::new();
            fs::File::open(&path).ok()?.read_to_end(&mut data).ok()?;
            Some(hash_bytes(&console, data))
        })
        .await
        .ok()
        .flatten()
    } else {
        None
    };
    // By the hash first; the title only when the hash isn't listed.
    let by_hash = hash.as_deref().and_then(|h| pick(&list, Some(h), ""));
    let found = by_hash.or_else(|| titles.iter().find_map(|t| pick(&list, None, t)));
    memo.insert(memo_key, Match { id: found, by_hash: by_hash.is_some(), size, modified, at: now });
    let _ = fs::create_dir_all(cache(data_dir));
    let _ = crate::state_file::write_bytes(&memo_file, serde_json::to_string(&memo).unwrap_or_default().as_bytes());
    Ok(found.map(|id| (id, by_hash.is_some())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_and_byte_order_are_undone_before_hashing() {
        let body = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
        let mut nes = b"NES\x1a".to_vec();
        nes.extend([0u8; 12]);
        nes.extend(&body);
        assert_eq!(hash_bytes("nes", nes), hash_bytes("gb", body.clone()));
        let z64 = vec![0x80, 0x37, 0x12, 0x40, 9, 10, 11, 12];
        let v64 = vec![0x37, 0x80, 0x40, 0x12, 10, 9, 12, 11];
        let n64 = vec![0x40, 0x12, 0x37, 0x80, 12, 11, 10, 9];
        assert_eq!(hash_bytes("n64", v64), hash_bytes("n64", z64.clone()));
        assert_eq!(hash_bytes("n64", n64), hash_bytes("n64", z64));
    }

    #[test]
    fn titles_match_without_regions_or_punctuation() {
        assert_eq!(plain_title("The Legend of Zelda - Ocarina of Time (USA) (Rev 1)"), "legend of zelda ocarina of time");
        let list = vec![
            ListGame { id: 1, title: "~Hack~ Legend of Zelda, The: Ocarina of Time".into(), hashes: vec![] },
            ListGame { id: 2, title: "Legend of Zelda: Ocarina of Time".into(), hashes: vec!["abc".into()] },
            ListGame { id: 3, title: "Super Mario 64".into(), hashes: vec!["fff".into()] },
        ];
        assert_eq!(pick(&list, Some("fff"), "whatever"), Some(3));
        assert_eq!(pick(&list, None, "The Legend of Zelda - Ocarina of Time (USA)"), Some(2));
        assert_eq!(pick(&list, None, "Unknown Game"), None);
    }
}

#[cfg(test)]
mod real {
    /// Matches this PC's ROMs with the saved RetroAchievements account (read only):
    /// `cargo test -p cerberus-launcher real_classics -- --ignored --nocapture`.
    #[test]
    #[ignore = "uses this PC's ROMs and RetroAchievements account"]
    fn real_classics_find_their_games() {
        let data = std::path::PathBuf::from(std::env::var("APPDATA").unwrap()).join("app.cerberus.launcher");
        let s: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(data.join("settings.json")).unwrap()).unwrap();
        let (user, key) = (s["ra_user"].as_str().unwrap().to_string(), s["ra_key"].as_str().unwrap().to_string());
        let dir = tempfile::tempdir().unwrap();
        let http = reqwest::Client::builder().user_agent("CerberusLauncher").build().unwrap();
        for (path, console, title) in [
            (r"D:\ROMS\Nintendo 64\Mario Kart 64 (USA)\Mario Kart 64 (USA).z64", "n64", "MARIOKART64"),
            (r"D:\ROMS\Playstation\Crash Bandicoot (USA)\Crash Bandicoot (USA).cue", "ps1", "Crash Bandicoot (USA)"),
            (r"D:\ROMS\Playstation 2\Dragon Ball Z - Budokai Tenkaichi 3 (USA) (EnJa)\Dragon Ball Z - Budokai Tenkaichi 3 (USA) (En,Ja).iso", "ps2", "Dragon Ball Z - Budokai Tenkaichi 3 (USA) (En,Ja)"),
        ] {
            let p = std::path::Path::new(path);
            let stem = p.file_stem().unwrap().to_string_lossy().to_string();
            let id = tauri::async_runtime::block_on(super::game_for(&http, dir.path(), &user, &key, p, console, &[title.into(), stem]));
            println!("{console} {title} -> {id:?}");
        }
    }
}
