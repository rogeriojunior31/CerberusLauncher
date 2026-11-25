//! The player's favorite games, by id, in `favorites.json`. Kept apart from the
//! library because Steam and Epic games are read from their stores each time,
//! and a favorite must survive that.

use std::collections::BTreeSet;
use std::path::Path;

fn file(data: &Path) -> std::path::PathBuf {
    data.join("favorites.json")
}

pub fn load(data: &Path) -> BTreeSet<String> {
    crate::state_file::load_or_default(&file(data))
}

pub fn set(data: &Path, id: &str, on: bool) -> Result<(), String> {
    crate::state_file::update(&file(data), |all: &mut BTreeSet<String>| {
        if on {
            all.insert(id.to_string());
        } else {
            all.remove(id);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favorites_are_kept_across_a_restart_and_removed_again() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load(dir.path()).is_empty());
        set(dir.path(), "steam:570", true).unwrap();
        set(dir.path(), "recomp:zelda64recomp", true).unwrap();
        set(dir.path(), "steam:570", false).unwrap();
        let all = load(dir.path());
        assert_eq!(all.into_iter().collect::<Vec<_>>(), ["recomp:zelda64recomp"]);
    }
}
