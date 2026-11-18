//! Keeps Cerberus's own recomp catalog (`src-tauri/catalog/recomps.json`) up to date.
//!
//!   cargo run -p catalog-builder -- update     refresh what GitHub knows about each entry
//!   cargo run -p catalog-builder -- discover   look for new projects, for review
//!
//! `update` only touches what comes from GitHub: the release (tag and date), each
//! system's file with its SHA-256 (so the launcher downloads it directly), the
//! file-name pattern, size, stars and systems. Everything written by hand (names,
//! descriptions, save places, how the game copy is handed over) stays as it is.
//!
//! `discover` never adds to the catalog itself: it writes `catalog/candidates.json`,
//! which a person reviews (the weekly Action opens a pull request with it).

mod assets;
mod github;

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use assets::{SYSTEMS, pattern, pick};
use github::GitHub;

const CATALOG: &str = "src-tauri/catalog/recomps.json";
const CANDIDATES: &str = "src-tauri/catalog/candidates.json";

/// Searches that find recompilations, decompilation ports and native PC ports.
const SEARCHES: &[&str] = &[
    "recompiled in:name fork:false archived:false",
    "recomp in:name fork:false archived:false",
    "topic:static-recompilation fork:false",
    "topic:n64recomp fork:false",
    "topic:decompilation pc port fork:false archived:false",
];
/// Fewer stars than this is usually a personal experiment, not a playable release.
const MIN_STARS: u64 = 20;

fn main() {
    let command = std::env::args().nth(1).unwrap_or_default();
    let root = workspace_root();
    let result = match command.as_str() {
        "update" => update(&root),
        "discover" => discover(&root),
        _ => Err("usage: catalog-builder <update|discover>".to_string()),
    };
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

/// The repository's top folder, whether run from it or from this tool's folder.
fn workspace_root() -> PathBuf {
    let here = std::env::current_dir().expect("a working folder");
    here.ancestors().find(|dir| dir.join(CATALOG).is_file()).map(Path::to_path_buf).unwrap_or(here)
}

fn read_list(path: &Path) -> Result<Vec<Value>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn write_list(path: &Path, list: &[Value]) -> Result<(), String> {
    let mut text = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    text.push('\n');
    fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// The GitHub-derived fields of one entry, from its repository and newest release.
fn automatic_fields(gh: &GitHub, repo: &str) -> Result<Option<Map<String, Value>>, String> {
    let info = gh.repo(repo)?;
    let Some(latest) = gh.latest(repo)? else {
        return Ok(None);
    };
    let mut files = Map::new();
    let mut patterns = Map::new();
    let mut systems = Vec::new();
    let mut size = 0;
    for system in SYSTEMS {
        let Some(file) = pick(&latest.files, system) else {
            continue;
        };
        systems.push(json!(system));
        patterns.insert(system.into(), json!(pattern(&file.name, &latest.tag)));
        if let Some(sha256) = &file.sha256 {
            files.insert(system.into(), json!({ "name": file.name, "sha256": sha256 }));
        }
        if system == "windows" || size == 0 {
            size = file.size;
        }
    }
    if systems.is_empty() {
        return Ok(None);
    }
    let mut fields = Map::new();
    fields.insert("tag".into(), json!(latest.tag));
    fields.insert("released".into(), json!(latest.date));
    fields.insert("systems".into(), Value::Array(systems));
    fields.insert("assets".into(), Value::Object(patterns));
    fields.insert("files".into(), Value::Object(files));
    fields.insert("size_text".into(), json!(format!("{} MB", (size as f64 / 1_048_576.0).ceil() as u64)));
    fields.insert("stars".into(), json!(info.stargazers_count));
    Ok(Some(fields))
}

/// Puts the automatic fields into an entry, leaving everything written by hand alone.
fn merge_automatic(entry: &mut Value, fields: Map<String, Value>) {
    let Some(object) = entry.as_object_mut() else {
        return;
    };
    for (key, value) in fields {
        object.insert(key, value);
    }
}

fn update(root: &Path) -> Result<(), String> {
    let gh = GitHub::new();
    let path = root.join(CATALOG);
    let mut list = read_list(&path)?;
    let mut changed = 0;
    for entry in &mut list {
        let Some(repo) = entry.get("repo").and_then(Value::as_str).map(str::to_string) else {
            continue;
        };
        match automatic_fields(&gh, &repo) {
            Ok(Some(fields)) => {
                let before = entry.clone();
                merge_automatic(entry, fields);
                if *entry != before {
                    changed += 1;
                    println!("atualizado: {repo}");
                }
            }
            Ok(None) => println!("sem release utilizável: {repo}"),
            // One project failing (renamed, private now) mustn't stop the rest.
            Err(error) => eprintln!("erro em {repo}: {error}"),
        }
    }
    write_list(&path, &list)?;
    println!("{changed} entradas atualizadas de {}.", list.len());
    Ok(())
}

/// `owner/Some_Game-Recomp` -> `some-game-recomp`: a plain id the launcher uses as a folder name.
fn id_for(repo_name: &str) -> String {
    let lower = repo_name.to_lowercase();
    let mut id: String = lower.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    while id.contains("--") {
        id = id.replace("--", "-");
    }
    id.trim_matches('-').to_string()
}

/// A candidate entry: the automatic fields filled in, the curated ones left for a person.
fn candidate(repo: &github::Repo, fields: Map<String, Value>) -> Value {
    let mut entry = Map::new();
    entry.insert("id".into(), json!(id_for(&repo.name)));
    entry.insert("title".into(), json!(id_for(&repo.name)));
    entry.insert("name".into(), json!(repo.name.replace(['-', '_'], " ")));
    entry.insert("repo".into(), json!(repo.full_name));
    entry.insert("about".into(), json!(repo.description.clone().unwrap_or_default()));
    entry.insert("license".into(), json!(repo.license.as_ref().and_then(|l| l.spdx_id.clone()).unwrap_or_default()));
    // For the reviewer to fill in: which game, which console, what the player must bring.
    entry.insert("game".into(), json!(""));
    entry.insert("console".into(), json!(""));
    entry.insert("origin".into(), json!(""));
    entry.insert("needs".into(), json!(""));
    entry.insert("cover".into(), Value::Null);
    entry.insert("hero".into(), Value::Null);
    entry.insert("icon".into(), Value::Null);
    entry.insert("saves".into(), json!([]));
    let mut value = Value::Object(entry);
    merge_automatic(&mut value, fields);
    value
}

fn discover(root: &Path) -> Result<(), String> {
    let gh = GitHub::new();
    if !gh.has_token() {
        eprintln!("aviso: sem GITHUB_TOKEN o GitHub limita a 60 consultas por hora; a busca pode parar no meio.");
    }
    let catalog = read_list(&root.join(CATALOG))?;
    let known: Vec<String> = catalog.iter().filter_map(|e| e.get("repo")?.as_str().map(str::to_lowercase)).collect();
    let mut seen: Vec<String> = Vec::new();
    let mut candidates = Vec::new();
    for query in SEARCHES {
        let repos = match gh.search(query) {
            Ok(repos) => repos,
            Err(error) => {
                eprintln!("busca \"{query}\": {error}");
                continue;
            }
        };
        for repo in repos {
            let key = repo.full_name.to_lowercase();
            if known.contains(&key) || seen.contains(&key) || repo.fork || repo.archived || repo.stargazers_count < MIN_STARS {
                continue;
            }
            seen.push(key);
            // Only projects with a release the launcher can install are worth a reviewer's time.
            match automatic_fields(&gh, &repo.full_name) {
                Ok(Some(fields)) => {
                    println!("candidato: {} ({} estrelas)", repo.full_name, repo.stargazers_count);
                    candidates.push(candidate(&repo, fields));
                }
                Ok(None) => {}
                Err(error) => eprintln!("erro em {}: {error}", repo.full_name),
            }
        }
    }
    candidates.sort_by_key(|c| std::cmp::Reverse(c.get("stars").and_then(Value::as_u64).unwrap_or(0)));
    write_list(&root.join(CANDIDATES), &candidates)?;
    println!("{} candidatos em {CANDIDATES}, para revisão.", candidates.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_plain_folder_names() {
        assert_eq!(id_for("Zelda64Recomp"), "zelda64recomp");
        assert_eq!(id_for("Some_Game--Recomp."), "some-game-recomp");
    }

    #[test]
    fn automatic_fields_never_replace_curated_ones() {
        let mut entry = json!({"id": "x", "about": "Escrito à mão", "saves": ["{local}/X/saves"], "tag": "v1"});
        let mut fields = Map::new();
        fields.insert("tag".into(), json!("v2"));
        fields.insert("stars".into(), json!(10));
        merge_automatic(&mut entry, fields);
        assert_eq!(entry["tag"], "v2");
        assert_eq!(entry["about"], "Escrito à mão");
        assert_eq!(entry["saves"][0], "{local}/X/saves");
    }

    #[test]
    fn candidates_leave_the_curated_fields_for_a_person() {
        let repo = github::Repo {
            full_name: "dev/Cool_Game-Recomp".into(),
            name: "Cool_Game-Recomp".into(),
            description: Some("A recompilation".into()),
            stargazers_count: 50,
            archived: false,
            fork: false,
            license: None,
        };
        let entry = candidate(&repo, Map::new());
        assert_eq!(entry["id"], "cool-game-recomp");
        assert_eq!(entry["game"], "");
        assert_eq!(entry["about"], "A recompilation");
    }
}
