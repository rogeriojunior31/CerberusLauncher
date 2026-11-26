//! The player's own word on a game: its status (playing, beaten...), a rating
//! in stars and notes. The card on the game page, the labels the Library's
//! filter uses, and the random pick.

use std::sync::atomic::{AtomicBool, Ordering};

use cerberus_shared::{Game, GameStatus, tr};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::art::{Poster, cover_sources};
use crate::icons::{Icon, Svg};
use crate::{Library, Route, api, format};

/// Set when a note changes, so the Library lists games again when opened
/// (refreshing at once would rebuild the game page being edited).
static CHANGED: AtomicBool = AtomicBool::new(false);

pub fn take_changed() -> bool {
    CHANGED.swap(false, Ordering::Relaxed)
}

pub fn label(status: GameStatus) -> &'static str {
    match status {
        GameStatus::Playing => tr!("Jogando"),
        GameStatus::PlanToPlay => tr!("Quero jogar"),
        GameStatus::Beaten => tr!("Zerado"),
        GameStatus::Completed => tr!("Completo"),
        GameStatus::OnHold => tr!("Em pausa"),
        GameStatus::Abandoned => tr!("Abandonado"),
    }
}

/// The CSS class for its color.
pub fn class(status: GameStatus) -> &'static str {
    match status {
        GameStatus::Playing => "st-playing",
        GameStatus::PlanToPlay => "st-plan",
        GameStatus::Beaten => "st-beaten",
        GameStatus::Completed => "st-completed",
        GameStatus::OnHold => "st-hold",
        GameStatus::Abandoned => "st-dropped",
    }
}

/// "Your game": status, stars and notes. Saved as they change.
#[component]
pub fn NoteCard(game: Game) -> impl IntoView {
    view! { <NoteCardFor id=game.id.clone() status=game.status rating=game.rating notes=game.notes.clone() /> }
}

/// The same card for anything with an id: a library game or a classic.
#[component]
pub fn NoteCardFor(id: String, status: Option<GameStatus>, rating: u8, notes: String) -> impl IntoView {
    let library = expect_context::<Library>();
    let id = StoredValue::new(id);
    let status = RwSignal::new(status);
    let rating = RwSignal::new(rating);
    let saved_notes = StoredValue::new(notes.clone());
    let notes = RwSignal::new(notes);
    let hover = RwSignal::new(0u8);
    let typing = StoredValue::new(None::<leptos::prelude::TimeoutHandle>);
    let save = move || {
        CHANGED.store(true, Ordering::Relaxed);
        let (Some(game_id), Some(now_status), Some(now_rating), Some(now_notes)) = (id.try_get_value(), status.try_get_untracked(), rating.try_get_untracked(), notes.try_get_untracked()) else {
            return;
        };
        // The game as listed carries it too: a page drawn again shows what was typed.
        library.patch_game(&game_id, |g| {
            g.status = now_status;
            g.rating = now_rating;
            g.notes = now_notes.clone();
        });
        spawn_local(async move {
            library.report(api::set_game_note(&game_id, now_status, now_rating, now_notes).await);
        });
    };
    let options = GameStatus::ALL
        .into_iter()
        .enumerate()
        .map(|(i, s)| view! { <option value=i.to_string() selected=move || status.get() == Some(s)>{label(s)}</option> })
        .collect_view();
    let chips = view! {
        <select class=move || format!("select st-select {}", status.get().map(class).unwrap_or(""))
            on:change=move |e| {
                status.set(event_target_value(&e).parse::<usize>().ok().and_then(|i| GameStatus::ALL.get(i).copied()));
                save();
            }>
            <option value="" selected=move || status.get().is_none()>{tr!("Sem situação")}</option>
            {options}
        </select>
    };
    let stars = (1..=5u8)
        .map(|n| {
            let lit = move || {
                let shown = if hover.get() > 0 { hover.get() } else { rating.get() };
                n <= shown
            };
            view! {
                <button type="button" class="star" class:lit=lit role="radio" aria-checked=move || (rating.get() == n).to_string() aria-label=tr!("{} de 5 estrelas", n)
                    on:mouseenter=move |_| hover.set(n) on:mouseleave=move |_| hover.set(0)
                    on:click=move |_| {
                        rating.update(|r| *r = if *r == n { 0 } else { n });
                        save();
                    }>
                    <Svg icon=Icon::Star size=22 />
                </button>
            }
        })
        .collect_view();
    view! {
        <section class="card rail-card note-card">
            <h3>{tr!("Seu jogo")}</h3>
            <label class="st-row"><span>{tr!("Situação")}</span>{chips}</label>
            <div class="stars" role="radiogroup" aria-label=tr!("Sua nota")>{stars}</div>
            <textarea class="input note-text" rows="3" placeholder=tr!("Anotações: onde parou, o que falta, dicas...") maxlength="4000"
                prop:value=move || notes.get() on:input=move |e| {
                    notes.set(event_target_value(&e));
                    // Kept 800 ms after the last key too, not only when leaving the box.
                    if let Some(Some(old)) = typing.try_get_value() {
                        old.clear();
                    }
                    let handle = leptos::prelude::set_timeout_with_handle(
                        move || {
                            if notes.try_get_untracked().is_some_and(|n| Some(n) != saved_notes.try_get_value()) {
                                if let Some(n) = notes.try_get_untracked() {
                                    let _ = saved_notes.try_set_value(n);
                                }
                                save();
                            }
                        },
                        std::time::Duration::from_millis(800),
                    )
                    .ok();
                    let _ = typing.try_set_value(handle);
                }
                on:blur=move |_| {
                    if notes.get_untracked() != saved_notes.get_value() {
                        saved_notes.set_value(notes.get_untracked());
                        save();
                    }
                }></textarea>
        </section>
    }
}

/// "Play something": one installed game at random from `pool`, with Play,
/// Pick another and its page.
#[component]
pub fn RandomPick(pool: Vec<Game>, open: RwSignal<bool>) -> impl IntoView {
    let library = expect_context::<Library>();
    let pool = StoredValue::new(pool);
    let pick = move || {
        pool.with_value(|all| {
            if all.is_empty() {
                return None;
            }
            let i = (js_sys::Math::random() * all.len() as f64) as usize;
            all.get(i.min(all.len() - 1)).cloned()
        })
    };
    let chosen = RwSignal::new(pick());
    // "Pick another" never shows the same game again when there's a choice.
    let pick_other = move || {
        let now = chosen.get_untracked().map(|g| g.id);
        let more_than_one = pool.with_value(|all| all.len() > 1);
        for _ in 0..8 {
            let next = pick();
            if !more_than_one || next.as_ref().map(|g| &g.id) != now.as_ref() {
                return next;
            }
        }
        pick()
    };
    // Esc closes it, as every dialog.
    let keys = window_event_listener(leptos::ev::keydown, move |e| {
        if e.key() == "Escape" && open.get_untracked() {
            open.set(false);
        }
    });
    on_cleanup(move || keys.remove());
    move || {
        let Some(g) = chosen.get() else {
            return view! {
                <div class="backdrop-dim" on:click=move |_| open.set(false)>
                    <div class="dialog" role="dialog" aria-modal="true" on:click=|e| e.stop_propagation()>
                        <h2>{tr!("Nada para sortear")}</h2>
                        <p class="muted">{tr!("Nenhum jogo instalado com esses filtros.")}</p>
                        <div class="dialog-footer"><button type="button" class="btn" on:click=move |_| open.set(false)>{tr!("Fechar")}</button></div>
                    </div>
                </div>
            }
            .into_any();
        };
        let (play_game, page) = (g.clone(), g.id.clone());
        let (label_text, icon) = crate::launch_action(&g);
        let played = if g.playtime_secs > 0 { tr!("{} jogados", format::playtime(g.playtime_secs)) } else { tr!("Você ainda não jogou").to_string() };
        view! {
            <div class="backdrop-dim" on:click=move |_| open.set(false)>
                <div class="dialog random-pick" role="dialog" aria-modal="true" on:click=|e| e.stop_propagation()>
                    <span class="kick">{tr!("Que tal jogar")}</span>
                    <div class="rp-body">
                        <div class="rp-cover"><Poster sources=cover_sources(&g) title=g.name.clone() /></div>
                        <div class="rp-info">
                            <h2>{g.name.clone()}</h2>
                            <small class="muted">{played}</small>
                            {g.status.map(|s| view! { <span class=format!("st-chip on {}", class(s))>{label(s)}</span> })}
                        </div>
                    </div>
                    <div class="dialog-footer">
                        <button type="button" class="btn" on:click=move |_| { open.set(false); library.go(Route::Game(page.clone())); }>{tr!("Ver página")}</button>
                        <button type="button" class="btn" on:click=move |_| chosen.set(pick_other())><Svg icon=Icon::Refresh size=16 />{tr!("Sortear outro")}</button>
                        <button type="button" class="btn main" on:click=move |_| { open.set(false); crate::play_game(library, play_game.clone()); }>
                            <Svg icon size=18 />{label_text}
                        </button>
                    </div>
                </div>
            </div>
        }
        .into_any()
    }
}
