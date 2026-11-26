//! The Microsoft account behind the Xbox app: signing in on Microsoft's own
//! page (Cerberus never sees the password), then the account's PC games from
//! Xbox Live's title history, so games owned but not on this PC appear in the
//! library (Game Pass ones come from the public catalog, `gamepass.rs`).
//!
//! The same public flow Playnite's Xbox library uses: Live sign-in with the
//! Xbox app's client, then Xbox Live user and XSTS tokens. Only the Live
//! refresh token is kept.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use cerberus_shared::{tr, Game, NextStep, Source};
use serde::{Deserialize, Serialize};
use serde_json::json;

const CLIENT_ID: &str = "38cd2fa8-66fd-4760-afb2-405eb65d5b0c";
const REDIRECT: &str = "https://login.live.com/oauth20_desktop.srf";
const SCOPE: &str = "Xboxlive.signin Xboxlive.offline_access";
const FRESH_FOR: u64 = 3600;

fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// Microsoft's sign-in page; it returns to the desktop redirect with a code.
pub fn login_url() -> String {
    format!(
        "https://login.live.com/oauth20_authorize.srf?client_id={CLIENT_ID}&response_type=code&approval_prompt=auto&scope={}&redirect_uri={}",
        encode(SCOPE),
        encode(REDIRECT)
    )
}

/// The code in Microsoft's own return address, when that's where the window is.
pub fn code_in(url: &tauri::Url) -> Option<String> {
    if url.scheme() != "https" || url.host_str() != Some("login.live.com") || url.path() != "/oauth20_desktop.srf" {
        return None;
    }
    url.query_pairs()
        .find(|(k, _)| k == "code")
        .map(|(_, v)| v.into_owned())
        .filter(|c| (8..=2048).contains(&c.len()) && c.chars().all(|c| c.is_ascii_alphanumeric() || "-_.!*$".contains(c)))
}

/// The sign-in was refused or cancelled (an `error` on the return address).
pub fn refused(url: &tauri::Url) -> bool {
    url.host_str() == Some("login.live.com") && url.path() == "/oauth20_desktop.srf" && url.query_pairs().any(|(k, _)| k == "error")
}

pub enum Grant<'a> {
    Code(&'a str),
    Refresh(&'a str),
}

#[derive(Deserialize)]
struct LiveToken {
    access_token: String,
    refresh_token: String,
}

/// The account's Xbox Live access: the header value and its user id.
pub struct Session {
    pub refresh: String,
    pub auth: String,
    pub xuid: String,
    pub gamertag: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct XblReply {
    token: String,
    display_claims: Claims,
}

#[derive(Deserialize)]
struct Claims {
    xui: Vec<serde_json::Map<String, serde_json::Value>>,
}

pub async fn sign_in(client: &reqwest::Client, grant: Grant<'_>) -> Result<Session, String> {
    let body = match grant {
        Grant::Code(code) => format!(
            "client_id={CLIENT_ID}&code={}&grant_type=authorization_code&redirect_uri={}&scope={}",
            encode(code),
            encode(REDIRECT),
            encode(SCOPE)
        ),
        Grant::Refresh(refresh) => format!(
            "client_id={CLIENT_ID}&refresh_token={}&grant_type=refresh_token&redirect_uri={}&scope={}",
            encode(refresh),
            encode(REDIRECT),
            encode(SCOPE)
        ),
    };
    let refused = || tr!("A Microsoft recusou o acesso: entre de novo em Configurações › Xbox.").to_string();
    let live = client
        .post("https://login.live.com/oauth20_token.srf")
        .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !live.status().is_success() {
        return Err(refused());
    }
    let live: LiveToken = live.json().await.map_err(|e| e.to_string())?;
    let user = client
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .header("x-xbl-contract-version", "1")
        .json(&json!({
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT",
            "Properties": { "AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com", "RpsTicket": format!("d={}", live.access_token) }
        }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !user.status().is_success() {
        return Err(refused());
    }
    let user: XblReply = user.json().await.map_err(|e| e.to_string())?;
    let xsts = client
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .header("x-xbl-contract-version", "1")
        .json(&json!({
            "RelyingParty": "http://xboxlive.com",
            "TokenType": "JWT",
            "Properties": { "UserTokens": [user.token], "SandboxId": "RETAIL" }
        }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !xsts.status().is_success() {
        return Err(tr!("Essa conta Microsoft ainda não tem um perfil Xbox: abra o app Xbox uma vez e crie o seu.").into());
    }
    let xsts: XblReply = xsts.json().await.map_err(|e| e.to_string())?;
    let claim = |name: &str| xsts.display_claims.xui.first().and_then(|c| c.get(name)).and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let (uhs, xuid) = (claim("uhs"), claim("xid"));
    if uhs.is_empty() || xuid.is_empty() {
        return Err(refused());
    }
    Ok(Session { refresh: live.refresh_token, auth: format!("XBL3.0 x={uhs};{}", xsts.token), xuid, gamertag: claim("gtg") })
}

/// A PC game on the account.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Owned {
    pub name: String,
    pub family: String,
    pub product: String,
    pub cover: Option<String>,
    pub hero: Option<String>,
}

#[derive(Deserialize)]
struct History {
    #[serde(default)]
    titles: Vec<Title>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Title {
    #[serde(default)]
    name: String,
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    devices: Vec<String>,
    #[serde(default)]
    pfn: Option<String>,
    #[serde(default)]
    display_image: Option<String>,
}

/// The account's PC games: the titles it has, as Xbox Live lists them.
pub async fn library(client: &reqwest::Client, session: &Session) -> Result<Vec<Owned>, String> {
    let (_, language) = crate::gamepass::market();
    let url = format!("https://titlehub.xboxlive.com/users/xuid({})/titles/titlehistory/decoration/detail,image", session.xuid);
    let response = client
        .get(&url)
        .header("Authorization", &session.auth)
        .header("x-xbl-contract-version", "2")
        .header("Accept-Language", language)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(tr!("O Xbox Live não entregou os jogos da conta.").into());
    }
    let history: History = response.json().await.map_err(|e| e.to_string())?;
    Ok(pc_games(history))
}

/// Games for Windows only (not the console's, not apps), each package once.
fn pc_games(history: History) -> Vec<Owned> {
    let mut owned: Vec<Owned> = history
        .titles
        .into_iter()
        .filter(|t| t.kind == "Game" && t.devices.iter().any(|d| d == "PC"))
        .filter_map(|t| {
            let family = t.pfn.filter(|p| !p.is_empty())?;
            Some(Owned { name: t.name.trim().to_string(), family, product: String::new(), cover: t.display_image.as_deref().map(crate::net::secure), hero: None })
        })
        .collect();
    owned.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    owned.dedup_by(|a, b| a.family.eq_ignore_ascii_case(&b.family));
    owned
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Lookup {
    #[serde(default)]
    products: Vec<serde_json::Value>,
}

/// The Store's product (for installing) and its official art, by package.
pub async fn with_store_details(client: &reqwest::Client, mut owned: Vec<Owned>) -> Vec<Owned> {
    let (market, language) = crate::gamepass::market();
    for game in owned.iter_mut() {
        let url = format!(
            "https://displaycatalog.mp.microsoft.com/v7.0/products/lookup?alternateId=PackageFamilyName&value={}&market={market}&languages={language}",
            encode(&game.family)
        );
        let Ok(response) = client.get(&url).send().await else { continue };
        let Ok(found) = response.json::<Lookup>().await else { continue };
        let Some(product) = found.products.into_iter().next() else { continue };
        if let Some(id) = product.get("ProductId").and_then(|v| v.as_str()) {
            game.product = id.to_string();
        }
        let images = product.pointer("/LocalizedProperties/0/Images").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let image = |purpose: &str| {
            images.iter().filter(|i| i.get("ImagePurpose").and_then(|p| p.as_str()) == Some(purpose)).find_map(|i| i.get("Uri").and_then(|u| u.as_str())).map(|u| if u.starts_with("//") { format!("https:{u}") } else { u.to_string() })
        };
        game.cover = image("Poster").or_else(|| image("BoxArt")).or(game.cover.take());
        game.hero = image("SuperHeroArt").or_else(|| image("TitledHeroArt"));
    }
    owned
}

// ---------- kept on disk ----------

#[derive(Default, Serialize, Deserialize)]
struct Cache {
    at: u64,
    games: Vec<Owned>,
}

fn file(data: &Path) -> PathBuf {
    data.join("xbox-library.json")
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

/// The account's games not on this PC (and not listed already, by Game Pass or
/// another store's copy) join as games to install.
pub fn merge(games: &mut Vec<Game>, owned: &[Owned]) {
    let families: std::collections::HashSet<String> = games
        .iter()
        .filter_map(|g| match &g.source {
            Source::Xbox { family, .. } if !family.is_empty() => Some(family.to_lowercase()),
            _ => None,
        })
        .collect();
    let plain = |name: &str| name.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
    let installed: std::collections::HashSet<String> = games.iter().filter(|g| !g.install_dir.is_empty()).map(|g| plain(&g.name)).collect();
    for o in owned {
        if families.contains(&o.family.to_lowercase()) || installed.contains(&plain(&o.name)) {
            continue;
        }
        games.push(Game {
            id: format!("xbox:{}", o.family),
            name: o.name.clone(),
            source: Source::Xbox { family: o.family.clone(), app: "Game".into(), product: o.product.clone(), game_pass: false },
            install_dir: String::new(),
            cover_url: o.cover.clone(),
            hero_url: o.hero.clone(),
            next_step: NextStep::Install,
            ..crate::library::blank_game()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_code_comes_only_from_microsofts_return_address() {
        let ok = tauri::Url::parse("https://login.live.com/oauth20_desktop.srf?code=M.C531_BAY.2.U.abc123-def&lc=1046").unwrap();
        assert_eq!(code_in(&ok).as_deref(), Some("M.C531_BAY.2.U.abc123-def"));
        assert!(code_in(&tauri::Url::parse("https://evil.example/oauth20_desktop.srf?code=abcdefgh123").unwrap()).is_none());
        assert!(code_in(&tauri::Url::parse("https://login.live.com/oauth20_authorize.srf?code=abcdefgh123").unwrap()).is_none());
        assert!(refused(&tauri::Url::parse("https://login.live.com/oauth20_desktop.srf?error=access_denied").unwrap()));
        assert!(login_url().contains("client_id=38cd2fa8-66fd-4760-afb2-405eb65d5b0c"));
    }

    #[test]
    fn only_pc_games_once_each() {
        let history: History = serde_json::from_value(json!({ "titles": [
            { "name": "Halo Infinite", "type": "Game", "devices": ["XboxSeries", "PC"], "pfn": "Microsoft.254428597CFE2_8wekyb3d8bbwe", "displayImage": "http://img/halo" },
            { "name": "Forza Horizon 4", "type": "Game", "devices": ["XboxOne"], "pfn": "Microsoft.SunriseBaseGame_8wekyb3d8bbwe" },
            { "name": "Netflix", "type": "App", "devices": ["PC"], "pfn": "4DF9E0F8.Netflix_mcm4njqhnhss8" },
            { "name": "Halo Infinite", "type": "Game", "devices": ["PC"], "pfn": "Microsoft.254428597CFE2_8wekyb3d8bbwe" }
        ]})).unwrap();
        let games = pc_games(history);
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].name, "Halo Infinite");
    }

    #[test]
    fn owned_games_skip_what_is_listed_already() {
        let owned = vec![
            Owned { name: "Chained Beasts".into(), family: "F.Chained_x".into(), ..Default::default() },
            Owned { name: "Halo Infinite".into(), family: "Microsoft.Halo_y".into(), product: "9PP5G1F0C2B6".into(), ..Default::default() },
        ];
        let mut games = vec![Game {
            id: "xbox:F.Chained_x".into(),
            name: "Chained Beasts".into(),
            source: Source::Xbox { family: "F.Chained_x".into(), app: "Game".into(), product: String::new(), game_pass: true },
            install_dir: "C:/XboxGames/Chained Beasts/Content".into(),
            ..crate::library::blank_game()
        }];
        merge(&mut games, &owned);
        assert_eq!(games.len(), 2);
        assert_eq!(games[1].next_step, NextStep::Install);
        assert!(matches!(&games[1].source, Source::Xbox { product, .. } if product == "9PP5G1F0C2B6"));
    }
}
