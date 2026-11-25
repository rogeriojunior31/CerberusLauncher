//! Filtros visuais: a picture filter for a game (sharper, livelier colours, an
//! old TV, film), applied by ReShade while the game runs. The tiles preview each
//! look on the game's own picture with CSS, close to what the shader does.

use cerberus_shared::{tk, tr};
use cerberus_shared::{Look, LookStatus};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::icons::{Icon, Svg};
use crate::{Library, api};

const LOOKS: [(&str, &str, &str); 4] = [
    ("sharp", tk!("Nítido"), tk!("Realça os detalhes e as bordas.")),
    ("vivid", tk!("Cores vivas"), tk!("Mais saturação e contraste.")),
    ("crt", "CRT", tk!("Linhas e tela curva de TV antiga.")),
    ("film", tk!("Filme"), tk!("Tons quentes e cantos escuros.")),
];
const DEFAULT_STRENGTH: u8 = 60;

#[component]
pub fn LooksCard(game_id: String, preview: Option<String>) -> impl IntoView {
    let library = expect_context::<Library>();
    let id = StoredValue::new(game_id);
    let status = RwSignal::new(None::<LookStatus>);
    let busy = RwSignal::new(None::<&'static str>);
    let strength = RwSignal::new(DEFAULT_STRENGTH);

    let load = move || {
        spawn_local(async move {
            if let Some(s) = library.report(api::look_status(&id.get_value()).await) {
                if let Some(look) = &s.current {
                    strength.set(look.strength);
                }
                status.set(Some(s));
            }
        });
    };
    load();

    let choose = move |look: Option<Look>| {
        let first = look.is_some() && status.get_untracked().is_some_and(|s| s.current.is_none());
        busy.set(Some(if first { tr!("Preparando o filtro (na primeira vez o Cerberus baixa o ReShade, uns 5 MB)...") } else { tr!("Salvando...") }));
        spawn_local(async move {
            let result = api::set_look(&id.get_value(), look).await;
            busy.set(None);
            if let Some(s) = library.report(result) {
                status.set(Some(s));
                library.games.refetch();
            }
        });
    };
    let current_id = move || status.get().and_then(|s| s.current).map(|l| l.id);
    let running = move || library.running.with(|all| all.contains(&id.get_value()));

    move || {
        let Some(s) = status.get() else {
            return view! { <section class="card looks"><crate::game_overview::Waiting text=tr!("Carregando os filtros...") rows=2 retry=Callback::new(move |_| load()) /></section> }.into_any();
        };
        if let Some(why) = s.unsupported.clone() {
            return view! {
                // Not for this game: one quiet line at the end, not a card that leads the tab.
                <p class="muted small looks-off"><b>{tr!("Filtros visuais")}</b>{" · "}{why}</p>
            }
            .into_any();
        }
        let image = preview.clone();
        let tile = move |look: Option<(&'static str, &'static str, &'static str)>| {
            let look_id = look.map(|(id, _, _)| id);
            let on = move || current_id().as_deref() == look_id;
            let picture = image.clone().map(|src| view! { <img src=src alt="" draggable="false" /> });
            let (name, about) = look.map_or((tr!("Sem filtro"), tr!("O jogo como ele é.")), |(_, n, a)| (n, a));
            let class = format!("lk-tile lk-{}", look_id.unwrap_or("none"));
            view! {
                <button type="button" class=class class:on=on aria-pressed=move || on().to_string()
                    disabled=move || busy.get().is_some()
                    style=move || format!("--s:{}", f32::from(strength.get()) / 100.0)
                    on:click=move |_| {
                        if !on() {
                            choose(look_id.map(|id| Look { id: id.to_string(), strength: strength.get_untracked() }));
                        }
                    }>
                    <div class="lk-pic">{picture}</div>
                    <b>{name}</b>
                    <small>{about}</small>
                </button>
            }
        };
        let tiles = view! { {tile(None)} {LOOKS.iter().map(|l| tile(Some((l.0, cerberus_shared::i18n::t(l.1), cerberus_shared::i18n::t(l.2))))).collect_view()} };
        let has_look = s.current.is_some();
        view! {
            <section class="card looks">
                <div class="card-h">
                    <div>
                        <h3>{tr!("Filtros visuais")}</h3>
                        <p class="muted small">{tr!("Aplicados pelo ReShade enquanto o jogo roda. Nada fica na pasta do jogo depois que ele fecha.")}</p>
                    </div>
                </div>
                <div class="lk-grid">{tiles}</div>
                <Show when=move || has_look>
                    <label class="cz-zoom lk-strength">
                        <span>{tr!("Intensidade")}</span>
                        <input type="range" min="10" max="100" step="5" prop:value=move || strength.get().to_string()
                            disabled=move || busy.get().is_some()
                            on:input=move |e| { if let Ok(v) = event_target_value(&e).parse::<u8>() { strength.set(v) } }
                            on:change=move |_| {
                                if let Some(id) = current_id() {
                                    choose(Some(Look { id, strength: strength.get_untracked() }));
                                }
                            } />
                        <span class="gs-value">{move || format!("{}%", strength.get())}</span>
                    </label>
                </Show>
                {move || busy.get().map(|text| view! { <p class="fr-status"><span class="spin"></span>{text}</p> })}
                <Show when=running>
                    <p class="muted small">{tr!("Mudanças valem na próxima vez que o jogo abrir.")}</p>
                </Show>
                <p class="muted small lk-tip"><Svg icon=Icon::Settings size=14 />{tr!("No jogo, a tecla Home abre o menu do ReShade para ajustes finos.")}</p>
            </section>
        }
        .into_any()
    }
}
