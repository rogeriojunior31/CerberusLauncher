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
