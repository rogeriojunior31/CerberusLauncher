//! Recomps the player installed without Cerberus (by hand, with another
//! launcher), brought into the library where they are: no download, and their
//! folder stays theirs. Cerberus never updates or deletes an adopted folder.
//!
//! A program is matched to the catalog by the name the catalog gives the
//! recomp's program (`Spaghettify.exe`): that is a sure match. Without one, by
//! names that look alike (`Zelda64Recompiled.exe` and "Zelda 64: Recompiled"),
//! which is only a suggestion the player confirms.

use std::fs;
use std::path::{Path, PathBuf};

use cerberus_shared::{Game, Source};

use super::{CatalogEntry, MARKER, apply_art, installed_game, library_id};
use crate::library::LocalLibrary;

/// A program found in a folder and the recomp it is.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub recomp_id: String,
    pub program: PathBuf,
    /// The program has the exact name the catalog gives; otherwise a look-alike.
    pub sure: bool,
}

/// Lower-case letters and digits only: "Zelda 64: Recompiled" -> "zelda64recompiled".
fn plain(text: &str) -> String {
    text.chars().filter(char::is_ascii_alphanumeric).flat_map(|c| c.to_lowercase()).collect()
}

/// Program names too common to say which game they are.
const GENERIC: &[&str] = &["launcher", "game", "setup", "unins000", "uninstall", "start", "play", "main", "app", "updater", "crashhandler"];

fn file_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_lowercase()
}

/// The names an entry's program goes by, plainly written: the catalog's
/// program, the release file's name, the recomp's and the repository's names.
fn names_of(entry: &CatalogEntry) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(start) = &entry.start {
        names.push(plain(file_name(start).trim_end_matches(".exe")));
    }
    if let Some(pattern) = entry.assets.get("windows") {
        // `Zelda64Recompiled-*-Windows.zip` -> "zelda64recompiled".
        let head = pattern.split(['*', '-', '_', '.']).next().unwrap_or_default();
        names.push(plain(head));
    }
    names.push(plain(&entry.name));
    names.push(plain(entry.repo.rsplit('/').next().unwrap_or_default()));
    names.retain(|n| n.len() >= 5);
    names
}

/// The recomps a program could be, the surest first. A program with the exact
/// name the catalog gives one recomp is that recomp; a name shared by several,
/// or only alike, gives suggestions.
pub fn candidates(program: &Path, catalog: &[CatalogEntry]) -> Vec<Found> {
    let name = program.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    let stem = name.trim_end_matches(".exe");
    if stem.is_empty() || GENERIC.contains(&stem) {
        // "launcher.exe" says nothing on its own, but the catalog may still name it exactly.
        let exact: Vec<&CatalogEntry> = catalog.iter().filter(|e| e.start.as_deref().map(file_name).as_deref() == Some(name.as_str())).collect();
        return exact.iter().map(|e| Found { recomp_id: e.id.clone(), program: program.to_path_buf(), sure: false }).collect();
    }
    let exact: Vec<&CatalogEntry> = catalog.iter().filter(|e| e.start.as_deref().map(file_name).as_deref() == Some(name.as_str())).collect();
    if !exact.is_empty() {
        let sure = exact.len() == 1;
        return exact.into_iter().map(|e| Found { recomp_id: e.id.clone(), program: program.to_path_buf(), sure }).collect();
    }
    let wanted = plain(stem);
    if wanted.len() < 5 {
        return Vec::new();
    }
    let mut alike: Vec<&CatalogEntry> = catalog
        .iter()
        .filter(|e| names_of(e).iter().any(|n| *n == wanted || (n.len() >= 8 && wanted.len() >= 8 && (n.contains(&wanted) || wanted.contains(n.as_str())))))
        .collect();
    alike.sort_by_key(|e| std::cmp::Reverse(e.downloads));
    alike.truncate(5);
    alike.into_iter().map(|e| Found { recomp_id: e.id.clone(), program: program.to_path_buf(), sure: false }).collect()
}

/// Programs in a folder and the folders below it (three levels), skipping
/// Cerberus's own installs, which are in the library already.
fn programs_in(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if dir.join(MARKER).is_file() {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten().take(2000) {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() && depth > 0 {
            programs_in(&path, depth - 1, out);
        } else if kind.is_file() && path.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")) {
            out.push(path);
        }
    }
}

/// The recomps in a folder: one program per recomp, sure matches first. A
/// look-alike counts only when it points at a single recomp.
pub fn scan(folder: &Path, catalog: &[CatalogEntry]) -> Vec<Found> {
    let mut programs = Vec::new();
    programs_in(folder, 3, &mut programs);
    let mut found: Vec<Found> = Vec::new();
    for program in programs {
        let options = candidates(&program, catalog);
        let pick = match options.as_slice() {
            [one] => one.clone(),
            many => match many.iter().find(|f| f.sure) {
                Some(sure) => sure.clone(),
                None => continue,
            },
        };
        match found.iter_mut().find(|f| f.recomp_id == pick.recomp_id) {
            // A sure match wins over a look-alike for the same recomp.
            Some(have) if !have.sure && pick.sure => *have = pick,
            Some(_) => {}
            None => found.push(pick),
        }
    }
    found.sort_by_key(|f| !f.sure);
    found
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (dunce::canonicalize(a), dunce::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Puts a recomp the player installed into the library, linked to its catalog
/// entry. Bringing the same program twice keeps one game; a game added by hand
/// with that program becomes the recomp, keeping its play time.
pub fn adopt(library: &LocalLibrary, entry: &CatalogEntry, program: &Path) -> Result<Game, String> {
    if !program.is_file() {
        return Err(format!("{} não existe.", program.display()));
    }
    let program = dunce::canonicalize(program).map_err(|e| format!("{}: {e}", program.display()))?;
    let dir = program.parent().ok_or("O programa precisa estar numa pasta.")?.to_path_buf();
    let exe = program.to_string_lossy().into_owned();
    let install_dir = dir.to_string_lossy().into_owned();

    if let Some(existing) = installed_game(library, entry)? {
        let Source::Recomp { exe: current, .. } = &existing.source else {
            return Err("O registro do recomp está diferente do esperado.".into());
        };
        if same_file(Path::new(current), &program) {
            return Ok(existing);
        }
        if !existing.external {
            return Err(format!(
                "{} já está instalado pelo Cerberus em {}. Desinstale esse antes de trazer outra pasta.",
                existing.name, existing.install_dir
            ));
        }
        // Moved or reinstalled elsewhere: the link follows it.
        return library.update(&existing.id, |game| {
            if let Source::Recomp { exe: current, .. } = &mut game.source {
                *current = exe.clone();
            }
            game.install_dir = install_dir.clone();
        });
    }

    let source = Source::Recomp { recomp_id: entry.id.clone(), version: String::new(), exe: exe.clone(), generation: 0 };
    let added_by_hand = library.games()?.into_iter().find(|g| matches!(&g.source, Source::Local { exe: e } if same_file(Path::new(e), &program)));
    if let Some(local) = added_by_hand {
        return library.update(&local.id, |game| {
            game.source = source.clone();
            game.install_dir = install_dir.clone();
            game.external = true;
        });
    }

    let mut game = crate::library::game_from_exe(&program);
    game.id = library_id(&entry.id);
    game.name = entry.name.clone();
    game.source = source;
    game.install_dir = install_dir;
    game.external = true;
    apply_art(&mut game);
    library.add(game.clone())?;
    Ok(game)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, name: &str, start: Option<&str>) -> CatalogEntry {
        CatalogEntry { id: id.into(), name: name.into(), start: start.map(str::to_string), repo: format!("dev/{id}"), ..Default::default() }
    }

    fn catalog() -> Vec<CatalogEntry> {
        vec![
            entry("spaghettikart", "SpaghettiKart", Some("Spaghettify.exe")),
            entry("zelda64recomp", "Zelda 64: Recompiled", None),
            entry("a-launcher", "Game A", Some("launcher.exe")),
            entry("b-launcher", "Game B", Some("launcher.exe")),
        ]
    }

    #[test]
    fn exact_names_are_sure_and_look_alikes_are_suggestions() {
        let c = catalog();
        let sure = candidates(Path::new("C:/Games/SK/Spaghettify.exe"), &c);
        assert_eq!((sure[0].recomp_id.as_str(), sure[0].sure), ("spaghettikart", true));
        let alike = candidates(Path::new("C:/Games/Zelda/Zelda64Recompiled.exe"), &c);
        assert_eq!((alike[0].recomp_id.as_str(), alike[0].sure), ("zelda64recomp", false));
        // A name two recomps share is only ever a suggestion, and a generic one alone says nothing.
        assert!(candidates(Path::new("C:/x/launcher.exe"), &c).iter().all(|f| !f.sure));
        assert!(candidates(Path::new("C:/x/game.exe"), &c).is_empty());
    }

    #[test]
    fn scanning_finds_each_recomp_once_and_skips_cerberus_installs() {
        let dir = tempfile::tempdir().unwrap();
        let sk = dir.path().join("Games/SpaghettiKart");
        fs::create_dir_all(&sk).unwrap();
        fs::write(sk.join("Spaghettify.exe"), b"MZ").unwrap();
        fs::write(sk.join("unins000.exe"), b"MZ").unwrap();
        let ours = dir.path().join("Cerberus/zelda64recomp");
        fs::create_dir_all(&ours).unwrap();
        fs::write(ours.join(MARKER), "zelda64recomp").unwrap();
        fs::write(ours.join("Zelda64Recompiled.exe"), b"MZ").unwrap();
        let found = scan(dir.path(), &catalog());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].recomp_id, "spaghettikart");
        assert!(found[0].sure);
    }

    #[test]
    fn adopting_twice_keeps_one_game_and_a_hand_added_one_becomes_the_recomp() {
        let dir = tempfile::tempdir().unwrap();
        let library = LocalLibrary::load(&dir.path().join("data"));
        let sk = dir.path().join("SpaghettiKart");
        fs::create_dir_all(&sk).unwrap();
        let program = sk.join("Spaghettify.exe");
        fs::write(&program, b"MZ").unwrap();
        let c = catalog();

        // Added by hand first, with play time: it becomes the recomp and keeps the time.
        let mut by_hand = crate::library::game_from_exe(&program);
        by_hand.playtime_secs = 600;
        library.add(by_hand.clone()).unwrap();
        let first = adopt(&library, &c[0], &program).unwrap();
        assert_eq!(first.id, by_hand.id);
        assert!(first.external);
        assert_eq!(first.playtime_secs, 600);
        assert!(matches!(&first.source, Source::Recomp { recomp_id, .. } if recomp_id == "spaghettikart"));

        let again = adopt(&library, &c[0], &program).unwrap();
        assert_eq!(again.id, first.id);
        assert_eq!(library.games().unwrap().len(), 1);
    }

    #[test]
    fn a_cerberus_install_is_never_replaced_by_an_outside_folder() {
        let dir = tempfile::tempdir().unwrap();
        let library = LocalLibrary::load(&dir.path().join("data"));
        let ours = dir.path().join("ours");
        fs::create_dir_all(&ours).unwrap();
        fs::write(ours.join("Spaghettify.exe"), b"MZ").unwrap();
        let mut installed = crate::library::game_from_exe(&ours.join("Spaghettify.exe"));
        installed.id = library_id("spaghettikart");
        installed.source = Source::Recomp { recomp_id: "spaghettikart".into(), version: "1.0".into(), exe: ours.join("Spaghettify.exe").to_string_lossy().into(), generation: 0 };
        library.add(installed).unwrap();

        let theirs = dir.path().join("theirs");
        fs::create_dir_all(&theirs).unwrap();
        fs::write(theirs.join("Spaghettify.exe"), b"MZ").unwrap();
        let error = adopt(&library, &catalog()[0], &theirs.join("Spaghettify.exe")).unwrap_err();
        assert!(error.contains("já está instalado pelo Cerberus"));
    }
}

#[cfg(test)]
#[test]
#[ignore = "reads the downloaded catalog on this PC and a folder given in CERBERUS_SCAN"]
fn scan_real_folder() {
    let cache = std::path::PathBuf::from(std::env::var("APPDATA").unwrap()).join("app.cerberus.launcher/cache");
    super::catalog::load_copy(&cache);
    let folder = std::path::PathBuf::from(std::env::var("CERBERUS_SCAN").unwrap());
    let all = super::catalog::all();
    println!("catálogo: {} entradas", all.len());
    for f in scan(&folder, &all) {
        println!("{} sure={} {}", f.recomp_id, f.sure, f.program.display());
    }
}
