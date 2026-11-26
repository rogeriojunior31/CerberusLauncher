//! The first time Cerberus opens: a short setup, like a new console. How you
//! play, where your games are (and which of them have recomps), where recomps
//! install, and saves in the cloud. Every step can be changed later in
//! Configurações, and the whole setup can be skipped.

use cerberus_shared::tr;
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

/// What else Cerberus found on this PC on its own: Steam and Epic games already
/// in the library, and RetroArch for the classics. Nothing found, nothing shown.
#[component]
fn OnThisPc() -> impl IntoView {
    let library = expect_context::<Library>();
    let emulation = LocalResource::new(api::emulation_status);
    move || {
        let (steam, epic) = library.games.with(|g| {
            g.as_ref().and_then(|r| r.as_ref().ok()).map_or((0, 0), |list| {
                let count = |f: fn(&cerberus_shared::Source) -> bool| list.iter().filter(|g| f(&g.source)).count();
                (count(|s| matches!(s, cerberus_shared::Source::Steam { .. })), count(|s| matches!(s, cerberus_shared::Source::Epic { .. })))
            })
        });
        let retroarch = emulation.get().and_then(Result::ok).is_some_and(|v| v.retroarch.is_some());
        let games = |n: usize| if n == 1 { tr!("1 jogo").to_string() } else { tr!("{} jogos", n) };
        let mut found = Vec::new();
        if steam > 0 {
            found.push(("Steam", games(steam)));
        }
        if epic > 0 {
            found.push(("Epic Games", games(epic)));
        }
        if retroarch {
            found.push(("RetroArch", tr!("para os clássicos").to_string()));
        }
        if found.is_empty() {
            return ().into_any();
        }
        let chips = found
            .into_iter()
            .map(|(name, what)| view! { <span class="fr-chip"><Svg icon=Icon::Check size=16 /><b>{name}</b><small>{what}</small></span> })
            .collect_view();
        view! {
            <div class="fr-found">
                <p class="fr-note">{tr!("Também encontrado neste PC")}</p>
                <div class="fr-chips">{chips}</div>
            </div>
        }
        .into_any()
    }
}

/// The language first, so the rest of the setup reads right. Each language is
/// written in itself; choosing one reloads the window in it.
fn language_pick() -> impl IntoView {
    use cerberus_shared::i18n::Lang;
    let chosen = crate::i18n::choice().filter(|c| c != "auto");
    let pick = |code: Option<&'static str>, label: String| {
        let on = chosen.as_deref() == code;
        view! {
            <button type="button" role="radio" class="fr-lang" class:on=on aria-checked=on.to_string()
                on:click=move |_| spawn_local(async move {
                    let _ = api::set_language(code.map(str::to_string)).await;
                    crate::i18n::choose(code);
                })>{label}</button>
        }
    };
    let auto = tr!("Automático ({})", crate::i18n::system().name());
    view! {
        <div class="fr-langs" role="radiogroup" aria-label=tr!("Idioma")>
            <span class="fr-langs-h"><Svg icon=Icon::Discover size=16 />{tr!("Idioma")}</span>
            {pick(None, auto)}
            {Lang::ALL.iter().map(|l| pick(Some(l.code()), l.name().to_string())).collect_view()}
        </div>
    }
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
            <h1>{tr!("Bem-vindo ao Cerberus")}</h1>
            <p class="fr-lead">{tr!("Seus jogos, recompilados para PC, numa biblioteca só. Um minuto de configuração e pronto.")}</p>
            <div class="fr-three">
                <div class="fr-card"><Svg icon=Icon::Search size=26 /><b>{tr!("Acha seus jogos")}</b><span>{tr!("Aponte as pastas das suas ROMs e discos: ele mostra quais já têm recomp.")}</span></div>
                <div class="fr-card"><Svg icon=Icon::Download size=26 /><b>{tr!("Instala e atualiza")}</b><span>{tr!("Direto da página de cada projeto, conferindo cada arquivo.")}</span></div>
                <div class="fr-card"><Svg icon=Icon::Settings size=26 /><b>{tr!("Do seu jeito")}</b><span>{tr!("Ajustes, mods e backups de saves para cada jogo, na mesa ou na TV.")}</span></div>
            </div>
            {language_pick()}
        }
        .into_any(),
        1 => {
            let pick = move |on_tv: bool| {
                view! {
                    <button type="button" role="radio" class="fr-pick" class:on=move || tv.get() == on_tv aria-checked=move || (tv.get() == on_tv).to_string()
                        on:click=move |_| tv.set(on_tv)>
                        <Svg icon=if on_tv { Icon::Tv } else { Icon::Library } size=40 />
                        <b>{if on_tv { tr!("Na TV") } else { tr!("Na mesa") }}</b>
                        <span>{if on_tv {
                            tr!("Tela cheia, feita para controle. O Cerberus abre assim toda vez.")
                        } else {
                            tr!("Numa janela, com mouse e teclado. O Big Picture Mode fica a um clique.")
                        }}</span>
                    </button>
                }
            };
            view! {
                <h1>{tr!("Como você vai jogar?")}</h1>
                <p class="fr-lead">{tr!("Dá para trocar quando quiser.")}</p>
                <div class="fr-two" role="radiogroup" aria-label=tr!("Como você vai jogar?")>{pick(false)}{pick(true)}</div>
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
                                on:click=move |_| change(Box::pin(api::remove_rom_dir_owned(path.clone())))>{tr!("Remover")}</button>
                        </div>
                    }
                })
                .collect_view();
            let found = found_games(library);
            let status = if busy.get() {
                view! { <p class="fr-status"><span class="spin"></span>{tr!("Procurando nas suas pastas...")}</p> }.into_any()
            } else if !found.is_empty() {
                let text = if found.len() == 1 { tr!("1 jogo seu já tem recomp").to_string() } else { tr!("{} jogos seus já têm recomp", found.len()) };
                view! { <p class="fr-status ok"><Svg icon=Icon::Check size=18 />{text}</p>{covers(found)} }.into_any()
            } else if !s.rom_dirs.is_empty() {
                view! { <p class="fr-status">{tr!("Nenhum desses jogos tem recomp ainda. Quando aparecer, ele mostra no Descobrir.")}</p> }.into_any()
            } else {
                ().into_any()
            };
            let main = s.rom_dirs.is_empty();
            view! {
                <h1>{tr!("Onde ficam seus jogos?")}</h1>
                <p class="fr-lead">{tr!("As pastas com suas ROMs, imagens de disco e backups. O Cerberus encontra os que têm recomp. Nada sai do seu PC.")}</p>
                <div class="fr-box">
                    {dirs}
                    <button type="button" class="btn" class:main=main disabled=move || busy.get()
                        on:click=move |_| change(Box::pin(api::add_rom_dir()))>
                        <Svg icon=Icon::Folder size=18 />{if main { tr!("Adicionar pasta") } else { tr!("Adicionar outra pasta") }}
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
                <h1>{tr!("Onde instalar os recomps?")}</h1>
                <p class="fr-lead">{tr!("Cada recomp fica numa pasta própria aqui dentro. Dá para trocar depois em Configurações.")}</p>
                <div class="fr-box">
                    <div class="fr-dir big">
                        <Svg icon=Icon::Folder size=18 /><span title=s.recomps_dir.clone()>{s.recomps_dir.clone()}</span>
                        <button type="button" class="btn sm" disabled=move || busy.get()
                            on:click=move |_| change(Box::pin(api::pick_recomps_dir()))>{tr!("Trocar")}</button>
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
                            on:click=move |_| change(Box::pin(api::set_cloud_dir(true)))>{tr!("Remover")}</button>
                    </div>
                }
            });
            let has = s.cloud_dir.is_some();
            view! {
                <Svg icon=Icon::Saves size=44 />
                <h1>{tr!("Saves seguros em qualquer PC")}</h1>
                <p class="fr-lead">{tr!("Escolha uma pasta que o OneDrive, Google Drive ou Dropbox sincroniza. Depois de cada partida, os saves vão para lá, e o outro PC os encontra antes de jogar.")}</p>
                <div class="fr-box">
                    {chosen}
                    <button type="button" class="btn" class:main=!has disabled=move || busy.get()
                        on:click=move |_| change(Box::pin(api::set_cloud_dir(false)))>
                        <Svg icon=Icon::Folder size=18 />{if has { tr!("Trocar pasta") } else { tr!("Escolher pasta sincronizada") }}
                    </button>
                </div>
                <p class="fr-note">{tr!("Opcional. Sem ela, os backups ficam só neste PC.")}</p>
            }
            .into_any()
        }
        _ => {
            let found = found_games(library);
            let waiting = found.iter().filter(|e| e.installed.is_none()).count();
            let lead = match (found.len(), waiting) {
                (0, _) => tr!("Encontre algo para jogar no Descobrir.").to_string(),
                (1, 0) => tr!("Seu jogo com recomp já está instalado. É só jogar.").to_string(),
                (n, 0) => tr!("Seus {} jogos com recomp já estão instalados. É só jogar.", n),
                (1, _) => tr!("1 jogo seu tem recomp, pronto para instalar no Descobrir.").to_string(),
                (n, w) if w == n => tr!("{} jogos seus têm recomp, prontos para instalar no Descobrir.", n),
                (n, w) => tr!("{} jogos seus têm recomp. {} ainda estão no Descobrir, prontos para instalar.", n, w),
            };
            view! {
                <img class="fr-logo done" src="brand/cerberus-pixel-icone.svg" alt="" draggable="false" />
                <h1>{tr!("Tudo pronto")}</h1>
                <p class="fr-lead">{lead}</p>
                {(!found.is_empty()).then(|| covers(found))}
                <OnThisPc />
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
        0 => tr!("Começar"),
        s if s + 1 >= STEPS => tr!("Ir para o Cerberus"),
        _ => tr!("Continuar"),
    };

    view! {
        <div class="fr" role="dialog" aria-modal="true" aria-label=tr!("Configuração inicial")
            on:keydown=move |e| {
                // Enter on a button, link or field does what that one says, not also Continue.
                let on_control = e.target().and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::Element>(t).ok()).is_some_and(|el| matches!(el.tag_name().as_str(), "BUTTON" | "A" | "INPUT" | "SELECT" | "TEXTAREA"));
                if e.key() == "Enter" && !on_control && !busy.get_untracked() {
                    next()
                }
            }>
            <div class="fr-aurora" aria-hidden="true"><i></i><i></i><i></i></div>
            <div class="fr-top" data-tauri-drag-region>
                <div class="brand fr-brand" data-tauri-drag-region>
                    <img class="mark" src="brand/cerberus-pixel-icone.svg" alt=tr!("Cerberus Launcher") draggable="false" />
                    <img class="word" src="brand/cerberus-launcher-pixel.svg#wordmark" alt="" draggable="false" />
                </div>
                // The setup covers the window title bar, so it brings its own buttons.
                <div class="win">
                    <button type="button" aria-label=tr!("Minimizar") on:click=|_| api::window_action("minimize")>
                        <Svg icon=Icon::Minimize size=14 />
                    </button>
                    <button type="button" aria-label=tr!("Maximizar") on:click=|_| api::window_action("toggleMaximize")>
                        <Svg icon=Icon::Maximize size=14 />
                    </button>
                    <button type="button" class="close" aria-label=tr!("Fechar") on:click=|_| api::window_action("close")>
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
                            <Svg icon=Icon::Back size=16 />{tr!("Voltar")}
                        </button>
                    </Show>
                </div>
                <div class="fr-dots" aria-hidden="true">{dots}</div>
                <div class="fr-side end">
                    <Show when=move || { step.get() > 0 && step.get() + 1 < STEPS }>
                        <button type="button" class="link" disabled=move || busy.get() on:click=move |_| finish()>{tr!("Pular configuração")}</button>
                    </Show>
                    <button type="button" class="btn main" disabled=move || busy.get() on:click=move |_| next()>{next_label}</button>
                </div>
            </div>
        </div>
    }
}
