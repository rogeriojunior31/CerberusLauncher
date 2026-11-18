//! Home: a rotating banner, then shelves of what to play next.

use std::time::Duration;

use cerberus_shared::{Game, RecompEntry, Source};
use leptos::prelude::*;

use crate::art::{Backdrop, Poster, cover_sources, hero_sources, is_low_res, recomp_covers, recomp_heroes};
use crate::icons::{Icon, Svg};
use crate::tiles::{Shelf, TileInfo, game_tile, recomp_tile, source_name};
use crate::versions::one_each;
use crate::{Library, Route, play_game};

/// One banner slide.
#[derive(Clone)]
struct Feature {
    kicker: String,
    title: String,
    based: String,
    about: String,
    heroes: Vec<String>,
    /// The backdrop is a small screenshot, shown blurred.
    soft: bool,
    covers: Vec<String>,
    open: Route,
    /// The game to start from the banner, when it is in the library.
    game: Option<Game>,
}

fn game_feature(game: &Game) -> Feature {
    Feature {
        kicker: "Continue jogando".to_string(),
        title: game.name.clone(),
        based: source_name(&game.source).to_string(),
        about: String::new(),
        heroes: hero_sources(game),
        soft: game.hero_url.as_deref().is_some_and(is_low_res) && matches!(game.source, Source::Recomp { .. }),
        covers: cover_sources(game),
        open: Route::Game(game.id.clone()),
        game: Some(game.clone()),
    }
}

fn recomp_feature(entry: &RecompEntry) -> Feature {
    Feature {
        kicker: format!("Recomp · {}", entry.console),
        title: entry.name.clone(),
        based: format!("Baseado em {}", entry.game),
        about: entry.about.clone(),
        heroes: recomp_heroes(entry),
        soft: entry.hero.as_deref().is_some_and(is_low_res),
        covers: recomp_covers(entry),
        open: Route::Game(format!("recomp:{}", entry.id)),
        game: None,
    }
}

/// Up to five: games played lately that have a backdrop, then recomps not installed yet.
fn features(games: &[Game], recomps: &[RecompEntry]) -> Vec<Feature> {
    let mut played: Vec<&Game> = games.iter().filter(|g| g.last_played.is_some()).collect();
    played.sort_by_key(|g| std::cmp::Reverse(g.last_played));
    let with_art = played.into_iter().filter(|g| !hero_sources(g).is_empty());
    let mut list: Vec<Feature> = with_art.take(2).map(game_feature).collect();
    // Then the most played recomps you don't have yet, games you own first.
    let mut games_once: Vec<RecompEntry> = one_each(recomps).into_iter().map(|(r, _)| r).collect();
    games_once.retain(|r| r.installed.is_none() && r.hero.is_some() && r.problem.is_none());
    games_once.sort_by_key(|r| (r.owned_copy.is_none(), std::cmp::Reverse(r.downloads)));
    list.extend(games_once.iter().map(recomp_feature));
    list.truncate(5);
    list
}

/// Recomps you don't have yet, most played first, each game once.
fn not_installed(recomps: &[RecompEntry]) -> Vec<RecompEntry> {
    let mut list: Vec<RecompEntry> = one_each(recomps).into_iter().map(|(r, _)| r).collect();
    list.retain(|r| r.installed.is_none() && r.problem.is_none());
    list.sort_by_key(|r| std::cmp::Reverse(r.downloads));
    list
}

#[component]
pub fn Home() -> impl IntoView {
    let library = expect_context::<Library>();
    move || {
        let games = library.games.get().and_then(Result::ok).unwrap_or_default();
        let recomps = library.recomps.get().and_then(Result::ok).unwrap_or_default();
        let progress = library.progress.get();
        let features = features(&games, &recomps);

        let mut recent: Vec<&Game> = games.iter().filter(|g| g.last_played.is_some() || g.is_tracked()).collect();
        recent.sort_by_key(|g| std::cmp::Reverse(g.last_played));
        let recent: Vec<TileInfo> = recent.into_iter().take(14).map(|g| game_tile(g, library)).collect();

        let candidates = not_installed(&recomps);
        let tile = |r: &RecompEntry| recomp_tile(r, progress.get(&r.id));
        let found: Vec<TileInfo> = candidates.iter().filter(|r| r.owned_copy.is_some()).map(tile).collect();
        let popular: Vec<TileInfo> = candidates.iter().take(20).map(tile).collect();
        let mut newest = candidates.clone();
        newest.sort_by(|a, b| b.added.cmp(&a.added));
        let newest: Vec<TileInfo> = newest.iter().take(20).map(tile).collect();
        let everything: Vec<TileInfo> = games.iter().take(24).map(|g| game_tile(g, library)).collect();

        let slider = (!features.is_empty()).then(|| view! { <Slider features /> });
        let recent = (!recent.is_empty()).then(|| view! { <Shelf title="Jogue de novo".to_string() tiles=recent /> });
        let found = (!found.is_empty()).then(|| {
            view! {
                <Shelf
                    title="Encontrado nas suas pastas".to_string()
                    note="Você já tem esses jogos; cada um tem uma versão nativa para PC.".to_string()
                    tiles=found
                />
            }
        });
        let popular = (!popular.is_empty()).then(|| {
            view! {
                <Shelf
                    title="Populares agora".to_string()
                    note="Versões nativas para PC de jogos de console. Você precisa da sua própria cópia do jogo.".to_string()
                    tiles=popular
                    see_all=Route::Discover
                />
            }
        });
        let newest = (!newest.is_empty()).then(|| {
            view! { <Shelf title="Adicionados recentemente".to_string() tiles=newest see_all=Route::Discover /> }
        });
        let everything = (!everything.is_empty()).then(|| {
            view! { <Shelf title="Na sua biblioteca".to_string() tiles=everything see_all=Route::Library /> }
        });
        let loading = (games.is_empty() && recomps.is_empty()).then(|| view! { <p class="muted">"Carregando..."</p> });

        view! { <div class="page home">{loading}{slider}{recent}{found}{popular}{newest}{everything}</div> }
    }
}

#[component]
fn Slider(features: Vec<Feature>) -> impl IntoView {
    let library = expect_context::<Library>();
    let count = features.len();
    let current = RwSignal::new(0usize);
    let step = move |by: isize| current.update(|i| *i = (*i as isize + by).rem_euclid(count as isize) as usize);

    // Turns on its own every eight seconds, like a shop window.
    if count > 1 {
        let handle = set_interval_with_handle(move || step(1), Duration::from_secs(8));
        if let Ok(handle) = handle {
            on_cleanup(move || handle.clear());
        }
    }

    let slides = features
        .into_iter()
        .enumerate()
        .map(|(index, feature)| {
            let on = move || current.get() == index;
            let open = feature.open.clone();
            let open_cover = feature.open.clone();
            let game = feature.game.clone();
            let primary = match game {
                Some(game) => {
                    let on_play = move |_| play_game(library, game.clone());
                    view! {
                        <button type="button" class="btn main big" on:click=on_play>
                            <Svg icon=Icon::Play size=18 />"Jogar"
                        </button>
                    }
                    .into_any()
                }
                None => {
                    let open = open.clone();
                    view! {
                        <button type="button" class="btn main big" on:click=move |_| library.go(open.clone())>
                            "Ver e instalar"
                        </button>
                    }
                    .into_any()
                }
            };
            let about = (!feature.about.is_empty()).then(|| view! { <p>{feature.about.clone()}</p> });
            view! {
                <div class="slide" class:on=on aria-hidden=move || (!on()).to_string()>
                    <Backdrop sources=feature.heroes.clone() soft=feature.soft />
                    <div class="sl-in">
                        <span class="kick">{feature.kicker.clone()}</span>
                        <h2>{feature.title.clone()}</h2>
                        <span class="based">{feature.based.clone()}</span>
                        {about}
                        <div class="acts">
                            {primary}
                            <button type="button" class="btn big glass" on:click=move |_| library.go(open.clone())>
                                "Detalhes"
                            </button>
                        </div>
                    </div>
                    <button type="button" class="sl-cover" tabindex="-1" aria-hidden="true" on:click=move |_| library.go(open_cover.clone())>
                        <Poster sources=feature.covers.clone() title=feature.title.clone() />
                    </button>
                </div>
            }
        })
        .collect_view();

    let dots = (0..count)
        .map(|index| {
            view! {
                <button
                    type="button"
                    class="dot"
                    class:on=move || current.get() == index
                    aria-label=format!("Destaque {}", index + 1)
                    on:click=move |_| current.set(index)
                ></button>
            }
        })
        .collect_view();
    let controls = (count > 1).then(|| {
        view! {
            <div class="sl-ui">{dots}</div>
            <div class="sl-arrows">
                <button type="button" class="round glass" aria-label="Anterior" on:click=move |_| step(-1)>
                    <Svg icon=Icon::Left size=18 />
                </button>
                <button type="button" class="round glass" aria-label="Próximo" on:click=move |_| step(1)>
                    <Svg icon=Icon::Right size=18 />
                </button>
            </div>
        }
    });

    view! { <section class="slider" aria-roledescription="carrossel">{slides}{controls}</section> }
}
