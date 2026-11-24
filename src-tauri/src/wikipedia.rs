//! The original game's Wikipedia summary, in the player's language, for a
//! recomp's page: the article's opening paragraph as plain text and its link.
//! Public API, no account. Kept for a month, including "not found", so each
//! game is asked about once.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use cerberus_shared::WikiAbout;
use cerberus_shared::i18n::{Lang, lang};
use serde_json::Value;

const FRESH: Duration = Duration::from_secs(30 * 24 * 3600);

fn wiki_lang() -> &'static str {
    if lang() == Lang::Pt { "pt" } else { "en" }
}

fn file(data_dir: &Path, title: &str) -> PathBuf {
    // The name as a file: letters and digits only, so any title is safe on disk.
    let safe: String = title.chars().map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    data_dir.join("cache").join("wiki").join(format!("{}-{}.json", wiki_lang(), safe.chars().take(120).collect::<String>()))
}

/// An article about a game: its short description says so.
fn about_a_game(description: &str) -> bool {
    let d = description.to_lowercase();
    // "série de jogos eletrônicos" is about many games, not this one.
    let many = ["série", "series", "franquia", "franchise"].iter().any(|w| d.contains(w));
    !many && ["jogo", "videojogo", "video game", "videogame"].iter().any(|w| d.contains(w))
}

fn user_agent() -> String {
    format!("CerberusLauncher/{}", env!("CARGO_PKG_VERSION"))
}

async fn get(http: &reqwest::Client, url: &str) -> Option<Value> {
    let request = http.get(url).header("User-Agent", user_agent()).timeout(Duration::from_secs(15));
    let reply = crate::net::send(request).await.ok()?;
    if !reply.status().is_success() {
        return None;
    }
    reply.json().await.ok()
}

/// The summary of one article, when it's about a game.
async fn summary(http: &reqwest::Client, title: &str) -> Option<WikiAbout> {
    let path = title.replace(' ', "_");
    let url = format!("https://{}.wikipedia.org/api/rest_v1/page/summary/{}", wiki_lang(), crate::retroachievements::encode(&path));
    let v = get(http, &url).await?;
    parse(&v)
}

fn parse(v: &Value) -> Option<WikiAbout> {
    let text = |key: &str| v.get(key).and_then(Value::as_str).unwrap_or_default().trim().to_string();
    if text("type") != "standard" || !about_a_game(&text("description")) {
        return None;
    }
    let url = v.pointer("/content_urls/desktop/page").and_then(Value::as_str)?.to_string();
    let extract = text("extract");
    (url.starts_with("https://") && !extract.is_empty()).then(|| WikiAbout { title: text("title"), extract, url })
}

async fn find(http: &reqwest::Client, game: &str) -> Option<WikiAbout> {
    // The exact title first; then the first few search results.
    if let Some(found) = summary(http, game).await {
        return Some(found);
    }
    let url = format!(
        "https://{}.wikipedia.org/w/api.php?action=query&list=search&srsearch={}&srlimit=3&format=json",
        wiki_lang(),
        crate::retroachievements::encode(game)
    );
    let v = get(http, &url).await?;
    let titles: Vec<String> =
        v.pointer("/query/search").and_then(Value::as_array).into_iter().flatten().filter_map(|r| r.get("title").and_then(Value::as_str)).map(str::to_string).collect();
    for title in titles {
        if let Some(found) = summary(http, &title).await {
            return Some(found);
        }
    }
    None
}

/// The original game's article, from the copy kept on disk when it's recent.
pub async fn about(http: &reqwest::Client, data_dir: &Path, game: &str) -> Option<WikiAbout> {
    let game = game.trim();
    if game.is_empty() {
        return None;
    }
    let path = file(data_dir, game);
    let fresh = fs::metadata(&path).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age < FRESH);
    if fresh {
        if let Some(kept) = fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<Option<WikiAbout>>(&t).ok()) {
            return kept;
        }
    }
    let found = find(http, game).await;
    let _ = fs::create_dir_all(path.parent().unwrap_or(data_dir));
    let _ = fs::write(&path, serde_json::to_string(&found).unwrap_or_default());
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_articles_about_games_are_kept() {
        let game = serde_json::json!({
            "type": "standard", "title": "The Legend of Zelda: Majora's Mask", "description": "videojogo de 2000",
            "extract": "The Legend of Zelda: Majora's Mask é um jogo eletrônico de ação-aventura.",
            "content_urls": { "desktop": { "page": "https://pt.wikipedia.org/wiki/The_Legend_of_Zelda:_Majora's_Mask" } }
        });
        let found = parse(&game).unwrap();
        assert!(found.extract.starts_with("The Legend") && found.url.starts_with("https://pt.wikipedia.org/"));
        let mut series = game.clone();
        series["description"] = "série de mídia".into();
        assert!(parse(&series).is_none(), "a series isn't the game");
        series["description"] = "série de jogos eletrônicos".into();
        assert!(parse(&series).is_none(), "nor a series of games");
        let mut many = game.clone();
        many["type"] = "disambiguation".into();
        assert!(parse(&many).is_none());
        let mut odd = game;
        odd["content_urls"]["desktop"]["page"] = "javascript:alert(1)".into();
        assert!(parse(&odd).is_none());
    }
}
