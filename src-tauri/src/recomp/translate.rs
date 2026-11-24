//! Catalog text in Portuguese, where it can be done exactly.
//!
//! The site's catalog is in English. Its tags are a short fixed list, and almost
//! every "what you need" line follows one pattern ("Your own copy of {game}"),
//! so those are translated here word for word. Free text (descriptions, setup
//! steps, feature lists) stays as the developer wrote it: a guess at a sentence
//! would be worse than the original.

/// Tags with a usual Portuguese name; names, numbers and brands stay as they are.
const TAGS: &[(&str, &str)] = &[
    ("controller support", "Suporte a controle"),
    ("controller", "Controle"),
    ("mods", "Mods"),
    ("mod support", "Suporte a mods"),
    ("high fps", "FPS alto"),
    ("keyboard & mouse", "Teclado e mouse"),
    ("keyboard and mouse", "Teclado e mouse"),
    ("mouse and keyboard", "Teclado e mouse"),
    ("keyboard", "Teclado"),
    ("online", "Online"),
    ("upscaling", "Upscaling"),
    ("hd textures", "Texturas HD"),
    ("multiplayer", "Multijogador"),
    ("achievements", "Conquistas"),
    ("save states", "Save states"),
    ("ultrawide", "Ultrawide"),
    ("experimental ultrawide", "Ultrawide experimental"),
    ("split screen", "Tela dividida"),
    ("local co-op", "Cooperativo local"),
    ("co-op", "Cooperativo"),
    ("netplay", "Jogo em rede"),
    ("fan translation", "Tradução de fãs"),
    ("gyro", "Giroscópio"),
    ("ray tracing", "Ray tracing"),
    ("high resolution", "Alta resolução"),
    ("graphics", "Gráficos"),
    ("cheat menu", "Menu de trapaças"),
    ("cheats", "Trapaças"),
    ("randomizer", "Randomizer"),
    ("3d conversion", "Conversão 3D"),
    ("fight", "Luta"),
    ("mouse look", "Câmera com mouse"),
    ("snowboarding", "Snowboard"),
    ("3rdperson", "Terceira pessoa"),
    ("action", "Ação"),
    ("adventure", "Aventura"),
    ("racing", "Corrida"),
    ("rpg", "RPG"),
    ("sci-fi", "Ficção científica"),
    ("remaster", "Remaster"),
    ("open source", "Código aberto"),
    ("bring your own game", "Traga seu jogo"),
    ("dos classic", "Clássico de DOS"),
    ("no edition exclusives", "Sem exclusivos de edição"),
    ("4k shadows", "Sombras em 4K"),
    ("dlss support", "Suporte a DLSS"),
    ("adaptive aspect-ratio", "Proporção adaptável"),
    ("native", "Nativo"),
];

pub fn tag(text: &str) -> String {
    let key = text.trim().to_lowercase();
    match TAGS.iter().find(|(english, _)| *english == key) {
        Some((_, portuguese)) => portuguese.to_string(),
        None => text.to_string(),
    }
}

const NEEDS: &[(&str, &str)] = &[
    ("Your own copy of ", "Sua cópia de "),
    ("Your own dump of ", "Seu dump de "),
    ("Your own disc image of ", "Sua imagem de disco de "),
    ("Your own USA disc image of ", "Sua imagem de disco americana de "),
    ("Your own USA copy of ", "Sua cópia americana de "),
];

/// "Your own copy of Mario Kart 64" -> "Sua cópia de Mario Kart 64".
pub fn needs(text: &str) -> String {
    for (english, portuguese) in NEEDS {
        if let Some(rest) = text.strip_prefix(english) {
            return format!("{portuguese}{rest}");
        }
    }
    text.to_string()
}

/// The catalog is kept in Portuguese (the curated list is written in it, the
/// site's is translated on load); in English, tags and "what you need" go back
/// to English where the translation is known, and everything else stays as is.
pub fn shown_tag(text: &str) -> String {
    if cerberus_shared::i18n::lang() == cerberus_shared::i18n::Lang::Pt {
        return text.to_string();
    }
    match TAGS.iter().find(|(_, portuguese)| *portuguese == text) {
        Some((english, _)) => capitalize(english),
        None => text.to_string(),
    }
}

pub fn shown_needs(text: &str) -> String {
    if cerberus_shared::i18n::lang() == cerberus_shared::i18n::Lang::Pt {
        return text.to_string();
    }
    for (english, portuguese) in NEEDS {
        if let Some(rest) = text.strip_prefix(portuguese) {
            return format!("{english}{rest}");
        }
    }
    text.to_string()
}

/// "controller support" -> "Controller support" (the list keeps English in lowercase).
fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_players_see_the_catalog_in_english() {
        use cerberus_shared::i18n::{Lang, with_lang};
        let shown = with_lang(Lang::En, || (shown_tag("Suporte a controle"), shown_tag("Guitar Hero"), shown_needs("Sua cópia de Mario Kart 64")));
        assert_eq!(shown.0, "Controller support");
        assert_eq!(shown.1, "Guitar Hero");
        assert_eq!(shown.2, "Your own copy of Mario Kart 64");
        assert_eq!(shown_tag("Suporte a controle"), "Suporte a controle");
    }

    #[test]
    fn translates_known_tags_and_keeps_names() {
        assert_eq!(tag("Controller Support"), "Suporte a controle");
        assert_eq!(tag("mods"), "Mods");
        assert_eq!(tag("Guitar Hero"), "Guitar Hero");
        assert_eq!(tag("60 FPS"), "60 FPS");
    }

    #[test]
    fn translates_the_usual_needs_line() {
        assert_eq!(needs("Your own copy of The Legend of Zelda: Majora's Mask"), "Sua cópia de The Legend of Zelda: Majora's Mask");
        assert_eq!(needs("Your own USA disc image of Skate 3"), "Sua imagem de disco americana de Skate 3");
        assert_eq!(needs("A PC and a controller"), "A PC and a controller");
    }
}
