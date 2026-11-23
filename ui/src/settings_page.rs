//! Configurações: a menu of sections on the left, grouped (App, Jogos,
//! Clássicos, Contas, Sistema), and the chosen section on the right as cards
//! of rows: a title, a line about it, and its control.

use cerberus_shared::{tk, tr};
use cerberus_shared::{RaProfile, SettingsView, SourceView};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::icons::{Icon, Svg};
use crate::{Library, Route, api, format};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    General,
    Appearance,
    BigPicture,
    Library,
    Install,
    Saves,
    Art,
    Catalog,
    Emulation,
    Retro,
    Pc,
    Storage,
    About,
}

const MENU: &[(&str, &[(Section, &str, Icon)])] = &[
    (tk!("App"), &[(Section::General, tk!("Geral"), Icon::Settings), (Section::Appearance, tk!("Aparência"), Icon::Edit), (Section::BigPicture, tk!("Big Picture Mode"), Icon::Tv)]),
    (
        tk!("Jogos"),
        &[
            (Section::Library, tk!("Biblioteca e ROMs"), Icon::Library),
            (Section::Install, tk!("Instalação"), Icon::Download),
            (Section::Saves, tk!("Saves e nuvem"), Icon::Saves),
            (Section::Art, tk!("Arte dos jogos"), Icon::Edit),
            (Section::Catalog, tk!("Catálogo e fontes"), Icon::Discover),
        ],
    ),
    (tk!("Clássicos"), &[(Section::Emulation, tk!("Emulação"), Icon::Gamepad)]),
    (tk!("Contas"), &[(Section::Retro, "RetroAchievements", Icon::Trophy)]),
    (tk!("Sistema"), &[(Section::Pc, tk!("Seu PC"), Icon::Check), (Section::Storage, tk!("Armazenamento"), Icon::Folder), (Section::About, tk!("Sobre"), Icon::Info)]),
];

type Task = std::pin::Pin<Box<dyn std::future::Future<Output = Result<SettingsView, String>>>>;

/// A switch for an on/off setting.
fn switch(on: bool, label: &'static str, busy: RwSignal<bool>, toggle: impl Fn() + 'static) -> impl IntoView {
    view! {
        <button type="button" class="switch" class:on=on role="switch" aria-checked=on.to_string() aria-label=label
            disabled=move || busy.get() on:click=move |_| toggle()>
            <i></i>
        </button>
    }
}

fn row(title: impl IntoView, about: impl IntoView, control: impl IntoView) -> impl IntoView {
    view! {
        <div class="set-row">
            <div><b>{title}</b><small>{about}</small></div>
            <div class="row">{control}</div>
        </div>
    }
}

const CALM_KEY: &str = "cerberus.calm";

/// Fewer animations: remembered per PC, applied to the whole window.
pub fn calm_saved() -> bool {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(CALM_KEY).ok().flatten())
        .is_some_and(|v| v == "1")
}

pub fn apply_calm(on: bool) {
    if let Some(root) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.document_element()) {
        let _ = if on { root.class_list().add_1("calm") } else { root.class_list().remove_1("calm") };
    }
}

fn save_calm(on: bool) {
    if let Some(s) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = s.set_item(CALM_KEY, if on { "1" } else { "0" });
    }
    apply_calm(on);
}

#[component]
pub fn SettingsPage() -> impl IntoView {
    let library = expect_context::<Library>();
    let view_state: RwSignal<Option<SettingsView>> = RwSignal::new(None);
    let busy = RwSignal::new(false);
    let section = RwSignal::new(Section::General);

    spawn_local(async move {
        view_state.set(library.report(api::get_settings().await));
    });

    // Runs one change; the page shows the settings as they are after it.
    let apply = move |task: Task, done: &'static str| {
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

    let menu = MENU
        .iter()
        .map(|(group, items)| {
            let links = items
                .iter()
                .map(|(which, label, icon)| {
                    let which = *which;
                    view! {
                        <button type="button" class="set-link" class:on=move || section.get() == which on:click=move |_| section.set(which)>
                            <Svg icon=*icon size=18 />{cerberus_shared::i18n::t(label)}
                        </button>
                    }
                })
                .collect_view();
            view! { <div class="set-group-label">{cerberus_shared::i18n::t(group)}</div>{links} }
        })
        .collect_view();

    let body = move || {
        let Some(s) = view_state.get() else {
            return view! { <p class="muted">{tr!("Carregando...")}</p> }.into_any();
        };
        match section.get() {
            Section::General => general(s, busy, apply).into_any(),
            Section::Appearance => appearance().into_any(),
            Section::BigPicture => big_picture(s, busy, apply).into_any(),
            Section::Library => roms(s, busy, apply).into_any(),
            Section::Install => install(s, busy, apply).into_any(),
            Section::Saves => saves(s, busy, apply).into_any(),
            Section::Art => view! { <SteamGridDbKey has=s.steamgriddb_key.clone().is_some_and(|k| !k.is_empty()) /> {art_cache(s, busy, apply)} }.into_any(),
            Section::Catalog => view! {
                {catalog(s.clone(), busy, apply)}
                <Sources sources=s.sources.clone() busy refresh=move |url| apply(Box::pin(api::refresh_source(url)), "")
                    apply_source=move |url: String, add: bool| {
                        if add { apply(Box::pin(api::add_source(url)), tr!("Fonte adicionada.")) } else { apply(Box::pin(api::remove_source(url)), tr!("Fonte removida.")) }
                    } />
            }
            .into_any(),
            Section::Emulation => emulation(s).into_any(),
            Section::Retro => view! { <RetroAchievements linked=s.ra_user.clone() /> }.into_any(),
            Section::Pc => view! { <YourPc /> }.into_any(),
            Section::Storage => storage(s, busy, apply).into_any(),
            Section::About => about(s, busy, apply).into_any(),
        }
    };

    view! {
        <div class="page">
            <header class="page-h"><div><h1>{tr!("Configurações")}</h1></div></header>
            <div class="set-layout">
                <nav class="set-nav" aria-label=tr!("Seções")>{menu}</nav>
                <div class="settings">{body}</div>
            </div>
        </div>
    }
}

fn general(s: SettingsView, busy: RwSignal<bool>, apply: impl Fn(Task, &'static str) + Copy + 'static) -> impl IntoView {
    let page = if s.start_page.is_empty() { "home".to_string() } else { s.start_page.clone() };
    let choice = move |id: &'static str, label: &'static str| {
        let on = page == id;
        view! {
            <button type="button" class="seg" class:on=on role="radio" aria-checked=on.to_string() disabled=move || busy.get()
                on:click=move |_| apply(Box::pin(api::set_start_page(id.to_string())), "")>{label}</button>
        }
    };
    view! {
        <section class="card set">
            <h3>{tr!("Ao abrir")}</h3>
            {row(tr!("Página inicial"), tr!("Onde a janela abre. Quem joga no Big Picture Mode abre direto nele."),
                view! { <div class="segs" role="radiogroup">{choice("home", tr!("Início"))}{choice("library", tr!("Biblioteca"))}{choice("discover", tr!("Descobrir"))}</div> })}
            <MonitorRow chosen=s.monitor.clone() />
            {row(tr!("Configuração inicial"), tr!("As telas de boas-vindas da primeira vez, para rever pastas, instalação e nuvem."),
                view! { <button type="button" class="btn sm" disabled=move || busy.get()
                    on:click=move |_| apply(Box::pin(api::rerun_setup()), tr!("Na próxima vez que o Cerberus abrir, a configuração inicial aparece."))>{tr!("Mostrar de novo")}</button> })}
        </section>
        <section class="card set">
            <h3>{tr!("Ao ligar o PC")}</h3>
            {row(tr!("Abrir com o Windows"), tr!("O Cerberus abre sozinho quando você entra no Windows, pronto para os avisos de atualização."),
                switch(s.autostart, tr!("Abrir com o Windows"), busy, move || {
                    let on = !s.autostart;
                    apply(Box::pin(api::set_autostart(on)), if on { tr!("O Cerberus vai abrir com o Windows.") } else { tr!("O Cerberus não abre mais com o Windows.") })
                }))}
            {row(tr!("Começar escondido na bandeja"), tr!("Ao abrir com o Windows, fica perto do relógio sem mostrar a janela. Abrir pelo atalho mostra a janela normalmente."),
                switch(s.start_hidden, tr!("Começar escondido na bandeja"), busy, move || {
                    let on = !s.start_hidden;
                    apply(Box::pin(api::set_start_hidden(on)), if on { tr!("Ao abrir com o Windows, o Cerberus fica na bandeja.") } else { tr!("Ao abrir com o Windows, a janela aparece.") })
                }))}
        </section>
        <section class="card set">
            <h3>{tr!("Ao fechar a janela")}</h3>
            {row(tr!("Continuar na bandeja"), tr!("O Cerberus fica perto do relógio do Windows, pronto para abrir. Para sair de vez, clique com o botão direito no ícone dele e escolha Sair."),
                switch(s.close_to_tray, tr!("Continuar na bandeja"), busy, move || {
                    let on = !s.close_to_tray;
                    apply(Box::pin(api::set_close_to_tray(on)), if on { tr!("Ao fechar a janela, o Cerberus fica na bandeja.") } else { tr!("Ao fechar a janela, o Cerberus fecha.") })
                }))}
            <p class="muted small">{tr!("Com um jogo aberto, fechar a janela sempre deixa o Cerberus na bandeja, para contar o tempo de jogo e guardar os saves no fim da partida. Ele fecha sozinho quando o jogo terminar.")}</p>
        </section>
    }
}

/// Which monitor Cerberus and Big Picture open on; only with more than one plugged in.
#[component]
fn MonitorRow(chosen: Option<String>) -> impl IntoView {
    let library = expect_context::<Library>();
    let monitors = LocalResource::new(api::list_monitors);
    move || {
        let list = monitors.get().and_then(Result::ok).unwrap_or_default();
        let unplugged = chosen.as_ref().is_some_and(|c| !list.iter().any(|m| &m.name == c));
        if list.len() < 2 && !unplugged {
            return None;
        }
        let chosen = chosen.clone();
        let options = list.into_iter().map(|m| {
            let on = chosen.as_deref() == Some(m.name.as_str());
            view! { <option value=m.name.clone() selected=on>{m.label}</option> }
        }).collect_view();
        let gone = unplugged.then(|| view! { <option value=chosen.clone().unwrap_or_default() selected=true>{tr!("O escolhido (desligado agora)")}</option> });
        Some(row("Monitor", tr!("Onde a janela do Cerberus e o Big Picture Mode abrem."),
            view! {
                <select class="select" aria-label=tr!("Monitor")
                    on:change=move |e| {
                        let name = event_target_value(&e);
                        leptos::task::spawn_local(async move {
                            library.report(api::set_monitor((!name.is_empty()).then_some(name)).await);
                        });
                    }>
                    <option value="" selected=chosen.is_none()>{tr!("Onde o Windows abrir")}</option>
                    {options}
                    {gone}
                </select>
            }))
    }
}

/// The app's language: automatic (the system's) or one picked; the window redraws in it.
fn language_row() -> impl IntoView {
    use cerberus_shared::i18n::Lang;
    let chosen = crate::i18n::choice();
    let options = Lang::ALL.iter().map(|lang| {
        let on = chosen.as_deref().is_some_and(|c| Lang::from_code(c) == *lang);
        view! { <option value=lang.code() selected=on>{lang.name()}</option> }
    }).collect_view();
    row(tr!("Idioma"), tr!("Os textos do Cerberus. No automático, segue o idioma do Windows."),
        view! {
            <select class="select" aria-label=tr!("Idioma") on:change=move |e| {
                let code = event_target_value(&e);
                let code = (!code.is_empty()).then_some(code);
                spawn_local(async move {
                    let _ = api::set_language(code.clone()).await;
                    crate::i18n::choose(code.as_deref());
                });
            }>
                <option value="" selected=chosen.is_none()>{tr!("Automático (idioma do sistema)")}</option>
                {options}
            </select>
        })
}

fn appearance() -> impl IntoView {
    let calm = RwSignal::new(calm_saved());
    let busy = RwSignal::new(false);
    view! {
        <section class="card set">
            <h3>{tr!("Aparência")}</h3>
            {language_row()}
            {move || row(tr!("Menos animações"), tr!("Sem transições nem movimento, para PCs mais simples ou para quem prefere assim."),
                switch(calm.get(), tr!("Menos animações"), busy, move || { let on = !calm.get_untracked(); save_calm(on); calm.set(on); }))}
        </section>
    }
}

fn big_picture(s: SettingsView, busy: RwSignal<bool>, apply: impl Fn(Task, &'static str) + Copy + 'static) -> impl IntoView {
    let library = expect_context::<Library>();
    let sounds = RwSignal::new(crate::tv::sound_enabled());
    let local = RwSignal::new(false);
    let on = s.start_in_tv;
    view! {
        <section class="card set">
            <div class="card-h">
                <h3>{tr!("Big Picture Mode")}</h3>
                <button type="button" class="btn sm main" on:click=move |_| library.go(Route::BigPicture)><Svg icon=Icon::Tv size=16 />{tr!("Abrir agora")}</button>
            </div>
            {row(tr!("Abrir no Big Picture Mode"), tr!("Tela cheia e feita para controle, toda vez que o Cerberus abrir."),
                switch(on, tr!("Abrir no Big Picture Mode"), busy, move || apply(Box::pin(api::set_start_in_tv(!on)), "")))}
            {move || row(tr!("Sons de navegação"), tr!("Um toque curto ao mover, escolher e voltar."),
                switch(sounds.get(), tr!("Sons de navegação"), local, move || { let v = !sounds.get_untracked(); crate::tv::set_sound_enabled(v); sounds.set(v); }))}
            {row(tr!("Entrar e sair pelo controle"), tr!("Segure o botão Menu por um segundo, ou aperte F11 no teclado."), ())}
        </section>
    }
}

fn roms(s: SettingsView, busy: RwSignal<bool>, apply: impl Fn(Task, &'static str) + Copy + 'static) -> impl IntoView {
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
                        on:click=move |_| apply(Box::pin(api::remove_rom_dir_owned(target.clone())), tr!("Pasta removida."))>{tr!("Remover")}</button>
                </div>
            }
        })
        .collect_view();
    let summary = if s.rom_dirs.is_empty() {
        tr!("Aponte as pastas onde você guarda ROMs e imagens de disco. O Cerberus mostra quais desses jogos têm recomp. Nada sai do seu PC.").to_string()
    } else {
        tr!("{} arquivos de jogo encontrados · {} recomps para jogos que você tem.", s.roms_found, s.roms_matched)
    };
    let scan = (!s.rom_dirs.is_empty()).then(|| {
        view! { <button type="button" class="btn sm" disabled=move || busy.get() on:click=move |_| apply(Box::pin(api::scan_roms()), tr!("Pastas verificadas de novo."))>{tr!("Procurar de novo")}</button> }
    });
    view! {
        <section class="card set">
            <h3>{tr!("Suas ROMs e jogos")}</h3>
            {row(tr!("Pastas de ROMs e jogos"), summary, view! {
                {scan}
                <button type="button" class="btn sm main" disabled=move || busy.get() on:click=move |_| apply(Box::pin(api::add_rom_dir()), "")>
                    <Svg icon=Icon::Plus size=16 />{tr!("Adicionar pasta")}
                </button>
            })}
            {rom_rows}
        </section>
    }
}

fn install(s: SettingsView, busy: RwSignal<bool>, apply: impl Fn(Task, &'static str) + Copy + 'static) -> impl IntoView {
    let reset = (!s.recomps_dir_is_default).then(|| {
        view! { <button type="button" class="btn sm" disabled=move || busy.get() on:click=move |_| apply(Box::pin(api::reset_recomps_dir()), tr!("Novos recomps vão para a pasta padrão."))>{tr!("Usar a padrão")}</button> }
    });
    let free = s.install_free_bytes.map(|free| tr!(" Espaço livre: {} GiB.", format::decimal(format!("{:.1}", free as f64 / 1073741824.0)))).unwrap_or_default();
    view! {
        <section class="card set">
            <h3>{tr!("Instalação")}</h3>
            {row(tr!("Pasta dos recomps"), view! { <span class="path">{s.recomps_dir.clone()}</span><br/>{tr!("Jogos já instalados continuam onde estão; só as próximas instalações vão para a nova pasta.{}", free)} }, view! {
                {reset}
                <button type="button" class="btn sm" disabled=move || busy.get() on:click=move |_| apply(Box::pin(api::pick_recomps_dir()), "")>
                    <Svg icon=Icon::Folder size=16 />{tr!("Escolher pasta")}
                </button>
            })}
        </section>
    }
}

fn saves(s: SettingsView, busy: RwSignal<bool>, apply: impl Fn(Task, &'static str) + Copy + 'static) -> impl IntoView {
    let pending = (s.pending_uploads > 0).then(|| tr!(" Envios pendentes na última verificação: {}. O Cerberus tenta de novo a cada minuto.", s.pending_uploads)).unwrap_or_default();
    let on = s.cloud_dir.is_some();
    view! {
        <section class="card set">
            <h3>{tr!("Saves na nuvem")}</h3>
            {row(if on { tr!("Ligado") } else { tr!("Desligado") }, view! {
                {s.cloud_dir.clone().map(|d| view! { <span class="path">{d}</span><br/> })}
                {tr!("Escolha uma pasta que o OneDrive, Google Drive ou Dropbox sincroniza. Ao abrir um jogo, o Cerberus guarda o progresso local antes de trazer saves mais novos de outro PC.{}", pending)}
            }, view! {
                {on.then(|| view! {
                    <button type="button" class="btn sm" disabled=move || busy.get() on:click=move |_| apply(Box::pin(api::retry_save_uploads()), tr!("Envios verificados."))>{tr!("Verificar envios")}</button>
                    <button type="button" class="btn sm" disabled=move || busy.get() on:click=move |_| apply(Box::pin(api::set_cloud_dir(true)), tr!("Saves na nuvem desligados."))>{tr!("Desligar")}</button>
                })}
                <button type="button" class="btn sm main" disabled=move || busy.get() on:click=move |_| apply(Box::pin(api::set_cloud_dir(false)), "")>
                    <Svg icon=Icon::Folder size=16 />{tr!("Escolher pasta")}
                </button>
            })}
            {row(tr!("Backups neste PC"), tr!("Depois de cada partida, antes de atualizar e antes de desinstalar, o Cerberus guarda os 10 backups mais recentes de cada jogo."), ())}
        </section>
    }
}

fn art_cache(s: SettingsView, busy: RwSignal<bool>, apply: impl Fn(Task, &'static str) + Copy + 'static) -> impl IntoView {
    view! {
        <section class="card set">
            <h3>{tr!("Imagens guardadas")}</h3>
            {row(tr!("Capas e fundos"), tr!("{} guardados para abrir rápido e funcionar offline.", format::megabytes(s.art_cache_bytes)),
                view! { <button type="button" class="btn sm" disabled=move || busy.get()
                    on:click=move |_| apply(Box::pin(api::clear_art_cache()), tr!("Imagens apagadas. Elas voltam conforme as páginas precisarem."))>{tr!("Limpar")}</button> })}
        </section>
    }
}

fn catalog(s: SettingsView, busy: RwSignal<bool>, apply: impl Fn(Task, &'static str) + Copy + 'static) -> impl IntoView {
    view! {
        <section class="card set">
            <h3>{tr!("Catálogo de recomps")}</h3>
            {row(format!("{} recomps", s.catalog_count), if s.site_catalog { tr!("Lista de {}, atualizada sozinha a cada 6 horas, mais o catálogo próprio do Cerberus. Cada recomp é baixado da página do próprio desenvolvedor (GitHub, GitLab ou o servidor do projeto).", s.catalog_source) } else { tr!("Só o catálogo próprio do Cerberus e as suas fontes. Cada recomp é baixado da página do próprio desenvolvedor.").to_string() },
                view! { <button type="button" class="btn sm" disabled=move || busy.get() on:click=move |_| apply(Box::pin(api::refresh_catalog()), tr!("Catálogo atualizado."))>{tr!("Atualizar agora")}</button> })}
            {row(tr!("Catálogo do recomplauncher.com"), tr!("A lista grande, com quase 1.900 recomps de mais de 25 consoles. Desligado, o Descobrir mostra só o catálogo do Cerberus e as suas fontes."),
                switch(s.site_catalog, tr!("Catálogo do recomplauncher.com"), busy, move || {
                    let on = !s.site_catalog;
                    apply(Box::pin(api::set_site_catalog(on)), if on { tr!("Catálogo do recomplauncher.com ligado.") } else { tr!("Usando só o catálogo do Cerberus.") })
                }))}
        </section>
    }
}

fn emulation(s: SettingsView) -> impl IntoView {
    view! { <Emulation roms=s.roms.len() ra_user=s.ra_user.clone() /> }
}

/// RetroArch for the classics: install or pick it, its cores, and RetroAchievements in it.
#[component]
fn Emulation(roms: usize, ra_user: Option<String>) -> impl IntoView {
    let library = expect_context::<Library>();
    let state = RwSignal::new(None::<cerberus_shared::EmulationView>);
    let busy = RwSignal::new(None::<String>);
    let progress = RwSignal::new((0u64, 0u64));
    let password = RwSignal::new(String::new());
    spawn_local(async move {
        state.set(library.report(api::emulation_status().await));
    });
    api::on_emu_progress(move |p| progress.set(p));
    let run = move |label: String, task: std::pin::Pin<Box<dyn std::future::Future<Output = Result<cerberus_shared::EmulationView, String>>>>, done: &'static str| {
        busy.set(Some(label));
        spawn_local(async move {
            let got = library.report(task.await);
            busy.set(None);
            if let Some(v) = got {
                state.set(Some(v));
                if !done.is_empty() {
                    library.toast(done);
                }
            }
        });
    };
    let ra_linked = ra_user.is_some();

    move || {
        let Some(v) = state.get() else {
            return view! { <section class="card set"><h3>{tr!("Emulação")}</h3><p class="muted">{tr!("Verificando...")}</p></section> }.into_any();
        };
        let working = busy.get();
        let status = match (&v.retroarch, &working) {
            (_, Some(label)) if label == "install" => {
                let (done, total) = progress.get();
                let text = if total > 0 && done >= total {
                    tr!("Extraindo o RetroArch... leva um minuto.").to_string()
                } else if total > 0 {
                    tr!("Baixando o RetroArch · {} de {}", format::megabytes(done), format::megabytes(total))
                } else {
                    tr!("Preparando o RetroArch...").to_string()
                };
                view! { <small>{text}</small> }.into_any()
            }
            (Some(path), _) => view! { <small><span class="path">{path.clone()}</span><br/>{if v.managed { tr!("Instalado pelo Cerberus, do site oficial do libretro.") } else { tr!("O seu RetroArch.") }}</small> }.into_any(),
            (None, _) => view! { <small>{tr!("O RetroArch roda NES, Super Nintendo, Nintendo 64, Game Boy, GBA, Mega Drive, PlayStation, PlayStation 2, PSP, GameCube e Wii. O Cerberus instala a versão oficial (cerca de 200 MB) numa pasta só dele, ou usa o seu.")}</small> }.into_any(),
        };
        let has = v.retroarch.is_some();
        let disabled = move || busy.get().is_some();
        let cores = v
            .cores
            .iter()
            .map(|c| {
                let core = c.core.clone();
                let installing = working.as_deref() == Some(core.as_str());
                let control = if c.installed {
                    view! { <span class="pill ok"><Svg icon=Icon::Check size=14 />{tr!("Instalado")}</span> }.into_any()
                } else {
                    let core = core.clone();
                    view! {
                        <button type="button" class="btn sm" disabled=move || disabled() || !has
                            on:click=move |_| {
                                let name = core.clone();
                                run(name.clone(), Box::pin(api::install_core(name)), tr!("Núcleo instalado."));
                            }>
                            {if installing { tr!("Baixando...") } else { tr!("Instalar") }}
                        </button>
                    }
                    .into_any()
                };
                view! {
                    <div class="set-row sub">
                        <div><b>{c.consoles.clone()}</b><small>{tr!("Núcleo {}", c.core)}</small></div>
                        {control}
                    </div>
                }
            })
            .collect_view();
        let retro = match (&v.ra_user, ra_linked) {
            (Some(user), _) => view! { <span class="pill ok"><Svg icon=Icon::Trophy size=14 />{tr!("Conectado como {}", user)}</span> }.into_any(),
            (None, false) => view! { <small>{tr!("Conecte sua conta em Contas › RetroAchievements primeiro.")}</small> }.into_any(),
            (None, true) => view! {
                <form class="row" on:submit=move |e: leptos::ev::SubmitEvent| {
                    e.prevent_default();
                    let pass = password.get_untracked();
                    password.set(String::new());
                    run("ra".to_string(), Box::pin(api::link_retroarch_achievements(pass)), tr!("RetroArch conectado ao RetroAchievements."));
                }>
                    <input class="input" type="password" placeholder=tr!("Sua senha do RetroAchievements") prop:value=move || password.get()
                        on:input=move |e| password.set(event_target_value(&e)) />
                    <button type="submit" class="btn sm main" disabled=move || disabled() || !has || password.get().is_empty()>{tr!("Conectar")}</button>
                </form>
            }
            .into_any(),
        };
        view! {
            <section class="card set">
                <div class="card-h">
                    <h3>"RetroArch"</h3>
                    <div class="row">
                        <button type="button" class="btn sm" disabled=disabled
                            on:click=move |_| run("pick".to_string(), Box::pin(api::pick_retroarch()), "")>{tr!("Usar o meu")}</button>
                        <button type="button" class="btn sm main" disabled=disabled
                            on:click=move |_| run("install".to_string(), Box::pin(api::install_retroarch()), tr!("RetroArch instalado."))>
                            <Svg icon=Icon::Download size=16 />{if has { tr!("Reinstalar") } else { tr!("Instalar") }}
                        </button>
                    </div>
                </div>
                {row(if has { tr!("Pronto") } else { tr!("Não instalado") }, status, ())}
            </section>
            <section class="card set">
                <h3>{tr!("Núcleos")}</h3>
                <p class="muted small">{tr!("Cada console usa um núcleo do RetroArch, baixado do site oficial do libretro.")}</p>
                {cores}
            </section>
            <section class="card set">
                <h3>"BIOS"</h3>
                <p class="muted small">{tr!("A BIOS é o programa interno do console. O PlayStation 2 só roda com a do seu próprio console; o PlayStation roda sem, mas fica mais compatível com ela. O Cerberus nunca baixa BIOS: aponte a pasta onde você guarda as suas.")}</p>
                {row("PlayStation", if v.ps1_bios { tr!("BIOS encontrada.") } else { tr!("Opcional. Arquivo de 512 KB, como scph5501.bin.") },
                    if v.ps1_bios { view! { <span class="pill ok"><Svg icon=Icon::Check size=14 />{tr!("Pronta")}</span> }.into_any() } else { view! { <span class="pill soon">{tr!("Embutida")}</span> }.into_any() })}
                {row("PlayStation 2", if v.ps2_bios { tr!("BIOS encontrada.") } else { tr!("Necessária. Arquivos de 4 MB, como SCPH-70012.bin.") },
                    if v.ps2_bios { view! { <span class="pill ok"><Svg icon=Icon::Check size=14 />{tr!("Pronta")}</span> }.into_any() } else { view! { <span class="pill soon">{tr!("Falta")}</span> }.into_any() })}
                <div class="row">
                    <button type="button" class="btn sm main" disabled=move || disabled() || !has
                        on:click=move |_| {
                            busy.set(Some("bios".to_string()));
                            spawn_local(async move {
                                let got = library.report(api::import_bios().await);
                                busy.set(None);
                                if let Some((view, said)) = got {
                                    state.set(Some(view));
                                    if !said.is_empty() {
                                        library.toast(said);
                                    }
                                }
                            });
                        }>
                        <Svg icon=Icon::Folder size=16 />{tr!("Adicionar BIOS de uma pasta")}
                    </button>
                </div>
            </section>
            <section class="card set">
                <h3>{tr!("Conquistas no RetroArch")}</h3>
                {row("RetroAchievements", tr!("O Cerberus troca sua senha por um código de acesso no site e grava só o código no RetroArch. A senha não fica guardada. O arquivo de configurações do RetroArch ganha uma cópia de segurança antes."), retro)}
            </section>
            <section class="card set">
                {row(tr!("Seus clássicos"), tr!("{} jogos encontrados nas pastas de ROMs. Jogue pela aba Clássicos da Biblioteca.", roms),
                    view! { <button type="button" class="btn sm" on:click=move |_| library.go(Route::Library)>{tr!("Ver na Biblioteca")}</button> })}
            </section>
        }
        .into_any()
    }
}

/// What the PC has that games need, with a way to put it right.
#[component]
fn YourPc() -> impl IntoView {
    let library = expect_context::<Library>();
    let state = RwSignal::new(None::<cerberus_shared::PcStatus>);
    let working = RwSignal::new(false);
    let working_dx = RwSignal::new(false);
    spawn_local(async move {
        state.set(library.report(api::pc_status().await));
    });
    let install_dx = move |_| {
        working_dx.set(true);
        spawn_local(async move {
            let got = library.report(api::install_dx().await);
            working_dx.set(false);
            if let Some(s) = got {
                library.toast(if s.dx_missing.is_empty() { tr!("DirectX instalado.") } else { tr!("O DirectX terminou, mas ainda falta uma parte.") });
                state.set(Some(s));
            }
        });
    };
    let install = move |_| {
        working.set(true);
        spawn_local(async move {
            let got = library.report(api::install_vc().await);
            working.set(false);
            if let Some(s) = got {
                state.set(Some(s));
                library.toast(tr!("Visual C++ instalado."));
            }
        });
    };
    move || {
        let s = state.get()?;
        let (about, pill) = match (&s.vc_version, s.vc_ok) {
            (Some(v), true) => (tr!("Versão {}. Em dia para os recomps de hoje.", v), view! { <span class="pill ok"><Svg icon=Icon::Check size=14 />{tr!("Em dia")}</span> }.into_any()),
            (Some(v), false) => (tr!("Versão {}, antiga: recomps novos fecham ao abrir (erro 0xC0000139).", v), view! { <span class="pill soon">{tr!("Antigo")}</span> }.into_any()),
            (None, _) => (tr!("Não instalado. Sem ele, a maioria dos recomps não abre (erro 0xC0000135).").to_string(), view! { <span class="pill soon">{tr!("Falta")}</span> }.into_any()),
        };
        // Up to date, reinstalling is only a repair: no call to action.
        let label = match (&s.vc_version, s.vc_ok) {
            (Some(_), true) => tr!("Reinstalar"),
            (Some(_), false) => tr!("Atualizar"),
            (None, _) => tr!("Instalar"),
        };
        let calm = s.vc_ok;
        let dx_ok = s.dx_missing.is_empty();
        let (dx_about, dx_pill) = if dx_ok {
            (tr!("D3DX, XInput 1.3 e XAudio 2.7 instalados, para os ports de PC mais antigos.").to_string(), view! { <span class="pill ok"><Svg icon=Icon::Check size=14 />{tr!("Em dia")}</span> }.into_any())
        } else {
            (tr!("Falta: {}. Ports de PC mais antigos fecham ao abrir sem eles (erro 0xC0000135).", s.dx_missing.join(", ")), view! { <span class="pill soon">{tr!("Falta")}</span> }.into_any())
        };
        Some(view! {
            <section class="card set">
                <h3>{tr!("Seu PC")}</h3>
                <p class="muted small">{tr!("O que os jogos precisam no Windows. Quando um jogo fecha logo ao abrir, o Cerberus diz qual destes falta.")}</p>
                {row(tr!("Visual C++ 2015-2022 (64 bits)"), about, pill)}
                <div class="row">
                    <button type="button" class="btn sm" class:main=!calm disabled=move || working.get() on:click=install>
                        {move || if working.get() { tr!("Instalando... confirme no Windows").to_string() } else { tr!("{} pelo site da Microsoft", label) }}
                    </button>
                </div>
                {row(tr!("DirectX (componentes antigos)"), dx_about, dx_pill)}
                <div class="row">
                    <button type="button" class="btn sm" class:main=!dx_ok disabled=move || working_dx.get() on:click=install_dx>
                        {move || if working_dx.get() { tr!("Instalando... confirme no Windows") } else if dx_ok { tr!("Reinstalar pelo site da Microsoft") } else { tr!("Instalar pelo site da Microsoft") }}
                    </button>
                </div>
                <p class="muted small">{tr!("Os instaladores oficiais são baixados da Microsoft e o Windows pede sua permissão.")}</p>
            </section>
        })
    }
}

fn storage(s: SettingsView, busy: RwSignal<bool>, apply: impl Fn(Task, &'static str) + Copy + 'static) -> impl IntoView {
    let library = expect_context::<Library>();
    let free = s.download_free_bytes.map(|free| tr!(" Espaço livre para downloads: {} GiB.", format::decimal(format!("{:.1}", free as f64 / 1073741824.0)))).unwrap_or_default();
    view! {
        <section class="card set">
            <h3>{tr!("Armazenamento")}</h3>
            {row(tr!("Dados do Cerberus"), view! { <span class="path">{s.data_dir.clone()}</span><br/>{tr!("Biblioteca, backups de saves e caches.{}", free)} },
                view! { <button type="button" class="btn sm" on:click=move |_| spawn_local(async move { library.report(api::open_data_dir().await); })>{tr!("Abrir pasta")}</button> })}
        </section>
        {art_cache(s, busy, apply)}
    }
}

fn about(s: SettingsView, _busy: RwSignal<bool>, _apply: impl Fn(Task, &'static str) + Copy + 'static) -> impl IntoView {
    let library = expect_context::<Library>();
    view! {
        <section class="card set">
            <h3>{tr!("Sobre")}</h3>
            <div class="set-row">
                <div>
                    <b>{tr!("Cerberus Launcher {}", s.version)}</b>
                    <small>{tr!("Código aberto, licença MIT. Os recomps vêm das releases dos próprios desenvolvedores, e o Cerberus nunca baixa arquivos de jogo.")}</small>
                </div>
                <img class="about-logo" src="brand/cerberus-launcher-pixel.svg" alt=tr!("Logo do Cerberus Launcher") />
            </div>
            {row(tr!("Pedir um recomp"), tr!("Sugira um projeto para o catálogo, pelo GitHub do Cerberus."),
                view! { <button type="button" class="btn sm" on:click=move |_| spawn_local(async move { library.report(api::open_recomp_request().await); })><Svg icon=Icon::External size=16 />{tr!("Abrir")}</button> })}
        </section>
    }
}

/// Linking a RetroAchievements account and showing its progress.
#[component]
fn RetroAchievements(linked: Option<String>) -> impl IntoView {
    let library = expect_context::<Library>();
    let profile = RwSignal::new(None::<RaProfile>);
    let loading = RwSignal::new(linked.is_some());
    let user = RwSignal::new(String::new());
    let key = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    if linked.is_some() {
        spawn_local(async move {
            let got = library.report(api::ra_profile().await).flatten();
            loading.set(false);
            profile.set(got);
        });
    }
    let connect = move |e: leptos::ev::SubmitEvent| {
        e.prevent_default();
        busy.set(true);
        spawn_local(async move {
            let got = library.report(api::ra_connect(user.get_untracked(), key.get_untracked()).await);
            busy.set(false);
            if let Some(p) = got {
                library.toast(tr!("Conectado ao RetroAchievements como {}.", p.user));
                key.set(String::new());
                profile.set(Some(p));
            }
        });
    };
    let disconnect = move |_| {
        spawn_local(async move {
            if library.report(api::ra_disconnect().await).is_some() {
                profile.set(None);
                library.toast(tr!("Conta do RetroAchievements desconectada deste PC."));
            }
        });
    };
    let open = move |keys: bool| spawn_local(async move { library.report(api::ra_open(keys).await); });

    move || {
        if loading.get() {
            return view! { <section class="card set"><h3>"RetroAchievements"</h3><p class="muted">{tr!("Lendo sua conta...")}</p></section> }.into_any();
        }
        match profile.get() {
            Some(p) => {
                let games = p
                    .games
                    .iter()
                    .map(|g| {
                        let percent = if g.total > 0 { g.achieved * 100 / g.total } else { 0 };
                        view! {
                            <div class="ra-game">
                                {g.icon.clone().map(|i| view! { <img src=i alt="" /> })}
                                <div class="ra-game-text">
                                    <b>{g.title.clone()}</b>
                                    <small>{tr!("{} · {} de {} conquistas", g.console, g.achieved, g.total)}</small>
                                    <div class="ra-bar"><i style=format!("width:{percent}%")></i></div>
                                </div>
                            </div>
                        }
                    })
                    .collect_view();
                let now = (!p.playing_now.is_empty()).then(|| view! { <small>{tr!("Agora: {}", p.playing_now)}</small> });
                view! {
                    <section class="card set">
                        <h3>"RetroAchievements"</h3>
                        <div class="ra-head">
                            {p.picture.clone().map(|src| view! { <img class="ra-avatar" src=src alt="" /> })}
                            <div class="ra-who">
                                <b>{p.user.clone()}</b>
                                <small>{format!("{} pontos hardcore · {} softcore · {} pontos reais", format::count(p.points), format::count(p.softcore_points), format::count(p.true_points))}</small>
                                {now}
                            </div>
                            <div class="row">
                                <button type="button" class="btn sm" on:click=move |_| open(false)><Svg icon=Icon::External size=16 />{tr!("Ver perfil")}</button>
                                <button type="button" class="btn sm" on:click=disconnect>{tr!("Desconectar")}</button>
                            </div>
                        </div>
                        <h4 class="ra-sub">{tr!("Jogados recentemente")}</h4>
                        <div class="ra-games">{games}</div>
                        <p class="muted small">{tr!("As conquistas aparecem quando você joga num emulador conectado à sua conta. Quando a emulação chegar ao Cerberus, ele conecta os emuladores para você.")}</p>
                    </section>
                }
                .into_any()
            }
            None => view! {
                <section class="card set">
                    <h3>"RetroAchievements"</h3>
                    <p class="muted small">{tr!("Conquistas para jogos clássicos, as primeiras do Cerberus. Conecte sua conta com o seu usuário e a sua chave da API (em retroachievements.org, Configurações › Chaves). A chave fica só neste PC.")}</p>
                    <form class="ra-form" on:submit=connect>
                        <input class="input" placeholder=tr!("Usuário") autocomplete="username" prop:value=move || user.get() on:input=move |e| user.set(event_target_value(&e)) />
                        <input class="input" type="password" placeholder=tr!("Chave da Web API") prop:value=move || key.get() on:input=move |e| key.set(event_target_value(&e)) />
                        <button type="submit" class="btn sm main" disabled=move || busy.get() || user.get().trim().is_empty() || key.get().trim().is_empty()>
                            {move || if busy.get() { tr!("Conectando...") } else { tr!("Conectar") }}
                        </button>
                    </form>
                    <button type="button" class="link" on:click=move |_| open(true)>{tr!("Pegar minha chave no RetroAchievements")}</button>
                </section>
            }
            .into_any(),
        }
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
            let count = if source.hydra { tr!("Hydra · {} publicações", source.games) }
                else if source.synced_at.is_some() { tr!("{} projetos em Descobrir", source.games) }
                else { String::new() };
            view! {
                <div>
                    <div class="set-row sub">
                        <div>
                            <b>{source.name}</b>
                            <small class="path">{source.url}</small>
                            <small>{count}</small>
                            {source.synced_at.map(|t| view! { <small>{tr!("Última sincronização em {}", format::date_time(t))}</small> })}
                            {source.error.map(|e| view! { <small role="status">{e}</small> })}
                        </div>
                        <div class="row">
                            {source.hydra.then(|| view! {
                                <button type="button" class="btn sm" aria-expanded=move || opened.get().to_string()
                                    on:click=move |_| opened.update(|v| *v = !*v)>
                                    {move || if opened.get() { tr!("Fechar lista") } else { tr!("Consultar") }}
                                </button>
                            })}
                            <button type="button" class="btn sm" disabled=move || busy.get()
                                on:click=move |_| refresh(target.get_value())>{tr!("Atualizar")}</button>
                            <button type="button" class="btn sm" disabled=move || busy.get()
                                on:click=move |_| apply_source(target.get_value(), false)>{tr!("Remover")}</button>
                        </div>
                    </div>
                    {move || opened.get().then(|| view! { <HydraListings url=target.get_value() /> })}
                </div>
            }
        })
        .collect_view();
    view! {
        <section class="card set">
            <h3>{tr!("Fontes de catálogo")}</h3>
            <form class="set-row" on:submit=on_add>
                <div>
                    <b>{tr!("Adicionar por URL")}</b>
                    <small>{tr!("Consulte títulos, tamanhos e datas de listas Hydra aqui. Fontes de projetos para instalar aparecem em Descobrir.")}</small>
                </div>
                <div class="row">
                    <input class="input" type="url" required aria-label=tr!("URL da fonte") placeholder=tr!("https://.../jogos.json") prop:value=move || url.get() on:input=move |e| url.set(event_target_value(&e)) />
                    <button type="submit" class="btn sm main" disabled=move || busy.get()>{tr!("Adicionar")}</button>
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
        let Some(result) = page.get() else { return view! { <p>{tr!("Carregando lista…")}</p> }.into_any(); };
        let page = match result {
            Ok(p) => p,
            Err(e) => return view! { <p role="status">{e}</p> }.into_any(),
        };
        if page.items.is_empty() { return view! { <p>{tr!("Nenhuma publicação encontrada.")}</p> }.into_any(); }
        let start = offset.get_untracked();
        let end = start + page.items.len();
        let first_page = start == 0;
        let last_page = end >= page.total;
        let rows = page.items.into_iter().map(|item| view! {
            <tr><th scope="row">{item.title}</th><td>{item.file_size.unwrap_or_else(|| "—".into())}</td>
                <td>{item.upload_date.unwrap_or_else(|| "—".into())}</td></tr>
        }).collect_view();
        view! {
            <table><thead><tr><th>{tr!("Título")}</th><th>{tr!("Tamanho")}</th><th>{tr!("Data publicada pela fonte")}</th></tr></thead><tbody>{rows}</tbody></table>
            <div class="row">
                <small>{tr!("{}–{} de {}", start + 1, end, page.total)}</small>
                <button class="btn sm" disabled=first_page on:click=move |_| offset.set(start.saturating_sub(25))>{tr!("Anterior")}</button>
                <button class="btn sm" disabled=last_page on:click=move |_| offset.set(end)>{tr!("Próxima")}</button>
            </div>
        }.into_any()
    };
    view! {
        <div class="source-listings">
            <input class="input" type="search" aria-label=tr!("Buscar título na fonte") placeholder=tr!("Buscar título nesta fonte")
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
                library.toast(if key.trim().is_empty() { tr!("Chave do SteamGridDB removida.") } else { tr!("Chave do SteamGridDB guardada.") });
            }
        });
    };
    view! {
        <section class="card set">
            <h3>{tr!("Arte dos jogos")}</h3>
            <div class="set-row">
                <div>
                    <b>{tr!("Chave do SteamGridDB")}</b>
                    <small>{move || if saved.get() { tr!("Guardada. Use \"Personalizar arte\" na aba Gerenciar de cada jogo.") } else { tr!("Gratuita: crie uma conta no SteamGridDB e gere a chave de API.") }}</small>
                </div>
                <div class="row">
                    <button type="button" class="btn sm" on:click=move |_| spawn_local(async move { library.report(api::open_steamgriddb_keys().await); })>{tr!("Pegar chave")}</button>
                    <Show when=move || saved.get()>
                        <button type="button" class="btn sm" disabled=move || busy.get() on:click=move |_| store(String::new())>{tr!("Remover")}</button>
                    </Show>
                </div>
            </div>
            <form class="set-row sub" on:submit=move |e: leptos::ev::SubmitEvent| { e.prevent_default(); store(input.get_untracked()); }>
                <input class="input" type="password" placeholder=move || if saved.get() { tr!("Trocar a chave") } else { tr!("Cole a chave aqui") }
                    prop:value=move || input.get() on:input=move |e| input.set(event_target_value(&e)) />
                <button type="submit" class="btn sm main" disabled=move || busy.get() || input.get().trim().is_empty()>{tr!("Guardar")}</button>
            </form>
        </section>
    }
}
