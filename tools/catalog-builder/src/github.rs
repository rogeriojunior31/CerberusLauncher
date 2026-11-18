//! The few GitHub questions the builder asks. With `GITHUB_TOKEN` set (as in
//! GitHub Actions) the limit is 5000 an hour; without it, 60.

use std::time::Duration;

use reqwest::blocking::Client;
use serde::Deserialize;

use crate::assets::ReleaseFile;

pub struct GitHub {
    client: Client,
    token: Option<String>,
}

#[derive(Deserialize)]
pub struct Repo {
    pub full_name: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub stargazers_count: u64,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub fork: bool,
    #[serde(default)]
    pub license: Option<License>,
}

#[derive(Deserialize)]
pub struct License {
    #[serde(default)]
    pub spdx_id: Option<String>,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    size: u64,
    #[serde(default)]
    digest: Option<String>,
}

/// A project's newest full release.
pub struct Latest {
    pub tag: String,
    /// `YYYY-MM-DD`.
    pub date: String,
    pub files: Vec<ReleaseFile>,
}

#[derive(Deserialize)]
struct SearchPage {
    items: Vec<Repo>,
}

impl GitHub {
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent("cerberus-catalog-builder")
            .timeout(Duration::from_secs(30))
            .build()
            .expect("an HTTP client");
        let token = std::env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty());
        Self { client, token }
    }

    pub fn has_token(&self) -> bool {
        self.token.is_some()
    }

    fn get<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T, String> {
        let mut request = self.client.get(url).header("Accept", "application/vnd.github+json");
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let reply = request.send().map_err(|e| format!("{url}: {e}"))?;
        let status = reply.status();
        if !status.is_success() {
            return Err(format!("{url}: GitHub answered {status}"));
        }
        reply.json().map_err(|e| format!("{url}: {e}"))
    }

    pub fn repo(&self, repo: &str) -> Result<Repo, String> {
        self.get(&format!("https://api.github.com/repos/{repo}"))
    }

    /// The newest release that isn't a draft or a pre-release, if there is one.
    pub fn latest(&self, repo: &str) -> Result<Option<Latest>, String> {
        let releases: Vec<Release> = self.get(&format!("https://api.github.com/repos/{repo}/releases?per_page=10"))?;
        let Some(release) = releases.into_iter().find(|r| !r.draft && !r.prerelease) else {
            return Ok(None);
        };
        let files = release
            .assets
            .into_iter()
            .map(|a| ReleaseFile {
                sha256: a.digest.and_then(|d| d.strip_prefix("sha256:").map(str::to_lowercase)),
                name: a.name,
                size: a.size,
            })
            .collect();
        let date = release.published_at.unwrap_or_default().chars().take(10).collect();
        Ok(Some(Latest { tag: release.tag_name, date, files }))
    }

    /// Repositories matching a GitHub search, most starred first.
    pub fn search(&self, query: &str) -> Result<Vec<Repo>, String> {
        let encoded: String = query.chars().map(|c| if c == ' ' { '+' } else { c }).collect();
        let url = format!("https://api.github.com/search/repositories?q={encoded}&sort=stars&order=desc&per_page=50");
        let page: SearchPage = self.get(&url)?;
        Ok(page.items)
    }
}
