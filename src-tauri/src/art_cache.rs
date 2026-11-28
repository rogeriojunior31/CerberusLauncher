//! Game art kept on disk: covers and backdrops are downloaded once, then shown
//! from the copy, so they appear at once, offline too, and no page asks a
//! server for twenty pictures at the same moment.
//!
//! Lists hand the UI the copy's path when there is one, and the online address
//! otherwise; the addresses still missing are fetched in the background for next
//! time. A UI value starting with `http` is online, anything else is a path.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use cerberus_shared::{Game, RecompEntry};
use reqwest::Client;
use sha2::{Digest, Sha256};

/// Art bigger than this is not a cover; it's a mistake or something else.
const MAX_BYTES: usize = 16 * 1024 * 1024;

pub struct ArtCache {
    dir: PathBuf,
    /// Addresses being fetched now, so two listings don't fetch the same one twice.
    busy: Mutex<HashSet<String>>,
    /// The folder's file names, with the folder's time when listed.
    listed: Mutex<Option<(std::time::SystemTime, std::collections::HashSet<std::ffi::OsString>)>>,
    /// Sites whose pictures failed, already said this run (one line per site, not per picture).
    told: Mutex<HashSet<String>>,
    /// Where a failure is said (the history), set once the app is up.
    on_failure: std::sync::OnceLock<Box<dyn Fn(&str, usize, &str) + Send + Sync>>,
    /// Addresses the site said don't exist (404, 410), and when: not asked
    /// again for a week, so a picture taken down isn't fetched (and reported)
    /// on every start. Kept beside the folder.
    gone: Mutex<std::collections::HashMap<String, u64>>,
}

/// How long an address that doesn't exist (404, 410) is left alone.
const GONE_FOR: u64 = 7 * 86_400;

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default()
}

/// The sites, among `failed` (address, reason), not said yet: each with how
/// many of its pictures failed and the first reason.
fn sites_to_tell(failed: &[(String, String)], told: &mut HashSet<String>) -> Vec<(String, usize, String)> {
    let mut out: Vec<(String, usize, String)> = Vec::new();
    for (url, why) in failed {
        let site = url.split("://").nth(1).and_then(|r| r.split('/').next()).unwrap_or(url).to_lowercase();
        match out.iter_mut().find(|(s, _, _)| *s == site) {
            Some(entry) => entry.1 += 1,
            None if !told.contains(&site) => out.push((site, 1, why.clone())),
            None => {}
        }
    }
    for (site, _, _) in &out {
        told.insert(site.clone());
    }
    out
}

impl ArtCache {
    pub fn new(dir: PathBuf) -> Self {
        let gone = fs::read_to_string(Self::gone_file_in(&dir)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        Self { dir, busy: Mutex::new(HashSet::new()), listed: Mutex::new(None), told: Mutex::new(HashSet::new()), on_failure: std::sync::OnceLock::new(), gone: Mutex::new(gone) }
    }

    /// Beside the art folder, not in it (the folder's names are the pictures).
    fn gone_file_in(dir: &Path) -> PathBuf {
        dir.with_file_name("art-gone.json")
    }

    /// The address was said not to exist lately.
    fn is_gone(&self, url: &str, now: u64) -> bool {
        self.gone.lock().unwrap_or_else(|p| p.into_inner()).get(url).is_some_and(|at| now < at + GONE_FOR)
    }

    fn mark_gone(&self, urls: &[String], now: u64) {
        if urls.is_empty() {
            return;
        }
        let mut gone = self.gone.lock().unwrap_or_else(|p| p.into_inner());
        gone.retain(|_, at| now < *at + GONE_FOR);
        for url in urls {
            gone.insert(url.clone(), now);
        }
        let _ = crate::state_file::write_bytes(&Self::gone_file_in(&self.dir), serde_json::to_string(&*gone).unwrap_or_default().as_bytes());
    }

    /// Says pictures that wouldn't come (`site`, how many, why), once per site.
    pub fn report_failures_with(&self, say: impl Fn(&str, usize, &str) + Send + Sync + 'static) {
        let _ = self.on_failure.set(Box::new(say));
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn file_for(&self, url: &str) -> PathBuf {
        let name = format!("{:x}", Sha256::digest(url.as_bytes()));
        self.dir.join(&name[..32])
    }

    /// The copy on disk of an online picture, when there is one.
    pub fn copy_of(&self, url: &str) -> Option<PathBuf> {
        Some(self.file_for(url)).filter(|f| f.is_file())
    }

    /// Replaces an online address with its copy on disk when there is one;
    /// otherwise notes the address in `missing`.
    /// The pictures on disk, listed once (a listing of a thousand games would
    /// otherwise ask the disk about each picture).
    /// The pictures in the folder: listed again only when the folder changed
    /// (thousands of names, asked for on every library listing).
    fn on_disk(&self) -> std::collections::HashSet<std::ffi::OsString> {
        let changed = std::fs::metadata(&self.dir).and_then(|m| m.modified()).ok();
        let mut kept = self.listed.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((at, names)) = kept.as_ref() {
            if changed.is_some() && Some(*at) == changed {
                return names.clone();
            }
        }
        let names: std::collections::HashSet<std::ffi::OsString> = std::fs::read_dir(&self.dir)
            .map(|entries| entries.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_file())).map(|e| e.file_name()).collect())
            .unwrap_or_default();
        if let Some(at) = changed {
            *kept = Some((at, names.clone()));
        }
        names
    }

    fn swap(&self, value: &mut Option<String>, missing: &mut Vec<String>, here: &std::collections::HashSet<std::ffi::OsString>) {
        let Some(url) = value.as_ref().filter(|v| v.starts_with("http")) else {
            return;
        };
        let file = self.file_for(url);
        if file.file_name().is_some_and(|n| here.contains(n)) {
            *value = Some(file.to_string_lossy().into_owned());
            return;
        }
        missing.push(url.clone());
    }

    /// The online art each game shows first: art already on disk (Steam's own
    /// cache, a cover the player picked) is left alone.
    pub fn use_copies_for_games(&self, games: &mut [Game], missing: &mut Vec<String>) {
        let here = self.on_disk();
        for game in games {
            if game.cover_path.is_none() {
                self.swap(&mut game.cover_url, missing, &here);
            }
            if game.hero_path.is_none() {
                self.swap(&mut game.hero_url, missing, &here);
            }
        }
    }

    pub fn use_copies_for_recomps(&self, entries: &mut [RecompEntry], missing: &mut Vec<String>) {
        let here = self.on_disk();
        for entry in entries {
            self.swap(&mut entry.cover, missing, &here);
            self.swap(&mut entry.hero, missing, &here);
            self.swap(&mut entry.icon, missing, &here);
        }
    }

    /// Fetches what's missing, one at a time, in the background.
    pub fn fetch_later(self: &Arc<Self>, client: Client, urls: Vec<String>) {
        let now = now_secs();
        let fresh: Vec<String> = {
            let mut busy = self.busy.lock().unwrap_or_else(|p| p.into_inner());
            urls.into_iter().filter(|url| !self.is_gone(url, now) && busy.insert(url.clone())).collect()
        };
        if fresh.is_empty() {
            return;
        }
        let cache = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            let mut failed = Vec::new();
            let mut gone = Vec::new();
            for url in fresh {
                // A picture that won't come now is tried again on the next listing;
                // one the site says doesn't exist, only after a week.
                match cache.fetch(&client, &url).await {
                    Err(Fetch::Gone(why)) => {
                        gone.push(url.clone());
                        failed.push((url.clone(), why));
                    }
                    Err(Fetch::Failed(why)) => failed.push((url.clone(), why)),
                    Ok(()) => {}
                }
                cache.busy.lock().unwrap_or_else(|p| p.into_inner()).remove(&url);
            }
            cache.mark_gone(&gone, now_secs());
            let tell = sites_to_tell(&failed, &mut cache.told.lock().unwrap_or_else(|p| p.into_inner()));
            if let Some(say) = cache.on_failure.get() {
                for (site, count, why) in tell {
                    say(&site, count, &why);
                }
            }
        });
    }

    async fn fetch(&self, client: &Client, url: &str) -> Result<(), Fetch> {
        let failed = |e: String| Fetch::Failed(e);
        let reply = crate::net::send(client.get(url)).await.map_err(|e| failed(e.to_string()))?;
        let status = reply.status().as_u16();
        if !reply.status().is_success() {
            let why = cerberus_shared::tr!("o site respondeu {}", status);
            return Err(if matches!(status, 404 | 410) { Fetch::Gone(why) } else { Fetch::Failed(why) });
        }
        let bytes = crate::net::read_limited(reply, MAX_BYTES as u64).await.map_err(failed)?;
        if !is_image(&bytes) {
            return Err(Fetch::Failed(cerberus_shared::tr!("não veio uma imagem").into()));
        }
        fs::create_dir_all(&self.dir).map_err(|e| failed(e.to_string()))?;
        let file = self.file_for(url);
        let temp = file.with_extension("part");
        fs::write(&temp, &bytes).map_err(|e| failed(e.to_string()))?;
        fs::rename(&temp, &file).map_err(|e| failed(e.to_string()))?;
        // A new picture: the folder is listed again next time.
        *self.listed.lock().unwrap_or_else(|p| p.into_inner()) = None;
        Ok(())
    }

    /// Deletes every copy; they come back as pages need them.
    pub fn clear(&self) -> Result<u64, String> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Ok(0);
        };
        let mut freed = 0;
        for entry in entries.flatten() {
            freed += entry.metadata().map(|m| m.len()).unwrap_or(0);
            let _ = fs::remove_file(entry.path());
        }
        Ok(freed)
    }
}

/// Why a picture didn't come: the site says it doesn't exist (left alone for a
/// week), or anything else (tried again next time).
enum Fetch {
    Gone(String),
    Failed(String),
}

/// PNG, JPEG or WebP, by their first bytes. Some hosts answer 200 with a small
/// text file (a link to another file, an error page), which must not become a cover.
fn is_image(bytes: &[u8]) -> bool {
    let png = bytes.starts_with(&[0x89, b'P', b'N', b'G']);
    let jpeg = bytes.starts_with(&[0xFF, 0xD8, 0xFF]);
    let webp = bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP";
    png || jpeg || webp
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_picture_that_does_not_exist_is_left_alone_for_a_week_across_starts() {
        let dir = tempfile::tempdir().unwrap();
        let art = dir.path().join("art");
        let cache = super::ArtCache::new(art.clone());
        let url = "https://shared.akamai.steamstatic.com/x/header.jpg".to_string();
        cache.mark_gone(&[url.clone()], 1_000);
        assert!(cache.is_gone(&url, 1_000 + 60));
        // Another start reads it back.
        let again = super::ArtCache::new(art);
        assert!(again.is_gone(&url, 1_000 + 3600), "kept across starts");
        assert!(!again.is_gone(&url, 1_000 + super::GONE_FOR), "asked again after a week");
        assert!(!again.is_gone("https://other/y.jpg", 1_000));
    }
    use super::*;

    #[test]
    fn the_kept_listing_sees_a_picture_added_to_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ArtCache::new(dir.path().to_path_buf());
        assert!(cache.on_disk().is_empty());
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(dir.path().join("a.jpg"), b"x").unwrap();
        assert!(cache.on_disk().contains(std::ffi::OsStr::new("a.jpg")), "the folder changed: listed again");
        assert_eq!(cache.on_disk().len(), 1);
    }

    #[test]
    fn a_failing_site_is_said_once_not_per_picture() {
        let mut told = HashSet::new();
        let failed = vec![
            ("https://cdn.example.com/a.jpg".to_string(), "timeout".to_string()),
            ("https://cdn.example.com/b.jpg".to_string(), "timeout".to_string()),
            ("https://other.org/c.png".to_string(), "404".to_string()),
        ];
        let said = sites_to_tell(&failed, &mut told);
        assert_eq!(said, vec![("cdn.example.com".to_string(), 2, "timeout".to_string()), ("other.org".to_string(), 1, "404".to_string())]);
        assert!(sites_to_tell(&failed, &mut told).is_empty(), "said already this run");
    }

    #[test]
    fn knows_pictures_from_text() {
        assert!(is_image(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A]));
        assert!(is_image(&[0xFF, 0xD8, 0xFF, 0xE0]));
        assert!(is_image(b"RIFF\x10\x00\x00\x00WEBPVP8 "));
        assert!(!is_image(b"Star Fox 64 (USA).png"));
        assert!(!is_image(b"<!DOCTYPE html>"));
    }

    #[test]
    fn uses_the_copy_when_there_is_one() {
        let dir = std::env::temp_dir().join(format!("cerberus-art-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let cache = ArtCache::new(dir.clone());
        let url = "https://example.com/cover.png".to_string();
        fs::write(cache.file_for(&url), b"\x89PNG").unwrap();

        let mut value = Some(url);
        let mut missing = Vec::new();
        cache.swap(&mut value, &mut missing, &cache.on_disk());
        assert!(value.unwrap().starts_with(&dir.to_string_lossy().into_owned()));
        assert!(missing.is_empty());
    }

    #[test]
    fn notes_what_is_missing() {
        let cache = ArtCache::new(std::env::temp_dir().join("cerberus-art-empty"));
        let mut value = Some("https://example.com/nothing.png".to_string());
        let mut missing = Vec::new();
        cache.swap(&mut value, &mut missing, &cache.on_disk());
        assert_eq!(missing, vec!["https://example.com/nothing.png"]);
        assert!(value.unwrap().starts_with("https://"));
    }

    #[test]
    fn leaves_paths_alone() {
        let cache = ArtCache::new(std::env::temp_dir().join("cerberus-art-paths"));
        let mut value = Some(r"C:\Steam\appcache\librarycache\620\library_hero.jpg".to_string());
        let mut missing = Vec::new();
        cache.swap(&mut value, &mut missing, &cache.on_disk());
        assert!(missing.is_empty());
    }
}
