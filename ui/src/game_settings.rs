//! The Ajustes tab: the game's own settings (full screen, resolution, frame
//! rate...), changed here and written to the game's settings file.

use cerberus_shared::{GameSetting, GameSettings};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::icons::{Icon, Svg};
use crate::{Library, api};

/// Up to this many choices show as buttons side by side; more, as a list.
const AS_BUTTONS: usize = 4;

#[component]
pub fn GameSettingsTab(game_id: String) -> impl IntoView {
    let library = expect_context::<Library>();
    let id = StoredValue::new(game_id);
    let state = RwSignal::new(None::<Result<Option<GameSettings>, String>>);
    let busy = RwSignal::new(false);
    let advanced = RwSignal::new(false);
    let armed_reset = RwSignal::new(false);

    spawn_local(async move {
        state.set(Some(api::game_settings(&id.get_value()).await));
    });

    // One change at a time; the reply is the settings as the file now has them.
    let apply = move |setting: Option<String>, choice: Option<usize>| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        spawn_local(async move {
            let reset = setting.is_none();
            let result = api::set_game_setting(&id.get_value(), setting, choice).await;
            busy.set(false);
            armed_reset.set(false);
            if let Some(now) = library.report(result) {
                state.set(Some(Ok(Some(now))));
                if reset {
                    library.toast("Ajustes de volta ao padrão do jogo.");
                }
            }
        });
    };

    let running = move || library.running.with(|all| all.contains(&id.get_value()));

    move || match state.get() {
        None => view! { <p class="muted">"Lendo os ajustes do jogo..."</p> }.into_any(),
        Some(Err(error)) => view! { <p class="problem">{error}</p> }.into_any(),
        Some(Ok(None)) => view! {
            <div class="empty small">
                <Svg icon=Icon::Settings size=28 />
                <h2>"Sem ajustes no catálogo"</h2>
                <p class="muted">"Esse jogo muda as opções no próprio menu."</p>
            </div>
        }
        .into_any(),
        Some(Ok(Some(all))) => {
            let has_advanced = all.settings.iter().any(|s| s.advanced);
            let files = all.files.join(" · ");
            let shown: Vec<GameSetting> = all.settings.into_iter().filter(|s| advanced.get() || !s.advanced).collect();
            let mut last_group = String::new();
            let rows = shown
                .into_iter()
                .map(|s| {
                    let heading = (s.group != last_group && !s.group.is_empty()).then(|| view! { <h4 class="set-group">{s.group.clone()}</h4> });
                    last_group = s.group.clone();
                    view! { {heading} <SettingRow setting=s busy apply=apply /> }
                })
                .collect_view();
            let reset_label = move || if armed_reset.get() { "Clique de novo para confirmar" } else { "Restaurar padrão" };
            let advanced_toggle = has_advanced.then(|| {
                view! {
                    <button type="button" class="chip-btn" class:on=move || advanced.get() aria-pressed=move || advanced.get().to_string()
                        on:click=move |_| advanced.update(|a| *a = !*a)>"Avançados"</button>
                }
            });
            view! {
                <section class="card settings">
                    <div class="card-h">
                        <div>
                            <h3>"Ajustes do jogo"</h3>
                            <p class="muted small">"Gravados direto no arquivo de configuração do jogo. Valem na próxima vez que ele abrir."</p>
                        </div>
                        <div class="row">
                            {advanced_toggle}
                            <button type="button" class="btn sm" disabled=move || busy.get() || running()
                                on:click=move |_| if armed_reset.get_untracked() { apply(None, None) } else { armed_reset.set(true) }>
                                {reset_label}
                            </button>
                        </div>
                    </div>
                    <Show when=running>
                        <p class="problem small">"Feche o jogo para mudar os ajustes: ele grava os próprios ajustes ao fechar."</p>
                    </Show>
                    <div class="set-list" class:locked=running>{rows}</div>
                    <p class="muted small path">{files}</p>
                </section>
            }
            .into_any()
        }
    }
}

#[component]
fn SettingRow<F>(setting: GameSetting, busy: RwSignal<bool>, apply: F) -> impl IntoView
where
    F: Fn(Option<String>, Option<usize>) + Copy + Send + Sync + 'static,
{
    let id = StoredValue::new(setting.id.clone());
    let current = setting.current;
    let applies = setting.applies;
    let control = if setting.choices.len() <= AS_BUTTONS {
        let buttons = setting
            .choices
            .iter()
            .enumerate()
            .map(|(i, label)| {
                view! {
                    <button type="button" class="seg" class:on=i == current aria-pressed=(i == current).to_string()
                        disabled=move || busy.get() || !applies
                        on:click=move |_| if i != current { apply(Some(id.get_value()), Some(i)) }>
                        {label.clone()}
                    </button>
                }
            })
            .collect_view();
        view! { <div class="segs" role="group" aria-label=setting.label.clone()>{buttons}</div> }.into_any()
    } else {
        let options = setting
            .choices
            .iter()
            .enumerate()
            .map(|(i, label)| view! { <option value=i.to_string() selected=i == current>{label.clone()}</option> })
            .collect_view();
        view! {
            <select class="select" aria-label=setting.label.clone() disabled=move || busy.get() || !applies
                on:change=move |e| {
                    if let Ok(i) = event_target_value(&e).parse::<usize>() {
                        apply(Some(id.get_value()), Some(i));
                    }
                }>
                {options}
            </select>
        }
        .into_any()
    };
    let help = (!setting.help.is_empty()).then(|| view! { <small>{setting.help.clone()}</small> });
    let off = (!applies).then(|| view! { <small class="dim">"Não se aplica com os ajustes atuais."</small> });
    view! {
        <div class="set-row" class:off=!applies>
            <div class="set-text"><b>{setting.label}</b>{help}{off}</div>
            {control}
        </div>
    }
}
