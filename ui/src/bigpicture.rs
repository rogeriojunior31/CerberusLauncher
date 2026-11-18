//! Big Picture: the library full screen, for a TV and a controller.
//!
//! One row of big covers with the chosen game's art behind it. A controller
//! (A plays, B leaves, the D-pad or left stick moves) or the keyboard (arrows,
//! Enter, Esc) drives it. Controllers are read through the browser's Gamepad API,
//! polled a few times a second; a button counts once per press.

use std::time::Duration;

use cerberus_shared::Game;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

use crate::art::{Backdrop, Poster, cover_sources, hero_sources};
use crate::tiles::source_name;
use crate::{Library, Route, api, format, play_game};

#[derive(Clone, Copy, PartialEq)]
enum Input {
    Left,
    Right,
    Play,
    Leave,
}

/// Standard controller layout: 0 is A (Cross), 1 is B (Circle), 14/15 the D-pad's left/right.
fn pad_input() -> Option<Input> {
    let window = web_sys::window()?;
    let pads = window.navigator().get_gamepads().ok()?;
    for pad in pads.iter() {
        let Ok(pad) = pad.dyn_into::<web_sys::Gamepad>() else {
            continue;
        };
        let pressed = |i: u32| pad.buttons().get(i).dyn_into::<web_sys::GamepadButton>().is_ok_and(|b| b.pressed());
        let stick = pad.axes().get(0).as_f64().unwrap_or(0.0);
        if pressed(0) {
            return Some(Input::Play);
        }
        if pressed(1) {
            return Some(Input::Leave);
        }
        if pressed(14) || stick < -0.6 {
            return Some(Input::Left);
        }
        if pressed(15) || stick > 0.6 {
            return Some(Input::Right);
        }
    }
    None
}

/// Games worth putting on a TV: the ones played lately first, then the rest.
fn order(mut games: Vec<Game>) -> Vec<Game> {
    games.sort_by_key(|g| (std::cmp::Reverse(g.last_played), g.name.to_lowercase()));
    games
}

#[component]
pub fn BigPicture() -> impl IntoView {
    let library = expect_context::<Library>();
    let selected = RwSignal::new(0usize);
    let games = move || order(library.games.get().and_then(Result::ok).unwrap_or_default());

    api::window_toggle("setFullscreen", true);

    let act = move |input: Input| {
        let list = games();
        if list.is_empty() {
            if input == Input::Leave {
                library.go(Route::Home);
            }
            return;
        }
        let last = list.len() - 1;
        match input {
            Input::Left => selected.update(|i| *i = i.saturating_sub(1)),
            Input::Right => selected.update(|i| *i = (*i + 1).min(last)),
            Input::Play => play_game(library, list[selected.get_untracked().min(last)].clone()),
            Input::Leave => library.go(Route::Home),
        }
    };

    // Keyboard.
    let on_key = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |event: web_sys::KeyboardEvent| {
        let input = match event.key().as_str() {
            "ArrowLeft" => Input::Left,
            "ArrowRight" => Input::Right,
            "Enter" | " " => Input::Play,
            "Escape" | "Backspace" => Input::Leave,
            _ => return,
        };
        event.prevent_default();
        act(input);
    });
    let window = web_sys::window().expect("a window");
    let _ = window.add_event_listener_with_callback("keydown", on_key.as_ref().unchecked_ref());
    let listener = StoredValue::new_local(Some(on_key));

    // Controllers: an input is acted on when it starts, and held left/right repeats slowly.
    let held = StoredValue::new_local((None::<Input>, 0u32));
    let poll = set_interval_with_handle(
        move || {
            let now = pad_input();
            let (before, ticks) = held.get_value();
            let repeat = now.is_some() && now == before && matches!(now, Some(Input::Left | Input::Right)) && ticks >= 4;
            if let Some(input) = now.filter(|_| now != before || repeat) {
                act(input);
            }
            let ticks = if now == before && !repeat { ticks + 1 } else { 0 };
            held.set_value((now, ticks));
        },
        Duration::from_millis(60),
    );

    on_cleanup(move || {
        if let Ok(handle) = poll {
            handle.clear();
        }
        listener.update_value(|slot| {
            let Some(on_key) = slot.take() else {
                return;
            };
            if let Some(window) = web_sys::window() {
                let _ = window.remove_event_listener_with_callback("keydown", on_key.as_ref().unchecked_ref());
            }
        });
        api::window_toggle("setFullscreen", false);
    });

    let backdrop = move || {
        let list = games();
        let game = list.get(selected.get())?.clone();
        Some(view! { <Backdrop sources=hero_sources(&game) /> })
    };
    let row = move || {
        let list = games();
        let current = selected.get();
        // Only the covers around the chosen one, so the row stays centered on it.
        let start = current.saturating_sub(3);
        list.into_iter()
            .enumerate()
            .skip(start)
            .take(8)
            .map(|(index, game)| {
                let on = index == current;
                view! {
                    <button type="button" class="bp-tile" class:on=on on:click=move |_| selected.set(index)>
                        <Poster sources=cover_sources(&game) title=game.name.clone() />
                    </button>
                }
            })
            .collect_view()
    };
    let info = move || {
        let list = games();
        let game = list.get(selected.get())?.clone();
        let played = game.last_played.map(|t| format!("Jogado {}", format::ago(t))).unwrap_or_else(|| source_name(&game.source).to_string());
        Some(view! {
            <div class="bp-info">
                <h1>{game.name.clone()}</h1>
                <span class="muted">{played}</span>
            </div>
        })
    };

    view! {
        <div class="bp">
            <div class="bp-bg">{backdrop}</div>
            <div class="bp-shade"></div>
            <header class="bp-top">
                <span class="mark">"C"</span>
                <span class="word">"Cerberus"</span>
            </header>
            {info}
            <div class="bp-row">{row}</div>
            <footer class="bp-hints">
                <span><b class="key">"A"</b>" Jogar"</span>
                <span><b class="key">"◀ ▶"</b>" Navegar"</span>
                <span><b class="key">"B"</b>" Sair"</span>
            </footer>
        </div>
    }
}
