//! The menu a right click on a game's cover opens, as in Hydra: play, its
//! page, favorite, and its folder, without opening the page first.

use cerberus_shared::Game;
use leptos::prelude::*;

use crate::icons::{Icon, Svg};
use crate::{Library, Route, api};

/// The game the menu is open for, and where (window pixels).
#[derive(Clone, Copy)]
pub struct GameMenu(pub RwSignal<Option<(Game, i32, i32)>>);

const WIDTH: i32 = 230;
const HEIGHT: i32 = 250;

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

#[component]
pub fn GameMenuLayer() -> impl IntoView {
    let library = expect_context::<Library>();
    let menu = expect_context::<GameMenu>();
    move || {
        let (game, x, y) = menu.0.get()?;
        let running = library.running.with(|all| all.contains(&game.id));
        let (play_label, play_icon) = if running { ("Jogando", Icon::Play) } else { crate::launch_action(&game) };
        let has_folder = !game.install_dir.is_empty();
        let (id, on) = (game.id.clone(), game.favorite);
        let page_id = game.id.clone();
        let folder_id = game.id.clone();
        let play = game.clone();
        Some(view! {
            <div class="ctx-catch" on:click=move |_| menu.close() on:contextmenu=move |e| { e.prevent_default(); menu.close(); }></div>
            <div class="ctx" role="menu" style:left=format!("{x}px") style:top=format!("{y}px")
                on:keydown=move |e| if e.key() == "Escape" { menu.close(); }>
                <span class="ctx-h">{game.name.clone()}</span>
                <button type="button" role="menuitem" disabled=running on:click=move |_| { menu.close(); crate::play_game(library, play.clone()); }>
                    <Svg icon=play_icon size=16 />{play_label}
                </button>
                <button type="button" role="menuitem" on:click=move |_| { menu.close(); library.go(Route::Game(page_id.clone())); }>
                    <Svg icon=Icon::Info size=16 />"Página do jogo"
                </button>
                <button type="button" role="menuitem" on:click=move |_| { menu.close(); crate::toggle_favorite(library, id.clone(), !on); }>
                    <Svg icon=if on { Icon::HeartFill } else { Icon::Heart } size=16 />
                    {if on { "Tirar dos favoritos" } else { "Adicionar aos favoritos" }}
                </button>
                {
                    let id = game.id.clone();
                    view! {
                        <button type="button" role="menuitem" on:click=move |_| {
                            menu.close();
                            let id = id.clone();
                            leptos::task::spawn_local(async move {
                                if library.report(api::create_shortcut(&id).await).is_some() {
                                    library.toast("Atalho criado na área de trabalho.");
                                }
                            });
                        }>
                            <Svg icon=Icon::External size=16 />"Criar atalho na área de trabalho"
                        </button>
                    }
                }
                {has_folder.then(|| view! {
                    <button type="button" role="menuitem" on:click=move |_| {
                        menu.close();
                        let id = folder_id.clone();
                        leptos::task::spawn_local(async move { library.report(api::open_folder(&id).await); });
                    }>
                        <Svg icon=Icon::Folder size=16 />"Abrir pasta do jogo"
                    </button>
                })}
            </div>
        })
    }
}
