//! The TV screen's sections: Início, Biblioteca, Descobrir, Downloads, Configurações.

use cerberus_shared::tr;
use cerberus_shared::{Game, InstallStep, RecompEntry, Source};
use leptos::prelude::*;
use leptos::task::spawn_local;

use super::{Btn, Focus, Sheet, Sound, Tv, View, glyph, play};
use crate::art::{Poster, cover_sources, hero_sources, recomp_covers, recomp_heroes};
use crate::discover::start_install;
use crate::versions::one_each;
use crate::{Library, api, format, play_game};

/// Something on a shelf: a game in the library, or a recomp from the catalog.
#[derive(Clone, PartialEq)]
pub enum Item {
    Game(Game),
    Entry(RecompEntry),
    /// A game from the ROM folders, with its console.
    Classic(cerberus_shared::RomView, &'static str),
}

impl Item {
    pub fn id(&self) -> String {
        match self {
            Item::Game(g) => g.id.clone(),
            Item::Entry(e) => format!("recomp:{}", e.id),
            Item::Classic(r, _) => format!("classic:{}", r.path),
        }
    }
    pub fn name(&self) -> String {
        match self {
            Item::Game(g) => crate::tiles::game_names(g, expect_context::<Library>()).0,
            Item::Entry(e) => crate::tiles::entry_names(e).0,
            Item::Classic(r, _) => crate::classics::display_name(r),
        }
    }
    pub fn covers(&self) -> Vec<String> {
        match self {
            Item::Game(g) => cover_sources(g),
            Item::Entry(e) => recomp_covers(e),
            Item::Classic(r, c) => crate::classics::box_art(r, c),
        }
    }
    pub fn heroes(&self) -> Vec<String> {
        match self {
            Item::Game(g) => {
                let h = hero_sources(g);
                if h.is_empty() { cover_sources(g) } else { h }
            }
            Item::Entry(e) => {
                let h = recomp_heroes(e);
                if h.is_empty() { recomp_covers(e) } else { h }
            }
            Item::Classic(r, c) => crate::classics::box_art(r, c),
        }
    }
    pub fn recomp_id(&self) -> Option<String> {
        match self {
            Item::Game(Game { source: Source::Recomp { recomp_id, .. }, .. }) => Some(recomp_id.clone()),
            Item::Entry(e) => Some(e.id.clone()),
            _ => None,
        }
    }
}

/// The catalog entry behind a library game, if it is a recomp.
pub fn entry_of(library: Library, game: &Game) -> Option<RecompEntry> {
    let Source::Recomp { recomp_id, .. } = &game.source else { return None };
    library.recomps.with_untracked(|r| r.as_ref().and_then(|r| r.as_ref().ok()).and_then(|all| all.iter().find(|e| e.id == *recomp_id).cloned()))
}

/// Download progress for a recomp, 0-100, while one runs.
pub fn progress_of(library: Library, recomp_id: &str) -> Option<u64> {
    library.progress.with(|p| match p.get(recomp_id)? {
        InstallStep::Downloading { done, total } | InstallStep::Paused { done, total } if *total > 0 => Some(done.min(total) * 100 / total),
        InstallStep::Downloading { .. } => Some(0),
        InstallStep::Unpacking => Some(100),
        _ => None,
    })
}

/// A cover on a shelf or grid, with its name and what's going on with it.
pub fn tile(item: Item, key: String, on: impl Fn() -> bool + Copy + Send + Sync + 'static) -> impl IntoView {
    let library = expect_context::<Library>();
    let id = item.id();
    let rid = item.recomp_id();
    let badge = {
        let item = item.clone();
        move || {
            if library.running.with(|r| r.contains(&id)) {
                return Some(tr!("Jogando"));
            }
            match &item {
                Item::Game(g) if g.next_step != cerberus_shared::NextStep::Play => Some(tr!("Configurar")),
                Item::Entry(e) if e.installed.is_some() => Some(tr!("Instalado")),
                Item::Classic(..) => Some(tr!("Clássico")),
                Item::Entry(e) if e.owned_copy.is_some() => Some(tr!("Você tem")),
                _ => None,
            }
        }
    };
    let bar = move || {
        let percent = rid.as_deref().and_then(|r| progress_of(library, r))?;
        Some(view! { <div class="tv-bar"><i style=format!("width:{percent}%")></i></div> })
    };
    // With a mouse: a click opens the game's page, as Y does.
    let tv = expect_context::<Tv>();
    let clicked = item.clone();
    let on_click = move |_| {
        play(Sound::Select);
        match &clicked {
            Item::Classic(r, c) => open_classic(library, tv, r, c),
            other => tv.open(View::Game(other.id())),
        }
    };
    view! {
        <div class="tv-tile" class:on=on data-tvf=key on:click=on_click>
            <div class="tv-cover">
                <Poster sources=item.covers() title=item.name() />
                {move || badge().map(|b| view! { <span class="tv-tag">{b}</span> })}
                {bar}
            </div>
            <span class="tv-name">{item.name()}</span>
        </div>
    }
}

/// Hidden games stay out of Big Picture.
fn library_games(library: Library) -> Vec<Game> {
    library.games.get().and_then(Result::ok).unwrap_or_default().into_iter().filter(|g| !g.hidden && g.part_ids.is_empty() && g.next_step != cerberus_shared::NextStep::Install).collect()
}

/// The player's classics an emulator here opens (as Play decides it), the
/// ones played lately first.
fn classics(roms: &[cerberus_shared::RomView], players: &[cerberus_shared::ClassicPlayer]) -> Vec<Item> {
    let mut list: Vec<Item> = roms
        .iter()
        .filter_map(|r| {
            let console = crate::classics::console_of(r);
            crate::classics::ready_player(players, console).map(|_| Item::Classic(r.clone(), console))
        })
        .collect();
    list.sort_by_key(|i| match i {
        Item::Classic(r, _) => (std::cmp::Reverse(r.last_played), crate::classics::display_name(r)),
        _ => (std::cmp::Reverse(None), String::new()),
    });
    list
}

/// Starts the classic itself, in the emulator that opens it here. Its recomp,
/// if any, has a tile of its own once installed.
fn open_classic(library: Library, _tv: Tv, rom: &cerberus_shared::RomView, console: &str) {
    play(Sound::Select);
    crate::classics::play_original(library, rom.path.clone(), console.to_string());
}

fn catalog(library: Library) -> Vec<RecompEntry> {
    library.recomps.get().and_then(Result::ok).unwrap_or_default()
}

/// The opening screen: a big picture of the game in focus, the games played
/// lately in a dock, and shelves below.
#[component]
pub fn Home() -> impl IntoView {
    let library = expect_context::<Library>();
    let tv = expect_context::<Tv>();
    let focus = Focus::new("home", false);
    tv.keep_position(focus);

    library.roms.refetch();
    let shelves = Memo::new(move |_| {
        let classic_items = classics(&library.roms.get().unwrap_or_default(), &library.players.get().unwrap_or_default());
        let mut games = library_games(library);
        games.sort_by_key(|g| (std::cmp::Reverse(g.last_played), g.name.to_lowercase()));
        let dock: Vec<Item> = games.iter().take(12).cloned().map(Item::Game).collect();
        let rest: Vec<Item> = games.iter().skip(12).take(14).cloned().map(Item::Game).collect();
        let favorites: Vec<Item> = games.iter().filter(|g| g.favorite).take(14).cloned().map(Item::Game).collect();
        let lead = one_each(&catalog(library)).into_iter().map(|(e, _)| e).collect::<Vec<_>>();
        let mut owned: Vec<RecompEntry> = lead.iter().filter(|e| e.owned_copy.is_some() && e.installed.is_none()).cloned().collect();
        owned.sort_by_key(|e| std::cmp::Reverse(e.downloads));
        let mut popular: Vec<RecompEntry> = lead.iter().filter(|e| e.installed.is_none() && e.latest.is_some()).cloned().collect();
        popular.sort_by_key(|e| std::cmp::Reverse(e.downloads));
        let mut fresh: Vec<RecompEntry> = lead.iter().filter(|e| e.latest.is_some()).cloned().collect();
        fresh.sort_by(|a, b| b.released.cmp(&a.released));
        let entries = |list: Vec<RecompEntry>| list.into_iter().take(14).map(Item::Entry).collect::<Vec<_>>();
        vec![
            (tr!("Jogados recentemente"), dock),
            (tr!("Favoritos"), favorites),
            (tr!("Sua biblioteca"), rest),
            (tr!("Seus clássicos"), classic_items.into_iter().take(14).collect()),
            (tr!("Você já tem o jogo"), entries(owned)),
            (tr!("Populares"), entries(popular)),
            (tr!("Atualizados recentemente"), entries(fresh)),
        ]
    });
    let rows = move || shelves.with(|s| s.iter().map(|(_, items)| items.len()).collect::<Vec<_>>());
    Effect::new(move |_| focus.clamp(&rows()));

    let focused = move || {
        let (r, c) = (focus.row.get(), focus.col.get());
        shelves.with(|s| s.get(r).and_then(|(_, items)| items.get(c).cloned()))
    };
    Effect::new(move |_| {
        if let Some(item) = focused() {
            tv.show_backdrop(item.heroes());
        }
    });

    tv.hints.set(vec![(Btn::A, tr!("Jogar")), (Btn::Y, tr!("Página do jogo")), (Btn::X, tr!("Opções"))]);
    tv.handle_page(move |btn| {
        let item = focused();
        match btn {
            Btn::Up | Btn::Down | Btn::Left | Btn::Right => {
                if focus.step(btn, &rows()) {
                    play(Sound::Move);
                }
                true
            }
            // In the dock, A plays at once; elsewhere it opens the game.
            Btn::A => {
                match item {
                    Some(Item::Game(g)) if focus.row.get_untracked() == 0 => {
                        play(Sound::Select);
                        play_game(library, g);
                    }
                    Some(Item::Classic(r, c)) => open_classic(library, tv, &r, c),
                    Some(i) => tv.open(View::Game(i.id())),
                    None => {}
                }
                true
            }
            Btn::Y => {
                match item {
                    Some(Item::Classic(r, c)) => open_classic(library, tv, &r, c),
                    Some(i) => tv.open(View::Game(i.id())),
                    None => {}
                }
                true
            }
            Btn::X => {
                if let Some(i) = item.filter(|i| !matches!(i, Item::Classic(..))) {
                    tv.sheet.set(Some(Sheet::Options(i.id())));
                    play(Sound::Select);
                }
                true
            }
            _ => false,
        }
    });

    let hero = move || {
        let item = focused()?;
        let status = match &item {
            Item::Game(g) if library.running.with(|r| r.contains(&g.id)) => tr!("Jogando agora").to_string(),
            Item::Game(g) => match g.last_played {
                Some(t) => tr!("Jogado {} · {}", format::ago(t), format::playtime(g.playtime_secs)),
                None => tr!("Na sua biblioteca").to_string(),
            },
            Item::Entry(e) if e.owned_copy.is_some() => tr!("Você já tem o jogo").to_string(),
            Item::Entry(e) => format!("{} downloads", format::count(e.downloads)),
            Item::Classic(r, c) => crate::classics::classic_sub(r, c, &library.players.get_untracked().unwrap_or_default()),
        };
        let entry = match &item {
            Item::Game(g) => entry_of(library, g),
            Item::Entry(e) => Some(e.clone()),
            Item::Classic(..) => None,
        };
        let based = entry.clone().filter(|e| !e.game.is_empty()).map(|e| tr!("Baseado em {} · {}", e.game, e.console));
        let logo = match &item {
            Item::Game(g) => g.logo_path.clone().or_else(|| entry.as_ref().and_then(|e| e.logo.clone())),
            Item::Entry(e) => e.logo.clone(),
            Item::Classic(..) => None,
        };
        let title = match logo {
            Some(path) => view! { <img class="tv-logo" src=crate::art::src(&path) alt=item.name() /> }.into_any(),
            None => view! { <h1>{item.name()}</h1> }.into_any(),
        };
        let action = match &item {
            Item::Game(g) if focus.row.get() == 0 => crate::launch_action(g).0,
            Item::Game(_) => tr!("Abrir"),
            Item::Entry(_) => tr!("Ver e instalar"),
            Item::Classic(..) => tr!("Jogar"),
        };
        let kind = tv.pad.get();
        Some(view! {
            <div class="tv-hero">
                <span class="tv-kicker">{status}</span>
                {title}
                {based.map(|b| view! { <p class="tv-based">{b}</p> })}
                <div class="tv-hero-actions">
                    <span>{glyph(Btn::A, kind)}{action}</span>
                    <span>{glyph(Btn::Y, kind)}{tr!("Página do jogo")}</span>
                </div>
            </div>
        })
    };

    let body = move || {
        let all = shelves.get();
        if all.iter().all(|(_, items)| items.is_empty()) {
            return view! { <p class="tv-empty">{tr!("Carregando sua biblioteca...")}</p> }.into_any();
        }
        all.into_iter()
            .enumerate()
            .filter(|(_, (_, items))| !items.is_empty())
            .map(|(r, (title, items))| {
                let tiles = items
                    .into_iter()
                    .enumerate()
                    .map(|(c, item)| tile(item, focus.key(r, c), move || focus.is(r, c)))
                    .collect_view();
                view! {
                    <section class="tv-shelf" class:dock=r == 0 class:current=move || focus.row.get() == r>
                        {(r > 0).then(|| view! { <h2>{title}</h2> })}
                        <div class="tv-row">{tiles}</div>
                    </section>
                }
            })
            .collect_view()
            .into_any()
    };

    let scrolled = move || focus.row.get() > 0;
    view! {
        <div class="tv-page home" class:scrolled=scrolled>
            {hero}
            <div class="tv-shelves">{body}</div>
        </div>
    }
}

/// Columns in TV grids.
const COLUMNS: usize = 6;

fn grid_rows(len: usize) -> Vec<usize> {
    (0..len.div_ceil(COLUMNS)).map(|r| (len - r * COLUMNS).min(COLUMNS)).collect()
}

/// Every game in the library, the ones played lately first.
#[component]
pub fn LibraryGrid() -> impl IntoView {
    let library = expect_context::<Library>();
    let tv = expect_context::<Tv>();
    let focus = Focus::new("lib", true);
    tv.keep_position(focus);
    // Row 0 is the chips: kinds of game, then the order.
    let kind = RwSignal::new(0usize);
    let order = RwSignal::new(0usize);
    let kinds: [&'static str; 6] = [tr!("Todos"), tr!("Favoritos"), tr!("Jogando"), "Recomps", "Steam", "Epic"];
    let orders: [&'static str; 3] = [tr!("Recentes"), tr!("A a Z"), tr!("Mais jogados")];
    let games = Memo::new(move |_| {
        let mut list: Vec<Game> = library_games(library)
            .into_iter()
            .filter(|g| match kind.get() {
                1 => g.favorite,
                2 => g.status == Some(cerberus_shared::GameStatus::Playing),
                3 => matches!(g.source, cerberus_shared::Source::Recomp { .. }),
                4 => matches!(g.source, cerberus_shared::Source::Steam { .. }),
                5 => matches!(g.source, cerberus_shared::Source::Epic { .. }),
                _ => true,
            })
            .collect();
        match order.get() {
            1 => list.sort_by_key(|g| g.name.to_lowercase()),
            2 => list.sort_by_key(|g| std::cmp::Reverse(g.playtime_secs)),
            _ => list.sort_by_key(|g| (std::cmp::Reverse(g.last_played), g.name.to_lowercase())),
        }
        list
    });
    let rows = move || {
        let mut r = vec![kinds.len() + 1];
        r.extend(grid_rows(games.with(Vec::len)));
        r
    };
    Effect::new(move |_| focus.clamp(&rows()));
    let focused = move || {
        let row = focus.row.get();
        (row > 0).then(|| games.with(|g| g.get((row - 1) * COLUMNS + focus.col.get()).cloned())).flatten()
    };
    let choose_chip = move |c: usize| {
        play(Sound::Select);
        if c < kinds.len() {
            kind.set(c);
        } else {
            order.update(|o| *o = (*o + 1) % orders.len());
        }
    };
    Effect::new(move |_| {
        if let Some(g) = focused() {
            tv.show_backdrop(Item::Game(g).heroes());
        }
    });
    tv.hints.set(vec![(Btn::A, tr!("Abrir")), (Btn::Y, tr!("Jogar")), (Btn::X, tr!("Opções"))]);
    tv.handle_page(move |btn| match btn {
        Btn::Up | Btn::Down | Btn::Left | Btn::Right => {
            if focus.step(btn, &rows()) {
                play(Sound::Move);
            }
            true
        }
        Btn::A if focus.row.get_untracked() == 0 => {
            choose_chip(focus.col.get_untracked());
            true
        }
        Btn::A => {
            if let Some(g) = focused() {
                tv.open(View::Game(g.id));
            }
            true
        }
        Btn::Y => {
            if let Some(g) = focused() {
                play(Sound::Select);
                play_game(library, g);
            }
            true
        }
        Btn::X => {
            if let Some(g) = focused() {
                tv.sheet.set(Some(Sheet::Options(g.id)));
                play(Sound::Select);
            }
            true
        }
        _ => false,
    });
    let grid = move || {
        let list = games.get();
        if list.is_empty() {
            return view! { <p class="tv-empty">{tr!("Nenhum jogo com esse filtro.")}</p> }.into_any();
        }
        list.into_iter()
            .enumerate()
            .map(|(i, g)| {
                let (r, c) = (i / COLUMNS + 1, i % COLUMNS);
                tile(Item::Game(g), focus.key(r, c), move || focus.is(r, c))
            })
            .collect_view()
            .into_any()
    };
    view! {
        <div class="tv-page">
            <h1 class="tv-title">{tr!("Biblioteca")}<small>{move || tr!("{} jogos", games.with(Vec::len))}</small></h1>
            <div class="tv-chips">
                {kinds.into_iter().enumerate().map(|(c, label)| view! {
                    <span class="tv-chip" class:sel=move || kind.get() == c class:on=move || focus.is(0, c) data-tvf=focus.key(0, c)
                        on:mouseenter=move |_| focus.set(0, c) on:click=move |_| choose_chip(c)>{label}</span>
                }).collect_view()}
                <span class="tv-chip order" class:on=move || focus.is(0, kinds.len()) data-tvf=focus.key(0, kinds.len())
                    on:mouseenter=move |_| focus.set(0, kinds.len()) on:click=move |_| choose_chip(kinds.len())>
                    {move || tr!("Ordem: {}", orders[order.get()])}
                </span>
            </div>
            <div class="tv-grid">{grid}</div>
        </div>
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Order {
    Popular,
    Newest,
    Name,
}

/// The catalog: search (on-screen keyboard), "you have the game", sort, and a grid.
#[component]
pub fn Discover() -> impl IntoView {
    let library = expect_context::<Library>();
    let tv = expect_context::<Tv>();
    let focus = Focus::new("disc", true);
    tv.keep_position(focus);
    let owned_only = RwSignal::new(false);
    let order = RwSignal::new(Order::Popular);
    // A genre from the catalog ("" for all), changed by A on its chip.
    let genre = RwSignal::new(String::new());
    // The catalog's genres, the most common first.
    let genres = Memo::new(move |_| {
        let mut counts: Vec<(String, usize)> = Vec::new();
        for e in one_each(&catalog(library)).into_iter().map(|(lead, _)| lead) {
            if crate::discover::genre_label(&e.genre).is_none() {
                continue;
            }
            match counts.iter_mut().find(|(g, _)| *g == e.genre) {
                Some((_, n)) => *n += 1,
                None => counts.push((e.genre.clone(), 1)),
            }
        }
        counts.sort_by(|a, b| b.1.cmp(&a.1));
        counts.into_iter().map(|(g, _)| g).collect::<Vec<_>>()
    });
    // The grid grows as the focus nears its end.
    let shown = RwSignal::new(48usize);

    let results = Memo::new(move |_| {
        let needle = tv.search.get().to_lowercase();
        let all = catalog(library);
        let mut list: Vec<RecompEntry> = one_each(&all)
            .into_iter()
            .filter_map(|(lead, _)| {
                if needle.split_whitespace().all(|w| format!("{} {}", lead.name, lead.game).to_lowercase().contains(w)) {
                    return Some(lead);
                }
                // A game shows when any of its versions matches.
                all.iter().find(|e| e.title == lead.title && needle.split_whitespace().all(|w| format!("{} {}", e.name, e.game).to_lowercase().contains(w))).cloned()
            })
            .filter(|e| !owned_only.get() || e.owned_copy.is_some())
            .filter(|e| genre.with(|g| g.is_empty() || e.genre == *g))
            .collect();
        match order.get() {
            Order::Popular => list.sort_by_key(|e| std::cmp::Reverse(e.downloads)),
            Order::Newest => list.sort_by(|a, b| b.added.cmp(&a.added)),
            Order::Name => list.sort_by_key(|e| e.name.to_lowercase()),
        }
        list
    });
    let visible = move || results.with(Vec::len).min(shown.get());
    // Row 0: the chips. Rows below: the grid.
    let rows = move || {
        let mut r = vec![4];
        r.extend(grid_rows(visible()));
        r
    };
    Effect::new(move |_| focus.clamp(&rows()));
    Effect::new(move |_| {
        if focus.row.get() * COLUMNS + 3 * COLUMNS >= shown.get_untracked() {
            shown.update(|n| *n += 36);
        }
    });
    let focused = move || {
        let r = focus.row.get();
        (r > 0).then(|| results.with(|l| l.get((r - 1) * COLUMNS + focus.col.get()).cloned())).flatten()
    };
    Effect::new(move |_| {
        if let Some(e) = focused() {
            tv.show_backdrop(Item::Entry(e).heroes());
        }
    });
    // A on a chip (or a click): search, owned only, the next genre, the next order.
    let press = move |col: usize| {
        play(Sound::Select);
        match col {
            0 => tv.sheet.set(Some(Sheet::Keyboard)),
            1 => owned_only.update(|o| *o = !*o),
            2 => genres.with_untracked(|all| {
                // The next genre, then back to all of them.
                let now = genre.get_untracked();
                let next = match all.iter().position(|g| *g == now) {
                    _ if now.is_empty() => all.first().cloned(),
                    Some(i) => all.get(i + 1).cloned(),
                    None => None,
                };
                genre.set(next.unwrap_or_default());
            }),
            _ => order.update(|o| {
                *o = match o {
                    Order::Popular => Order::Newest,
                    Order::Newest => Order::Name,
                    Order::Name => Order::Popular,
                }
            }),
        }
    };
    tv.hints.set(vec![(Btn::A, tr!("Abrir")), (Btn::Y, tr!("Buscar"))]);
    tv.handle_page(move |btn| match btn {
        Btn::Up | Btn::Down | Btn::Left | Btn::Right => {
            if focus.step(btn, &rows()) {
                play(Sound::Move);
            }
            true
        }
        Btn::A => {
            if focus.row.get_untracked() == 0 {
                press(focus.col.get_untracked());
            } else if let Some(e) = focused() {
                tv.open(View::Game(format!("recomp:{}", e.id)));
            }
            true
        }
        Btn::Y => {
            tv.sheet.set(Some(Sheet::Keyboard));
            play(Sound::Select);
            true
        }
        Btn::X => {
            if let Some(e) = focused() {
                tv.sheet.set(Some(Sheet::Options(format!("recomp:{}", e.id))));
                play(Sound::Select);
            }
            true
        }
        _ => false,
    });

    let chip = move |c: usize, label: Box<dyn Fn() -> String + Send + Sync>| {
        view! { <span class="tv-chip" class:on=move || focus.is(0, c) data-tvf=focus.key(0, c) on:click=move |_| { focus.set(0, c); press(c); }>{move || label()}</span> }
    };
    let grid = move || {
        let n = visible();
        results.with(|list| {
            list.iter()
                .take(n)
                .cloned()
                .enumerate()
                .map(|(i, e)| {
                    let (r, c) = (i / COLUMNS + 1, i % COLUMNS);
                    tile(Item::Entry(e), focus.key(r, c), move || focus.is(r, c))
                })
                .collect_view()
        })
    };
    view! {
        <div class="tv-page">
            <h1 class="tv-title">{tr!("Descobrir")}<small>{move || tr!("{} jogos", results.with(Vec::len))}</small></h1>
            <div class="tv-chips">
                {chip(0, Box::new(move || { let q = tv.search.get(); if q.is_empty() { tr!("Buscar").to_string() } else { tr!("Buscar: {}", q) } }))}
                {chip(1, Box::new(move || if owned_only.get() { tr!("✓ Tenho o jogo").to_string() } else { tr!("Tenho o jogo").to_string() }))}
                {chip(2, Box::new(move || genre.with(|g| crate::discover::genre_label(g).map_or_else(|| tr!("Gênero: todos").to_string(), |l| tr!("Gênero: {}", l)))))}
                {chip(3, Box::new(move || match order.get() { Order::Popular => tr!("Ordem: populares"), Order::Newest => tr!("Ordem: novos"), Order::Name => tr!("Ordem: A a Z") }.to_string()))}
            </div>
            <div class="tv-grid">{grid}</div>
            {move || results.with(Vec::is_empty).then(|| view! { <p class="tv-empty">{tr!("Nenhum jogo encontrado.")}</p> })}
        </div>
    }
}

/// Downloads running now, and updates ready for installed recomps.
#[component]
pub fn Downloads() -> impl IntoView {
    let library = expect_context::<Library>();
    let tv = expect_context::<Tv>();
    let focus = Focus::new("dl", true);
    tv.keep_position(focus);
    let entries = Memo::new(move |_| catalog(library));
    let active = Memo::new(move |_| {
        let ids: Vec<String> = library.progress.with(|p| p.keys().cloned().collect());
        entries.with(|all| ids.iter().filter_map(|id| all.iter().find(|e| e.id == *id).cloned()).collect::<Vec<_>>())
    });
    let updates = Memo::new(move |_| {
        let games = library_games(library);
        entries.with(|all| {
            all.iter()
                .filter(|e| e.installed.is_some() && e.latest.as_ref().is_some_and(|l| Some(&l.version) != e.installed.as_ref()))
                .filter(|e| !games.iter().any(|g| g.external && matches!(&g.source, Source::Recomp { recomp_id, .. } if *recomp_id == e.id)))
                .filter(|e| !library.progress.with(|p| p.contains_key(&e.id)))
                .cloned()
                .collect::<Vec<_>>()
        })
    });
    let list = move || {
        let mut all: Vec<(RecompEntry, bool)> = active.get().into_iter().map(|e| (e, true)).collect();
        all.extend(updates.get().into_iter().map(|e| (e, false)));
        all
    };
    let rows = move || vec![1; list().len()];
    Effect::new(move |_| focus.clamp(&rows()));
    tv.hints.set(vec![(Btn::A, tr!("Abrir / atualizar"))]);
    tv.handle_page(move |btn| match btn {
        Btn::Up | Btn::Down => {
            if focus.step(btn, &rows()) {
                play(Sound::Move);
            }
            true
        }
        Btn::A => {
            if let Some((e, running)) = list().get(focus.row.get_untracked()).cloned() {
                if running {
                    tv.open(View::Game(format!("recomp:{}", e.id)));
                } else {
                    play(Sound::Select);
                    start_install(library, e.id);
                }
            }
            true
        }
        _ => false,
    });
    let body = move || {
        let items = list();
        if items.is_empty() {
            return view! { <p class="tv-empty">{tr!("Nada baixando agora e nenhuma atualização pendente.")}</p> }.into_any();
        }
        items
            .into_iter()
            .enumerate()
            .map(|(r, (e, running))| {
                let rid = e.id.clone();
                let from_to = format!("{} → {}", e.installed.clone().unwrap_or_default(), e.latest.as_ref().map(|l| l.version.clone()).unwrap_or_default());
                let detail = move || {
                    if running {
                        match library.progress.with(|p| p.get(&rid).cloned()) {
                            Some(InstallStep::Downloading { done, total }) if total > 0 => tr!("Baixando · {} de {}", format::megabytes(done), format::megabytes(total)),
                            Some(InstallStep::Unpacking) => tr!("Preparando arquivos").to_string(),
                            Some(InstallStep::Paused { done, total }) => tr!("Pausado · {} de {}", format::megabytes(done), format::megabytes(total)),
                            Some(InstallStep::Failed { message }) => tr!("Falhou · {}", message),
                            _ => tr!("Começando").to_string(),
                        }
                    } else {
                        from_to.clone()
                    }
                };
                let rid2 = e.id.clone();
                let bar = move || running.then(|| progress_of(library, &rid2)).flatten().map(|p| view! { <div class="tv-bar wide"><i style=format!("width:{p}%")></i></div> });
                view! {
                    <div class="tv-line" class:on=move || focus.is(r, 0) data-tvf=focus.key(r, 0)>
                        <div class="tv-line-cover"><Poster sources=recomp_covers(&e) title=e.name.clone() /></div>
                        <div class="tv-line-text"><b>{e.name.clone()}</b><span>{detail}</span>{bar}</div>
                        <span class="tv-line-act">{if running { tr!("Abrir") } else { tr!("Atualizar") }}</span>
                    </div>
                }
            })
            .collect_view()
            .into_any()
    };
    view! {
        <div class="tv-page">
            <h1 class="tv-title">{tr!("Downloads")}</h1>
            <div class="tv-list">{body}</div>
        </div>
    }
}

/// A few choices that matter on a TV.
#[component]
pub fn Settings() -> impl IntoView {
    let library = expect_context::<Library>();
    let tv = expect_context::<Tv>();
    let focus = Focus::new("set", true);
    tv.keep_position(focus);
    let start_in_tv = RwSignal::new(false);
    let sounds = RwSignal::new(super::sound::enabled());
    spawn_local(async move {
        if let Ok(s) = api::get_settings().await {
            start_in_tv.set(s.start_in_tv);
        }
    });
    const ROWS: usize = 4;
    tv.hints.set(vec![(Btn::A, tr!("Escolher"))]);
    tv.handle_page(move |btn| match btn {
        Btn::Up | Btn::Down => {
            if focus.step(btn, &[1; ROWS]) {
                play(Sound::Move);
            }
            true
        }
        Btn::A | Btn::Left | Btn::Right => {
            play(Sound::Select);
            match focus.row.get_untracked() {
                0 => {
                    let on = !start_in_tv.get_untracked();
                    spawn_local(async move {
                        if library.report(api::set_start_in_tv(on).await).is_some() {
                            start_in_tv.set(on);
                        }
                    });
                }
                1 => {
                    let on = !sounds.get_untracked();
                    super::sound::set_enabled(on);
                    sounds.set(on);
                }
                2 if btn == Btn::A => library.go(crate::Route::Home),
                3 if btn == Btn::A => api::window_action("close"),
                _ => {}
            }
            true
        }
        _ => false,
    });
    let row = move |r: usize, name: &'static str, about: &'static str, value: Box<dyn Fn() -> String + Send + Sync>| {
        view! {
            <div class="tv-line setting" class:on=move || focus.is(r, 0) data-tvf=focus.key(r, 0)>
                <div class="tv-line-text"><b>{name}</b><span>{about}</span></div>
                <span class="tv-line-act">{move || value()}</span>
            </div>
        }
    };
    let on_off = |s: RwSignal<bool>| -> Box<dyn Fn() -> String + Send + Sync> { Box::new(move || if s.get() { "◀ Ligado ▶".into() } else { "◀ Desligado ▶".into() }) };
    view! {
        <div class="tv-page">
            <h1 class="tv-title">{tr!("Configurações")}</h1>
            <div class="tv-list">
                {row(0, tr!("Abrir o Cerberus no Big Picture Mode"), tr!("Toda vez que o Cerberus abrir, em tela cheia para o controle."), on_off(start_in_tv))}
                {row(1, tr!("Sons de navegação"), tr!("Um toque curto ao mover, escolher e voltar."), on_off(sounds))}
                {row(2, tr!("Sair do Big Picture Mode"), tr!("Volta para a janela normal. Segurar o botão Menu também sai."), Box::new(|| tr!("Sair").into()))}
                {row(3, tr!("Fechar o Cerberus"), tr!("Fecha o launcher."), Box::new(|| tr!("Fechar").into()))}
            </div>
            <p class="tv-note">{move || tr!("Controle: {}", match tv.pad.get() { super::PadKind::Xbox => tr!("Xbox (ou compatível)"), super::PadKind::PlayStation => "PlayStation", super::PadKind::Nintendo => tr!("Nintendo") })}</p>
        </div>
    }
    .into_any()
}
