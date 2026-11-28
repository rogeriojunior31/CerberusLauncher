//! One key for a game's name, the same across stores: "DARK SOULS™ III",
//! "Dark Souls 3" and "Dark Souls III (PC)" are one game; "Forza Horizon 5
//! Standard Edition" is "Forza Horizon 5". A year in parentheses stays, so
//! "DOOM (2016)" is not "DOOM".

/// Edition words that come before "Edition" at the end of a name and don't
/// make it another game.
const EDITIONS: &[&str] = &["standard", "deluxe", "ultimate", "gold", "goty", "definitive", "complete", "digital", "premium"];

/// Famous short names, as the key of the full one.
const ALIASES: &[(&str, &str)] = &[("halomcc", "halothemasterchiefcollection")];

/// The key: lowercase letters and digits only, with marks, platform words and
/// edition words at the end left out, and Roman numerals (II to XX) as digits.
pub fn key(name: &str) -> String {
    let lower = name.to_lowercase();
    // Parentheses and brackets about the platform go; others (a year) stay as words.
    let mut text = String::with_capacity(lower.len());
    let mut rest = lower.as_str();
    while let Some(start) = rest.find(['(', '[']) {
        text.push_str(&rest[..start]);
        let close = if rest.as_bytes()[start] == b'(' { ')' } else { ']' };
        let Some(end) = rest[start..].find(close).map(|e| start + e) else {
            rest = &rest[start + 1..];
            continue;
        };
        let inside = rest[start + 1..end].trim();
        if !matches!(inside, "pc" | "windows" | "windows 10" | "windows pc" | "for windows") {
            text.push(' ');
            text.push_str(inside);
            text.push(' ');
        }
        rest = &rest[end + 1..];
    }
    text.push_str(rest);
    let mut words: Vec<&str> = text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    // From the end: "… Standard Edition", "… Game of the Year Edition", "… for Windows", "… Windows 10 Edition", "… PC".
    loop {
        let n = words.len();
        let ends = |tail: &[&str]| n > tail.len() && words[n - tail.len()..] == *tail;
        let cut = if ends(&["game", "of", "the", "year", "edition"]) {
            5
        } else if ends(&["windows", "10", "edition"]) || ends(&["for", "windows", "10"]) {
            3
        } else if n > 2 && words[n - 1] == "edition" && (EDITIONS.contains(&words[n - 2]) || words[n - 2] == "windows" || words[n - 2] == "pc") {
            2
        } else if ends(&["for", "windows"]) || ends(&["windows", "10"]) {
            2
        } else if n > 1 && (words[n - 1] == "goty" || words[n - 1] == "pc") {
            1
        } else {
            0
        };
        if cut == 0 {
            break;
        }
        words.truncate(n - cut);
    }
    let joined: String = words.iter().map(|w| roman(w).map_or_else(|| (*w).to_string(), |n| n.to_string())).collect();
    ALIASES.iter().find(|(short, _)| *short == joined).map_or(joined, |(_, full)| (*full).to_string())
}

/// II to XX as a number. "I" and "X" alone stay words ("I Am Bread", "Halo X").
fn roman(word: &str) -> Option<u32> {
    const NUMERALS: [(&str, u32); 18] = [
        ("ii", 2), ("iii", 3), ("iv", 4), ("v", 5), ("vi", 6), ("vii", 7), ("viii", 8), ("ix", 9), ("xi", 11),
        ("xii", 12), ("xiii", 13), ("xiv", 14), ("xv", 15), ("xvi", 16), ("xvii", 17), ("xviii", 18), ("xix", 19), ("xx", 20),
    ];
    NUMERALS.iter().find(|(n, _)| *n == word).map(|(_, v)| *v)
}

#[cfg(test)]
mod tests {
    use super::key;

    #[test]
    fn the_same_game_has_one_key_across_stores() {
        assert_eq!(key("DARK SOULS™ III"), key("Dark Souls III"));
        assert_eq!(key("Dark Souls 3"), key("DARK SOULS™ III"));
        assert_eq!(key("Forza Horizon 5 Standard Edition"), key("Forza Horizon 5"));
        assert_eq!(key("DOOM Eternal (PC)"), key("DOOM Eternal"));
        assert_eq!(key("Minecraft for Windows"), key("Minecraft"));
        assert_eq!(key("Minecraft: Windows 10 Edition"), key("Minecraft"));
        assert_eq!(key("Fallout 4: Game of the Year Edition"), key("Fallout 4"));
        assert_eq!(key("Halo MCC"), key("Halo: The Master Chief Collection"));
        assert_eq!(key("Grand Theft Auto V"), key("Grand Theft Auto 5"));
        assert_eq!(key("Final Fantasy XIV"), key("Final Fantasy 14"));
    }

    #[test]
    fn different_games_keep_different_keys() {
        assert_ne!(key("Hotline Miami"), key("Hotline Miami 2"));
        assert_ne!(key("DOOM (2016)"), key("DOOM"), "a year in parentheses tells them apart");
        assert_ne!(key("I Am Bread"), key("1 Am Bread"), "a lone I stays a word");
        assert_ne!(key("Gold Edition"), key(""), "a name that is only edition words keeps them");
        assert_ne!(key("Baldur's Gate: Enhanced Edition"), key("Baldur's Gate"), "an enhanced edition is not listed: left apart");
        assert_eq!(key("Mass Effect Legendary Edition"), "masseffectlegendaryedition");
    }
}

/// The same game owned in several stores: groups of library ids, the copy to
/// show first leading (installed, then the one played last, then Steam, Xbox,
/// Epic). Games alone, hidden, or of one store twice are left out.
pub fn copies(games: &[crate::Game]) -> Vec<Vec<String>> {
    use crate::{NextStep, Source};
    let store = |g: &crate::Game| match g.source {
        Source::Steam { .. } => Some(0),
        Source::Xbox { .. } => Some(1),
        Source::Epic { .. } => Some(2),
        _ => None,
    };
    let mut by_key: std::collections::HashMap<String, Vec<&crate::Game>> = std::collections::HashMap::new();
    for g in games.iter().filter(|g| !g.hidden && g.part_of.is_none() && store(g).is_some()) {
        let k = key(&g.name);
        if !k.is_empty() {
            by_key.entry(k).or_default().push(g);
        }
    }
    let mut groups: Vec<Vec<String>> = by_key
        .into_values()
        .filter_map(|mut list| {
            list.sort_by_key(|g| (g.next_step == NextStep::Install, std::cmp::Reverse(g.last_played.unwrap_or(0)), store(g)));
            // One copy per store: two of the same store are two games.
            let mut seen = std::collections::HashSet::new();
            list.retain(|g| seen.insert(store(g)));
            (list.len() > 1).then(|| list.into_iter().map(|g| g.id.clone()).collect())
        })
        .collect();
    groups.sort();
    groups
}

#[cfg(test)]
mod copies_tests {
    use super::copies;
    use crate::{Game, NextStep, Source};

    fn game(id: &str, name: &str, source: Source, installed: bool, played: Option<u64>) -> Game {
        let mut g: Game = serde_json::from_value(serde_json::json!({"id": id, "name": name, "install_dir": "", "source": {"kind": "local", "exe": ""}})).unwrap();
        g.source = source;
        g.next_step = if installed { NextStep::Play } else { NextStep::Install };
        g.last_played = played;
        g
    }

    #[test]
    fn one_game_in_two_stores_leads_with_the_installed_copy() {
        let xbox = Source::Xbox { family: "f".into(), app: "Game".into(), product: "p".into(), game_pass: false };
        let games = vec![
            game("steam:1", "DOOM Eternal", Source::Steam { app_id: 1 }, false, Some(900)),
            game("xbox:f", "DOOM Eternal (PC)", xbox.clone(), true, None),
            game("steam:2", "Hotline Miami", Source::Steam { app_id: 2 }, true, None),
            game("local:x", "Hotline Miami", Source::Local { exe: "x.exe".into() }, true, None),
            game("steam:3", "Halo", Source::Steam { app_id: 3 }, true, None),
            game("steam:4", "Halo", Source::Steam { app_id: 4 }, true, None),
        ];
        assert_eq!(copies(&games), vec![vec!["xbox:f".to_string(), "steam:1".to_string()]], "installed first; a game added by hand or a second Steam copy is not grouped");
    }

    #[test]
    fn neither_installed_the_one_played_last_leads() {
        let epic = Source::Epic { app_name: "a".into(), namespace: "n".into(), item_id: "i".into() };
        let games = vec![
            game("epic:a", "Control", epic, false, Some(500)),
            game("steam:9", "Control", Source::Steam { app_id: 9 }, false, Some(100)),
        ];
        assert_eq!(copies(&games), vec![vec!["epic:a".to_string(), "steam:9".to_string()]]);
    }
}
