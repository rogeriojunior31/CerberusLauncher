//! Steam achievements from the Steam client's own files on this PC: no key, no
//! login, no public profile.
//!
//! - `appcache/stats/UserGameStatsSchema_<app>.bin`: the game's achievements
//!   (names and descriptions in many languages, icons, whether they're secret).
//! - `appcache/stats/UserGameStats_<account>_<app>.bin`: which ones this
//!   account unlocked, and when. Each stat is a bit field; bit N of stat S is
//!   the achievement listed under S's `bits` as N.
//! - `userdata/<account>/config/localconfig.vdf`: Steam's own play time and
//!   last played per game.
//!
//! The first two are binary KeyValues, the third the text kind. Steam fills the
//! cache for games its client on this PC has synced, so a game never played
//! here may be missing.
//!
//! How rare each achievement is comes from Steam's public, keyless endpoint,
//! kept on disk for a week.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use cerberus_shared::i18n::{Lang, lang};
use cerberus_shared::{Achievement, AchievementGame, AchievementSource};

/// A KeyValues value: text, a number, or a nested object (keys in file order).
#[derive(Debug, Clone, PartialEq)]
pub enum Kv {
    Text(String),
    Int(i64),
    Float(f32),
    Object(Vec<(String, Kv)>),
}

impl Kv {
    /// A child by key, ignoring letter case (Steam isn't consistent about it).
    pub fn get(&self, key: &str) -> Option<&Kv> {
        match self {
            Kv::Object(items) => items.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn children(&self) -> &[(String, Kv)] {
        match self {
            Kv::Object(items) => items,
            _ => &[],
        }
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            Kv::Text(t) => Some(t),
            _ => None,
        }
    }

    /// A number, also when Steam wrote it as text.
    pub fn int(&self) -> Option<i64> {
        match self {
            Kv::Int(n) => Some(*n),
            Kv::Float(f) => Some(*f as i64),
            Kv::Text(t) => t.trim().parse().ok(),
            Kv::Object(_) => None,
        }
    }
}

/// Binary KeyValues, as Steam's appcache keeps them. `None` when malformed.
pub fn binary_kv(data: &[u8]) -> Option<Kv> {
    let mut at = 0;
    let object = read_object(data, &mut at, 0)?;
    Some(Kv::Object(object))
}

fn read_object(data: &[u8], at: &mut usize, depth: usize) -> Option<Vec<(String, Kv)>> {
    if depth > 64 {
        return None;
    }
    let mut items = Vec::new();
    while *at < data.len() {
        let kind = data[*at];
        *at += 1;
        if kind == 8 || kind == 11 {
            return Some(items);
        }
        let key = read_cstr(data, at)?;
        let value = match kind {
            0 => Kv::Object(read_object(data, at, depth + 1)?),
            1 => Kv::Text(read_cstr(data, at)?),
            2 | 4 | 6 => Kv::Int(i64::from(i32::from_le_bytes(take::<4>(data, at)?))),
            3 => Kv::Float(f32::from_le_bytes(take::<4>(data, at)?)),
            5 => Kv::Text(read_wide(data, at)?),
            7 => Kv::Int(u64::from_le_bytes(take::<8>(data, at)?) as i64),
            10 => Kv::Int(i64::from_le_bytes(take::<8>(data, at)?)),
            _ => return None,
        };
        items.push((key, value));
    }
    // A file may end without the last end marker.
    Some(items)
}

fn take<const N: usize>(data: &[u8], at: &mut usize) -> Option<[u8; N]> {
    let bytes = data.get(*at..*at + N)?.try_into().ok()?;
    *at += N;
    Some(bytes)
}

fn read_cstr(data: &[u8], at: &mut usize) -> Option<String> {
    let end = data.get(*at..)?.iter().position(|&b| b == 0)? + *at;
    let text = String::from_utf8_lossy(&data[*at..end]).into_owned();
    *at = end + 1;
    Some(text)
}

fn read_wide(data: &[u8], at: &mut usize) -> Option<String> {
    let mut units = Vec::new();
    loop {
        let unit = u16::from_le_bytes(take::<2>(data, at)?);
        if unit == 0 {
            return Some(String::from_utf16_lossy(&units));
        }
        units.push(unit);
    }
}

/// Text KeyValues (`localconfig.vdf`). Lenient: what can't be read is skipped.
pub fn text_kv(text: &str) -> Kv {
    let mut tokens = Tokens { chars: text.chars().peekable() };
    Kv::Object(read_text_object(&mut tokens, 0))
}

struct Tokens<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

enum Token {
    Text(String),
    Open,
    Close,
}

impl Iterator for Tokens<'_> {
    type Item = Token;
    fn next(&mut self) -> Option<Token> {
        loop {
            let c = self.chars.next()?;
            match c {
                '{' => return Some(Token::Open),
                '}' => return Some(Token::Close),
                '"' => {
                    let mut out = String::new();
                    while let Some(c) = self.chars.next() {
                        match c {
                            '"' => break,
                            '\\' => match self.chars.next() {
                                Some('n') => out.push('\n'),
                                Some('t') => out.push('\t'),
                                Some(other) => out.push(other),
                                None => break,
                            },
                            other => out.push(other),
                        }
                    }
                    return Some(Token::Text(out));
                }
                '/' if self.chars.peek() == Some(&'/') => {
                    for c in self.chars.by_ref() {
                        if c == '\n' {
                            break;
                        }
                    }
                }
                c if c.is_whitespace() => {}
                other => {
                    // A bare word.
                    let mut out = other.to_string();
                    while let Some(&c) = self.chars.peek() {
                        if c.is_whitespace() || c == '{' || c == '}' || c == '"' {
                            break;
                        }
                        out.push(c);
                        self.chars.next();
                    }
                    return Some(Token::Text(out));
                }
            }
        }
    }
}

fn read_text_object(tokens: &mut Tokens, depth: usize) -> Vec<(String, Kv)> {
    let mut items = Vec::new();
    while let Some(token) = tokens.next() {
        match token {
            Token::Close => return items,
            Token::Open => {
                // A block without a key: read and drop it.
                read_text_object(tokens, depth + 1);
            }
            Token::Text(key) => match tokens.next() {
                Some(Token::Text(value)) => items.push((key, Kv::Text(value))),
                Some(Token::Open) if depth < 64 => items.push((key, Kv::Object(read_text_object(tokens, depth + 1)))),
                _ => return items,
            },
        }
    }
    items
}

/// One account's data on this PC.
pub struct Account {
    pub stats_dir: PathBuf,
    pub id: String,
    pub localconfig: Option<PathBuf>,
}

/// The account whose achievement files were written last (the one playing on this PC).
pub fn account(steam: &Path) -> Option<Account> {
    let stats_dir = steam.join("appcache").join("stats");
    let mut newest: Option<(SystemTime, String)> = None;
    for entry in fs::read_dir(&stats_dir).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(rest) = name.strip_prefix("UserGameStats_").filter(|_| !name.starts_with("UserGameStatsSchema_")) else { continue };
        let Some(id) = rest.split('_').next().filter(|id| id.chars().all(|c| c.is_ascii_digit())) else { continue };
        let modified = entry.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
        if newest.as_ref().is_none_or(|(t, _)| modified > *t) {
            newest = Some((modified, id.to_string()));
        }
    }
    let (_, id) = newest?;
    let localconfig = steam.join("userdata").join(&id).join("config").join("localconfig.vdf");
    Some(Account { stats_dir, localconfig: localconfig.is_file().then_some(localconfig), id })
}

/// Steam's play time (seconds) and last played (Unix seconds) per app, from `localconfig.vdf`.
pub fn playtimes(localconfig: &Path) -> HashMap<u32, (u64, Option<u64>)> {
    let Ok(text) = fs::read_to_string(localconfig) else { return HashMap::new() };
    let root = text_kv(&text);
    let apps = root
        .get("UserLocalConfigStore")
        .and_then(|s| s.get("Software"))
        .and_then(|s| s.get("Valve"))
        .and_then(|s| s.get("Steam"))
        .and_then(|s| s.get("apps"));
    let mut out = HashMap::new();
    for (id, app) in apps.map(Kv::children).unwrap_or_default() {
        let Ok(id) = id.parse::<u32>() else { continue };
        let minutes = app.get("Playtime").and_then(Kv::int).unwrap_or(0).max(0) as u64;
        let last = app.get("LastPlayed").and_then(Kv::int).filter(|t| *t > 0).map(|t| t as u64);
        if minutes > 0 || last.is_some() {
            out.insert(id, (minutes * 60, last));
        }
    }
    out
}

/// A text in the player's language: Steam's `brazilian` for Portuguese, else English, else any.
fn localized(value: Option<&Kv>) -> String {
    match value {
        Some(Kv::Text(t)) => t.clone(),
        Some(map @ Kv::Object(_)) => {
            let wanted: &[&str] = if lang() == Lang::Pt { &["brazilian", "portuguese", "english"] } else { &["english"] };
            wanted
                .iter()
                .find_map(|l| map.get(l).and_then(Kv::text))
                .or_else(|| map.children().first().and_then(|(_, v)| v.text()))
                .unwrap_or_default()
                .to_string()
        }
        _ => String::new(),
    }
}

fn icon_url(app: u32, hash: Option<&str>) -> Option<String> {
    let hash = hash.map(str::trim).filter(|h| !h.is_empty())?;
    let file = if hash.contains('.') { hash.to_string() } else { format!("{hash}.jpg") };
    Some(format!("https://shared.akamai.steamstatic.com/community_assets/images/apps/{app}/{file}"))
}

/// The game's name and its achievements, with this account's unlocks.
pub fn read_game(account: &Account, app: u32) -> Option<(String, Vec<Achievement>)> {
    let schema = binary_kv(&fs::read(account.stats_dir.join(format!("UserGameStatsSchema_{app}.bin"))).ok()?)?;
    let root = schema.get(&app.to_string()).or_else(|| schema.children().first().map(|(_, v)| v))?;
    let name = root.get("gamename").and_then(Kv::text).unwrap_or_default().to_string();
    let mine = fs::read(account.stats_dir.join(format!("UserGameStats_{}_{app}.bin", account.id)))
        .ok()
        .and_then(|d| binary_kv(&d));
    let cache = mine.as_ref().map(|m| m.get("cache").unwrap_or(m));
    let mut out = Vec::new();
    for (stat_id, stat) in root.get("stats").map(Kv::children).unwrap_or_default() {
        let Some(bits) = stat.get("bits") else { continue };
        let stat_mine = cache.and_then(|c| c.get(stat_id));
        let field = stat_mine.and_then(|s| s.get("data")).and_then(Kv::int).unwrap_or(0) as u64;
        for (bit, ach) in bits.children() {
            let Ok(bit) = bit.parse::<u32>() else { continue };
            let display = ach.get("display");
            let api = ach.get("name").and_then(Kv::text).unwrap_or_default().to_string();
            let unlocked = bit < 64 && field >> bit & 1 == 1;
            let time = stat_mine
                .and_then(|s| s.get("AchievementTimes"))
                .and_then(|t| t.get(&bit.to_string()))
                .and_then(Kv::int)
                .filter(|t| *t > 0)
                .map(|t| t as u64);
            out.push(Achievement {
                id: if api.is_empty() { format!("{stat_id}.{bit}") } else { api },
                name: localized(display.and_then(|d| d.get("name"))),
                description: localized(display.and_then(|d| d.get("desc"))),
                icon: icon_url(app, display.and_then(|d| d.get("icon")).and_then(Kv::text)),
                icon_locked: icon_url(app, display.and_then(|d| d.get("icon_gray")).and_then(Kv::text)),
                hidden: display.and_then(|d| d.get("hidden")).and_then(Kv::int).unwrap_or(0) != 0,
                // Steam keeps a time for nearly every unlock; one without is still unlocked.
                unlocked_at: if unlocked { Some(time.unwrap_or(0)) } else { None },
                hardcore_at: None,
                points: 0,
                rarity: None,
            });
        }
    }
    Some((name, out))
}

/// Every app this account has achievement data for.
pub fn apps(account: &Account) -> Vec<u32> {
    let Ok(entries) = fs::read_dir(&account.stats_dir) else { return Vec::new() };
    let mut out: Vec<u32> = entries
        .flatten()
        .filter_map(|e| e.file_name().to_string_lossy().strip_prefix("UserGameStatsSchema_")?.strip_suffix(".bin")?.parse().ok())
        .collect();
    out.sort_unstable();
    out
}

/// The game's pictures, best first: Steam's copy on disk, the tall cover online,
/// and the wide header every game has.
pub fn art(app: u32) -> Vec<String> {
    crate::steam::local_cover(app)
        .into_iter()
        .chain([
            format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{app}/library_600x900.jpg"),
            format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{app}/header.jpg"),
        ])
        .collect()
}

/// The hub's entry for a game, from its achievements and Steam's play time.
pub fn game_entry(app: u32, name: String, list: &[Achievement], played: Option<&(u64, Option<u64>)>) -> AchievementGame {
    AchievementGame {
        source: AchievementSource::Steam,
        key: app.to_string(),
        name: if name.trim().is_empty() { format!("App {app}") } else { name },
        art: art(app),
        platform: "Steam".into(),
        total: list.len() as u32,
        unlocked: list.iter().filter(|a| a.unlocked_at.is_some()).count() as u32,
        hardcore: 0,
        points: 0,
        points_total: 0,
        last_unlock: list.iter().filter_map(|a| a.unlocked_at).filter(|t| *t > 0).max(),
        playtime_secs: played.map(|p| p.0).filter(|s| *s > 0),
        last_played: played.and_then(|p| p.1),
        // Every achievement: completed, from the last unlock.
        award: (!list.is_empty() && list.iter().all(|a| a.unlocked_at.is_some())).then_some(cerberus_shared::GameAward::Completed),
        award_at: (!list.is_empty() && list.iter().all(|a| a.unlocked_at.is_some())).then(|| list.iter().filter_map(|a| a.unlocked_at).max()).flatten(),
    }
}

/// A game's name from the Steam store, for games Steam's cache doesn't name.
pub async fn store_name(http: &reqwest::Client, app: &str) -> Option<String> {
    let url = format!("https://store.steampowered.com/api/appdetails?appids={app}&filters=basic");
    let v: serde_json::Value = crate::net::send(http.get(&url).timeout(Duration::from_secs(15))).await.ok()?.json().await.ok()?;
    let name = v.get(app)?.get("data")?.get("name")?.as_str()?.trim().to_string();
    (!name.is_empty()).then_some(name)
}

const RARITY_FOR: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// How many players have each achievement (percent), from Steam's public
/// endpoint; kept on disk for a week, and the old copy used when offline.
pub async fn rarity(http: &reqwest::Client, cache_dir: &Path, app: u32) -> HashMap<String, f32> {
    let file = cache_dir.join(format!("steam-rarity-{app}.json"));
    let fresh = fs::metadata(&file).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age < RARITY_FOR);
    let cached = || fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str::<HashMap<String, f32>>(&t).ok());
    if fresh {
        if let Some(map) = cached() {
            return map;
        }
    }
    let url = format!("https://api.steampowered.com/ISteamUserStats/GetGlobalAchievementPercentagesForApp/v2/?gameid={app}");
    let reply = crate::net::send(http.get(&url).timeout(Duration::from_secs(15))).await;
    let parsed = match reply {
        Ok(r) if r.status().is_success() => r.json::<serde_json::Value>().await.ok().map(|v| rarity_from(&v)),
        _ => None,
    };
    match parsed {
        Some(map) if !map.is_empty() => {
            let _ = fs::create_dir_all(cache_dir);
            let _ = crate::state_file::write_bytes(&file, serde_json::to_string(&map).unwrap_or_default().as_bytes());
            map
        }
        _ => cached().unwrap_or_default(),
    }
}

fn rarity_from(v: &serde_json::Value) -> HashMap<String, f32> {
    v.pointer("/achievementpercentages/achievements")
        .and_then(|a| a.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|a| {
                    let name = a.get("name")?.as_str()?.to_string();
                    let pct = match a.get("percent")? {
                        serde_json::Value::Number(n) => n.as_f64()?,
                        serde_json::Value::String(s) => s.parse().ok()?,
                        _ => return None,
                    };
                    Some((name, pct as f32))
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds binary KeyValues for tests.
    fn obj(key: &str, items: Vec<u8>) -> Vec<u8> {
        let mut out = vec![0];
        out.extend(key.as_bytes());
        out.push(0);
        out.extend(items);
        out.push(8);
        out
    }
    fn text(key: &str, value: &str) -> Vec<u8> {
        let mut out = vec![1];
        out.extend(key.as_bytes());
        out.push(0);
        out.extend(value.as_bytes());
        out.push(0);
        out
    }
    fn int(key: &str, value: i32) -> Vec<u8> {
        let mut out = vec![2];
        out.extend(key.as_bytes());
        out.push(0);
        out.extend(value.to_le_bytes());
        out
    }

    fn schema() -> Vec<u8> {
        let display = |en: &str, pt: &str, hidden: i32| {
            obj("display", [obj("name", [text("english", en), text("brazilian", pt)].concat()), text("icon", "abc"), int("hidden", hidden)].concat())
        };
        let bits = obj("bits", [obj("0", [text("name", "ACH_A"), display("Ante Up!", "Aposta!", 0)].concat()), obj("1", [text("name", "ACH_B"), display("Secret", "Segredo", 1)].concat())].concat());
        let mut file = obj("4000", [text("gamename", "Test Game"), obj("stats", obj("1", [int("type", 4), bits].concat()))].concat());
        file.push(8);
        file
    }

    fn user() -> Vec<u8> {
        let mut file = obj("cache", [int("crc", 1), obj("1", [int("data", 0b01), obj("AchievementTimes", int("0", 1_700_000_000))].concat())].concat());
        file.push(8);
        file
    }

    #[test]
    fn reads_achievements_and_unlocks_from_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("UserGameStatsSchema_4000.bin"), schema()).unwrap();
        fs::write(dir.path().join("UserGameStats_77_4000.bin"), user()).unwrap();
        let account = Account { stats_dir: dir.path().to_path_buf(), id: "77".into(), localconfig: None };
        let (name, list) = cerberus_shared::i18n::with_lang(Lang::En, || read_game(&account, 4000)).unwrap();
        assert_eq!(name, "Test Game");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "Ante Up!");
        assert_eq!(list[0].unlocked_at, Some(1_700_000_000));
        assert!(list[1].unlocked_at.is_none() && list[1].hidden);
        assert_eq!(list[0].icon.as_deref(), Some("https://shared.akamai.steamstatic.com/community_assets/images/apps/4000/abc.jpg"));
        let pt = cerberus_shared::i18n::with_lang(Lang::Pt, || read_game(&account, 4000)).unwrap();
        assert_eq!(pt.1[0].name, "Aposta!");
        assert_eq!(apps(&account), vec![4000]);
        let entry = game_entry(4000, name, &list, Some(&(3600, Some(1_700_000_100))));
        assert_eq!((entry.total, entry.unlocked, entry.percent()), (2, 1, 50));
        assert_eq!(entry.last_unlock, Some(1_700_000_000));
    }

    #[test]
    fn a_game_without_unlocks_has_them_all_locked() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("UserGameStatsSchema_4000.bin"), schema()).unwrap();
        let account = Account { stats_dir: dir.path().to_path_buf(), id: "77".into(), localconfig: None };
        let (_, list) = read_game(&account, 4000).unwrap();
        assert!(list.iter().all(|a| a.unlocked_at.is_none()));
    }

    #[test]
    fn broken_files_are_refused_not_panicked_on() {
        assert!(binary_kv(&[0, b'a', 0, 1, b'x']).is_none());
        assert!(binary_kv(&[99]).is_none());
        assert!(binary_kv(&[2, b'k', 0, 1]).is_none());
    }

    #[test]
    fn steam_play_time_comes_from_localconfig() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("localconfig.vdf");
        fs::write(
            &file,
            "\"UserLocalConfigStore\"\n{\n\t\"Software\"\n\t{\n\t\t\"Valve\"\n\t\t{\n\t\t\t\"steam\"\n\t\t\t{\n\t\t\t\t\"apps\"\n\t\t\t\t{\n\t\t\t\t\t\"4000\"\n\t\t\t\t\t{\n\t\t\t\t\t\t\"LastPlayed\"\t\t\"1700000000\"\n\t\t\t\t\t\t\"Playtime\"\t\t\"90\"\n\t\t\t\t\t}\n\t\t\t\t\t\"5\"\n\t\t\t\t\t{\n\t\t\t\t\t\t\"cloud\"\t\t\"1\"\n\t\t\t\t\t}\n\t\t\t\t}\n\t\t\t}\n\t\t}\n\t}\n}\n",
        )
        .unwrap();
        let times = playtimes(&file);
        assert_eq!(times.get(&4000), Some(&(5400, Some(1_700_000_000))));
        assert!(!times.contains_key(&5));
    }

    #[test]
    fn rarity_reads_numbers_and_text() {
        let v = serde_json::json!({"achievementpercentages": {"achievements": [{"name": "A", "percent": 12.5}, {"name": "B", "percent": "0.3"}]}});
        let map = rarity_from(&v);
        assert_eq!(map.get("A"), Some(&12.5));
        assert_eq!(map.get("B"), Some(&0.3));
    }
}
