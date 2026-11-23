//! Which language the backend's messages (errors, notices, the tray menu) use:
//! the one picked in the settings, or Windows' own.

use cerberus_shared::i18n::{Lang, set_lang};

/// Sets the language for a saved choice; none follows Windows.
pub fn apply(choice: Option<&str>) {
    let lang = match choice {
        Some(code) if !code.is_empty() => Lang::from_code(code),
        _ => system(),
    };
    set_lang(lang);
}

/// Windows' display language: Portuguese for Portuguese, English otherwise.
#[cfg(windows)]
fn system() -> Lang {
    use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
    const LANG_PORTUGUESE: u16 = 0x16;
    // SAFETY: a plain query with no arguments.
    let id = unsafe { GetUserDefaultUILanguage() };
    if id & 0x3ff == LANG_PORTUGUESE { Lang::Pt } else { Lang::En }
}

#[cfg(not(windows))]
fn system() -> Lang {
    let code = std::env::var("LANG").unwrap_or_default();
    Lang::from_code(&code)
}
