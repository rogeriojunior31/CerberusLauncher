//! Game tiles (poster, title, one line under it) and shelves of them.

use cerberus_shared::tr;
use cerberus_shared::{Game, InstallStep, RecompEntry, Source};
use leptos::html;
use leptos::prelude::*;

use crate::art::{Poster, cover_sources, recomp_covers};
use crate::format;
use crate::icons::{Icon, Svg};
use crate::{Library, Route};

#[derive(Clone, PartialEq)]
pub struct TileInfo {
    pub title: String,
    pub sub: String,
    pub covers: Vec<String>,
    /// A corner chip: "Atualização", "Jogando"...
    pub badge: Option<String>,
    /// The player's favorite: a heart on the cover.
    pub favorite: bool,
    /// The library game behind the tile, for its right-click menu.
    pub game: Option<Game>,
    pub open: Route,
    /// Catalog tiles follow installation events without rebuilding the surrounding shelf.
    recomp_id: Option<String>,
    /// Owned but not on this PC: the cover is dimmed.
    pub absent: bool,
    /// Achievements unlocked and in all, for a bar on the cover.
    pub progress: Option<(u32, u32)>,
}

pub fn source_name(source: &Source) -> &'static str {
    match source {
        Source::Steam { .. } => tr!("Steam"),
        Source::Epic { .. } => tr!("Epic Games"),
        Source::Local { .. } => tr!("Adicionado à mão"),
        Source::Recomp { .. } => tr!("Recomp"),
    }
}

/// Whether a newer release is out for an installed recomp.
fn has_update(game: &Game, library: Library) -> bool {
    let Source::Recomp { recomp_id, version, .. } = &game.source else {
        return false;
    };
    if game.external || game.updates_paused {
        return false;
    }
    library.recomps.with_untracked(|list| {
        let Some(Ok(entries)) = list else {
            return false;
        };
        let entry = entries.iter().find(|e| e.id == *recomp_id);
        entry.and_then(|e| e.latest.as_ref()).is_some_and(|latest| latest.version != *version)
    })
}

/// A recomp's game and version: the original game's name players know
/// ("Mario Kart 64"), and the recomp's own ("SpaghettiKart") when it differs.
pub fn names_of(name: &str, entry: Option<&RecompEntry>) -> (String, Option<String>) {
    let Some(entry) = entry.filter(|e| !e.game.trim().is_empty()) else { return (name.to_string(), None) };
    let game = entry.game.trim().to_string();
    let version = (!name.trim().eq_ignore_ascii_case(&game)).then(|| name.to_string());
    (game, version)
}

/// The names a library game shows: the original game's first, the recomp after.
pub fn game_names(game: &Game, library: Library) -> (String, Option<String>) {
    let Source::Recomp { recomp_id, .. } = &game.source else { return (game.name.clone(), None) };
    // Tracked: lists drawn before the catalog arrives show the game's name once it does.
    library.recomps.with(|r| {
        let entry = r.as_ref().and_then(|r| r.as_ref().ok()).and_then(|all| all.iter().find(|e| e.id == *recomp_id));
        names_of(&game.name, entry)
    })
}

/// A recomp of the catalog: its game first, the recomp after.
pub fn entry_names(entry: &RecompEntry) -> (String, Option<String>) {
    names_of(&entry.name, Some(entry))
}

pub fn game_tile(game: &Game, library: Library) -> TileInfo {
    let running = library.running.with_untracked(|all| all.contains(&game.id));
    let sub = match game.last_played {
        _ if running => tr!("Jogando agora").to_string(),
        Some(when) => tr!("Jogado {}", format::ago(when)),
        None => source_name(&game.source).to_string(),
    };
    // A recomp shows its game's name; which recomp it is comes first on the line under it.
    let (title, version) = game_names(game, library);
    let sub = match version {
        Some(v) => format!("{v} · {sub}"),
        None => sub,
    };
    let badge = if running {
        Some(tr!("Jogando").to_string())
    } else if let Some(waiting) = waiting_for(game) {
        Some(waiting.to_string())
    } else if has_update(game, library) {
        Some(tr!("Atualização").to_string())
    } else {
        None
    };
    TileInfo {
        title,
        sub,
        covers: cover_sources(game),
        badge,
        favorite: game.favorite,
        game: Some(game.clone()),
        open: Route::Game(game.id.clone()),
        recomp_id: None,
        absent: game.next_step == cerberus_shared::NextStep::Install,
        progress: None,
    }
}

pub fn recomp_tile(entry: &RecompEntry) -> TileInfo {
    let update = match (&entry.installed, &entry.latest) {
        (Some(have), Some(newest)) => *have != newest.version,
        _ => false,
    };
    let badge = if update {
        Some(tr!("Atualização").to_string())
    } else if entry.installed.is_some() {
        Some(tr!("Instalado").to_string())
    } else {
        None
    };
    let (title, version) = entry_names(entry);
    TileInfo {
        title,
        sub: version.map_or_else(|| entry.console.clone(), |v| format!("{v} · {}", entry.console)),
        covers: recomp_covers(entry),
        badge,
        favorite: false,
        game: None,
        open: Route::Game(format!("recomp:{}", entry.id)),
        recomp_id: Some(entry.id.clone()),
        absent: false,
        progress: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, game: &str) -> RecompEntry {
        serde_json::from_value(serde_json::json!({
            "id": "x", "title": game, "name": name, "game": game, "console": "Nintendo 64", "origin": "n64",
            "stars": 0, "recommended": false, "installed": null, "icon": null, "cover": null, "hero": null, "adds": "", "needs": "", "tags": [], "features": [], "setup_steps": [], "why": "", "released": "", "added": "", "owned_copy": null, "handoff": null, "accepts_folder": false, "latest": null, "problem": null, "author": "", "repo": "", "about": "", "downloads": 0
        }))
        .unwrap()
    }

    #[test]
    fn a_recomp_is_shown_by_its_game_with_the_recomp_after() {
        assert_eq!(names_of("SpaghettiKart", Some(&entry("SpaghettiKart", "Mario Kart 64"))), ("Mario Kart 64".into(), Some("SpaghettiKart".into())));
        assert_eq!(names_of("StepMania", Some(&entry("StepMania", "StepMania"))), ("StepMania".into(), None), "a PC port named like its game");
        assert_eq!(names_of("My Port", Some(&entry("My Port", " "))), ("My Port".into(), None), "no game in the catalog: its own name");
        assert_eq!(names_of("Hollow Knight", None), ("Hollow Knight".into(), None));
    }
}

#[component]
pub fn Tile(info: TileInfo) -> impl IntoView {
    let library = expect_context::<Library>();
    let open = info.open.clone();
    let on_open = move |_| library.go(open.clone());
    let menu = expect_context::<crate::game_menu::GameMenu>();
    let menu_game = info.game.clone();
    let on_menu = move |e: leptos::ev::MouseEvent| {
        if let Some(game) = menu_game.clone() {
            menu.open(game, &e);
        }
    };
    let step = Memo::new(move |_| {
        info.recomp_id.as_ref().and_then(|id| library.progress.with(|all| all.get(id).cloned()))
    });
    let overlay = move || {
        let step = step.get();
        let text = match step {
            Some(InstallStep::Downloading { .. }) => Some(tr!("Baixando")),
            Some(InstallStep::Unpacking) => Some(tr!("Preparando")),
            Some(InstallStep::Paused { .. }) => Some(tr!("Pausado")),
            _ => info.badge.as_deref(),
        };
        let badge = text.map(|text| view! { <span class="chip">{text.to_owned()}</span> });
        let bar = match step {
            Some(InstallStep::Downloading { done, total } | InstallStep::Paused { done, total }) if total > 0 => {
                let percent = u128::from(done.min(total)) * 100 / u128::from(total);
                Some(view! { <span class="bar-p"><i style:width=format!("{percent}%")></i></span> })
            }
            _ => None,
        };
        (badge, bar)
    };
    // Play (or Install) right on the cover, without opening the page first.
    let quick = info.game.clone().filter(|g| !matches!(g.next_step, cerberus_shared::NextStep::ChooseCopy { .. } | cerberus_shared::NextStep::Locate { .. })).map(|g| {
        let (label, icon) = crate::launch_action(&g);
        let label = format!("{label} · {}", g.name);
        view! {
            <div class="qp-zone">
                <button type="button" class="qp" title=label.clone() aria-label=label
                    on:click=move |e| { e.stop_propagation(); crate::play_game(library, g.clone()); }>
                    <Svg icon size=20 />
                </button>
            </div>
        }
    });
    view! {
        <div class="tile" class:absent=info.absent>
            {quick}
            <button type="button" class="tile-b" on:click=on_open on:contextmenu=on_menu title=info.title.clone()>
                <div class="poster-wrap">
                    <Poster sources=info.covers.clone() title=info.title.clone() />
                    {info.favorite.then(|| view! { <span class="fav-mark" title=tr!("Favorito")><Svg icon=Icon::HeartFill size=16 /></span> })}
                    {info.progress.filter(|(_, total)| *total > 0).map(|(got, total)| {
                        let pct = got * 100 / total;
                        let tip = if got == total { tr!("Platinado · {} conquistas", total) } else { tr!("{} de {} conquistas", got, total) };
                        view! {
                            <span class="ach-mini" class:done=got == total title=tip.clone() aria-label=tip>
                                <i style=format!("width: {pct}%")></i>
                            </span>
                        }
                    })}
                    {overlay}
                </div>
                <span class="t">{info.title.clone()}</span>
                <span class="s">{info.sub.clone()}</span>
            </button>
        </div>
    }
}

/// A titled row of tiles that scrolls sideways, with arrows.
#[component]
pub fn Shelf(
    title: String,
    tiles: Vec<TileInfo>,
    #[prop(optional)] note: Option<String>,
    #[prop(optional)] see_all: Option<Route>,
) -> impl IntoView {
    let library = expect_context::<Library>();
    let track = NodeRef::<html::Div>::new();
    let scroll = move |direction: f64| {
        let Some(row) = track.get() else {
            return;
        };
        let step = row.client_width() as f64 * 0.85;
        row.scroll_by_with_x_and_y(direction * step, 0.0);
    };
    let see = see_all.map(|route| {
        view! { <button type="button" class="link" on:click=move |_| library.go(route.clone())>{tr!("Ver tudo")}</button> }
    });
    let note = note.map(|text| view! { <p>{text}</p> });
    let cards = tiles.into_iter().map(|info| view! { <Tile info /> }).collect_view();
    // The arrows only when the row scrolls, and each one off at its end.
    let at_start = RwSignal::new(true);
    let at_end = RwSignal::new(false);
    let measure = move || {
        if let Some(row) = track.get_untracked() {
            let (left, width, full) = (row.scroll_left(), row.client_width(), row.scroll_width());
            at_start.set(left <= 2);
            at_end.set(left + width >= full - 2);
        }
    };
    Effect::new(move || {
        if track.get().is_some() {
            request_animation_frame(measure);
        }
    });
    let resized = window_event_listener(leptos::ev::resize, move |_| measure());
    on_cleanup(move || resized.remove());

    view! {
        <section class="shelf">
            <div class="shelf-h">
                <div>
                    <h2>{title}</h2>
                    {note}
                </div>
                <div class="pager">
                    {see}
                    <Show when=move || !(at_start.get() && at_end.get())>
                        <button type="button" class="round" aria-label=tr!("Anterior") disabled=move || at_start.get() on:click=move |_| scroll(-1.0)>
                            <Svg icon=Icon::Left size=18 />
                        </button>
                        <button type="button" class="round" aria-label=tr!("Próximo") disabled=move || at_end.get() on:click=move |_| scroll(1.0)>
                            <Svg icon=Icon::Right size=18 />
                        </button>
                    </Show>
                </div>
            </div>
            <div class="track" node_ref=track on:scroll=move |_| measure()>{cards}</div>
        </section>
    }
}

/// The tile's note when a step comes before playing.
fn waiting_for(game: &Game) -> Option<&'static str> {
    match game.next_step {
        cerberus_shared::NextStep::Play => None,
        cerberus_shared::NextStep::RunInstaller => Some(tr!("Instalação pendente")),
        cerberus_shared::NextStep::ChooseCopy { .. } => Some(tr!("Falta sua cópia")),
        cerberus_shared::NextStep::Locate { .. } => Some(tr!("Programa não encontrado")),
        // Dimmed already, and under its own filter: no chip on every cover.
        cerberus_shared::NextStep::Install => None,
    }
}
