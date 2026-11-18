//! Release info from GitHub, kept on disk for six hours.
//!
//! Without a login GitHub answers 60 requests an hour per IP, and Discover asks
//! about every recomp at once, so asking again on every visit would run out fast.
//! The copy on disk also keeps Discover working offline.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use reqwest::{Client, StatusCode};
use serde::Deserialize;

use super::assets::Asset;

const FRESH_FOR: Duration = Duration::from_secs(6 * 60 * 60);

pub struct LatestRelease {
    pub version: String,
    pub assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    size: u64,
    browser_download_url: String,
    /// "sha256:<hex>", added by GitHub to uploads from mid-2025 on.
    digest: Option<String>,
}

/// The newest full release (pre-releases are betas and drafts are unpublished).
/// `fresh`: skip the copy on disk, for an install or update about to happen.
pub async fn latest(client: &Client, cache_dir: &Path, repo: &str, fresh: bool) -> Result<LatestRelease, String> {
    let file = cache_file(cache_dir, repo);
    let cached = fs::read_to_string(&file).ok();
    let young = age(&file).is_some_and(|age| age < FRESH_FOR);
    let text = match cached {
        Some(text) if young && !fresh => text,
        cached => match fetch(client, repo).await {
            Ok(text) => {
                let _ = fs::create_dir_all(cache_dir);
                let _ = fs::write(&file, &text);
                text
            }
            // Old info beats none: its files are usually still there to download.
            Err(error) => cached.ok_or(error)?,
        },
    };
    parse_latest(&text)
}

async fn fetch(client: &Client, repo: &str) -> Result<String, String> {
    let url = format!("https://api.github.com/repos/{repo}/releases?per_page=10");
    let request = client.get(url).header("Accept", "application/vnd.github+json");
    let reply = request.send().await.map_err(|_| "Sem conexão com o GitHub.".to_string())?;
    let status = reply.status();
    if status == StatusCode::FORBIDDEN || status == StatusCode::TOO_MANY_REQUESTS {
        return Err("O GitHub limitou as consultas por agora. Tente de novo em alguns minutos.".into());
    }
    if !status.is_success() {
        return Err(format!("O GitHub respondeu {}.", status.as_u16()));
    }
    reply.text().await.map_err(|e| e.to_string())
}

fn parse_latest(text: &str) -> Result<LatestRelease, String> {
    let releases: Vec<GhRelease> = serde_json::from_str(text).map_err(|_| "Resposta inesperada do GitHub.".to_string())?;
    let full = releases.into_iter().find(|r| !r.draft && !r.prerelease);
    let release = full.ok_or("Esse projeto ainda não tem uma versão publicada.")?;
    let assets = release.assets.into_iter().map(to_asset).collect();
    Ok(LatestRelease { version: release.tag_name, assets })
}

fn to_asset(asset: GhAsset) -> Asset {
    let sha256 = asset.digest.and_then(|d| d.strip_prefix("sha256:").map(str::to_lowercase));
    Asset { name: asset.name, size: asset.size, url: asset.browser_download_url, sha256 }
}

fn cache_file(dir: &Path, repo: &str) -> PathBuf {
    dir.join(format!("{}.json", repo.replace('/', "__")))
}

fn age(file: &Path) -> Option<Duration> {
    let modified = fs::metadata(file).and_then(|m| m.modified()).ok()?;
    SystemTime::now().duration_since(modified).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_betas_and_drafts() {
        let json = r#"[
            {"tag_name":"v2.0-beta","prerelease":true,"assets":[]},
            {"tag_name":"v2.0","draft":true,"assets":[]},
            {"tag_name":"v1.2.2","assets":[{"name":"Game-Windows.zip","size":10,
              "browser_download_url":"https://github.com/a/b/releases/download/v1.2.2/Game-Windows.zip",
              "digest":"sha256:ABC123"}]}
        ]"#;
        let latest = parse_latest(json).unwrap();
        assert_eq!(latest.version, "v1.2.2");
        assert_eq!(latest.assets[0].sha256.as_deref(), Some("abc123"));
    }

    #[test]
    fn older_uploads_have_no_checksum() {
        let json = r#"[{"tag_name":"v1","assets":[{"name":"a.zip","size":1,"browser_download_url":"u","digest":null}]}]"#;
        assert_eq!(parse_latest(json).unwrap().assets[0].sha256, None);
    }

    #[test]
    fn says_when_there_is_no_release_yet() {
        assert!(parse_latest("[]").is_err());
    }
}
