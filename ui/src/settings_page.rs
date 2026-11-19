//! Configurações: a menu of sections on the left, grouped (App, Jogos,
//! Clássicos, Contas, Sistema), and the chosen section on the right as cards
//! of rows: a title, a line about it, and its control.

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
    ("App", &[(Section::General, "Geral", Icon::Settings), (Section::Appearance, "Aparência", Icon::Edit), (Section::BigPicture, "Big Picture Mode", Icon::Tv)]),
    (
        "Jogos",
        &[
            (Section::Library, "Biblioteca e ROMs", Icon::Library),
            (Section::Install, "Instalação", Icon::Download),
            (Section::Saves, "Saves e nuvem", Icon::Saves),
            (Section::Art, "Arte dos jogos", Icon::Edit),
            (Section::Catalog, "Catálogo e fontes", Icon::Discover),
        ],
    ),
    ("Clássicos", &[(Section::Emulation, "Emulação", Icon::Gamepad)]),
    ("Contas", &[(Section::Retro, "RetroAchievements", Icon::Trophy)]),
    ("Sistema", &[(Section::Pc, "Seu PC", Icon::Check), (Section::Storage, "Armazenamento", Icon::Folder), (Section::About, "Sobre", Icon::Info)]),
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
                            <Svg icon=*icon size=18 />{*label}
                        </button>
                    }
                })
                .collect_view();
            view! { <div class="set-group-label">{*group}</div>{links} }
        })
        .collect_view();

    let body = move || {
        let Some(s) = view_state.get() else {
            return view! { <p class="muted">"Carregando..."</p> }.into_any();
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
                        if add { apply(Box::pin(api::add_source(url)), "Fonte adicionada.") } else { apply(Box::pin(api::remove_source(url)), "Fonte removida.") }
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
            <header class="page-h"><div><h1>"Configurações"</h1></div></header>
            <div class="set-layout">
                <nav class="set-nav" aria-label="Seções">{menu}</nav>
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
            <h3>"Ao abrir"</h3>
            {row("Página inicial", "Onde a janela abre. Quem joga no Big Picture Mode abre direto nele.",
                view! { <div class="segs" role="radiogroup">{choice("home", "Início")}{choice("library", "Biblioteca")}{choice("discover", "Descobrir")}</div> })}
            {row("Configuração inicial", "As telas de boas-vindas da primeira vez, para rever pastas, instalação e nuvem.",
                view! { <button type="button" class="btn sm" disabled=move || busy.get()
                    on:click=move |_| apply(Box::pin(api::rerun_setup()), "Na próxima vez que o Cerberus abrir, a configuração inicial aparece.")>"Mostrar de novo"</button> })}
        </section>
        <section class="card set">
            <h3>"Ao ligar o PC"</h3>
            {row("Abrir com o Windows", "O Cerberus abre sozinho quando você entra no Windows, pronto para os avisos de atualização.",
                switch(s.autostart, "Abrir com o Windows", busy, move || {
                    let on = !s.autostart;
                    apply(Box::pin(api::set_autostart(on)), if on { "O Cerberus vai abrir com o Windows." } else { "O Cerberus não abre mais com o Windows." })
                }))}
            {row("Começar escondido na bandeja", "Ao abrir com o Windows, fica perto do relógio sem mostrar a janela. Abrir pelo atalho mostra a janela normalmente.",
                switch(s.start_hidden, "Começar escondido na bandeja", busy, move || {
                    let on = !s.start_hidden;
                    apply(Box::pin(api::set_start_hidden(on)), if on { "Ao abrir com o Windows, o Cerberus fica na bandeja." } else { "Ao abrir com o Windows, a janela aparece." })
                }))}
        </section>
        <section class="card set">
            <h3>"Ao fechar a janela"</h3>
            {row("Continuar na bandeja", "O Cerberus fica perto do relógio do Windows, pronto para abrir. Para sair de vez, clique com o botão direito no ícone dele e escolha Sair.",
                switch(s.close_to_tray, "Continuar na bandeja", busy, move || {
                    let on = !s.close_to_tray;
                    apply(Box::pin(api::set_close_to_tray(on)), if on { "Ao fechar a janela, o Cerberus fica na bandeja." } else { "Ao fechar a janela, o Cerberus fecha." })
                }))}
            <p class="muted small">"Com um jogo aberto, fechar a janela sempre deixa o Cerberus na bandeja, para contar o tempo de jogo e guardar os saves no fim da partida. Ele fecha sozinho quando o jogo terminar."</p>
        </section>
    }
}

fn appearance() -> impl IntoView {
    let calm = RwSignal::new(calm_saved());
    let busy = RwSignal::new(false);
    view! {
        <section class="card set">
            <h3>"Aparência"</h3>
            {move || row("Menos animações", "Sem transições nem movimento, para PCs mais simples ou para quem prefere assim.",
                switch(calm.get(), "Menos animações", busy, move || { let on = !calm.get_untracked(); save_calm(on); calm.set(on); }))}
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
                <h3>"Big Picture Mode"</h3>
                <button type="button" class="btn sm main" on:click=move |_| library.go(Route::BigPicture)><Svg icon=Icon::Tv size=16 />"Abrir agora"</button>
            </div>
            {row("Abrir no Big Picture Mode", "Tela cheia e feita para controle, toda vez que o Cerberus abrir.",
                switch(on, "Abrir no Big Picture Mode", busy, move || apply(Box::pin(api::set_start_in_tv(!on)), "")))}
            {move || row("Sons de navegação", "Um toque curto ao mover, escolher e voltar.",
                switch(sounds.get(), "Sons de navegação", local, move || { let v = !sounds.get_untracked(); crate::tv::set_sound_enabled(v); sounds.set(v); }))}
            {row("Entrar e sair pelo controle", "Segure o botão Menu por um segundo, ou aperte F11 no teclado.", ())}
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
                        on:click=move |_| apply(Box::pin(api::remove_rom_dir_owned(target.clone())), "Pasta removida.")>"Remover"</button>
                </div>
            }
        })
        .collect_view();
    let summary = if s.rom_dirs.is_empty() {
        "Aponte as pastas onde você guarda ROMs e imagens de disco. O Cerberus mostra quais desses jogos têm recomp. Nada sai do seu PC.".to_string()
    } else {
        format!("{} arquivos de jogo encontrados · {} recomps para jogos que você tem.", s.roms_found, s.roms_matched)
    };
    let scan = (!s.rom_dirs.is_empty()).then(|| {
        view! { <button type="button" class="btn sm" disabled=move || busy.get() on:click=move |_| apply(Box::pin(api::scan_roms()), "Pastas verificadas de novo.")>"Procurar de novo"</button> }
    });
    view! {
        <section class="card set">
            <h3>"Suas ROMs e jogos"</h3>
            {row("Pastas de ROMs e jogos", summary, view! {
                {scan}
                <button type="button" class="btn sm main" disabled=move || busy.get() on:click=move |_| apply(Box::pin(api::add_rom_dir()), "")>
                    <Svg icon=Icon::Plus size=16 />"Adicionar pasta"
                </button>
            })}
            {rom_rows}
        </section>
    }
}

fn install(s: SettingsView, busy: RwSignal<bool>, apply: impl Fn(Task, &'static str) + Copy + 'static) -> impl IntoView {
    let reset = (!s.recomps_dir_is_default).then(|| {
        view! { <button type="button" class="btn sm" disabled=move || busy.get() on:click=move |_| apply(Box::pin(api::reset_recomps_dir()), "Novos recomps vão para a pasta padrão.")>"Usar a padrão"</button> }
    });
    let free = s.install_free_bytes.map(|free| format!(" Espaço livre: {:.1} GiB.", free as f64 / 1073741824.0)).unwrap_or_default();
    view! {
        <section class="card set">
            <h3>"Instalação"</h3>
            {row("Pasta dos recomps", view! { <span class="path">{s.recomps_dir.clone()}</span><br/>{format!("Jogos já instalados continuam onde estão; só as próximas instalações vão para a nova pasta.{free}")} }, view! {
                {reset}
                <button type="button" class="btn sm" disabled=move || busy.get() on:click=move |_| apply(Box::pin(api::pick_recomps_dir()), "")>
                    <Svg icon=Icon::Folder size=16 />"Escolher pasta"
                </button>
            })}
        </section>
    }
}

fn saves(s: SettingsView, busy: RwSignal<bool>, apply: impl Fn(Task, &'static str) + Copy + 'static) -> impl IntoView {
    let pending = (s.pending_uploads > 0).then(|| format!(" Envios pendentes na última verificação: {}. O Cerberus tenta de novo a cada minuto.", s.pending_uploads)).unwrap_or_default();
    let on = s.cloud_dir.is_some();
    view! {
        <section class="card set">
            <h3>"Saves na nuvem"</h3>
            {row(if on { "Ligado" } else { "Desligado" }, view! {
                {s.cloud_dir.clone().map(|d| view! { <span class="path">{d}</span><br/> })}
                {format!("Escolha uma pasta que o OneDrive, Google Drive ou Dropbox sincroniza. Ao abrir um jogo, o Cerberus guarda o progresso local antes de trazer saves mais novos de outro PC.{pending}")}
            }, view! {
                {on.then(|| view! {
                    <button type="button" class="btn sm" disabled=move || busy.get() on:click=move |_| apply(Box::pin(api::retry_save_uploads()), "Envios verificados.")>"Verificar envios"</button>
                    <button type="button" class="btn sm" disabled=move || busy.get() on:click=move |_| apply(Box::pin(api::set_cloud_dir(true)), "Saves na nuvem desligados.")>"Desligar"</button>
                })}
                <button type="button" class="btn sm main" disabled=move || busy.get() on:click=move |_| apply(Box::pin(api::set_cloud_dir(false)), "")>
                    <Svg icon=Icon::Folder size=16 />"Escolher pasta"
                </button>
            })}
            {row("Backups neste PC", "Depois de cada partida, antes de atualizar e antes de desinstalar, o Cerberus guarda os 10 backups mais recentes de cada jogo.", ())}
        </section>
    }
}

fn art_cache(s: SettingsView, busy: RwSignal<bool>, apply: impl Fn(Task, &'static str) + Copy + 'static) -> impl IntoView {
    view! {
        <section class="card set">
            <h3>"Imagens guardadas"</h3>
            {row("Capas e fundos", format!("{} guardados para abrir rápido e funcionar offline.", format::megabytes(s.art_cache_bytes)),
                view! { <button type="button" class="btn sm" disabled=move || busy.get()
                    on:click=move |_| apply(Box::pin(api::clear_art_cache()), "Imagens apagadas. Elas voltam conforme as páginas precisarem.")>"Limpar"</button> })}
        </section>
    }
}

fn catalog(s: SettingsView, busy: RwSignal<bool>, apply: impl Fn(Task, &'static str) + Copy + 'static) -> impl IntoView {
    view! {
        <section class="card set">
            <h3>"Catálogo de recomps"</h3>
            {row(format!("{} recomps", s.catalog_count), if s.site_catalog { format!("Lista de {}, atualizada sozinha a cada 6 horas, mais o catálogo próprio do Cerberus. Cada recomp é baixado da página do próprio desenvolvedor (GitHub, GitLab ou o servidor do projeto).", s.catalog_source) } else { "Só o catálogo próprio do Cerberus e as suas fontes. Cada recomp é baixado da página do próprio desenvolvedor.".to_string() },
                view! { <button type="button" class="btn sm" disabled=move || busy.get() on:click=move |_| apply(Box::pin(api::refresh_catalog()), "Catálogo atualizado.")>"Atualizar agora"</button> })}
            {row("Catálogo do recomplauncher.com", "A lista grande, com quase 1.900 recomps de mais de 25 consoles. Desligado, o Descobrir mostra só o catálogo do Cerberus e as suas fontes.",
                switch(s.site_catalog, "Catálogo do recomplauncher.com", busy, move || {
                    let on = !s.site_catalog;
                    apply(Box::pin(api::set_site_catalog(on)), if on { "Catálogo do recomplauncher.com ligado." } else { "Usando só o catálogo do Cerberus." })
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
            return view! { <section class="card set"><h3>"Emulação"</h3><p class="muted">"Verificando..."</p></section> }.into_any();
        };
        let working = busy.get();
        let status = match (&v.retroarch, &working) {
            (_, Some(label)) if label == "install" => {
                let (done, total) = progress.get();
                let text = if total > 0 && done >= total {
                    "Extraindo o RetroArch... leva um minuto.".to_string()
                } else if total > 0 {
                    format!("Baixando o RetroArch · {} de {}", format::megabytes(done), format::megabytes(total))
                } else {
                    "Preparando o RetroArch...".to_string()
                };
                view! { <small>{text}</small> }.into_any()
            }
            (Some(path), _) => view! { <small><span class="path">{path.clone()}</span><br/>{if v.managed { "Instalado pelo Cerberus, do site oficial do libretro." } else { "O seu RetroArch." }}</small> }.into_any(),
            (None, _) => view! { <small>"O RetroArch roda NES, Super Nintendo, Nintendo 64, Game Boy, GBA, Mega Drive, PlayStation, PlayStation 2, PSP, GameCube e Wii. O Cerberus instala a versão oficial (cerca de 200 MB) numa pasta só dele, ou usa o seu."</small> }.into_any(),
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
                    view! { <span class="pill ok"><Svg icon=Icon::Check size=14 />"Instalado"</span> }.into_any()
                } else {
                    let core = core.clone();
                    view! {
                        <button type="button" class="btn sm" disabled=move || disabled() || !has
                            on:click=move |_| {
                                let name = core.clone();
                                run(name.clone(), Box::pin(api::install_core(name)), "Núcleo instalado.");
                            }>
                            {if installing { "Baixando..." } else { "Instalar" }}
                        </button>
                    }
                    .into_any()
                };
                view! {
                    <div class="set-row sub">
                        <div><b>{c.consoles.clone()}</b><small>{format!("Núcleo {}", c.core)}</small></div>
                        {control}
                    </div>
                }
            })
            .collect_view();
        let retro = match (&v.ra_user, ra_linked) {
            (Some(user), _) => view! { <span class="pill ok"><Svg icon=Icon::Trophy size=14 />{format!("Conectado como {user}")}</span> }.into_any(),
            (None, false) => view! { <small>"Conecte sua conta em Contas › RetroAchievements primeiro."</small> }.into_any(),
            (None, true) => view! {
                <form class="row" on:submit=move |e: leptos::ev::SubmitEvent| {
                    e.prevent_default();
                    let pass = password.get_untracked();
                    password.set(String::new());
                    run("ra".to_string(), Box::pin(api::link_retroarch_achievements(pass)), "RetroArch conectado ao RetroAchievements.");
                }>
                    <input class="input" type="password" placeholder="Sua senha do RetroAchievements" prop:value=move || password.get()
                        on:input=move |e| password.set(event_target_value(&e)) />
                    <button type="submit" class="btn sm main" disabled=move || disabled() || !has || password.get().is_empty()>"Conectar"</button>
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
                            on:click=move |_| run("pick".to_string(), Box::pin(api::pick_retroarch()), "")>"Usar o meu"</button>
                        <button type="button" class="btn sm main" disabled=disabled
                            on:click=move |_| run("install".to_string(), Box::pin(api::install_retroarch()), "RetroArch instalado.")>
                            <Svg icon=Icon::Download size=16 />{if has { "Reinstalar" } else { "Instalar" }}
                        </button>
                    </div>
                </div>
                {row(if has { "Pronto" } else { "Não instalado" }, status, ())}
            </section>
            <section class="card set">
                <h3>"Núcleos"</h3>
                <p class="muted small">"Cada console usa um núcleo do RetroArch, baixado do site oficial do libretro."</p>
                {cores}
            </section>
            <section class="card set">
                <h3>"BIOS"</h3>
                <p class="muted small">"A BIOS é o programa interno do console. O PlayStation 2 só roda com a do seu próprio console; o PlayStation roda sem, mas fica mais compatível com ela. O Cerberus nunca baixa BIOS: aponte a pasta onde você guarda as suas."</p>
                {row("PlayStation", if v.ps1_bios { "BIOS encontrada." } else { "Opcional. Arquivo de 512 KB, como scph5501.bin." },
                    if v.ps1_bios { view! { <span class="pill ok"><Svg icon=Icon::Check size=14 />"Pronta"</span> }.into_any() } else { view! { <span class="pill soon">"Embutida"</span> }.into_any() })}
                {row("PlayStation 2", if v.ps2_bios { "BIOS encontrada." } else { "Necessária. Arquivos de 4 MB, como SCPH-70012.bin." },
                    if v.ps2_bios { view! { <span class="pill ok"><Svg icon=Icon::Check size=14 />"Pronta"</span> }.into_any() } else { view! { <span class="pill soon">"Falta"</span> }.into_any() })}
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
                        <Svg icon=Icon::Folder size=16 />"Adicionar BIOS de uma pasta"
                    </button>
                </div>
            </section>
            <section class="card set">
                <h3>"Conquistas no RetroArch"</h3>
                {row("RetroAchievements", "O Cerberus troca sua senha por um código de acesso no site e grava só o código no RetroArch. A senha não fica guardada. O arquivo de configurações do RetroArch ganha uma cópia de segurança antes.", retro)}
            </section>
            <section class="card set">
                {row("Seus clássicos", format!("{roms} jogos encontrados nas pastas de ROMs. Jogue pela aba Clássicos da Biblioteca."),
                    view! { <button type="button" class="btn sm" on:click=move |_| library.go(Route::Library)>"Ver na Biblioteca"</button> })}
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
                library.toast(if s.dx_missing.is_empty() { "DirectX instalado." } else { "O DirectX terminou, mas ainda falta uma parte." });
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
                library.toast("Visual C++ instalado.");
            }
        });
    };
    move || {
        let s = state.get()?;
        let (about, pill) = match (&s.vc_version, s.vc_ok) {
            (Some(v), true) => (format!("Versão {v}. Em dia para os recomps de hoje."), view! { <span class="pill ok"><Svg icon=Icon::Check size=14 />"Em dia"</span> }.into_any()),
            (Some(v), false) => (format!("Versão {v}, antiga: recomps novos fecham ao abrir (erro 0xC0000139)."), view! { <span class="pill soon">"Antigo"</span> }.into_any()),
            (None, _) => ("Não instalado. Sem ele, a maioria dos recomps não abre (erro 0xC0000135).".to_string(), view! { <span class="pill soon">"Falta"</span> }.into_any()),
        };
        // Up to date, reinstalling is only a repair: no call to action.
        let label = match (&s.vc_version, s.vc_ok) {
            (Some(_), true) => "Reinstalar",
            (Some(_), false) => "Atualizar",
            (None, _) => "Instalar",
        };
        let calm = s.vc_ok;
        let dx_ok = s.dx_missing.is_empty();
        let (dx_about, dx_pill) = if dx_ok {
            ("D3DX, XInput 1.3 e XAudio 2.7 instalados, para os ports de PC mais antigos.".to_string(), view! { <span class="pill ok"><Svg icon=Icon::Check size=14 />"Em dia"</span> }.into_any())
        } else {
            (format!("Falta: {}. Ports de PC mais antigos fecham ao abrir sem eles (erro 0xC0000135).", s.dx_missing.join(", ")), view! { <span class="pill soon">"Falta"</span> }.into_any())
        };
        Some(view! {
            <section class="card set">
                <h3>"Seu PC"</h3>
                <p class="muted small">"O que os jogos precisam no Windows. Quando um jogo fecha logo ao abrir, o Cerberus diz qual destes falta."</p>
                {row("Visual C++ 2015-2022 (64 bits)", about, pill)}
                <div class="row">
                    <button type="button" class="btn sm" class:main=!calm disabled=move || working.get() on:click=install>
                        {move || if working.get() { "Instalando... confirme no Windows".to_string() } else { format!("{label} pelo site da Microsoft") }}
                    </button>
                </div>
                {row("DirectX (componentes antigos)", dx_about, dx_pill)}
                <div class="row">
                    <button type="button" class="btn sm" class:main=!dx_ok disabled=move || working_dx.get() on:click=install_dx>
                        {move || if working_dx.get() { "Instalando... confirme no Windows" } else if dx_ok { "Reinstalar pelo site da Microsoft" } else { "Instalar pelo site da Microsoft" }}
                    </button>
                </div>
                <p class="muted small">"Os instaladores oficiais são baixados da Microsoft e o Windows pede sua permissão."</p>
            </section>
        })
    }
}

fn storage(s: SettingsView, busy: RwSignal<bool>, apply: impl Fn(Task, &'static str) + Copy + 'static) -> impl IntoView {
    let library = expect_context::<Library>();
    let free = s.download_free_bytes.map(|free| format!(" Espaço livre para downloads: {:.1} GiB.", free as f64 / 1073741824.0)).unwrap_or_default();
    view! {
        <section class="card set">
            <h3>"Armazenamento"</h3>
            {row("Dados do Cerberus", view! { <span class="path">{s.data_dir.clone()}</span><br/>{format!("Biblioteca, backups de saves e caches.{free}")} },
                view! { <button type="button" class="btn sm" on:click=move |_| spawn_local(async move { library.report(api::open_data_dir().await); })>"Abrir pasta"</button> })}
        </section>
        {art_cache(s, busy, apply)}
    }
}

fn about(s: SettingsView, _busy: RwSignal<bool>, _apply: impl Fn(Task, &'static str) + Copy + 'static) -> impl IntoView {
    let library = expect_context::<Library>();
    view! {
        <section class="card set">
            <h3>"Sobre"</h3>
            <div class="set-row">
                <div>
                    <b>{format!("Cerberus Launcher {}", s.version)}</b>
                    <small>"Código aberto, licença MIT. Os recomps vêm das releases dos próprios desenvolvedores, e o Cerberus nunca baixa arquivos de jogo."</small>
                </div>
                <img class="about-logo" src="brand/cerberus-launcher-pixel.svg" alt="Logo do Cerberus Launcher" />
            </div>
            {row("Pedir um recomp", "Sugira um projeto para o catálogo, pelo GitHub do Cerberus.",
                view! { <button type="button" class="btn sm" on:click=move |_| spawn_local(async move { library.report(api::open_recomp_request().await); })><Svg icon=Icon::External size=16 />"Abrir"</button> })}
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
                library.toast(format!("Conectado ao RetroAchievements como {}.", p.user));
                key.set(String::new());
                profile.set(Some(p));
            }
        });
    };
    let disconnect = move |_| {
        spawn_local(async move {
            if library.report(api::ra_disconnect().await).is_some() {
                profile.set(None);
                library.toast("Conta do RetroAchievements desconectada deste PC.");
            }
        });
    };
    let open = move |keys: bool| spawn_local(async move { library.report(api::ra_open(keys).await); });

    move || {
        if loading.get() {
            return view! { <section class="card set"><h3>"RetroAchievements"</h3><p class="muted">"Lendo sua conta..."</p></section> }.into_any();
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
                                    <small>{format!("{} · {} de {} conquistas", g.console, g.achieved, g.total)}</small>
                                    <div class="ra-bar"><i style=format!("width:{percent}%")></i></div>
                                </div>
                            </div>
                        }
                    })
                    .collect_view();
                let now = (!p.playing_now.is_empty()).then(|| view! { <small>{format!("Agora: {}", p.playing_now)}</small> });
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
                                <button type="button" class="btn sm" on:click=move |_| open(false)><Svg icon=Icon::External size=16 />"Ver perfil"</button>
                                <button type="button" class="btn sm" on:click=disconnect>"Desconectar"</button>
                            </div>
                        </div>
                        <h4 class="ra-sub">"Jogados recentemente"</h4>
                        <div class="ra-games">{games}</div>
                        <p class="muted small">"As conquistas aparecem quando você joga num emulador conectado à sua conta. Quando a emulação chegar ao Cerberus, ele conecta os emuladores para você."</p>
                    </section>
                }
                .into_any()
            }
            None => view! {
                <section class="card set">
                    <h3>"RetroAchievements"</h3>
                    <p class="muted small">"Conquistas para jogos clássicos, as primeiras do Cerberus. Conecte sua conta com o seu usuário e a sua chave da API (em retroachievements.org, Configurações › Chaves). A chave fica só neste PC."</p>
                    <form class="ra-form" on:submit=connect>
                        <input class="input" placeholder="Usuário" autocomplete="username" prop:value=move || user.get() on:input=move |e| user.set(event_target_value(&e)) />
                        <input class="input" type="password" placeholder="Chave da Web API" prop:value=move || key.get() on:input=move |e| key.set(event_target_value(&e)) />
                        <button type="submit" class="btn sm main" disabled=move || busy.get() || user.get().trim().is_empty() || key.get().trim().is_empty()>
                            {move || if busy.get() { "Conectando..." } else { "Conectar" }}
                        </button>
                    </form>
                    <button type="button" class="link" on:click=move |_| open(true)>"Pegar minha chave no RetroAchievements"</button>
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
