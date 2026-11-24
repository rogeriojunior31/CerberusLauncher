//! Library: every game, in tabs (Todos, Favoritos, Coleções, Recomps, PC,
//! Clássicos, and Ocultos when the player hid some), with search, a filter by
//! store for PC games, and sorting.

use cerberus_shared::tr;
use cerberus_shared::{Game, GameStatus, Source};
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
            Category::All => tr!("Todos"),
            Category::Favorites => tr!("Favoritos"),
            Category::Collections => tr!("Coleções"),
            Category::Hidden => tr!("Ocultos"),
            Category::Recomps => tr!("Recomps"),
            Category::Pc => "PC",
            Category::Classics => tr!("Clássicos"),
        }
    }

    /// Hidden games are only under Ocultos.
    fn keeps(self, game: &Game, shelf: Option<&str>) -> bool {
        // A release carrying several games shows them, not itself.
        if !game.part_ids.is_empty() {
            return false;
        }
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
            Filter::All => tr!("Todas as origens"),
            Filter::Steam => tr!("Steam"),
            Filter::Epic => tr!("Epic"),
            Filter::Added => tr!("Adicionados"),
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
    Rating,
    /// Closest to 100% first; games without achievements at the end.
    Achievements,
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
    /// Installed games, or the account's games this PC doesn't have.
    absent: RwSignal<bool>,
    /// Only games the player marked with this status.
    status: RwSignal<Option<GameStatus>>,
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
            absent: RwSignal::new(false),
            status: RwSignal::new(None),
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
                library.toast(tr!("Coleção {} apagada. Os jogos continuam na biblioteca.", name));
            }
        });
    };
    view! {
        <div class="chips" role="group" aria-label=tr!("Coleções")>
            {chips}
            {move || match editing.get() {
                Some(_) => view! {
                    <form class="shelf-form" on:submit=move |e| { e.prevent_default(); save(); }>
                        <input class="input" type="text" maxlength="40" placeholder=tr!("Nome da coleção") aria-label=tr!("Nome da coleção") node_ref=name_box
                            prop:value=move || typed.get() on:input=move |e| typed.set(event_target_value(&e))
                            on:keydown=move |e| if e.key() == "Escape" { editing.set(None); } />
                        <button type="submit" class="chip-btn on">{tr!("Salvar")}</button>
                        <button type="button" class="chip-btn" on:click=move |_| editing.set(None)>{tr!("Cancelar")}</button>
                    </form>
                }.into_any(),
                None => view! {
                    <button type="button" class="chip-btn" on:click=move |_| { typed.set(String::new()); editing.set(Some(None)); }>
                        <Svg icon=Icon::Plus size=14 />{tr!("Nova coleção")}
                    </button>
                    {move || open.get().map(|name| {
                        let old = name.clone();
                        view! {
                            <button type="button" class="chip-btn" title=tr!("Renomear coleção") on:click=move |_| { typed.set(old.clone()); editing.set(Some(Some(old.clone()))); }>
                                <Svg icon=Icon::Edit size=14 />{tr!("Renomear")}
                            </button>
                            <button type="button" class="chip-btn" title=tr!("Apagar a coleção {} (os jogos ficam)", name) on:click=remove>
                                <Svg icon=Icon::Trash size=14 />{tr!("Apagar")}
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
    let LibraryView { category, search, filter, sort, compact, shelf, absent, status } = expect_context();
    // A status, rating or note changed on a game's page: list the games again.
    if crate::game_notes::take_changed() {
        library.games.refetch();
    }
    let picking = RwSignal::new(false);
    // Achievement progress for the covers, from what's already kept on this PC (no internet).
    let achievements = LocalResource::new(|| crate::api::achievements_hub(false));
    let progress_of = move || -> std::collections::HashMap<String, (u32, u32)> {
        achievements
            .get()
            .and_then(Result::ok)
            .map(|hub| {
                hub.games
                    .into_iter()
                    .filter(|g| g.source == cerberus_shared::AchievementSource::Steam)
                    .map(|g| (format!("steam:{}", g.key), (g.unlocked, g.total)))
                    .collect()
            })
            .unwrap_or_default()
    };
    // The games on screen, for "Play something" to pick from.
    let shown = StoredValue::new(Vec::<Game>::new());
    let is_absent = |g: &Game| g.next_step == cerberus_shared::NextStep::Install;
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
        status.set(None);
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
    // How many games each chip would show, for the tab and installed state in view.
    let counts = Memo::new(move |_| {
        let kind = category.get();
        let away = absent.get();
        library.games.with(|all| {
            let all = all.as_ref().and_then(|r| r.as_ref().ok());
            let in_tab: Vec<&Game> = all.into_iter().flatten().filter(|g| kind.keeps(g, None) || kind == Category::Collections).collect();
            let stores = Filter::STORES.map(|f| in_tab.iter().filter(|g| f.keeps(g) && is_absent(g) == away).count());
            let missing = in_tab.iter().filter(|g| is_absent(g)).count();
            (stores, in_tab.len() - missing, missing)
        })
    });
    let chips = Filter::STORES.into_iter().enumerate().map(|(i, which)| {
        let active = move || filter.get() == which;
        view! {
            <button type="button" class="chip-btn" class:on=active aria-pressed=move || active().to_string()
                on:click=move |_| filter.set(which)>
                {which.label()}<span class="chip-n">{move || counts.get().0[i]}</span>
            </button>
        }
    }).collect_view();
    // Status chips: only the statuses some game in view has, with how many.
    let statuses = move || {
        let kind = category.get();
        let away = absent.get();
        let counts: Vec<(GameStatus, usize)> = library.games.with(|all| {
            let all: Vec<&Game> = all.as_ref().and_then(|r| r.as_ref().ok()).into_iter().flatten().filter(|g| kind.keeps(g, None) && is_absent(g) == away).collect();
            GameStatus::ALL.into_iter().map(|s| (s, all.iter().filter(|g| g.status == Some(s)).count())).filter(|(_, n)| *n > 0).collect()
        });
        (!counts.is_empty()).then(|| {
            let chips = counts
                .into_iter()
                .map(|(s, n)| {
                    view! {
                        <button type="button" class=format!("chip-btn st {}", crate::game_notes::class(s)) class:on=move || status.get() == Some(s)
                            aria-pressed=move || (status.get() == Some(s)).to_string()
                            on:click=move |_| status.update(|cur| *cur = if *cur == Some(s) { None } else { Some(s) })>
                            <i class="st-dot"></i>{crate::game_notes::label(s)}<span class="chip-n">{n}</span>
                        </button>
                    }
                })
                .collect_view();
            view! { <div class="chips" role="group" aria-label=tr!("Situação")>{chips}</div> }
        })
    };
    let presence = move || {
        let (_, here, missing) = counts.get();
        (missing > 0).then(|| view! {
            <div class="segs" role="group" aria-label=tr!("Instalação")>
                <button type="button" class="seg" class:on=move || !absent.get() aria-pressed=move || (!absent.get()).to_string()
                    on:click=move |_| absent.set(false)>{tr!("Instalados")}<span class="chip-n">{here}</span></button>
                <button type="button" class="seg" class:on=move || absent.get() aria-pressed=move || absent.get().to_string()
                    on:click=move |_| absent.set(true)>{tr!("Não instalados")}<span class="chip-n">{missing}</span></button>
            </div>
        })
    };

    let results = move || {
        if category.get() == Category::Classics {
            let needle = search.get().trim().to_lowercase();
            return view! { <crate::classics::ClassicsGrid needle /> }.into_any();
        }
        let mut games = match library.games.get() {
            None => return view! { <p class="muted" role="status">{tr!("Procurando jogos…")}</p> }.into_any(),
            Some(Err(error)) => return view! {
                <div class="empty">
                    <h2>{tr!("Não foi possível carregar a biblioteca")}</h2>
                    <p class="problem" role="alert">{error}</p>
                    <button type="button" class="btn" on:click=move |_| {
                        library.games.refetch();
                        library.recomps.refetch();
                    }>{tr!("Tentar novamente")}</button>
                </div>
            }.into_any(),
            Some(Ok(games)) => games,
        };
        if games.is_empty() {
            return view! {
                <div class="empty">
                    <Svg icon=Icon::Library size=32 />
                    <h2>{tr!("Sua biblioteca está vazia")}</h2>
                    <p class="muted">{tr!("Adicione um jogo do PC ou encontre um recomp no catálogo.")}</p>
                    <button type="button" class="btn" on:click=move |_| library.go(Route::Discover)>{tr!("Explorar catálogo")}</button>
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
                    <h2>{tr!("Nenhuma coleção ainda")}</h2>
                    <p class="muted">{tr!("Crie uma coleção acima, ou clique com o botão direito na capa de um jogo e escolha Nova coleção.")}</p>
                </div>
            }.into_any();
        }
        let away = absent.get();
        let only = status.get();
        games.retain(|g| {
            kind.keeps(g, open.as_deref())
                && source.keeps(g)
                && is_absent(g) == away
                && only.is_none_or(|s| g.status == Some(s))
                && (needle.is_empty() || {
                    // Both names find it: the game's and the recomp's.
                    let (name, version) = crate::tiles::game_names(g, library);
                    format!("{name} {}", version.unwrap_or_default()).to_lowercase().contains(&needle)
                })
        });
        if games.is_empty() && kind == Category::Hidden && needle.is_empty() {
            return view! {
                <div class="empty">
                    <Svg icon=Icon::EyeOff size=32 />
                    <h2>{tr!("Nenhum jogo oculto")}</h2>
                    <p class="muted">{tr!("Todos os jogos voltaram para a biblioteca.")}</p>
                </div>
            }.into_any();
        }
        if games.is_empty() && kind == Category::Collections && needle.is_empty() {
            return view! {
                <div class="empty">
                    <Svg icon=Icon::Shelf size=32 />
                    <h2>{tr!("Essa coleção está vazia")}</h2>
                    <p class="muted">{tr!("Clique com o botão direito na capa de um jogo e marque esta coleção.")}</p>
                </div>
            }.into_any();
        }
        if games.is_empty() && kind == Category::Favorites && needle.is_empty() {
            return view! {
                <div class="empty">
                    <Svg icon=Icon::Heart size=32 />
                    <h2>{tr!("Nenhum favorito ainda")}</h2>
                    <p class="muted">{tr!("Na página de um jogo, clique em Favoritar. Os favoritos aparecem aqui e no menu lateral.")}</p>
                </div>
            }.into_any();
        }
        if games.is_empty() {
            return view! {
                <div class="empty">
                    <Svg icon=Icon::Search size=32 />
                    <h2>{tr!("Nenhum jogo encontrado")}</h2>
                    <button type="button" class="btn" on:click=clear>{tr!("Limpar busca e filtros")}</button>
                </div>
            }.into_any();
        }
        let progress = progress_of();
        // list_games already returns alphabetical order, including ties below.
        match sort.get() {
            // A to Z by the name shown: a recomp sits under its game's letter.
            Sort::Name => games.sort_by_cached_key(|g| crate::tiles::game_names(g, library).0.to_lowercase()),
            Sort::Recent => games.sort_by_key(|g| std::cmp::Reverse(g.last_played)),
            Sort::Playtime => games.sort_by_key(|g| std::cmp::Reverse(g.playtime_secs)),
            Sort::Rating => games.sort_by_key(|g| std::cmp::Reverse(g.rating)),
            Sort::Achievements => games.sort_by_key(|g| {
                // Per mille, so 3/4 and 75/100 rank the same; then more unlocked first.
                let (unlocked, total) = progress.get(&g.id).copied().filter(|p| p.1 > 0).unwrap_or((0, 0));
                std::cmp::Reverse((total > 0, if total > 0 { unlocked * 1000 / total } else { 0 }, unlocked))
            }),
        }
        shown.set_value(games.clone());
        let count = games.len();
        let list = compact.get();
        let items = games.iter().map(|g| {
            let mut info = game_tile(g, library);
            info.progress = progress.get(&g.id).copied();
            if !list {
                return view! { <Tile info /> }.into_any();
            }
            let hours = if g.playtime_secs > 0 {
                format::playtime(g.playtime_secs)
            } else if g.is_tracked() {
                tr!("Sem tempo registrado").into()
            } else {
                "—".into()
            };
            let badge = info.badge.map(|text| view! { <span class="library-status">{text}</span> });
            view! {
                <button type="button" class="library-row" on:click=move |_| library.go(info.open.clone())>
                    <span class="library-cover" aria-hidden="true"><Poster sources=info.covers title=g.name.clone() /></span>
                    <span class="library-title"><strong>{g.name.clone()}</strong><small>{source_name(&g.source)}</small></span>
                    {badge}
                    <span class="library-time" title=tr!("Tempo de jogo")>{hours}</span>
                    <Svg icon=Icon::Right size=16 />
                </button>
            }.into_any()
        }).collect_view();
        view! {
            <section class="catalog-results">
                <h2>{if count == 1 { tr!("1 jogo").into() } else { tr!("{} jogos", count) }}</h2>
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
                <h1>{tr!("Biblioteca")}</h1>
                <div class="row">
                    <button type="button" class="btn" on:click=move |_| picking.set(true) title=tr!("Sorteia um dos jogos instalados que estão na tela")>
                        <Svg icon=Icon::Dice size=18 />{tr!("Jogar algo aleatório")}
                    </button>
                    <button type="button" class="btn" on:click=move |_| library.add_game()>
                        <Svg icon=Icon::Plus size=18 />{tr!("Adicionar jogo")}
                    </button>
                </div>
            </header>
            {move || picking.get().then(|| {
                let pool: Vec<Game> = shown.get_value().into_iter().filter(|g| !is_absent(g)).collect();
                view! { <crate::game_notes::RandomPick pool open=picking /> }
            })}
            {move || recovery.get().and_then(Result::ok).flatten().map(|text| view! {
                <p class="card" role="status" style="overflow-wrap: anywhere">{text}</p>
            })}
            <section class="catalog-filters" aria-label=tr!("Filtros da biblioteca")>
                <div class="toolbar">
                    <label class="search">
                        <Svg icon=Icon::Search size=18 />
                        <input node_ref=input type="search" placeholder=tr!("Buscar na biblioteca") aria-label=tr!("Buscar na biblioteca")
                            prop:value=move || search.get() on:input=move |e| search.set(event_target_value(&e)) />
                    </label>
                    <select class="select" aria-label=tr!("Ordenar biblioteca")
                        on:change=move |e| sort.set(match event_target_value(&e).as_str() {
                            "recent" => Sort::Recent, "time" => Sort::Playtime, "rating" => Sort::Rating, "achievements" => Sort::Achievements, _ => Sort::Name,
                        })>
                        <option value="name" selected=move || sort.get() == Sort::Name>{tr!("A a Z")}</option>
                        <option value="recent" selected=move || sort.get() == Sort::Recent>{tr!("Jogados recentemente")}</option>
                        <option value="time" selected=move || sort.get() == Sort::Playtime>{tr!("Mais jogados")}</option>
                        <option value="rating" selected=move || sort.get() == Sort::Rating>{tr!("Sua nota")}</option>
                        <option value="achievements" selected=move || sort.get() == Sort::Achievements>{tr!("Conquistas")}</option>
                    </select>
                    <div class="library-modes" role="group" aria-label=tr!("Visualização")>
                        {mode(false, Icon::Library, tr!("Ver em grade"))}
                        {mode(true, Icon::List, tr!("Ver em lista"))}
                    </div>
                </div>
                <div class="lib-tabs">
                    <div class="segs big" role="tablist" aria-label=tr!("Tipo de jogo")>{tabs}</div>
                    <Show when=move || matches!(category.get(), Category::All | Category::Pc)>
                        <div class="chips" role="group" aria-label=tr!("Origem do jogo")>{chips.clone()}</div>
                        {presence}
                    </Show>
                    {statuses}
                    <Show when=move || category.get() == Category::Collections>
                        <ShelfBar shelves open=Signal::derive(open_shelf) shelf />
                    </Show>
                </div>
            </section>
            {results}
        </div>
    }
}
