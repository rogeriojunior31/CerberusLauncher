//! The frame around every page: sidebar, title bar, and toasts.

use leptos::prelude::*;

use crate::icons::{Icon, Svg};
use crate::{Library, Route, Toast, api};

#[component]
pub fn Sidebar() -> impl IntoView {
    let library = expect_context::<Library>();
    let item = move |route: Route, icon: Icon, label: &'static str| {
        let target = route.clone();
        let on = move || library.route.with(|now| now.section() == target);
        let count = (route == Route::Downloads).then(|| {
            view! { <b class="n">{move || { let n = library.progress.with(|p| p.len()); (n > 0).then_some(n) }}</b> }
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
                <span class="mark" aria-hidden="true">"C"</span>
                <span class="word" data-tauri-drag-region>"Cerberus"</span>
            </div>
            <nav class="navs" aria-label="Seções">
                {item(Route::Home, Icon::Home, "Início")}
                {item(Route::Library, Icon::Library, "Biblioteca")}
                {item(Route::Discover, Icon::Discover, "Descobrir")}
                {item(Route::Downloads, Icon::Download, "Downloads")}
                {item(Route::Saves, Icon::Saves, "Saves")}
            </nav>
            <div class="side-foot">
                <button type="button" class="nav" on:click=move |_| library.add_game()>
                    <Svg icon=Icon::Plus />
                    <span>"Adicionar jogo"</span>
                </button>
                {item(Route::BigPicture, Icon::Tv, "Tela grande")}
                {item(Route::Settings, Icon::Settings, "Configurações")}
            </div>
        </aside>
    }
}

#[component]
pub fn TitleBar() -> impl IntoView {
    view! {
        <div class="topbar" data-tauri-drag-region>
            <div class="win">
                <button type="button" aria-label="Minimizar" on:click=|_| api::window_action("minimize")>
                    <Svg icon=Icon::Minimize size=14 />
                </button>
                <button type="button" aria-label="Maximizar" on:click=|_| api::window_action("toggleMaximize")>
                    <Svg icon=Icon::Maximize size=14 />
                </button>
                <button type="button" class="close" aria-label="Fechar" on:click=|_| api::window_action("close")>
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
                        <button type="button" aria-label="Fechar" on:click=dismiss><Svg icon=Icon::Close size=14 /></button>
                    </div>
                }
            })
            .collect_view()
    };
    view! { <div class="toasts" aria-live="polite">{list}</div> }
}
