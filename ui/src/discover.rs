//! Discover: every recomp in the catalog, plus the install flow shared with the game page.

use cerberus_shared::{InstallStep, RecompEntry};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::icons::{Icon, Svg};
use crate::tiles::{Tile, recomp_tile};
use crate::versions::one_each;
use crate::{Library, api, format::megabytes};

/// Starts installing (or updating) a recomp; progress fills in from the backend's events.
pub fn start_install(library: Library, id: String) {
    let starting = InstallStep::Downloading { done: 0, total: 0 };
    library.progress.update(|all| {
        all.insert(id.clone(), starting);
    });
    spawn_local(async move {
        let result = api::install_recomp(&id).await;
        library.progress.update(|all| {
            all.remove(&id);
        });
        library.games.refetch();
        library.recomps.refetch();
        if let Some(game) = library.report(result) {
            library.toast(format!("{} está pronto para jogar.", game.name));
        }
    });
}

pub fn cancel_install(library: Library, id: String) {
    spawn_local(async move {
        library.report(api::cancel_install(&id).await);
    });
}

/// A progress bar with what is happening, and a cancel button while downloading.
pub fn progress_view(step: InstallStep, recomp_id: String) -> impl IntoView {
    let library = expect_context::<Library>();
    // Only a download stops cleanly; unpacking is quick and half-unpacked files help no one.
    let cancellable = matches!(step, InstallStep::Downloading { .. });
    let (label, percent) = match step {
        InstallStep::Downloading { done, total } if total > 0 => {
            let percent = done * 100 / total;
            (format!("Baixando {percent}% · {} de {}", megabytes(done), megabytes(total)), percent)
        }
        InstallStep::Downloading { .. } => ("Preparando...".to_string(), 0),
        InstallStep::Unpacking => ("Instalando...".to_string(), 100),
        InstallStep::Finished => ("Pronto".to_string(), 100),
        InstallStep::Failed { .. } => ("Falhou".to_string(), 0),
    };
    let cancel = cancellable.then(|| {
        view! {
            <button type="button" class="btn" on:click=move |_| cancel_install(library, recomp_id.clone())>"Cancelar"</button>
        }
    });
    view! {
        <div class="progress-row">
            <div class="progress" role="progressbar" aria-valuenow=percent aria-valuemin="0" aria-valuemax="100">
                <div class="bar" style:width=format!("{percent}%")></div>
                <span>{label}</span>
            </div>
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

fn matches_search(entry: &RecompEntry, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let haystack = format!("{} {} {}", entry.name, entry.game, entry.author).to_lowercase();
    needle.split_whitespace().all(|word| haystack.contains(word))
}

#[component]
pub fn DiscoverPage() -> impl IntoView {
    let library = expect_context::<Library>();
    let search = RwSignal::new(String::new());
    let console = RwSignal::new(String::new());
    let sort = RwSignal::new(Sort::Popular);
    let only_owned = RwSignal::new(false);
    let shown = RwSignal::new(PAGE);
    // A new search or filter starts from the top of the list again.
    Effect::new(move |_| {
        search.track();
        console.track();
        sort.track();
        only_owned.track();
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

    let filtered = move || {
        let needle = search.get().to_lowercase();
        let wanted_console = console.get();
        let mut games: Vec<(RecompEntry, usize)> = one_each(&entries())
            .into_iter()
            .filter(|(e, _)| wanted_console.is_empty() || e.origin == wanted_console)
            .filter(|(e, _)| !only_owned.get() || e.owned_copy.is_some())
            .filter(|(e, _)| matches_search(e, &needle))
            .collect();
        sort_games(&mut games, sort.get());
        games
    };

    let grid = move || {
        if library.recomps.get().is_none() {
            return view! { <p class="muted">"Carregando o catálogo..."</p> }.into_any();
        }
        if let Some(Err(error)) = library.recomps.get() {
            return view! { <p class="problem">{error}</p> }.into_any();
        }
        let games = filtered();
        let total = games.len();
        if total == 0 {
            return view! {
                <div class="empty">
                    <h2>"Nada encontrado"</h2>
                    <p class="muted">"Tente outro nome ou tire os filtros."</p>
                </div>
            }
            .into_any();
        }
        let progress = library.progress.get();
        let tiles = games
            .into_iter()
            .take(shown.get())
            .map(|(entry, versions)| {
                let mut info = recomp_tile(&entry, progress.get(&entry.id));
                info.sub = match (&entry.latest, &entry.problem) {
                    _ if versions > 1 => format!("{} · {versions} versões", entry.console),
                    (Some(release), _) if release.size > 0 => format!("{} · {}", entry.console, megabytes(release.size)),
                    (None, Some(_)) => format!("{} · Indisponível", entry.console),
                    _ => entry.console.clone(),
                };
                if entry.owned_copy.is_some() && info.badge.is_none() {
                    info.badge = Some("Você tem".to_string());
                }
                view! { <Tile info /> }
            })
            .collect_view();
        let more = (total > shown.get()).then(|| {
            view! {
                <div class="more">
                    <button type="button" class="btn" on:click=move |_| shown.update(|n| *n += PAGE)>
                        {format!("Mostrar mais ({} de {total})", shown.get())}
                    </button>
                </div>
            }
        });
        view! { <div class="grid">{tiles}</div>{more} }.into_any()
    };

    let console_chips = move || {
        let all = consoles();
        let top: Vec<_> = all.into_iter().take(9).collect();
        let chip = move |id: String, label: String| {
            let value = id.clone();
            view! {
                <button type="button" class="chip-btn" class:on=move || console.get() == id on:click=move |_| console.set(value.clone())>
                    {label}
                </button>
            }
        };
        let mut chips = vec![chip(String::new(), "Todos".to_string()).into_any()];
        chips.extend(top.into_iter().map(|(id, name, n)| chip(id, format!("{name} · {n}")).into_any()));
        chips
    };

    let count = move || one_each(&entries()).len();

    view! {
        <div class="page">
            <header class="page-h">
                <div>
                    <h1>"Descobrir"</h1>
                    <p class="muted">
                        {move || format!("{} jogos com versão nativa para PC. ", count())}
                        "Você precisa da sua própria cópia do jogo. Catálogo: recomplauncher.com."
                    </p>
                </div>
            </header>
            <div class="toolbar">
                <label class="search">
                    <Svg icon=Icon::Search size=18 />
                    <input
                        type="search"
                        placeholder="Buscar jogo, recomp ou autor"
                        prop:value=move || search.get()
                        on:input=move |e| search.set(event_target_value(&e))
                    />
                </label>
                <button type="button" class="chip-btn" class:on=move || only_owned.get() on:click=move |_| only_owned.update(|o| *o = !*o)>
                    "Tenho o jogo"
                </button>
                <select
                    class="select"
                    aria-label="Ordenar"
                    on:change=move |e| sort.set(match event_target_value(&e).as_str() {
                        "new" => Sort::Newest,
                        "updated" => Sort::Updated,
                        "name" => Sort::Name,
                        _ => Sort::Popular,
                    })
                >
                    <option value="popular">"Populares"</option>
                    <option value="new">"Adicionados recentemente"</option>
                    <option value="updated">"Atualizados recentemente"</option>
                    <option value="name">"A a Z"</option>
                </select>
            </div>
            <div class="chips">{console_chips}</div>
            {grid}
        </div>
    }
}
