//! The player's collections of games and the games they hid from the library,
//! by id, in `collections.json`. Kept apart from the library (like favorites)
//! because Steam and Epic games are read from their stores each time.

use cerberus_shared::tr;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use cerberus_shared::Game;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shelf {
    #[serde(default)]
    pub hidden: BTreeSet<String>,
    /// In the order the player made them.
    #[serde(default)]
    pub collections: Vec<Collection>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    pub name: String,
    #[serde(default)]
    pub games: Vec<String>,
}

fn file(data: &Path) -> PathBuf {
    data.join("collections.json")
}

pub fn load(data: &Path) -> Shelf {
    crate::state_file::load_or_default(&file(data))
}

/// A change refused by `f` leaves the file as it was.
fn change(data: &Path, f: impl FnOnce(&mut Shelf) -> Result<(), String>) -> Result<(), String> {
    let path = file(data);
    let mut shelf: Shelf = match crate::state_file::read(&path) {
        crate::state_file::Read::Found(shelf) => shelf,
        crate::state_file::Read::Missing => Shelf::default(),
        crate::state_file::Read::Unreadable(_) => return crate::state_file::update(&path, |_: &mut Shelf| ()),
    };
    f(&mut shelf)?;
    crate::state_file::update(&path, |kept: &mut Shelf| *kept = shelf)
}

/// A name the player typed, tidied; empty or too long is refused.
fn name_of(typed: &str) -> Result<String, String> {
    let name: String = typed.split_whitespace().collect::<Vec<_>>().join(" ");
    match name.chars().count() {
        0 => Err(tr!("Dê um nome para a coleção.").into()),
        41.. => Err(tr!("Use um nome de até 40 letras.").into()),
        _ => Ok(name),
    }
}

fn find<'a>(shelf: &'a mut Shelf, name: &str) -> Option<&'a mut Collection> {
    shelf.collections.iter_mut().find(|c| c.name.eq_ignore_ascii_case(name))
}

pub fn set_hidden(data: &Path, id: &str, on: bool) -> Result<(), String> {
    change(data, |shelf| {
        if on {
            shelf.hidden.insert(id.to_string());
        } else {
            shelf.hidden.remove(id);
        }
        Ok(())
    })
}

pub fn create(data: &Path, typed: &str) -> Result<String, String> {
    let name = name_of(typed)?;
    change(data, |shelf| {
        if find(shelf, &name).is_some() {
            return Err(tr!("Já existe uma coleção chamada {}.", name));
        }
        shelf.collections.push(Collection { name: name.clone(), games: Vec::new() });
        Ok(())
    })?;
    Ok(name)
}

/// Puts the game in the collection (made when it doesn't exist) or takes it out.
pub fn set_in(data: &Path, collection: &str, id: &str, on: bool) -> Result<(), String> {
    let name = name_of(collection)?;
    change(data, |shelf| {
        if find(shelf, &name).is_none() {
            if !on {
                return Ok(());
            }
            shelf.collections.push(Collection { name: name.clone(), games: Vec::new() });
        }
        let c = find(shelf, &name).expect("just made");
        c.games.retain(|g| g != id);
        if on {
            c.games.push(id.to_string());
        }
        Ok(())
    })
}

pub fn rename(data: &Path, old: &str, typed: &str) -> Result<String, String> {
    let name = name_of(typed)?;
    change(data, |shelf| {
        if !name.eq_ignore_ascii_case(old) && find(shelf, &name).is_some() {
            return Err(tr!("Já existe uma coleção chamada {}.", name));
        }
        let c = find(shelf, old).ok_or(tr!("Essa coleção não existe mais."))?;
        c.name = name.clone();
        Ok(())
    })?;
    Ok(name)
}

/// The games stay in the library; only the collection goes.
pub fn delete(data: &Path, name: &str) -> Result<(), String> {
    change(data, |shelf| {
        shelf.collections.retain(|c| !c.name.eq_ignore_ascii_case(name));
        Ok(())
    })
}

pub fn names(data: &Path) -> Vec<String> {
    load(data).collections.into_iter().map(|c| c.name).collect()
}

/// Marks the games hidden and with their collections.
pub fn apply(data: &Path, games: &mut [Game]) {
    let shelf = load(data);
    for game in games {
        game.hidden = shelf.hidden.contains(&game.id);
        game.collections = shelf.collections.iter().filter(|c| c.games.contains(&game.id)).map(|c| c.name.clone()).collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collections_and_hidden_games_survive_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path();
        assert_eq!(create(data, "  Para  jogar ").unwrap(), "Para jogar");
        assert!(create(data, "para JOGAR").is_err(), "no two with one name");
        assert!(create(data, "   ").is_err());
        set_in(data, "Para jogar", "steam:570", true).unwrap();
        set_in(data, "Zerados", "recomp:zelda64recomp", true).unwrap();
        set_in(data, "Zerados", "steam:570", true).unwrap();
        set_in(data, "Zerados", "steam:570", true).unwrap();
        set_in(data, "Para jogar", "steam:570", false).unwrap();
        set_hidden(data, "epic:x", true).unwrap();
        assert_eq!(names(data), ["Para jogar", "Zerados"]);
        assert!(rename(data, "Zerados", "para jogar").is_err());
        assert_eq!(rename(data, "zerados", "Terminados").unwrap(), "Terminados");

        let game = |id: &str| serde_json::from_value::<Game>(serde_json::json!({
            "id": id, "name": id, "source": {"kind": "local", "exe": "x.exe"}, "install_dir": "", "cover_path": null, "cover_url": null
        })).unwrap();
        let mut games = vec![game("steam:570"), game("epic:x")];
        apply(data, &mut games);
        assert_eq!(games[0].collections, ["Terminados"]);
        assert!(!games[0].hidden);
        assert!(games[1].hidden);
        assert_eq!(load(data).collections[1].games, ["recomp:zelda64recomp", "steam:570"]);

        delete(data, "Para jogar").unwrap();
        assert_eq!(names(data), ["Terminados"]);
    }
}
