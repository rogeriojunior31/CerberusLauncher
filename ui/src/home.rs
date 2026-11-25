//! Home: a rotating banner, then shelves of what to play next.

use cerberus_shared::tr;
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
    /// Your numbers with it: last session, total, status.
    facts: Vec<String>,
    /// The game's logo, drawn instead of the title when it loads.
    logo: Option<String>,
    /// A Steam game: its store description fills `about`.
    steam_app: Option<u32>,
}

fn game_feature(game: &Game, recomps: &[RecompEntry]) -> Feature {
    let mut facts = Vec::new();
    if let Some(last) = game.last_played {
        facts.push(tr!("Jogado {}", crate::format::ago(last)));
    }
    if let Some(s) = game.last_session_secs.filter(|s| *s >= 60) {
        facts.push(tr!("última sessão de {}", crate::format::playtime(s)));
    }
    if game.playtime_secs >= 60 {
        facts.push(tr!("{} no total", crate::format::playtime(game.playtime_secs)));
    }
    if let Some(s) = game.status {
        facts.push(crate::game_notes::label(s).to_string());
    }
    let steam_app = match &game.source {
        Source::Steam { app_id } => Some(*app_id),
        _ => None,
    };
    let logo = game.logo_path.as_deref().map(crate::art::src).or_else(|| {
        steam_app.map(|id| format!("https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/{id}/logo.png"))
    });
    let entry = match &game.source {
        Source::Recomp { recomp_id, .. } => recomps.iter().find(|e| e.id == *recomp_id),
        _ => None,
    };
    let (title, version) = crate::tiles::names_of(&game.name, entry);
    Feature {
        kicker: tr!("Continue jogando").to_string(),
        title,
        based: version.unwrap_or_else(|| source_name(&game.source).to_string()),
        facts,
        logo,
        steam_app,
        about: String::new(),
        heroes: hero_sources(game),
        soft: game.hero_url.as_deref().is_some_and(is_low_res) && matches!(game.source, Source::Recomp { .. }),
        covers: cover_sources(game),
        open: Route::Game(game.id.clone()),
        game: Some(game.clone()),
    }
}

fn recomp_feature(entry: &RecompEntry) -> Feature {
    let (title, version) = crate::tiles::entry_names(entry);
    Feature {
        kicker: tr!("Recomp · {}", entry.console),
        title,
        based: version.unwrap_or_default(),
        facts: Vec::new(),
        logo: entry.logo.as_deref().map(crate::art::src),
        steam_app: None,
        about: entry.about.clone(),
        heroes: recomp_heroes(entry),
        soft: entry.hero.as_deref().is_some_and(is_low_res),
        covers: recomp_covers(entry),
        open: Route::Game(format!("recomp:{}", entry.id)),
        game: None,
    }
}

/// Up to five: the player's own games played lately (up to four), then one
/// recomp they don't have yet, so the top of Home is their collection.
fn features(games: &[Game], recomps: &[RecompEntry]) -> Vec<Feature> {
    let mut played: Vec<&Game> = games.iter().filter(|g| g.last_played.is_some()).collect();
    played.sort_by_key(|g| std::cmp::Reverse(g.last_played));
    let with_art = played.into_iter().filter(|g| !hero_sources(g).is_empty());
    let mut list: Vec<Feature> = with_art.take(4).map(|g| game_feature(g, recomps)).collect();
    // Then the most played recomps you don't have yet, games you own first.
    let mut games_once: Vec<RecompEntry> = one_each(recomps).into_iter().map(|(r, _)| r).collect();
    games_once.retain(|r| r.installed.is_none() && r.hero.is_some() && r.problem.is_none());
    games_once.sort_by_key(|r| (r.owned_copy.is_none(), std::cmp::Reverse(r.downloads)));
    let remaining = if list.is_empty() { 5 } else { 1 };
    list.extend(games_once.iter().take(remaining).map(recomp_feature));
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
    // A classic played since the list was read shows in "Jogue de novo".
    library.roms.refetch();
    move || {
        let library_state = library.games.get();
        let read_error = library_state.as_ref().and_then(|state| state.as_ref().err()).cloned();
        // Home is about what's on this PC: owned-but-not-installed games stay in the Library.
        let games: Vec<Game> = library_state.and_then(Result::ok).unwrap_or_default().into_iter().filter(|g| g.next_step != cerberus_shared::NextStep::Install).collect();
        let recomps = library.recomps.get().and_then(Result::ok).unwrap_or_default();
        let features = features(&games, &recomps);

        // Every kind of game played, newest first: the classics too.
        let mut recent: Vec<(Option<u64>, TileInfo)> = games
            .iter()
            .filter(|g| !g.hidden && g.part_ids.is_empty() && (g.last_played.is_some() || g.is_tracked()))
            .map(|g| (g.last_played, game_tile(g, library)))
            .collect();
        let players = library.players.get().unwrap_or_default();
        recent.extend(
            crate::classics::standalone(library).iter()
                .filter(|r| r.last_played.is_some())
                .map(|r| (r.last_played, crate::classics::tile(r, &players, &recomps, &games))),
        );
        recent.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
        let recent: Vec<TileInfo> = recent.into_iter().take(14).map(|(_, tile)| tile).collect();

        let candidates = not_installed(&recomps);
        let found: Vec<TileInfo> = candidates.iter().filter(|r| r.owned_copy.is_some()).map(recomp_tile).collect();
        let popular: Vec<TileInfo> = candidates.iter().take(20).map(recomp_tile).collect();
        let mut newest = candidates.clone();
        newest.sort_by(|a, b| b.added.cmp(&a.added));
        let newest: Vec<TileInfo> = newest.iter().take(20).map(recomp_tile).collect();
        // The games played the most, not the first letters of the alphabet.
        let mut most: Vec<&Game> = games.iter().filter(|g| !g.hidden && g.part_ids.is_empty() && g.playtime_secs > 0).collect();
        most.sort_by_key(|g| std::cmp::Reverse(g.playtime_secs));
        let everything: Vec<TileInfo> = most
            .into_iter()
            .take(24)
            .map(|g| {
                let mut tile = game_tile(g, library);
                tile.sub = tr!("{} jogados", crate::format::playtime(g.playtime_secs));
                tile
            })
            .collect();

        let slider = (!features.is_empty()).then(|| view! { <Slider features /> });
        let recent = (!recent.is_empty()).then(|| view! { <Shelf title=tr!("Jogue de novo").to_string() tiles=recent /> });
        let favorites: Vec<TileInfo> = games.iter().filter(|g| g.favorite && !g.hidden && g.part_ids.is_empty()).map(|g| game_tile(g, library)).collect();
        let favorites = (!favorites.is_empty()).then(|| view! { <Shelf title=tr!("Favoritos").to_string() tiles=favorites see_all=Route::Library /> });
        let found = (!found.is_empty()).then(|| {
            view! {
                <Shelf
                    title=tr!("Encontrado nas suas pastas").to_string()
                    note=tr!("Você já tem esses jogos; cada um tem uma versão nativa para PC.").to_string()
                    tiles=found
                />
            }
        });
        let popular = (!popular.is_empty()).then(|| {
            view! {
                <Shelf
                    title=tr!("Populares agora").to_string()
                    note=tr!("Versões nativas para PC de jogos de console. Você precisa da sua própria cópia do jogo.").to_string()
                    tiles=popular
                    see_all=Route::Discover
                />
            }
        });
        let newest = (!newest.is_empty()).then(|| {
            view! { <Shelf title=tr!("Adicionados recentemente").to_string() tiles=newest see_all=Route::Discover /> }
        });
        let everything = (!everything.is_empty()).then(|| {
            view! { <Shelf title=tr!("Mais jogados").to_string() tiles=everything see_all=Route::Library /> }
        });
        let error = read_error.as_ref().map(|message| view! {
            <div class="card" role="alert">
                <p>{message.clone()}</p>
                <button type="button" class="btn" on:click=move |_| library.go(Route::Library)>{tr!("Abrir biblioteca")}</button>
            </div>
        });
        let loading = (read_error.is_none() && games.is_empty() && recomps.is_empty()).then(|| view! { <p class="muted">{tr!("Carregando...")}</p> });

        view! { <div class="page home">{error}{loading}{slider}{recent}{favorites}{everything}{found}{popular}{newest}</div> }
    }
}

#[component]
fn Slider(features: Vec<Feature>) -> impl IntoView {
    let library = expect_context::<Library>();
    let count = features.len();
    let current = RwSignal::new(0usize);
    let step = move |by: isize| current.update(|i| *i = (*i as isize + by).rem_euclid(count as isize) as usize);

    let reduced_motion = window().match_media("(prefers-reduced-motion: reduce)").ok().flatten();
    let rotating = RwSignal::new(!reduced_motion.as_ref().is_some_and(|media| media.matches()));
    let slider = NodeRef::<leptos::html::Section>::new();
    if count > 1 {
        let tick = move || {
            let hovering = slider.get_untracked().is_some_and(|el| el.matches(":hover").unwrap_or(false));
            if rotating.get_untracked() && !hovering {
                step(1);
            }
        };
        if let Ok(timer) = set_interval_with_handle(tick, Duration::from_secs(8)) {
            on_cleanup(move || timer.clear());
        }
    }

    let dots = features.iter().enumerate().map(|(index, feature)| {
        let name = tr!("Destaque {} de {} · {}", index + 1, count, feature.title);
        view! {
            <input type="radio" name="home-highlight" class="dot" aria-label=name.clone() title=name
                prop:checked=move || current.get() == index on:focus=move |_| rotating.set(false) on:change=move |_| current.set(index) />
        }
    }).collect_view();
    let slides = features
        .into_iter()
        .enumerate()
        .map(|(index, feature)| {
            let on = move || current.get() == index;
            let open = feature.open.clone();
            let open_cover = feature.open.clone();
            let game = feature.game.clone();
            let installed = game.is_some();
            let primary = match game {
                Some(game) => {
                    let (label, icon) = crate::launch_action(&game);
                    let on_play = move |_| play_game(library, game.clone());
                    view! {
                        <button type="button" class="btn main big" on:click=on_play>
                            <Svg icon size=18 />{label}
                        </button>
                    }
                    .into_any()
                }
                None => {
                    let open = open.clone();
                    view! {
                        <button type="button" class="btn main big" on:click=move |_| library.go(open.clone())>
                            {tr!("Ver e instalar")}
                        </button>
                    }
                    .into_any()
                }
            };
            // A Steam game's short description comes from its store page (kept on disk).
            let store = feature.steam_app.map(|app| LocalResource::new(move || async move { crate::api::steam_details(app).await.ok().map(|d| d.short) }));
            let fixed_about = feature.about.clone();
            let about = move || {
                let text = store.and_then(|s| s.get().flatten()).unwrap_or_else(|| fixed_about.clone());
                (!text.is_empty()).then(|| view! { <p class="sl-about">{text}</p> })
            };
            let facts = {
                let why = (!feature.kicker.is_empty()).then(|| view! { <span class="sl-why">{feature.kicker.clone()}</span> });
                let items = feature.facts.iter().map(|f| view! { <span>{f.clone()}</span> }).collect_view();
                view! { <div class="sl-facts">{why}{items}</div> }
            };
            let logo_failed = RwSignal::new(false);
            let title_text = StoredValue::new(feature.title.clone());
            let heading = match feature.logo.clone() {
                Some(src) => view! {
                    <Show when=move || !logo_failed.get() fallback=move || view! { <h2>{title_text.get_value()}</h2> }>
                        <img class="sl-logo" src=src.clone() alt=title_text.get_value() on:error=move |_| logo_failed.set(true) />
                    </Show>
                }
                .into_any(),
                None => view! { <h2>{feature.title.clone()}</h2> }.into_any(),
            };
            view! {
                <div class="slide" class:on=on inert=move || !on() aria-hidden=move || (!on()).to_string()>
                    <Backdrop sources=feature.heroes.clone() soft=feature.soft />
                    <div class="sl-in">
                        {heading}
                        <span class="based">{feature.based.clone()}</span>
                        {facts}
                        {about}
                        <div class="acts">
                            {primary}
                            {installed.then(|| view! {
                                <button type="button" class="btn big glass" on:click=move |_| library.go(open.clone())>{tr!("Detalhes")}</button>
                            })}
                        </div>
                    </div>
                    <button type="button" class="sl-cover" tabindex="-1" aria-hidden="true" on:click=move |_| library.go(open_cover.clone())>
                        <Poster sources=feature.covers.clone() title=feature.title.clone() />
                    </button>
                </div>
            }
        })
        .collect_view();

    let rotation_label = move || if rotating.get() { tr!("Pausar rotação") } else { tr!("Retomar rotação") };
    let controls = (count > 1).then(|| view! {
        <div class="sl-controls">
            <div class="sl-dots" role="radiogroup" aria-label=tr!("Escolher destaque")>{dots}</div>
            <div class="sl-arrows">
                <button type="button" class="round glass" aria-label=rotation_label title=rotation_label
                    on:click=move |_| rotating.update(|value| *value = !*value)>
                    {move || view! { <Svg icon=if rotating.get() { Icon::Pause } else { Icon::Play } size=16 /> }}
                </button>
                <button type="button" class="round glass" aria-label=tr!("Destaque anterior") on:focus=move |_| rotating.set(false) on:click=move |_| step(-1)>
                    <Svg icon=Icon::Left size=18 />
                </button>
                <button type="button" class="round glass" aria-label=tr!("Próximo destaque") on:focus=move |_| rotating.set(false) on:click=move |_| step(1)>
                    <Svg icon=Icon::Right size=18 />
                </button>
            </div>
        </div>
    });

    view! {
        <section node_ref=slider class="slider" class:multiple=(count > 1) aria-label=tr!("Destaques") aria-roledescription=tr!("carrossel")>
            <div on:focusin=move |_| rotating.set(false)>{slides}</div>
            {controls}
        </section>
    }
}
