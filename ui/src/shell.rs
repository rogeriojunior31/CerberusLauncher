//! The frame around every page: sidebar, title bar, and toasts.

use cerberus_shared::tr;
use leptos::prelude::*;

use crate::icons::{Icon, Svg};
use crate::{Library, Route, Toast, api};

#[component]
pub fn Sidebar() -> impl IntoView {
    let library = expect_context::<Library>();
    let activity = expect_context::<crate::news::Activity>();
    let item = move |route: Route, icon: Icon, label: &'static str| {
        let target = route.clone();
        let on = move || library.route.with(|now| now.section() == target);
        let downloads = route == Route::Downloads;
        let count = matches!(route, Route::Downloads | Route::News).then(|| {
            view! { <b class="n">{move || {
                let n = if downloads {
                    library.progress.with(|p| p.len())
                } else {
                    crate::news::update_count(library) + activity.unread()
                };
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
                {item(Route::Achievements, Icon::Trophy, tr!("Conquistas"))}
                {item(Route::Downloads, Icon::Download, tr!("Downloads"))}
                {item(Route::Saves, Icon::Saves, tr!("Saves"))}
                {item(Route::News, Icon::Bell, tr!("Novidades"))}
            </nav>
            <SideGames />
            <div class="side-foot">
                <crate::updates::UpdateNotice />
                <div class="foot-row">
                    <button type="button" class="foot-icon foot-add" title=tr!("Adicionar jogo") aria-label=tr!("Adicionar jogo") on:click=move |_| library.add_game()>
                        <Svg icon=Icon::Plus size=20 />
                    </button>
                    <button type="button" class="foot-icon" title=tr!("Big Picture Mode") aria-label=tr!("Big Picture Mode")
                        on:click=move |_| library.go(Route::BigPicture)>
                        <Svg icon=Icon::Tv size=19 />
                    </button>
                    <button type="button" class="foot-icon" class:on=move || library.route.with(|r| r.section() == Route::Settings)
                        title=tr!("Configurações") aria-label=tr!("Configurações") on:click=move |_| library.go(Route::Settings)>
                        <Svg icon=Icon::Settings size=19 />
                    </button>
                </div>
            </div>
        </aside>
    }
}

/// The games ready to play right now, under the sections: favorites first,
/// then the ones played lately. A field narrows the list when it gets long.
#[component]
fn SideGames() -> impl IntoView {
    let library = expect_context::<Library>();
    let query = RwSignal::new(String::new());
    let ready = Memo::new(move |_| {
        let mut list: Vec<cerberus_shared::Game> = library
            .games
            .get()
            .and_then(Result::ok)
            .unwrap_or_default()
            .into_iter()
            .filter(|g| !g.hidden && matches!(g.next_step, cerberus_shared::NextStep::Play | cerberus_shared::NextStep::RunInstaller))
            .collect();
        list.sort_by_key(|g| (!g.favorite, std::cmp::Reverse(g.last_played), g.name.to_lowercase()));
        list
    });
    // The tray icon's menu offers the games played most recently.
    let sent = StoredValue::new(Vec::<String>::new());
    Effect::new(move |_| {
        let mut recent: Vec<cerberus_shared::Game> = ready.get().into_iter().filter(|g| g.last_played.is_some()).collect();
        recent.sort_by_key(|g| std::cmp::Reverse(g.last_played));
        recent.truncate(5);
        let ids: Vec<String> = recent.iter().map(|g| format!("{}|{}", g.id, g.name)).collect();
        if sent.with_value(|s| *s != ids) {
            sent.set_value(ids);
            leptos::task::spawn_local(api::set_tray_games(recent));
        }
    });
    let shown = move || {
        let q = query.get().to_lowercase();
        ready.get().into_iter().filter(|g| q.is_empty() || g.name.to_lowercase().contains(&q)).collect::<Vec<_>>()
    };
    let rows = move || {
        shown()
            .into_iter()
            .map(|g| {
                let route = Route::Game(g.id.clone());
                let here = route.clone();
                let on = move || library.route.with(|now| *now == here);
                let gid = g.id.clone();
                let running = move || library.running.with(|r| r.contains(&gid));
                let game = StoredValue::new(g.clone());
                view! {
                    <div class="sg-item" class:on=on>
                        <button type="button" class="sg-open" title=g.name.clone() on:click=move |_| library.go(route.clone())>
                            <span class="fav-thumb"><crate::art::Poster sources=crate::art::cover_sources(&g) title=g.name.clone() /></span>
                            <span class="sg-name">{g.name.clone()}</span>
                            {g.favorite.then(|| view! { <span class="sg-fav"><Svg icon=Icon::HeartFill size=11 /></span> })}
                            <Show when=running><i class="sg-live" title=tr!("Jogando agora")></i></Show>
                        </button>
                        <button type="button" class="sg-play" title=tr!("Jogar") aria-label=tr!("Jogar")
                            on:click=move |_| crate::play_game(library, game.get_value())>
                            <Svg icon=Icon::Play size=13 />
                        </button>
                    </div>
                }
            })
            .collect_view()
    };
    view! {
        <Show when=move || ready.with(|r| !r.is_empty())>
            <div class="side-games">
                <div class="sg-head">
                    <span class="side-h">{tr!("Prontos para jogar")}<b>{move || ready.with(Vec::len)}</b></span>
                </div>
                <Show when=move || ready.with(|r| r.len() > 8)>
                    <label class="sg-search">
                        <Svg icon=Icon::Search size=14 />
                        <input type="search" placeholder=tr!("Filtrar jogos") prop:value=move || query.get()
                            on:input=move |e| query.set(event_target_value(&e)) />
                    </label>
                </Show>
                <div class="sg-list">
                    {rows}
                    <Show when=move || shown().is_empty()><p class="sg-none">{tr!("Nenhum jogo com esse nome.")}</p></Show>
                </div>
            </div>
        </Show>
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
