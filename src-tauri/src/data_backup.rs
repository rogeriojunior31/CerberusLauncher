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
const SECRETS: [&str; 4] = ["steam_key", "ra_key", "ra_token", "steamgriddb_key"];
/// Belong to this PC: a restore leaves them as they are.
const THIS_PC: [&str; 11] = [
    "steam_key", "steam_id", "ra_user", "ra_key", "ra_token", "steamgriddb_key", "recomps_dir", "rom_dirs", "cloud_dir", "retroarch", "monitor",
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

/// Puts a copy back; returns how many files came from it and where the
/// previous data was saved.
pub fn restore(data: &Path, from: &Path) -> Result<(usize, std::path::PathBuf), String> {
    let file = fs::File::open(from).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|_| tr!("Esse arquivo não é uma cópia do Cerberus.").to_string())?;
    if zip.by_name(MARK).is_err() {
        return Err(tr!("Esse arquivo não é uma cópia do Cerberus.").into());
    }
    // What's here now, kept before anything is replaced.
    let before = data.join(format!("before-restore-{}", now()));
    fs::create_dir_all(before.join("covers")).map_err(|e| e.to_string())?;
    for name in FILES.iter().chain([&SETTINGS]) {
        let _ = fs::copy(data.join(name), before.join(name));
    }
    if let Ok(entries) = fs::read_dir(data.join("covers")) {
        for entry in entries.flatten() {
            let _ = fs::copy(entry.path(), before.join("covers").join(entry.file_name()));
        }
    }

    let mut count = 0;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        let target = if FILES.contains(&name.as_str()) {
            data.join(&name)
        } else if let Some(picture) = name.strip_prefix("covers/").filter(|n| plain_name(n)) {
            fs::create_dir_all(data.join("covers")).map_err(|e| e.to_string())?;
            data.join("covers").join(picture)
        } else if name == SETTINGS {
            let mut text = String::new();
            entry.read_to_string(&mut text).map_err(|e| e.to_string())?;
            restore_settings(data, &text)?;
            count += 1;
            continue;
        } else {
            // The mark, or anything a copy doesn't have.
            continue;
        };
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        let temp = target.with_extension("restoring");
        fs::write(&temp, &bytes).map_err(|e| e.to_string())?;
        fs::rename(&temp, &target).map_err(|e| e.to_string())?;
        count += 1;
    }
    Ok((count, before))
}

/// The copy's settings, with this PC's keys, accounts and folders kept.
fn restore_settings(data: &Path, text: &str) -> Result<(), String> {
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
    let settings: crate::settings::Settings = serde_json::from_value(incoming).map_err(|_| tr!("As configurações da cópia estão corrompidas.").to_string())?;
    crate::settings::save(data, &settings)
}

#[cfg(test)]
mod tests {
    use super::*;

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
            let a = fs::read(real.join(name)).ok();
            assert_eq!(a, fs::read(fresh.join(name)).ok(), "{name}");
        }
        let settings = crate::settings::load(&fresh);
        assert!(settings.steam_key.is_none() && settings.ra_token.is_none(), "an empty PC gets no keys");
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
