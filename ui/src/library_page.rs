//! Library: every game, in tabs (Todos, Favoritos, Coleções, Recomps, PC,
//! Clássicos, and Ocultos when the player hid some), with search, a filter by
//! store for PC games, and sorting.

use cerberus_shared::{Game, Source};
use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::{Library, Route, format};
use crate::art::Poster;
use crate::icons::{Icon, Svg};
use crate::tiles::{Tile, game_tile, source_name};

/// The kind of game a tab shows.
#[derive(Clone, Copy, PartialEq)]
enum Category {
    All,
    Favorites,
    Collections,
    Recomps,
    Pc,
    Classics,
    Hidden,
}

impl Category {
    const ALL: [Category; 7] =
        [Category::All, Category::Favorites, Category::Collections, Category::Recomps, Category::Pc, Category::Classics, Category::Hidden];

    fn label(self) -> &'static str {
        match self {
            Category::All => "Todos",
            Category::Favorites => "Favoritos",
            Category::Collections => "Coleções",
            Category::Hidden => "Ocultos",
            Category::Recomps => "Recomps",
            Category::Pc => "PC",
            Category::Classics => "Clássicos",
        }
    }

    /// Hidden games are only under Ocultos.
    fn keeps(self, game: &Game, shelf: Option<&str>) -> bool {
        if game.hidden != (self == Category::Hidden) {
            return false;
        }
        match self {
            Category::All | Category::Hidden => true,
            Category::Favorites => game.favorite,
            Category::Collections => shelf.is_some_and(|name| game.collections.iter().any(|c| c == name)),
            Category::Recomps => matches!(game.source, Source::Recomp { .. }),
            Category::Pc => !matches!(game.source, Source::Recomp { .. }),
            Category::Classics => false,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Filter {
    All,
    Steam,
    Epic,
    Added,
}

impl Filter {
    /// The stores a PC game comes from.
    const STORES: [Filter; 4] = [Filter::All, Filter::Steam, Filter::Epic, Filter::Added];

    fn label(self) -> &'static str {
        match self {
            Filter::All => "Todas as origens",
            Filter::Steam => "Steam",
            Filter::Epic => "Epic",
            Filter::Added => "Adicionados",
        }
    }

    fn keeps(self, game: &Game) -> bool {
        match (self, &game.source) {
            (Filter::All, _) => true,
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
    category: RwSignal<Category>,
    search: RwSignal<String>,
    filter: RwSignal<Filter>,
    sort: RwSignal<Sort>,
    compact: RwSignal<bool>,
    /// The collection open under Coleções.
    shelf: RwSignal<Option<String>>,
}

impl Default for LibraryView {
    fn default() -> Self {
        Self {
            category: RwSignal::new(Category::All),
            search: RwSignal::new(String::new()),
            filter: RwSignal::new(Filter::All),
            sort: RwSignal::new(Sort::Name),
            compact: RwSignal::new(false),
            shelf: RwSignal::new(None),
        }
    }
}

/// Under Coleções: one chip per collection, and making, renaming or removing one.
#[component]
fn ShelfBar(shelves: LocalResource<Result<Vec<String>, String>>, open: Signal<Option<String>>, shelf: RwSignal<Option<String>>) -> impl IntoView {
    let library = expect_context::<Library>();
    // What the name box is for: a new collection, or renaming the open one.
    let editing = RwSignal::new(None::<Option<String>>);
    let typed = RwSignal::new(String::new());
    // A box made after the page loads doesn't take `autofocus`: focused here.
    let name_box = NodeRef::<leptos::html::Input>::new();
    Effect::new(move |_| {
        if let Some(input) = name_box.get() {
            let _ = input.focus();
        }
    });
    // One save at a time: a second Enter while the first is on its way does nothing.
    let saving = RwSignal::new(false);
    let save = move || {
        let Some(target) = editing.get_untracked() else { return };
        if saving.get_untracked() {
            return;
        }
        saving.set(true);
        let name = typed.get_untracked();
        leptos::task::spawn_local(async move {
            let done = match &target {
                None => crate::api::create_collection(&name).await,
                Some(old) => crate::api::rename_collection(old, &name).await,
            };
            if let Some(name) = library.report(done) {
                shelf.set(Some(name));
                editing.set(None);
                library.games.refetch();
            }
            saving.set(false);
        });
    };
    let chips = move || {
        shelves.get().and_then(Result::ok).unwrap_or_default().into_iter().map(|name| {
            let here = name.clone();
            let active = move || open.get().as_deref() == Some(here.as_str());
            let pick = name.clone();
            view! {
                <button type="button" class="chip-btn" class:on=active.clone() aria-pressed=move || active().to_string()
                    on:click=move |_| shelf.set(Some(pick.clone()))>{name}</button>
            }
        }).collect_view()
    };
    let remove = move |_| {
        let Some(name) = open.get_untracked() else { return };
        leptos::task::spawn_local(async move {
            if library.report(crate::api::delete_collection(&name).await).is_some() {
                shelf.set(None);
                library.games.refetch();
                library.toast(format!("Coleção {name} apagada. Os jogos continuam na biblioteca."));
            }
        });
    };
    view! {
        <div class="chips" role="group" aria-label="Coleções">
            {chips}
            {move || match editing.get() {
                Some(_) => view! {
                    <form class="shelf-form" on:submit=move |e| { e.prevent_default(); save(); }>
                        <input class="input" type="text" maxlength="40" placeholder="Nome da coleção" aria-label="Nome da coleção" node_ref=name_box
                            prop:value=move || typed.get() on:input=move |e| typed.set(event_target_value(&e))
                            on:keydown=move |e| if e.key() == "Escape" { editing.set(None); } />
                        <button type="submit" class="chip-btn on">"Salvar"</button>
                        <button type="button" class="chip-btn" on:click=move |_| editing.set(None)>"Cancelar"</button>
                    </form>
                }.into_any(),
                None => view! {
                    <button type="button" class="chip-btn" on:click=move |_| { typed.set(String::new()); editing.set(Some(None)); }>
                        <Svg icon=Icon::Plus size=14 />"Nova coleção"
                    </button>
                    {move || open.get().map(|name| {
                        let old = name.clone();
                        view! {
                            <button type="button" class="chip-btn" title="Renomear coleção" on:click=move |_| { typed.set(old.clone()); editing.set(Some(Some(old.clone()))); }>
                                <Svg icon=Icon::Edit size=14 />"Renomear"
                            </button>
                            <button type="button" class="chip-btn" title=format!("Apagar a coleção {name} (os jogos ficam)") on:click=remove>
                                <Svg icon=Icon::Trash size=14 />"Apagar"
                            </button>
                        }
                    })}
                }.into_any(),
            }}
        </div>
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
    let LibraryView { category, search, filter, sort, compact, shelf } = expect_context();
    let shelves = LocalResource::new(move || {
        library.games.track();
        crate::api::list_collections()
    });
    let any_hidden = move || library.games.with(|g| g.as_ref().and_then(|r| r.as_ref().ok()).is_some_and(|all| all.iter().any(|g| g.hidden)));
    // The open collection, once it's known to exist: the first one otherwise.
    let open_shelf = move || {
        let names = shelves.get().and_then(Result::ok).unwrap_or_default();
        shelf.get().filter(|s| names.contains(s)).or_else(|| names.first().cloned())
    };
    let recovery = LocalResource::new(move || {
        library.games.track();
        crate::api::library_recovery_notice()
    });
    let input = NodeRef::<leptos::html::Input>::new();
    let clear = move |_| {
        search.set(String::new());
        filter.set(Filter::All);
        if let Some(input) = input.get_untracked() {
            let _ = input.focus();
        }
    };
    let tabs = move || Category::ALL.into_iter().filter(|which| *which != Category::Hidden || any_hidden() || category.get() == Category::Hidden).map(|which| {
        let active = move || category.get() == which;
        view! {
            <button type="button" class="seg" role="tab" class:on=active aria-selected=move || active().to_string()
                on:click=move |_| { category.set(which); filter.set(Filter::All); }>{which.label()}</button>
        }
    }).collect_view();
    let chips = Filter::STORES.into_iter().map(|which| {
        let active = move || filter.get() == which;
        view! {
            <button type="button" class="chip-btn" class:on=active aria-pressed=move || active().to_string()
                on:click=move |_| filter.set(which)>{which.label()}</button>
        }
    }).collect_view();

    let results = move || {
        if category.get() == Category::Classics {
            let needle = search.get().trim().to_lowercase();
            return view! { <crate::classics::ClassicsGrid needle /> }.into_any();
        }
        let mut games = match library.games.get() {
            None => return view! { <p class="muted" role="status">"Procurando jogos…"</p> }.into_any(),
            Some(Err(error)) => return view! {
                <div class="empty">
                    <h2>"Não foi possível carregar a biblioteca"</h2>
                    <p class="problem" role="alert">{error}</p>
                    <button type="button" class="btn" on:click=move |_| {
                        library.games.refetch();
                        library.recomps.refetch();
                    }>"Tentar novamente"</button>
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
        let kind = category.get();
        let open = open_shelf();
        if kind == Category::Collections && open.is_none() {
            return view! {
                <div class="empty">
                    <Svg icon=Icon::Shelf size=32 />
                    <h2>"Nenhuma coleção ainda"</h2>
                    <p class="muted">"Crie uma coleção acima, ou clique com o botão direito na capa de um jogo e escolha Nova coleção."</p>
                </div>
            }.into_any();
        }
        games.retain(|g| kind.keeps(g, open.as_deref()) && source.keeps(g) && (needle.is_empty() || g.name.to_lowercase().contains(&needle)));
        if games.is_empty() && kind == Category::Hidden && needle.is_empty() {
            return view! {
                <div class="empty">
                    <Svg icon=Icon::EyeOff size=32 />
                    <h2>"Nenhum jogo oculto"</h2>
                    <p class="muted">"Todos os jogos voltaram para a biblioteca."</p>
                </div>
            }.into_any();
        }
        if games.is_empty() && kind == Category::Collections && needle.is_empty() {
            return view! {
                <div class="empty">
                    <Svg icon=Icon::Shelf size=32 />
                    <h2>"Essa coleção está vazia"</h2>
                    <p class="muted">"Clique com o botão direito na capa de um jogo e marque esta coleção."</p>
                </div>
            }.into_any();
        }
        if games.is_empty() && kind == Category::Favorites && needle.is_empty() {
            return view! {
                <div class="empty">
                    <Svg icon=Icon::Heart size=32 />
                    <h2>"Nenhum favorito ainda"</h2>
                    <p class="muted">"Na página de um jogo, clique em Favoritar. Os favoritos aparecem aqui e no menu lateral."</p>
                </div>
            }.into_any();
        }
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
            {move || recovery.get().and_then(Result::ok).flatten().map(|text| view! {
                <p class="card" role="status" style="overflow-wrap: anywhere">{text}</p>
            })}
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
                <div class="lib-tabs">
                    <div class="segs big" role="tablist" aria-label="Tipo de jogo">{tabs}</div>
                    <Show when=move || matches!(category.get(), Category::All | Category::Pc)>
                        <div class="chips" role="group" aria-label="Origem do jogo">{chips.clone()}</div>
                    </Show>
                    <Show when=move || category.get() == Category::Collections>
                        <ShelfBar shelves open=Signal::derive(open_shelf) shelf />
                    </Show>
                </div>
            </section>
            {results}
        </div>
    }
}
