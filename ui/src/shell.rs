//! The frame around every page: sidebar, title bar, and toasts.

use cerberus_shared::tr;
use leptos::prelude::*;

use crate::icons::{Icon, Svg};
use crate::{Library, Route, Toast, api};

#[component]
pub fn Sidebar() -> impl IntoView {
    let library = expect_context::<Library>();
    let item = move |route: Route, icon: Icon, label: &'static str| {
        let target = route.clone();
        let on = move || library.route.with(|now| now.section() == target);
        let downloads = route == Route::Downloads;
        let count = matches!(route, Route::Downloads | Route::News).then(|| {
            view! { <b class="n">{move || {
                let n = if downloads { library.progress.with(|p| p.len()) } else { crate::news::update_count(library) };
                (n > 0).then_some(n)
            }}</b> }
        });
        view! {
            <button type="button" class="nav" class:on=on title=label on:click=move |_| library.go(route.clone())>
                <Svg icon />
                <span>{label}</span>
                {count}
            </button>
        }
    };

    view! {
        <aside class="side">
            <div class="brand" data-tauri-drag-region>
                <img class="mark" src="brand/cerberus-pixel-icone.svg" alt=tr!("Cerberus Launcher") draggable="false" />
                <img class="word" src="brand/cerberus-launcher-pixel.svg#wordmark" alt="" draggable="false" />
            </div>
            <nav class="navs" aria-label=tr!("Seções")>
                {item(Route::Home, Icon::Home, tr!("Início"))}
                {item(Route::Library, Icon::Library, tr!("Biblioteca"))}
                {item(Route::Discover, Icon::Discover, tr!("Descobrir"))}
                {item(Route::Downloads, Icon::Download, tr!("Downloads"))}
                {item(Route::Saves, Icon::Saves, tr!("Saves"))}
                {item(Route::News, Icon::Bell, tr!("Novidades"))}
            </nav>
            {move || {
                let mut favorites: Vec<cerberus_shared::Game> = library.games.get().and_then(Result::ok).unwrap_or_default().into_iter().filter(|g| g.favorite && !g.hidden).collect();
                favorites.sort_by_key(|g| (std::cmp::Reverse(g.last_played), g.name.to_lowercase()));
                (!favorites.is_empty()).then(|| {
                    let rows = favorites.into_iter().take(8).map(|g| {
                        let route = Route::Game(g.id.clone());
                        let here = route.clone();
                        let on = move || library.route.with(|now| *now == here);
                        view! {
                            <button type="button" class="fav-item" class:on=on title=g.name.clone() on:click=move |_| library.go(route.clone())>
                                <span class="fav-thumb"><crate::art::Poster sources=crate::art::cover_sources(&g) title=g.name.clone() /></span>
                                <span>{g.name.clone()}</span>
                            </button>
                        }
                    }).collect_view();
                    view! {
                        <div class="side-favs">
                            <span class="side-h"><Svg icon=Icon::HeartFill size=12 />{tr!("Favoritos")}</span>
                            {rows}
                        </div>
                    }
                })
            }}
            <div class="side-foot">
                <button type="button" class="nav" on:click=move |_| library.add_game()>
                    <Svg icon=Icon::Plus />
                    <span>{tr!("Adicionar jogo")}</span>
                </button>
                {item(Route::BigPicture, Icon::Tv, tr!("Big Picture Mode"))}
                {item(Route::Settings, Icon::Settings, tr!("Configurações"))}
            </div>
        </aside>
    }
}

#[component]
pub fn TitleBar() -> impl IntoView {
    view! {
        <div class="topbar" data-tauri-drag-region>
            <div class="win">
                <button type="button" aria-label=tr!("Minimizar") on:click=|_| api::window_action("minimize")>
                    <Svg icon=Icon::Minimize size=14 />
                </button>
                <button type="button" aria-label=tr!("Maximizar") on:click=|_| api::window_action("toggleMaximize")>
                    <Svg icon=Icon::Maximize size=14 />
                </button>
                <button type="button" class="close" aria-label=tr!("Fechar") on:click=|_| api::window_action("close")>
                    <Svg icon=Icon::Close size=14 />
                </button>
            </div>
        </div>
    }
}

#[component]
pub fn Toasts() -> impl IntoView {
    let library = expect_context::<Library>();
    let list = move || {
        library
            .toasts
            .get()
            .into_iter()
            .map(|toast: Toast| {
                let id = toast.id;
                let dismiss = move |_| library.toasts.update(|all| all.retain(|t| t.id != id));
                view! {
                    <div class="toast" class:bad=toast.error role=if toast.error { "alert" } else { "status" }>
                        <span>{toast.text}</span>
                        <button type="button" aria-label=tr!("Fechar") on:click=dismiss><Svg icon=Icon::Close size=14 /></button>
                    </div>
                }
            })
            .collect_view()
    };
    view! { <div class="toasts" aria-live="polite">{list}</div> }
}
