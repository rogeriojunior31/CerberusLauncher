//! What the developers of the player's Steam games posted lately: the game's
//! own announcements on Steam (`ISteamNews/GetNewsForApp`, public, no key, no
//! account), for Novidades.
//!
//! Each game's answer is kept on disk and asked again after six hours, so the
//! page opens at once. Only plain text is kept (tags and markup taken out), and
//! only Steam's own links can be opened from it.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cerberus_shared::NewsItem;
use serde_json::Value;

const FRESH: Duration = Duration::from_secs(6 * 3600);
/// Games asked about at most, and posts older than this aren't shown.
pub const MOST_GAMES: usize = 12;
const MAX_AGE: u64 = 90 * 24 * 3600;
const SHOWN: usize = 24;

fn file(data_dir: &Path, app: u32) -> PathBuf {
    data_dir.join("cache").join("news").join(format!("{app}.json"))
}

fn kept(data_dir: &Path, app: u32) -> Option<(Vec<NewsItem>, bool)> {
    let path = file(data_dir, app);
    let items = serde_json::from_str(&fs::read_to_string(&path).ok()?).ok()?;
    let fresh = fs::metadata(&path).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age < FRESH);
    Some((items, fresh))
}

/// Text without BBCode, HTML tags or Steam's `{PLACEHOLDERS}`, on one line.
pub fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut skip: Option<char> = None;
    // After a `{PLACEHOLDER}`, the rest of its word is a picture's path.
    let mut path = false;
    for c in text.chars() {
        if path {
            if !c.is_whitespace() {
                continue;
            }
            path = false;
        }
        match skip {
            Some('}') if c == '}' => {
                skip = None;
                path = true;
            }
            Some(end) if c == end => skip = None,
            Some(_) => {}
            None => match c {
                '[' => skip = Some(']'),
                '<' => skip = Some('>'),
                '{' => skip = Some('}'),
                c => out.push(c),
            },
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A link from a post that may be opened: https, on Steam.
pub fn steam_link(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    !host.contains(['@', ':'])
        && ["steampowered.com", "steamcommunity.com", "akamaihd.net", "steamstatic.com"].iter().any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

fn parse(app: u32, name: &str, v: &Value) -> Vec<NewsItem> {
    v.pointer("/appnews/newsitems")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|n| {
            let url = n.get("url").and_then(Value::as_str).unwrap_or_default();
            let title = plain(n.get("title").and_then(Value::as_str).unwrap_or_default());
            (!title.is_empty() && steam_link(url)).then(|| NewsItem {
                app_id: app,
                game: name.to_string(),
                title,
                summary: plain(n.get("contents").and_then(Value::as_str).unwrap_or_default()),
                url: url.to_string(),
                date: n.get("date").and_then(Value::as_u64).unwrap_or_default(),
            })
        })
        .collect()
}

async fn fetch(http: &reqwest::Client, data_dir: &Path, app: u32, name: &str) -> Option<Vec<NewsItem>> {
    let url = format!("https://api.steampowered.com/ISteamNews/GetNewsForApp/v2/?appid={app}&count=3&maxlength=320&feeds=steam_community_announcements&format=json");
    let v: Value = http.get(&url).timeout(Duration::from_secs(15)).send().await.ok()?.json().await.ok()?;
    let items = parse(app, name, &v);
    let path = file(data_dir, app);
    let _ = fs::create_dir_all(path.parent().unwrap_or(data_dir));
    let _ = fs::write(&path, serde_json::to_string(&items).unwrap_or_default());
    Some(items)
}

async fn of_game(http: &reqwest::Client, data_dir: &Path, app: u32, name: &str) -> Vec<NewsItem> {
    match kept(data_dir, app) {
        Some((items, true)) => items,
        // Old: what's kept is better than nothing if Steam doesn't answer.
        Some((items, false)) => fetch(http, data_dir, app, name).await.unwrap_or(items),
        None => fetch(http, data_dir, app, name).await.unwrap_or_default(),
    }
}

/// The latest posts of these games (Steam app id and name), newest first.
pub async fn latest(http: &reqwest::Client, data_dir: &Path, games: Vec<(u32, String)>) -> Vec<NewsItem> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default();
    let asks = games.iter().take(MOST_GAMES).map(|(app, name)| of_game(http, data_dir, *app, name));
    let mut all: Vec<NewsItem> = futures_util::future::join_all(asks).await.into_iter().flatten().filter(|n| now.saturating_sub(n.date) < MAX_AGE).collect();
    all.sort_by(|a, b| b.date.cmp(&a.date));
    all.truncate(SHOWN);
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posts_become_plain_text_with_only_steam_links() {
        assert_eq!(plain("[h1]Patch 1.2[/h1] <b>Fixed</b> {STEAM_CLAN_IMAGE}/a/b.png   crashes\n[url=x]here[/url]"), "Patch 1.2 Fixed crashes here");
        let v = serde_json::json!({ "appnews": { "newsitems": [
            { "title": "Update [b]2[/b]", "url": "https://steamstore-a.akamaihd.net/news/externalpost/x/1", "contents": "Hello", "date": 10 },
            { "title": "Elsewhere", "url": "https://evil.example/steampowered.com", "contents": "x", "date": 11 },
            { "title": "Plain http", "url": "http://store.steampowered.com/news/1", "contents": "x", "date": 12 },
            { "title": "", "url": "https://store.steampowered.com/news/2", "contents": "x", "date": 13 }
        ] } });
        let items = parse(413150, "Stardew Valley", &v);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Update 2");
        assert!(steam_link("https://store.steampowered.com/news/app/1") && steam_link("https://steamcommunity.com/games/1/announcements/detail/2"));
        assert!(!steam_link("https://steampowered.com.evil.io/x") && !steam_link("https://user@steamcommunity.com/") && !steam_link("javascript:alert(1)"));
    }
}
