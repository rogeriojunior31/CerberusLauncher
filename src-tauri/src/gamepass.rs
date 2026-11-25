//! PC Game Pass: the games in the subscription, from Microsoft's public
//! catalog (no sign-in needed). Listed in the library as not installed (they
//! install through the Xbox app), and their official art goes on the Xbox games
//! already installed. Kept on disk and asked again every 12 hours.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use cerberus_shared::{Game, NextStep, Source};
use serde::{Deserialize, Serialize};

/// "All PC games" in Game Pass.
const ALL_PC: &str = "fdd9e2a7-0fee-49f6-ad69-4354098401ff";
const FRESH_FOR: u64 = 12 * 3600;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Product {
    pub id: String,
    pub title: String,
    /// The installed package's family name, to recognise the game when it's on this PC.
    pub family: String,
    pub poster: Option<String>,
    pub hero: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct Cache {
    at: u64,
    market: String,
    products: Vec<Product>,
}

fn file(data: &Path) -> PathBuf {
    data.join("gamepass.json")
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default()
}

/// The market and language the catalog is asked in, from the app's language.
pub fn market() -> (&'static str, &'static str) {
    match cerberus_shared::i18n::lang() {
        cerberus_shared::i18n::Lang::Pt => ("BR", "pt-br"),
        _ => ("US", "en-us"),
    }
}

/// What was read last time; nothing (and no wait) before the first answer.
pub fn cached(data: &Path) -> Vec<Product> {
    fs::read_to_string(file(data)).ok().and_then(|t| serde_json::from_str::<Cache>(&t).ok()).map(|c| c.products).unwrap_or_default()
}

/// The list is old (or for another market): time to ask again.
pub fn stale(data: &Path) -> bool {
    let Some(cache) = fs::read_to_string(file(data)).ok().and_then(|t| serde_json::from_str::<Cache>(&t).ok()) else { return true };
    cache.market != market().0 || now().saturating_sub(cache.at) > FRESH_FOR
}

#[derive(Deserialize)]
struct Sigl {
    id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Products {
    #[serde(default)]
    products: Vec<CatalogProduct>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct CatalogProduct {
    product_id: String,
    #[serde(default)]
    localized_properties: Vec<Localized>,
    #[serde(default)]
    display_sku_availabilities: Vec<SkuAvailability>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Localized {
    #[serde(default)]
    product_title: String,
    #[serde(default)]
    images: Vec<Image>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Image {
    image_purpose: String,
    uri: String,
    #[serde(default)]
    width: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SkuAvailability {
    sku: Sku,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Sku {
    #[serde(default)]
    properties: SkuProperties,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SkuProperties {
    #[serde(default)]
    packages: Vec<Package>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Package {
    package_family_name: Option<String>,
}

/// The catalog's addresses start with "//".
fn https(uri: &str) -> String {
    if uri.starts_with("//") { format!("https:{uri}") } else { uri.to_string() }
}

fn product_of(p: CatalogProduct) -> Option<Product> {
    let local = p.localized_properties.into_iter().next()?;
    let image = |purpose: &str| {
        local.images.iter().filter(|i| i.image_purpose == purpose).max_by_key(|i| i.width).map(|i| https(&i.uri))
    };
    let family = p
        .display_sku_availabilities
        .iter()
        .flat_map(|s| s.sku.properties.packages.iter())
        .find_map(|pk| pk.package_family_name.clone())
        .unwrap_or_default();
    Some(Product {
        id: p.product_id,
        title: local.product_title.trim().to_string(),
        family,
        poster: image("Poster").or_else(|| image("BoxArt")),
        hero: image("SuperHeroArt").or_else(|| image("TitledHeroArt")),
    })
}

/// Asks Microsoft for the catalog and keeps it on disk.
pub async fn refresh(client: &reqwest::Client, data: &Path) -> Result<Vec<Product>, String> {
    let (market, language) = market();
    let url = format!("https://catalog.gamepass.com/sigls/v2?id={ALL_PC}&language={language}&market={market}");
    let response = crate::net::send(client.get(&url)).await.map_err(|e| e.to_string())?;
    let list: Vec<Sigl> = response.json().await.map_err(|e| e.to_string())?;
    let ids: Vec<String> = list.into_iter().filter_map(|s| s.id).collect();
    let mut products = Vec::new();
    for chunk in ids.chunks(20) {
        let url = format!(
            "https://displaycatalog.mp.microsoft.com/v7.0/products?bigIds={}&market={market}&languages={language}",
            chunk.join(",")
        );
        let response = crate::net::send(client.get(&url)).await.map_err(|e| e.to_string())?;
        let page: Products = response.json().await.map_err(|e| e.to_string())?;
        products.extend(page.products.into_iter().filter_map(product_of));
    }
    if products.is_empty() {
        return Err("Game Pass: empty catalog".into());
    }
    let cache = Cache { at: now(), market: market.to_string(), products: products.clone() };
    if let Ok(text) = serde_json::to_string(&cache) {
        let temp = file(data).with_extension("json.tmp");
        if fs::write(&temp, text).is_ok() {
            let _ = fs::rename(temp, file(data));
        }
    }
    Ok(products)
}

/// Official art on the installed Xbox games, and the rest of Game Pass as
/// games not installed yet (installed through the Xbox app).
/// A Game Pass game not installed whose name the player already has from
/// another store (owned on Epic, Steam...): only the owned copy shows.
pub fn drop_owned_elsewhere(games: &mut Vec<Game>) {
    let plain = |name: &str| name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect::<String>();
    let catalog_only = |g: &Game| matches!(g.source, Source::Xbox { game_pass: true, .. }) && g.install_dir.is_empty();
    let elsewhere: std::collections::HashSet<String> = games.iter().filter(|g| !catalog_only(g)).map(|g| plain(&g.name)).collect();
    games.retain(|g| !catalog_only(g) || !elsewhere.contains(&plain(&g.name)));
}

pub fn merge(games: &mut Vec<Game>, catalog: &[Product]) {
    let by_family: HashMap<&str, &Product> = catalog.iter().filter(|p| !p.family.is_empty()).map(|p| (p.family.as_str(), p)).collect();
    let mut here = std::collections::HashSet::new();
    for game in games.iter_mut() {
        let Source::Xbox { family, product, game_pass, .. } = &mut game.source else { continue };
        here.insert(family.to_lowercase());
        if let Some(p) = by_family.get(family.as_str()) {
            *product = p.id.clone();
            *game_pass = true;
            if p.poster.is_some() {
                game.cover_url = p.poster.clone();
                game.cover_path = None;
            }
            game.hero_url = p.hero.clone().or(game.hero_url.take());
        }
    }
    // A game already installed from another store (9 Kings on Steam) isn't listed again.
    let plain = |name: &str| name.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
    let installed: std::collections::HashSet<String> = games.iter().filter(|g| !g.install_dir.is_empty()).map(|g| plain(&g.name)).collect();
    for p in catalog {
        if !p.family.is_empty() && here.contains(&p.family.to_lowercase()) {
            continue;
        }
        if installed.contains(&plain(&p.title)) {
            continue;
        }
        let key = if p.family.is_empty() { p.id.clone() } else { p.family.clone() };
        games.push(Game {
            id: format!("xbox:{key}"),
            name: p.title.clone(),
            source: Source::Xbox { family: p.family.clone(), app: "Game".into(), product: p.id.clone(), game_pass: true },
            install_dir: String::new(),
            cover_path: None,
            cover_url: p.poster.clone(),
            hero_path: None,
            hero_url: p.hero.clone(),
            next_step: NextStep::Install,
            ..crate::library::blank_game()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_game_owned_elsewhere_hides_its_game_pass_entry() {
        let game = |name: &str, source: Source, dir: &str| Game { name: name.into(), source, install_dir: dir.into(), ..crate::library::blank_game() };
        let pass = |name: &str| game(name, Source::Xbox { family: "f".into(), app: "Game".into(), product: "p".into(), game_pass: true }, "");
        let mut games = vec![
            pass("Rocket League®"),
            game("Rocket League", Source::Epic { app_name: "a".into(), namespace: "n".into(), item_id: "i".into() }, ""),
            pass("Halo Infinite"),
        ];
        drop_owned_elsewhere(&mut games);
        let names: Vec<_> = games.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, vec!["Rocket League", "Halo Infinite"]);
    }

    const PAGE: &str = r#"{"Products":[{"ProductId":"9NNM7PKZN3JF","LocalizedProperties":[{"ProductTitle":"9 Kings ","Images":[
        {"ImagePurpose":"Poster","Uri":"//store-images.s-microsoft.com/image/apps.1","Width":1440},
        {"ImagePurpose":"BoxArt","Uri":"//store-images.s-microsoft.com/image/apps.2","Width":2160},
        {"ImagePurpose":"SuperHeroArt","Uri":"//store-images.s-microsoft.com/image/apps.3","Width":3840}]}],
        "DisplaySkuAvailabilities":[{"Sku":{"Properties":{"Packages":[{"PackageFamilyName":"HoodedHorse.9Kings_znaey1dw2bdpr"}]}}}]}]}"#;

    #[test]
    fn reads_a_catalog_page() {
        let page: Products = serde_json::from_str(PAGE).unwrap();
        let p = product_of(page.products.into_iter().next().unwrap()).unwrap();
        assert_eq!(p.title, "9 Kings");
        assert_eq!(p.family, "HoodedHorse.9Kings_znaey1dw2bdpr");
        assert_eq!(p.poster.as_deref(), Some("https://store-images.s-microsoft.com/image/apps.1"));
        assert_eq!(p.hero.as_deref(), Some("https://store-images.s-microsoft.com/image/apps.3"));
    }

    #[test]
    fn installed_games_get_art_and_the_rest_wait_to_install() {
        let catalog = vec![
            Product { id: "A".into(), title: "Here".into(), family: "Pub.Here_abc".into(), poster: Some("p".into()), hero: None },
            Product { id: "B".into(), title: "Elsewhere".into(), family: "Pub.Else_def".into(), poster: Some("q".into()), hero: Some("h".into()) },
        ];
        let mut games = vec![Game {
            id: "xbox:Pub.Here_abc".into(),
            name: "Here".into(),
            source: Source::Xbox { family: "Pub.Here_abc".into(), app: "Game".into(), product: String::new(), game_pass: false },
            ..crate::library::blank_game()
        }];
        // Elsewhere is installed from Steam: not listed twice.
        let mut steam = games.clone();
        steam.push(Game { id: "steam:1".into(), name: "Elsewhere".into(), install_dir: "C:/g".into(), ..crate::library::blank_game() });
        merge(&mut steam, &catalog);
        assert_eq!(steam.len(), 2, "the Steam copy, no Game Pass one");
        merge(&mut games, &catalog);
        assert_eq!(games.len(), 2);
        assert_eq!(games[0].cover_url.as_deref(), Some("p"));
        assert!(matches!(&games[0].source, Source::Xbox { product, game_pass: true, .. } if product == "A"));
        assert_eq!(games[1].next_step, NextStep::Install);
        assert_eq!(games[1].id, "xbox:Pub.Else_def");
    }
}
