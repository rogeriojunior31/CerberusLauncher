//! The menu a right click on a game's cover opens, as in Hydra: play, its
//! page, favorite, collections, hiding, and its folder, without opening the page first.

use cerberus_shared::tr;
use cerberus_shared::Game;
use leptos::prelude::*;

use crate::icons::{Icon, Svg};
use crate::{Library, Route, api};

/// The game the menu is open for, and where (window pixels).
#[derive(Clone, Copy)]
pub struct GameMenu(pub RwSignal<Option<(Game, i32, i32)>>);

const WIDTH: i32 = 230;

thread_local! {
    /// What had the focus when the menu opened: it gets it back on close.
    static OPENER: std::cell::RefCell<Option<web_sys::HtmlElement>> = const { std::cell::RefCell::new(None) };
}
const HEIGHT: i32 = 420;

impl GameMenu {
    /// Opens the menu at the pointer, kept inside the window. From the keyboard
    /// (Enter on a button: no pointer), it opens under the button instead.
    pub fn open(self, game: Game, event: &leptos::ev::MouseEvent) {
        event.prevent_default();
        let opener = document().active_element().map(|el| wasm_bindgen::JsCast::unchecked_into::<web_sys::HtmlElement>(el));
        OPENER.with(|o| *o.borrow_mut() = opener);
        let (w, h) = window_size();
        let (mut x, mut y) = (event.client_x(), event.client_y());
        if event.detail() == 0 {
            if let Some(el) = event.current_target().and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::Element>(t).ok()) {
                let r = el.get_bounding_client_rect();
                x = (r.right() as i32) - WIDTH;
                y = r.bottom() as i32 + 6;
            }
        }
        let x = x.min(w - WIDTH - 8).max(8);
        let y = y.min(h - HEIGHT - 8).max(8);
        self.0.set(Some((game, x, y)));
        // The first choice takes the focus, so the keyboard lands in the menu.
        request_animation_frame(|| {
            if let Some(first) = document().query_selector(".ctx [role^=menuitem]:not([disabled])").ok().flatten() {
                let first: web_sys::HtmlElement = wasm_bindgen::JsCast::unchecked_into(first);
                let _ = first.focus();
            }
        });
    }

    fn close(self) {
        self.0.set(None);
        // Back where the keyboard was, not lost on the page.
        if let Some(el) = OPENER.with(|o| o.borrow_mut().take()) {
            let _ = el.focus();
        }
    }

    /// Open for this game, for the trigger's aria-expanded.
    pub fn is_open_for(self, id: &str) -> bool {
        self.0.with(|m| m.as_ref().is_some_and(|(g, _, _)| g.id == id))
    }
}

fn window_size() -> (i32, i32) {
    let window = leptos::prelude::window();
    let size = |v: Result<wasm_bindgen::JsValue, _>| v.ok().and_then(|v| v.as_f64()).unwrap_or(1280.0) as i32;
    (size(window.inner_width()), size(window.inner_height()))
}

/// The collections in the menu: each one checked when the game is in it, and a new one.
#[component]
fn Shelves(id: String, in_shelves: Vec<String>) -> impl IntoView {
    let library = expect_context::<Library>();
    let menu = expect_context::<GameMenu>();
    let names = LocalResource::new(api::list_collections);
    let id = StoredValue::new(id);
    let in_shelves = StoredValue::new(in_shelves);
    let typing = RwSignal::new(false);
    let typed = RwSignal::new(String::new());
    // A box made after the page loads doesn't take `autofocus`: focused here.
    let name_box = NodeRef::<leptos::html::Input>::new();
    Effect::new(move |_| {
        if let Some(input) = name_box.get() {
            let _ = input.focus();
        }
    });
    let put = move |name: String, on: bool| {
        // Read before closing: the menu takes its values with it.
        let id = id.get_value();
        menu.close();
        leptos::task::spawn_local(async move {
            if library.report(api::set_in_collection(&name, &id, on).await).is_some() {
                library.patch_game(&id, |g| {
                    g.collections.retain(|c| c != &name);
                    if on {
                        g.collections.push(name.clone());
                    }
                });
                library.toast(if on { tr!("Adicionado à coleção {}.", name) } else { tr!("Tirado da coleção {}.", name) });
            }
        });
    };
    view! {
        <span class="ctx-h">{tr!("Coleções")}</span>
        {move || names.get().and_then(Result::ok).unwrap_or_default().into_iter().map(|name| {
            let on = in_shelves.with_value(|s| s.contains(&name));
            let label = name.clone();
            view! {
                <button type="button" role="menuitemcheckbox" aria-checked=on.to_string() on:click=move |_| put(name.clone(), !on)>
                    <Svg icon=if on { Icon::Check } else { Icon::Shelf } size=16 />{label}
                </button>
            }
        }).collect_view()}
        {move || if typing.get() {
            view! {
                <form class="ctx-form" on:submit=move |e| { e.prevent_default(); put(typed.get_untracked(), true); }>
                    <input class="input" type="text" maxlength="40" placeholder=tr!("Nome da coleção") aria-label=tr!("Nome da nova coleção") node_ref=name_box
                        prop:value=move || typed.get() on:input=move |e| typed.set(event_target_value(&e))
                        on:keydown=move |e| if e.key() == "Escape" {
                            // Esc here closes the box, not the whole menu.
                            e.stop_propagation();
                            typing.set(false);
                            typed.set(String::new());
                        } />
                </form>
            }.into_any()
        } else {
            view! {
                <button type="button" role="menuitem" on:click=move |_| typing.set(true)>
                    <Svg icon=Icon::Plus size=16 />{tr!("Nova coleção…")}
                </button>
            }.into_any()
        }}
    }
}

/// Up and down move between the menu's items.
fn step_items(e: &web_sys::KeyboardEvent) {
    // Typing a collection's name: the arrows, Home and End move in the text.
    let typing = e.target().and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::Element>(t).ok()).is_some_and(|el| matches!(el.tag_name().as_str(), "INPUT" | "TEXTAREA"));
    if typing {
        return;
    }
    let key = e.key();
    let step: i32 = match key.as_str() {
        "ArrowDown" => 1,
        "ArrowUp" => -1,
        "Home" | "End" => 0,
        _ => return,
    };
    e.prevent_default();
    let Some(list) = document().query_selector_all(".ctx [role^=menuitem]:not([disabled])").ok() else { return };
    let n = list.length() as i32;
    if n == 0 {
        return;
    }
    // Home and End: the first and the last item.
    if step == 0 {
        let index = if key == "Home" { 0 } else { n - 1 };
        if let Some(item) = list.item(index as u32) {
            let item: web_sys::HtmlElement = wasm_bindgen::JsCast::unchecked_into(item);
            let _ = item.focus();
        }
        return;
    }
    let active = document().active_element();
    let at = (0..n).find(|i| list.item(*i as u32).zip(active.clone()).is_some_and(|(a, b)| wasm_bindgen::JsValue::from(a) == wasm_bindgen::JsValue::from(b))).unwrap_or(-1);
    if let Some(next) = list.item(((at + step + n) % n) as u32) {
        let next: web_sys::HtmlElement = wasm_bindgen::JsCast::unchecked_into(next);
        let _ = next.focus();
    }
}

#[component]
pub fn GameMenuLayer() -> impl IntoView {
    let library = expect_context::<Library>();
    let menu = expect_context::<GameMenu>();
    let keys = window_event_listener(leptos::ev::keydown, move |e: web_sys::KeyboardEvent| {
        if menu.0.with_untracked(Option::is_none) {
            return;
        }
        let in_box = e.target().and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::Element>(t).ok()).is_some_and(|el| el.tag_name() == "INPUT");
        if e.key() == "Escape" && !in_box {
            menu.close();
        } else {
            step_items(&e);
        }
    });
    on_cleanup(move || keys.remove());
    move || {
        let (game, x, y) = menu.0.get()?;
        let running = library.running.with(|all| all.contains(&game.id));
        let (play_label, play_icon) = if running { (tr!("Jogando"), Icon::Play) } else { crate::launch_action(&game) };
        let has_folder = !game.install_dir.is_empty();
        // Recomps and games added by hand are the launcher's to remove.
        let removable = matches!(game.source, cerberus_shared::Source::Recomp { .. } | cerberus_shared::Source::Local { .. });
        let remove_label = if matches!(game.source, cerberus_shared::Source::Recomp { .. }) && !game.external { tr!("Desinstalar…") } else { tr!("Remover da biblioteca…") };
        let remove_id = game.id.clone();
        let (id, on) = (game.id.clone(), game.favorite);
        let page_id = game.id.clone();
        let folder_id = game.id.clone();
        let play = game.clone();
        let on_page = library.route.get_untracked() == Route::Game(game.id.clone());
        let original = {
            let games = library.games.get_untracked().and_then(Result::ok).unwrap_or_default();
            let recomps = library.recomps.get_untracked().and_then(Result::ok).unwrap_or_default();
            let players = library.players.get_untracked().unwrap_or_default();
            library.roms.get_untracked().unwrap_or_default().into_iter()
                .find(|r| crate::classics::recomp_game(r, &games, &recomps).is_some_and(|g| g.id == game.id))
                .and_then(|r| {
                    let console = crate::classics::console_of(&r);
                    crate::classics::ready_player(&players, console).map(|name| (r.path.clone(), console.to_string(), name))
                })
        };
        let hidden = game.hidden;
        let hide_id = game.id.clone();
        let in_shelves = game.collections.clone();
        let shelf_id = game.id.clone();
        Some(view! {
            <div class="ctx-catch" on:click=move |_| menu.close() on:contextmenu=move |e| { e.prevent_default(); menu.close(); }></div>
            <div class="ctx" role="menu" style:left=format!("{x}px") style:top=format!("{y}px")
                on:keydown=move |e| if e.key() == "Escape" { menu.close(); }>
                <span class="ctx-h">{crate::tiles::game_names(&game, library).0}</span>
                {(!on_page).then(|| view! {
                    <button type="button" role="menuitem" disabled=running on:click=move |_| { menu.close(); crate::play_game(library, play.clone()); }>
                        <Svg icon=play_icon size=16 />{play_label}
                    </button>
                })}
                {original.filter(|_| !on_page).map(|(path, console, name)| view! {
                    <button type="button" role="menuitem" on:click=move |_| { menu.close(); crate::classics::play_original(library, path.clone(), console.clone()); }>
                        <Svg icon=Icon::Disc size=16 />{tr!("Jogar o original no {}", name)}
                    </button>
                })}
                // Not offered on the page itself.
                {(library.route.get_untracked() != Route::Game(page_id.clone())).then(|| view! {
                    <button type="button" role="menuitem" on:click=move |_| { menu.close(); library.go(Route::Game(page_id.clone())); }>
                        <Svg icon=Icon::Info size=16 />{tr!("Página do jogo")}
                    </button>
                })}
                // On the game's page the heart sits beside this menu: not twice.
                {(!on_page).then(|| view! {
                    <button type="button" role="menuitem" on:click=move |_| { menu.close(); crate::toggle_favorite(library, id.clone(), !on); }>
                        <Svg icon=if on { Icon::HeartFill } else { Icon::Heart } size=16 />
                        {if on { tr!("Tirar dos favoritos") } else { tr!("Adicionar aos favoritos") }}
                    </button>
                })}
                <Shelves id=shelf_id in_shelves />
                // The game's files: a group of their own, not under Coleções.
                {has_folder.then(|| view! { <span class="ctx-sep" role="separator"></span> })}
                {has_folder.then(|| {
                    let id = game.id.clone();
                    view! {
                        <button type="button" role="menuitem" on:click=move |_| {
                            menu.close();
                            let id = id.clone();
                            leptos::task::spawn_local(async move {
                                if library.report(api::create_shortcut(&id).await).is_some() {
                                    library.toast(tr!("Atalho criado na área de trabalho."));
                                }
                            });
                        }>
                            <Svg icon=Icon::External size=16 />{tr!("Criar atalho na área de trabalho")}
                        </button>
                    }
                })}
                {has_folder.then(|| view! {
                    <button type="button" role="menuitem" on:click=move |_| {
                        menu.close();
                        let id = folder_id.clone();
                        leptos::task::spawn_local(async move { library.report(api::open_folder(&id).await); });
                    }>
                        <Svg icon=Icon::Folder size=16 />{tr!("Abrir pasta do jogo")}
                    </button>
                })}
                // Taking it off the shelf last, apart from the rest.
                <span class="ctx-sep" role="separator"></span>
                <button type="button" role="menuitem" on:click=move |_| {
                    menu.close();
                    let id = hide_id.clone();
                    leptos::task::spawn_local(async move {
                        if library.report(api::set_hidden(&id, !hidden).await).is_some() {
                            library.patch_game(&id, |g| g.hidden = !hidden);
                            library.toast(if hidden { tr!("O jogo voltou para a biblioteca.") } else { tr!("Jogo oculto. Ele fica na aba Ocultos da biblioteca.") });
                        }
                    });
                }>
                    <Svg icon=Icon::EyeOff size=16 />{if hidden { tr!("Mostrar na biblioteca") } else { tr!("Ocultar da biblioteca") }}
                </button>
                {removable.then(|| view! {
                    <button type="button" role="menuitem" class="danger" on:click=move |_| {
                        menu.close();
                        crate::game::open_to_remove(library, remove_id.clone());
                    }>
                        <Svg icon=Icon::Trash size=16 />{remove_label}
                    </button>
                })}
            </div>
        })
    }
}
