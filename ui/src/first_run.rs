//! The first time Cerberus opens: a short setup, like a new console. How you
//! play, where your games are (and which of them have recomps), where recomps
//! install, and saves in the cloud. Every step can be changed later in
//! Configurações, and the whole setup can be skipped.

use cerberus_shared::{RecompEntry, SettingsView};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::art::{Poster, recomp_covers};
use crate::icons::{Icon, Svg};
use crate::{Library, Route, api, format::disk_size};

const STEPS: usize = 6;

/// One recomp per game the player has, most popular first, for the "found" covers.
fn found_games(library: Library) -> Vec<RecompEntry> {
    let mut found: Vec<RecompEntry> = library
        .recomps
        .get()
        .and_then(Result::ok)
        .unwrap_or_default()
        .into_iter()
        .filter(|e| e.owned_copy.is_some())
        .collect();
    found.sort_by_key(|e| std::cmp::Reverse(e.downloads));
    let mut seen = Vec::new();
    found.retain(|e| {
        let new = !seen.contains(&e.title);
        seen.push(e.title.clone());
        new
    });
    found
}

fn covers(list: Vec<RecompEntry>) -> impl IntoView {
    let tiles = list
        .into_iter()
        .take(6)
        .map(|e| view! { <div class="fr-cover" title=e.game.clone()><Poster sources=recomp_covers(&e) title=e.game.clone() /></div> })
        .collect_view();
    view! { <div class="fr-covers">{tiles}</div> }
}

#[component]
pub fn FirstRun(settings: SettingsView, open: RwSignal<bool>) -> impl IntoView {
    let library = expect_context::<Library>();
    let step = RwSignal::new(0usize);
    let tv = RwSignal::new(settings.start_in_tv);
    let state = RwSignal::new(settings);
    let busy = RwSignal::new(false);

    // Runs one settings change; the steps show the settings as they are after it.
    let change = move |task: std::pin::Pin<Box<dyn std::future::Future<Output = Result<SettingsView, String>>>>| {
        busy.set(true);
        spawn_local(async move {
            if let Some(next) = library.report(task.await) {
                state.set(next);
                library.recomps.refetch();
            }
            busy.set(false);
        });
    };

    let finish = move || {
        busy.set(true);
        spawn_local(async move {
            let done = library.report(api::finish_setup(tv.get_untracked()).await);
            busy.set(false);
            if done.is_some() {
                open.set(false);
                if tv.get_untracked() {
                    library.go(Route::BigPicture);
                } else if !found_games(library).is_empty() {
                    library.go(Route::Library);
                }
            }
        });
    };
    let next = move || {
        if step.get_untracked() + 1 >= STEPS {
            finish();
        } else {
            step.update(|s| *s += 1);
        }
    };

    let body = move || match step.get() {
        0 => view! {
            <img class="fr-logo" src="brand/cerberus-pixel-icone.svg" alt="" draggable="false" />
            <h1>"Bem-vindo ao Cerberus"</h1>
            <p class="fr-lead">"Seus jogos, recompilados para PC, numa biblioteca só. Um minuto de configuração e pronto."</p>
            <div class="fr-three">
                <div class="fr-card"><Svg icon=Icon::Search size=26 /><b>"Acha seus jogos"</b><span>"Aponte as pastas das suas ROMs e discos: ele mostra quais já têm recomp."</span></div>
                <div class="fr-card"><Svg icon=Icon::Download size=26 /><b>"Instala e atualiza"</b><span>"Direto da página de cada projeto, conferindo cada arquivo."</span></div>
                <div class="fr-card"><Svg icon=Icon::Settings size=26 /><b>"Do seu jeito"</b><span>"Ajustes, mods e backups de saves para cada jogo, na mesa ou na TV."</span></div>
            </div>
        }
        .into_any(),
        1 => {
            let pick = move |on_tv: bool| {
                view! {
                    <button type="button" role="radio" class="fr-pick" class:on=move || tv.get() == on_tv aria-checked=move || (tv.get() == on_tv).to_string()
                        on:click=move |_| tv.set(on_tv)>
                        <Svg icon=if on_tv { Icon::Tv } else { Icon::Library } size=40 />
                        <b>{if on_tv { "Na TV" } else { "Na mesa" }}</b>
                        <span>{if on_tv {
                            "Tela cheia, feita para controle. O Cerberus abre assim toda vez."
                        } else {
                            "Numa janela, com mouse e teclado. O Big Picture Mode fica a um clique."
                        }}</span>
                    </button>
                }
            };
            view! {
                <h1>"Como você vai jogar?"</h1>
                <p class="fr-lead">"Dá para trocar quando quiser."</p>
                <div class="fr-two" role="radiogroup" aria-label="Como você vai jogar?">{pick(false)}{pick(true)}</div>
            }
            .into_any()
        }
        2 => {
            let s = state.get();
            let dirs = s
                .rom_dirs
                .iter()
                .map(|d| {
                    let path = d.clone();
                    view! {
                        <div class="fr-dir">
                            <Svg icon=Icon::Folder size=18 /><span title=d.clone()>{d.clone()}</span>
                            <button type="button" class="btn sm" disabled=move || busy.get()
                                on:click=move |_| change(Box::pin(api::remove_rom_dir_owned(path.clone())))>"Remover"</button>
                        </div>
                    }
                })
                .collect_view();
            let found = found_games(library);
            let status = if busy.get() {
                view! { <p class="fr-status"><span class="spin"></span>"Procurando nas suas pastas..."</p> }.into_any()
            } else if !found.is_empty() {
                let text = if found.len() == 1 { "1 jogo seu já tem recomp".to_string() } else { format!("{} jogos seus já têm recomp", found.len()) };
                view! { <p class="fr-status ok"><Svg icon=Icon::Check size=18 />{text}</p>{covers(found)} }.into_any()
            } else if !s.rom_dirs.is_empty() {
                view! { <p class="fr-status">"Nenhum desses jogos tem recomp ainda. Quando aparecer, ele mostra no Descobrir."</p> }.into_any()
            } else {
                ().into_any()
            };
            let main = s.rom_dirs.is_empty();
            view! {
                <h1>"Onde ficam seus jogos?"</h1>
                <p class="fr-lead">"As pastas com suas ROMs, imagens de disco e backups. O Cerberus encontra os que têm recomp. Nada sai do seu PC."</p>
                <div class="fr-box">
                    {dirs}
                    <button type="button" class="btn" class:main=main disabled=move || busy.get()
                        on:click=move |_| change(Box::pin(api::add_rom_dir()))>
                        <Svg icon=Icon::Folder size=18 />{if main { "Adicionar pasta" } else { "Adicionar outra pasta" }}
                    </button>
                </div>
                {status}
            }
            .into_any()
        }
        3 => {
            let s = state.get();
            let free = s.install_free_bytes.map(|b| view! { <p class="fr-note">{format!("{} livres neste disco", disk_size(b))}</p> });
            view! {
                <h1>"Onde instalar os recomps?"</h1>
                <p class="fr-lead">"Cada recomp fica numa pasta própria aqui dentro. Dá para trocar depois em Configurações."</p>
                <div class="fr-box">
                    <div class="fr-dir big">
                        <Svg icon=Icon::Folder size=18 /><span title=s.recomps_dir.clone()>{s.recomps_dir.clone()}</span>
                        <button type="button" class="btn sm" disabled=move || busy.get()
                            on:click=move |_| change(Box::pin(api::pick_recomps_dir()))>"Trocar"</button>
                    </div>
                    {free}
                </div>
            }
            .into_any()
        }
        4 => {
            let s = state.get();
            let chosen = s.cloud_dir.clone().map(|d| {
                view! {
                    <div class="fr-dir">
                        <Svg icon=Icon::Check size=18 /><span title=d.clone()>{d.clone()}</span>
                        <button type="button" class="btn sm" disabled=move || busy.get()
                            on:click=move |_| change(Box::pin(api::set_cloud_dir(true)))>"Remover"</button>
                    </div>
                }
            });
            let has = s.cloud_dir.is_some();
            view! {
                <Svg icon=Icon::Saves size=44 />
                <h1>"Saves seguros em qualquer PC"</h1>
                <p class="fr-lead">"Escolha uma pasta que o OneDrive, Google Drive ou Dropbox sincroniza. Depois de cada partida, os saves vão para lá, e o outro PC os encontra antes de jogar."</p>
                <div class="fr-box">
                    {chosen}
                    <button type="button" class="btn" class:main=!has disabled=move || busy.get()
                        on:click=move |_| change(Box::pin(api::set_cloud_dir(false)))>
                        <Svg icon=Icon::Folder size=18 />{if has { "Trocar pasta" } else { "Escolher pasta sincronizada" }}
                    </button>
                </div>
                <p class="fr-note">"Opcional. Sem ela, os backups ficam só neste PC."</p>
            }
            .into_any()
        }
        _ => {
            let found = found_games(library);
            let waiting = found.iter().filter(|e| e.installed.is_none()).count();
            let lead = match (found.len(), waiting) {
                (0, _) => "Encontre algo para jogar no Descobrir.".to_string(),
                (1, 0) => "Seu jogo com recomp já está instalado. É só jogar.".to_string(),
                (n, 0) => format!("Seus {n} jogos com recomp já estão instalados. É só jogar."),
                (1, _) => "1 jogo seu tem recomp, pronto para instalar no Descobrir.".to_string(),
                (n, w) if w == n => format!("{n} jogos seus têm recomp, prontos para instalar no Descobrir."),
                (n, w) => format!("{n} jogos seus têm recomp. {w} ainda estão no Descobrir, prontos para instalar."),
            };
            view! {
                <img class="fr-logo done" src="brand/cerberus-pixel-icone.svg" alt="" draggable="false" />
                <h1>"Tudo pronto"</h1>
                <p class="fr-lead">{lead}</p>
                {(!found.is_empty()).then(|| covers(found))}
            }
            .into_any()
        }
    };

    let dots = move || {
        (0..STEPS)
            .map(|i| {
                let past = move || step.get() > i;
                view! { <i class:on=move || step.get() == i class:past=past></i> }
            })
            .collect_view()
    };
    let started = move || step.get() > 0;
    let next_label = move || match step.get() {
        0 => "Começar",
        s if s + 1 >= STEPS => "Ir para o Cerberus",
        _ => "Continuar",
    };

    view! {
        <div class="fr" role="dialog" aria-modal="true" aria-label="Configuração inicial"
            on:keydown=move |e| if e.key() == "Enter" && !busy.get_untracked() { next() }>
            <div class="fr-aurora" aria-hidden="true"><i></i><i></i><i></i></div>
            <div class="fr-top" data-tauri-drag-region>
                <div class="brand fr-brand" data-tauri-drag-region>
                    <img class="mark" src="brand/cerberus-pixel-icone.svg" alt="Cerberus Launcher" draggable="false" />
                    <img class="word" src="brand/cerberus-launcher-pixel.svg#wordmark" alt="" draggable="false" />
                </div>
                // The setup covers the window title bar, so it brings its own buttons.
                <div class="win">
                    <button type="button" aria-label="Minimizar" on:click=|_| api::window_action("minimize")>
                        <Svg icon=Icon::Minimize size=14 />
                    </button>
                    <button type="button" aria-label="Maximizar" on:click=|_| api::window_action("toggleMaximize")>
                        <Svg icon=Icon::Maximize size=14 />
                    </button>
                    <button type="button" class="close" aria-label="Fechar" on:click=|_| api::window_action("close")>
                        <Svg icon=Icon::Close size=14 />
                    </button>
                </div>
            </div>
            <div class="fr-stage">
                // A new element per step, so each one plays its entrance.
                {move || { let _ = step.get(); view! { <div class="fr-step">{body}</div> } }}
            </div>
            <div class="fr-foot">
                <div class="fr-side">
                    <Show when=started>
                        <button type="button" class="btn" on:click=move |_| step.update(|s| *s -= 1)>
                            <Svg icon=Icon::Back size=16 />"Voltar"
                        </button>
                    </Show>
                </div>
                <div class="fr-dots" aria-hidden="true">{dots}</div>
                <div class="fr-side end">
                    <Show when=move || { step.get() > 0 && step.get() + 1 < STEPS }>
                        <button type="button" class="link" disabled=move || busy.get() on:click=move |_| finish()>"Pular configuração"</button>
                    </Show>
                    <button type="button" class="btn main" disabled=move || busy.get() on:click=move |_| next()>{next_label}</button>
                </div>
            </div>
        </div>
    }
}
