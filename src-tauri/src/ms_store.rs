//! A game's page from the Microsoft Store (public catalog, no sign-in): the
//! description, pictures, developer and release date, for Xbox games and for
//! games from other stores that Steam doesn't sell. Kept on disk for a week.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use cerberus_shared::{StoreBlock, StoreDetails, StoreShot};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const CATALOG: &str = "https://displaycatalog.mp.microsoft.com/v7.0";
const WEEK: u64 = 7 * 86_400;

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn dir(data: &Path) -> PathBuf {
    data.join("cache").join("ms-store")
}

/// "DARK SOULS™ III" and "Dark Souls III" are the same game.
fn plain(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

#[derive(Serialize, Deserialize)]
struct Kept<T> {
    at: u64,
    value: T,
}

fn read<T: for<'de> Deserialize<'de>>(file: &Path, fresh_for: u64) -> Option<T> {
    let kept: Kept<T> = serde_json::from_str(&fs::read_to_string(file).ok()?).ok()?;
    (now() < kept.at + fresh_for).then_some(kept.value)
}

fn write<T: Serialize>(file: &Path, value: T) {
    if let Some(parent) = file.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = crate::state_file::write_bytes(&file, serde_json::to_string(&Kept { at: now(), value }).unwrap_or_default().as_bytes());
}

/// The Store's product for a game with exactly this name (its add-ons left out).
pub async fn find(http: &reqwest::Client, data: &Path, name: &str) -> Option<String> {
    let file = dir(data).join(format!("find-{}.json", plain(name)));
    if let Some(found) = read::<Option<String>>(&file, WEEK) {
        return found;
    }
    let (market, language) = crate::gamepass::market();
    let url = format!("{CATALOG}/productFamilies/autosuggest?market={market}&languages={language}&productFamilyNames=Games&query={}", urlencode(name));
    let v: Value = crate::net::send(http.get(&url)).await.ok()?.json().await.ok()?;
    let wanted = plain(name);
    let found = v
        .get("Results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|r| r.get("Products").and_then(Value::as_array).into_iter().flatten())
        .find(|p| p.get("Type").and_then(Value::as_str) == Some("Game") && p.get("Title").and_then(Value::as_str).is_some_and(|t| plain(t) == wanted))
        .and_then(|p| p.get("ProductId").and_then(Value::as_str))
        .map(str::to_string);
    write(&file, &found);
    found
}

fn urlencode(text: &str) -> String {
    text.bytes().map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// The product's page, in the player's language.
pub async fn details(http: &reqwest::Client, data: &Path, product: &str) -> Option<StoreDetails> {
    let product: String = product.chars().filter(char::is_ascii_alphanumeric).collect();
    let (market, language) = crate::gamepass::market();
    let file = dir(data).join(format!("{product}-{language}.json"));
    if let Some(kept) = read::<StoreDetails>(&file, WEEK) {
        return Some(kept);
    }
    let url = format!("{CATALOG}/products?bigIds={product}&market={market}&languages={language}");
    let v: Value = crate::net::send(http.get(&url)).await.ok()?.json().await.ok()?;
    let found = read_product(v.pointer("/Products/0")?)?;
    write(&file, &found);
    Some(found)
}

fn https(uri: &str) -> String {
    if let Some(rest) = uri.strip_prefix("//") { format!("https://{rest}") } else { uri.to_string() }
}

fn read_product(p: &Value) -> Option<StoreDetails> {
    let lp = p.pointer("/LocalizedProperties/0")?;
    let text = |k: &str| lp.get(k).and_then(Value::as_str).unwrap_or_default().trim().to_string();
    let name = text("ProductTitle");
    if name.is_empty() {
        return None;
    }
    // The description's paragraphs, as the page shows Steam's.
    let about = text("ProductDescription")
        .replace("\r\n", "\n")
        .split('\n')
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| match l.strip_prefix("• ").or_else(|| l.strip_prefix("- ")) {
            Some(item) => StoreBlock { kind: "li".into(), text: item.to_string(), src: None },
            None => StoreBlock { kind: "p".into(), text: l.to_string(), src: None },
        })
        .collect();
    let images: Vec<&Value> = lp.get("Images").and_then(Value::as_array).into_iter().flatten().collect();
    let screenshots = images
        .iter()
        .filter(|i| i.get("ImagePurpose").and_then(Value::as_str) == Some("Screenshot"))
        .filter_map(|i| i.get("Uri").and_then(Value::as_str))
        .map(|u| StoreShot { thumb: format!("{}?w=480", https(u)), full: https(u) })
        .collect();
    let background = images
        .iter()
        .find(|i| i.get("ImagePurpose").and_then(Value::as_str) == Some("SuperHeroArt"))
        .and_then(|i| i.get("Uri").and_then(Value::as_str))
        .map(https);
    let released = p.pointer("/MarketProperties/0/OriginalReleaseDate").and_then(Value::as_str).and_then(|d| d.get(..10)).map(|d| {
        // "2016-02-17" → "17/02/2016"
        let parts: Vec<&str> = d.split('-').collect();
        if parts.len() == 3 { format!("{}/{}/{}", parts[2], parts[1], parts[0]) } else { d.to_string() }
    });
    let genres = p
        .pointer("/Properties/Categories")
        .and_then(Value::as_array)
        .map(|c| c.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default();
    let one = |k: &str| -> Vec<String> { Some(text(k)).filter(|s| !s.is_empty()).into_iter().collect() };
    Some(StoreDetails {
        origin: "msstore".into(),
        app: 0,
        name,
        short: text("ShortDescription").lines().next().unwrap_or_default().trim().to_string(),
        about,
        developers: one("DeveloperName"),
        publishers: one("PublisherName"),
        released: released.unwrap_or_default(),
        coming_soon: false,
        genres,
        features: Vec::new(),
        metacritic: None,
        recommendations: None,
        controller: String::new(),
        languages: Vec::new(),
        minimum: Vec::new(),
        recommended: Vec::new(),
        screenshots,
        movies: Vec::new(),
        website: lp.get("PublisherWebsiteUri").and_then(Value::as_str).filter(|u| u.starts_with("https://")).map(str::to_string),
        background,
        is_free: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_a_store_product() {
        let p = json!({
            "LocalizedProperties": [{
                "ProductTitle": "Rocket League®", "ShortDescription": "JOGUE DE GRAÇA!\r\n\r\nMais", "DeveloperName": "PSYONIX LLC", "PublisherName": "PSYONIX LLC",
                "ProductDescription": "Futebol com carros.\r\n\r\n• Rocket Pass\r\n• Torneios",
                "Images": [{ "ImagePurpose": "Screenshot", "Uri": "//store-images.s-microsoft.com/a" }, { "ImagePurpose": "SuperHeroArt", "Uri": "//store-images.s-microsoft.com/h" }]
            }],
            "MarketProperties": [{ "OriginalReleaseDate": "2016-02-17T00:00:00.0000000Z" }],
            "Properties": { "Categories": ["Sports"] }
        });
        let d = read_product(&p).unwrap();
        assert_eq!((d.name.as_str(), d.short.as_str(), d.released.as_str()), ("Rocket League®", "JOGUE DE GRAÇA!", "17/02/2016"));
        assert_eq!(d.about.iter().map(|b| b.kind.as_str()).collect::<Vec<_>>(), vec!["p", "li", "li"]);
        assert_eq!(d.screenshots[0].full, "https://store-images.s-microsoft.com/a");
        assert_eq!(d.background.as_deref(), Some("https://store-images.s-microsoft.com/h"));
        assert_eq!(d.origin, "msstore", "the page says where it came from");
        assert_eq!(d.genres, vec!["Sports"]);
    }
}
