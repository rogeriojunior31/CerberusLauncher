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
const HEIGHT: i32 = 420;

impl GameMenu {
    /// Opens the menu at the pointer, kept inside the window.
    pub fn open(self, game: Game, event: &leptos::ev::MouseEvent) {
        event.prevent_default();
        let (w, h) = window_size();
        let x = event.client_x().min(w - WIDTH - 8).max(8);
        let y = event.client_y().min(h - HEIGHT - 8).max(8);
        self.0.set(Some((game, x, y)));
    }

    fn close(self) {
        self.0.set(None);
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
                library.games.refetch();
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
                        prop:value=move || typed.get() on:input=move |e| typed.set(event_target_value(&e)) />
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

#[component]
pub fn GameMenuLayer() -> impl IntoView {
    let library = expect_context::<Library>();
    let menu = expect_context::<GameMenu>();
    move || {
        let (game, x, y) = menu.0.get()?;
        let running = library.running.with(|all| all.contains(&game.id));
        let (play_label, play_icon) = if running { (tr!("Jogando"), Icon::Play) } else { crate::launch_action(&game) };
        let has_folder = !game.install_dir.is_empty();
        let (id, on) = (game.id.clone(), game.favorite);
        let page_id = game.id.clone();
        let folder_id = game.id.clone();
        let play = game.clone();
        let hidden = game.hidden;
        let hide_id = game.id.clone();
        let in_shelves = game.collections.clone();
        let shelf_id = game.id.clone();
        Some(view! {
            <div class="ctx-catch" on:click=move |_| menu.close() on:contextmenu=move |e| { e.prevent_default(); menu.close(); }></div>
            <div class="ctx" role="menu" style:left=format!("{x}px") style:top=format!("{y}px")
                on:keydown=move |e| if e.key() == "Escape" { menu.close(); }>
                <span class="ctx-h">{game.name.clone()}</span>
                <button type="button" role="menuitem" disabled=running on:click=move |_| { menu.close(); crate::play_game(library, play.clone()); }>
                    <Svg icon=play_icon size=16 />{play_label}
                </button>
                <button type="button" role="menuitem" on:click=move |_| { menu.close(); library.go(Route::Game(page_id.clone())); }>
                    <Svg icon=Icon::Info size=16 />{tr!("Página do jogo")}
                </button>
                <button type="button" role="menuitem" on:click=move |_| { menu.close(); crate::toggle_favorite(library, id.clone(), !on); }>
                    <Svg icon=if on { Icon::HeartFill } else { Icon::Heart } size=16 />
                    {if on { tr!("Tirar dos favoritos") } else { tr!("Adicionar aos favoritos") }}
                </button>
                <button type="button" role="menuitem" on:click=move |_| {
                    menu.close();
                    let id = hide_id.clone();
                    leptos::task::spawn_local(async move {
                        if library.report(api::set_hidden(&id, !hidden).await).is_some() {
                            library.games.refetch();
                            library.toast(if hidden { tr!("O jogo voltou para a biblioteca.") } else { tr!("Jogo oculto. Ele fica na aba Ocultos da biblioteca.") });
                        }
                    });
                }>
                    <Svg icon=Icon::EyeOff size=16 />{if hidden { tr!("Mostrar na biblioteca") } else { tr!("Ocultar da biblioteca") }}
                </button>
                <Shelves id=shelf_id in_shelves />
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
            </div>
        })
    }
}
