//! Library: every game, with search, a filter by where it comes from, and sorting.

use cerberus_shared::{Game, Source};
use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::{Library, Route, format};
use crate::art::Poster;
use crate::icons::{Icon, Svg};
use crate::tiles::{Tile, game_tile, source_name};

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
    Playtime,
}

#[derive(Clone, Copy)]
pub struct LibraryView {
    search: RwSignal<String>,
    filter: RwSignal<Filter>,
    sort: RwSignal<Sort>,
    compact: RwSignal<bool>,
}

impl Default for LibraryView {
    fn default() -> Self {
        Self {
            search: RwSignal::new(String::new()),
            filter: RwSignal::new(Filter::All),
            sort: RwSignal::new(Sort::Name),
            compact: RwSignal::new(false),
        }
    }
}

fn navigate_rows(event: web_sys::KeyboardEvent) {
    if event.alt_key() || event.ctrl_key() || event.meta_key() || event.shift_key() {
        return;
    }
    let Some(row) = event.target().and_then(|target| target.dyn_into::<web_sys::HtmlButtonElement>().ok()) else {
        return;
    };
    let next = match event.key().as_str() {
        "ArrowDown" => row.next_element_sibling(),
        "ArrowUp" => row.previous_element_sibling(),
        "Home" => row.parent_element().and_then(|list| list.first_element_child()),
        "End" => row.parent_element().and_then(|list| list.last_element_child()),
        _ => return,
    };
    event.prevent_default();
    if let Some(button) = next.and_then(|el| el.dyn_into::<web_sys::HtmlButtonElement>().ok()) {
        let _ = button.focus();
    }
}

#[component]
pub fn LibraryPage() -> impl IntoView {
    let library = expect_context::<Library>();
    let LibraryView { search, filter, sort, compact } = expect_context();
    let input = NodeRef::<leptos::html::Input>::new();
    let clear = move |_| {
        search.set(String::new());
        filter.set(Filter::All);
        if let Some(input) = input.get_untracked() {
            let _ = input.focus();
        }
    };
    let chips = Filter::ALL.into_iter().map(|which| {
        let active = move || filter.get() == which;
        view! {
            <button type="button" class="chip-btn" class:on=active aria-pressed=move || active().to_string()
                on:click=move |_| filter.set(which)>{which.label()}</button>
        }
    }).collect_view();

    let results = move || {
        let mut games = match library.games.get() {
            None => return view! { <p class="muted" role="status">"Procurando jogos…"</p> }.into_any(),
            Some(Err(error)) => return view! {
                <div class="empty">
                    <h2>"Não foi possível carregar a biblioteca"</h2>
                    <p class="problem" role="alert">{error}</p>
                    <button type="button" class="btn" on:click=move |_| library.games.refetch()>"Tentar novamente"</button>
                </div>
            }.into_any(),
            Some(Ok(games)) => games,
        };
        if games.is_empty() {
            return view! {
                <div class="empty">
                    <Svg icon=Icon::Library size=32 />
                    <h2>"Sua biblioteca está vazia"</h2>
                    <p class="muted">"Adicione um jogo do PC ou encontre um recomp no catálogo."</p>
                    <button type="button" class="btn" on:click=move |_| library.go(Route::Discover)>"Explorar catálogo"</button>
                </div>
            }.into_any();
        }
        let needle = search.get().trim().to_lowercase();
        let source = filter.get();
        games.retain(|g| source.keeps(g) && (needle.is_empty() || g.name.to_lowercase().contains(&needle)));
        if games.is_empty() {
            return view! {
                <div class="empty">
                    <Svg icon=Icon::Search size=32 />
                    <h2>"Nenhum jogo encontrado"</h2>
                    <button type="button" class="btn" on:click=clear>"Limpar busca e filtros"</button>
                </div>
            }.into_any();
        }
        // list_games already returns alphabetical order, including ties below.
        match sort.get() {
            Sort::Name => {}
            Sort::Recent => games.sort_by_key(|g| std::cmp::Reverse(g.last_played)),
            Sort::Playtime => games.sort_by_key(|g| std::cmp::Reverse(g.playtime_secs)),
        }
        let count = games.len();
        let list = compact.get();
        let items = games.iter().map(|g| {
            let info = game_tile(g, library);
            if !list {
                return view! { <Tile info /> }.into_any();
            }
            let hours = if g.playtime_secs > 0 {
                format::playtime(g.playtime_secs)
            } else if g.is_tracked() {
                "Sem tempo registrado".into()
            } else {
                "—".into()
            };
            let badge = info.badge.map(|text| view! { <span class="library-status">{text}</span> });
            view! {
                <button type="button" class="library-row" on:click=move |_| library.go(info.open.clone())>
                    <span class="library-cover" aria-hidden="true"><Poster sources=info.covers title=g.name.clone() /></span>
                    <span class="library-title"><strong>{g.name.clone()}</strong><small>{source_name(&g.source)}</small></span>
                    {badge}
                    <span class="library-time" title="Tempo de jogo">{hours}</span>
                    <Svg icon=Icon::Right size=16 />
                </button>
            }.into_any()
        }).collect_view();
        view! {
            <section class="catalog-results">
                <h2>{if count == 1 { "1 jogo".into() } else { format!("{count} jogos") }}</h2>
                <div class=if list { "library-list" } else { "grid" }
                    on:keydown=move |e| { if list { navigate_rows(e); } }>{items}</div>
            </section>
        }.into_any()
    };

    let mode = move |list, icon, label: &'static str| {
        let active = move || compact.get() == list;
        view! {
            <button type="button" class:on=active aria-pressed=move || active().to_string()
                title=label aria-label=label on:click=move |_| compact.set(list)><Svg icon size=18 /></button>
        }
    };

    view! {
        <div class="page">
            <header class="page-h">
                <h1>"Biblioteca"</h1>
                <button type="button" class="btn" on:click=move |_| library.add_game()>
                    <Svg icon=Icon::Plus size=18 />"Adicionar jogo"
                </button>
            </header>
            <section class="catalog-filters" aria-label="Filtros da biblioteca">
                <div class="toolbar">
                    <label class="search">
                        <Svg icon=Icon::Search size=18 />
                        <input node_ref=input type="search" placeholder="Buscar na biblioteca" aria-label="Buscar na biblioteca"
                            prop:value=move || search.get() on:input=move |e| search.set(event_target_value(&e)) />
                    </label>
                    <select class="select" aria-label="Ordenar biblioteca"
                        on:change=move |e| sort.set(match event_target_value(&e).as_str() {
                            "recent" => Sort::Recent, "time" => Sort::Playtime, _ => Sort::Name,
                        })>
                        <option value="name" selected=move || sort.get() == Sort::Name>"A a Z"</option>
                        <option value="recent" selected=move || sort.get() == Sort::Recent>"Jogados recentemente"</option>
                        <option value="time" selected=move || sort.get() == Sort::Playtime>"Mais jogados"</option>
                    </select>
                    <div class="library-modes" role="group" aria-label="Visualização">
                        {mode(false, Icon::Library, "Ver em grade")}
                        {mode(true, Icon::List, "Ver em lista")}
                    </div>
                </div>
                <div class="chips" role="group" aria-label="Origem do jogo">{chips}</div>
            </section>
            {results}
        </div>
    }
}
