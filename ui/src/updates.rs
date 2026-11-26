//! Updating Cerberus: the notice in the sidebar when a newer version is out,
//! the window with its notes and the Update button, and "What's new" once
//! after an update.

use cerberus_shared::{AppUpdate, tr};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::icons::{Icon, Svg};
use crate::{Library, api, format};

/// What the app knows about updates, shared by the sidebar, the window and Settings.
#[derive(Clone, Copy)]
pub struct Updates {
    pub found: RwSignal<Option<AppUpdate>>,
    pub open: RwSignal<bool>,
    /// Bytes downloaded and total, while updating.
    pub progress: RwSignal<Option<(u64, u64)>>,
    pub whats_new: RwSignal<Option<AppUpdate>>,
}

impl Updates {
    pub fn start() -> Self {
        let updates = Self { found: RwSignal::new(None), open: RwSignal::new(false), progress: RwSignal::new(None), whats_new: RwSignal::new(None) };
        api::on_update_available(move |u| updates.found.set(Some(u)));
        api::on_update_progress(move |p| updates.progress.set(Some(p)));
        spawn_local(async move {
            if let Ok(Some(u)) = api::update_found().await {
                updates.found.set(Some(u));
            }
            if let Ok(Some(u)) = api::whats_new().await {
                updates.whats_new.set(Some(u));
            }
        });
        updates
    }
}

/// Release notes in Markdown as plain blocks: headings, list items, paragraphs.
/// Text only: nothing from the notes is put in the page as HTML.
pub fn notes_view(markdown: &str) -> impl IntoView + use<> {
    let clean = |s: &str| s.replace("**", "").replace('`', "").trim().to_string();
    let mut blocks = Vec::new();
    for line in markdown.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(h) = t.strip_prefix("### ").or_else(|| t.strip_prefix("## ")).or_else(|| t.strip_prefix("# ")) {
            blocks.push(view! { <h4>{clean(h)}</h4> }.into_any());
        } else if let Some(item) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            blocks.push(view! { <p class="li">{clean(item)}</p> }.into_any());
        } else {
            blocks.push(view! { <p>{clean(t)}</p> }.into_any());
        }
    }
    view! { <div class="about-body notes">{blocks}</div> }
}

/// The sidebar notice; opens the update window.
#[component]
pub fn UpdateNotice() -> impl IntoView {
    let updates = expect_context::<Updates>();
    move || {
        updates.found.get().map(|u| {
            view! {
                <button type="button" class="nav update-nav" on:click=move |_| updates.open.set(true)>
                    <Svg icon=Icon::Download />
                    <span>{tr!("Nova versão {}", u.version)}</span>
                </button>
            }
        })
    }
}

/// The update window: notes, size, Update now with progress.
#[component]
pub fn UpdateDialog() -> impl IntoView {
    let library = expect_context::<Library>();
    let updates = expect_context::<Updates>();
    let busy = RwSignal::new(false);
    let install = move |_| {
        busy.set(true);
        updates.progress.set(Some((0, 0)));
        spawn_local(async move {
            // On success Cerberus closes and the installer takes over.
            let result = api::update_install().await;
            busy.set(false);
            updates.progress.set(None);
            library.report(result);
        });
    };
    // Esc closes it (not while installing), and the main button takes the focus as it opens.
    let main_button = NodeRef::<leptos::html::Button>::new();
    let keys = window_event_listener(leptos::ev::keydown, move |e| {
        if e.key() == "Escape" && updates.open.get_untracked() && !busy.get_untracked() {
            updates.open.set(false);
        }
    });
    on_cleanup(move || keys.remove());
    Effect::new(move |_| {
        if updates.open.get() {
            request_animation_frame(move || {
                if let Some(b) = main_button.get_untracked() {
                    let _ = b.focus();
                }
            });
        }
    });
    move || {
        let (true, Some(u)) = (updates.open.get(), updates.found.get()) else { return ().into_any() };
        let date = u.published.split('T').next().unwrap_or_default().to_string();
        view! {
            <div class="backdrop-dim" on:click=move |_| if !busy.get_untracked() { updates.open.set(false) }>
                <div class="dialog update-dialog" role="dialog" aria-modal="true" aria-labelledby="upd-title" on:click=|e| e.stop_propagation()>
                    <div class="upd-h">
                        <img src="brand/cerberus-pixel-icone.svg" alt="" />
                        <div>
                            <h2 id="upd-title">{tr!("Cerberus {} está disponível", u.version)}</h2>
                            <small class="muted">{tr!("Você tem a {} · {} · {}", api::app_version(), date, format::megabytes(u.size))}</small>
                        </div>
                    </div>
                    <div class="upd-notes">{notes_view(&u.notes)}</div>
                    {move || updates.progress.get().filter(|_| busy.get()).map(|(done, total)| {
                        let pct = if total == 0 { 0 } else { done * 100 / total };
                        view! {
                            <div class="upd-progress" role="progressbar" aria-valuenow=pct.to_string()>
                                <div class="ach-bar"><i style=format!("width: {pct}%")></i></div>
                                <small class="muted">{if pct >= 100 { tr!("Conferindo e instalando… o Cerberus vai fechar e abrir de novo.").to_string() } else { tr!("Baixando {}%", pct) }}</small>
                            </div>
                        }
                    })}
                    <p class="muted small">{tr!("O instalador vem do GitHub do Cerberus e é conferido antes de instalar. Seus jogos, saves e configurações ficam como estão.")}</p>
                    <div class="dialog-footer">
                        <button type="button" class="btn" disabled=move || busy.get() on:click=move |_| updates.open.set(false)>{tr!("Depois")}</button>
                        <button type="button" class="btn main" node_ref=main_button disabled=move || busy.get() on:click=install>
                            <Svg icon=Icon::Download size=18 />{move || if busy.get() { tr!("Atualizando…") } else { tr!("Atualizar agora") }}
                        </button>
                    </div>
                </div>
            </div>
        }
        .into_any()
    }
}

/// "What's new" once after an update.
#[component]
pub fn WhatsNewDialog() -> impl IntoView {
    let updates = expect_context::<Updates>();
    let close = move || {
        updates.whats_new.set(None);
        spawn_local(async move {
            let _ = api::whats_new_seen().await;
        });
    };
    let main_button = NodeRef::<leptos::html::Button>::new();
    let keys = window_event_listener(leptos::ev::keydown, move |e| {
        if e.key() == "Escape" && updates.whats_new.with_untracked(Option::is_some) {
            close();
        }
    });
    on_cleanup(move || keys.remove());
    Effect::new(move |_| {
        if updates.whats_new.with(Option::is_some) {
            request_animation_frame(move || {
                if let Some(b) = main_button.get_untracked() {
                    let _ = b.focus();
                }
            });
        }
    });
    move || {
        updates.whats_new.get().map(|u| {
            view! {
                <div class="backdrop-dim" on:click=move |_| close()>
                    <div class="dialog update-dialog" role="dialog" aria-modal="true" aria-labelledby="new-title" on:click=|e| e.stop_propagation()>
                        <div class="upd-h">
                            <img src="brand/cerberus-pixel-icone.svg" alt="" />
                            <div>
                                <h2 id="new-title">{tr!("O que há de novo na {}", u.version)}</h2>
                                <small class="muted">{tr!("O Cerberus foi atualizado.")}</small>
                            </div>
                        </div>
                        <div class="upd-notes">{notes_view(&u.notes)}</div>
                        <div class="dialog-footer">
                            <button type="button" class="btn main" node_ref=main_button on:click=move |_| close()>{tr!("Entendi")}</button>
                        </div>
                    </div>
                </div>
            }
        })
    }
}
