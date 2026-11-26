//! Game pictures: the poster (cover) and the wide backdrop (hero).
//!
//! Every picture is a list of sources tried in order (the copy on disk, then the
//! one online); when one fails to load the next is used, and with none left a
//! placeholder with the game's name takes its place.

use cerberus_shared::{Game, RecompEntry};
use leptos::prelude::*;

use crate::api;

/// The backend sends art either online (`http...`) or as a path to its copy on
/// disk; the webview can only load the latter through the asset protocol.
pub fn src(value: &str) -> String {
    if value.starts_with("http") {
        return value.to_string();
    }
    api::convert_file_src(value)
}

fn sources(values: &[&Option<String>]) -> Vec<String> {
    values.iter().filter_map(|v| v.as_deref()).map(src).collect()
}

pub fn cover_sources(game: &Game) -> Vec<String> {
    let mut list = sources(&[&game.cover_path, &game.cover_url]);
    // Older and smaller Steam games have no tall cover, only the wide header:
    // drawn whole on its blurred copy, never the name on a blank card.
    if let cerberus_shared::Source::Steam { app_id } = &game.source {
        list.push(format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{app_id}/header.jpg"));
    }
    list
}

pub fn hero_sources(game: &Game) -> Vec<String> {
    sources(&[&game.hero_path, &game.hero_url])
}

pub fn recomp_covers(entry: &RecompEntry) -> Vec<String> {
    sources(&[&entry.cover, &entry.icon])
}

pub fn recomp_heroes(entry: &RecompEntry) -> Vec<String> {
    sources(&[&entry.hero])
}

/// Small screenshots stretched across a banner look blocky; blurring them turns
/// them into a soft backdrop instead.
pub fn is_low_res(url: &str) -> bool {
    url.contains("libretro-thumbnails") || url.contains("Named_Snaps")
}

/// A 2:3 poster. Covers that aren't 2:3 (an N64 box is wider than tall) sit whole
/// on a blurred copy of themselves instead of being cropped.
#[component]
pub fn Poster(sources: Vec<String>, title: String) -> impl IntoView {
    let attempt = RwSignal::new(0usize);
    let picture = move || match sources.get(attempt.get()).cloned() {
        Some(url) => {
            let on_error = move |_| attempt.update(|n| *n += 1);
            view! {
                <img class="blur-fill" src=url.clone() alt="" aria-hidden="true" loading="lazy" />
                <img class="art" src=url alt="" loading="lazy" on:error=on_error />
            }
            .into_any()
        }
        None => view! { <span class="ph">{title.clone()}</span> }.into_any(),
    };
    view! { <div class="poster">{picture}</div> }
}

/// The wide backdrop behind a banner or a game's page. `soft` for small
/// screenshots (the recomps' originals), which look blocky stretched this wide.
#[component]
pub fn Backdrop(sources: Vec<String>, #[prop(optional)] soft: bool) -> impl IntoView {
    let attempt = RwSignal::new(0usize);
    move || {
        let url = sources.get(attempt.get()).cloned()?;
        let class = if soft || is_low_res(&url) { "backdrop soft" } else { "backdrop" };
        Some(view! { <img class=class src=url alt="" on:error=move |_| attempt.update(|n| *n += 1) /> })
    }
}
