//! A Steam game's store page, for its page in Cerberus: description, pictures
//! and trailers, requirements, languages, controller support and the facts
//! (`store.steampowered.com/api/appdetails`, public, no key).
//!
//! The answer is kept on disk per language and shown at once; when it is older
//! than three days it's asked again in the background (stale while revalidate).
//! Steam's HTML is never put in the page as HTML: it's turned into plain blocks
//! of text and pictures here, so nothing from outside can run in the app.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use cerberus_shared::i18n::{Lang, lang};
use cerberus_shared::{StoreBlock, StoreDetails, StoreLanguage, StoreMovie, StoreShot, tr};
use serde_json::Value;

const FRESH: Duration = Duration::from_secs(3 * 24 * 3600);

fn steam_lang() -> &'static str {
    if lang() == Lang::Pt { "brazilian" } else { "english" }
}

fn file(data_dir: &Path, app: u32) -> PathBuf {
    data_dir.join("cache").join("store").join(format!("{app}-{}.json", steam_lang()))
}

fn kept(data_dir: &Path, app: u32) -> Option<(StoreDetails, bool)> {
    let path = file(data_dir, app);
    let details: StoreDetails = serde_json::from_str(&fs::read_to_string(&path).ok()?).ok()?;
    let fresh = fs::metadata(&path).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age < FRESH);
    Some((details, fresh))
}

async fn fetch(http: &reqwest::Client, data_dir: &Path, app: u32) -> Result<StoreDetails, String> {
    let url = format!("https://store.steampowered.com/api/appdetails?appids={app}&l={}&cc=br", steam_lang());
    let reply = http
        .get(&url)
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|_| tr!("Não foi possível falar com a loja da Steam.").to_string())?;
    let v: Value = reply.json().await.map_err(|_| tr!("A loja da Steam respondeu algo inesperado.").to_string())?;
    let entry = v.get(app.to_string()).ok_or(tr!("A loja da Steam respondeu algo inesperado."))?;
    if entry.get("success").and_then(Value::as_bool) != Some(true) {
        return Err(tr!("A loja da Steam não tem a página desse jogo.").into());
    }
    let details = parse(app, entry.get("data").ok_or(tr!("A loja da Steam respondeu algo inesperado."))?);
    let path = file(data_dir, app);
    let _ = fs::create_dir_all(path.parent().unwrap_or(data_dir));
    let _ = fs::write(&path, serde_json::to_string(&details).unwrap_or_default());
    Ok(details)
}

/// The store page: kept copy at once (asked again later when old), or Steam now.
pub async fn details(http: &reqwest::Client, data_dir: &Path, app: u32) -> Result<StoreDetails, String> {
    match kept(data_dir, app) {
        Some((d, true)) => Ok(d),
        Some((d, false)) => {
            let (http, dir) = (http.clone(), data_dir.to_path_buf());
            tauri::async_runtime::spawn(async move {
                let _ = fetch(&http, &dir, app).await;
            });
            Ok(d)
        }
        None => fetch(http, data_dir, app).await,
    }
}

fn text(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or_default().trim().to_string()
}

fn strings(v: &Value, key: &str) -> Vec<String> {
    v.get(key).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

fn names(v: &Value, key: &str) -> Vec<String> {
    v.get(key).and_then(Value::as_array).into_iter().flatten().filter_map(|x| x.get("description").and_then(Value::as_str)).map(str::to_string).collect()
}

fn https(url: &str) -> String {
    url.strip_prefix("http://").map(|rest| format!("https://{rest}")).unwrap_or_else(|| url.to_string())
}

pub fn parse(app: u32, d: &Value) -> StoreDetails {
    let categories: Vec<u64> = d.get("categories").and_then(Value::as_array).into_iter().flatten().filter_map(|c| c.get("id").and_then(Value::as_u64)).collect();
    // Steam category 28 = full controller support, 18 = partial; `controller_support` when present wins.
    let controller = match d.get("controller_support").and_then(Value::as_str) {
        Some("full") => "full",
        Some("partial") => "partial",
        _ if categories.contains(&28) => "full",
        _ if categories.contains(&18) => "partial",
        _ => "",
    }
    .to_string();
    let screenshots = d
        .get("screenshots")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|s| Some(StoreShot { thumb: https(s.get("path_thumbnail")?.as_str()?), full: https(s.get("path_full")?.as_str()?) }))
        .collect();
    let movies = d
        .get("movies")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let mp4 = m.pointer("/mp4/max").or_else(|| m.pointer("/mp4/480")).and_then(Value::as_str).map(https);
            let hls = m.get("hls_h264").and_then(Value::as_str).map(https);
            if mp4.is_none() && hls.is_none() {
                return None;
            }
            Some(StoreMovie { name: text(m, "name"), thumb: m.get("thumbnail").and_then(Value::as_str).map(https).unwrap_or_default(), mp4, hls })
        })
        .collect();
    let requirements = |which: &str| d.pointer(&format!("/pc_requirements/{which}")).and_then(Value::as_str).map(requirement_rows).unwrap_or_default();
    StoreDetails {
        app,
        name: text(d, "name"),
        short: plain(&text(d, "short_description")),
        about: blocks(&text(d, "about_the_game")),
        developers: strings(d, "developers"),
        publishers: strings(d, "publishers"),
        released: d.pointer("/release_date/date").and_then(Value::as_str).unwrap_or_default().to_string(),
        coming_soon: d.pointer("/release_date/coming_soon").and_then(Value::as_bool).unwrap_or(false),
        genres: names(d, "genres"),
        features: names(d, "categories"),
        metacritic: d.pointer("/metacritic/score").and_then(Value::as_u64).map(|s| s as u32),
        recommendations: d.pointer("/recommendations/total").and_then(Value::as_u64),
        controller,
        languages: languages(&text(d, "supported_languages")),
        minimum: requirements("minimum"),
        recommended: requirements("recommended"),
        screenshots,
        movies,
        website: Some(text(d, "website")).filter(|w| w.starts_with("https://") || w.starts_with("http://")),
        background: d.get("background_raw").or_else(|| d.get("background")).and_then(Value::as_str).map(https),
        is_free: d.get("is_free").and_then(Value::as_bool).unwrap_or(false),
    }
}

/// Text with the HTML tags gone and the common entities turned back.
pub fn plain(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&reg;", "®")
        .replace("&trade;", "™")
        .replace("&copy;", "©");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Steam's description as blocks: headings, paragraphs, list items and its
/// own pictures (only from Steam's servers). Everything else is dropped.
pub fn blocks(html: &str) -> Vec<StoreBlock> {
    let mut out = Vec::new();
    let mut rest = html;
    let mut current = String::new();
    let mut kind = "p";
    let flush = |out: &mut Vec<StoreBlock>, current: &mut String, kind: &str| {
        let t = plain(current);
        if !t.is_empty() {
            out.push(StoreBlock { kind: kind.into(), text: t, src: None });
        }
        current.clear();
    };
    while let Some(start) = rest.find('<') {
        current.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('>') else { break };
        let tag = &rest[start + 1..start + end];
        let lower = tag.to_lowercase();
        let name: String = lower.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
        let closing = lower.starts_with('/');
        // Scripts and styles: their content isn't text for the player.
        if !closing && (name == "script" || name == "style") {
            let after = &rest[start + end + 1..];
            let close = format!("</{name}");
            rest = match after.to_lowercase().find(&close) {
                Some(at) => after[at..].find('>').map_or("", |e| &after[at + e + 1..]),
                None => "",
            };
            continue;
        }
        match name.as_str() {
            "p" | "div" | "br" if closing || name == "br" || !current.trim().is_empty() => {
                flush(&mut out, &mut current, kind);
                kind = "p";
            }
            "h1" | "h2" | "h3" | "h4" | "h5" => {
                flush(&mut out, &mut current, kind);
                kind = if closing { "p" } else { "h" };
            }
            "li" => {
                flush(&mut out, &mut current, kind);
                kind = if closing { "p" } else { "li" };
            }
            "ul" | "ol" => flush(&mut out, &mut current, kind),
            "img" => {
                flush(&mut out, &mut current, kind);
                let src = tag.split("src=\"").nth(1).and_then(|s| s.split('"').next()).map(https);
                if let Some(src) = src.filter(|s| s.contains(".steamstatic.com/") || s.contains(".steampowered.com/")) {
                    out.push(StoreBlock { kind: "img".into(), text: String::new(), src: Some(src) });
                }
            }
            _ => {}
        }
        rest = &rest[start + end + 1..];
    }
    current.push_str(rest);
    flush(&mut out, &mut current, kind);
    out
}

/// "<strong>SO:</strong> Windows 10" list items as (label, value) pairs; a line
/// without a label keeps an empty label.
pub fn requirement_rows(html: &str) -> Vec<(String, String)> {
    blocks(html)
        .into_iter()
        .filter(|b| b.kind == "li")
        .map(|b| match b.text.split_once(':') {
            Some((label, value)) if label.len() <= 40 => (label.trim().to_string(), value.trim().to_string()),
            _ => (String::new(), b.text),
        })
        .collect()
}

/// "English<strong>*</strong>, French, ..." with a starred note for full audio.
pub fn languages(html: &str) -> Vec<StoreLanguage> {
    let first = html.split("<br").next().unwrap_or(html);
    first
        .split(',')
        .filter_map(|part| {
            let audio = part.contains('*');
            let name = plain(part).replace('*', "").trim().to_string();
            (!name.is_empty()).then_some(StoreLanguage { name, audio })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requirements_become_label_and_value_rows() {
        let html = "<strong>Mínimos:</strong><br><ul class=\"bb_ul\"><li>Requer um processador de 64 bits<br></li><li><strong>SO:</strong> Windows 10 64bit<br></li><li><strong>Memória:</strong> 8 GB de RAM<br></li></ul>";
        let rows = requirement_rows(html);
        assert_eq!(rows[0], (String::new(), "Requer um processador de 64 bits".into()));
        assert_eq!(rows[1], ("SO".into(), "Windows 10 64bit".into()));
        assert_eq!(rows[2], ("Memória".into(), "8 GB de RAM".into()));
    }

    #[test]
    fn languages_know_which_have_audio() {
        let html = "Inglês<strong>*</strong>, Dinamarquês, Português (Brasil)<strong>*</strong><br><strong>*</strong>idiomas com suporte total de áudio";
        let all = languages(html);
        assert_eq!(all.len(), 3);
        assert!(all[0].audio && !all[1].audio && all[2].audio);
        assert_eq!(all[2].name, "Português (Brasil)");
    }

    #[test]
    fn descriptions_keep_text_and_steam_pictures_only() {
        let html = "<h2 class=\"bb_tag\">Sobre</h2><p class=\"bb_paragraph\">Um &amp; dois</p><p></p><ul><li>Item</li></ul><img src=\"https://shared.akamai.steamstatic.com/a.gif\"><img src=\"https://evil.example/x.png\"><script>alert(1)</script><p>Fim</p>";
        let b = blocks(html);
        let kinds: Vec<&str> = b.iter().map(|x| x.kind.as_str()).collect();
        assert_eq!(kinds, ["h", "p", "li", "img", "p"]);
        assert_eq!(b[1].text, "Um & dois");
        assert_eq!(b[3].src.as_deref(), Some("https://shared.akamai.steamstatic.com/a.gif"));
        assert_eq!(b[4].text, "Fim");
    }
}
