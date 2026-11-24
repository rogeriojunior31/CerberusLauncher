//! The menu over a game (Ctrl+F12): its own see-through window, laid over the
//! game's. Esc or Ctrl+F12 gives the game back.

use cerberus_shared::tr;
use cerberus_shared::OverlayView;
use wasm_bindgen::JsCast;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::art::{Poster, cover_sources};
use crate::format;
use crate::icons::{Icon, Svg};

/// Whether this window is the menu over a game, not the launcher.
pub fn is_overlay() -> bool {
    js_sys::Reflect::get(&window(), &"__CERBERUS_OVERLAY".into()).is_ok_and(|v| v.is_truthy())
}

fn clock() -> String {
    let now = js_sys::Date::new_0();
    format!("{:02}:{:02}", now.get_hours(), now.get_minutes())
}

/// Moves the keyboard to the next (or previous) button of the menu.
fn move_focus(step: i32) {
    let Ok(found) = document().query_selector_all(".gm button:not([disabled])") else { return };
    let buttons: Vec<web_sys::HtmlElement> = (0..found.length()).filter_map(|i| found.item(i)?.dyn_into().ok()).collect();
    if buttons.is_empty() {
        return;
    }
    let active = document().active_element();
    let at = buttons.iter().position(|b| active.as_ref().is_some_and(|a| a == b.as_ref() as &web_sys::Element));
    let next = match at {
        Some(i) => (i as i32 + step).clamp(0, buttons.len() as i32 - 1) as usize,
        None => 0,
    };
    let _ = buttons[next].focus();
}

#[component]
pub fn OverlayMenu() -> impl IntoView {
    if let Some(root) = document().document_element() {
        let _ = root.class_list().add_1("overlay-mode");
    }
    let state = LocalResource::new(api::overlay_state);
    let problem = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);
    let quitting = RwSignal::new(false);
    // Seconds since the menu last asked, so the play time keeps counting.
    let ticks = RwSignal::new(0u64);
    let now = RwSignal::new(clock());
    let back_ref = NodeRef::<leptos::html::Button>::new();

    api::on_overlay_opened(move || {
        problem.set(None);
        quitting.set(false);
        ticks.set(0);
        state.refetch();
    });
    api::on_screenshot(move |_: String| state.refetch());
    let timer = set_interval_with_handle(
        move || {
            ticks.update(|t| *t += 1);
            now.set(clock());
        },
        std::time::Duration::from_secs(1),
    );
    on_cleanup(move || {
        if let Ok(t) = timer {
            t.clear();
        }
    });
    // The keyboard starts on "Voltar ao jogo", and on "Cancelar" when asked to close the game.
    let cancel_ref = NodeRef::<leptos::html::Button>::new();
    Effect::new(move |_| {
        let Some(Ok(_)) = state.get() else { return };
        let target = if quitting.get() { cancel_ref.get() } else { back_ref.get() };
        if let Some(button) = target {
            let _ = button.focus();
        }
    });

    let back = move || spawn_local(async move { let _ = api::overlay_close().await; });
    let run = move |task: std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>>>>| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        problem.set(None);
        spawn_local(async move {
            if let Err(e) = task.await {
                problem.set(Some(e));
            }
            busy.set(false);
        });
    };
    // Controller and keyboard: directions move between the buttons, A (Enter)
    // presses, B (Esc) steps back. Menu isn't used: it may still be held from
    // the View+Menu that opened this.
    let pad_kind = RwSignal::new(crate::tv::input::PadKind::default());
    crate::tv::input::listen(
        move |btn| {
            use crate::tv::input::Btn;
            match btn {
                Btn::B => {
                    if quitting.get_untracked() {
                        quitting.set(false);
                    } else {
                        back();
                    }
                }
                Btn::A => {
                    let focused = document().active_element().and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok());
                    if let Some(el) = focused {
                        el.click();
                    }
                }
                Btn::Left | Btn::Up => move_focus(-1),
                Btn::Right | Btn::Down => move_focus(1),
                _ => {}
            }
        },
        pad_kind,
    );

    view! {
        <div class="gm">
            {move || match state.get() {
                None => view! { <p class="gm-wait">{tr!("Carregando...")}</p> }.into_any(),
                Some(Err(e)) => view! {
                    <div class="gm-panel">
                        <p class="problem">{e}</p>
                        <button type="button" class="btn" node_ref=back_ref on:click=move |_| back()>{tr!("Voltar")}</button>
                    </div>
                }.into_any(),
                Some(Ok(OverlayView { game, session_secs, shots })) => {
                    let name = game.name.clone();
                    let total = game.playtime_secs;
                    view! {
                        <header class="gm-top">
                            <div class="gm-game">
                                <div class="gm-cover"><Poster sources=cover_sources(&game) title=name.clone() /></div>
                                <div>
                                    <h1>{name.clone()}</h1>
                                    <p class="muted">
                                        {move || tr!("Jogando há {}", format::session_duration(session_secs + ticks.get()))}
                                        {(total > 0).then(|| tr!(" · {} no total", format::playtime(total)))}
                                    </p>
                                </div>
                            </div>
                            <span class="gm-clock">{move || now.get()}</span>
                        </header>
                        <div class="gm-panel">
                            <Show when=move || !quitting.get() fallback=move || view! {
                                <div class="gm-confirm" role="alertdialog" aria-label=tr!("Fechar o jogo")>
                                    <p>{tr!("Fechar o jogo? O que não foi salvo nele se perde.")}</p>
                                    <div class="gm-row">
                                        <button type="button" class="btn warn" disabled=move || busy.get()
                                            on:click=move |_| run(Box::pin(api::overlay_quit_game()))>
                                            <Svg icon=Icon::Close size=18 />{tr!("Fechar o jogo")}
                                        </button>
                                        <button type="button" class="btn" node_ref=cancel_ref on:click=move |_| quitting.set(false)>{tr!("Cancelar")}</button>
                                    </div>
                                </div>
                            }>
                                <div class="gm-actions">
                                    <button type="button" class="gm-act primary" node_ref=back_ref on:click=move |_| back()>
                                        <Svg icon=Icon::Play size=26 /><span>{tr!("Voltar ao jogo")}</span>
                                    </button>
                                    <button type="button" class="gm-act" disabled=move || busy.get()
                                        on:click=move |_| run(Box::pin(api::overlay_screenshot()))>
                                        <Svg icon=Icon::Photo size=26 /><span>{tr!("Tirar foto")}</span>
                                    </button>
                                    <button type="button" class="gm-act" disabled=move || busy.get()
                                        on:click=move |_| run(Box::pin(api::overlay_open_launcher()))>
                                        <Svg icon=Icon::External size=26 /><span>{tr!("Abrir no Cerberus")}</span>
                                    </button>
                                    <button type="button" class="gm-act" on:click=move |_| quitting.set(true)>
                                        <Svg icon=Icon::Close size=26 /><span>{tr!("Fechar o jogo")}</span>
                                    </button>
                                </div>
                            </Show>
                            {move || problem.get().map(|e| view! { <p class="problem">{e}</p> })}
                            {(!shots.is_empty()).then(|| view! {
                                <div class="gm-shots" aria-label=tr!("Últimas fotos")>
                                    {shots.iter().map(|s| view! {
                                        <img src=api::convert_file_src(&s.path) alt=tr!("Foto de {}", format::ago(s.taken)) />
                                    }).collect_view()}
                                </div>
                            })}
                        </div>
                        <p class="gm-hint">{tr!("Esc, B ou Ctrl+F12 volta ao jogo · no controle, View+Menu (ou segurar View) abre e fecha, View+RB tira foto · F12 tira foto sem abrir o menu")}</p>
                    }.into_any()
                }
            }}
        </div>
    }
}
