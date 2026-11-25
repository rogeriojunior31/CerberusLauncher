//! Library: every game, in tabs (Todos, Favoritos, Coleções, Recomps, PC,
//! Clássicos, and Ocultos when the player hid some), with search, a filter by
//! store for PC games, and sorting.

use cerberus_shared::tr;
use cerberus_shared::{Game, GameStatus, RomView, Source};
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
            Category::Recomps => tr!("Nativos"),
            Category::Pc => "PC",
            Category::Classics => tr!("Clássicos"),
        }
    }

    /// What the tab holds, in a sentence: the words the whole app uses.
    fn meaning(self) -> Option<&'static str> {
        match self {
            Category::Recomps => Some(tr!("Versões nativas para PC de jogos de console (recomps, decomps e ports): rodam sem emulador, quase sempre com a sua cópia original do jogo.")),
            Category::Pc => Some(tr!("Jogos de PC: da Steam, da Epic e os que você adicionou.")),
            Category::Classics => Some(tr!("Seus jogos originais (ROMs e discos) das suas pastas, abertos num emulador. Os que têm versão nativa instalada ficam juntos dela.")),
            _ => None,
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
    /// Recomps and decomps (with any merged classic inside).
    Recomps,
    Steam,
    Epic,
    Added,
    /// The classics in the ROM folders.
    Roms,
}

impl Filter {
    /// Where a game comes from.
    const STORES: [Filter; 6] = [Filter::All, Filter::Recomps, Filter::Steam, Filter::Epic, Filter::Added, Filter::Roms];

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

/// One line of the library: a game, or a classic from the ROM folders.
enum Row {
    Game(Game),
    Classic(RomView),
}

/// The search finds a game by both names: the game's and the recomp's.
fn found(library: Library, game: &Game, needle: &str) -> bool {
    needle.is_empty() || {
        let (name, version) = crate::tiles::game_names(game, library);
        format!("{name} {}", version.unwrap_or_default()).to_lowercase().contains(needle)
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

/// Arrows move between the covers of the grid as they sit on screen (up and
/// down by a whole row), Home and End to the first and last.
fn navigate_grid(event: web_sys::KeyboardEvent) {
    if event.alt_key() || event.ctrl_key() || event.meta_key() || event.shift_key() {
        return;
    }
    let Some(grid) = event.current_target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) else { return };
    let Ok(found) = grid.query_selector_all(".tile-b") else { return };
    let tiles: Vec<web_sys::HtmlElement> = (0..found.length()).filter_map(|i| found.item(i)).filter_map(|n| n.dyn_into::<web_sys::HtmlElement>().ok()).collect();
    let Some(active) = document().active_element() else { return };
    let Some(at) = tiles.iter().position(|t| wasm_bindgen::JsValue::from(t.clone()) == wasm_bindgen::JsValue::from(active.clone())) else { return };
    // Covers in the first row: the grid's width in columns.
    // Measured on screen (each cover's own offset is inside its tile).
    let row_of = |t: &web_sys::HtmlElement| t.get_bounding_client_rect().top().round() as i64;
    let top = row_of(&tiles[0]);
    let columns = tiles.iter().take_while(|t| (row_of(t) - top).abs() < 4).count().max(1) as i64;
    let last = tiles.len() as i64 - 1;
    let to = match event.key().as_str() {
        "ArrowRight" => at as i64 + 1,
        "ArrowLeft" => at as i64 - 1,
        "ArrowDown" => at as i64 + columns,
        "ArrowUp" => at as i64 - columns,
        "Home" => 0,
        "End" => last,
        _ => return,
    };
    event.prevent_default();
    if let Some(tile) = tiles.get(to.clamp(0, last) as usize) {
        let _ = tile.focus();
        tile.scroll_into_view_with_bool(false);
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
    // Fresh play times for the classics shown under the other games.
    library.roms.refetch();
    let clear = move |_| {
        search.set(String::new());
        filter.set(Filter::All);
        status.set(None);
        absent.set(false);
        if let Some(input) = input.get_untracked() {
            let _ = input.focus();
        }
    };
    let tabs = move || Category::ALL.into_iter().filter(|which| *which != Category::Hidden || any_hidden() || category.get() == Category::Hidden).map(|which| {
        let active = move || category.get() == which;
        view! {
            <button type="button" class="seg" role="tab" class:on=active aria-selected=move || active().to_string() title=which.meaning()
                on:click=move |_| { category.set(which); filter.set(Filter::All); }>{which.label()}</button>
        }
    }).collect_view();
    // How many games each chip would show, for the tab and installed state in view.
    let counts = Memo::new(move |_| {
        let kind = category.get();
        let away = absent.get();
        let needle = search.get().trim().to_lowercase();
        let roms = if kind == Category::All { crate::classics::matching(crate::classics::standalone(library), &needle).len() } else { 0 };
        library.games.with(|all| {
            let all = all.as_ref().and_then(|r| r.as_ref().ok());
            let in_tab: Vec<&Game> = all
                .into_iter()
                .flatten()
                .filter(|g| kind.keeps(g, None) || kind == Category::Collections)
                .filter(|g| found(library, g, &needle))
                .collect();
            let mut stores = Filter::STORES.map(|f| in_tab.iter().filter(|g| f.keeps(g) && is_absent(g) == away).count());
            // The classics are in the ROM folders: counted as here, never as not installed.
            if !away {
                stores[0] += roms;
            }
            stores[5] = if away { 0 } else { roms };
            let missing = in_tab.iter().filter(|g| is_absent(g)).count();
            (stores, in_tab.len() - missing + roms, missing)
        })
    });
    // Status chips: only the statuses some game in view has, with how many.
    let statuses = move || {
        let kind = category.get();
        let away = absent.get();
        let counts: Vec<(GameStatus, usize)> = library.games.with(|all| {
            let needle = search.get().trim().to_lowercase();
            let all: Vec<&Game> = all.as_ref().and_then(|r| r.as_ref().ok()).into_iter().flatten()
                .filter(|g| kind.keeps(g, None) && is_absent(g) == away && found(library, g, &needle)).collect();
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
        // Always drawn, so the row never jumps; "Não instalados 0" just can't be picked.
        let (_, here, missing) = counts.get();
        view! {
            <div class="segs" role="group" aria-label=tr!("Instalação")>
                <button type="button" class="seg" class:on=move || !absent.get() aria-pressed=move || (!absent.get()).to_string()
                    on:click=move |_| absent.set(false)>{tr!("Instalados")}<span class="chip-n">{here}</span></button>
                <button type="button" class="seg" class:on=move || absent.get() aria-pressed=move || absent.get().to_string()
                    disabled=missing == 0 && !absent.get_untracked()
                    on:click=move |_| absent.set(true)>{tr!("Não instalados")}<span class="chip-n">{missing}</span></button>
            </div>
        }
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
        // Todos is every game, the classics in the ROM folders among the others
        // (those an installed recomp plays are that game), unless a filter they
        // can't match (a store, a status, not installed) is on.
        let with_classics = kind == Category::All && matches!(source, Filter::All | Filter::Roms) && !away && only.is_none();
        let classic_rows: Vec<RomView> =
            if with_classics { crate::classics::matching(crate::classics::standalone(library), &needle) } else { Vec::new() };
        games.retain(|g| {
            kind.keeps(g, open.as_deref())
                && source != Filter::Roms
                && source.keeps(g)
                && is_absent(g) == away
                && only.is_none_or(|s| g.status == Some(s))
                && found(library, g, &needle)
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
        if games.is_empty() && classic_rows.is_empty() {
            // Nothing here: where the search does find something, one click away.
            let in_catalog = if needle.is_empty() { 0 } else {
                let all = library.recomps.get().and_then(Result::ok).unwrap_or_default();
                crate::versions::one_each(&all).into_iter().filter(|(e, _)| crate::discover::matches_search(e, &needle)).count()
            };
            let elsewhere = if needle.is_empty() || away { 0 } else {
                library.games.with(|all| all.as_ref().and_then(|r| r.as_ref().ok()).map_or(0, |all| all.iter().filter(|g| is_absent(g) && found(library, g, &needle)).count()))
            };
            let query = search.get_untracked();
            return view! {
                <div class="empty">
                    <Svg icon=Icon::Search size=32 />
                    <h2>{tr!("Nenhum jogo encontrado")}</h2>
                    <div class="row">
                        {(in_catalog > 0).then(|| {
                            let q = query.clone();
                            view! { <button type="button" class="btn main" on:click=move |_| crate::discover::open_search(library, q.clone())>
                                {if in_catalog == 1 { tr!("1 no Descobrir").to_string() } else { tr!("{} no Descobrir", in_catalog) }}
                            </button> }
                        })}
                        {(elsewhere > 0).then(|| view! {
                            <button type="button" class="btn" on:click=move |_| absent.set(true)>
                                {if elsewhere == 1 { tr!("1 em Não instalados").to_string() } else { tr!("{} em Não instalados", elsewhere) }}
                            </button>
                        })}
                        <button type="button" class="btn" on:click=clear>{tr!("Limpar busca e filtros")}</button>
                    </div>
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
        // Classics join the same order: by the name shown, when played, how long.
        let players = library.players.get().unwrap_or_default();
        let recomps = library.recomps.get().and_then(Result::ok).unwrap_or_default();
        let lib_games = library.games.get().and_then(Result::ok).unwrap_or_default();
        let mut rows: Vec<Row> = games.into_iter().map(Row::Game).collect();
        rows.extend(classic_rows.into_iter().map(Row::Classic));
        let name_of = |r: &Row| match r {
            Row::Game(g) => crate::tiles::game_names(g, library).0.to_lowercase(),
            Row::Classic(rom) => crate::classics::display_name(rom).to_lowercase(),
        };
        match sort.get() {
            Sort::Name => rows.sort_by_cached_key(name_of),
            Sort::Recent => rows.sort_by_cached_key(|r| match r {
                Row::Game(g) => (std::cmp::Reverse(g.last_played), name_of(r)),
                Row::Classic(rom) => (std::cmp::Reverse(rom.last_played), name_of(r)),
            }),
            Sort::Playtime => rows.sort_by_cached_key(|r| match r {
                Row::Game(g) => (std::cmp::Reverse(g.playtime_secs), name_of(r)),
                Row::Classic(rom) => (std::cmp::Reverse(rom.playtime_secs), name_of(r)),
            }),
            // Rating and achievements are the library games' own: classics after them, in the order above.
            Sort::Rating | Sort::Achievements => rows.sort_by_key(|r| matches!(r, Row::Classic(_))),
        }
        let count = rows.len();
        let list = compact.get();
        let items = rows.iter().map(|row| {
            let g = match row {
                Row::Game(g) => g,
                Row::Classic(rom) => {
                    let info = crate::classics::tile(rom, &players, &recomps, &lib_games);
                    if !list {
                        return view! { <Tile info /> }.into_any();
                    }
                    let hours = if rom.playtime_secs > 0 { format::playtime(rom.playtime_secs) } else { "—".into() };
                    let open = info.open.clone();
                    return view! {
                        <button type="button" class="library-row" on:click=move |_| library.go(open.clone())>
                            <span class="library-cover" aria-hidden="true"><Poster sources=info.covers title=info.title.clone() /></span>
                            <span class="library-title"><strong>{info.title.clone()}</strong><small>{info.sub.clone()}</small></span>
                            <span class="library-time" title=tr!("Tempo de jogo")>{hours}</span>
                            <Svg icon=Icon::Right size=16 />
                        </button>
                    }.into_any();
                }
            };
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
                    on:keydown=move |e| { if list { navigate_rows(e) } else { navigate_grid(e) } }>{items}</div>
            </section>
        }.into_any()
    };

    // Origin, install state and status fold under "Filtros"; the button counts the ones on.
    let filters_open = RwSignal::new(false);
    let active_filters = move || status.get().is_some() as usize;
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
                        <kbd class="search-key" aria-hidden="true" title=tr!("Atalho: /")>"/"</kbd>
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
                    <button type="button" class="btn lib-filter-btn" class:on=move || { filters_open.get() || active_filters() > 0 }
                        aria-expanded=move || filters_open.get().to_string() on:click=move |_| filters_open.update(|o| *o = !*o)>
                        <Svg icon=Icon::Filter size=18 />{tr!("Filtros")}
                        {move || (active_filters() > 0).then(|| view! { <span class="tab-n">{active_filters()}</span> })}
                    </button>
                    <div class="library-modes" role="group" aria-label=tr!("Visualização")>
                        {mode(false, Icon::Library, tr!("Ver em grade"))}
                        {mode(true, Icon::List, tr!("Ver em lista"))}
                    </div>
                </div>
                <div class="lib-tabs">
                    <div class="segs big" role="tablist" aria-label=tr!("Tipo de jogo")>{tabs}</div>
                    // Always in sight: a search never hides the games that aren't installed without saying so.
                    <Show when=move || !matches!(category.get(), Category::Classics | Category::Hidden)>
                        <div class="lib-presence">{presence}</div>
                    </Show>
                    <Show when=move || category.get() == Category::Collections>
                        <ShelfBar shelves open=Signal::derive(open_shelf) shelf />
                    </Show>
                </div>
                <div class="lib-filters" class:hidden=move || !filters_open.get()>
                    {statuses}
                    <Show when=move || { active_filters() > 0 }>
                        <button type="button" class="link" on:click=move |_| status.set(None)>{tr!("Limpar filtros")}</button>
                    </Show>
                </div>
            </section>
            {results}
        </div>
    }
}
