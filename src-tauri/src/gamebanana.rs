//! Mods from GameBanana (gamebanana.com), whose API is public and needs no
//! account, for the ports that read packs from a `mods` folder beside their
//! program: the Harbour Masters family (Ship of Harkinian, Starship,
//! SpaghettiKart, Ghostship), which load `.otr` and `.o2r` files by themselves;
//! and Unleashed and Marathon Recompiled, whose mods are folders with a
//! `mod.ini` that their mod loader turns on from a list (see `hedge`).
//!
//! A mod on GameBanana can offer several files; the newest one that passed
//! GameBanana's virus scan and comes as a .zip or .7z is the one installed.
//! Only the packs inside go into the mods folder, and the launcher remembers
//! which (as for Thunderstore), so removing a mod takes exactly those out.

use cerberus_shared::tr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use cerberus_shared::ModItem;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::AsyncWriteExt;

const API: &str = "https://gamebanana.com/apiv11";
const LIST_FRESH: Duration = Duration::from_secs(60 * 60);
/// Texture packs can be large; more than this is not installed from here.
const MAX_FILE_BYTES: u64 = 4 << 30;
/// The most downloaded mods, in pages of 50.
const PAGES: u32 = 3;

/// How a game reads its mods.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// .otr/.o2r packs in `mods`.
    Packs,
    /// Folders with a mod.ini, listed in ModsDB.ini; the game's user folder's name.
    Hedge(&'static str),
}

/// Recomps with a GameBanana game page whose mods install from here.
const GAMES: &[(&str, u64, Kind)] = &[
    ("shipwright", 16121, Kind::Packs),
    ("starship", 21612, Kind::Packs),
    ("spaghettikart", 22970, Kind::Packs),
    ("ghostship", 24131, Kind::Packs),
    ("unleashedrecomp", 21975, Kind::Hedge("UnleashedRecomp")),
    ("marathonrecomp", 23176, Kind::Hedge("MarathonRecomp")),
];

/// The recomp's game on GameBanana, when its mods install from here.
pub fn game_for(recomp_id: &str) -> Option<(u64, Kind)> {
    GAMES.iter().find(|(id, ..)| *id == recomp_id).map(|(_, game, kind)| (*game, *kind))
}

/// The packs go beside the program, in `mods`.
pub fn mods_dir(program: &Path) -> PathBuf {
    program.parent().unwrap_or(program).join("mods")
}

/// One mod, as GameBanana lists it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Listed {
    pub id: u64,
    pub name: String,
    pub author: String,
    pub category: String,
    pub image: String,
    pub page: String,
    pub likes: u64,
    /// The mod's version, or when it was last updated: a change means an update.
    pub version: String,
}

/// `gamebanana:<id>`, the id it has among the game's mods.
pub fn key(id: u64) -> String {
    format!("gamebanana:{id}")
}

pub fn id_of(key: &str) -> Option<u64> {
    key.strip_prefix("gamebanana:")?.parse().ok()
}

fn text(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or_default().trim().to_string()
}

fn number(v: &Value, k: &str) -> u64 {
    v.get(k).and_then(Value::as_u64).unwrap_or(0)
}

fn flag(v: &Value, k: &str) -> bool {
    v.get(k).and_then(Value::as_bool).unwrap_or(false)
}

/// A screenshot's small version, only from GameBanana's own image server.
fn image_of(v: &Value) -> String {
    let first = v.get("_aPreviewMedia").and_then(|m| m.get("_aImages")).and_then(Value::as_array).and_then(|l| l.first());
    let Some(img) = first else { return String::new() };
    let base = text(img, "_sBaseUrl");
    let file = ["_sFile220", "_sFile530", "_sFile"].iter().map(|k| text(img, k)).find(|f| !f.is_empty()).unwrap_or_default();
    if base.starts_with("https://images.gamebanana.com/") && !file.is_empty() && !file.contains(['/', '\\']) {
        format!("{base}/{file}")
    } else {
        String::new()
    }
}

/// The mods of one page of GameBanana's list; obsolete ones, ones without
/// files and ones with a content rating are left out.
fn parse_list(page: &Value) -> Vec<Listed> {
    let records = page.get("_aRecords").and_then(Value::as_array);
    records
        .into_iter()
        .flatten()
        .filter(|r| text(r, "_sModelName") == "Mod" && flag(r, "_bHasFiles") && !flag(r, "_bIsObsolete") && !flag(r, "_bHasContentRatings"))
        .filter_map(|r| {
            let id = number(r, "_idRow");
            let updated = number(r, "_tsDateUpdated").max(number(r, "_tsDateAdded"));
            let version = match text(r, "_sVersion") {
                v if v.is_empty() => format!("{updated}"),
                v => v,
            };
            (id > 0).then(|| Listed {
                id,
                name: text(r, "_sName"),
                author: r.get("_aSubmitter").map(|s| text(s, "_sName")).unwrap_or_default(),
                category: r.get("_aRootCategory").map(|c| text(c, "_sName")).unwrap_or_default(),
                image: image_of(r),
                page: format!("https://gamebanana.com/mods/{id}"),
                likes: number(r, "_nLikeCount"),
                version,
            })
        })
        .collect()
}

async fn get_json(client: &Client, url: &str) -> Result<Value, String> {
    let reply = client.get(url).timeout(Duration::from_secs(30)).send().await.map_err(|_| tr!("Sem conexão com o GameBanana.").to_string())?;
    if !reply.status().is_success() {
        return Err(tr!("O GameBanana respondeu {}.", reply.status().as_u16()));
    }
    reply.json().await.map_err(|_| tr!("Resposta inesperada do GameBanana.").to_string())
}

fn age(file: &Path) -> Option<Duration> {
    SystemTime::now().duration_since(fs::metadata(file).ok()?.modified().ok()?).ok()
}

/// The game's most downloaded mods; kept on disk for an hour.
pub async fn list(client: &Client, cache: &Path, game: u64) -> Result<Vec<Listed>, String> {
    let file = cache.join(format!("gamebanana-{game}.json"));
    let on_disk = fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str::<Vec<Listed>>(&t).ok());
    if let Some(list) = on_disk.as_ref().filter(|_| age(&file).is_some_and(|a| a < LIST_FRESH)) {
        return Ok(list.clone());
    }
    let mut all: Vec<Listed> = Vec::new();
    for page in 1..=PAGES {
        let url = format!("{API}/Mod/Index?_nPerpage=50&_aFilters%5BGeneric_Game%5D={game}&_sSort=Generic_MostDownloaded&_nPage={page}");
        let reply = match get_json(client, &url).await {
            Ok(reply) => reply,
            // Old info beats none.
            Err(error) if all.is_empty() => return on_disk.ok_or(error),
            Err(_) => break,
        };
        let found = parse_list(&reply);
        let complete = reply.get("_aMetadata").is_some_and(|m| flag(m, "_bIsComplete"));
        all.extend(found.into_iter().filter(|m| !all.iter().any(|seen| seen.id == m.id)).collect::<Vec<_>>());
        if complete {
            break;
        }
    }
    let _ = fs::create_dir_all(cache);
    let _ = crate::state_file::write_bytes(&file, serde_json::to_string(&all).unwrap_or_default().as_bytes());
    Ok(all)
}

/// The file of a mod to install.
#[derive(Debug, Clone, PartialEq)]
pub struct ModFile {
    pub id: u64,
    pub name: String,
    pub size: u64,
}

/// The newest file that passed the virus scan and the launcher can open.
fn pick_file(profile: &Value) -> Result<ModFile, String> {
    if flag(profile, "_bIsTrashed") || flag(profile, "_bIsWithheld") || flag(profile, "_bHasContentRatings") {
        return Err(tr!("Esse mod não está disponível por aqui.").into());
    }
    let files = profile.get("_aFiles").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut usable: Vec<&Value> = files.iter().filter(|f| text(f, "_sAvResult") == "clean").collect();
    usable.sort_by_key(|f| std::cmp::Reverse(number(f, "_tsDateAdded")));
    let opens = |name: &str| {
        let lower = name.to_lowercase();
        lower.ends_with(".zip") || lower.ends_with(".7z")
    };
    if let Some(f) = usable.iter().find(|f| opens(&text(f, "_sFile"))) {
        return Ok(ModFile { id: number(f, "_idRow"), name: text(f, "_sFile"), size: number(f, "_nFilesize") });
    }
    if usable.iter().any(|f| text(f, "_sFile").to_lowercase().ends_with(".rar")) {
        return Err(tr!("Esse mod vem em .rar, que o Cerberus ainda não abre. A página dele explica como instalar à mão.").into());
    }
    Err(tr!("Esse mod não tem um arquivo .zip ou .7z aprovado pelo antivírus do GameBanana.").into())
}

/// Which file of the mod would be installed, asking GameBanana.
pub async fn file_of(client: &Client, id: u64) -> Result<ModFile, String> {
    let profile = get_json(client, &format!("{API}/Mod/{id}/ProfilePage")).await?;
    pick_file(&profile)
}

/// Downloads a mod's file into `dir`, streamed to disk; returns its path.
pub async fn download(client: &Client, file: &ModFile, dir: &Path) -> Result<PathBuf, String> {
    if file.size > MAX_FILE_BYTES {
        return Err(tr!("Esse mod é grande demais para instalar por aqui.").into());
    }
    let reply = client
        .get(format!("https://gamebanana.com/dl/{}", file.id))
        .timeout(Duration::from_secs(60 * 60))
        .send()
        .await
        .map_err(|_| tr!("Sem conexão com o GameBanana.").to_string())?;
    if !reply.status().is_success() {
        return Err(tr!("O GameBanana respondeu {}.", reply.status().as_u16()));
    }
    let expected = reply.content_length().unwrap_or(file.size);
    if expected > MAX_FILE_BYTES {
        return Err(tr!("Esse mod é grande demais para instalar por aqui.").into());
    }
    crate::disk::require(dir, expected.saturating_mul(2))?;
    tokio::fs::create_dir_all(dir).await.map_err(|e| e.to_string())?;
    let extension = if file.name.to_lowercase().ends_with(".7z") { "7z" } else { "zip" };
    let path = dir.join(format!("gamebanana-{}.{extension}", file.id));
    let mut out = tokio::fs::File::create(&path).await.map_err(|e| e.to_string())?;
    let mut written = 0u64;
    let mut stream = reply.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| tr!("A conexão caiu durante o download do mod.").to_string())?;
        written += chunk.len() as u64;
        if written > MAX_FILE_BYTES {
            drop(out);
            let _ = tokio::fs::remove_file(&path).await;
            return Err(tr!("Esse mod é grande demais para instalar por aqui.").into());
        }
        out.write_all(&chunk).await.map_err(|e| e.to_string())?;
    }
    out.flush().await.map_err(|e| e.to_string())?;
    Ok(path)
}

/// Packs the game loads by itself: .otr/.o2r (Ship of Harkinian and family),
/// .nrm (N64: Recompiled).
fn is_pack(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".otr") || lower.ends_with(".o2r") || lower.ends_with(".nrm")
}

/// A folder name from a game's own file, kept for as long as the app runs
/// (Hedge Mod Manager mods are switched on in that folder's `cpkredir.ini`).
pub fn named(user: &str) -> &'static str {
    static NAMES: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());
    let mut names = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(found) = names.iter().find(|n| **n == user) {
        return found;
    }
    let kept: &'static str = Box::leak(user.to_string().into_boxed_str());
    names.push(kept);
    kept
}

fn packs_in(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() && depth < 6 {
            packs_in(&entry.path(), depth + 1, out);
        } else if kind.is_file() && is_pack(&entry.file_name().to_string_lossy()) {
            out.push(entry.path());
        }
    }
}

/// Unpacks the archive in a temporary folder and moves its packs into
/// `mods`; returns their names. The archive and the folder are removed.
/// `mine`: the packs this mod placed before, which it may replace.
pub fn place_packs(archive: &Path, mods: &Path, mine: &[String]) -> Result<Vec<String>, String> {
    let unpacked = archive.with_extension("unpacked");
    let _ = fs::remove_dir_all(&unpacked);
    let opened = if archive.extension().is_some_and(|e| e == "7z") {
        crate::recomp::extract_7z(archive, &unpacked)
    } else {
        crate::recomp::extract_zip(archive, &unpacked)
    };
    let placed = opened.and_then(|()| {
        let mut packs = Vec::new();
        packs_in(&unpacked, 0, &mut packs);
        if packs.is_empty() {
            return Err(tr!("Esse mod não tem pacotes .otr, .o2r ou .nrm que o jogo carregue sozinho; a página dele explica como instalar.").into());
        }
        let wanted: Vec<String> = packs.iter().filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())).collect();
        crate::mods::check_free(mods, &wanted, mine)?;
        fs::create_dir_all(mods).map_err(|e| e.to_string())?;
        let mut names = Vec::new();
        for pack in packs {
            // Only the bare file name is kept, so nothing lands outside the mods folder.
            let Some(name) = pack.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
            let target = mods.join(&name);
            if fs::rename(&pack, &target).is_err() {
                fs::copy(&pack, &target).map_err(|e| e.to_string())?;
            }
            if !names.contains(&name) {
                names.push(name);
            }
        }
        Ok(names)
    });
    let _ = fs::remove_dir_all(&unpacked);
    let _ = fs::remove_file(archive);
    placed
}

/// The UI's row for a mod.
pub fn item(m: &Listed, installed: &crate::mods::Installed) -> ModItem {
    let key = key(m.id);
    ModItem {
        installed: installed.0.get(&key).map(|(v, _)| v.clone()),
        full_name: key,
        name: m.name.clone(),
        owner: m.author.clone(),
        description: m.category.clone(),
        icon: m.image.clone(),
        version: m.version.clone(),
        page: m.page.clone(),
        downloads: 0,
        likes: m.likes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn knows_the_harbour_masters_ports() {
        assert_eq!(game_for("shipwright"), Some((16121, Kind::Packs)));
        assert_eq!(game_for("unleashedrecomp"), Some((21975, Kind::Hedge("UnleashedRecomp"))));
        assert_eq!(game_for("zelda64recomp"), None);
        assert_eq!(mods_dir(Path::new(r"C:\Games\soh\soh.exe")), Path::new(r"C:\Games\soh\mods"));
        assert_eq!(id_of(&key(477979)), Some(477979));
        assert_eq!(id_of("Owner-Name"), None);
    }

    #[test]
    fn reads_a_list_page() {
        let page = serde_json::json!({"_aRecords": [
            {"_idRow": 1, "_sModelName": "Mod", "_sName": "3DS Link", "_bHasFiles": true, "_sVersion": "6.0.2", "_nLikeCount": 331,
             "_aSubmitter": {"_sName": "Djipi"}, "_aRootCategory": {"_sName": "Skins"},
             "_aPreviewMedia": {"_aImages": [{"_sBaseUrl": "https://images.gamebanana.com/img/ss/mods", "_sFile": "a.jpg", "_sFile220": "220-90_a.jpg"}]}},
            {"_idRow": 2, "_sModelName": "Mod", "_sName": "Old", "_bHasFiles": true, "_bIsObsolete": true},
            {"_idRow": 3, "_sModelName": "Mod", "_sName": "Rated", "_bHasFiles": true, "_bHasContentRatings": true},
            {"_idRow": 4, "_sModelName": "Mod", "_sName": "No version", "_bHasFiles": true, "_tsDateUpdated": 1700000000,
             "_aPreviewMedia": {"_aImages": [{"_sBaseUrl": "https://evil.example/img", "_sFile": "a.jpg"}]}}
        ]});
        let list = parse_list(&page);
        assert_eq!(list.iter().map(|m| m.id).collect::<Vec<_>>(), vec![1, 4]);
        assert_eq!(list[0].image, "https://images.gamebanana.com/img/ss/mods/220-90_a.jpg");
        assert_eq!(list[0].author, "Djipi");
        assert_eq!(list[1].version, "1700000000");
        assert_eq!(list[1].image, "", "pictures only from GameBanana");
    }

    #[test]
    fn picks_the_newest_clean_archive() {
        let profile = serde_json::json!({"_aFiles": [
            {"_idRow": 10, "_sFile": "old.zip", "_sAvResult": "clean", "_tsDateAdded": 1},
            {"_idRow": 11, "_sFile": "new.7z", "_sAvResult": "clean", "_tsDateAdded": 3},
            {"_idRow": 12, "_sFile": "newest.zip", "_sAvResult": "pending", "_tsDateAdded": 5}
        ]});
        assert_eq!(pick_file(&profile).unwrap().id, 11);
        let rar = serde_json::json!({"_aFiles": [{"_idRow": 1, "_sFile": "a.rar", "_sAvResult": "clean"}]});
        assert!(pick_file(&rar).unwrap_err().contains(".rar"));
        let gone = serde_json::json!({"_bIsTrashed": true, "_aFiles": []});
        assert!(pick_file(&gone).is_err());
    }

    #[test]
    fn places_only_the_packs() {
        let root = tempfile::tempdir().unwrap();
        let archive = root.path().join("gamebanana-1.zip");
        let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        for (name, body) in [("Pack/textures.o2r", "o2r"), ("readme.txt", "hi"), ("alt/Model.OTR", "otr")] {
            zip.start_file(name, options).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        let mods = root.path().join("game").join("mods");
        let mut placed = place_packs(&archive, &mods, &[]).unwrap();
        placed.sort();
        assert_eq!(placed, vec!["Model.OTR", "textures.o2r"]);
        assert_eq!(fs::read_to_string(mods.join("textures.o2r")).unwrap(), "o2r");
        assert!(!mods.join("readme.txt").exists());
        assert!(!archive.exists(), "the download is removed");
    }
}

#[cfg(test)]
#[test]
#[ignore = "asks GameBanana for Ship of Harkinian's mods and installs a small one in a temporary folder"]
fn a_real_mod_installs() {
    let client = Client::builder().user_agent("CerberusLauncher").build().unwrap();
    let temp = tempfile::tempdir().unwrap();
    tauri::async_runtime::block_on(async {
        let list = list(&client, temp.path(), 16121).await.unwrap();
        println!("{} mods; first: {} by {}", list.len(), list[0].name, list[0].author);
        for m in list.iter().take(40) {
            let Ok(file) = file_of(&client, m.id).await else { continue };
            if file.size > 3_000_000 {
                continue;
            }
            let archive = download(&client, &file, &temp.path().join("dl")).await.unwrap();
            match place_packs(&archive, &temp.path().join("mods"), &[]) {
                Ok(placed) => {
                    println!("{} ({}): {placed:?}", m.name, file.name);
                    return;
                }
                Err(e) => println!("{}: {e}", m.name),
            }
        }
        panic!("no small mod found");
    });
}
