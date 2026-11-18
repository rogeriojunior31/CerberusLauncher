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
}

impl ArtCache {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, busy: Mutex::new(HashSet::new()) }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn file_for(&self, url: &str) -> PathBuf {
        let name = format!("{:x}", Sha256::digest(url.as_bytes()));
        self.dir.join(&name[..32])
    }

    /// Replaces an online address with its copy on disk when there is one;
    /// otherwise notes the address in `missing`.
    fn swap(&self, value: &mut Option<String>, missing: &mut Vec<String>) {
        let Some(url) = value.as_ref().filter(|v| v.starts_with("http")) else {
            return;
        };
        let file = self.file_for(url);
        if file.is_file() {
            *value = Some(file.to_string_lossy().into_owned());
            return;
        }
        missing.push(url.clone());
    }

    /// The online art each game shows first: art already on disk (Steam's own
    /// cache, a cover the player picked) is left alone.
    pub fn use_copies_for_games(&self, games: &mut [Game], missing: &mut Vec<String>) {
        for game in games {
            if game.cover_path.is_none() {
                self.swap(&mut game.cover_url, missing);
            }
            if game.hero_path.is_none() {
                self.swap(&mut game.hero_url, missing);
            }
        }
    }

    pub fn use_copies_for_recomps(&self, entries: &mut [RecompEntry], missing: &mut Vec<String>) {
        for entry in entries {
            self.swap(&mut entry.cover, missing);
            self.swap(&mut entry.hero, missing);
            self.swap(&mut entry.icon, missing);
        }
    }

    /// Fetches what's missing, one at a time, in the background.
    pub fn fetch_later(self: &Arc<Self>, client: Client, urls: Vec<String>) {
        let fresh: Vec<String> = {
            let mut busy = self.busy.lock().unwrap_or_else(|p| p.into_inner());
            urls.into_iter().filter(|url| busy.insert(url.clone())).collect()
        };
        if fresh.is_empty() {
            return;
        }
        let cache = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            for url in fresh {
                // A picture that won't come now is tried again on the next listing.
                let _ = cache.fetch(&client, &url).await;
                cache.busy.lock().unwrap_or_else(|p| p.into_inner()).remove(&url);
            }
        });
    }

    async fn fetch(&self, client: &Client, url: &str) -> Result<(), String> {
        let reply = client.get(url).send().await.map_err(|e| e.to_string())?;
        if !reply.status().is_success() {
            return Err(format!("{} answered {}", url, reply.status()));
        }
        let bytes = reply.bytes().await.map_err(|e| e.to_string())?;
        if bytes.len() > MAX_BYTES || !is_image(&bytes) {
            return Err(format!("{url} is not a picture"));
        }
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let file = self.file_for(url);
        let temp = file.with_extension("part");
        fs::write(&temp, &bytes).map_err(|e| e.to_string())?;
        fs::rename(&temp, &file).map_err(|e| e.to_string())
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
    use super::*;

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
        cache.swap(&mut value, &mut missing);
        assert!(value.unwrap().starts_with(&dir.to_string_lossy().into_owned()));
        assert!(missing.is_empty());
    }

    #[test]
    fn notes_what_is_missing() {
        let cache = ArtCache::new(std::env::temp_dir().join("cerberus-art-empty"));
        let mut value = Some("https://example.com/nothing.png".to_string());
        let mut missing = Vec::new();
        cache.swap(&mut value, &mut missing);
        assert_eq!(missing, vec!["https://example.com/nothing.png"]);
        assert!(value.unwrap().starts_with("https://"));
    }

    #[test]
    fn leaves_paths_alone() {
        let cache = ArtCache::new(std::env::temp_dir().join("cerberus-art-paths"));
        let mut value = Some(r"C:\Steam\appcache\librarycache\620\library_hero.jpg".to_string());
        let mut missing = Vec::new();
        cache.swap(&mut value, &mut missing);
        assert!(missing.is_empty());
    }
}
