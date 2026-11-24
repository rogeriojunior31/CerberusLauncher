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
    InGame,
    Notices,
    BigPicture,
    Controllers,
    Library,
    Install,
    Saves,
    Art,
    Catalog,
    Emulation,
    Steam,
    Epic,
    Retro,
    Pc,
    Storage,
    About,
}

const MENU: &[(&str, &[(Section, &str, Icon)])] = &[
    (tk!("App"), &[(Section::General, tk!("Geral"), Icon::Settings), (Section::Appearance, tk!("Aparência"), Icon::Edit), (Section::InGame, tk!("Durante o jogo"), Icon::Gamepad), (Section::Notices, tk!("Notificações"), Icon::Bell), (Section::BigPicture, tk!("Big Picture Mode"), Icon::Tv), (Section::Controllers, tk!("Controles"), Icon::Gamepad)]),
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
    (tk!("Contas"), &[(Section::Steam, "Steam", Icon::Gamepad), (Section::Epic, "Epic Games", Icon::Gamepad), (Section::Retro, "RetroAchievements", Icon::Trophy)]),
    (tk!("Sistema"), &[(Section::Pc, tk!("Seu PC"), Icon::Check), (Section::Storage, tk!("Armazenamento"), Icon::Folder), (Section::About, tk!("Sobre"), Icon::Info)]),
];

/// What the search finds: each section's options, by the titles their rows show.
const FIND: &[(Section, &[&str])] = &[
    (Section::General, &[tk!("Página inicial"), "Monitor", tk!("Configuração inicial"), tk!("Abrir com o Windows"), tk!("Começar escondido na bandeja"), tk!("Continuar na bandeja")]),
    (Section::Appearance, &[tk!("Idioma"), tk!("Tema"), tk!("Cor de destaque"), tk!("Tamanho"), tk!("Menos animações")]),
    (Section::InGame, &[tk!("Menu sobre o jogo"), tk!("Fotos com F12"), tk!("Sair da frente enquanto joga"), tk!("Aviso de conquista"), tk!("Mostrar sobre o jogo"), tk!("Canto da tela")]),
    (Section::Notices, &[tk!("Jogo pronto para jogar"), tk!("Jogo fechou com erro"), tk!("Foto salva"), tk!("Avisos do Windows"), tk!("Conquistas")]),
    (Section::BigPicture, &[tk!("Abrir no Big Picture Mode"), tk!("Sons de navegação"), tk!("Entrar e sair pelo controle")]),
    (Section::Controllers, &[tk!("Controles conectados"), tk!("No Big Picture Mode")]),
    (Section::Library, &[tk!("Pastas de ROMs e jogos")]),
    (Section::Install, &[tk!("Pasta dos recomps"), tk!("Limite de velocidade"), tk!("Mais devagar enquanto você joga")]),
    (Section::Saves, &[tk!("Saves na nuvem"), tk!("Backups neste PC")]),
    (Section::Art, &[tk!("Chave do SteamGridDB"), tk!("Capas e fundos")]),
    (Section::Catalog, &[tk!("Catálogo do recomplauncher.com"), tk!("Fontes de catálogo"), tk!("Adicionar por URL")]),
    (Section::Emulation, &[tk!("Núcleos"), tk!("Conquistas no RetroArch"), tk!("Seus clássicos"), tk!("Emuladores"), tk!("Quem abre cada console"), tk!("Usar as pastas dos emuladores")]),
    (Section::Steam, &[tk!("Chave da Web API da Steam"), tk!("Privacidade do perfil"), tk!("Adicionar todos à Steam")]),
    (Section::Retro, &[]),
    (Section::Pc, &[tk!("Visual C++ 2015-2022 (64 bits)")]),
    (Section::Storage, &[tk!("Dados do Cerberus"), tk!("Copiar meus dados"), tk!("Restaurar de uma cópia")]),
    (Section::About, &[tk!("Atualizações"), tk!("Pedir um recomp")]),
];

/// Lower case and without accents, so "notificacao" finds "Notificações".
fn plain(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            c => c,
        })
        .collect()
}

/// A search hit: the section, its menu label and icon, and the option's title (none: the section itself).
type Hit = (Section, &'static str, Icon, Option<&'static str>);

/// The options whose title, or section, has every word typed, in the current
/// language or in Portuguese.
fn search(query: &str) -> Vec<Hit> {
    let words: Vec<String> = plain(query).split_whitespace().map(str::to_string).collect();
    if words.is_empty() {
        return Vec::new();
    }
    let t = cerberus_shared::i18n::t;
    let has = |text: String| words.iter().all(|w| text.contains(w.as_str()));
    let mut hits = Vec::new();
    for (_, items) in MENU {
        for (which, label, icon) in items.iter() {
            let section_words = format!("{} {}", plain(t(label)), plain(label));
            if has(section_words.clone()) {
                hits.push((*which, *label, *icon, None));
            }
            let titles = FIND.iter().find(|(s, _)| s == which).map(|(_, t)| *t).unwrap_or_default();
            for title in titles {
                if has(format!("{} {} {}", plain(t(title)), plain(title), section_words)) {
                    hits.push((*which, *label, *icon, Some(*title)));
                }
            }
        }
    }
    hits
}

/// After a section opens from the search: brings the option into view and marks it for a moment.
fn point_at(title: &'static str) {
    set_timeout(
        move || {
            let Some(doc) = web_sys::window().and_then(|w| w.document()) else { return };
            let Ok(all) = doc.query_selector_all(".settings .set-row b, .settings h3") else { return };
            let wanted = cerberus_shared::i18n::t(title);
            for i in 0..all.length() {
                let Some(node) = all.item(i) else { continue };
                if node.text_content().is_some_and(|t| t.trim() == wanted) {
                    let el: web_sys::Element = wasm_bindgen::JsCast::unchecked_into(node);
                    let target = el.closest(".set-row").ok().flatten().unwrap_or(el);
                    let opts = web_sys::ScrollIntoViewOptions::new();
                    opts.set_block(web_sys::ScrollLogicalPosition::Center);
                    opts.set_behavior(web_sys::ScrollBehavior::Smooth);
                    target.scroll_into_view_with_scroll_into_view_options(&opts);
                    let _ = target.class_list().add_1("found");
                    set_timeout(move || { let _ = target.class_list().remove_1("found"); }, std::time::Duration::from_millis(1800));
                    return;
                }
            }
        },
        std::time::Duration::from_millis(80),
    );
}

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

const SIZE_KEY: &str = "cerberus-size";
/// The window's size choices, in percent.
const SIZES: [u32; 4] = [90, 100, 110, 125];

/// The size the player picked for everything in the window (Big Picture keeps its own).
pub fn size_saved() -> u32 {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(SIZE_KEY).ok().flatten())
        .and_then(|v| v.parse().ok())
        .filter(|v| SIZES.contains(v))
        .unwrap_or(100)
}

pub fn apply_size(percent: u32) {
    if let Some(root) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.document_element()) {
        let style = wasm_bindgen::JsCast::unchecked_ref::<web_sys::HtmlElement>(&root).style();
        let _ = style.set_property("--zoom", &format!("{}", f64::from(percent) / 100.0));
    }
}

fn save_size(percent: u32) {
    if let Some(s) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = s.set_item(SIZE_KEY, &percent.to_string());
    }
    apply_size(percent);
}

const THEME_KEY: &str = "cerberus-theme";
const ACCENT_KEY: &str = "cerberus-accent";
/// The backgrounds: id and name.
const THEMES: [(&str, &str); 2] = [("dark", tk!("Escuro")), ("oled", tk!("Preto (OLED)"))];
/// The colors for what to do (buttons, bars, the current page): id, name, and the swatch.
const ACCENTS: [(&str, &str, &str); 5] = [
    ("orange", tk!("Laranja"), "linear-gradient(135deg, #ff9a3d, #ff3d4d)"),
    ("blue", tk!("Azul"), "linear-gradient(135deg, #5cc8ff, #3d6bff)"),
    ("green", tk!("Verde"), "linear-gradient(135deg, #a6e45a, #1fbf75)"),
    ("purple", tk!("Roxo"), "linear-gradient(135deg, #c58bff, #7a5cff)"),
    ("pink", tk!("Rosa"), "linear-gradient(135deg, #ff8ac2, #ff3d7a)"),
];

fn stored(key: &str) -> Option<String> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten()).and_then(|s| s.get_item(key).ok().flatten())
}

/// The theme and accent picked on this PC (the first of each when none).
pub fn look_saved() -> (&'static str, &'static str) {
    let theme = stored(THEME_KEY).and_then(|v| THEMES.iter().find(|t| t.0 == v)).map_or(THEMES[0].0, |t| t.0);
    let accent = stored(ACCENT_KEY).and_then(|v| ACCENTS.iter().find(|a| a.0 == v)).map_or(ACCENTS[0].0, |a| a.0);
    (theme, accent)
}

/// Puts the theme and accent on the whole window (the stylesheet reads them).
pub fn apply_look((theme, accent): (&str, &str)) {
    if let Some(root) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.document_element()) {
        let _ = root.set_attribute("data-theme", theme);
        let _ = root.set_attribute("data-accent", accent);
    }
}

fn save_look(key: &str, value: &str) {
    if let Some(s) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = s.set_item(key, value);
    }
    apply_look(look_saved());
}

fn save_calm(on: bool) {
    if let Some(s) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = s.set_item(CALM_KEY, if on { "1" } else { "0" });
    }
    apply_calm(on);
}

thread_local! {
    /// The section the next Settings page opens on, when another page asks for one.
    static NEXT: std::cell::Cell<Option<Section>> = const { std::cell::Cell::new(None) };
}

/// Opens Configurações › Emulação.
/// Settings › Biblioteca e ROMs, where the ROM folders are.
pub fn open_rom_folders(library: Library) {
    NEXT.with(|n| n.set(Some(Section::Library)));
    library.go(crate::Route::Settings);
}

pub fn open_emulation(library: Library) {
    NEXT.with(|n| n.set(Some(Section::Emulation)));
    library.go(crate::Route::Settings);
}

#[component]
pub fn SettingsPage() -> impl IntoView {
    let library = expect_context::<Library>();
    let view_state: RwSignal<Option<SettingsView>> = RwSignal::new(None);
    let busy = RwSignal::new(false);
    let section = RwSignal::new(NEXT.with(|n| n.take()).unwrap_or(Section::General));
    let query = RwSignal::new(String::new());

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

    let open_hit = move |(which, _, _, title): Hit| {
        section.set(which);
        query.set(String::new());
        if let Some(title) = title {
            point_at(title);
        }
    };
    let hits = Memo::new(move |_| query.with(|q| search(q)));
    let results = move || {
        let list = hits.get();
        if list.is_empty() {
            return view! { <p class="muted small set-none">{tr!("Nada com esse nome.")}</p> }.into_any();
        }
        list.into_iter()
            .take(14)
            .map(|hit| {
                let (_, label, icon, title) = hit;
                let t = cerberus_shared::i18n::t;
                view! {
                    <button type="button" class="set-link set-hit" on:click=move |_| open_hit(hit)>
                        <Svg icon=icon size=18 />
                        <span>{t(title.unwrap_or(label))}{title.map(|_| view! { <small>{t(label)}</small> })}</span>
                    </button>
                }
            })
            .collect_view()
            .into_any()
    };
    let find = view! {
        <input class="input set-find" type="search" aria-label=tr!("Buscar nas configurações") placeholder=tr!("Buscar nas configurações")
            prop:value=move || query.get()
            on:input=move |e| query.set(event_target_value(&e))
            on:keydown=move |e| match e.key().as_str() {
                "Enter" => {
                    if let Some(first) = hits.get_untracked().into_iter().next() {
                        open_hit(first);
                    }
                }
                "Escape" => query.set(String::new()),
                _ => {}
            } />
    };

    let body = move || {
        let Some(s) = view_state.get() else {
            return view! { <p class="muted">{tr!("Carregando...")}</p> }.into_any();
        };
        match section.get() {
            Section::General => general(s, busy, apply).into_any(),
            Section::Appearance => appearance().into_any(),
            Section::InGame => in_game(s, busy, apply).into_any(),
            Section::Notices => notices(s, busy, apply).into_any(),
            Section::BigPicture => big_picture(s, busy, apply).into_any(),
            Section::Controllers => controllers().into_any(),
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
            Section::Steam => view! { <SteamAccountCard /> <SteamShortcutsCard /> }.into_any(),
            Section::Epic => view! { <EpicCard /> }.into_any(),
            Section::Retro => view! { <RetroAchievements linked=s.ra_user.clone() retroarch=s.ra_retroarch /> }.into_any(),
            Section::Pc => view! { <YourPc /> }.into_any(),
            Section::Storage => storage(s, busy, apply).into_any(),
            Section::About => about(s, busy, apply).into_any(),
        }
    };

    view! {
        <div class="page">
            <header class="page-h"><div><h1>{tr!("Configurações")}</h1></div></header>
            <div class="set-layout">
                <nav class="set-nav" aria-label=tr!("Seções")>
                    {find}
                    <div class="set-menu" class:hidden=move || !query.with(|q| q.trim().is_empty())>{menu}</div>
                    <Show when=move || !query.with(|q| q.trim().is_empty())>
                        <div class="set-hits">{results}</div>
                    </Show>
                </nav>
                <div class="settings">
                    // The open section's name, for screen readers (the menu shows it highlighted).
                    <h2 class="vh">{move || {
                        let now = section.get();
                        MENU.iter().flat_map(|(_, items)| items.iter()).find(|(s, _, _)| *s == now)
                            .map(|(_, label, _)| cerberus_shared::i18n::t(label)).unwrap_or_default()
                    }}</h2>
                    {body}
                </div>
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
    let size = RwSignal::new(size_saved());
    let look = RwSignal::new(look_saved());
    let busy = RwSignal::new(false);
    let themes = THEMES
        .into_iter()
        .map(|(id, name)| {
            let on = move || look.get().0 == id;
            view! {
                <button type="button" class="seg" role="radio" class:on=on aria-checked=move || on().to_string()
                    on:click=move |_| { save_look(THEME_KEY, id); look.set(look_saved()); }>{cerberus_shared::i18n::t(name)}</button>
            }
        })
        .collect_view();
    let accents = ACCENTS
        .into_iter()
        .map(|(id, name, swatch)| {
            let on = move || look.get().1 == id;
            let name = cerberus_shared::i18n::t(name);
            view! {
                <button type="button" class="swatch" role="radio" class:on=on aria-checked=move || on().to_string() aria-label=name title=name
                    style=format!("background: {swatch}") on:click=move |_| { save_look(ACCENT_KEY, id); look.set(look_saved()); }>
                    <Svg icon=Icon::Check size=16 />
                </button>
            }
        })
        .collect_view();
    let sizes = SIZES
        .into_iter()
        .map(|p| {
            view! {
                <button type="button" class="seg" role="radio" class:on=move || size.get() == p aria-checked=move || (size.get() == p).to_string()
                    on:click=move |_| { save_size(p); size.set(p); }>{format!("{p}%")}</button>
            }
        })
        .collect_view();
    view! {
        <section class="card set">
            <h3>{tr!("Aparência")}</h3>
            {language_row()}
            {row(tr!("Tema"), tr!("O fundo da janela. Preto (OLED) apaga o fundo de vez, bom para telas OLED e quartos escuros."),
                view! { <div class="segs" role="radiogroup" aria-label=tr!("Tema")>{themes}</div> })}
            {row(tr!("Cor de destaque"), tr!("A cor dos botões principais, das barras de progresso e da página aberta."),
                view! { <div class="swatches" role="radiogroup" aria-label=tr!("Cor de destaque")>{accents}</div> })}
            {row(tr!("Tamanho"), tr!("Textos, capas e botões da janela. O Big Picture Mode segue o tamanho da tela."),
                view! { <div class="segs" role="radiogroup" aria-label=tr!("Tamanho")>{sizes}</div> })}
            {move || row(tr!("Menos animações"), tr!("Sem transições nem movimento, para PCs mais simples ou para quem prefere assim."),
                switch(calm.get(), tr!("Menos animações"), busy, move || { let on = !calm.get_untracked(); save_calm(on); calm.set(on); }))}
        </section>
    }
}

/// The controllers connected now, a live check of their buttons, and what each button does.
fn controllers() -> impl IntoView {
    use crate::tv::input::PadKind;
    let pads = RwSignal::new(crate::tv::input::connected_pads());
    let poll = set_interval_with_handle(move || pads.set(crate::tv::input::connected_pads()), std::time::Duration::from_millis(150)).ok();
    on_cleanup(move || {
        if let Some(p) = poll {
            p.clear();
        }
    });
    let list = move || {
        let now = pads.get();
        if now.is_empty() {
            return view! {
                <div class="pad-empty">
                    <Svg icon=Icon::Gamepad size=30 />
                    <div>
                        <b>{tr!("Nenhum controle por aqui")}</b>
                        <p class="muted small">{tr!("Conecte por cabo ou Bluetooth e aperte qualquer botão: o Windows só mostra o controle depois do primeiro botão. Xbox, PlayStation e Switch Pro funcionam.")}</p>
                    </div>
                </div>
            }
            .into_any();
        }
        now.into_iter()
            .map(|(name, kind, held)| {
                let brand = match kind {
                    PadKind::Xbox => "Xbox",
                    PadKind::PlayStation => "PlayStation",
                    PadKind::Nintendo => "Nintendo",
                };
                view! {
                    <div class="pad-row">
                        <span class="pad-ico"><Svg icon=Icon::Gamepad size=22 /></span>
                        <div class="pad-t"><b>{name}</b><small class="muted">{tr!("Botões no estilo {}", brand)}</small></div>
                        <span class="pad-held" class:on=held.is_some()>{held.map(|h| tr!("Apertado: {}", h)).unwrap_or_else(|| tr!("Aperte um botão para testar").to_string())}</span>
                        <span class="pill ok">{tr!("Conectado")}</span>
                    </div>
                }
            })
            .collect_view()
            .into_any()
    };
    let guide = [
        ("A", tr!("Escolher, jogar")),
        ("B", tr!("Voltar")),
        ("X", tr!("Opções do jogo")),
        ("Y", tr!("Página do jogo")),
        ("LB / RB", tr!("Trocar de seção")),
        ("Menu", tr!("Configurações; segurado, sai do Big Picture")),
        ("View + Menu", tr!("Menu sobre o jogo, durante a partida")),
        (tr!("Segurar View"), tr!("Também abre o menu sobre o jogo")),
        ("View + RB", tr!("Foto do jogo, durante a partida")),
    ]
    .into_iter()
    .map(|(key, what)| view! { <div class="pad-key"><kbd>{key}</kbd><span>{what}</span></div> })
    .collect_view();
    view! {
        <section class="card set">
            <div class="card-h"><h3>{tr!("Controles conectados")}</h3></div>
            <div class="pad-list">{list}</div>
        </section>
        <section class="card set">
            <div class="card-h"><h3>{tr!("No Big Picture Mode")}</h3></div>
            <p class="muted small">{tr!("O analógico esquerdo só começa a mover depois de ser visto parado, para um controle com drift não andar sozinho.")}</p>
            <div class="pad-guide">{guide}</div>
        </section>
    }
}

/// Which notices to give, and whether Windows shows them.
fn notices(s: SettingsView, busy: RwSignal<bool>, apply: impl Fn(Task, &'static str) + Copy + 'static) -> impl IntoView {
    let now = s.notices.clone();
    let change = move |edit: fn(&mut cerberus_shared::Notices)| {
        let mut next = now.clone();
        edit(&mut next);
        move || apply(Box::pin(api::set_notices(next.clone())), "")
    };
    let n = s.notices.clone();
    view! {
        <section class="card set">
            <div class="card-h"><h3>{tr!("Notificações")}</h3></div>
            {row(tr!("Jogo pronto para jogar"), tr!("Quando um recomp termina de instalar ou atualizar, ou o instalador de um jogo termina."),
                switch(n.installed, tr!("Jogo pronto para jogar"), busy, change(|x| x.installed = !x.installed)))}
            {row(tr!("Jogo fechou com erro"), tr!("Com o motivo e a dica do que fazer, quando dá para saber."),
                switch(n.crashed, tr!("Jogo fechou com erro"), busy, change(|x| x.crashed = !x.crashed)))}
            {row(tr!("Foto salva"), tr!("Confirma cada foto tirada com F12."),
                switch(n.screenshots, tr!("Foto salva"), busy, change(|x| x.screenshots = !x.screenshots)))}
        </section>
        <section class="card set">
            <div class="card-h"><h3>{tr!("Onde aparecem")}</h3></div>
            {row(tr!("Avisos do Windows"), tr!("Com o Cerberus fora da frente (jogando, ou na bandeja), o aviso aparece no canto do Windows. Desligado, aparece só dentro do Cerberus."),
                switch(n.windows, tr!("Avisos do Windows"), busy, change(|x| x.windows = !x.windows)))}
            {row(tr!("Conquistas"), tr!("O aviso de conquista sobre o jogo tem seus próprios ajustes."),
                view! { <span class="muted small">{tr!("Em Durante o jogo")}</span> })}
        </section>
    }
}

/// What Cerberus does while a game is open.
fn in_game(s: SettingsView, busy: RwSignal<bool>, apply: impl Fn(Task, &'static str) + Copy + 'static) -> impl IntoView {
    let library = expect_context::<Library>();
    let now = s.in_game.clone();
    // One switch changes one choice; the rest go back as they are.
    let change = move |edit: fn(&mut cerberus_shared::InGame), done: &'static str| {
        let mut next = now.clone();
        edit(&mut next);
        move || apply(Box::pin(api::set_in_game(next.clone())), done)
    };
    let c = s.in_game.clone();
    let corners = [("tl", tr!("Em cima, à esquerda")), ("tr", tr!("Em cima, à direita")), ("bl", tr!("Embaixo, à esquerda")), ("br", tr!("Embaixo, à direita"))]
        .into_iter()
        .map(|(code, label)| {
            let on = c.corner == code;
            let mut next = c.clone();
            next.corner = code.to_string();
            view! {
                <button type="button" class="seg" class:on=on role="radio" aria-checked=on.to_string() disabled=move || busy.get() || !next.unlock_notice
                    on:click={ let next = next.clone(); move |_| apply(Box::pin(api::set_in_game(next.clone())), tr!("Canto do aviso trocado.")) }>{label}</button>
            }
        })
        .collect_view();
    view! {
        <section class="card set">
            <div class="card-h"><h3>{tr!("Durante o jogo")}</h3></div>
            {row(tr!("Menu sobre o jogo"), tr!("Ctrl+F12, ou View + Menu no controle, abre o menu por cima do jogo: tempo jogado, fotos e fechar o jogo."),
                switch(c.menu, tr!("Menu sobre o jogo"), busy, change(|g| g.menu = !g.menu, "")))}
            {row(tr!("Fotos com F12"), tr!("F12 salva uma foto do jogo em Fotos, na página dele. O F12 da Steam continua funcionando."),
                switch(c.screenshots, tr!("Fotos com F12"), busy, change(|g| g.screenshots = !g.screenshots, "")))}
            {row(tr!("Sair da frente enquanto joga"), tr!("O Cerberus minimiza quando um jogo abre e volta quando ele fecha."),
                switch(c.minimize, tr!("Sair da frente enquanto joga"), busy, change(|g| g.minimize = !g.minimize, "")))}
        </section>
        <section class="card set">
            <div class="card-h">
                <h3>{tr!("Aviso de conquista")}</h3>
                <button type="button" class="btn sm" disabled=!c.unlock_notice
                    on:click=move |_| spawn_local(async move { library.report(api::unlock_test().await); })>{tr!("Testar")}</button>
            </div>
            {row(tr!("Mostrar sobre o jogo"), tr!("Um aviso pequeno, sem tirar o foco do jogo, quando uma conquista é desbloqueada. Diz quando ela é rara."),
                switch(c.unlock_notice, tr!("Mostrar sobre o jogo"), busy, change(|g| g.unlock_notice = !g.unlock_notice, "")))}
            {row(tr!("Canto da tela"), (), view! { <div class="segs corner-segs" role="radiogroup" aria-label=tr!("Canto da tela")>{corners}</div> })}
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
            {download_speed(s.download_limit, s.slow_while_playing, busy, apply)}
        </section>
    }
}

/// Download speed: a limit, and going slower while a game is open.
fn download_speed(limit: u32, slow: bool, busy: RwSignal<bool>, apply: impl Fn(Task, &'static str) + Copy + 'static) -> impl IntoView {
    let options = [0u32, 1, 2, 5, 10, 20, 50]
        .into_iter()
        .map(|mb| {
            let label = if mb == 0 { tr!("Sem limite").to_string() } else { format!("{mb} MB/s") };
            view! { <option value=mb.to_string() selected=mb == limit>{label}</option> }
        })
        .collect_view();
    view! {
        {row(tr!("Limite de velocidade"), tr!("Para não ocupar a conexão inteira de quem mais usa a internet aí."), view! {
            <select class="select" aria-label=tr!("Limite de velocidade") disabled=move || busy.get()
                on:change=move |e| {
                    let mb = event_target_value(&e).parse().unwrap_or(0);
                    apply(Box::pin(api::set_download_limit(mb, slow)), tr!("Limite de velocidade salvo."));
                }>{options}</select>
        })}
        {row(tr!("Mais devagar enquanto você joga"), tr!("Com um jogo aberto, os downloads ficam em até 3 MB/s, para não dar lag. Voltam ao normal quando você fecha o jogo."), view! {
            <button type="button" class="switch" class:on=slow role="switch" aria-checked=slow.to_string() aria-label=tr!("Mais devagar enquanto você joga")
                disabled=move || busy.get()
                on:click=move |_| apply(Box::pin(api::set_download_limit(limit, !slow)), if slow { tr!("Downloads na velocidade normal mesmo jogando.") } else { tr!("Downloads mais devagar enquanto você joga.") })>
                <i></i>
            </button>
        })}
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
            {row(format!("{} recomps", s.catalog_count), if s.site_catalog { tr!("Lista de {}, atualizada sozinha a cada 6 horas, mais o catálogo próprio do Cerberus. Cada recomp é baixado da página do próprio desenvolvedor (GitHub, GitLab ou o servidor do projeto).", s.catalog_source) } else { tr!("Só o catálogo próprio do Cerberus e as suas fontes de projetos. Cada recomp é baixado da página do próprio desenvolvedor.").to_string() },
                view! { <button type="button" class="btn sm" disabled=move || busy.get() on:click=move |_| apply(Box::pin(api::refresh_catalog()), tr!("Catálogo atualizado."))>{tr!("Atualizar agora")}</button> })}
            {row(tr!("Catálogo do recomplauncher.com"), tr!("A lista grande, com recomps de mais de 25 consoles. Desligado, o Descobrir mostra só o catálogo do Cerberus e as suas fontes de projetos."),
                switch(s.site_catalog, tr!("Catálogo do recomplauncher.com"), busy, move || {
                    let on = !s.site_catalog;
                    apply(Box::pin(api::set_site_catalog(on)), if on { tr!("Catálogo do recomplauncher.com ligado.") } else { tr!("Usando só o catálogo do Cerberus.") })
                }))}
        </section>
    }
}

fn emulation(s: SettingsView) -> impl IntoView {
    view! { <EmulationPage roms=s.roms.len() ra_user=s.ra_user.clone() ra_retroarch=s.ra_retroarch /> }
}

/// A console's name, as players know it.
fn console_name(id: &str) -> &'static str {
    match id {
        "ps1" => "PlayStation",
        "ps2" => "PlayStation 2",
        "ps3" => "PlayStation 3",
        "psp" => "PSP",
        "gc" => "GameCube",
        "wii" => "Wii",
        "wiiu" => "Wii U",
        _ => "",
    }
}

/// Consoles RetroArch plays with the cores Cerberus installs.
const RETROARCH_PLAYS: [&str; 5] = ["ps1", "ps2", "psp", "gc", "wii"];


/// The consoles that come on discs, each opened by RetroArch or a standalone emulator.
const DISC_CONSOLES: [&str; 7] = ["ps1", "ps2", "ps3", "psp", "gc", "wii", "wiiu"];

/// How a console stands now.
#[derive(Clone, Copy, PartialEq)]
enum Ready {
    Yes,
    NeedsBios,
    NeedsCore,
    NoEmulator,
}

/// Emulação: the consoles at a glance (who opens each and whether it's ready),
/// then every emulator as a card (RetroArch with its cores and BIOS, and the
/// standalone ones), the emulators' game folders, and achievements.
#[component]
fn EmulationPage(roms: usize, ra_user: Option<String>, ra_retroarch: bool) -> impl IntoView {
    let library = expect_context::<Library>();
    let emu = RwSignal::new(None::<cerberus_shared::EmulationView>);
    let st = RwSignal::new(None::<cerberus_shared::StandaloneStatus>);
    let busy = RwSignal::new(None::<String>);
    let progress = RwSignal::new((0u64, 0u64));
    let refresh = move || {
        spawn_local(async move {
            if let Some(v) = library.report(api::emulation_status().await) {
                emu.set(Some(v));
            }
            if let Some(v) = library.report(api::standalone_status().await) {
                st.set(Some(v));
            }
        });
    };
    refresh();
    api::on_emu_progress(move |p| progress.set(p));
    // Runs one action, says what it did, and reads both states again.
    let act = move |label: &str, task: std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>>>>| {
        busy.set(Some(label.to_string()));
        progress.set((0, 0));
        spawn_local(async move {
            let said = library.report(task.await);
            busy.set(None);
            if let Some(text) = said.filter(|t| !t.is_empty()) {
                library.toast(text);
            }
            refresh();
        });
    };
    let ra_linked = ra_user.is_some();
    let disabled = move || busy.get().is_some();

    move || {
        let (Some(v), Some(s)) = (emu.get(), st.get()) else {
            return view! { <section class="card set"><h3>{tr!("Emulação")}</h3><p class="muted">{tr!("Verificando...")}</p></section> }.into_any();
        };
        let working = busy.get();
        let has_ra = v.retroarch.is_some();
        let core_ok = |console: &str| v.cores.iter().any(|c| c.console == console && c.installed);
        let emulator = |id: &str| s.emulators.iter().find(|e| e.id == id).cloned();

        // ---------- the consoles ----------
        let state_of = |console: &str, who: &str| -> Ready {
            match who {
                "retroarch" if !has_ra => Ready::NoEmulator,
                "retroarch" if !core_ok(console) => Ready::NeedsCore,
                "retroarch" if console == "ps2" && !v.ps2_bios => Ready::NeedsBios,
                "retroarch" => Ready::Yes,
                "" => Ready::NoEmulator,
                id => match emulator(id) {
                    Some(e) if e.exe.is_none() => Ready::NoEmulator,
                    Some(e) if e.bios == Some(false) => Ready::NeedsBios,
                    Some(_) => Ready::Yes,
                    None => Ready::NoEmulator,
                },
            }
        };
        let installed: Vec<cerberus_shared::StandaloneView> = s.emulators.iter().filter(|e| e.exe.is_some()).cloned().collect();
        // The player's own classics, by console: their consoles come first.
        let my_roms = library.roms.get().unwrap_or_default();
        let players = library.players.get().unwrap_or_default();
        let mine = |console: &str| my_roms.iter().filter(|r| crate::classics::console_of(r) == console).count();
        let playable = my_roms.iter().filter(|r| crate::classics::ready_player(&players, crate::classics::console_of(r)).is_some()).count();
        let mut order: Vec<&str> = DISC_CONSOLES.to_vec();
        order.sort_by_key(|c| std::cmp::Reverse(mine(c)));
        let tiles = order
            .into_iter()
            .map(|console| {
                let console = &console;
                let owned = mine(console);
                let who = s.play_with.iter().find(|(c, _)| c == console).map(|(_, w)| w.clone()).unwrap_or_default();
                let ready = state_of(console, &who);
                let (class, label) = match ready {
                    Ready::Yes => ("ok", tr!("Pronto")),
                    Ready::NeedsBios => ("warn", tr!("Falta a BIOS")),
                    Ready::NeedsCore => ("warn", tr!("Falta o núcleo")),
                    Ready::NoEmulator => ("off", tr!("Sem emulador")),
                };
                let mut options: Vec<(String, String)> = Vec::new();
                if RETROARCH_PLAYS.contains(console) {
                    options.push(("retroarch".into(), "RetroArch".into()));
                }
                options.extend(installed.iter().filter(|e| e.consoles.iter().any(|c| c == console)).map(|e| (e.id.clone(), e.name.clone())));
                let picker = if options.len() > 1 {
                    let c = console.to_string();
                    let opts = options.into_iter().map(|(id, name)| { let on = id == who; view! { <option value=id selected=on>{name}</option> } }).collect_view();
                    view! {
                        <select class="select emu-who" aria-label=tr!("Jogar {} com", console_name(console)) disabled=disabled on:change=move |ev| {
                            let (console, who) = (c.clone(), event_target_value(&ev));
                            act("who", Box::pin(async move { api::set_play_with(&console, &who).await.map(|_| String::new()) }));
                        }>{opts}</select>
                    }
                    .into_any()
                } else {
                    let name = options.first().map(|(_, n)| n.clone()).unwrap_or_else(|| tr!("Instale um emulador abaixo").to_string());
                    view! { <span class="emu-who-one">{name}</span> }.into_any()
                };
                let games = match owned {
                    0 => tr!("Nenhum jogo seu").to_string(),
                    1 => tr!("1 jogo seu").to_string(),
                    n => tr!("{} jogos seus", n),
                };
                view! {
                    <div class=format!("emu-tile {class}") class:idle=owned == 0>
                        <div class="emu-tile-h"><b>{console_name(console)}</b><span class="emu-dot">{label}</span></div>
                        <small class="emu-mine">{games}</small>
                        {picker}
                    </div>
                }
            })
            .collect_view();
        let retro_consoles: Vec<&cerberus_shared::CoreView> = v.cores.iter().filter(|c| !DISC_CONSOLES.contains(&c.console.as_str())).collect();
        let retro_missing = retro_consoles.iter().filter(|c| !c.installed).count();
        let retro_ready = has_ra && retro_missing == 0;
        // Judged by the player's own cartridges: a core for a console they have no game of isn't missing.
        let carts: Vec<&str> = my_roms.iter().map(crate::classics::console_of)
            .filter(|c| !DISC_CONSOLES.contains(c) && v.cores.iter().any(|core| core.console == *c)).collect();
        // Games of consoles nothing here plays yet (3DS, Xbox 360...).
        let mut unplayable: Vec<&'static str> = my_roms.iter().map(crate::classics::console_of)
            .filter(|c| !DISC_CONSOLES.contains(c) && !v.cores.iter().any(|core| core.console == *c))
            .map(crate::classics::console_name).collect();
        unplayable.sort_unstable();
        unplayable.dedup();
        let mut carts_missing: Vec<&str> = carts.iter().copied().filter(|c| !core_ok(c)).collect();
        carts_missing.sort_unstable();
        carts_missing.dedup();
        let carts_ready = has_ra && if carts.is_empty() { retro_ready } else { carts_missing.is_empty() };
        let cart_games = match carts.len() {
            0 => tr!("Nenhum jogo seu").to_string(),
            1 => tr!("1 jogo seu").to_string(),
            n => tr!("{} jogos seus", n),
        };
        let retro_tile = view! {
            <div class=format!("emu-tile wide {}", if carts_ready { "ok" } else if has_ra { "warn" } else { "off" })>
                <div class="emu-tile-h">
                    <b>{tr!("Cartuchos")}</b>
                    <span class="emu-dot">{if carts_ready { tr!("Pronto").to_string() } else if !has_ra { tr!("Sem emulador").to_string() } else if carts.is_empty() { tr!("Faltam {} núcleos", retro_missing) } else { tr!("Faltam {} núcleos", carts_missing.len()) }}</span>
                </div>
                <small class="emu-mine">{cart_games}</small>
                <span class="emu-who-one">{tr!("NES, Super Nintendo, Nintendo 64, Game Boy, GBA e Mega Drive pelo RetroArch")}</span>
            </div>
            {(!unplayable.is_empty()).then(|| view! {
                <div class="emu-tile wide off">
                    <div class="emu-tile-h"><b>{unplayable.join(", ")}</b><span class="emu-dot">{tr!("Sem emulação")}</span></div>
                    <span class="emu-who-one">{tr!("Você tem jogos desses consoles, mas o Cerberus ainda não tem emulador para eles.")}</span>
                </div>
            })}
        };

        // ---------- RetroArch's card ----------
        let total_cores = v.cores.len();
        let have_cores = v.cores.iter().filter(|c| c.installed).count();
        let ra_status = match (&v.retroarch, working.as_deref()) {
            (_, Some("retroarch")) => {
                let (done, total) = progress.get();
                if total > 0 && done >= total { tr!("Extraindo… leva um minuto.").to_string() } else if total > 0 { tr!("Baixando {}%…", done * 100 / total) } else { tr!("Preparando…").to_string() }
            }
            (_, Some("cores")) => {
                let (done, total) = progress.get();
                tr!("Instalando núcleos · {} de {}", done, total)
            }
            (Some(_), _) if v.managed => tr!("Instalado pelo Cerberus").to_string(),
            (Some(path), _) => tr!("Encontrado em {}", path),
            (None, _) => tr!("Não instalado").to_string(),
        };
        let chips = v
            .cores
            .iter()
            .map(|c| {
                let core = c.core.clone();
                let mine = working.as_deref() == Some(core.as_str());
                if c.installed {
                    view! { <span class="emu-chip ok" title=tr!("Núcleo {}", c.core)><Svg icon=Icon::Check size=12 />{c.consoles.clone()}</span> }.into_any()
                } else {
                    view! {
                        <button type="button" class="emu-chip" title=tr!("Núcleo {}", c.core) disabled=move || disabled() || !has_ra on:click=move |_| {
                            let name = core.clone();
                            act(&name.clone(), Box::pin(async move { api::install_core(name).await.map(|_| tr!("Núcleo instalado.").to_string()) }));
                        }><Svg icon=Icon::Download size=12 />{if mine { tr!("Baixando...").to_string() } else { c.consoles.clone() }}</button>
                    }
                    .into_any()
                }
            })
            .collect_view();
        let all_cores = (has_ra && have_cores < total_cores).then(|| view! {
            <button type="button" class="btn sm" disabled=disabled on:click=move |_| act("cores", Box::pin(async move {
                api::install_all_cores().await.map(|(_, n)| tr!("{} núcleos instalados.", n))
            }))><Svg icon=Icon::Download size=16 />{tr!("Instalar todos")}</button>
        });
        let ra_bios = (has_ra).then(|| {
            let ps2 = v.ps2_bios;
            let ps1 = v.ps1_bios;
            view! {
                <div class="emu-line">
                    <span class=if ps2 { "emu-flag ok" } else { "emu-flag warn" }>{if ps2 { tr!("BIOS do PS2 pronta") } else { tr!("Falta a BIOS do PS2") }}</span>
                    <span class=if ps1 { "emu-flag ok" } else { "emu-flag" }>{if ps1 { tr!("BIOS do PS1 pronta") } else { tr!("BIOS do PS1: opcional") }}</span>
                    <button type="button" class="btn sm" disabled=disabled on:click=move |_| act("ra-bios", Box::pin(async move { api::import_bios().await.map(|(_, said)| said) }))>
                        <Svg icon=Icon::Folder size=16 />{tr!("Adicionar BIOS")}</button>
                </div>
            }
        });
        let ra_card = view! {
            <article class="emu-card ra" class:on=has_ra>
                <header>
                    <div><b>"RetroArch"</b><small>{tr!("Vários consoles, um núcleo para cada")}</small></div>
                    <span class=if has_ra { "pill ok" } else { "pill soon" }>{if has_ra { tr!("Instalado") } else { tr!("Não instalado") }}</span>
                </header>
                <p class="emu-status" title=v.retroarch.clone().unwrap_or_default()>{ra_status}</p>
                <div class="emu-cores-h"><span>{tr!("Núcleos · {} de {}", have_cores, total_cores)}</span>{all_cores}</div>
                <div class="emu-chips">{chips}</div>
                {ra_bios}
                <footer>
                    <button type="button" class="btn sm" disabled=disabled on:click=move |_| act("pick", Box::pin(async move { api::pick_retroarch().await.map(|_| String::new()) }))>{tr!("Usar o meu")}</button>
                    <button type="button" class="btn sm" class:main=!has_ra disabled=disabled on:click=move |_| act("retroarch", Box::pin(async move { api::install_retroarch().await.map(|_| tr!("RetroArch instalado.").to_string()) }))>
                        <Svg icon=Icon::Download size=16 />{if has_ra { tr!("Reinstalar") } else { tr!("Instalar") }}</button>
                </footer>
            </article>
        };

        // ---------- standalone emulators ----------
        let cards = s
            .emulators
            .iter()
            .cloned()
            .map(|e| {
                let consoles = e.consoles.iter().map(|c| console_name(c)).collect::<Vec<_>>().join(", ");
                let mine = working.as_deref() == Some(e.id.as_str());
                let installed = e.exe.is_some();
                let status = if mine {
                    let (done, total) = progress.get();
                    if total > 0 { tr!("Baixando {}%…", done * 100 / total) } else { tr!("Preparando…").to_string() }
                } else {
                    match (&e.exe, e.managed) {
                        (Some(_), true) => tr!("Instalado pelo Cerberus").to_string(),
                        (Some(path), false) => tr!("Encontrado em {}", path),
                        (None, _) => tr!("Não instalado").to_string(),
                    }
                };
                let bios = (installed && e.bios.is_some()).then(|| {
                    let ok = e.bios == Some(true);
                    let what = if e.firmware {
                        if ok { tr!("Firmware do PS3 instalado") } else { tr!("Falta o firmware do PS3") }
                    } else if ok {
                        tr!("BIOS pronta")
                    } else {
                        tr!("Falta a BIOS")
                    };
                    let id = e.id.clone();
                    let button = if e.firmware { tr!("Instalar firmware") } else { tr!("Adicionar BIOS") };
                    view! {
                        <div class="emu-line">
                            <span class=if ok { "emu-flag ok" } else { "emu-flag warn" }>{what}</span>
                            <button type="button" class="btn sm" disabled=disabled on:click=move |_| {
                                let id = id.clone();
                                act("bios", Box::pin(async move { api::standalone_bios(&id).await.map(|(_, said)| said) }))
                            }><Svg icon=Icon::Folder size=16 />{button}</button>
                        </div>
                    }
                });
                let (i1, i2, i3, i4) = (e.id.clone(), e.id.clone(), e.id.clone(), e.id.clone());
                let name = e.name.clone();
                let actions = if !installed {
                    view! {
                        <button type="button" class="btn sm" disabled=disabled on:click=move |_| { let id = i1.clone(); act(&id.clone(), Box::pin(async move { api::pick_standalone(&id).await.map(|_| String::new()) })) }>{tr!("Já tenho")}</button>
                        <button type="button" class="btn sm main" disabled=disabled on:click=move |_| {
                            let id = i2.clone();
                            let done = tr!("{} instalado. Seus jogos dele já podem abrir por aqui.", name);
                            act(&id.clone(), Box::pin(async move { api::install_standalone(&id).await.map(|_| done) }))
                        }><Svg icon=Icon::Download size=16 />{tr!("Instalar")}</button>
                    }
                    .into_any()
                } else {
                    let managed = e.managed;
                    view! {
                        <button type="button" class="btn sm" on:click=move |_| { let id = i3.clone(); spawn_local(async move { library.report(api::open_standalone(&id).await); }) }>{tr!("Abrir")}</button>
                        <button type="button" class="btn sm" disabled=disabled on:click=move |_| {
                            let id = i4.clone();
                            if managed {
                                act(&id.clone(), Box::pin(async move { api::remove_standalone(&id).await.map(|_| String::new()) }))
                            } else {
                                act(&id.clone(), Box::pin(async move { api::pick_standalone(&id).await.map(|_| String::new()) }))
                            }
                        }>{if managed { tr!("Remover") } else { tr!("Trocar") }}</button>
                    }
                    .into_any()
                };
                view! {
                    <article class="emu-card" class:on=installed>
                        <header>
                            <div><b>{e.name.clone()}</b><small>{consoles}</small></div>
                            <span class=if installed { "pill ok" } else { "pill soon" }>{if installed { tr!("Instalado") } else { tr!("Não instalado") }}</span>
                        </header>
                        <p class="emu-status" title=e.exe.clone().unwrap_or_default()>{status}</p>
                        {bios}
                        <footer>{actions}</footer>
                    </article>
                }
            })
            .collect_view();

        // ---------- folders and achievements ----------
        let use_folders = s.use_folders;
        let folder_list = (use_folders && !s.folders.is_empty()).then(|| {
            let items = s.folders.iter().map(|f| view! { <li class="path">{f.clone()}</li> }).collect_view();
            view! { <ul class="emu-folders">{items}</ul> }
        });
        let retro = match (&v.ra_user, ra_linked, ra_retroarch) {
            (Some(user), _, _) => view! { <span class="pill ok"><Svg icon=Icon::Trophy size=14 />{tr!("Conectado como {}", user)}</span> }.into_any(),
            (None, true, true) => view! { <span class="pill ok"><Svg icon=Icon::Trophy size=14 />{tr!("Conecta sozinho ao abrir um jogo")}</span> }.into_any(),
            (None, true, false) => view! { <small>{tr!("Informe sua senha em Contas › RetroAchievements para ligar as conquistas no RetroArch.")}</small> }.into_any(),
            (None, false, _) => view! { <small>{tr!("Conecte sua conta em Contas › RetroAchievements primeiro.")}</small> }.into_any(),
        };

        view! {
            <section class="card set emu-top">
                <div>
                    <h3>{tr!("Emulação")}</h3>
                    <p class="muted small">{tr!("{} clássicos nas suas pastas · {} já abrem aqui", roms, playable)}</p>
                </div>
                <button type="button" class="btn sm" on:click=move |_| library.go(Route::Library)>{tr!("Ver na Biblioteca")}</button>
            </section>
            <section class="card set">
                <h3>{tr!("Seus consoles")}</h3>
                <p class="muted small">{tr!("Quem abre os jogos de cada console, e se já dá para jogar.")}</p>
                <div class="emu-tiles">{tiles}{retro_tile}</div>
            </section>
            <section class="card set">
                <h3>{tr!("Emuladores")}</h3>
                <p class="muted small">{tr!("Cada um vem da página oficial do projeto, sem BIOS nem firmware: esses arquivos são do seu console. Se você já tem algum, use Já tenho.")}</p>
                <div class="emu-cards">{ra_card}{cards}</div>
            </section>
            <section class="card set">
                <h3>{tr!("Jogos dos emuladores")}</h3>
                {row(tr!("Usar as pastas dos emuladores"), tr!("As pastas de jogos que os seus emuladores já conhecem (as listas do RetroArch também) entram nos clássicos, além das suas pastas de ROMs. O Cerberus só lê essas pastas."),
                    switch(use_folders, tr!("Usar as pastas dos emuladores"), RwSignal::new(false), move || act("folders", Box::pin(async move { api::set_emulator_folders(!use_folders).await.map(|_| String::new()) }))))}
                {folder_list}
                {(use_folders && s.folders.is_empty()).then(|| view! { <p class="muted small">{tr!("Nenhum emulador deste PC lista pastas de jogos ainda.")}</p> })}
            </section>
            <section class="card set">
                <h3>{tr!("Conquistas no RetroArch")}</h3>
                {row("RetroAchievements", tr!("A conta é a de Contas › RetroAchievements. O Cerberus grava no RetroArch só o código de acesso, nunca a senha, e faz uma cópia de segurança das configurações dele antes."), retro)}
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
    let restoring = RwSignal::new(false);
    let library = expect_context::<Library>();
    let free = s.download_free_bytes.map(|free| tr!(" Espaço livre para downloads: {} GiB.", format::decimal(format!("{:.1}", free as f64 / 1073741824.0)))).unwrap_or_default();
    view! {
        <section class="card set">
            <h3>{tr!("Armazenamento")}</h3>
            {row(tr!("Dados do Cerberus"), view! { <span class="path">{s.data_dir.clone()}</span><br/>{tr!("Biblioteca, backups de saves e caches.{}", free)} },
                view! { <button type="button" class="btn sm" on:click=move |_| spawn_local(async move { library.report(api::open_data_dir().await); })>{tr!("Abrir pasta")}</button> })}
            {row(tr!("Copiar meus dados"), tr!("Um arquivo .zip com a biblioteca, favoritos, coleções, notas, tempo de jogo, capas que você escolheu e configurações. Suas chaves e senhas ficam de fora. Os saves têm os próprios backups."),
                view! { <button type="button" class="btn sm" on:click=move |_| spawn_local(async move {
                    if let Some(Some(n)) = library.report(api::export_data().await) {
                        library.toast(tr!("Cópia salva com {} arquivos.", n));
                    }
                })>{tr!("Salvar cópia")}</button> })}
            {move || row(tr!("Restaurar de uma cópia"), tr!("Traz os dados de uma cópia, por exemplo de outro PC. Os dados de agora são guardados antes, e as chaves, contas e pastas deste PC ficam. O Cerberus abre de novo no fim."),
                if restoring.get() {
                    view! {
                        <div class="row nowrap">
                            <button type="button" class="btn sm" on:click=move |_| restoring.set(false)>{tr!("Cancelar")}</button>
                            <button type="button" class="btn sm main" on:click=move |_| spawn_local(async move {
                                restoring.set(false);
                                if let Some(Some(n)) = library.report(api::import_data().await) {
                                    library.toast(tr!("{} arquivos restaurados. O Cerberus vai abrir de novo.", n));
                                }
                            })>{tr!("Escolher a cópia")}</button>
                        </div>
                    }.into_any()
                } else {
                    view! { <button type="button" class="btn sm" on:click=move |_| restoring.set(true)>{tr!("Restaurar")}</button> }.into_any()
                })}
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
            {update_row()}
            {row(tr!("Pedir um recomp"), tr!("Sugira um projeto para o catálogo, pelo GitHub do Cerberus."),
                view! { <button type="button" class="btn sm" on:click=move |_| spawn_local(async move { library.report(api::open_recomp_request().await); })><Svg icon=Icon::External size=16 />{tr!("Abrir")}</button> })}
        </section>
    }
}

/// Settings › About: look for a newer Cerberus now.
fn update_row() -> impl IntoView {
    let library = expect_context::<Library>();
    let updates = expect_context::<crate::updates::Updates>();
    let checking = RwSignal::new(false);
    let check = move |_| {
        checking.set(true);
        spawn_local(async move {
            let got = library.report(api::update_check().await);
            checking.set(false);
            match got {
                Some(Some(u)) => {
                    updates.found.set(Some(u));
                    updates.open.set(true);
                }
                Some(None) => library.toast(tr!("Você já está na versão mais nova.")),
                None => {}
            }
        });
    };
    row(
        tr!("Atualizações"),
        tr!("O Cerberus procura versões novas no GitHub ao abrir e a cada 6 horas, e avisa no menu lateral. Você escolhe quando instalar."),
        view! {
            <button type="button" class="btn sm" disabled=move || checking.get() on:click=check>
                <Svg icon=Icon::Refresh size=16 />{move || if checking.get() { tr!("Procurando…") } else { tr!("Procurar agora") }}
            </button>
        },
    )
}

/// Linking a RetroAchievements account and showing its progress.
/// Steam's Web API, optional: with the player's own key, the achievements hub
/// reads every owned game and the achievements unlocked on other PCs.
/// Your other games inside Steam, for the Steam Deck and Steam's Big Picture.
#[component]
fn SteamShortcutsCard() -> impl IntoView {
    let library = expect_context::<Library>();
    let busy = RwSignal::new(false);
    let add_all = move |_| {
        busy.set(true);
        spawn_local(async move {
            match library.report(api::add_all_to_steam().await) {
                Some(0) => library.toast(tr!("Seus jogos já estão todos na Steam.")),
                Some(1) => library.toast(tr!("1 jogo foi para a Steam. Ele aparece quando a Steam abrir.")),
                Some(n) => library.toast(tr!("{} jogos foram para a Steam. Eles aparecem quando a Steam abrir.", n)),
                None => {}
            }
            busy.set(false);
        });
    };
    view! {
        <section class="card set">
            <h3>{tr!("Seus jogos na Steam")}</h3>
            {row(tr!("Adicionar todos à Steam"), tr!("Recomps, jogos que você adicionou e os da Epic entram na Steam como jogos de fora, com capa, e abrem pelo Cerberus. Os que já estão lá ficam como estão. Feche a Steam antes."),
                view! { <button type="button" class="btn sm" disabled=move || busy.get() on:click=add_all>
                    {move || if busy.get() { tr!("Adicionando…") } else { tr!("Adicionar todos") }}</button> })}
        </section>
    }
}

/// The Epic Games Launcher: found or not, and its installed games in the library.
#[component]
fn EpicCard() -> impl IntoView {
    let library = expect_context::<Library>();
    let status = LocalResource::new(api::epic_status);
    let body = move || match status.get() {
        None => view! { <p class="muted small">{tr!("Procurando a Epic neste PC…")}</p> }.into_any(),
        Some(Err(e)) => view! { <p class="problem">{e}</p> }.into_any(),
        Some(Ok((found, games))) => {
            let (pill, text) = match (found, games) {
                (false, 0) => (
                    view! { <span class="pill">{tr!("Não encontrada")}</span> }.into_any(),
                    tr!("A Epic Games Launcher não está neste PC. Quando estiver, os jogos instalados por ela entram sozinhos na Biblioteca.").to_string(),
                ),
                (_, 0) => (
                    view! { <span class="pill ok">{tr!("Encontrada")}</span> }.into_any(),
                    tr!("Nenhum jogo da Epic instalado. Os que você instalar por ela entram sozinhos na Biblioteca.").to_string(),
                ),
                (_, 1) => (view! { <span class="pill ok">{tr!("Encontrada")}</span> }.into_any(), tr!("1 jogo instalado pela Epic está na Biblioteca.").to_string()),
                (_, n) => (view! { <span class="pill ok">{tr!("Encontrada")}</span> }.into_any(), tr!("{} jogos instalados pela Epic estão na Biblioteca.", n)),
            };
            view! {
                <div class="set-row">
                    <div><b>{tr!("Epic Games Launcher")}</b><p class="muted small">{text}</p></div>
                    {pill}
                </div>
                <div class="row">
                    <button type="button" class="btn sm" disabled=games == 0 on:click=move |_| library.go(Route::Library)>{tr!("Ver na Biblioteca")}</button>
                    <button type="button" class="btn sm" disabled=!found on:click=move |_| spawn_local(async move { library.report(api::epic_open().await); })>
                        <Svg icon=Icon::External size=16 />{tr!("Abrir a Epic")}
                    </button>
                </div>
                <p class="muted small">{tr!("Sem conta e sem senha: o Cerberus lê o que a Epic já guarda no PC. Os jogos abrem pela Epic, que precisa estar instalada. Tempo de jogo é contado; conquistas da Epic ainda não.")}</p>
            }
            .into_any()
        }
    };
    view! {
        <section class="card set">
            <div class="card-h"><h3>"Epic Games"</h3></div>
            {body}
        </section>
    }
}

#[component]
fn SteamAccountCard() -> impl IntoView {
    let library = expect_context::<Library>();
    let account = LocalResource::new(api::steam_account);
    let local = LocalResource::new(api::steam_local_account);
    let key = RwSignal::new(String::new());
    let profile = RwSignal::new(String::new());
    let other = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let open = move |page: &'static str| spawn_local(async move { library.report(api::steam_open(page).await); });
    let connect = move |e: leptos::ev::SubmitEvent| {
        e.prevent_default();
        busy.set(true);
        spawn_local(async move {
            let typed = profile.get_untracked();
            let chosen = (other.get_untracked() && !typed.trim().is_empty()).then_some(typed);
            let got = library.report(api::steam_connect(key.get_untracked(), chosen).await);
            busy.set(false);
            if let Some(a) = got {
                key.set(String::new());
                library.toast(tr!("Steam conectada como {}.", a.name));
                account.refetch();
            }
        });
    };
    let disconnect = move |_| {
        spawn_local(async move {
            if library.report(api::steam_disconnect().await).is_some() {
                library.toast(tr!("Steam desconectada. As conquistas voltam a vir só do cache deste PC."));
                account.refetch();
            }
        });
    };
    move || {
        let state = account.get();
        let body = match state {
            None => view! { <p class="muted">{tr!("Lendo sua conta...")}</p> }.into_any(),
            Some(Ok(Some(a))) => {
                let warn = (!a.games_public).then(|| view! {
                    <div class="ach-note">
                        <Svg icon=Icon::Info size=18 />
                        <span>{tr!("Os detalhes de jogo do seu perfil estão privados, então a Steam não entrega seus jogos nem suas conquistas. Em Privacidade, deixe \"Detalhes de jogo\" como Público.")}</span>
                        <button type="button" class="btn sm" on:click=move |_| open("privacy")>{tr!("Abrir privacidade")}</button>
                    </div>
                });
                view! {
                    <div class="ra-head">
                        {a.avatar.clone().map(|src| view! { <img class="ra-avatar" src=src alt="" /> })}
                        <div class="ra-who">
                            <b>{a.name.clone()}</b>
                            <small>{if a.games_public { tr!("{} jogos na conta · detalhes de jogo públicos", a.games) } else { tr!("Detalhes de jogo privados").to_string() }}</small>
                        </div>
                        <div class="row">
                            <button type="button" class="btn sm" on:click=move |_| open("profile")><Svg icon=Icon::External size=16 />{tr!("Ver perfil")}</button>
                            <button type="button" class="btn sm" on:click=disconnect>{tr!("Desconectar")}</button>
                        </div>
                    </div>
                    {warn}
                    <p class="muted small">{tr!("Com a Web API, o hub de conquistas mostra todos os seus jogos da Steam, com as conquistas e o tempo de jogo de qualquer PC. A chave fica só neste PC.")}</p>
                }
                .into_any()
            }
            Some(result) => {
                let problem = result.err().map(|e| view! { <p class="problem">{e}</p> });
                let here = local.get().and_then(Result::ok).flatten();
                let here_text = here.clone().map(|(_, name)| tr!("Conta da Steam neste PC: {}", name));
                view! {
                    {problem}
                    <p class="muted small">{tr!("Opcional. Sem isso, as conquistas da Steam vêm do cache deste PC, só dos jogos que a Steam já sincronizou aqui. Com a sua chave da Web API (gratuita), vêm todos os seus jogos e as conquistas feitas em qualquer PC. O seu perfil precisa ter os detalhes de jogo públicos.")}</p>
                    {here_text.map(|t| view! { <p class="small"><b>{t}</b></p> })}
                    <form class="ra-form" on:submit=connect>
                        <input class="input" type="password" placeholder=tr!("Chave da Web API da Steam") autocomplete="off"
                            prop:value=move || key.get() on:input=move |e| key.set(event_target_value(&e)) />
                        <Show when=move || other.get() || here.is_none()>
                            <input class="input" placeholder=tr!("Seu perfil: link ou SteamID64")
                                prop:value=move || profile.get() on:input=move |e| profile.set(event_target_value(&e)) />
                        </Show>
                        <button type="submit" class="btn sm main" disabled=move || busy.get() || key.get().trim().is_empty()>
                            {move || if busy.get() { tr!("Conectando...") } else { tr!("Conectar") }}
                        </button>
                    </form>
                    <div class="row">
                        <button type="button" class="link" on:click=move |_| open("key")>{tr!("Pegar minha chave na Steam")}</button>
                        <button type="button" class="link" on:click=move |_| open("privacy")>{tr!("Privacidade do perfil")}</button>
                        <button type="button" class="link" on:click=move |_| other.update(|o| *o = !*o)>{tr!("Usar outra conta")}</button>
                    </div>
                }
                .into_any()
            }
        };
        view! {
            <section class="card set">
                <h3>"Steam"</h3>
                {body}
                <div class="row">
                    <button type="button" class="link" on:click=move |_| spawn_local(async move { library.report(api::unlock_test().await); })>
                        {tr!("Testar o aviso de conquista")}
                    </button>
                </div>
            </section>
        }
    }
}

#[component]
fn RetroAchievements(linked: Option<String>, retroarch: bool) -> impl IntoView {
    let library = expect_context::<Library>();
    let profile = RwSignal::new(None::<RaProfile>);
    let loading = RwSignal::new(linked.is_some());
    let user = RwSignal::new(String::new());
    let key = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let retroarch = RwSignal::new(retroarch);
    let link = move |e: leptos::ev::SubmitEvent| {
        e.prevent_default();
        let pass = password.get_untracked();
        password.set(String::new());
        spawn_local(async move {
            if library.report(api::link_retroarch_achievements(pass).await).is_some() {
                retroarch.set(true);
                library.toast(tr!("Conquistas ligadas no RetroArch."));
            }
        });
    };
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
            let pass = Some(password.get_untracked()).filter(|p| !p.is_empty());
            let with_pass = pass.is_some();
            let got = library.report(api::ra_connect(user.get_untracked(), key.get_untracked(), pass).await);
            busy.set(false);
            password.set(String::new());
            if let Some(p) = got {
                library.toast(tr!("Conectado ao RetroAchievements como {}.", p.user));
                key.set(String::new());
                if with_pass {
                    retroarch.set(true);
                }
                profile.set(Some(p));
            }
        });
    };
    let disconnect = move |_| {
        spawn_local(async move {
            if library.report(api::ra_disconnect().await).is_some() {
                profile.set(None);
                retroarch.set(false);
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
                                <small>{tr!("{} pontos hardcore · {} softcore · {} pontos reais", format::count(p.points), format::count(p.softcore_points), format::count(p.true_points))}</small>
                                {now}
                            </div>
                            <div class="row">
                                <button type="button" class="btn sm" on:click=move |_| open(false)><Svg icon=Icon::External size=16 />{tr!("Ver perfil")}</button>
                                <button type="button" class="btn sm" on:click=disconnect>{tr!("Desconectar")}</button>
                            </div>
                        </div>
                        <h4 class="ra-sub">{tr!("Jogados recentemente")}</h4>
                        <div class="ra-games">{games}</div>
                        <h4 class="ra-sub">{tr!("Conquistas nos clássicos")}</h4>
                        {move || if retroarch.get() {
                            view! { <p class="muted small"><span class="pill ok"><Svg icon=Icon::Check size=14 />{tr!("Ligadas")}</span>" "{tr!("O RetroArch usa esta conta em todo jogo aberto pelo Cerberus.")}</p> }.into_any()
                        } else {
                            view! {
                                <p class="muted small">{tr!("Para ganhar conquistas jogando, o RetroArch precisa entrar na sua conta. Informe a senha uma vez: o Cerberus troca por um código de acesso e não guarda a senha.")}</p>
                                <form class="ra-form" on:submit=link>
                                    <input class="input" type="password" placeholder=tr!("Sua senha do RetroAchievements") autocomplete="current-password"
                                        prop:value=move || password.get() on:input=move |e| password.set(event_target_value(&e)) />
                                    <button type="submit" class="btn sm main" disabled=move || password.get().is_empty()>{tr!("Ligar")}</button>
                                </form>
                            }.into_any()
                        }}
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
                        <input class="input" type="password" placeholder=tr!("Senha (opcional, para as conquistas no RetroArch)") autocomplete="current-password"
                            prop:value=move || password.get() on:input=move |e| password.set(event_target_value(&e)) />
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
    // Hydra lists are reference only: one folded group under the project sources, said once.
    let (hydra, projects): (Vec<SourceView>, Vec<SourceView>) = sources.into_iter().partition(|s| s.hydra);
    let hydra_total: usize = hydra.iter().map(|s| s.games).sum();
    let hydra_count = hydra.len();
    let unfolded = RwSignal::new(false);
    let row = move |source: SourceView| {
        {
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
        }
    };
    let rows = projects.into_iter().map(row).collect_view();
    let hydra_rows = hydra.into_iter().map(row).collect_view();
    let hydra_group = (hydra_count > 0).then(|| view! {
        <div class="set-row sub hydra-group">
            <div>
                <b>{if hydra_count == 1 { tr!("1 lista do Hydra").to_string() } else { tr!("{} listas do Hydra", hydra_count) }}</b>
                <small>{tr!("{} publicações, só para consulta: não aparecem no Descobrir nem na Biblioteca, e o Cerberus não baixa nada delas.", format::count(hydra_total as u64))}</small>
            </div>
            <div class="row">
                <button type="button" class="btn sm" aria-expanded=move || unfolded.get().to_string()
                    on:click=move |_| unfolded.update(|v| *v = !*v)>
                    {move || if unfolded.get() { tr!("Recolher") } else { tr!("Mostrar listas") }}
                </button>
            </div>
        </div>
        <div class="hydra-lists" class:hidden=move || !unfolded.get()>{hydra_rows}</div>
    });
    view! {
        <section class="card set">
            <h3>{tr!("Fontes de catálogo")}</h3>
            <form class="set-row" on:submit=on_add>
                <div>
                    <b>{tr!("Adicionar por URL")}</b>
                    <small>{tr!("Fontes de projetos (no formato do Cerberus ou do RecompLauncher) entram no Descobrir e instalam da página do desenvolvedor. Listas do Hydra ficam só para consulta aqui: títulos, tamanhos e datas.")}</small>
                </div>
                <div class="row">
                    <input class="input" type="url" required aria-label=tr!("URL da fonte") placeholder=tr!("https://.../jogos.json") prop:value=move || url.get() on:input=move |e| url.set(event_target_value(&e)) />
                    <button type="submit" class="btn sm main" disabled=move || busy.get()>{tr!("Adicionar")}</button>
                </div>
            </form>
            {rows}
            {hydra_group}
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_search_finds_options_with_or_without_accents() {
        assert!(search("  ").is_empty());
        let canto = search("canto");
        assert!(canto.iter().any(|h| h.0 == Section::InGame && h.3 == Some("Canto da tela")));
        let plain_words = search("notificacoes");
        assert!(plain_words.iter().any(|h| h.0 == Section::Notices && h.3.is_none()), "the section itself");
        assert!(search("bandeja windows").iter().all(|h| h.0 == Section::General));
        assert!(search("zzzz").is_empty());
        // Every option points at a section the menu has.
        for (section, _) in FIND {
            assert!(MENU.iter().any(|(_, items)| items.iter().any(|(s, _, _)| s == section)));
        }
    }
}
