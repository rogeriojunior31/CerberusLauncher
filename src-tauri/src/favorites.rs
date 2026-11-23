//! The player's favorite games, by id, in `favorites.json`. Kept apart from the
//! library because Steam and Epic games are read from their stores each time,
//! and a favorite must survive that.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

fn file(data: &Path) -> std::path::PathBuf {
    data.join("favorites.json")
}

pub fn load(data: &Path) -> BTreeSet<String> {
    fs::read_to_string(file(data)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn set(data: &Path, id: &str, on: bool) -> Result<(), String> {
    let mut all = load(data);
    if on {
        all.insert(id.to_string());
    } else {
        all.remove(id);
    }
    let text = serde_json::to_string_pretty(&all).map_err(|e| e.to_string())?;
    let target = file(data);
    let temp = target.with_extension("json.tmp");
    fs::write(&temp, text).map_err(|e| e.to_string())?;
    fs::rename(&temp, &target).map_err(|e| e.to_string())
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
