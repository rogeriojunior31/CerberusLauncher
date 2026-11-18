//! Numbers and dates the way people read them.

use wasm_bindgen::JsValue;

pub fn megabytes(bytes: u64) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb < 1.0 {
        return format!("{} KB", (bytes / 1024).max(1));
    }
    format!("{mb:.0} MB")
}

/// 401060 -> "401 mil", 7730 -> "7,7 mil", 527 -> "527".
pub fn count(n: u64) -> String {
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
        (0, 0) => "menos de 1 min".to_string(),
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// How long ago, in words: "agora há pouco", "há 3 h", "ontem", "há 5 dias", or the date.
pub fn ago(seconds: u64) -> String {
    let now = (js_sys::Date::now() / 1000.0) as u64;
    let elapsed = now.saturating_sub(seconds);
    match elapsed {
        0..3_600 => "agora há pouco".to_string(),
        3_600..86_400 => format!("há {} h", elapsed / 3_600),
        86_400..172_800 => "ontem".to_string(),
        172_800..2_592_000 => format!("há {} dias", elapsed / 86_400),
        _ => date_time(seconds).split(',').next().unwrap_or_default().to_string(),
    }
}

/// Unix seconds as a local date and time, e.g. "18/11/2026, 14:05".
pub fn date_time(seconds: u64) -> String {
    let date = js_sys::Date::new(&JsValue::from_f64(seconds as f64 * 1000.0));
    let options = js_sys::Object::new();
    let set = |key: &str, value: &str| js_sys::Reflect::set(&options, &key.into(), &value.into());
    let _ = set("dateStyle", "short");
    let _ = set("timeStyle", "short");
    date.to_locale_string("pt-BR", &options).into()
}
