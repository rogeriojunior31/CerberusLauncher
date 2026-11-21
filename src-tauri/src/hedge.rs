//! Mods for Unleashed Recompiled and Marathon Recompiled, the way their
//! built-in mod loader reads them: each mod is a folder with a `mod.ini`, and
//! the loader turns on the ones listed in a `ModsDB.ini`, whose place a
//! `cpkredir.ini` in the game's user folder names. These are the same files
//! Hedge Mod Manager writes, so a player who uses it keeps their list: the
//! launcher only adds and takes out its own lines (ids `cerberus_gb_<mod>`).

use std::fs;
use std::path::{Path, PathBuf};

/// An .ini file kept in order: sections with their keys and values.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Ini(Vec<(String, Vec<(String, String)>)>);

impl Ini {
    pub fn parse(text: &str) -> Ini {
        let mut ini = Ini::default();
        for line in text.trim_start_matches('\u{feff}').lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                ini.0.push((name.trim().to_string(), Vec::new()));
                continue;
            }
            let Some((key, value)) = line.split_once('=') else { continue };
            let value = value.trim();
            let value = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')).unwrap_or(value);
            if ini.0.is_empty() {
                ini.0.push((String::new(), Vec::new()));
            }
            let section = &mut ini.0.last_mut().expect("a section").1;
            section.push((key.trim().to_string(), value.to_string()));
        }
        ini
    }

    fn section(&self, name: &str) -> Option<&Vec<(String, String)>> {
        self.0.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, keys)| keys)
    }

    fn section_mut(&mut self, name: &str) -> &mut Vec<(String, String)> {
        let at = match self.0.iter().position(|(n, _)| n.eq_ignore_ascii_case(name)) {
            Some(at) => at,
            None => {
                self.0.push((name.to_string(), Vec::new()));
                self.0.len() - 1
            }
        };
        &mut self.0[at].1
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.section(section)?.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
    }

    pub fn set(&mut self, section: &str, key: &str, value: &str) {
        let keys = self.section_mut(section);
        match keys.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
            Some((_, v)) => *v = value.to_string(),
            None => keys.push((key.to_string(), value.to_string())),
        }
    }

    pub fn remove(&mut self, section: &str, key: &str) {
        self.section_mut(section).retain(|(k, _)| !k.eq_ignore_ascii_case(key));
    }

    /// Written as Hedge Mod Manager writes it: every value in quotes.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for (name, keys) in &self.0 {
            if !name.is_empty() {
                out.push_str(&format!("[{name}]\n"));
            }
            for (k, v) in keys {
                out.push_str(&format!("{k}=\"{v}\"\n"));
            }
            out.push('\n');
        }
        out
    }
}

/// The game's user folder: beside the program when it's portable, else
/// `%APPDATA%\<user>`.
pub fn user_dir(user: &str, exe_dir: &Path) -> PathBuf {
    if exe_dir.join("portable.txt").is_file() {
        return exe_dir.to_path_buf();
    }
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| exe_dir.to_path_buf()).join(user)
}

/// The ModsDB.ini the game reads. When there's none yet, one is made in the
/// mods folder and named in a new (or turned on) cpkredir.ini.
fn mods_db(user: &str, exe_dir: &Path) -> Result<PathBuf, String> {
    let cfg_path = user_dir(user, exe_dir).join("cpkredir.ini");
    let mut cfg = Ini::parse(&fs::read_to_string(&cfg_path).unwrap_or_default());
    let named = cfg.get("CPKREDIR", "ModsDbIni").filter(|p| !p.is_empty()).map(PathBuf::from);
    let db = named.clone().unwrap_or_else(|| exe_dir.join("mods").join("ModsDB.ini"));
    let off = cfg.get("CPKREDIR", "Enabled").is_some_and(|v| v == "0" || v.eq_ignore_ascii_case("false"));
    if named.is_none() || off {
        cfg.set("CPKREDIR", "Enabled", "1");
        cfg.set("CPKREDIR", "ModsDbIni", &db.display().to_string());
        if let Some(dir) = cfg_path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        fs::write(&cfg_path, cfg.text()).map_err(|e| format!("Não foi possível gravar o cpkredir.ini: {e}"))?;
    }
    Ok(db)
}

fn active(db: &Ini) -> Vec<String> {
    let count: usize = db.get("Main", "ActiveModCount").and_then(|v| v.parse().ok()).unwrap_or(0);
    (0..count).filter_map(|i| db.get("Main", &format!("ActiveMod{i}")).map(str::to_string)).collect()
}

fn set_active(db: &mut Ini, list: &[String]) {
    let old: usize = db.get("Main", "ActiveModCount").and_then(|v| v.parse().ok()).unwrap_or(0);
    for i in 0..old.max(list.len()) {
        db.remove("Main", &format!("ActiveMod{i}"));
    }
    db.set("Main", "ActiveModCount", &list.len().to_string());
    for (i, id) in list.iter().enumerate() {
        db.set("Main", &format!("ActiveMod{i}"), id);
    }
}

/// Changes ModsDB.ini; the first time the launcher touches someone's file, a
/// copy is kept beside it.
fn change_db(path: &Path, change: impl FnOnce(&mut Ini)) -> Result<(), String> {
    let before = fs::read_to_string(path).ok();
    let backup = path.with_extension("ini.cerberus.bak");
    if before.is_some() && !backup.exists() {
        let _ = fs::copy(path, &backup);
    }
    let mut db = Ini::parse(before.as_deref().unwrap_or_default());
    if db.get("Main", "ActiveModCount").is_none() {
        db.set("Main", "ActiveModCount", "0");
    }
    db.section_mut("Mods");
    change(&mut db);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(path, db.text()).map_err(|e| format!("Não foi possível gravar o ModsDB.ini: {e}"))
}

pub fn db_id(mod_id: u64) -> String {
    format!("cerberus_gb_{mod_id}")
}

/// The folders with a mod.ini, leaving out ones inside another.
fn mod_folders(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let entries: Vec<_> = entries.flatten().collect();
    if entries.iter().any(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case("mod.ini") && e.path().is_file()) {
        out.push(dir.to_path_buf());
        return;
    }
    for entry in entries {
        if depth < 6 && entry.file_type().is_ok_and(|t| t.is_dir()) {
            mod_folders(&entry.path(), depth + 1, out);
        }
    }
}

fn mod_ini(dir: &Path) -> Option<PathBuf> {
    fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| p.is_file() && p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("mod.ini")))
}

/// A folder name Windows takes.
fn plain(name: &str) -> String {
    let cleaned: String = name.chars().map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { ' ' } else { c }).collect();
    let cleaned = cleaned.trim().trim_end_matches('.').trim().to_string();
    if cleaned.is_empty() || cleaned.starts_with('.') { String::new() } else { cleaned.chars().take(80).collect() }
}

fn move_dir(from: &Path, to: &Path) -> Result<(), String> {
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    crate::recomp::copy_tree(from, to).map_err(|e| format!("Não foi possível copiar o mod: {e}"))
}

/// Takes out folders the launcher put in `mods` (only plain names holding a mod.ini).
fn remove_folders(mods: &Path, names: &[String]) {
    for name in names {
        let plain_name = Path::new(name).components().count() == 1 && !name.contains("..");
        let dir = mods.join(name);
        if plain_name && mod_ini(&dir).is_some() {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

/// Unpacks the mod, moves its folder into `mods` and turns it on in
/// ModsDB.ini; returns the folder's name. `old` are the folders an earlier
/// version left, replaced. The archive and the unpacked copy are removed.
pub fn place(archive: &Path, mods: &Path, user: &str, mod_id: u64, name: &str, old: &[String]) -> Result<Vec<String>, String> {
    let unpacked = archive.with_extension("unpacked");
    let _ = fs::remove_dir_all(&unpacked);
    let opened = if archive.extension().is_some_and(|e| e == "7z") {
        crate::recomp::extract_7z(archive, &unpacked)
    } else {
        crate::recomp::extract_zip(archive, &unpacked)
    };
    let placed = opened.and_then(|()| {
        let mut found = Vec::new();
        mod_folders(&unpacked, 0, &mut found);
        let folder = match found.as_slice() {
            [] => return Err("Esse mod não tem mod.ini, então o jogo não o carrega. A página dele explica como instalar.".into()),
            [one] => one.clone(),
            many => {
                let names: Vec<String> = many.iter().filter_map(|d| d.file_name()).map(|n| n.to_string_lossy().into_owned()).take(4).collect();
                return Err(format!("Esse mod vem em várias versões ({}); escolha uma na página dele e instale à mão.", names.join(", ")));
            }
        };
        remove_folders(mods, old);
        let own = folder.file_name().filter(|_| folder != unpacked).map(|n| plain(&n.to_string_lossy())).unwrap_or_default();
        let base = [own, plain(name)].into_iter().find(|n| !n.is_empty()).unwrap_or_else(|| format!("gamebanana-{mod_id}"));
        // Someone else's folder with that name stays.
        let target_name = if mods.join(&base).exists() { format!("{base} (gb {mod_id})") } else { base };
        let target = mods.join(&target_name);
        let _ = fs::remove_dir_all(&target);
        fs::create_dir_all(mods).map_err(|e| e.to_string())?;
        move_dir(&folder, &target)?;
        let ini = mod_ini(&target).ok_or("O mod.ini sumiu ao mover o mod.")?;
        let exe_dir = mods.parent().unwrap_or(mods);
        let db = mods_db(user, exe_dir)?;
        let id = db_id(mod_id);
        change_db(&db, |db| {
            db.set("Mods", &id, &ini.display().to_string());
            let mut list = active(db);
            if !list.contains(&id) {
                list.push(id.clone());
            }
            set_active(db, &list);
        })?;
        Ok(vec![target_name])
    });
    let _ = fs::remove_dir_all(&unpacked);
    let _ = fs::remove_file(archive);
    placed
}

/// Takes the mod out of ModsDB.ini and its folders out of `mods`.
pub fn remove(mods: &Path, user: &str, mod_id: u64, folders: &[String]) -> Result<(), String> {
    let exe_dir = mods.parent().unwrap_or(mods);
    let cfg = Ini::parse(&fs::read_to_string(user_dir(user, exe_dir).join("cpkredir.ini")).unwrap_or_default());
    if let Some(db) = cfg.get("CPKREDIR", "ModsDbIni").filter(|p| !p.is_empty()).map(PathBuf::from).filter(|p| p.is_file()) {
        let id = db_id(mod_id);
        change_db(&db, |db| {
            db.remove("Mods", &id);
            let list: Vec<String> = active(db).into_iter().filter(|m| *m != id).collect();
            set_active(db, &list);
        })?;
    }
    remove_folders(mods, folders);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip_of(path: &Path, files: &[(&str, &str)]) {
        let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
        for (name, body) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn reads_and_writes_ini_like_hedge_mod_manager() {
        let ini = Ini::parse("\u{feff}[Main]\nActiveModCount=\"1\"\nActiveMod0=\"theirs\"\n; note\n[Mods]\ntheirs=\"C:\\x\\mod.ini\"\n");
        assert_eq!(ini.get("main", "activemodcount"), Some("1"));
        assert_eq!(ini.get("Mods", "theirs"), Some("C:\\x\\mod.ini"));
        assert_eq!(Ini::parse(&ini.text()), ini);
    }

    #[test]
    fn installs_beside_someone_elses_list_and_takes_only_its_own_out() {
        let root = tempfile::tempdir().unwrap();
        let game = root.path().join("game");
        fs::create_dir_all(&game).unwrap();
        fs::write(game.join("portable.txt"), "").unwrap();
        // Hedge Mod Manager is in use already: its list is kept.
        let theirs = root.path().join("hmm").join("ModsDB.ini");
        fs::create_dir_all(theirs.parent().unwrap()).unwrap();
        fs::write(&theirs, "[Main]\nActiveModCount=\"1\"\nActiveMod0=\"theirs\"\n\n[Mods]\ntheirs=\"C:\\x\\mod.ini\"\n").unwrap();
        fs::write(game.join("cpkredir.ini"), format!("[CPKREDIR]\nEnabled=\"1\"\nModsDbIni=\"{}\"\n", theirs.display())).unwrap();
        let mods = game.join("mods");
        fs::create_dir_all(mods.join("Chip Miku")).unwrap();
        fs::write(mods.join("Chip Miku").join("readme.txt"), "someone else's").unwrap();

        let archive = root.path().join("gamebanana-7.zip");
        zip_of(&archive, &[("Pack/Chip Miku/mod.ini", "[Desc]\nTitle=\"Miku\"\n"), ("Pack/Chip Miku/disk/a.ar", "ar"), ("Pack/readme.txt", "hi")]);
        let placed = place(&archive, &mods, "UnleashedRecomp", 7, "Miku", &[]).unwrap();
        assert_eq!(placed, vec!["Chip Miku (gb 7)"]);
        assert!(mods.join("Chip Miku (gb 7)/disk/a.ar").is_file());
        assert!(mods.join("Chip Miku/readme.txt").is_file(), "the folder already there stays");
        let db = Ini::parse(&fs::read_to_string(&theirs).unwrap());
        assert_eq!(active(&db), vec!["theirs", "cerberus_gb_7"]);
        assert!(db.get("Mods", "cerberus_gb_7").unwrap().ends_with("mod.ini"));
        assert!(theirs.with_extension("ini.cerberus.bak").is_file());

        remove(&mods, "UnleashedRecomp", 7, &placed).unwrap();
        let db = Ini::parse(&fs::read_to_string(&theirs).unwrap());
        assert_eq!(active(&db), vec!["theirs"]);
        assert_eq!(db.get("Mods", "theirs"), Some("C:\\x\\mod.ini"));
        assert_eq!(db.get("Mods", "cerberus_gb_7"), None);
        assert!(!mods.join("Chip Miku (gb 7)").exists());
        assert!(mods.join("Chip Miku").exists());
    }

    #[test]
    fn makes_the_list_when_there_is_none_and_refuses_unclear_mods() {
        let root = tempfile::tempdir().unwrap();
        let game = root.path().join("game");
        fs::create_dir_all(&game).unwrap();
        fs::write(game.join("portable.txt"), "").unwrap();
        let mods = game.join("mods");
        let archive = root.path().join("gamebanana-9.zip");
        zip_of(&archive, &[("mod.ini", "[Main]\n"), ("a.ar", "ar")]);
        assert_eq!(place(&archive, &mods, "MarathonRecomp", 9, "Blue: Shoes?", &[]).unwrap(), vec!["Blue  Shoes"]);
        let cfg = Ini::parse(&fs::read_to_string(game.join("cpkredir.ini")).unwrap());
        assert_eq!(cfg.get("CPKREDIR", "Enabled"), Some("1"));
        let db = PathBuf::from(cfg.get("CPKREDIR", "ModsDbIni").unwrap());
        assert_eq!(db, mods.join("ModsDB.ini"));
        assert_eq!(active(&Ini::parse(&fs::read_to_string(&db).unwrap())), vec!["cerberus_gb_9"]);

        let two = root.path().join("gamebanana-10.zip");
        zip_of(&two, &[("Red/mod.ini", ""), ("Blue/mod.ini", "")]);
        assert!(place(&two, &mods, "MarathonRecomp", 10, "x", &[]).unwrap_err().contains("várias versões"));
        let none = root.path().join("gamebanana-11.zip");
        zip_of(&none, &[("readme.txt", "")]);
        assert!(place(&none, &mods, "MarathonRecomp", 11, "x", &[]).unwrap_err().contains("mod.ini"));
    }
}

#[cfg(test)]
#[test]
#[ignore = "asks GameBanana for Unleashed Recompiled's mods and installs a small one in a temporary portable game"]
fn a_real_unleashed_mod_installs() {
    use crate::gamebanana;
    let client = reqwest::Client::builder().user_agent("CerberusLauncher").build().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let game = temp.path().join("game");
    fs::create_dir_all(&game).unwrap();
    fs::write(game.join("portable.txt"), "").unwrap();
    tauri::async_runtime::block_on(async {
        let list = gamebanana::list(&client, temp.path(), 21975).await.unwrap();
        println!("{} mods; first: {} by {}", list.len(), list[0].name, list[0].author);
        let mut refused = 0;
        for m in list.iter().take(40) {
            let Ok(file) = gamebanana::file_of(&client, m.id).await else { refused += 1; continue };
            if file.size > 5_000_000 {
                continue;
            }
            let archive = gamebanana::download(&client, &file, &temp.path().join("dl")).await.unwrap();
            match place(&archive, &game.join("mods"), "UnleashedRecomp", m.id, &m.name, &[]) {
                Ok(placed) => {
                    println!("{} ({}): {placed:?}", m.name, file.name);
                    println!("{}", fs::read_to_string(game.join("mods/ModsDB.ini")).unwrap());
                    remove(&game.join("mods"), "UnleashedRecomp", m.id, &placed).unwrap();
                    println!("after removing: {}", fs::read_to_string(game.join("mods/ModsDB.ini")).unwrap());
                    return;
                }
                Err(e) => println!("{}: {e}", m.name),
            }
        }
        panic!("no small mod found ({refused} refused)");
    });
}
