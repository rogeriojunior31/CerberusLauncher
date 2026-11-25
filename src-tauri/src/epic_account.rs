//! The Epic Games account: signing in through Epic's own page (Cerberus never
//! sees the password), and the account's library, so games owned but not on
//! this PC appear in the library too, to install through the Epic launcher.
//!
//! The flow follows Hydra's (MIT): Epic's login page, then its redirect page
//! hands an authorization code, traded for a token with the public client the
//! Legendary project uses. Only the refresh token is kept.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use cerberus_shared::{tr, Game, NextStep, Source};
use serde::{Deserialize, Serialize};

const CLIENT_ID: &str = "34a02cf8f4414e29b15921876da36f9a";
const CLIENT_SECRET: &str = "daafbccc737745039dffe53d94fc76cf";
const TOKEN_URL: &str = "https://account-public-service-prod.ol.epicgames.com/account/api/oauth/token";
const LIBRARY_URL: &str = "https://library-service.live.use1a.on.epicgames.com/library/api/public/items";
const FRESH_FOR: u64 = 3600;

/// Epic's page where the player signs in; it returns to the redirect page.
pub fn login_url() -> String {
    let redirect = format!("https://www.epicgames.com/id/api/redirect?clientId={CLIENT_ID}&responseType=code");
    format!("https://www.epicgames.com/id/login?redirectUrl={}", encode(&redirect))
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn redirect_url() -> String {
    format!("https://www.epicgames.com/id/api/redirect?clientId={CLIENT_ID}&responseType=code")
}

fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// The page that hands the code: Epic's own address, with our client, nothing else.
pub fn is_return(url: &tauri::Url) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some("www.epicgames.com")
        && url.path() == "/id/api/redirect"
        && url.query_pairs().any(|(k, v)| k == "clientId" && v == CLIENT_ID)
}

/// A code that looks like Epic's (letters, digits, - and _), or nothing.
pub fn valid_code(code: &str) -> Option<String> {
    let ok = (16..=512).contains(&code.len()) && code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    ok.then(|| code.to_string())
}

/// The account after signing in (or renewing): what is kept, and the short-lived access.
pub struct Session {
    pub access: String,
    pub refresh: String,
    pub name: String,
}

#[derive(Deserialize)]
struct TokenReply {
    access_token: String,
    refresh_token: String,
    #[serde(rename = "displayName", default)]
    display_name: String,
}

pub enum Grant<'a> {
    Code(&'a str),
    Refresh(&'a str),
}

pub async fn token(client: &reqwest::Client, grant: Grant<'_>) -> Result<Session, String> {
    let body = match grant {
        Grant::Code(code) => format!("grant_type=authorization_code&code={}&token_type=eg1", encode(code)),
        Grant::Refresh(refresh) => format!("grant_type=refresh_token&refresh_token={}&token_type=eg1", encode(refresh)),
    };
    let response = client
        .post(TOKEN_URL)
        .basic_auth(CLIENT_ID, Some(CLIENT_SECRET))
        .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(tr!("A Epic recusou o acesso: entre de novo em Configurações › Epic Games.").into());
    }
    let reply: TokenReply = response.json().await.map_err(|e| e.to_string())?;
    Ok(Session { access: reply.access_token, refresh: reply.refresh_token, name: reply.display_name })
}

/// A game the account owns.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Owned {
    pub app_name: String,
    pub namespace: String,
    pub item_id: String,
    pub title: String,
    pub cover: Option<String>,
    pub hero: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibraryPage {
    #[serde(default)]
    records: Vec<Record>,
    #[serde(default)]
    response_metadata: Option<Metadata>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    #[serde(default)]
    namespace: String,
    #[serde(default)]
    catalog_item_id: String,
    #[serde(default)]
    app_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    #[serde(default)]
    title: String,
    #[serde(default)]
    key_images: Vec<KeyImage>,
    #[serde(default)]
    categories: Vec<Category>,
    main_game_item: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct KeyImage {
    #[serde(rename = "type")]
    kind: String,
    url: String,
}

#[derive(Deserialize)]
struct Category {
    path: String,
}

/// A game, not an add-on, a mod, an Unreal Engine asset or an app (Hydra's rule).
fn is_game(namespace: &str, item: &Item) -> bool {
    const NOT_GAMES: [&str; 4] = ["addons", "addons/launchable", "mods", "software"];
    namespace != "ue"
        && item.main_game_item.is_none()
        && !item.categories.iter().any(|c| NOT_GAMES.contains(&c.path.as_str()))
        && item.categories.iter().any(|c| c.path == "games")
}

fn image(item: &Item, kinds: &[&str]) -> Option<String> {
    kinds.iter().find_map(|k| item.key_images.iter().find(|i| i.kind == *k).map(|i| i.url.clone()))
}

/// Everything the account owns that is a game, with its name and art.
pub async fn library(client: &reqwest::Client, access: &str) -> Result<Vec<Owned>, String> {
    let mut records = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..50 {
        let mut url = format!("{LIBRARY_URL}?includeMetadata=true");
        if let Some(c) = &cursor {
            url.push_str(&format!("&cursor={}", encode(c)));
        }
        let response = client.get(&url).bearer_auth(access).send().await.map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(tr!("A Epic não entregou a biblioteca da conta.").into());
        }
        let page: LibraryPage = response.json().await.map_err(|e| e.to_string())?;
        records.extend(page.records.into_iter().filter(|r| !r.app_name.is_empty() && !r.namespace.is_empty() && !r.catalog_item_id.is_empty()));
        cursor = page.response_metadata.and_then(|m| m.next_cursor).filter(|c| !c.is_empty());
        if cursor.is_none() {
            break;
        }
    }
    let (country, locale) = match cerberus_shared::i18n::lang() {
        cerberus_shared::i18n::Lang::Pt => ("BR", "pt-BR"),
        _ => ("US", "en-US"),
    };
    let mut by_namespace: HashMap<String, Vec<Record>> = HashMap::new();
    for r in records {
        if r.namespace != "ue" {
            by_namespace.entry(r.namespace.clone()).or_default().push(r);
        }
    }
    let mut owned = Vec::new();
    for (namespace, list) in by_namespace {
        for chunk in list.chunks(40) {
            let ids: Vec<&str> = chunk.iter().map(|r| r.catalog_item_id.as_str()).collect();
            let url = format!(
                "https://catalog-public-service-prod06.ol.epicgames.com/catalog/api/shared/namespace/{}/bulk/items?id={}&includeDLCDetails=true&includeMainGameDetails=true&country={country}&locale={locale}",
                encode(&namespace),
                ids.join(",")
            );
            let Ok(response) = client.get(&url).bearer_auth(access).send().await else { continue };
            let Ok(items) = response.json::<HashMap<String, Item>>().await else { continue };
            for record in chunk {
                let Some(item) = items.get(&record.catalog_item_id) else { continue };
                if !is_game(&namespace, item) || item.title.trim().is_empty() {
                    continue;
                }
                owned.push(Owned {
                    app_name: record.app_name.clone(),
                    namespace: namespace.clone(),
                    item_id: record.catalog_item_id.clone(),
                    title: item.title.trim().to_string(),
                    cover: image(item, &["DieselGameBoxTall", "OfferImageTall", "Thumbnail"]),
                    hero: image(item, &["DieselGameBox", "OfferImageWide", "DieselStoreFrontWide"]),
                });
            }
        }
    }
    owned.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    owned.dedup_by(|a, b| a.app_name == b.app_name);
    Ok(owned)
}

// ---------- kept on disk ----------

#[derive(Default, Serialize, Deserialize)]
struct Cache {
    at: u64,
    games: Vec<Owned>,
}

fn file(data: &Path) -> PathBuf {
    data.join("epic-library.json")
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default()
}

pub fn cached(data: &Path) -> Vec<Owned> {
    fs::read_to_string(file(data)).ok().and_then(|t| serde_json::from_str::<Cache>(&t).ok()).map(|c| c.games).unwrap_or_default()
}

pub fn stale(data: &Path) -> bool {
    fs::read_to_string(file(data)).ok().and_then(|t| serde_json::from_str::<Cache>(&t).ok()).is_none_or(|c| now().saturating_sub(c.at) > FRESH_FOR)
}

pub fn keep(data: &Path, games: Vec<Owned>) {
    if let Ok(text) = serde_json::to_string(&Cache { at: now(), games }) {
        let temp = file(data).with_extension("json.tmp");
        if fs::write(&temp, text).is_ok() {
            let _ = fs::rename(temp, file(data));
        }
    }
}

pub fn forget(data: &Path) {
    let _ = fs::remove_file(file(data));
}

/// The account's games not on this PC, as games to install; art on the ones that are.
pub fn merge(games: &mut Vec<Game>, owned: &[Owned]) {
    let here: HashMap<String, usize> = games
        .iter()
        .enumerate()
        .filter_map(|(i, g)| match &g.source {
            Source::Epic { app_name, .. } => Some((app_name.to_lowercase(), i)),
            _ => None,
        })
        .collect();
    for o in owned {
        if let Some(&i) = here.get(&o.app_name.to_lowercase()) {
            let game = &mut games[i];
            game.cover_url = game.cover_url.take().or_else(|| o.cover.clone());
            game.hero_url = game.hero_url.take().or_else(|| o.hero.clone());
            continue;
        }
        games.push(Game {
            id: format!("epic:{}", o.app_name),
            name: o.title.clone(),
            source: Source::Epic { app_name: o.app_name.clone(), namespace: o.namespace.clone(), item_id: o.item_id.clone() },
            install_dir: String::new(),
            cover_url: o.cover.clone(),
            hero_url: o.hero.clone(),
            next_step: NextStep::Install,
            ..crate::library::blank_game()
        });
    }
}

/// The Epic launcher's page for installing an owned game.
pub fn install_url(namespace: &str, item_id: &str, app_name: &str) -> String {
    format!("com.epicgames.launcher://apps/{namespace}%3A{item_id}%3A{app_name}?action=install")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_epics_own_return_page_counts() {
        assert!(is_return(&tauri::Url::parse(&redirect_url()).unwrap()));
        assert!(!is_return(&tauri::Url::parse("https://www.epicgames.com/id/api/redirect?clientId=other&responseType=code").unwrap()));
        assert!(!is_return(&tauri::Url::parse("http://www.epicgames.com/id/api/redirect?clientId=34a02cf8f4414e29b15921876da36f9a").unwrap()));
        assert_eq!(valid_code("a3f5b9c0d1e2f3a4b5c6").as_deref(), Some("a3f5b9c0d1e2f3a4b5c6"));
        assert!(valid_code("short").is_none());
        assert!(valid_code("abc def ghi jkl mno pqr").is_none());
        assert!(login_url().starts_with("https://www.epicgames.com/id/login?redirectUrl=https%3A%2F%2Fwww.epicgames.com%2Fid%2Fapi%2Fredirect"));
    }

    #[test]
    fn games_stay_and_the_rest_goes() {
        let item = |cats: &[&str], dlc: bool| Item {
            title: "x".into(),
            key_images: vec![],
            categories: cats.iter().map(|c| Category { path: c.to_string() }).collect(),
            main_game_item: dlc.then(|| serde_json::json!({})),
        };
        assert!(is_game("fn", &item(&["games", "applications"], false)));
        assert!(!is_game("fn", &item(&["games"], true)), "a DLC");
        assert!(!is_game("fn", &item(&["addons"], false)));
        assert!(!is_game("ue", &item(&["games"], false)), "Unreal Engine");
        assert!(!is_game("fn", &item(&["software"], false)));
    }

    #[test]
    fn owned_games_join_the_installed_ones() {
        let owned = vec![
            Owned { app_name: "Fortnite".into(), namespace: "fn".into(), item_id: "i1".into(), title: "Fortnite".into(), cover: Some("c".into()), hero: None },
            Owned { app_name: "Hades".into(), namespace: "h".into(), item_id: "i2".into(), title: "Hades".into(), cover: Some("d".into()), hero: Some("e".into()) },
        ];
        let mut games = vec![Game {
            id: "epic:Fortnite".into(),
            name: "Fortnite".into(),
            source: Source::Epic { app_name: "Fortnite".into(), namespace: "fn".into(), item_id: "i1".into() },
            install_dir: "C:/Games/Fortnite".into(),
            ..crate::library::blank_game()
        }];
        merge(&mut games, &owned);
        assert_eq!(games.len(), 2);
        assert_eq!(games[0].cover_url.as_deref(), Some("c"));
        assert_eq!(games[1].id, "epic:Hades");
        assert_eq!(games[1].next_step, NextStep::Install);
    }
}
