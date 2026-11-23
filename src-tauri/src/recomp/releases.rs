//! Release info from the project's server (GitHub, GitLab or Forgejo), kept on
//! disk for six hours.
//!
//! Without a login GitHub answers 60 requests an hour per IP, and Discover asks
//! about every recomp at once, so asking again on every visit would run out fast.
//! The copy on disk also keeps Discover working offline.

use cerberus_shared::tr;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use reqwest::{Client, StatusCode};
use serde::Deserialize;

use super::assets::Asset;
use super::forge::Forge;

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

/// GitLab's: files are links, named as the project likes.
#[derive(Deserialize)]
struct GlRelease {
    tag_name: String,
    #[serde(default)]
    upcoming_release: bool,
    #[serde(default)]
    assets: GlAssets,
}

#[derive(Deserialize, Default)]
struct GlAssets {
    #[serde(default)]
    links: Vec<GlLink>,
}

#[derive(Deserialize)]
struct GlLink {
    name: String,
    url: String,
    direct_asset_url: Option<String>,
}

/// The newest full release (pre-releases are betas and drafts are unpublished).
/// `fresh`: skip the copy on disk, for an install or update about to happen.
pub async fn latest(client: &Client, cache_dir: &Path, forge: &Forge<'_>, fresh: bool) -> Result<LatestRelease, String> {
    let file = cache_dir.join(format!("{}.json", forge.cache_name()));
    let cached = fs::read_to_string(&file).ok();
    let young = age(&file).is_some_and(|age| age < FRESH_FOR);
    let text = match cached {
        Some(text) if young && !fresh => text,
        cached => match fetch(client, forge).await {
            Ok(text) => {
                let _ = fs::create_dir_all(cache_dir);
                let _ = fs::write(&file, &text);
                text
            }
            // Old info beats none: its files are usually still there to download.
            Err(error) => cached.ok_or(error)?,
        },
    };
    match forge {
        Forge::GitLab(_) => parse_gitlab(&text),
        // Forgejo and Gitea answer in GitHub's shape.
        _ => parse_latest(&text, &forge.name()),
    }
}

async fn fetch(client: &Client, forge: &Forge<'_>) -> Result<String, String> {
    let name = forge.name();
    let mut request = client.get(forge.releases_api());
    if let Forge::GitHub(_) = forge {
        request = request.header("Accept", "application/vnd.github+json");
    }
    let reply = request.send().await.map_err(|_| tr!("Sem conexão com o {}.", name))?;
    let status = reply.status();
    if status == StatusCode::FORBIDDEN || status == StatusCode::TOO_MANY_REQUESTS {
        return Err(tr!("O {} limitou as consultas por agora. Tente de novo em alguns minutos.", name));
    }
    if !status.is_success() {
        return Err(tr!("O {1} respondeu {0}.", status.as_u16(), name));
    }
    reply.text().await.map_err(|e| e.to_string())
}

fn parse_latest(text: &str, from: &str) -> Result<LatestRelease, String> {
    let releases: Vec<GhRelease> = serde_json::from_str(text).map_err(|_| tr!("Resposta inesperada do {}.", from))?;
    let full = releases.into_iter().find(|r| !r.draft && !r.prerelease);
    let release = full.ok_or(tr!("Esse projeto ainda não tem uma versão publicada."))?;
    let assets = release.assets.into_iter().map(to_asset).collect();
    Ok(LatestRelease { version: release.tag_name, assets })
}

fn parse_gitlab(text: &str) -> Result<LatestRelease, String> {
    let releases: Vec<GlRelease> = serde_json::from_str(text).map_err(|_| tr!("Resposta inesperada do GitLab.").to_string())?;
    let release = releases.into_iter().find(|r| !r.upcoming_release).ok_or(tr!("Esse projeto ainda não tem uma versão publicada."))?;
    let assets = release
        .assets
        .links
        .into_iter()
        // GitLab tells no size or checksum; the catalog's checksum still applies.
        .map(|link| Asset { name: link.name, size: 0, url: link.direct_asset_url.unwrap_or(link.url), sha256: None })
        .filter(|a| a.url.starts_with("https://"))
        .collect();
    Ok(LatestRelease { version: release.tag_name, assets })
}

fn to_asset(asset: GhAsset) -> Asset {
    let sha256 = asset.digest.and_then(|d| d.strip_prefix("sha256:").map(str::to_lowercase));
    Asset { name: asset.name, size: asset.size, url: asset.browser_download_url, sha256 }
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
        let latest = parse_latest(json, "GitHub").unwrap();
        assert_eq!(latest.version, "v1.2.2");
        assert_eq!(latest.assets[0].sha256.as_deref(), Some("abc123"));
    }

    #[test]
    fn older_uploads_have_no_checksum() {
        let json = r#"[{"tag_name":"v1","assets":[{"name":"a.zip","size":1,"browser_download_url":"u","digest":null}]}]"#;
        assert_eq!(parse_latest(json, "GitHub").unwrap().assets[0].sha256, None);
    }

    #[test]
    fn reads_gitlabs_links() {
        let json = r#"[
            {"tag_name":"v1.0","upcoming_release":true,"assets":{"links":[]}},
            {"tag_name":"v0.9.2","assets":{"links":[
              {"name":"Game-v0.9.2-Windows","url":"https://gitlab.com/a/b/-/package_files/1/download","direct_asset_url":"https://gitlab.com/a/b/-/package_files/1/download"},
              {"name":"odd","url":"javascript:alert(1)"}],"sources":[]}}
        ]"#;
        let latest = parse_gitlab(json).unwrap();
        assert_eq!(latest.version, "v0.9.2");
        assert_eq!(latest.assets.len(), 1);
        assert_eq!(latest.assets[0].name, "Game-v0.9.2-Windows");
    }

    #[test]
    fn says_when_there_is_no_release_yet() {
        assert!(parse_latest("[]", "GitHub").is_err());
    }
}
