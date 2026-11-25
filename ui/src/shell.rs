//! The frame around every page: sidebar, title bar, and toasts.

use cerberus_shared::tr;
use leptos::prelude::*;

use crate::icons::{Icon, Svg};
use crate::{Library, Route, Toast, api};

/// The sections in menu order: Ctrl+1 to Ctrl+7.
const SECTIONS: [Route; 7] = [Route::Home, Route::Library, Route::Discover, Route::Achievements, Route::Downloads, Route::Saves, Route::News];

/// Keyboard shortcuts for the desktop window (Big Picture has its own controls):
/// "/" or Ctrl+F or Ctrl+K to search the page, Ctrl+, for settings, Ctrl+1-7 for the sections, Alt+Left to go back.
fn shortcuts(library: Library) {
    let handle = window_event_listener(leptos::ev::keydown, move |e: web_sys::KeyboardEvent| {
        if library.route.get_untracked() == Route::BigPicture {
            return;
        }
        // Alt+Left goes back, as in a browser.
        if e.alt_key() {
            if e.key() == "ArrowLeft" {
                e.prevent_default();
                library.back();
            }
            return;
        }
        let typing = document().active_element().is_some_and(|el| matches!(el.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT"));
        let ctrl = e.ctrl_key() || e.meta_key();
        let key = e.key();
        let search = (key == "/" && !typing) || (ctrl && matches!(key.as_str(), "f" | "F" | "k" | "K"));
        if search {
            let found = document().query_selector(".page input[type=search], .set-find").ok().flatten();
            e.prevent_default();
            match found {
                Some(input) => {
                    let input: web_sys::HtmlElement = wasm_bindgen::JsCast::unchecked_into(input);
                    let _ = input.focus();
                }
                // A page with no search of its own (a game's): the library's.
                None => {
                    library.go(Route::Library);
                    request_animation_frame(|| {
                        if let Some(input) = document().query_selector(".page input[type=search]").ok().flatten() {
                            let input: web_sys::HtmlElement = wasm_bindgen::JsCast::unchecked_into(input);
                            let _ = input.focus();
                        }
                    });
                }
            }
            return;
        }
        if !ctrl {
            return;
        }
        if key == "," {
            e.prevent_default();
            library.go(Route::Settings);
        } else if let Some(n) = key.parse::<usize>().ok().filter(|n| (1..=SECTIONS.len()).contains(n)) {
            e.prevent_default();
            library.go(SECTIONS[n - 1].clone());
        }
    });
    on_cleanup(move || handle.remove());
}

#[component]
pub fn Sidebar() -> impl IntoView {
    let library = expect_context::<Library>();
    let activity = expect_context::<crate::news::Activity>();
    shortcuts(library);
    let item = move |route: Route, icon: Icon, label: &'static str| {
        let target = route.clone();
        // A recomp's page belongs to the Library once it's installed, to Discover before.
        let on = move || {
            library.route.with(|now| {
                let in_library = |id: &String| library.games.with(|g| g.as_ref().and_then(|r| r.as_ref().ok()).is_some_and(|list| list.iter().any(|x| &x.id == id)));
                let section = match now {
                    Route::Game(id) if in_library(id) => Route::Library,
                    other => other.section(),
                };
                section == target
            })
        };
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
            <button type="button" class="nav" class:on=on.clone() aria-current=move || on().then_some("page")
                title=SECTIONS.iter().position(|r| *r == route).map_or_else(|| label.to_string(), |i| format!("{label} (Ctrl+{})", i + 1))
                on:click=move |_| library.go(route.clone())>
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
                        title=tr!("Configurações (Ctrl+,)") aria-label=tr!("Configurações") on:click=move |_| library.go(Route::Settings)>
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
            .filter(|g| !g.hidden && g.part_ids.is_empty() && matches!(g.next_step, cerberus_shared::NextStep::Play | cerberus_shared::NextStep::RunInstaller))
            .collect();
        list.sort_by_cached_key(|g| (!g.favorite, std::cmp::Reverse(g.last_played), crate::tiles::game_names(g, library).0.to_lowercase()));
        list
    });
    // The classics an emulator here opens, played lately first.
    let classics = Memo::new(move |_| {
        let players = library.players.get().unwrap_or_default();
        let mut list: Vec<(cerberus_shared::RomView, &'static str, String)> = crate::classics::standalone(library)
            .into_iter()
            .filter_map(|r| {
                let console = crate::classics::console_of(&r);
                crate::classics::ready_player(&players, console).map(|player| (r, console, player))
            })
            .collect();
        list.sort_by_cached_key(|(r, _, _)| (std::cmp::Reverse(r.last_played), crate::classics::display_name(r).to_lowercase()));
        list
    });
    let total = move || ready.with(Vec::len) + classics.with(Vec::len);
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
        ready
            .get()
            .into_iter()
            .filter(|g| {
                let (name, version) = crate::tiles::game_names(g, library);
                q.is_empty() || format!("{name} {}", version.unwrap_or_default()).to_lowercase().contains(&q)
            })
            .collect::<Vec<_>>()
    };
    // Names shown more than once (a recomp and its classic): those rows say which one they are.
    let twice = Memo::new(move |_| {
        let mut seen = std::collections::HashMap::<String, usize>::new();
        for g in ready.get() {
            *seen.entry(crate::tiles::game_names(&g, library).0.to_lowercase()).or_default() += 1;
        }
        for (r, _, _) in classics.get() {
            *seen.entry(crate::classics::display_name(&r).to_lowercase()).or_default() += 1;
        }
        seen.into_iter().filter(|(_, n)| *n > 1).map(|(k, _)| k).collect::<std::collections::HashSet<String>>()
    });
    let classic_rows = move || {
        let q = query.get().to_lowercase();
        classics
            .get()
            .into_iter()
            .filter(|(r, _, _)| q.is_empty() || crate::classics::display_name(r).to_lowercase().contains(&q))
            .map(|(rom, console, player)| {
                let route = Route::Classic(rom.path.clone());
                let here = route.clone();
                let on = move || library.route.with(|now| *now == here);
                let name = crate::classics::display_name(&rom);
                let tip = format!("{name} · {} · {player}", crate::classics::console_name(console));
                let which = twice.with(|t| t.contains(&name.to_lowercase())).then(|| format!("{} · {}", crate::classics::console_name(console), player));
                let (path, console_id) = (rom.path.clone(), console.to_string());
                view! {
                    <div class="sg-item" class:on=on>
                        <button type="button" class="sg-open" title=tip on:click=move |_| library.go(route.clone())>
                            <span class="fav-thumb"><crate::art::Poster sources=crate::classics::box_art(&rom, console) title=name.clone() /></span>
                            <span class="sg-name">{name}{which.map(|w| view! { <small>{w}</small> })}</span>
                        </button>
                        <button type="button" class="sg-play" title=tr!("Jogar no {}", player) aria-label=tr!("Jogar no {}", player)
                            on:click=move |_| crate::classics::play_original(library, path.clone(), console_id.clone())>
                            <Svg icon=Icon::Play size=13 />
                        </button>
                    </div>
                }
            })
            .collect_view()
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
                let (name, version) = crate::tiles::game_names(&g, library);
                let tip = version.as_ref().map_or_else(|| name.clone(), |v| format!("{name} · {v}"));
                let which = twice.with(|t| t.contains(&name.to_lowercase())).then(|| version.clone().unwrap_or_else(|| crate::tiles::source_name(&g.source).to_string()));
                view! {
                    <div class="sg-item" class:on=on>
                        <button type="button" class="sg-open" title=tip on:click=move |_| library.go(route.clone())>
                            <span class="fav-thumb"><crate::art::Poster sources=crate::art::cover_sources(&g) title=name.clone() /></span>
                            <span class="sg-name">{name}{which.map(|w| view! { <small>{w}</small> })}</span>
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
        <Show when=move || { total() > 0 }>
            <div class="side-games">
                <div class="sg-head">
                    <span class="side-h">{tr!("Prontos para jogar")}<b>{total}</b></span>
                </div>
                <Show when=move || { total() > 8 }>
                    <label class="sg-search">
                        <Svg icon=Icon::Search size=14 />
                        <input type="search" placeholder=tr!("Filtrar jogos") prop:value=move || query.get()
                            on:input=move |e| query.set(event_target_value(&e)) />
                    </label>
                </Show>
                <div class="sg-list">
                    {rows}
                    {classic_rows}
                    <Show when=move || shown().is_empty() && {
                        let q = query.get().to_lowercase();
                        !classics.with(|c| c.iter().any(|(r, _, _)| crate::classics::display_name(r).to_lowercase().contains(&q)))
                    }><p class="sg-none">{tr!("Nenhum jogo com esse nome.")}</p></Show>
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
