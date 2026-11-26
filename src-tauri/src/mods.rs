//! Mods from Thunderstore for recomps that have a community there
//! (thunderstore.io/c/zelda-64-recompiled and the like).
//!
//! The N64: Recompiled family reads mods from `<data folder>/mods`, where the data
//! folder is named after the program (`Zelda64Recompiled.exe` keeps its files in
//! `%LOCALAPPDATA%\Zelda64Recompiled`), or from `mods` beside the game when it
//! runs portable. A mod's zip can hold more than the game reads; only the files
//! it loads by itself are put in place, and the launcher remembers which, so
//! removing a mod takes exactly those out.

use cerberus_shared::tr;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use cerberus_shared::ModItem;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const COMMUNITIES_FRESH: Duration = Duration::from_secs(24 * 60 * 60);
const LIST_FRESH: Duration = Duration::from_secs(60 * 60);
const MAX_MOD_BYTES: u64 = 512 * 1024 * 1024;

/// Where a recomp's mods come from and go.
#[derive(Debug, Clone, PartialEq)]
pub struct ModSource {
    pub community: String,
    /// The game's data folder name, e.g. `Zelda64Recompiled`.
    pub data: String,
}

/// Recomps whose community and data folder are known for sure.
const KNOWN: &[(&str, &str, &str)] = &[
    ("zelda64recomp", "zelda-64-recompiled", "Zelda64Recompiled"),
    ("banjorecomp", "banjo-recompiled", "BanjoRecompiled"),
];

/// "Zelda 64: Recompiled" -> "zelda-64-recompiled", as Thunderstore names communities.
pub fn slug(name: &str) -> String {
    let lower = name.to_lowercase().replace(['\'', '’'], "");
    let words: Vec<&str> = lower.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect();
    words.join("-")
}

pub fn plain(name: &str) -> bool {
    !name.is_empty() && name.len() < 80 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') && !name.contains("..")
}

/// The mod source for a recomp: a known one, or (for the N64: Recompiled family,
/// whose program is named `...Recompiled`) the community named after the recomp.
pub fn source_for(recomp_id: &str, recomp_name: &str, exe: &Path, communities: &[String]) -> Option<ModSource> {
    if let Some((_, community, data)) = KNOWN.iter().find(|(id, _, _)| *id == recomp_id) {
        return Some(ModSource { community: community.to_string(), data: data.to_string() });
    }
    let stem = exe.file_stem()?.to_string_lossy().into_owned();
    if !stem.ends_with("Recompiled") || !plain(&stem) {
        return None;
    }
    let community = slug(recomp_name);
    if !communities.contains(&community) {
        return None;
    }
    Some(ModSource { community, data: stem })
}

/// Where the game reads mods: beside it when it runs portable, else its data folder.
pub fn mods_dir(install_dir: &Path, data: &str) -> PathBuf {
    if install_dir.join("portable.txt").is_file() {
        return install_dir.join("mods");
    }
    let env = |name: &str| std::env::var_os(name).map(PathBuf::from);
    let base = if cfg!(windows) {
        env("LOCALAPPDATA")
    } else {
        env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")))
    };
    base.unwrap_or_else(|| install_dir.to_path_buf()).join(data).join("mods")
}

fn age(file: &Path) -> Option<Duration> {
    let modified = fs::metadata(file).and_then(|m| m.modified()).ok()?;
    SystemTime::now().duration_since(modified).ok()
}

async fn get_json(client: &Client, url: &str) -> Result<Value, String> {
    let reply = client.get(url).timeout(Duration::from_secs(60)).send().await.map_err(|_| tr!("Sem conexão com o Thunderstore.").to_string())?;
    if !reply.status().is_success() {
        return Err(tr!("O Thunderstore respondeu {}.", reply.status().as_u16()));
    }
    reply.json().await.map_err(|e| e.to_string())
}

/// Every Thunderstore community's id, kept on disk for a day.
pub async fn communities(client: &Client, cache: &Path) -> Vec<String> {
    let file = cache.join("communities.json");
    let fresh = age(&file).is_some_and(|a| a < COMMUNITIES_FRESH);
    let on_disk: Option<Vec<String>> = fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok());
    if let (true, Some(list)) = (fresh, &on_disk) {
        return list.clone();
    }
    let mut ids = Vec::new();
    let mut next = Some("https://thunderstore.io/api/experimental/community/?page_size=100".to_string());
    // A few pages at most; there are a few hundred communities.
    for _ in 0..20 {
        let Some(url) = next.take() else {
            break;
        };
        let Ok(page) = get_json(client, &url).await else {
            return on_disk.unwrap_or_default();
        };
        let found = page.get("results").and_then(Value::as_array).into_iter().flatten();
        ids.extend(found.filter_map(|c| c.get("identifier")?.as_str().map(str::to_string)));
        next = page.pointer("/pagination/next_link").and_then(Value::as_str).map(str::to_string);
    }
    let _ = fs::create_dir_all(cache);
    let _ = fs::write(&file, serde_json::to_string(&ids).unwrap_or_default());
    ids
}

/// One mod, as Thunderstore lists it (its newest version).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub full_name: String,
    pub name: String,
    pub owner: String,
    pub description: String,
    pub icon: String,
    pub version: String,
    pub download_url: String,
    pub page: String,
    pub downloads: u64,
    pub rating: u64,
    /// Other mods it needs, as `Owner-Name`.
    pub dependencies: Vec<String>,
}

fn parse_packages(list: &Value) -> Vec<Package> {
    let mut out = Vec::new();
    for p in list.as_array().into_iter().flatten() {
        let flag = |key: &str| p.get(key).and_then(Value::as_bool).unwrap_or(false);
        if flag("is_deprecated") || flag("has_nsfw_content") {
            continue;
        }
        let versions = p.get("versions").and_then(Value::as_array);
        let Some(newest) = versions.and_then(|v| v.first()) else {
            continue;
        };
        let text = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
        let downloads = versions.into_iter().flatten().filter_map(|v| v.get("downloads")?.as_u64()).sum();
        let dependencies = newest
            .get("dependencies")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            // "Owner-Name-1.2.3" -> "Owner-Name"
            .map(|d| d.rsplit_once('-').map_or(d, |(name, _)| name).to_string())
            .collect();
        out.push(Package {
            full_name: text(p, "full_name"),
            name: text(p, "name").replace('_', " "),
            owner: text(p, "owner"),
            description: text(newest, "description"),
            icon: text(newest, "icon"),
            version: text(newest, "version_number"),
            download_url: text(newest, "download_url"),
            page: text(p, "package_url"),
            downloads,
            rating: p.get("rating_score").and_then(Value::as_u64).unwrap_or(0),
            dependencies,
        });
    }
    out.sort_by_key(|p| std::cmp::Reverse(p.downloads));
    out
}

/// Every mod in a community, most downloaded first; kept on disk for an hour.
pub async fn packages(client: &Client, cache: &Path, community: &str) -> Result<Vec<Package>, String> {
    if !plain(community) {
        return Err(tr!("Comunidade de mods inválida.").into());
    }
    let file = cache.join(format!("{community}.json"));
    let fresh = age(&file).is_some_and(|a| a < LIST_FRESH);
    let on_disk = fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
    let list = match (fresh, on_disk) {
        (true, Some(list)) => list,
        (_, on_disk) => {
            let url = format!("https://thunderstore.io/c/{community}/api/v1/package/");
            match get_json(client, &url).await {
                Ok(list) => {
                    let _ = fs::create_dir_all(cache);
                    let _ = fs::write(&file, list.to_string());
                    list
                }
                Err(error) => on_disk.ok_or(error)?,
            }
        }
    };
    Ok(parse_packages(&list))
}

/// What the launcher put in a game's mods folder: mod -> (version, files).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Installed(pub HashMap<String, (String, Vec<String>)>);

impl Installed {
    /// The files this mod placed before (an update may replace them).
    pub fn files_of(&self, mod_name: &str) -> Vec<String> {
        self.0.get(mod_name).map(|(_, files)| files.clone()).unwrap_or_default()
    }

    /// The files the other mods placed: never replaced nor removed for this one.
    pub fn others_files(&self, mod_name: &str) -> std::collections::HashSet<String> {
        self.0.iter().filter(|(name, _)| name.as_str() != mod_name).flat_map(|(_, (_, files))| files.iter().map(|f| f.to_lowercase())).collect()
    }
}

/// Refuses names already in the mods folder that aren't this mod's own: a
/// file the player put there by hand, or another mod's, is never replaced.
pub fn check_free(mods: &Path, names: &[String], mine: &[String]) -> Result<(), String> {
    let mine: std::collections::HashSet<String> = mine.iter().map(|f| f.to_lowercase()).collect();
    if let Some(taken) = names.iter().find(|n| !mine.contains(&n.to_lowercase()) && mods.join(n.as_str()).exists()) {
        return Err(tr!("Já existe {} na pasta de mods, de outro mod ou posto à mão; nada foi instalado. Remova-o primeiro se quiser trocar.", taken));
    }
    Ok(())
}

/// The record of a game's mods; a damaged one falls back to its last good copy
/// (and is kept aside), never silently to "no mods".
pub fn load_installed(file: &Path) -> Installed {
    crate::state_file::load_or_default(file)
}

pub fn save_installed(file: &Path, installed: &Installed) -> Result<(), String> {
    crate::state_file::save(file, installed)
}

/// Files the game loads by itself: N64: Recompiled mods, texture packs, and the
/// native library some mods bring for this system.
fn loads_itself(name: &str) -> bool {
    let lower = name.to_lowercase();
    let library = if cfg!(windows) {
        ".dll"
    } else if cfg!(target_os = "macos") {
        ".dylib"
    } else {
        ".so"
    };
    lower.ends_with(".nrm") || lower.ends_with(".rtz") || lower.ends_with(library)
}

/// Puts a mod zip's files the game reads into its mods folder; returns their names.
/// `mine`: the files this mod placed before, which it may replace.
pub fn unpack(zip_bytes: &[u8], mods: &Path, mine: &[String]) -> Result<Vec<String>, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).map_err(|e| tr!("O mod está corrompido: {}", e))?;
    // Every name first: one that belongs to someone else stops it before anything is written.
    let names: Vec<String> = (0..zip.len())
        .filter_map(|i| zip.by_index(i).ok().filter(|e| !e.is_dir()).and_then(|e| e.enclosed_name()).and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())))
        .filter(|n| loads_itself(n))
        .collect();
    check_free(mods, &names, mine)?;
    fs::create_dir_all(mods).map_err(|e| e.to_string())?;
    let mut placed = Vec::new();
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        // Only the bare file name is kept, so nothing can land outside the mods folder.
        let Some(name) = entry.enclosed_name().and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())) else {
            continue;
        };
        if !loads_itself(&name) {
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        let temp = mods.join(format!(".{name}.part"));
        fs::write(&temp, &bytes).map_err(|e| e.to_string())?;
        fs::rename(&temp, mods.join(&name)).map_err(|e| e.to_string())?;
        placed.push(name);
    }
    if placed.is_empty() {
        return Err(tr!("Esse mod não tem arquivos que o jogo carregue sozinho; a página dele explica como instalar.").into());
    }
    Ok(placed)
}

/// Downloads a mod, at most `MAX_MOD_BYTES`.
pub async fn download(client: &Client, url: &str) -> Result<Vec<u8>, String> {
    if !url.starts_with("https://") {
        return Err(tr!("Endereço de download inválido.").into());
    }
    let reply = client.get(url).timeout(Duration::from_secs(30 * 60)).send().await.map_err(|_| tr!("Sem conexão com o Thunderstore.").to_string())?;
    if !reply.status().is_success() {
        return Err(tr!("O Thunderstore respondeu {}.", reply.status().as_u16()));
    }
    // Read as it comes, up to the limit, even when the server doesn't say the size.
    crate::net::read_limited(reply, MAX_MOD_BYTES).await.map_err(|_| tr!("Esse mod é grande demais, ou o download falhou.").to_string())
}

/// Takes a mod's own files out of the mods folder; one another mod also lists stays.
pub fn remove_files(mods: &Path, files: &[String], others: &std::collections::HashSet<String>) {
    for name in files {
        let plain_name = Path::new(name).components().count() == 1;
        if plain_name && !others.contains(&name.to_lowercase()) {
            let _ = fs::remove_file(mods.join(name));
        }
    }
}

/// The UI's row for a mod.
pub fn item(p: &Package, installed: &Installed) -> ModItem {
    ModItem {
        full_name: p.full_name.clone(),
        name: p.name.clone(),
        owner: p.owner.clone(),
        description: p.description.clone(),
        icon: p.icon.clone(),
        version: p.version.clone(),
        page: p.page.clone(),
        downloads: p.downloads,
        installed: installed.0.get(&p.full_name).map(|(v, _)| v.clone()),
        likes: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_of_another_mod_is_neither_replaced_nor_removed() {
        use std::io::Write;
        let zip_of = |files: &[(&str, &str)]| {
            let mut buf = std::io::Cursor::new(Vec::new());
            let mut zip = zip::ZipWriter::new(&mut buf);
            for (name, body) in files {
                zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
                zip.write_all(body.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
            buf.into_inner()
        };
        let dir = tempfile::tempdir().unwrap();
        let mods = dir.path().join("mods");
        fs::create_dir_all(&mods).unwrap();
        // The player's own pack, put there by hand.
        fs::write(mods.join("cool.nrm"), "mine").unwrap();
        assert!(unpack(&zip_of(&[("cool.nrm", "theirs")]), &mods, &[]).unwrap_err().contains("cool.nrm"));
        assert_eq!(fs::read_to_string(mods.join("cool.nrm")).unwrap(), "mine");
        // Two mods sharing a pack: removing one keeps it for the other.
        let mut installed = Installed::default();
        let a = unpack(&zip_of(&[("shared.nrm", "x"), ("a.nrm", "a")]), &mods, &[]).unwrap();
        installed.0.insert("A".into(), ("1".into(), a.clone()));
        installed.0.insert("B".into(), ("1".into(), vec!["shared.nrm".into()]));
        remove_files(&mods, &a, &installed.others_files("A"));
        assert!(mods.join("shared.nrm").exists() && !mods.join("a.nrm").exists());
        // An update may replace its own files.
        assert!(unpack(&zip_of(&[("shared.nrm", "y")]), &mods, &["shared.nrm".into()]).is_ok());
    }
    use std::io::Write;

    #[test]
    fn names_communities_like_thunderstore() {
        assert_eq!(slug("Zelda 64: Recompiled"), "zelda-64-recompiled");
        assert_eq!(slug("Banjo-Kazooie: Nuts & Bolts Recompiled"), "banjo-kazooie-nuts-bolts-recompiled");
    }

    #[test]
    fn knows_the_big_two_and_finds_the_rest_by_name() {
        let communities = vec!["smash-64-recompiled".to_string()];
        let zelda = source_for("zelda64recomp", "Zelda 64: Recompiled", Path::new("Zelda64Recompiled.exe"), &[]).unwrap();
        assert_eq!(zelda.data, "Zelda64Recompiled");
        let smash = source_for("smash64", "Smash 64 Recompiled", Path::new(r"C:\g\Smash64Recompiled.exe"), &communities).unwrap();
        assert_eq!(smash.community, "smash-64-recompiled");
        assert_eq!(smash.data, "Smash64Recompiled");
        assert!(source_for("x", "Some Port", Path::new("game.exe"), &communities).is_none());
    }

    #[test]
    fn reads_a_package_list() {
        let list = serde_json::json!([
            {"full_name": "A-Cool_Mod", "name": "Cool_Mod", "owner": "A", "package_url": "https://t/a", "rating_score": 3,
             "versions": [{"version_number": "1.1.0", "download_url": "https://t/a.zip", "downloads": 5, "description": "d",
                           "icon": "https://t/i.png", "dependencies": ["B-Lib-2.0.0"]},
                          {"version_number": "1.0.0", "downloads": 7}]},
            {"full_name": "Old-Mod", "is_deprecated": true, "versions": [{}]}
        ]);
        let packages = parse_packages(&list);
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].name, "Cool Mod");
        assert_eq!(packages[0].downloads, 12);
        assert_eq!(packages[0].dependencies, vec!["B-Lib"]);
    }

    #[test]
    fn unpacks_only_what_the_game_loads() {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        for (name, body) in [("mods/cool.nrm", "mod"), ("README.md", "read me"), ("../../evil.nrm", "x"), ("icon.png", "png")] {
            zip.start_file(name, options).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        let bytes = zip.finish().unwrap().into_inner();
        let dir = std::env::temp_dir().join(format!("cerberus-mods-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let placed = unpack(&bytes, &dir, &[]).unwrap();
        assert_eq!(placed, vec!["cool.nrm"]);
        assert!(dir.join("cool.nrm").is_file());
        assert!(!dir.join("README.md").exists());
    }
}
