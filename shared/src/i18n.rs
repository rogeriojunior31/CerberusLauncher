//! The app's languages: Brazilian Portuguese, the language the texts are
//! written in, and English, kept in `shared/locales/en.json`.
//!
//! A text is its own key: `tr!("Biblioteca")` gives "Library" in English and
//! the text itself in Portuguese, so the code reads as the app does. Texts with
//! parts that change take `{}` in order, or `{0}`, `{1}`... where a language
//! needs them in another order: `tr!("{} jogos", n)`.
//!
//! A test checks every `tr!` text in the code has its English entry.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    Pt,
    En,
}

impl Lang {
    /// "pt-BR" or "en", as kept in the settings.
    pub fn code(self) -> &'static str {
        match self {
            Lang::Pt => "pt-BR",
            Lang::En => "en",
        }
    }

    /// The language for a code: a saved choice, or the system's ("pt-PT", "en-US"...).
    /// Portuguese speakers get Portuguese; everyone else, English.
    pub fn from_code(code: &str) -> Lang {
        if code.trim().to_ascii_lowercase().starts_with("pt") { Lang::Pt } else { Lang::En }
    }

    /// The name shown in the language picker, in that language itself.
    pub fn name(self) -> &'static str {
        match self {
            Lang::Pt => "Português (Brasil)",
            Lang::En => "English",
        }
    }

    pub const ALL: [Lang; 2] = [Lang::Pt, Lang::En];
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set_lang(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

thread_local! {
    static ONLY_HERE: std::cell::Cell<Option<Lang>> = const { std::cell::Cell::new(None) };
}

pub fn lang() -> Lang {
    if let Some(lang) = ONLY_HERE.with(|l| l.get()) {
        return lang;
    }
    if CURRENT.load(Ordering::Relaxed) == Lang::En as u8 { Lang::En } else { Lang::Pt }
}

/// Runs `f` in another language on this thread only (for tests, which run side by side).
pub fn with_lang<T>(lang: Lang, f: impl FnOnce() -> T) -> T {
    let before = ONLY_HERE.with(|l| l.replace(Some(lang)));
    let out = f();
    ONLY_HERE.with(|l| l.set(before));
    out
}

const EN_JSON: &str = include_str!("../locales/en.json");

fn english() -> &'static HashMap<String, String> {
    static MAP: OnceLock<HashMap<String, String>> = OnceLock::new();
    MAP.get_or_init(|| serde_json::from_str(EN_JSON).unwrap_or_default())
}

/// A text in the current language; Portuguese when there's no translation yet.
pub fn t(text: &'static str) -> &'static str {
    if lang() == Lang::Pt {
        return text;
    }
    english().get(text).map(String::as_str).filter(|s| !s.is_empty()).unwrap_or(text)
}

/// A text that comes from data (a history step kept in Portuguese), in the
/// current language when it has a translation; as it is otherwise.
pub fn translate(text: &str) -> String {
    if lang() == Lang::Pt {
        return text.to_string();
    }
    english().get(text).filter(|s| !s.is_empty()).cloned().unwrap_or_else(|| text.to_string())
}

/// A text with parts filled in: `{}` takes the next part, `{0}`, `{1}` a given one.
pub fn tf(text: &'static str, parts: &[&dyn Display]) -> String {
    fill(t(text), parts)
}

fn fill(template: &str, parts: &[&dyn Display]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut next = 0;
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open].replace("}}", "}"));
        let after = &rest[open + 1..];
        // "{{" is a literal brace.
        if let Some(stripped) = after.strip_prefix('{') {
            out.push('{');
            rest = stripped;
            continue;
        }
        let Some(close) = after.find('}') else {
            out.push_str(&rest[open..]);
            return out;
        };
        let inside = &after[..close];
        let index = if inside.is_empty() {
            next += 1;
            Some(next - 1)
        } else {
            inside.parse::<usize>().ok()
        };
        match index.and_then(|i| parts.get(i)) {
            Some(part) => out.push_str(&part.to_string()),
            None => {
                out.push('{');
                out.push_str(inside);
                out.push('}');
            }
        }
        rest = &after[close + 1..];
    }
    out.push_str(&rest.replace("}}", "}"));
    out
}

/// `tr!("Texto")` is the text in the current language (a `&'static str`);
/// `tr!("{} jogos", n)` fills in the parts (a `String`).
#[macro_export]
macro_rules! tr {
    ($text:literal) => {
        $crate::i18n::t($text)
    };
    ($text:literal, $($part:expr),+ $(,)?) => {
        $crate::i18n::tf($text, &[$(&$part as &dyn ::std::fmt::Display),+])
    };
}

/// `tk!("Texto")` marks a text to translate where it can't be translated yet:
/// a `const` table. It stays as written; `t(text)` translates it where it's shown.
#[macro_export]
macro_rules! tk {
    ($text:literal) => {
        $text
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    #[test]
    fn parts_are_filled_in_order_or_by_number() {
        assert_eq!(fill("{} de {}", &[&3, &"10"]), "3 de 10");
        assert_eq!(fill("{1} of {0}", &[&"A", &"B"]), "B of A");
        assert_eq!(fill("{{literal}} {}", &[&1]), "{literal} 1");
        assert_eq!(fill("sem {}", &[]), "sem {}");
    }

    #[test]
    fn codes_pick_the_language() {
        assert_eq!(Lang::from_code("pt-BR"), Lang::Pt);
        assert_eq!(Lang::from_code("pt-PT"), Lang::Pt);
        assert_eq!(Lang::from_code("en-US"), Lang::En);
        assert_eq!(Lang::from_code("es"), Lang::En);
    }

    /// Every `tr!("...")` text in the app, as the program sees it.
    fn texts_in_code() -> Vec<(String, String)> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
        let mut files = Vec::new();
        for dir in ["ui/src", "src-tauri/src", "shared/src"] {
            collect(&root.join(dir), &mut files);
        }
        let mut found = Vec::new();
        for file in files {
            let code = std::fs::read_to_string(&file).unwrap();
            let mut rest = code.as_str();
            while let Some(at) = rest.find("tr!(\"").into_iter().chain(rest.find("tk!(\"")).min() {
                let body = &rest[at + 5..];
                // Not the end of another macro's name, like include_str!("...").
                let own = rest[..at].chars().next_back().is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
                if !own {
                    rest = body;
                    continue;
                }
                let (text, used) = rust_string(body);
                found.push((text, file.display().to_string()));
                rest = &body[used..];
            }
        }
        found
    }

    fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") && !path.ends_with("i18n.rs") {
                out.push(path);
            }
        }
    }

    /// A Rust string literal's value (after the opening quote) and how many bytes it took.
    fn rust_string(body: &str) -> (String, usize) {
        let mut value = String::new();
        let mut chars = body.char_indices();
        while let Some((i, c)) = chars.next() {
            match c {
                '"' => return (value, i + 1),
                '\\' => match chars.next() {
                    Some((_, 'n')) => value.push('\n'),
                    Some((_, 't')) => value.push('\t'),
                    Some((_, '\n')) => {
                        // A line continuation skips the next line's indentation.
                        let skip = body[i + 2..].len() - body[i + 2..].trim_start().len();
                        for _ in 0..skip {
                            chars.next();
                        }
                    }
                    Some((_, other)) => value.push(other),
                    None => {}
                },
                _ => value.push(c),
            }
        }
        (value, body.len())
    }

    #[test]
    fn every_text_has_its_english() {
        let english = english();
        assert!(!english.is_empty() || EN_JSON.trim() == "{}", "en.json não é um JSON válido");
        let mut missing: Vec<String> = texts_in_code()
            .into_iter()
            .filter(|(text, _)| english.get(text).is_none_or(|en| en.is_empty()))
            .map(|(text, file)| format!("{text:?}  ({file})"))
            .collect();
        missing.sort();
        missing.dedup();
        assert!(missing.is_empty(), "{} textos sem inglês em shared/locales/en.json:\n{}", missing.len(), missing.join("\n"));
    }

    #[test]
    fn english_has_no_texts_the_code_dropped() {
        let used: std::collections::HashSet<String> = texts_in_code().into_iter().map(|(t, _)| t).collect();
        let mut stale: Vec<&String> = english().keys().filter(|k| !used.contains(*k)).collect();
        stale.sort();
        assert!(stale.is_empty(), "{} textos em en.json que o código não usa mais:\n{:?}", stale.len(), stale);
    }

    #[test]
    fn translations_keep_the_parts() {
        let count = |s: &str| s.matches('{').count() - s.matches("{{").count() * 2;
        let wrong: Vec<&String> = english().iter().filter(|(pt, en)| count(pt) != count(en)).map(|(pt, _)| pt).collect();
        assert!(wrong.is_empty(), "traduções com partes ({{}}) diferentes do original: {wrong:?}");
    }
}
