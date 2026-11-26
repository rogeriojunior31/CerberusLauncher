//! A copy of the player's own Cerberus data in one `.zip`, to keep or to carry
//! to another PC: the library and what they added to it (favorites,
//! collections, notes, custom art), play history, and settings.
//!
//! Keys and tokens never go into the copy. Restoring keeps this PC's keys,
//! folders and accounts, saves what's here first (in `before-restore-<time>`),
//! and only takes the files a copy can have, so a zip from elsewhere can't
//! write anything else.

use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use cerberus_shared::tr;
use serde_json::Value;

/// The files a copy holds, beside the data folder's `covers/` pictures.
const FILES: [&str; 9] = [
    "library.json",
    "favorites.json",
    "collections.json",
    "game-notes.json",
    "classics.json",
    "art.json",
    "sessions.jsonl",
    "trail.jsonl",
    "store-playtime.json",
];
const SETTINGS: &str = "settings.json";
const MARK: &str = "cerberus-backup.json";
/// Never written into a copy.
/// (The store accounts' refresh tokens too: a copy must never sign someone in.)
const SECRETS: [&str; 6] = ["steam_key", "ra_key", "ra_token", "steamgriddb_key", "epic_refresh", "xbox_refresh"];
/// Belong to this PC: a restore leaves them as they are.
/// (The store accounts signed in here stay too: a restore never swaps or signs out an account.)
const THIS_PC: [&str; 15] = [
    "steam_key", "steam_id", "ra_user", "ra_key", "ra_token", "steamgriddb_key", "recomps_dir", "rom_dirs", "cloud_dir", "retroarch", "monitor",
    "epic_user", "epic_refresh", "xbox_user", "xbox_refresh",
];

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default()
}

/// A picture name from `covers/`, with nothing that could leave the folder.
fn plain_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':']) && !name.starts_with('.')
}

/// Writes the copy to `out`; returns how many files went in.
pub fn export(data: &Path, out: &Path) -> Result<usize, String> {
    let file = fs::File::create(out).map_err(|e| tr!("Não foi possível criar a cópia: {}", e))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut count = 0;
    let mut put = |zip: &mut zip::ZipWriter<fs::File>, name: &str, bytes: &[u8]| -> Result<(), String> {
        zip.start_file(name, options).map_err(|e| e.to_string())?;
        zip.write_all(bytes).map_err(|e| e.to_string())?;
        count += 1;
        Ok(())
    };
    let mark = serde_json::json!({ "app": "Cerberus Launcher", "version": env!("CARGO_PKG_VERSION"), "made": now() });
    put(&mut zip, MARK, mark.to_string().as_bytes())?;
    for name in FILES {
        if let Ok(bytes) = fs::read(data.join(name)) {
            put(&mut zip, name, &bytes)?;
        }
    }
    if let Ok(text) = fs::read_to_string(data.join(SETTINGS)) {
        let mut settings: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        if let Some(map) = settings.as_object_mut() {
            for key in SECRETS {
                map.remove(key);
            }
        }
        put(&mut zip, SETTINGS, serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?.as_bytes())?;
    }
    if let Ok(entries) = fs::read_dir(data.join("covers")) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if entry.path().is_file() && plain_name(&name) {
                if let Ok(bytes) = fs::read(entry.path()) {
                    put(&mut zip, &format!("covers/{name}"), &bytes)?;
                }
            }
        }
    }
    zip.finish().map_err(|e| e.to_string())?;
    // The mark doesn't count as one of the player's files.
    Ok(count - 1)
}

/// Where a copy waits, checked and unpacked, until Cerberus starts again.
fn staging(data: &Path) -> std::path::PathBuf {
    data.join("restore-staging")
}

fn pending_mark(data: &Path) -> std::path::PathBuf {
    data.join("restore-pending.json")
}

/// A file of a copy bigger than this is refused (a copy holds text and small pictures).
const MOST_PER_FILE: u64 = 256 * 1024 * 1024;

/// Checks a copy and unpacks it beside the data, to be put in place the next
/// time Cerberus starts (`apply_pending`), before anything reads the data: the
/// running app (its library in memory, the play-time counter) can't write over
/// it. Returns how many files the copy has.
pub fn stage(data: &Path, from: &Path) -> Result<usize, String> {
    let file = fs::File::open(from).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|_| tr!("Esse arquivo não é uma cópia do Cerberus.").to_string())?;
    if zip.by_name(MARK).is_err() {
        return Err(tr!("Esse arquivo não é uma cópia do Cerberus.").into());
    }
    let stage = staging(data);
    let _ = fs::remove_dir_all(&stage);
    fs::create_dir_all(stage.join("covers")).map_err(|e| e.to_string())?;
    let mut count = 0;
    let result = (|| -> Result<(), String> {
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
            let name = entry.name().to_string();
            let target = if FILES.contains(&name.as_str()) || name == SETTINGS {
                stage.join(&name)
            } else if let Some(picture) = name.strip_prefix("covers/").filter(|n| plain_name(n)) {
                stage.join("covers").join(picture)
            } else {
                // The mark, or anything a copy doesn't have.
                continue;
            };
            if entry.size() > MOST_PER_FILE {
                return Err(tr!("A cópia tem um arquivo grande demais ({}): ela não parece ser do Cerberus.", name));
            }
            let mut bytes = Vec::new();
            entry.by_ref().take(MOST_PER_FILE + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            if name == SETTINGS {
                // Checked now, so a broken copy is refused before the restart.
                merged_settings(data, &String::from_utf8_lossy(&bytes))?;
            }
            crate::state_file::write_bytes(&target, &bytes)?;
            count += 1;
        }
        Ok(())
    })();
    if let Err(e) = result {
        let _ = fs::remove_dir_all(&stage);
        return Err(e);
    }
    crate::state_file::write_bytes(&pending_mark(data), now().to_string().as_bytes())?;
    Ok(count)
}

/// Puts a staged copy in place (called first thing when Cerberus starts).
/// What's here is kept first in `before-restore-<time>`; if that can't be
/// done, nothing is replaced. Play time is merged (the larger of the two for
/// each game), so hours played since the copy was made aren't lost.
/// `None` when no copy was waiting.
pub fn apply_pending(data: &Path) -> Option<Result<(usize, std::path::PathBuf), String>> {
    if !pending_mark(data).exists() {
        return None;
    }
    let result = apply(data);
    let _ = fs::remove_file(pending_mark(data));
    let _ = fs::remove_dir_all(staging(data));
    prune_before(data, 3);
    Some(result)
}

fn apply(data: &Path) -> Result<(usize, std::path::PathBuf), String> {
    let stage = staging(data);
    // What's here now, kept before anything is replaced: all of it, or nothing is replaced.
    let before = data.join(format!("before-restore-{}", now()));
    fs::create_dir_all(before.join("covers")).map_err(|e| e.to_string())?;
    for name in FILES.iter().chain([&SETTINGS]) {
        let here = data.join(name);
        if here.exists() {
            fs::copy(&here, before.join(name)).map_err(|e| tr!("Não foi possível guardar os dados atuais antes de trazer a cópia ({}): nada foi trocado.", e))?;
        }
    }
    for entry in fs::read_dir(data.join("covers")).into_iter().flatten().flatten() {
        fs::copy(entry.path(), before.join("covers").join(entry.file_name())).map_err(|e| tr!("Não foi possível guardar os dados atuais antes de trazer a cópia ({}): nada foi trocado.", e))?;
    }
    let mut count = 0;
    for name in FILES {
        let incoming = stage.join(name);
        if !incoming.exists() {
            continue;
        }
        if matches!(name, "store-playtime.json" | "classics.json") {
            merge_time(&data.join(name), &incoming)?;
        }
        replace(&incoming, &data.join(name))?;
        count += 1;
    }
    if stage.join(SETTINGS).exists() {
        let text = fs::read_to_string(stage.join(SETTINGS)).map_err(|e| e.to_string())?;
        let settings = merged_settings(data, &text)?;
        crate::settings::save(data, &settings)?;
        count += 1;
    }
    fs::create_dir_all(data.join("covers")).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(stage.join("covers")).into_iter().flatten().flatten() {
        replace(&entry.path(), &data.join("covers").join(entry.file_name()))?;
        count += 1;
    }
    Ok((count, before))
}

fn replace(from: &Path, to: &Path) -> Result<(), String> {
    let temp = to.with_extension("restoring");
    fs::copy(from, &temp).map_err(|e| e.to_string())?;
    fs::rename(&temp, to).map_err(|e| e.to_string())
}

/// Play time in the copy (`incoming`) merged with this PC's: for each game the
/// larger time and the latest day, written back into `incoming`.
fn merge_time(here: &Path, incoming: &Path) -> Result<(), String> {
    let read = |p: &Path| -> serde_json::Map<String, Value> { fs::read_to_string(p).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default() };
    let mut theirs = read(incoming);
    for (game, mine) in read(here) {
        let num = |v: &Value, k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0);
        match theirs.get_mut(&game) {
            Some(other) if other.is_object() => {
                if let Some(obj) = other.as_object_mut() {
                    for key in ["seconds", "last"] {
                        let best = num(&mine, key).max(obj.get(key).and_then(Value::as_u64).unwrap_or(0));
                        obj.insert(key.into(), Value::from(best));
                    }
                }
            }
            _ => {
                theirs.insert(game, mine);
            }
        }
    }
    crate::state_file::write_bytes(incoming, serde_json::to_string_pretty(&theirs).map_err(|e| e.to_string())?.as_bytes())
}

/// Keeps only the newest `keep` `before-restore-*` folders.
fn prune_before(data: &Path, keep: usize) {
    let mut found: Vec<_> = fs::read_dir(data).into_iter().flatten().flatten().filter(|e| e.file_name().to_string_lossy().starts_with("before-restore-")).map(|e| e.path()).collect();
    found.sort();
    let extra = found.len().saturating_sub(keep);
    for old in found.into_iter().take(extra) {
        let _ = fs::remove_dir_all(old);
    }
}

/// Puts a copy back at once (staged, then applied); returns how many files came
/// from it and where the previous data was saved. The app uses `stage` and
/// applies it when it starts again.
#[cfg(test)]
pub fn restore(data: &Path, from: &Path) -> Result<(usize, std::path::PathBuf), String> {
    stage(data, from)?;
    apply_pending(data).unwrap_or_else(|| Err(tr!("Esse arquivo não é uma cópia do Cerberus.").into()))
}

/// The copy's settings, with this PC's keys, accounts and folders kept.
fn merged_settings(data: &Path, text: &str) -> Result<crate::settings::Settings, String> {
    let mut incoming: Value = serde_json::from_str(text).map_err(|_| tr!("As configurações da cópia estão corrompidas.").to_string())?;
    let current: Value = fs::read_to_string(data.join(SETTINGS)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
    let Some(map) = incoming.as_object_mut() else { return Err(tr!("As configurações da cópia estão corrompidas.").into()) };
    for key in THIS_PC {
        match current.get(key) {
            Some(v) => {
                map.insert(key.to_string(), v.clone());
            }
            None => {
                map.remove(key);
            }
        }
    }
    // Checked against the app's own settings before it's written.
    serde_json::from_value(incoming).map_err(|_| tr!("As configurações da cópia estão corrompidas.").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_staged_copy_waits_for_the_restart_and_keeps_the_hours_played_since() {
        let old_pc = tempfile::tempdir().unwrap();
        write(old_pc.path(), "library.json", "[]");
        write(old_pc.path(), "store-playtime.json", r#"{"steam:1":{"seconds":100,"last":10},"steam:2":{"seconds":50,"last":5}}"#);
        let zip = old_pc.path().join("copy.zip");
        export(old_pc.path(), &zip).unwrap();

        let here = tempfile::tempdir().unwrap();
        write(here.path(), "library.json", r#"[{"x":1}]"#);
        write(here.path(), "store-playtime.json", r#"{"steam:1":{"seconds":900,"last":99}}"#);
        stage(here.path(), &zip).unwrap();
        // Nothing replaced while the app runs.
        assert_eq!(fs::read_to_string(here.path().join("library.json")).unwrap(), r#"[{"x":1}]"#);
        let (_, before) = apply_pending(here.path()).unwrap().unwrap();
        assert_eq!(fs::read_to_string(here.path().join("library.json")).unwrap(), "[]");
        assert_eq!(fs::read_to_string(before.join("library.json")).unwrap(), r#"[{"x":1}]"#);
        let time: Value = serde_json::from_str(&fs::read_to_string(here.path().join("store-playtime.json")).unwrap()).unwrap();
        assert_eq!(time["steam:1"]["seconds"], 900, "this PC's larger time stays");
        assert_eq!(time["steam:1"]["last"], 99);
        assert_eq!(time["steam:2"]["seconds"], 50, "the copy's games come in");
        assert!(apply_pending(here.path()).is_none(), "applied once");
    }

    fn write(dir: &Path, name: &str, text: &str) {
        fs::write(dir.join(name), text).unwrap();
    }

    #[test]
    fn a_copy_leaves_keys_out_and_a_restore_keeps_this_pcs() {
        let old_pc = tempfile::tempdir().unwrap();
        write(old_pc.path(), "favorites.json", r#"["steam:1"]"#);
        write(old_pc.path(), "trail.jsonl", "{\"a\":1}\n");
        write(old_pc.path(), SETTINGS, r#"{"steam_key":"SECRET","ra_token":"TOKEN","start_page":"library","recomps_dir":"D:\\old"}"#);
        fs::create_dir_all(old_pc.path().join("covers")).unwrap();
        write(&old_pc.path().join("covers"), "steam_1-cover.jpg", "jpg");
        let zip = old_pc.path().join("copy.zip");
        assert_eq!(export(old_pc.path(), &zip).unwrap(), 4);
        let mut text = String::new();
        let mut archive = zip::ZipArchive::new(fs::File::open(&zip).unwrap()).unwrap();
        archive.by_name(SETTINGS).unwrap().read_to_string(&mut text).unwrap();
        assert!(!text.contains("SECRET") && !text.contains("TOKEN"), "{text}");

        let new_pc = tempfile::tempdir().unwrap();
        write(new_pc.path(), SETTINGS, r#"{"steam_key":"MINE","recomps_dir":"E:\\new"}"#);
        write(new_pc.path(), "favorites.json", r#"["epic:x"]"#);
        let (count, before) = restore(new_pc.path(), &zip).unwrap();
        assert_eq!(count, 4);
        assert_eq!(fs::read_to_string(new_pc.path().join("favorites.json")).unwrap(), r#"["steam:1"]"#);
        assert_eq!(fs::read_to_string(before.join("favorites.json")).unwrap(), r#"["epic:x"]"#, "what was here is kept");
        assert!(new_pc.path().join("covers").join("steam_1-cover.jpg").exists());
        let settings = crate::settings::load(new_pc.path());
        assert_eq!(settings.steam_key.as_deref(), Some("MINE"));
        assert_eq!(settings.recomps_dir.as_deref(), Some(Path::new("E:\\new")));
        assert_eq!(settings.start_page, "library", "choices come from the copy");
        assert!(settings.ra_token.is_none());
    }

    #[test]
    fn store_accounts_never_go_into_a_copy_nor_change_on_restore() {
        let old_pc = tempfile::tempdir().unwrap();
        write(old_pc.path(), SETTINGS, r#"{"epic_user":"Old","epic_refresh":"EPIC-TOKEN","xbox_user":"OldTag","xbox_refresh":"XBOX-TOKEN","start_page":"library"}"#);
        let zip = old_pc.path().join("copy.zip");
        export(old_pc.path(), &zip).unwrap();
        let mut text = String::new();
        zip::ZipArchive::new(fs::File::open(&zip).unwrap()).unwrap().by_name(SETTINGS).unwrap().read_to_string(&mut text).unwrap();
        assert!(!text.contains("EPIC-TOKEN") && !text.contains("XBOX-TOKEN"), "{text}");

        // A PC signed in to other accounts keeps them; one signed out stays signed out.
        let new_pc = tempfile::tempdir().unwrap();
        write(new_pc.path(), SETTINGS, r#"{"epic_user":"Mine","epic_refresh":"MY-TOKEN"}"#);
        restore(new_pc.path(), &zip).unwrap();
        let settings = crate::settings::load(new_pc.path());
        assert_eq!((settings.epic_user.as_deref(), settings.epic_refresh.as_deref()), (Some("Mine"), Some("MY-TOKEN")));
        assert!(settings.xbox_user.is_none() && settings.xbox_refresh.is_none(), "no account appears from a copy");
        assert_eq!(settings.start_page, "library");
    }

    /// With the real data folder (read only): its copy, put back into an empty
    /// folder, has the same files, and no key.
    /// `cargo test -p cerberus-launcher real_backup -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_backup() {
        let real = std::path::PathBuf::from(std::env::var("APPDATA").unwrap()).join("app.cerberus.launcher");
        let out = tempfile::tempdir().unwrap();
        let zip = out.path().join("copy.zip");
        let count = export(&real, &zip).unwrap();
        println!("{count} files, {} KB", fs::metadata(&zip).unwrap().len() / 1024);
        let mut text = String::new();
        zip::ZipArchive::new(fs::File::open(&zip).unwrap()).unwrap().by_name(SETTINGS).unwrap().read_to_string(&mut text).unwrap();
        for key in SECRETS {
            assert!(!text.contains(&format!("\"{key}\"")), "{key} went into the copy");
        }
        let fresh = out.path().join("fresh");
        fs::create_dir_all(&fresh).unwrap();
        let (back, _) = restore(&fresh, &zip).unwrap();
        assert_eq!(back, count);
        for name in FILES {
            if matches!(name, "store-playtime.json" | "classics.json") {
                // Merged play time is written again: the same values, maybe in another order.
                let parse = |p: std::path::PathBuf| fs::read_to_string(p).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
                assert_eq!(parse(real.join(name)), parse(fresh.join(name)), "{name}");
                continue;
            }
            let a = fs::read(real.join(name)).ok();
            assert_eq!(a, fs::read(fresh.join(name)).ok(), "{name}");
        }
        let settings = crate::settings::load(&fresh);
        assert!(settings.steam_key.is_none() && settings.ra_token.is_none(), "an empty PC gets no keys");
        assert!(settings.epic_refresh.is_none() && settings.xbox_refresh.is_none(), "an empty PC gets no store account");
        println!("restored {back} files, the same as the originals");
    }

    #[test]
    fn a_zip_that_isnt_a_copy_or_reaches_out_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("plain.zip");
        let mut zip = zip::ZipWriter::new(fs::File::create(&plain).unwrap());
        zip.start_file("library.json", zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(b"[]").unwrap();
        zip.finish().unwrap();
        assert!(restore(dir.path(), &plain).is_err(), "no mark, not a copy");

        let sly = dir.path().join("sly.zip");
        let mut zip = zip::ZipWriter::new(fs::File::create(&sly).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        zip.start_file(MARK, o).unwrap();
        zip.write_all(b"{}").unwrap();
        zip.start_file("covers/../../evil.txt", o).unwrap();
        zip.write_all(b"x").unwrap();
        zip.start_file("other.exe", o).unwrap();
        zip.write_all(b"x").unwrap();
        zip.finish().unwrap();
        let data = dir.path().join("data");
        fs::create_dir_all(&data).unwrap();
        let (count, _) = restore(&data, &sly).unwrap();
        assert_eq!(count, 0);
        assert!(!dir.path().join("evil.txt").exists() && !data.join("other.exe").exists());
    }
}
