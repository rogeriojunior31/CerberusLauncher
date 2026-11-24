//! The interface's language. The choice is kept in the backend's settings and
//! mirrored in the browser storage, so the first frame already shows the right
//! language; "automatic" follows the system (Portuguese speakers get Portuguese,
//! everyone else English). Picking another one reloads the window.

use cerberus_shared::i18n::{Lang, lang, set_lang};

const KEY: &str = "cerberus-language";

fn storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}

/// The saved choice: "pt-BR", "en", or none for automatic.
fn saved() -> Option<String> {
    storage().and_then(|s| s.get_item(KEY).ok().flatten()).filter(|c| !c.is_empty() && c != "auto")
}

pub fn system() -> Lang {
    let code = web_sys::window().and_then(|w| w.navigator().language()).unwrap_or_default();
    Lang::from_code(&code)
}

fn resolve(choice: Option<&str>) -> Lang {
    choice.map(Lang::from_code).unwrap_or_else(system)
}

/// Before anything is drawn: the language to draw in.
/// The app is showing English.
pub fn is_english() -> bool {
    cerberus_shared::i18n::lang() == cerberus_shared::i18n::Lang::En
}

pub fn init() {
    let lang = resolve(saved().as_deref());
    set_lang(lang);
    if let Some(root) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.document_element()) {
        let _ = root.set_attribute("lang", lang.code());
    }
}

/// The settings arrived: if they chose another language than the one drawn
/// (changed on another run, or storage cleared), keep it and redraw.
pub fn follow(choice: Option<&str>) {
    let choice = choice.filter(|c| !c.is_empty());
    if let Some(s) = storage() {
        let _ = s.set_item(KEY, choice.unwrap_or("auto"));
    }
    if resolve(choice) != lang() {
        reload();
    }
}

/// The player picked a language (none: automatic): redraw everything in it.
pub fn choose(choice: Option<&str>) {
    if let Some(s) = storage() {
        let _ = s.set_item(KEY, choice.unwrap_or("auto"));
    }
    reload();
}

/// The saved choice, for the picker.
pub fn choice() -> Option<String> {
    saved()
}

fn reload() {
    if let Some(w) = web_sys::window() {
        let _ = w.location().reload();
    }
}
