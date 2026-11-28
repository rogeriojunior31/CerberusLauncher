//! Numbers and dates the way people read them.

use cerberus_shared::i18n::{Lang, lang};
use cerberus_shared::tr;
use wasm_bindgen::JsValue;

/// A decimal number with the language's separator: "7,7" or "7.7".
pub fn decimal(text: String) -> String {
    if lang() == Lang::Pt { text.replace('.', ",") } else { text }
}

pub fn megabytes(bytes: u64) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb < 1.0 {
        return format!("{} KB", (bytes / 1024).max(1));
    }
    format!("{mb:.0} MB")
}

/// Disk space: "629,4 GB", or MB below a gigabyte.
pub fn disk_size(bytes: u64) -> String {
    let gb = bytes as f64 / 1_073_741_824.0;
    if gb < 1.0 {
        return megabytes(bytes);
    }
    decimal(format!("{gb:.1} GB"))
}

/// 401060 -> "401 mil" ("401K"), 7730 -> "7,7 mil" ("7.7K"), 527 -> "527".
pub fn count(n: u64) -> String {
    if lang() == Lang::En {
        return match n {
            0..1_000 => n.to_string(),
            1_000..10_000 => format!("{:.1}K", n as f64 / 1000.0),
            10_000..1_000_000 => format!("{}K", n / 1000),
            _ => format!("{:.1}M", n as f64 / 1_000_000.0),
        };
    }
    match n {
        0..1_000 => n.to_string(),
        1_000..10_000 => format!("{:.1} mil", n as f64 / 1000.0).replace('.', ","),
        10_000..1_000_000 => format!("{} mil", n / 1000),
        _ => format!("{:.1} mi", n as f64 / 1_000_000.0).replace('.', ","),
    }
}

pub fn session_duration(seconds: u64) -> String {
    match (seconds / 3600, seconds / 60 % 60, seconds % 60) {
        (0, 0, s) => format!("{s} s"),
        (0, m, s) => format!("{m} min {s} s"),
        (h, m, s) => format!("{h} h {m} min {s} s"),
    }
}

pub fn playtime(seconds: u64) -> String {
    let minutes = seconds / 60;
    match (minutes / 60, minutes % 60) {
        (0, 0) => tr!("menos de 1 min").to_string(),
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// Xbox and Epic mark an unlock whose date didn't come with 1 (0 elsewhere):
/// a real date is after that, never 31/12/1969.
pub fn known_date(seconds: u64) -> bool {
    seconds > 1
}

/// When an achievement was unlocked: "há 3 dias", or just "Desbloqueada"
/// when the store didn't say.
pub fn unlocked_when(seconds: u64) -> String {
    if known_date(seconds) { ago(seconds) } else { tr!("Desbloqueada").to_string() }
}

/// How long ago, in words: "agora há pouco", "há 3 h", "ontem", "há 5 dias", or the date.
pub fn ago(seconds: u64) -> String {
    let now = (js_sys::Date::now() / 1000.0) as u64;
    let elapsed = now.saturating_sub(seconds);
    match elapsed {
        0..3_600 => tr!("agora há pouco").to_string(),
        3_600..86_400 => tr!("há {} h", elapsed / 3_600),
        86_400..172_800 => tr!("ontem").to_string(),
        172_800..2_592_000 => tr!("há {} dias", elapsed / 86_400),
        _ => date_time(seconds).split(',').next().unwrap_or_default().to_string(),
    }
}

/// How long since an account synced, to the minute: "agora", "há 3 min", then as `ago`.
pub fn synced_ago(seconds: u64) -> String {
    let now = (js_sys::Date::now() / 1000.0) as u64;
    match now.saturating_sub(seconds) {
        0..60 => tr!("agora").to_string(),
        e @ 60..3_600 => tr!("há {} min", e / 60),
        _ => ago(seconds),
    }
}

/// Unix seconds as a local time of day, e.g. "14:05".
pub fn clock(seconds: u64) -> String {
    let date = js_sys::Date::new(&JsValue::from_f64(seconds as f64 * 1000.0));
    let options = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&options, &"timeStyle".into(), &"short".into());
    date.to_locale_time_string_with_options(if lang() == Lang::En { "en-US" } else { "pt-BR" }, &options).into()
}

/// Unix seconds as a local date and time, e.g. "18/11/2026, 14:05".
pub fn date_time(seconds: u64) -> String {
    let date = js_sys::Date::new(&JsValue::from_f64(seconds as f64 * 1000.0));
    let options = js_sys::Object::new();
    let set = |key: &str, value: &str| js_sys::Reflect::set(&options, &key.into(), &value.into());
    let _ = set("dateStyle", "short");
    let _ = set("timeStyle", "short");
    date.to_locale_string(if lang() == Lang::En { "en-US" } else { "pt-BR" }, &options).into()
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_missing_unlock_date_is_never_shown_as_1969() {
        assert!(!super::known_date(0) && !super::known_date(1));
        assert!(super::known_date(1_795_000_000));
    }
}
