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
            <section class="card set">
                <h3>"Saves na nuvem"</h3>
                <div class="set-row">
                    <div>
                        <b>{if s.cloud_dir.is_some() { "Ligado" } else { "Desligado" }}</b>
                        {s.cloud_dir.clone().map(|d| view! { <small class="path">{d}</small> })}
                        <small>"Escolha uma pasta do OneDrive, Google Drive ou Dropbox. Cada backup de save vai pra lá, e ao abrir um jogo o Cerberus traz os saves mais novos de outro PC (os deste viram um backup antes)."</small>
                    </div>
                    <div class="row">
                        {s.cloud_dir.is_some().then(|| view! {
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
            <Sources sources=s.sources.clone() busy apply_source=move |url: String, add: bool| {
                if add { apply(Box::pin(api::add_source(url)), "Fonte adicionada; os jogos dela já estão em Descobrir.") }
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
                        <small>"Código aberto, licença MIT. Os recomps vêm do GitHub de cada desenvolvedor; o Cerberus nunca baixa arquivos de jogo."</small>
                    </div>
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

/// Catalog sources added by URL. Their games come from each developer's GitHub
/// releases, like the recomps; lists of downloads hosted elsewhere aren't taken.
#[component]
fn Sources(sources: Vec<SourceView>, busy: RwSignal<bool>, apply_source: impl Fn(String, bool) + Copy + Send + Sync + 'static) -> impl IntoView {
    let url = RwSignal::new(String::new());
    let on_add = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        let value = url.get_untracked();
        if !value.trim().is_empty() {
            apply_source(value, true);
            url.set(String::new());
        }
    };
    let rows = sources
        .into_iter()
        .map(|source| {
            let target = source.url.clone();
            view! {
                <div class="set-row sub">
                    <div>
                        <b>{source.name.clone()}</b>
                        <small class="path">{format!("{} · {} jogos", source.url, source.games)}</small>
                    </div>
                    <button type="button" class="btn sm" disabled=move || busy.get() on:click=move |_| apply_source(target.clone(), false)>"Remover"</button>
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
                    <small>"Um JSON com jogos publicados no GitHub pelos próprios desenvolvedores (jogos livres, homebrew, ports). Cada jogo é baixado da release oficial e conferido pelo checksum. Listas de downloads de terceiros (magnet, torrent, sites de arquivos) não são aceitas."</small>
                </div>
                <div class="row">
                    <input class="input" type="url" placeholder="https://.../jogos.json" prop:value=move || url.get() on:input=move |e| url.set(event_target_value(&e)) />
                    <button type="submit" class="btn sm main" disabled=move || busy.get()>"Adicionar"</button>
                </div>
            </form>
            {rows}
        </section>
    }
}
