//! Discover: every recomp in the catalog, plus the install flow shared with the game page.

use cerberus_shared::tr;
use cerberus_shared::{InstallStep, RecompEntry};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::icons::{Icon, Svg};
use crate::tiles::{Tile, recomp_tile};
use crate::versions::one_each;
use crate::{Library, api, format::megabytes};

/// Starts installing (or updating) a recomp; progress fills in from the backend's events.
pub fn start_install(library: Library, id: String) {
    spawn_local(install_one(library, id));
}

/// Installs one recomp and waits for it: for "Update all", one after the other.
pub async fn install_one(library: Library, id: String) {
    library.failed.update(|all| {
        all.remove(&id);
    });
    library.speed.update(|all| {
        all.remove(&id);
    });
    library.progress.update(|all| {
        // Carrying on from a pause starts where the bar stopped.
        let starting = match all.get(&id) {
            Some(InstallStep::Paused { done, total }) => InstallStep::Downloading { done: *done, total: *total },
            _ => InstallStep::Downloading { done: 0, total: 0 },
        };
        all.insert(id.clone(), starting);
    });
    let result = api::install_recomp(&id).await;
    // Paused: the bar stays, showing where it stopped, until it carries on.
    if result.as_ref().is_err_and(|e| e == cerberus_shared::PAUSED) {
        return;
    }
    library.progress.update(|all| {
        all.remove(&id);
    });
    library.speed.update(|all| {
        all.remove(&id);
    });
    // Kept for the Downloads page's "Didn't finish"; a cancel isn't a failure.
    if let Err(e) = &result {
        let cancelled = e.as_str() == cerberus_shared::CANCELLED_INSTALL || e.as_str() == cerberus_shared::i18n::t(cerberus_shared::CANCELLED_INSTALL);
        if !cancelled {
            library.failed.update(|all| {
                all.insert(id.clone(), e.clone());
            });
        }
    }
    library.games.refetch();
    library.recomps.refetch();
    if let Some(game) = library.report(result) {
        if game.setup_pending {
            library.toast(tr!("Download de {} concluído. Abra o instalador na página do jogo.", game.name));
        } else {
            library.toast(tr!("{} foi instalado.", game.name));
        }
    }
}

pub fn cancel_install(library: Library, id: String) {
    // A paused download has no install running to report back: clear its bar here.
    if is_paused(library, &id) {
        library.progress.update(|all| {
            all.remove(&id);
        });
    }
    spawn_local(async move {
        library.report(api::cancel_install(&id).await);
    });
}

pub fn pause_install(library: Library, id: String) {
    spawn_local(async move {
        library.report(api::pause_install(&id).await);
    });
}

pub fn is_paused(library: Library, id: &str) -> bool {
    library.progress.with(|all| matches!(all.get(id), Some(InstallStep::Paused { .. })))
}

/// A progress bar with what is happening, and pause or cancel while downloading.
pub fn progress_view(step: InstallStep, recomp_id: String) -> impl IntoView {
    let library = expect_context::<Library>();
    // Only a download stops cleanly; unpacking is quick and half-unpacked files help no one.
    let cancellable = matches!(step, InstallStep::Downloading { .. } | InstallStep::Paused { .. });
    let paused = matches!(step, InstallStep::Paused { .. });
    // Pausing before the first byte would keep nothing worth carrying on from.
    let pausable = matches!(step, InstallStep::Downloading { done, .. } if done > 0);
    let (label, percent) = match step {
        InstallStep::Downloading { done, total } if total > 0 => {
            let percent = u128::from(done.min(total)) * 100 / u128::from(total);
            (tr!("Baixando {}% · {} de {}", percent, megabytes(done), megabytes(total)), Some(percent))
        }
        InstallStep::Downloading { done, .. } if done > 0 => (tr!("Baixando · {}", megabytes(done)), None),
        InstallStep::Downloading { .. } => (tr!("Iniciando download…").into(), None),
        InstallStep::Paused { done, total } if total > 0 => {
            let percent = u128::from(done.min(total)) * 100 / u128::from(total);
            (tr!("Pausado em {}% · {} de {}", percent, megabytes(done), megabytes(total)), Some(percent))
        }
        InstallStep::Paused { done, .. } => (tr!("Pausado · {}", megabytes(done)), None),
        InstallStep::Unpacking => (tr!("Preparando arquivos…").into(), None),
        InstallStep::Finished => (tr!("Concluído").into(), Some(100)),
        InstallStep::Failed { message } => (tr!("Falha na instalação · {}", cerberus_shared::i18n::translate(&message)), Some(0)),
    };
    let (pause_id, resume_id) = (recomp_id.clone(), recomp_id.clone());
    let pause = pausable.then(|| {
        view! { <button type="button" class="btn" on:click=move |_| pause_install(library, pause_id.clone())>{tr!("Pausar")}</button> }
    });
    let resume = paused.then(|| {
        view! { <button type="button" class="btn primary" on:click=move |_| start_install(library, resume_id.clone())>{tr!("Continuar")}</button> }
    });
    let cancel = cancellable.then(|| {
        view! {
            <button type="button" class="btn" on:click=move |_| cancel_install(library, recomp_id.clone())>{tr!("Cancelar")}</button>
        }
    });
    view! {
        <div class="progress-row">
            <label class="install-progress">
                <span>{label}</span>
                <progress max="100" value=percent.map(|n| n.to_string())></progress>
            </label>
            {resume}
            {pause}
            {cancel}
        </div>
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Sort {
    Popular,
    Newest,
    Updated,
    Name,
}

/// How many tiles show before "Mostrar mais": enough to fill a big window,
/// few enough that the page stays quick with 1500 games.
const PAGE: usize = 60;

fn sort_games(games: &mut [(RecompEntry, usize)], sort: Sort) {
    match sort {
        Sort::Popular => games.sort_by_key(|(e, _)| std::cmp::Reverse(e.downloads)),
        Sort::Newest => games.sort_by(|(a, _), (b, _)| b.added.cmp(&a.added)),
        Sort::Updated => games.sort_by(|(a, _), (b, _)| b.released.cmp(&a.released)),
        Sort::Name => games.sort_by_key(|(e, _)| e.name.to_lowercase()),
    }
}

const BY_GAME_KEY: &str = "cerberus.discover.by-game";

/// Whether Discover lists games by the original game's name, remembered per PC.
fn by_game_saved() -> bool {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(BY_GAME_KEY).ok().flatten())
        .is_some_and(|v| v == "1")
}

fn save_by_game(on: bool) {
    if let Some(s) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = s.set_item(BY_GAME_KEY, if on { "1" } else { "0" });
    }
}

fn matches_search(entry: &RecompEntry, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let haystack = format!("{} {} {}", entry.name, entry.game, entry.author).to_lowercase();
    needle.split_whitespace().all(|word| haystack.contains(word))
}

/// The catalog's genres, in the player's words. "other" isn't offered.
pub fn genre_label(id: &str) -> Option<&'static str> {
    Some(match id {
        "action" => tr!("Ação"),
        "platformer" => tr!("Plataformas"),
        "shooter" => tr!("Tiro"),
        "rpg" => "RPG",
        "racing" => tr!("Corrida"),
        "fighting" => tr!("Luta"),
        "adventure" => tr!("Aventura"),
        "sports" => tr!("Esportes"),
        "puzzle" => tr!("Quebra-cabeça"),
        _ => return None,
    })
}

/// What a recomp adds, as the catalog tags it (several tags mean the same thing).
// Tags come in Portuguese (the catalog is kept in it) or back in English, so both are listed.
const FEATURES: [(&str, &[&str]); 7] = [
    ("controller", &["Controller", "Controller Support", "Controle", "Suporte a controle"]),
    ("achievements", &["Achievements", "Conquistas"]),
    ("coop", &["Local Co-op", "Split Screen", "Multiplayer", "Co-op", "Cooperativo local", "Tela dividida", "Multijogador", "Cooperativo"]),
    ("online", &["Online", "Netplay", "Jogo em rede"]),
    ("fps", &["High FPS", "60 FPS", "FPS alto"]),
    ("wide", &["Widescreen", "Ultrawide", "Ultrawide experimental", "Experimental ultrawide"]),
    ("mods", &["Mods", "Mod support", "Suporte a mods"]),
];

fn feature_label(id: &str) -> &'static str {
    match id {
        "controller" => tr!("Suporte a controle"),
        "achievements" => tr!("Conquistas"),
        "coop" => tr!("Multijogador local"),
        "online" => tr!("Online"),
        "fps" => tr!("Mais FPS"),
        "wide" => tr!("Tela larga"),
        "mods" => tr!("Mods"),
        _ => "",
    }
}

fn has_feature(entry: &RecompEntry, id: &str) -> bool {
    FEATURES.iter().find(|(f, _)| *f == id).is_some_and(|(_, tags)| entry.tags.iter().any(|t| tags.iter().any(|x| x.eq_ignore_ascii_case(t))))
}

#[component]
pub fn DiscoverPage() -> impl IntoView {
    let library = expect_context::<Library>();
    let search = RwSignal::new(String::new());
    let console = RwSignal::new(String::new());
    let sort = RwSignal::new(Sort::Popular);
    let only_owned = RwSignal::new(false);
    let genre = RwSignal::new(String::new());
    let feature = RwSignal::new(String::new());
    // Recomps by their own names ("Zelda 64: Recompiled"), or by the game they bring back ("Majora's Mask").
    let by_game = RwSignal::new(by_game_saved());
    let shown = RwSignal::new(PAGE);
    // A new search or filter starts from the top of the list again.
    Effect::new(move |_| {
        search.track();
        console.track();
        sort.track();
        only_owned.track();
        genre.track();
        feature.track();
        shown.set(PAGE);
    });

    let entries = move || library.recomps.get().and_then(Result::ok).unwrap_or_default();

    // The consoles with recomps, most recomps first, for the filter.
    let consoles = move || {
        let mut counts: Vec<(String, String, usize)> = Vec::new();
        for (entry, _) in one_each(&entries()) {
            match counts.iter_mut().find(|(id, _, _)| *id == entry.origin) {
                Some(found) => found.2 += 1,
                None => counts.push((entry.origin.clone(), entry.console.clone(), 1)),
            }
        }
        counts.sort_by_key(|(_, _, n)| std::cmp::Reverse(*n));
        counts
    };
    // Genres with recomps on the chosen platform, most first.
    let genres = move || {
        let wanted_console = console.get();
        let mut counts: Vec<(String, usize)> = Vec::new();
        for (entry, _) in one_each(&entries()) {
            if genre_label(&entry.genre).is_none() || !(wanted_console.is_empty() || entry.origin == wanted_console) {
                continue;
            }
            match counts.iter_mut().find(|(g, _)| *g == entry.genre) {
                Some(found) => found.1 += 1,
                None => counts.push((entry.genre.clone(), 1)),
            }
        }
        counts.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        counts
    };

    let filtered = move || {
        let needle = search.get().to_lowercase();
        let wanted_console = console.get();
        let all = entries();
        // A game shows when any of its versions matches the search; the tile
        // then shows that version ("Zelda 64: Recompiled" under its game).
        let mut games: Vec<(RecompEntry, usize)> = one_each(&all)
            .into_iter()
            .filter(|(e, _)| wanted_console.is_empty() || e.origin == wanted_console)
            .filter(|(e, _)| !only_owned.get() || e.owned_copy.is_some())
            .filter(|(e, _)| genre.with(|g| g.is_empty() || e.genre == *g))
            .filter(|(e, _)| feature.with(|f| f.is_empty() || has_feature(e, f)))
            .filter_map(|(lead, n)| {
                if matches_search(&lead, &needle) {
                    return Some((lead, n));
                }
                let hit = all.iter().find(|e| e.title == lead.title && matches_search(e, &needle))?;
                Some((hit.clone(), n))
            })
            .collect();
        sort_games(&mut games, sort.get());
        if by_game.get() && sort.get() == Sort::Name {
            games.sort_by_key(|(e, _)| if e.game.is_empty() { e.name.to_lowercase() } else { e.game.to_lowercase() });
        }
        games
    };

    let has_filters = move || !search.get().is_empty() || !console.get().is_empty() || only_owned.get() || !genre.get().is_empty() || !feature.get().is_empty();
    let clear = move |_| {
        search.set(String::new());
        console.set(String::new());
        only_owned.set(false);
        genre.set(String::new());
        feature.set(String::new());
    };

    let grid = move || {
        if library.recomps.get().is_none() {
            return view! { <p class="muted">{tr!("Carregando o catálogo...")}</p> }.into_any();
        }
        if let Some(Err(error)) = library.recomps.get() {
            return view! { <p class="problem">{error}</p> }.into_any();
        }
        let games = filtered();
        let total = games.len();
        if total == 0 {
            return view! {
                <div class="empty">
                    <Svg icon=Icon::Search size=32 />
                    <h2>{tr!("Nenhum jogo encontrado")}</h2>
                    <p class="muted">{tr!("Tente outro nome ou tire os filtros.")}</p>
                </div>
            }
            .into_any();
        }
        let tiles = games
            .into_iter()
            .take(shown.get())
            .map(|(entry, versions)| {
                let mut info = recomp_tile(&entry);
                if by_game.get() && !entry.game.is_empty() {
                    info.title = entry.game.clone();
                }
                info.sub = match (&entry.latest, &entry.problem) {
                    _ if by_game.get() => format!("{} · {}", entry.console, if versions > 1 { format!("{versions} recomps") } else { entry.name.clone() }),
                    _ if versions > 1 => tr!("{} · {} versões", entry.console, versions),
                    (Some(release), _) if release.size > 0 => format!("{} · {}", entry.console, megabytes(release.size)),
                    (None, Some(_)) => tr!("{} · Indisponível", entry.console),
                    _ => entry.console.clone(),
                };
                if entry.owned_copy.is_some() && info.badge.is_none() {
                    info.badge = Some(tr!("Você tem").to_string());
                }
                view! { <Tile info /> }
            })
            .collect_view();
        let more = (total > shown.get()).then(|| {
            view! {
                <div class="more">
                    <button type="button" class="btn" on:click=move |_| shown.update(|n| *n += PAGE)>
                        {tr!("Mostrar mais ({} de {})", shown.get(), total)}
                    </button>
                </div>
            }
        });
        view! {
            <section class="catalog-results">
                <h2>{if total == 1 { tr!("1 jogo").into() } else { tr!("{} jogos", total) }}</h2>
                <div class="grid">{tiles}</div>
                {more}
            </section>
        }.into_any()
    };

    view! {
        <div class="page">
            <header class="page-h">
                <div>
                    <h1>{tr!("Descobrir")}</h1>
                    <p class="muted">
                        {tr!("Jogue no PC com sua cópia original. Catálogo de recomplauncher.com.")}
                    </p>
                </div>
                <div class="row">
                <button type="button" class="btn" title=tr!("Recomps que você instalou sem o Cerberus")
                    on:click=move |_| crate::adopt::start(library)>
                    <Svg icon=Icon::Folder size=18 />{tr!("Trazer seus recomps")}
                </button>
                <button type="button" class="btn" title=tr!("Sugerir um projeto para o catálogo")
                    on:click=move |_| spawn_local(async move { library.report(api::open_recomp_request().await); })>
                    <Svg icon=Icon::Plus size=18 />{tr!("Pedir um recomp")}
                </button>
                </div>
            </header>
            <section class="catalog-filters" aria-label=tr!("Filtros do catálogo")>
                <div class="toolbar">
                    <label class="search">
                        <Svg icon=Icon::Search size=18 />
                        <input
                            type="search"
                            aria-label=tr!("Buscar jogo, recomp ou autor")
                            placeholder=tr!("Buscar jogo, recomp ou autor")
                            prop:value=move || search.get()
                            on:input=move |e| search.set(event_target_value(&e))
                        />
                    </label>
                    <div class="segs" role="radiogroup" aria-label=tr!("Mostrar por")>
                        <button type="button" class="seg" role="radio" class:on=move || !by_game.get() aria-checked=move || (!by_game.get()).to_string()
                            on:click=move |_| { by_game.set(false); save_by_game(false); }>{tr!("Recomps")}</button>
                        <button type="button" class="seg" role="radio" class:on=move || by_game.get() aria-checked=move || by_game.get().to_string()
                            on:click=move |_| { by_game.set(true); save_by_game(true); }>{tr!("Jogos")}</button>
                    </div>
                    <button type="button" class="chip-btn" aria-pressed=move || only_owned.get().to_string() class:on=move || only_owned.get() on:click=move |_| only_owned.update(|o| *o = !*o)>
                        {tr!("Tenho o jogo")}
                    </button>
                    <select
                        class="select"
                        aria-label=tr!("Ordenar")
                        on:change=move |e| sort.set(match event_target_value(&e).as_str() {
                            "new" => Sort::Newest,
                            "updated" => Sort::Updated,
                            "name" => Sort::Name,
                            _ => Sort::Popular,
                        })
                    >
                        <option value="popular">{tr!("Populares")}</option>
                        <option value="new">{tr!("Adicionados recentemente")}</option>
                        <option value="updated">{tr!("Atualizados recentemente")}</option>
                        <option value="name">{tr!("A a Z")}</option>
                    </select>
                </div>
                <div class="catalog-platform">
                    <label for="catalog-console">{tr!("Plataforma")}</label>
                    <select id="catalog-console" class="select" prop:value=move || console.get()
                        on:change=move |e| console.set(event_target_value(&e))>
                        <option value="">{tr!("Todas as plataformas")}</option>
                        {move || consoles().into_iter().map(|(id, name, n)| view! {
                            <option value=id>{format!("{name} ({n})")}</option>
                        }).collect_view()}
                    </select>
                    <label for="catalog-feature">{tr!("Recurso")}</label>
                    <select id="catalog-feature" class="select" prop:value=move || feature.get()
                        on:change=move |e| feature.set(event_target_value(&e))>
                        <option value="">{tr!("Qualquer recurso")}</option>
                        {FEATURES.iter().map(|(id, _)| view! { <option value=*id>{feature_label(id)}</option> }).collect_view()}
                    </select>
                    <Show when=has_filters>
                        <button type="button" class="link" on:click=clear>{tr!("Limpar filtros")}</button>
                    </Show>
                </div>
                <div class="genre-chips" role="radiogroup" aria-label=tr!("Gênero")>
                    <button type="button" class="gchip" role="radio" class:on=move || genre.with(String::is_empty)
                        aria-checked=move || genre.with(String::is_empty).to_string() on:click=move |_| genre.set(String::new())>{tr!("Todos os gêneros")}</button>
                    {move || genres().into_iter().map(|(id, n)| {
                        let mine = id.clone();
                        let pick = id.clone();
                        let on = move || genre.with(|g| *g == mine);
                        view! {
                            <button type="button" class="gchip" role="radio" class:on=on.clone() aria-checked=move || on().to_string()
                                on:click=move |_| genre.update(|g| *g = if *g == pick { String::new() } else { pick.clone() })>
                                {genre_label(&id).unwrap_or_default()}<small>{n}</small>
                            </button>
                        }
                    }).collect_view()}
                </div>
            </section>
            {grid}
        </div>
    }
}
