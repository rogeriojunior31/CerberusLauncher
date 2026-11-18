//! Library: every game, with search, a filter by where it comes from, and sorting.

use cerberus_shared::{Game, Source};
use leptos::prelude::*;

use crate::Library;
use crate::icons::{Icon, Svg};
use crate::tiles::{Tile, game_tile};

#[derive(Clone, Copy, PartialEq)]
enum Filter {
    All,
    Recomps,
    Steam,
    Epic,
    Added,
}

impl Filter {
    const ALL: [Filter; 5] = [Filter::All, Filter::Recomps, Filter::Steam, Filter::Epic, Filter::Added];

    fn label(self) -> &'static str {
        match self {
            Filter::All => "Todos",
            Filter::Recomps => "Recomps",
            Filter::Steam => "Steam",
            Filter::Epic => "Epic",
            Filter::Added => "Adicionados",
        }
    }

    fn keeps(self, game: &Game) -> bool {
        match (self, &game.source) {
            (Filter::All, _) => true,
            (Filter::Recomps, Source::Recomp { .. }) => true,
            (Filter::Steam, Source::Steam { .. }) => true,
            (Filter::Epic, Source::Epic { .. }) => true,
            (Filter::Added, Source::Local { .. }) => true,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Sort {
    Name,
    Recent,
}

#[component]
pub fn LibraryPage() -> impl IntoView {
    let library = expect_context::<Library>();
    let search = RwSignal::new(String::new());
    let filter = RwSignal::new(Filter::All);
    let sort = RwSignal::new(Sort::Name);

    let shown = move || {
        let games = library.games.get().and_then(Result::ok).unwrap_or_default();
        let needle = search.get().to_lowercase();
        let mut kept: Vec<Game> = games
            .into_iter()
            .filter(|g| filter.get().keeps(g))
            .filter(|g| needle.is_empty() || g.name.to_lowercase().contains(&needle))
            .collect();
        if sort.get() == Sort::Recent {
            kept.sort_by_key(|g| std::cmp::Reverse(g.last_played));
        }
        kept
    };
    let count = move || library.games.get().and_then(Result::ok).map(|g| g.len()).unwrap_or(0);

    let chips = Filter::ALL
        .into_iter()
        .map(|which| {
            view! {
                <button type="button" class="chip-btn" class:on=move || filter.get() == which on:click=move |_| filter.set(which)>
                    {which.label()}
                </button>
            }
        })
        .collect_view();

    let grid = move || {
        let games = shown();
        if library.games.get().is_none() {
            return view! { <p class="muted">"Procurando jogos..."</p> }.into_any();
        }
        if games.is_empty() {
            return view! {
                <div class="empty">
                    <h2>"Nada por aqui"</h2>
                    <p class="muted">"Nenhum jogo combina com a busca ou o filtro."</p>
                </div>
            }
            .into_any();
        }
        let tiles = games.iter().map(|g| view! { <Tile info=game_tile(g, library) /> }).collect_view();
        view! { <div class="grid">{tiles}</div> }.into_any()
    };

    view! {
        <div class="page">
            <header class="page-h">
                <div>
                    <h1>"Biblioteca"</h1>
                    <p class="muted">{move || format!("{} jogos", count())}</p>
                </div>
                <button type="button" class="btn" on:click=move |_| library.add_game()>
                    <Svg icon=Icon::Plus size=18 />"Adicionar jogo"
                </button>
            </header>
            <div class="toolbar">
                <label class="search">
                    <Svg icon=Icon::Search size=18 />
                    <input
                        type="search"
                        placeholder="Buscar na biblioteca"
                        prop:value=move || search.get()
                        on:input=move |e| search.set(event_target_value(&e))
                    />
                </label>
                <div class="chips">{chips}</div>
                <select
                    class="select"
                    aria-label="Ordenar"
                    on:change=move |e| sort.set(if event_target_value(&e) == "recent" { Sort::Recent } else { Sort::Name })
                >
                    <option value="name">"A a Z"</option>
                    <option value="recent">"Jogados recentemente"</option>
                </select>
            </div>
            {grid}
        </div>
    }
}
