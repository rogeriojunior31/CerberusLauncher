//! Steam store lookups for games added by hand: their official name and tall cover.
//! Both endpoints are public and need no API key.

use cerberus_shared::tr;
use reqwest::Client;
use serde::Deserialize;

const SEARCH_URL: &str = "https://store.steampowered.com/api/storesearch/";
const ITEMS_URL: &str = "https://api.steampowered.com/IStoreBrowseService/GetItems/v1/";
const ASSET_HOST: &str = "https://shared.akamai.steamstatic.com/store_item_assets/";

pub struct Found {
    pub name: String,
    pub art: Art,
}

/// A game's pictures on the Steam store.
#[derive(Default)]
pub struct Art {
    pub cover: Option<String>,
    /// The wide backdrop Steam shows on a game's library page.
    pub hero: Option<String>,
}

/// Tries each name in turn and returns the first one Steam knows exactly.
/// Used when adding a game, where a wrong guess is worse than no guess.
pub async fn identify(client: &Client, candidates: &[String]) -> Option<Found> {
    for candidate in candidates {
        let Ok(Some(app)) = search(client, candidate, true).await else {
            continue;
        };
        let art = art(client, app.id).await.unwrap_or_default();
        return Some(Found { name: app.name, art });
    }
    None
}

/// The art of the best match for a name the user typed; looser than `identify`
/// because the user asked for it and can see the result.
pub async fn art_for_name(client: &Client, name: &str) -> Result<Option<Art>, String> {
    let Some(app) = search(client, name, false).await? else {
        return Ok(None);
    };
    art(client, app.id).await.map(Some)
}

#[derive(Deserialize)]
struct SearchReply {
    #[serde(default)]
    items: Vec<SearchItem>,
}

#[derive(Deserialize)]
struct SearchItem {
    id: u32,
    name: String,
    #[serde(rename = "type")]
    kind: String,
}

/// The Steam app with exactly this name, if the store has one. Remembered in
/// `steam-match.json` (a name not found is asked again after a week).
pub async fn app_for(client: &Client, data: &std::path::Path, name: &str) -> Option<u32> {
    #[derive(serde::Serialize, Deserialize, Clone, Copy)]
    struct Seen {
        app: Option<u32>,
        at: u64,
    }
    let file = data.join("cache").join("steam-match.json");
    let mut known: std::collections::HashMap<String, Seen> = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let key = normalize(name);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    if let Some(seen) = known.get(&key) {
        if seen.app.is_some() || now < seen.at + 7 * 86_400 {
            return seen.app;
        }
    }
    let app = search(client, name, true).await.ok()?.map(|item| item.id);
    known.insert(key, Seen { app, at: now });
    let _ = std::fs::create_dir_all(file.parent()?);
    let _ = crate::state_file::write_bytes(&file, serde_json::to_string(&known).unwrap_or_default().as_bytes());
    app
}

async fn search(client: &Client, query: &str, exact: bool) -> Result<Option<SearchItem>, String> {
    let params = [("term", query), ("l", "english"), ("cc", "US")];
    let request = client.get(SEARCH_URL).query(&params);
    let reply: SearchReply = fetch_json(request).await?;
    let wanted = normalize(query);
    let mut apps = reply.items.into_iter().filter(|item| item.kind == "app");
    if exact {
        return Ok(apps.find(|item| normalize(&item.name) == wanted));
    }
    Ok(apps.next())
}

#[derive(Deserialize)]
struct ItemsReply {
    response: ItemsResponse,
}

#[derive(Deserialize)]
struct ItemsResponse {
    #[serde(default)]
    store_items: Vec<StoreItem>,
}

#[derive(Deserialize)]
struct StoreItem {
    assets: Option<Assets>,
}

#[derive(Deserialize)]
struct Assets {
    asset_url_format: String,
    library_capsule: Option<String>,
    library_hero: Option<String>,
    header: Option<String>,
}

/// Newer games keep their art under hash-named folders, so the URLs have to come
/// from the store API instead of being guessed from the app id.
async fn art(client: &Client, app_id: u32) -> Result<Art, String> {
    let input = format!(
        r#"{{"ids":[{{"appid":{app_id}}}],"context":{{"language":"english","country_code":"US"}},"data_request":{{"include_assets":true}}}}"#
    );
    let request = client.get(ITEMS_URL).query(&[("input_json", input)]);
    let reply: ItemsReply = fetch_json(request).await?;
    let first = reply.response.store_items.into_iter().next();
    let Some(assets) = first.and_then(|item| item.assets) else {
        return Ok(Art::default());
    };
    let url = |file: String| format!("{ASSET_HOST}{}", assets.asset_url_format.replace("${FILENAME}", &file));
    // The wide header is a poor fit for the card, but better than no picture.
    let cover = assets.library_capsule.clone().or(assets.header.clone()).map(url);
    let hero = assets.library_hero.clone().or(assets.header.clone()).map(url);
    Ok(Art { cover, hero })
}

async fn fetch_json<T: serde::de::DeserializeOwned>(request: reqwest::RequestBuilder) -> Result<T, String> {
    let offline = |e: reqwest::Error| tr!("Não foi possível falar com a Steam: {}", e);
    let reply = crate::net::send(request).await.map_err(|e| tr!("Não foi possível falar com a Steam: {}", e))?;
    let reply = reply.error_for_status().map_err(offline)?;
    reply.json().await.map_err(offline)
}

/// "DARK SOULS™ III" and "Dark Souls III" should count as the same game.
fn normalize(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_case_and_trademark_signs() {
        assert_eq!(normalize("DARK SOULS™ III"), normalize("Dark Souls III"));
    }

    #[test]
    fn keeps_different_games_apart() {
        assert_ne!(normalize("Hotline Miami"), normalize("Hotline Miami 2"));
    }
}
