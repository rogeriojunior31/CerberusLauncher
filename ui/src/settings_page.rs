//! Settings: where recomps install, the app's storage, and about.

use cerberus_shared::{SettingsView, SourceView};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::icons::{Icon, Svg};
use crate::{Library, api, format};

#[component]
pub fn SettingsPage() -> impl IntoView {
    let library = expect_context::<Library>();
    let view_state: RwSignal<Option<SettingsView>> = RwSignal::new(None);
    let busy = RwSignal::new(false);

    spawn_local(async move {
        view_state.set(library.report(api::get_settings().await));
    });

    // Runs one change; the page shows the settings as they are after it.
    let apply = move |task: std::pin::Pin<Box<dyn std::future::Future<Output = Result<SettingsView, String>>>>, done: &'static str| {
        busy.set(true);
        spawn_local(async move {
            if let Some(next) = library.report(task.await) {
                view_state.set(Some(next));
                if !done.is_empty() {
                    library.toast(done);
                }
            }
            busy.set(false);
        });
    };

    let body = move || {
        let Some(s) = view_state.get() else {
            return view! { <p class="muted">"Carregando..."</p> }.into_any();
        };
        let reset = (!s.recomps_dir_is_default).then(|| {
            view! {
                <button type="button" class="btn sm" disabled=move || busy.get()
                    on:click=move |_| apply(Box::pin(api::reset_recomps_dir()), "Novos recomps vão para a pasta padrão.")>
                    "Usar a padrão"
                </button>
            }
        });
        let rom_rows = s
            .rom_dirs
            .iter()
            .cloned()
            .map(|dir| {
                let target = dir.clone();
                view! {
                    <div class="set-row sub">
                        <small class="path">{dir}</small>
                        <button type="button" class="btn sm" disabled=move || busy.get()
                            on:click=move |_| apply(Box::pin(api::remove_rom_dir_owned(target.clone())), "Pasta removida.")>
                            "Remover"
                        </button>
                    </div>
                }
            })
            .collect_view();
        let rom_summary = if s.rom_dirs.is_empty() {
            "Aponte as pastas onde você guarda ROMs e imagens de disco. O Cerberus mostra quais desses jogos têm recomp. Nada sai do seu PC.".to_string()
        } else {
            format!("{} arquivos de jogo encontrados · {} recomps para jogos que você tem.", s.roms_found, s.roms_matched)
        };
        let scan = (!s.rom_dirs.is_empty()).then(|| {
            view! {
                <button type="button" class="btn sm" disabled=move || busy.get()
                    on:click=move |_| apply(Box::pin(api::scan_roms()), "Pastas verificadas de novo.")>
                    "Procurar de novo"
                </button>
            }
        });
        view! {
            <section class="card set">
                <h3>"Como você joga"</h3>
                <div class="set-row">
                    <div>
                        <b>"Abrir na Tela grande"</b>
                        <small>"Tela cheia e feita para controle, toda vez que o Cerberus abrir."</small>
                    </div>
                    <button type="button" class="chip-btn" class:on=s.start_in_tv aria-pressed=s.start_in_tv.to_string()
                        disabled=move || busy.get()
                        on:click=move |_| apply(Box::pin(api::set_start_in_tv(!s.start_in_tv)), "")>
                        {if s.start_in_tv { "Ligado" } else { "Desligado" }}
                    </button>
                </div>
            </section>
            <section class="card set">
                <h3>"Suas ROMs"</h3>
                <div class="set-row">
                    <div>
                        <b>"Pastas de ROMs e jogos"</b>
                        <small>{rom_summary}</small>
                    </div>
                    <div class="row">
                        {scan}
                        <button type="button" class="btn sm main" disabled=move || busy.get()
                            on:click=move |_| apply(Box::pin(api::add_rom_dir()), "")>
                            <Svg icon=Icon::Plus size=16 />"Adicionar pasta"
                        </button>
                    </div>
                </div>
                {rom_rows}
            </section>
            <SteamGridDbKey has=s.steamgriddb_key.clone().is_some_and(|k| !k.is_empty()) />
            <section class="card set">
                <h3>"Saves na nuvem"</h3>
                <div class="set-row">
                    <div>
                        <b>{if s.cloud_dir.is_some() { "Ligado" } else { "Desligado" }}</b>
                        {s.cloud_dir.clone().map(|d| view! { <small class="path">{d}</small> })}
                        <small>"Escolha uma pasta sincronizada. Ao abrir um jogo, o Cerberus guarda o progresso local antes de trazer saves mais novos de outro PC."</small>
                        {(s.pending_uploads > 0).then(|| view! {
                            <small role="status">{format!("Envios pendentes na última verificação: {}. O Cerberus tenta de novo a cada minuto.", s.pending_uploads)}</small>
                        })}
                    </div>
                    <div class="row">
                        {s.cloud_dir.is_some().then(|| view! {
                            <button type="button" class="btn sm" disabled=move || busy.get()
                                on:click=move |_| apply(Box::pin(api::retry_save_uploads()), "Envios verificados.")>"Verificar envios"</button>
                            <button type="button" class="btn sm" disabled=move || busy.get()
                                on:click=move |_| apply(Box::pin(api::set_cloud_dir(true)), "Saves na nuvem desligados.")>"Desligar"</button>
                        })}
                        <button type="button" class="btn sm main" disabled=move || busy.get()
                            on:click=move |_| apply(Box::pin(api::set_cloud_dir(false)), "")>
                            <Svg icon=Icon::Folder size=16 />"Escolher pasta"
                        </button>
                    </div>
                </div>
            </section>
            <Sources sources=s.sources.clone() busy refresh=move |url| apply(Box::pin(api::refresh_source(url)), "")
                apply_source=move |url: String, add: bool| {
                if add { apply(Box::pin(api::add_source(url)), "Fonte adicionada.") }
                else { apply(Box::pin(api::remove_source(url)), "Fonte removida.") }
            } />
            <section class="card set">
                <h3>"Catálogo de recomps"</h3>
                <div class="set-row">
                    <div>
                        <b>{format!("{} recomps", s.catalog_count)}</b>
                        <small>{format!("Lista de {}, atualizada sozinha a cada 6 horas. Cada recomp é baixado do GitHub do próprio desenvolvedor.", s.catalog_source)}</small>
                    </div>
                    <button type="button" class="btn sm" disabled=move || busy.get()
                        on:click=move |_| apply(Box::pin(api::refresh_catalog()), "Catálogo atualizado.")>
                        "Atualizar agora"
                    </button>
                </div>
            </section>
            <section class="card set">
                <h3>"Instalação"</h3>
                <div class="set-row">
                    <div>
                        <b>"Pasta dos recomps"</b>
                        <small class="path">{s.recomps_dir.clone()}</small>
                        {s.install_free_bytes.map(|free| view! { <small>{format!("Espaço livre para instalar: {:.1} GiB", free as f64 / 1073741824.0)}</small> })}
                        <small>"Jogos já instalados continuam onde estão; só as próximas instalações vão para a nova pasta."</small>
                    </div>
                    <div class="row">
                        {reset}
                        <button type="button" class="btn sm" disabled=move || busy.get()
                            on:click=move |_| apply(Box::pin(api::pick_recomps_dir()), "")>
                            <Svg icon=Icon::Folder size=16 />"Escolher pasta"
                        </button>
                    </div>
                </div>
            </section>
            <section class="card set">
                <h3>"Armazenamento"</h3>
                <div class="set-row">
                    <div>
                        <b>"Dados do Cerberus"</b>
                        <small class="path">{s.data_dir.clone()}</small>
                        <small>"Biblioteca, backups de saves e caches."</small>
                        {s.download_free_bytes.map(|free| view! { <small>{format!("Espaço livre para downloads: {:.1} GiB", free as f64 / 1073741824.0)}</small> })}
                    </div>
                    <button type="button" class="btn sm" on:click=move |_| spawn_local(async move { library.report(api::open_data_dir().await); })>
                        "Abrir pasta"
                    </button>
                </div>
                <div class="set-row">
                    <div>
                        <b>"Imagens guardadas"</b>
                        <small>{format!("{} de capas e fundos guardados para abrir rápido e funcionar offline.", format::megabytes(s.art_cache_bytes))}</small>
                    </div>
                    <button type="button" class="btn sm" disabled=move || busy.get()
                        on:click=move |_| apply(Box::pin(api::clear_art_cache()), "Imagens apagadas. Elas voltam conforme as páginas precisarem.")>
                        "Limpar"
                    </button>
                </div>
            </section>
            <section class="card set">
                <h3>"Sobre"</h3>
                <div class="set-row">
                    <div>
                        <b>{format!("Cerberus Launcher {}", s.version)}</b>
                        <small>"Código aberto, licença MIT. Os recomps vêm das releases dos desenvolvedores."</small>
                    </div>
                    <img class="about-logo" src="brand/cerberus-launcher-pixel.svg" alt="Logo do Cerberus Launcher" />
                </div>
            </section>
        }
        .into_any()
    };

    view! {
        <div class="page">
            <header class="page-h"><div><h1>"Configurações"</h1></div></header>
            <div class="settings">{body}</div>
        </div>
    }
}

#[component]
fn Sources(
    sources: Vec<SourceView>, busy: RwSignal<bool>,
    apply_source: impl Fn(String, bool) + Copy + Send + Sync + 'static,
    refresh: impl Fn(String) + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let url = RwSignal::new(String::new());
    let on_add = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        let value = url.get_untracked();
        if !busy.get_untracked() && !value.trim().is_empty() { apply_source(value, true); }
    };
    let rows = sources
        .into_iter()
        .map(|source| {
            let target = StoredValue::new(source.url.clone());
            let opened = RwSignal::new(false);
            let count = if source.hydra { format!("Hydra · {} publicações", source.games) }
                else if source.synced_at.is_some() { format!("{} projetos em Descobrir", source.games) }
                else { String::new() };
            view! {
                <div>
                    <div class="set-row sub">
                        <div>
                            <b>{source.name}</b>
                            <small class="path">{source.url}</small>
                            <small>{count}</small>
                            {source.synced_at.map(|t| view! { <small>{format!("Última sincronização em {}", format::date_time(t))}</small> })}
                            {source.error.map(|e| view! { <small role="status">{e}</small> })}
                        </div>
                        <div class="row">
                            {source.hydra.then(|| view! {
                                <button type="button" class="btn sm" aria-expanded=move || opened.get().to_string()
                                    on:click=move |_| opened.update(|v| *v = !*v)>
                                    {move || if opened.get() { "Fechar lista" } else { "Consultar" }}
                                </button>
                            })}
                            <button type="button" class="btn sm" disabled=move || busy.get()
                                on:click=move |_| refresh(target.get_value())>"Atualizar"</button>
                            <button type="button" class="btn sm" disabled=move || busy.get()
                                on:click=move |_| apply_source(target.get_value(), false)>"Remover"</button>
                        </div>
                    </div>
                    {move || opened.get().then(|| view! { <HydraListings url=target.get_value() /> })}
                </div>
            }
        })
        .collect_view();
    view! {
        <section class="card set">
            <h3>"Fontes de catálogo"</h3>
            <form class="set-row" on:submit=on_add>
                <div>
                    <b>"Adicionar por URL"</b>
                    <small>"Consulte títulos, tamanhos e datas de listas Hydra aqui. Fontes de projetos para instalar aparecem em Descobrir."</small>
                </div>
                <div class="row">
                    <input class="input" type="url" required aria-label="URL da fonte" placeholder="https://.../jogos.json" prop:value=move || url.get() on:input=move |e| url.set(event_target_value(&e)) />
                    <button type="submit" class="btn sm main" disabled=move || busy.get()>"Adicionar"</button>
                </div>
            </form>
            {rows}
        </section>
    }
}

#[component]
fn HydraListings(url: String) -> impl IntoView {
    let url = StoredValue::new(url);
    let query = RwSignal::new(String::new());
    let offset = RwSignal::new(0usize);
    let page = LocalResource::new(move || {
        let (url, q, start) = (url.get_value(), query.get(), offset.get());
        async move { api::source_listings(&url, &q, start).await }
    });
    let results = move || {
        let Some(result) = page.get() else { return view! { <p>"Carregando lista…"</p> }.into_any(); };
        let page = match result {
            Ok(p) => p,
            Err(e) => return view! { <p role="status">{e}</p> }.into_any(),
        };
        if page.items.is_empty() { return view! { <p>"Nenhuma publicação encontrada."</p> }.into_any(); }
        let start = offset.get_untracked();
        let end = start + page.items.len();
        let first_page = start == 0;
        let last_page = end >= page.total;
        let rows = page.items.into_iter().map(|item| view! {
            <tr><th scope="row">{item.title}</th><td>{item.file_size.unwrap_or_else(|| "—".into())}</td>
                <td>{item.upload_date.unwrap_or_else(|| "—".into())}</td></tr>
        }).collect_view();
        view! {
            <table><thead><tr><th>"Título"</th><th>"Tamanho"</th><th>"Data publicada pela fonte"</th></tr></thead><tbody>{rows}</tbody></table>
            <div class="row">
                <small>{format!("{}–{} de {}", start + 1, end, page.total)}</small>
                <button class="btn sm" disabled=first_page on:click=move |_| offset.set(start.saturating_sub(25))>"Anterior"</button>
                <button class="btn sm" disabled=last_page on:click=move |_| offset.set(end)>"Próxima"</button>
            </div>
        }.into_any()
    };
    view! {
        <div class="source-listings">
            <input class="input" type="search" aria-label="Buscar título na fonte" placeholder="Buscar título nesta fonte"
                on:input=move |e| { offset.set(0); query.set(event_target_value(&e)); } />
            {results}
        </div>
    }
}

/// The player's SteamGridDB key, for picking community art.
#[component]
fn SteamGridDbKey(has: bool) -> impl IntoView {
    let library = expect_context::<Library>();
    let saved = RwSignal::new(has);
    let input = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let store = move |key: String| {
        busy.set(true);
        spawn_local(async move {
            let done = library.report(api::set_steamgriddb_key(key.clone()).await);
            busy.set(false);
            if done.is_some() {
                saved.set(!key.trim().is_empty());
                input.set(String::new());
                library.toast(if key.trim().is_empty() { "Chave do SteamGridDB removida." } else { "Chave do SteamGridDB guardada." });
            }
        });
    };
    view! {
        <section class="card set">
            <h3>"Arte dos jogos"</h3>
            <div class="set-row">
                <div>
                    <b>"Chave do SteamGridDB"</b>
                    <small>{move || if saved.get() { "Guardada. Use \"Personalizar arte\" na aba Gerenciar de cada jogo." } else { "Gratuita: crie uma conta no SteamGridDB e gere a chave de API." }}</small>
                </div>
                <div class="row">
                    <button type="button" class="btn sm" on:click=move |_| spawn_local(async move { library.report(api::open_steamgriddb_keys().await); })>"Pegar chave"</button>
                    <Show when=move || saved.get()>
                        <button type="button" class="btn sm" disabled=move || busy.get() on:click=move |_| store(String::new())>"Remover"</button>
                    </Show>
                </div>
            </div>
            <form class="set-row sub" on:submit=move |e: leptos::ev::SubmitEvent| { e.prevent_default(); store(input.get_untracked()); }>
                <input class="input" type="password" placeholder=move || if saved.get() { "Trocar a chave" } else { "Cole a chave aqui" }
                    prop:value=move || input.get() on:input=move |e| input.set(event_target_value(&e)) />
                <button type="submit" class="btn sm main" disabled=move || busy.get() || input.get().trim().is_empty()>"Guardar"</button>
            </form>
        </section>
    }
}
