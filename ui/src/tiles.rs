//! Game tiles (poster, title, one line under it) and shelves of them.

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
    pub open: Route,
    /// Catalog tiles follow installation events without rebuilding the surrounding shelf.
    recomp_id: Option<String>,
}

pub fn source_name(source: &Source) -> &'static str {
    match source {
        Source::Steam { .. } => "Steam",
        Source::Epic { .. } => "Epic Games",
        Source::Local { .. } => "Adicionado à mão",
        Source::Recomp { .. } => "Recomp",
    }
}

/// Whether a newer release is out for an installed recomp.
fn has_update(game: &Game, library: Library) -> bool {
    let Source::Recomp { recomp_id, version, .. } = &game.source else {
        return false;
    };
    if game.external {
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

pub fn game_tile(game: &Game, library: Library) -> TileInfo {
    let running = library.running.with_untracked(|all| all.contains(&game.id));
    let sub = match game.last_played {
        _ if running => "Jogando agora".to_string(),
        Some(when) => format!("Jogado {}", format::ago(when)),
        None => source_name(&game.source).to_string(),
    };
    let badge = if running {
        Some("Jogando".to_string())
    } else if let Some(waiting) = waiting_for(game) {
        Some(waiting.to_string())
    } else if has_update(game, library) {
        Some("Atualização".to_string())
    } else {
        None
    };
    TileInfo {
        title: game.name.clone(),
        sub,
        covers: cover_sources(game),
        badge,
        open: Route::Game(game.id.clone()),
        recomp_id: None,
    }
}

pub fn recomp_tile(entry: &RecompEntry) -> TileInfo {
    let update = match (&entry.installed, &entry.latest) {
        (Some(have), Some(newest)) => *have != newest.version,
        _ => false,
    };
    let badge = if update {
        Some("Atualização".to_string())
    } else if entry.installed.is_some() {
        Some("Instalado".to_string())
    } else {
        None
    };
    TileInfo {
        title: entry.name.clone(),
        sub: entry.console.clone(),
        covers: recomp_covers(entry),
        badge,
        open: Route::Game(format!("recomp:{}", entry.id)),
        recomp_id: Some(entry.id.clone()),
    }
}

#[component]
pub fn Tile(info: TileInfo) -> impl IntoView {
    let library = expect_context::<Library>();
    let open = info.open.clone();
    let on_open = move |_| library.go(open.clone());
    let step = Memo::new(move |_| {
        info.recomp_id.as_ref().and_then(|id| library.progress.with(|all| all.get(id).cloned()))
    });
    let overlay = move || {
        let step = step.get();
        let text = match step {
            Some(InstallStep::Downloading { .. }) => Some("Baixando"),
            Some(InstallStep::Unpacking) => Some("Preparando"),
            _ => info.badge.as_deref(),
        };
        let badge = text.map(|text| view! { <span class="chip">{text.to_owned()}</span> });
        let bar = match step {
            Some(InstallStep::Downloading { done, total }) if total > 0 => {
                let percent = u128::from(done.min(total)) * 100 / u128::from(total);
                Some(view! { <span class="bar-p"><i style:width=format!("{percent}%")></i></span> })
            }
            _ => None,
        };
        (badge, bar)
    };
    view! {
        <div class="tile">
            <button type="button" class="tile-b" on:click=on_open title=info.title.clone()>
                <div class="poster-wrap">
                    <Poster sources=info.covers.clone() title=info.title.clone() />
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
        view! { <button type="button" class="link" on:click=move |_| library.go(route.clone())>"Ver tudo"</button> }
    });
    let note = note.map(|text| view! { <p>{text}</p> });
    let cards = tiles.into_iter().map(|info| view! { <Tile info /> }).collect_view();

    view! {
        <section class="shelf">
            <div class="shelf-h">
                <div>
                    <h2>{title}</h2>
                    {note}
                </div>
                <div class="pager">
                    {see}
                    <button type="button" class="round" aria-label="Anterior" on:click=move |_| scroll(-1.0)>
                        <Svg icon=Icon::Left size=18 />
                    </button>
                    <button type="button" class="round" aria-label="Próximo" on:click=move |_| scroll(1.0)>
                        <Svg icon=Icon::Right size=18 />
                    </button>
                </div>
            </div>
            <div class="track" node_ref=track>{cards}</div>
        </section>
    }
}

/// The tile's note when a step comes before playing.
fn waiting_for(game: &Game) -> Option<&'static str> {
    match game.next_step {
        cerberus_shared::NextStep::Play => None,
        cerberus_shared::NextStep::RunInstaller => Some("Instalação pendente"),
        cerberus_shared::NextStep::ChooseCopy { .. } => Some("Falta sua cópia"),
        cerberus_shared::NextStep::Locate { .. } => Some("Programa não encontrado"),
    }
}
