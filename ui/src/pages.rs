//! Downloads and Saves: the two lists the sidebar gives a page of their own.

use cerberus_shared::tr;
use cerberus_shared::{Game, Source};
use leptos::prelude::*;

use crate::art::{Poster, cover_sources};
use crate::discover::progress_view;
use crate::icons::{Icon, Svg};
use crate::{Library, Route, api, format};

#[component]
pub fn DownloadsPage() -> impl IntoView {
    let library = expect_context::<Library>();
    let rows = move || {
        let progress = library.progress.get();
        if progress.is_empty() {
            return view! {
                <div class="empty">
                    <Svg icon=Icon::Download size=40 />
                    <h2>{tr!("Nada baixando agora")}</h2>
                    <p class="muted">{tr!("Instale um recomp em Descobrir e o download aparece aqui.")}</p>
                    <button type="button" class="btn main" on:click=move |_| library.go(Route::Discover)>{tr!("Ir para Descobrir")}</button>
                </div>
            }
            .into_any();
        }
        let entries = library.recomps.get().and_then(Result::ok).unwrap_or_default();
        let list = progress
            .into_iter()
            .map(|(id, step)| {
                let entry = entries.iter().find(|e| e.id == id);
                let name = entry.map(|e| e.name.clone()).unwrap_or_else(|| id.clone());
                let covers = entry.map(crate::art::recomp_covers).unwrap_or_default();
                view! {
                    <div class="dl-row card">
                        <div class="thumb"><Poster sources=covers title=name.clone() /></div>
                        <div class="dl-info">
                            <b>{name}</b>
                            {progress_view(step, id)}
                        </div>
                    </div>
                }
            })
            .collect_view();
        view! { <div class="dl-list">{list}</div> }.into_any()
    };
    view! {
        <div class="page">
            <header class="page-h"><div><h1>{tr!("Downloads")}</h1></div></header>
            {rows}
        </div>
    }
}

#[component]
pub fn SavesPage() -> impl IntoView {
    let library = expect_context::<Library>();
    let rows = move || {
        let games = library.games.get().and_then(Result::ok).unwrap_or_default();
        let recomps: Vec<Game> = games.into_iter().filter(|g| matches!(g.source, Source::Recomp { .. })).collect();
        if recomps.is_empty() {
            return view! {
                <div class="empty">
                    <Svg icon=Icon::Saves size=40 />
                    <h2>{tr!("Nenhum recomp instalado")}</h2>
                    <p class="muted">{tr!("Os saves dos recomps são copiados sozinhos: quando você fecha o jogo, antes de atualizar e antes de desinstalar.")}</p>
                </div>
            }
            .into_any();
        }
        let list = recomps.into_iter().map(|game| view! { <SaveRow game /> }).collect_view();
        view! { <div class="dl-list">{list}</div> }.into_any()
    };
    view! {
        <div class="page">
            <header class="page-h">
                <div>
                    <h1>{tr!("Saves")}</h1>
                    <p class="muted">{tr!("Backups automáticos dos recomps. Os 10 mais recentes de cada jogo ficam guardados.")}</p>
                </div>
            </header>
            {rows}
        </div>
    }
}

#[component]
fn SaveRow(game: Game) -> impl IntoView {
    let library = expect_context::<Library>();
    let id = game.id.clone();
    let backups = LocalResource::new(move || {
        let id = id.clone();
        async move { api::list_backups(&id).await }
    });
    let summary = move || match backups.get() {
        None => tr!("Carregando...").to_string(),
        Some(Err(error)) => error,
        Some(Ok(list)) => match list.first() {
            None => tr!("Nenhum backup ainda").to_string(),
            Some(newest) => tr!("{} backup(s) · último {}", list.len(), format::ago(newest.created)),
        },
    };
    let route = Route::Game(game.id.clone());
    view! {
        <button type="button" class="dl-row card clickable" on:click=move |_| library.go(route.clone())>
            <div class="thumb"><Poster sources=cover_sources(&game) title=game.name.clone() /></div>
            <div class="dl-info">
                <b>{game.name.clone()}</b>
                <span class="muted">{summary}</span>
            </div>
            <Svg icon=Icon::Right />
        </button>
    }
}
