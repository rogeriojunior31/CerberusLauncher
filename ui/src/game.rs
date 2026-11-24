//! A game's page: the art across the top, the main action, and tabs.
//!
//! The same page serves a game in the library and a catalog recomp not yet
//! installed (`recomp:<id>`), so Discover and the library lead to one place.

use cerberus_shared::tr;
use cerberus_shared::{CopyHandoff, Game, RecompEntry, Source};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::art::{Backdrop, Poster, cover_sources, hero_sources, is_low_res, recomp_covers, recomp_heroes};
use crate::backups::Backups;
use crate::discover::{progress_view, start_install};
use crate::format::{self, megabytes};
use crate::icons::{Icon, Svg};
use crate::tiles::source_name;
use crate::versions::{Switching, VersionLine, versions_of};
use crate::{Library, api, play_game};

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Overview,
    Saves,
    Mods,
    Photos,
    Achievements,
    Settings,
    Manage,
}

#[component]
pub fn GamePage(id: String) -> impl IntoView {
    let library = expect_context::<Library>();
    let id = StoredValue::new(id);
    let tab = RwSignal::new(Tab::Overview);
    let content = move || {
        let game = library.games.get().and_then(Result::ok).and_then(|all| all.into_iter().find(|g| g.id == id.get_value()));
        let recomp_id = id.get_value().strip_prefix("recomp:").map(str::to_string);
        let entry = recomp_id.and_then(|rid| {
            let all = library.recomps.get().and_then(Result::ok)?;
            all.into_iter().find(|e| e.id == rid)
        });
        if game.is_none() && entry.is_none() {
            let loading = library.games.get().is_none() || library.recomps.get().is_none();
            let text = if loading { tr!("Carregando...") } else { tr!("Esse jogo não está mais na biblioteca.") };
            return view! { <div class="page"><p class="muted">{text}</p></div> }.into_any();
        }
        view! { <Page game entry tab /> }.into_any()
    };
    let switching = Switching(RwSignal::new(false));
    provide_context(switching);
    view! {
        <div class="gp" class:switching=move || switching.0.get()>
            <button type="button" class="btn glass back" on:click=move |_| library.back()>
                <Svg icon=Icon::Back size=18 />{tr!("Voltar")}
            </button>
            {content}
        </div>
    }
}

#[component]
fn Page(game: Option<Game>, entry: Option<RecompEntry>, tab: RwSignal<Tab>) -> impl IntoView {
    let library = expect_context::<Library>();
    // The game players know comes first; a recomp is a version of it.
    let title = match (&game, &entry) {
        (Some(g), _) => crate::tiles::game_names(g, library).0,
        (None, Some(e)) => crate::tiles::entry_names(e).0,
        (None, None) => String::new(),
    };
    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
        doc.set_title(&format!("{title} · Cerberus Launcher"));
    }
    on_cleanup(|| {
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            doc.set_title("Cerberus Launcher");
        }
    });
    let covers = game.as_ref().map(cover_sources).filter(|c| !c.is_empty()).or_else(|| entry.as_ref().map(recomp_covers)).unwrap_or_default();
    let heroes = game.as_ref().map(hero_sources).filter(|h| !h.is_empty()).or_else(|| entry.as_ref().map(recomp_heroes)).unwrap_or_default();
    let siblings = entry.as_ref().map_or(0, |e| {
        library.recomps.with_untracked(|list| match list {
            Some(Ok(all)) => versions_of(all, &e.title).len(),
            _ => 0,
        })
    });
    // Under the title: which version this is (a recomp), or where the game comes from.
    let meta = match (&game, &entry) {
        (_, Some(current)) => view! { <VersionLine current=current.clone() covers=covers.clone() /> }.into_any(),
        (Some(g), None) => view! { <span class="gp-source">{source_name(&g.source)}</span> }.into_any(),
        (None, None) => ().into_any(),
    };
    let heart = game.as_ref().map(|g| {
        let (id, on) = (g.id.clone(), g.favorite);
        let label = if on { tr!("Tirar dos favoritos") } else { tr!("Adicionar aos favoritos") };
        view! {
            <button type="button" class="btn glass icon-btn fav-btn" class:on=on title=label aria-label=label aria-pressed=on.to_string()
                on:click=move |_| crate::toggle_favorite(library, id.clone(), !on)>
                <Svg icon=if on { Icon::HeartFill } else { Icon::Heart } size=20 />
            </button>
        }
    });
    // Everything the right-click menu has (folder, collections, hide...), from the page too.
    let more = game.clone().map(|g| {
        let menu = expect_context::<crate::game_menu::GameMenu>();
        view! {
            <button type="button" class="btn glass icon-btn" title=tr!("Mais opções") aria-label=tr!("Mais opções")
                on:click=move |e| menu.open(g.clone(), &e)>
                <Svg icon=Icon::More size=22 />
            </button>
        }
    });
    // Your time with the game, under the title: Steam's count for Steam games.
    let played = game.as_ref().map(|g| {
        let text = match (g.playtime_secs, g.last_played) {
            (0, None) => tr!("Você ainda não jogou").to_string(),
            (t, Some(last)) => tr!("Jogou por {} · última vez {}", format::playtime(t), format::ago(last)),
            (t, None) => tr!("Jogou por {}", format::playtime(t)),
        };
        view! { <span class="gp-played"><Svg icon=Icon::Clock size=15 />{text}</span> }
    });
    let has_saves = matches!(game.as_ref().map(|g| &g.source), Some(Source::Recomp { .. }));
    // Every game Cerberus starts gets the tab: picture filters, and the catalog's settings when it has some.
    let has_settings = game.as_ref().is_some_and(|g| g.is_tracked());
    let manageable = game.is_some() || entry.is_some();
    // F12 takes pictures of the games Cerberus opens itself.
    let photos = matches!(game.as_ref().map(|g| &g.source), Some(Source::Recomp { .. } | Source::Local { .. }));
    // Steam games: their achievements, as the hub reads them.
    let steam_app = game.as_ref().and_then(|g| g.id.strip_prefix("steam:")).map(str::to_string);
    let has_achievements = steam_app.is_some();
    // Read once for the tab's count, the rail card and the tab itself.
    let achievements = steam_app.clone().map(|app| {
        LocalResource::new(move || {
            let app = app.clone();
            async move { api::game_achievements("steam", &app).await }
        })
    });
    provide_context(GameAchievementsData(achievements));

    if matches!(tab.get_untracked(), Tab::Saves | Tab::Mods) && !has_saves
        || tab.get_untracked() == Tab::Settings && !has_settings
        || tab.get_untracked() == Tab::Manage && !manageable
        || tab.get_untracked() == Tab::Achievements && !has_achievements
    {
        tab.set(Tab::Overview);
    }
    let action = primary_action(game.clone(), entry.clone(), siblings > 1);
    // The pill behind the chosen tab slides to it.
    Effect::new(move || {
        tab.track();
        request_animation_frame(|| {
            let Some(on) = document().query_selector(".gp-page .tabs .tab.on").ok().flatten() else { return };
            let on: web_sys::HtmlElement = wasm_bindgen::JsCast::unchecked_into(on);
            if let Some(bar) = on.parent_element() {
                let bar: web_sys::HtmlElement = wasm_bindgen::JsCast::unchecked_into(bar);
                // The first placement lands at once; only moves between tabs slide.
                let first = !bar.class_list().contains("pill-on");
                if first {
                    let _ = bar.class_list().add_1("pill-snap");
                }
                let _ = bar.style().set_property("--px", &format!("{}px", on.offset_left()));
                let _ = bar.style().set_property("--pw", &format!("{}px", on.offset_width()));
                let _ = bar.class_list().add_1("pill-on");
                if first {
                    request_animation_frame(move || {
                        let _ = bar.class_list().remove_1("pill-snap");
                    });
                }
            }
        });
    });
    let count = move |which: Tab| -> Option<String> {
        match which {
            Tab::Achievements => achievements.and_then(|r| r.get()).and_then(Result::ok).filter(|d| d.game.total > 0).map(|d| format!("{}/{}", d.game.unlocked, d.game.total)),
            _ => None,
        }
    };
    let tab_button = move |which: Tab, label: &'static str| {
        let icon = match which {
            Tab::Overview => Icon::Info,
            Tab::Saves => Icon::Saves,
            Tab::Settings => Icon::Settings,
            Tab::Mods => Icon::Puzzle,
            Tab::Photos => Icon::Photo,
            Tab::Achievements => Icon::Trophy,
            Tab::Manage => Icon::Edit,
        };
        view! {
            <button type="button" class="tab with-icon" role="tab" class:on=move || tab.get() == which aria-selected=move || (tab.get() == which).to_string()
                tabindex=move || if tab.get() == which { "0" } else { "-1" } on:click=move |_| tab.set(which)>
                <Svg icon size=16 />{label}
                {move || count(which).map(|n| view! { <span class="tab-n">{n}</span> })}
            </button>
        }
    };
    // Left and right move between tabs, as tab lists do.
    let tab_keys = move |e: leptos::ev::KeyboardEvent| {
        let step: i32 = match e.key().as_str() {
            "ArrowRight" => 1,
            "ArrowLeft" => -1,
            _ => return,
        };
        e.prevent_default();
        let Some(list) = document().query_selector_all(".gp-page .tabs .tab").ok() else { return };
        let n = list.length() as i32;
        let at = (0..n).find(|i| list.item(*i as u32).and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::Element>(t).ok()).is_some_and(|t| t.class_list().contains("on"))).unwrap_or(0);
        if let Some(next) = list.item(((at + step + n) % n) as u32) {
            let next: web_sys::HtmlElement = wasm_bindgen::JsCast::unchecked_into(next);
            next.click();
            let _ = next.focus();
        }
    };
    let tabs = view! {
        <nav class="tabs" role="tablist" aria-label=tr!("Seções do jogo") on:keydown=tab_keys>
            {tab_button(Tab::Overview, tr!("Visão geral"))}
            {has_saves.then(|| tab_button(Tab::Saves, tr!("Saves")))}
            {has_settings.then(|| tab_button(Tab::Settings, tr!("Ajustes")))}
            {has_saves.then(|| tab_button(Tab::Mods, "Mods"))}
            {has_achievements.then(|| tab_button(Tab::Achievements, tr!("Conquistas")))}
            {photos.then(|| tab_button(Tab::Photos, tr!("Fotos")))}
            {manageable.then(|| tab_button(Tab::Manage, tr!("Gerenciar")))}
        </nav>
    };

    let body_game = game.clone();
    let body_entry = entry.clone();
    let tab_body = move |which: Tab| match which {
        Tab::Overview => view! { <Overview game=body_game.clone() entry=body_entry.clone() tab /> }.into_any(),
        Tab::Achievements => match &steam_app {
            Some(app) => view! { <crate::achievements::AchievementsTab source="steam".to_string() key=app.clone() /> }.into_any(),
            None => ().into_any(),
        },
        Tab::Saves => match &body_game {
            Some(g) => view! { <Backups game_id=g.id.clone() /> }.into_any(),
            None => ().into_any(),
        },
        Tab::Mods => match &body_game {
            Some(g) => view! { <Mods game_id=g.id.clone() /> }.into_any(),
            None => ().into_any(),
        },
        Tab::Photos => match &body_game {
            Some(g) => view! { <Photos game_id=g.id.clone() /> }.into_any(),
            None => ().into_any(),
        },
        Tab::Settings => match &body_game {
            Some(g) => {
                let preview = hero_sources(g).into_iter().next().or_else(|| cover_sources(g).into_iter().next());
                let catalog = body_entry.as_ref().is_some_and(|e| e.settings_count > 0);
                view! {
                    <div class="settings-tab">
                        <crate::looks::LooksCard game_id=g.id.clone() preview />
                        {catalog.then(|| view! { <crate::game_settings::GameSettingsTab game_id=g.id.clone() /> })}
                    </div>
                }
                .into_any()
            }
            None => ().into_any(),
        },
        Tab::Manage => view! { <Manage game=body_game.clone() entry=body_entry.clone() /> }.into_any(),
    };
    // Each tab arrives with a short fade, so the switch reads as one place changing.
    let body = move || {
        let which = tab.get();
        view! { <div class="tab-body" role="tabpanel">{tab_body(which)}</div> }
    };

    view! {
        <div class="gp-hero">
            <Backdrop sources=heroes soft=entry.as_ref().and_then(|e| e.hero.as_deref()).is_some_and(is_low_res) />
            <div class="gp-shade"></div>
        </div>
        <div class="page gp-page">
            <section class="gp-head">
                <div class="gp-cover"><Poster sources=covers.clone() title=title.clone() /></div>
                <div class="gp-info">
                    {match game.as_ref().and_then(|g| g.logo_path.clone()).or_else(|| entry.as_ref().and_then(|e| e.logo.clone())) {
                        Some(logo) => view! { <h1 class="vh">{title.clone()}</h1><img class="gp-logo" src=crate::art::src(&logo) alt=title.clone() /> }.into_any(),
                        None => view! { <h1>{title.clone()}</h1> }.into_any(),
                    }}
                    {meta}
                    {played}
                </div>
                <div class="gp-act">{heart}{more}{action}</div>
            </section>
            {tabs}
            {body}
        </div>
    }
}

/// Play, Playing, Install with its size, the install's progress, or Update.
fn primary_action(game: Option<Game>, entry: Option<RecompEntry>, several: bool) -> impl IntoView {
    let library = expect_context::<Library>();
    let recomp_id = entry.as_ref().map(|e| e.id.clone());
    let more_open = RwSignal::new(false);
    move || {
        let step = recomp_id.as_ref().and_then(|rid| library.progress.with(|all| all.get(rid).cloned()));
        if let (Some(step), Some(rid)) = (step, recomp_id.clone()) {
            return progress_view(step, rid).into_any();
        }
        if let Some(game) = game.clone() {
            let running = library.running.with(|all| all.contains(&game.id));
            // Installed outside Cerberus: its updates are the player's.
            let update = entry.as_ref().filter(|_| !game.external).and_then(|e| {
                let newest = e.latest.as_ref()?;
                (e.installed.as_ref() != Some(&newest.version)).then(|| newest.version.clone())
            });
            let update_button = update.zip(recomp_id.clone()).map(|(version, rid)| {
                view! {
                    <button type="button" class="btn big" on:click=move |_| start_install(library, rid.clone())>
                        {tr!("Atualizar para {}", version)}
                    </button>
                }
            });
            let (label, icon) = if running { (tr!("Jogando"), Icon::Play) } else { crate::launch_action(&game) };
            // The copy the recomp was given, else the one found in the ROM folders.
            let original = game
                .game_copy
                .clone()
                .or_else(|| entry.as_ref().and_then(|e| e.owned_copy.clone()))
                .filter(|_| matches!(game.source, Source::Recomp { .. }))
                .and_then(|copy| crate::classics::emulated_console(&copy).map(|c| (copy, c.to_string())))
                .map(|(copy, console)| (copy, console));
            let play = view! {
                <button type="button" class="btn main big play" disabled=running on:click=move |_| play_game(library, game.clone())>
                    <Svg icon size=20 />{label}
                </button>
            };
            // The original game in its emulator is another way to play: under the Play button's arrow.
            let Some((copy, console)) = original else {
                return view! { {update_button}{play} }.into_any();
            };
            return view! {
                {update_button}
                <div class="split">
                    {play}
                    <button type="button" class="btn main big split-more" aria-label=tr!("Outras formas de jogar") aria-haspopup="menu"
                        aria-expanded=move || more_open.get().to_string() on:click=move |_| more_open.update(|o| *o = !*o)>
                        <Svg icon=Icon::Chevron size=18 />
                    </button>
                    {move || more_open.get().then(|| {
                        let (copy, console) = (copy.clone(), console.clone());
                        view! {
                            <div class="vm-scrim" on:click=move |_| more_open.set(false)></div>
                            <div class="split-menu" role="menu" on:keydown=move |e| if e.key() == "Escape" { more_open.set(false) }>
                                <button type="button" role="menuitem" autofocus=true on:click=move |_| {
                                    more_open.set(false);
                                    crate::classics::play_original(library, copy.clone(), console.clone());
                                }>
                                    <Svg icon=Icon::Gamepad size=18 />
                                    <span><b>{tr!("Jogar o original")}</b><small>{tr!("A sua cópia do jogo, no emulador")}</small></span>
                                </button>
                            </div>
                        }
                    })}
                </div>
            }
            .into_any();
        }
        let Some(entry) = entry.clone() else {
            return ().into_any();
        };
        match entry.latest.clone() {
            Some(release) => {
                let rid = entry.id.clone();
                view! {
                    <AlreadyInstalled recomp_id=entry.id.clone() />
                    <button type="button" class="btn main big play" on:click=move |_| start_install(library, rid.clone())>
                        <Svg icon=Icon::Download size=20 />{if several { tr!("Instalar {} · {}", entry.name, megabytes(release.size)) } else { tr!("Instalar · {}", megabytes(release.size)) }}
                    </button>
                }
                .into_any()
            }
            None => view! { <button type="button" class="btn big" disabled=true>{tr!("Indisponível")}</button> }.into_any(),
        }
    }
}

#[component]
fn Overview(game: Option<Game>, entry: Option<RecompEntry>, tab: RwSignal<Tab>) -> impl IntoView {
    // Project facts go to the side rail; the player's own numbers live in the play card.
    let mut facts: Vec<(String, String)> = Vec::new();
    if let Some(g) = &game {
        if let Source::Recomp { version, .. } = &g.source {
            let shown = if g.external && version.is_empty() { tr!("Instalado por você").to_string() } else { version.clone() };
            facts.push((tr!("Versão").to_string(), shown));
        }
    }
    if let Some(e) = &entry {
        if let Some(release) = &e.latest {
            if game.is_none() {
                facts.push((tr!("Última versão").to_string(), release.version.clone()));
            }
            if release.size > 0 {
                facts.push((tr!("Tamanho").to_string(), megabytes(release.size)));
            }
        }
        facts.push((tr!("Console").to_string(), e.console.clone()));
        if e.downloads > 0 {
            facts.push((tr!("Downloads").to_string(), format::count(e.downloads)));
        }
        if e.stars > 0 {
            facts.push((tr!("Estrelas").to_string(), format::count(e.stars)));
        }
        if e.players > 0 {
            facts.push((tr!("Jogadores recentes").to_string(), format::count(e.players)));
        }
        if e.releases > 0 {
            facts.push((tr!("Versões lançadas").to_string(), e.releases.to_string()));
        }
        if !e.first_release.is_empty() {
            facts.push((tr!("Primeira versão").to_string(), day(&e.first_release)));
        }
    }
    let facts_card = (!facts.is_empty()).then(|| {
        let rows = facts.into_iter().map(|(k, v)| view! { <div><dt>{k}</dt><dd>{v}</dd></div> }).collect_view();
        view! { <section class="card rail-card"><h3>{tr!("Detalhes")}</h3><dl class="facts">{rows}</dl></section> }
    });
    let play_card = game.clone().map(|g| view! { <crate::game_overview::PlayCard id=g.id.clone() total=g.playtime_secs last=g.last_played /> }.into_any());
    // Your numbers lead once there is a story to tell (an hour or more); before that they wait below.
    let played_a_lot = game.as_ref().is_some_and(|g| g.playtime_secs >= 3600);
    let note_card = game.clone().map(|g| view! { <crate::game_notes::NoteCard game=g /> }.into_any());
    let steam_app = game.as_ref().and_then(|g| match &g.source {
        Source::Steam { app_id } => Some(*app_id),
        _ => None,
    });
    if let Some(app) = steam_app {
        return view! { <SteamOverview app play_card note_card tab /> }.into_any();
    }

    let about = entry.as_ref().map(|e| e.about.clone()).filter(|a| !a.is_empty()).map(|text| view! { <p class="about">{text}</p> });
    let problem = entry.as_ref().and_then(|e| e.problem.clone()).map(|text| view! { <p class="problem">{text}</p> });
    let tags = entry.as_ref().filter(|e| !e.tags.is_empty() || !e.multiplayer.is_empty()).map(|e| {
        let players = match e.multiplayer.as_str() {
            "local" => Some(tr!("Multijogador local")),
            "online" => Some(tr!("Multijogador online")),
            "both" => Some(tr!("Multijogador local e online")),
            _ => None,
        };
        let players = players.map(|p| view! { <span class="tag players">{p}</span> });
        let chips = e.tags.iter().map(|tag| view! { <span class="tag">{tag.clone()}</span> }).collect_view();
        view! { <div class="ver-part"><span class="ver-label">{tr!("Recursos")}</span><div class="tags">{players}{chips}</div></div> }
    });
    let note = entry.as_ref().map(|e| e.player_note.clone()).filter(|n| !n.is_empty()).map(|text| {
        view! {
            <div class="note">
                <Svg icon=Icon::Check size=18 />
                <div><b>{tr!("Como começar")}</b><p>{text}</p></div>
            </div>
        }
    });
    let features = entry.as_ref().filter(|e| !e.features.is_empty()).map(|e| {
        let items = e.features.iter().map(|f| view! { <li><Svg icon=Icon::Check size=16 />{f.clone()}</li> }).collect_view();
        view! { <div class="ver-part"><span class="ver-label">{tr!("O que esta versão faz")}</span><ul class="features">{items}</ul></div> }
    });
    let setup = entry.as_ref().filter(|e| !e.setup_steps.is_empty()).map(|e| {
        let steps = e.setup_steps.iter().map(|s| view! { <li>{s.clone()}</li> }).collect_view();
        view! {
            <section class="card setup">
                <h3>{tr!("Precisa de configuração")}</h3>
                <p class="muted small">{tr!("Esse recomp precisa de alguns passos feitos à mão antes de jogar:")}</p>
                <ol>{steps}</ol>
            </section>
        }
    });
    let installed_recomp = game.as_ref().filter(|g| matches!(g.source, Source::Recomp { .. })).cloned();
    let setup_card = match (installed_recomp, &entry) {
        (Some(g), Some(e)) if e.handoff.as_ref().is_some_and(|h| *h != CopyHandoff::AsksItself) => {
            Some(view! { <EasySetup game=g entry=e.clone() /> }.into_any())
        }
        _ => None,
    };
    let needs = entry.as_ref().filter(|_| setup_card.is_none()).map(|e| {
        let copy = match &e.owned_copy {
            Some(path) => view! {
                <div class="need found">
                    <Svg icon=Icon::Check size=18 />
                    <div>
                        <b>{tr!("Sua cópia foi encontrada")}</b>
                        <span class="path">{path.clone()}</span>
                    </div>
                </div>
            }
            .into_any(),
            None => view! {
                <div class="need">
                    <Svg icon=Icon::Check size=18 />
                    <div>
                        <b>{tr!("Sua cópia do jogo")}</b>
                        <span>{if e.needs.is_empty() { tr!("Sua cópia de {}.", e.game) } else { e.needs.clone() }}</span>
                    </div>
                </div>
            }
            .into_any(),
        };
        view! {
            <aside class="card ready">
                <h3>{tr!("Antes de jogar")}</h3>
                {copy}
                <p class="muted small">{tr!("O Cerberus nunca baixa arquivos de jogo. Aponte suas pastas de ROMs em Configurações e ele avisa quais jogos você já tem.")}</p>
            </aside>
        }
    });

    let (play_top, play_bottom) = if played_a_lot { (play_card, None) } else { (None, play_card) };
    view! {
        <div class="ov">
            <div class="ov-main">
                {play_top}
                // The game first, then what this version brings, then what it needs.
                {entry.clone().and_then(|e| e.original.clone().map(|o| view! { <OriginalGameCard original=o game=e.game.clone() /> }))}
                {entry.clone().map(|e| view! {
                    <section class="card ver-card">
                        <h3>{tr!("Esta versão")}<span class="ver-name">{e.name.clone()}</span></h3>
                        {about}
                        {problem}
                        {tags}
                        {features}
                        {note}
                    </section>
                })}
                {setup}
                {play_bottom}
                {entry.clone().map(|e| view! { <SimilarGames entry=e /> })}
            </div>
            <aside class="ov-rail">
                {note_card}
                {setup_card}
                {needs}
                {facts_card}
            </aside>
        </div>
    }
    .into_any()
}

/// A Steam game's Overview: your play time, the store's pictures and text, and
/// the rail with achievements, details, controller, requirements, languages, links.
/// A Steam game's achievements, read once per page.
#[derive(Clone, Copy)]
pub struct GameAchievementsData(pub Option<LocalResource<Result<cerberus_shared::GameAchievements, String>>>);

#[component]
fn SteamOverview(app: u32, play_card: Option<AnyView>, note_card: Option<AnyView>, tab: RwSignal<Tab>) -> impl IntoView {
    let details = LocalResource::new(move || async move { api::steam_details(app).await });
    let on_all = Callback::new(move |_| tab.set(Tab::Achievements));
    let store = move || match details.get() {
        None => view! { <crate::game_overview::StoreSkeleton /> }.into_any(),
        Some(Err(e)) => view! { <p class="muted small">{e}</p> }.into_any(),
        Some(Ok(d)) => view! {
            <crate::game_overview::Gallery details=d.clone() />
            <crate::game_overview::About details=d />
        }
        .into_any(),
    };
    let rail = move || match details.get() {
        Some(Ok(d)) => {
            let site = d.website.is_some();
            view! {
                <crate::game_overview::StoreFacts details=d.clone() />
                <crate::game_overview::Controller details=d.clone() />
                <crate::game_overview::Requirements details=d.clone() />
                <crate::game_overview::Languages details=d />
                <crate::game_overview::Links app has_site=site />
            }
            .into_any()
        }
        None => view! { <div class="sk sk-card"></div> }.into_any(),
        Some(Err(_)) => view! { <crate::game_overview::Links app has_site=false /> }.into_any(),
    };
    view! {
        <div class="ov">
            <div class="ov-main">
                {play_card}
                {store}
            </div>
            <aside class="ov-rail">
                {note_card}
                {use_context::<GameAchievementsData>().and_then(|d| d.0).map(|data| view! { <crate::game_overview::AchievementsCard data on_all /> })}
                {rail}
            </aside>
        </div>
    }
}

/// Easy Setup: the recomp takes the player's copy of the game from the launcher
/// (on its command line, in a variable, or copied into its folder).
#[component]
fn EasySetup(game: Game, entry: RecompEntry) -> impl IntoView {
    let library = expect_context::<Library>();
    let id = StoredValue::new(game.id.clone());
    let busy = RwSignal::new(false);
    let current = RwSignal::new(game.game_copy.clone());
    // Chosen once, but no longer where the game reads it (deleted from its folder,
    // or a file that moved): the backend's next step says so, and the card agrees.
    let lost = RwSignal::new(matches!(game.next_step, cerberus_shared::NextStep::ChooseCopy { .. }) && game.game_copy.is_some());

    let accepts_folder = entry.accepts_folder;
    let only_folder = entry.only_folder;
    let choose_from = move |use_found: bool, folder: bool| {
        busy.set(true);
        spawn_local(async move {
            let set = library.report(api::set_game_copy(&id.get_value(), use_found, folder).await).flatten();
            busy.set(false);
            if let Some(game) = set {
                lost.set(false);
                current.set(game.game_copy.clone());
                library.games.refetch();
                library.toast(tr!("Pronto: o jogo vai abrir com a sua cópia."));
            }
        });
    };
    // A recomp that takes only extracted folders never gets a file picker.
    let choose = move |use_found: bool| choose_from(use_found, only_folder && !use_found);
    let pick = if only_folder { tr!("Escolher pasta...") } else { tr!("Escolher arquivo...") };
    let change = if only_folder { tr!("Trocar pasta...") } else { tr!("Trocar arquivo...") };
    let folder_button = move || {
        (accepts_folder && !only_folder).then(|| {
            view! { <button type="button" class="btn sm" disabled=move || busy.get() on:click=move |_| choose_from(false, true)>{tr!("Escolher pasta...")}</button> }
        })
    };

    let found = entry.owned_copy.clone();
    let what = if entry.needs.is_empty() { tr!("Sua cópia de {}.", entry.game) } else { entry.needs.clone() };
    let warning = move || lost.get().then(|| view! {
        <p class="problem small">{tr!("A cópia escolhida não está mais onde o jogo procura. Escolha de novo.")}</p>
    });
    let body = move || match (current.get().filter(|_| !lost.get()), found.clone()) {
        (Some(path), _) => view! {
            <div class="need found">
                <Svg icon=Icon::Check size=18 />
                <div><b>{tr!("Cópia selecionada")}</b><span class="path">{path}</span></div>
            </div>
            <button type="button" class="btn sm" disabled=move || busy.get() on:click=move |_| choose(false)>{change}</button>
        }
        .into_any(),
        (None, Some(path)) => view! {
            <div class="need">
                <Svg icon=Icon::Check size=18 />
                <div><b>{tr!("Encontramos sua cópia")}</b><span class="path">{path}</span></div>
            </div>
            <div class="row">
                <button type="button" class="btn sm main" disabled=move || busy.get() on:click=move |_| choose(true)>{tr!("Usar esta cópia")}</button>
                <button type="button" class="btn sm" disabled=move || busy.get() on:click=move |_| choose(false)>{tr!("Escolher outra...")}</button>
            </div>
        }
        .into_any(),
        (None, None) => view! {
            <p class="muted">{what.clone()}</p>
            <div class="row">
                <button type="button" class="btn sm main" disabled=move || busy.get() on:click=move |_| choose(false)>{pick}</button>
                {folder_button}
            </div>
        }
        .into_any(),
    };

    view! {
        <aside class="card ready">
            <h3>{tr!("Easy Setup")}</h3>
            {warning}
            {body}
            <p class="muted small">{tr!("O Cerberus entrega sua cópia ao jogo do jeito que ele espera. Nada sai do seu PC.")}</p>
        </aside>
    }
}

/// "por Autor · v1.2 · 3 mil downloads"; GameBanana tells likes instead, and
/// its "version" can be the date of the last update (only digits): left out.
fn mod_facts(m: &cerberus_shared::ModItem) -> String {
    let mut facts = vec![format!("por {}", m.owner)];
    let a_date = !m.version.is_empty() && m.version.chars().all(|c| c.is_ascii_digit());
    if !m.version.is_empty() && !a_date {
        facts.push(format!("v{}", m.version.trim_start_matches(['v', 'V'])));
    }
    if m.downloads > 0 {
        facts.push(format!("{} downloads", format::count(m.downloads)));
    } else if m.likes > 0 {
        facts.push(format!("{} curtidas", format::count(m.likes)));
    }
    facts.join(" · ")
}

#[component]
fn Mods(game_id: String) -> impl IntoView {
    let library = expect_context::<Library>();
    let id = StoredValue::new(game_id);
    let refresh = RwSignal::new(0u32);
    let search = RwSignal::new(String::new());
    let busy = RwSignal::new(None::<String>);
    let list = LocalResource::new(move || {
        refresh.track();
        let id = id.get_value();
        async move { api::list_mods(&id).await }
    });

    let act = move |full: String, install: bool| {
        busy.set(Some(full.clone()));
        spawn_local(async move {
            let id = id.get_value();
            let done = if install { api::install_mod(&id, &full).await } else { api::remove_mod(&id, &full).await };
            busy.set(None);
            if library.report(done).is_some() {
                library.toast(if install { tr!("Mod instalado. Ele vale na próxima vez que o jogo abrir.") } else { tr!("Mod removido.") });
            }
            refresh.update(|n| *n += 1);
        });
    };

    let rows = move || match list.get() {
        None => view! { <p class="muted">{tr!("Carregando os mods...")}</p> }.into_any(),
        Some(Err(error)) => view! { <p class="problem">{error}</p> }.into_any(),
        Some(Ok(None)) => view! { <p class="muted">{tr!("Esse jogo não tem mods no Thunderstore nem no GameBanana.")}</p> }.into_any(),
        Some(Ok(Some(mods))) => {
            let needle = search.get().to_lowercase();
            let shown: Vec<_> = mods
                .items
                .into_iter()
                .filter(|m| needle.is_empty() || format!("{} {} {}", m.name, m.owner, m.description).to_lowercase().contains(&needle))
                .take(60)
                .collect();
            let items = shown
                .into_iter()
                .map(|m| {
                    let full = m.full_name.clone();
                    let full_remove = m.full_name.clone();
                    let working = {
                        let full = m.full_name.clone();
                        move || busy.get().as_deref() == Some(full.as_str())
                    };
                    let action = match &m.installed {
                        Some(v) if *v == m.version => view! {
                            <button type="button" class="btn sm" disabled=working on:click=move |_| act(full_remove.clone(), false)>{tr!("Remover")}</button>
                        }
                        .into_any(),
                        Some(_) => view! {
                            <button type="button" class="btn sm main" disabled=working.clone() on:click=move |_| act(full.clone(), true)>{tr!("Atualizar")}</button>
                            <button type="button" class="btn sm" disabled=working on:click=move |_| act(full_remove.clone(), false)>{tr!("Remover")}</button>
                        }
                        .into_any(),
                        None => view! {
                            <button type="button" class="btn sm main" disabled=working on:click=move |_| act(full.clone(), true)>{tr!("Instalar")}</button>
                        }
                        .into_any(),
                    };
                    view! {
                        <li class="mod">
                            <img src=m.icon.clone() alt="" loading="lazy" />
                            <div>
                                <b>{m.name.clone()}</b>
                                <span class="muted small">{mod_facts(&m)}</span>
                                <span class="small">{m.description.clone()}</span>
                            </div>
                            <div class="row">{action}</div>
                        </li>
                    }
                })
                .collect_view();
            view! {
                <p class="muted small">{tr!("De {} · instalados em {}", mods.from, mods.folder)}</p>
                <ul class="mods">{items}</ul>
            }
            .into_any()
        }
    };

    view! {
        <section class="card">
            <div class="card-h">
                <h3>"Mods"</h3>
                <label class="search">
                    <Svg icon=Icon::Search size=18 />
                    <input type="search" placeholder=tr!("Buscar mods") prop:value=move || search.get() on:input=move |e| search.set(event_target_value(&e)) />
                </label>
            </div>
            {rows}
        </section>
    }
}

/// The game's DLSS files and their versions; each can be swapped for a DLL of
/// the same name the player has, and put back. Shown only when the game has DLSS.
#[component]
fn Dlss(id: String) -> impl IntoView {
    let library = expect_context::<Library>();
    let id = StoredValue::new(id);
    let files = LocalResource::new(move || async move { api::dlss_list(&id.get_value()).await });
    let busy = RwSignal::new(false);
    let act = move |path: String, restore: bool| {
        busy.set(true);
        spawn_local(async move {
            let game = id.get_value();
            if restore {
                if library.report(api::dlss_restore(&game, &path).await).is_some() {
                    library.toast(tr!("O DLSS original do jogo voltou."));
                }
            } else if let Some(Some(version)) = library.report(api::dlss_swap(&game, &path).await) {
                library.toast(tr!("DLSS trocado: agora na versão {}.", version));
            }
            busy.set(false);
            files.refetch();
        });
    };
    move || {
        let list = files.get().and_then(Result::ok).unwrap_or_default();
        (!list.is_empty()).then(|| view! {
            <div class="card">
                <h3>"DLSS"</h3>
                <p class="muted small">{tr!("Troque pela versão de outro jogo ou baixada por você (o Cerberus não baixa nada). O arquivo do jogo fica guardado. Em jogo online com anti-cheat, prefira não trocar. Uma atualização do jogo pode trazer o arquivo dele de volta.")}</p>
                <div class="dlss-list">
                    {list.into_iter().map(|f| {
                        let swap_path = f.path.clone();
                        let back_path = f.path.clone();
                        let swapped = f.original.clone();
                        view! {
                            <div class="dlss-row">
                                <span>
                                    <b>{f.label.clone()}</b>
                                    <small>{tr!("Versão {}", f.version)}{swapped.clone().map(|o| tr!(" · a do jogo era {}", o))}</small>
                                    <small class="path">{f.path.clone()}</small>
                                </span>
                                <div class="dlss-btns">
                                    <button type="button" class="btn sm" disabled=move || busy.get() on:click=move |_| act(swap_path.clone(), false)>{tr!("Trocar versão…")}</button>
                                    {swapped.map(|_| view! {
                                        <button type="button" class="btn sm" disabled=move || busy.get() on:click=move |_| act(back_path.clone(), true)>{tr!("Voltar à original")}</button>
                                    })}
                                </div>
                            </div>
                        }
                    }).collect_view()}
                </div>
            </div>
        })
    }
}

#[component]
fn Manage(game: Option<Game>, entry: Option<RecompEntry>) -> impl IntoView {
    let library = expect_context::<Library>();
    let armed = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let is_recomp = matches!(game.as_ref().map(|g| &g.source), Some(Source::Recomp { .. }));
    let editable = game.as_ref().is_some_and(|g| matches!(g.source, Source::Local { .. } | Source::Recomp { .. }));

    // Not on this PC: no folder to open and nothing for a shortcut to start.
    let here = game.as_ref().filter(|g| g.next_step != cerberus_shared::NextStep::Install);
    let folder = here.map(|g| {
        let id = g.id.clone();
        let on_folder = move |_| {
            let id = id.clone();
            spawn_local(async move {
                library.report(api::open_folder(&id).await);
            });
        };
        view! {
            <button type="button" class="row-btn" on:click=on_folder>
                <Svg icon=Icon::Folder />
                <span><b>{tr!("Abrir pasta do jogo")}</b><small>{g.install_dir.clone()}</small></span>
            </button>
        }
    });
    let moving = RwSignal::new(false);
    let relocate = game.as_ref().filter(|g| is_recomp && !g.external && !g.setup_pending).map(|g| {
        let id = g.id.clone();
        let on_move = move |_| {
            let id = id.clone();
            moving.set(true);
            spawn_local(async move {
                let moved = library.report(api::move_game(&id).await);
                moving.set(false);
                if let Some(Some(game)) = moved {
                    library.games.refetch();
                    library.toast(tr!("{} foi movido para {}.", game.name, game.install_dir));
                }
            });
        };
        view! {
            <button type="button" class="row-btn" on:click=on_move disabled=move || moving.get()>
                <Svg icon=Icon::Folder />
                <span>
                    <b>{move || if moving.get() { tr!("Movendo… pode levar alguns minutos") } else { tr!("Mover para outra pasta ou disco") }}</b>
                    <small>{tr!("Leva a pasta inteira, com saves, mods e configurações, e o Cerberus passa a abrir de lá")}</small>
                </span>
            </button>
        }
    });
    let shortcut = here.map(|g| {
        let id = g.id.clone();
        let on_shortcut = move |_| {
            let id = id.clone();
            spawn_local(async move {
                if library.report(api::create_shortcut(&id).await).is_some() {
                    library.toast(tr!("Atalho criado na área de trabalho."));
                }
            });
        };
        view! {
            <button type="button" class="row-btn" on:click=on_shortcut>
                <Svg icon=Icon::External />
                <span><b>{tr!("Criar atalho na área de trabalho")}</b><small>{tr!("Abre o jogo pelo Cerberus: tempo de jogo, backups e filtros continuam valendo")}</small></span>
            </button>
        }
    });
    // Steam: only games it doesn't have already.
    let steam = game.as_ref().filter(|g| !matches!(g.source, Source::Steam { .. })).map(|g| {
        let id = StoredValue::new(g.id.clone());
        let added = RwSignal::new(false);
        let working = RwSignal::new(false);
        spawn_local(async move {
            added.set(api::in_steam(&id.get_value()).await.unwrap_or(false));
        });
        let on_click = move |_| {
            working.set(true);
            spawn_local(async move {
                let id = id.get_value();
                if added.get_untracked() {
                    if library.report(api::remove_from_steam(&id).await).is_some() {
                        added.set(false);
                        library.toast(tr!("Tirado da Steam."));
                    }
                } else if let Some(done) = library.report(api::add_to_steam(&id).await) {
                    added.set(true);
                    library.toast(done);
                }
                working.set(false);
            });
        };
        view! {
            <button type="button" class="row-btn" disabled=move || working.get() on:click=on_click>
                <Svg icon=Icon::Gamepad />
                <span>
                    <b>{move || if added.get() { tr!("Tirar da Steam") } else { tr!("Adicionar à Steam") }}</b>
                    <small>{move || if added.get() {
                        tr!("Está na biblioteca da Steam como jogo de fora; tirar não apaga nada do jogo")
                    } else {
                        tr!("Aparece na biblioteca da Steam, com a capa, e abre pelo Cerberus (Big Picture e Steam Deck). Feche a Steam antes")
                    }}</small>
                </span>
            </button>
        }
    });
    let customize = game.clone().map(|g| {
        view! {
            <button type="button" class="row-btn" on:click=move |_| library.customizing.set(Some(g.clone()))>
                <Svg icon=Icon::Edit />
                <span><b>{tr!("Personalizar arte")}</b><small>{tr!("Capa, fundo e logo do SteamGridDB ou de um arquivo seu, com recorte e zoom")}</small></span>
            </button>
        }
    });
    let edit = game.clone().filter(|_| editable).map(|g| {
        view! {
            <button type="button" class="row-btn" on:click=move |_| library.editing.set(Some(g.clone()))>
                <Svg icon=Icon::Edit />
                <span><b>{tr!("Editar nome e capa")}</b><small>{tr!("Renomear, buscar outra capa ou escolher uma imagem sua")}</small></span>
            </button>
        }
    });
    let project = entry.as_ref().map(|e| {
        let rid = e.id.clone();
        let repo = e.repo.clone();
        let host = if e.host.is_empty() { "GitHub".to_string() } else { e.host.clone() };
        let on_project = move |_| {
            let rid = rid.clone();
            spawn_local(async move {
                library.report(api::open_project(&rid).await);
            });
        };
        view! {
            <button type="button" class="row-btn" on:click=on_project>
                <Svg icon=Icon::External />
                <span><b>{tr!("Projeto no {}", host)}</b><small>{repo}</small></span>
            </button>
        }
    });
    // Reinstalls the newest release over the game: missing or broken files come
    // back, the saves get a backup first, and files the game made stay.
    let repair = game.as_ref().filter(|g| is_recomp && !g.external && !g.setup_pending).zip(entry.as_ref()).map(|(_, e)| {
        let rid = e.id.clone();
        let downloading = {
            let rid = rid.clone();
            move || library.progress.with(|all| all.contains_key(&rid))
        };
        let version = e.latest.as_ref().map(|l| l.version.clone()).unwrap_or_default();
        let detail = if version.is_empty() {
            tr!("Baixa o jogo de novo por cima; saves, configurações e mods ficam").to_string()
        } else {
            tr!("Baixa a versão {} de novo por cima; saves, configurações e mods ficam", version)
        };
        view! {
            <button type="button" class="row-btn" disabled=downloading on:click=move |_| crate::discover::start_install(library, rid.clone())>
                <Svg icon=Icon::Download />
                <span><b>{tr!("Reparar instalação")}</b><small>{detail}</small></span>
            </button>
        }
    });
    let remove = game.clone().filter(|_| editable).map(|g| {
        let id = g.id.clone();
        let on_remove = move |_| {
            if !armed.get_untracked() {
                armed.set(true);
                return;
            }
            busy.set(true);
            let id = id.clone();
            spawn_local(async move {
                let removed = library.report(api::remove_game(&id).await);
                busy.set(false);
                if removed.is_some() {
                    library.games.refetch();
                    library.recomps.refetch();
                    library.go(crate::Route::Library);
                }
            });
        };
        let external = g.external;
        let (title, detail) = if external {
            (tr!("Remover da biblioteca"), tr!("A pasta é sua e continua no PC. Os backups de saves continuam guardados."))
        } else if is_recomp {
            (tr!("Desinstalar"), tr!("Apaga a pasta do jogo. Os backups de saves continuam guardados."))
        } else {
            (tr!("Remover da biblioteca"), tr!("Os arquivos do jogo continuam no seu PC."))
        };
        let label = move || if armed.get() { tr!("Clique de novo para confirmar") } else { title };
        view! {
            <button type="button" class="row-btn danger" on:click=on_remove disabled=move || busy.get()>
                <Svg icon=Icon::Trash />
                <span><b>{label}</b><small>{detail}</small></span>
            </button>
        }
    });

    let options = game.clone().filter(|g| g.is_tracked()).map(|g| view! { <LaunchOptions game=g /> });

    let program = game.as_ref().and_then(|g| match &g.source {
        Source::Recomp { exe, .. } | Source::Local { exe } => Some(view! {
            <div class="card launch">
                <h3>{if g.setup_pending { tr!("Instalador de primeira execução") } else { tr!("Programa de abertura") }}</h3>
                <p class="path">{exe.clone()}</p>
                {(!g.setup_pending).then(|| view! { <TrackingProgram id=g.id.clone() selected=g.tracking_exe.is_some() /> })}
            </div>
        }),
        _ => None,
    });
    let history = game.as_ref().filter(|g| g.is_tracked()).map(|g| view! { <History id=g.id.clone() /> });
    let dlss = game.as_ref().filter(|g| !matches!(g.source, Source::Recomp { .. })).map(|g| view! { <Dlss id=g.id.clone() /> });
    let link = game.as_ref().filter(|g| matches!(g.source, Source::Local { .. })).map(|g| view! { <LinkToCatalog id=g.id.clone() /> });
    let outside = game.as_ref().filter(|g| g.external).map(|_| view! {
        <div class="card">
            <h3>{tr!("Instalado fora do Cerberus")}</h3>
            <p class="muted small">{tr!("Você trouxe esse recomp de uma pasta sua. O Cerberus abre, conta o tempo e faz backup dos saves, mas não atualiza nem apaga os arquivos dele.")}</p>
        </div>
    });
    // Recomps installed by Cerberus: stay on this version until the player says so.
    let hold = game.as_ref().filter(|g| is_recomp && !g.external).map(|g| {
        let id = StoredValue::new(g.id.clone());
        let on = RwSignal::new(g.updates_paused);
        let flip = move |_| {
            let want = !on.get_untracked();
            spawn_local(async move {
                if library.report(api::set_updates_paused(&id.get_value(), want).await).is_some() {
                    on.set(want);
                    library.games.refetch();
                    library.toast(if want { tr!("Atualizações pausadas. O jogo fica nesta versão.") } else { tr!("Atualizações retomadas.") });
                }
            });
        };
        view! {
            <button type="button" class="row-btn" role="switch" aria-checked=move || on.get().to_string() on:click=flip>
                <Svg icon=Icon::Pause />
                <span>
                    <b>{tr!("Pausar atualizações")}</b>
                    <small>{move || if on.get() { tr!("Pausadas: o Cerberus não oferece versões novas até você retomar.") } else { tr!("Fica nesta versão, mesmo quando sair uma nova. Útil se a nova quebrar seus mods ou saves.") }}</small>
                </span>
                <span class="switch" class:on=move || on.get()><i></i></span>
            </button>
        }
    });
    view! { <div class="manage">{outside}{link}{program}{options}{dlss}{history}{folder}{relocate}{hold}{shortcut}{steam}{customize}{edit}{project}{repair}{remove}</div> }
}

#[component]
fn TrackingProgram(id: String, selected: bool) -> impl IntoView {
    let library = expect_context::<Library>();
    let id = StoredValue::new(id);
    let busy = RwSignal::new(false);
    let mode = LocalResource::new(move || {
        let id = id.get_value();
        async move { api::tracking_mode(&id).await }
    });
    let choose = move |automatic| {
        busy.set(true);
        spawn_local(async move {
            if library.report(api::pick_tracking_exe(&id.get_value(), automatic).await).flatten().is_some() {
                library.games.refetch();
                library.toast(tr!("A medição escolhida vale a partir da próxima abertura."));
            }
            busy.set(false);
        });
    };
    move || mode.get().and_then(Result::ok).flatten().map(|label| view! {
        <h3>{tr!("Processo acompanhado")}</h3>
        <p class="path">{label}</p>
        <div class="row">
            <button class="btn sm" disabled=move || busy.get() on:click=move |_| choose(false)>{tr!("Escolher executável")}</button>
            {selected.then(|| view! { <button class="btn sm" disabled=move || busy.get() on:click=move |_| choose(true)>{tr!("Usar automático")}</button> })}
        </div>
    })
}

/// Extra command-line options for a game the launcher starts itself.
#[component]
fn LaunchOptions(game: Game) -> impl IntoView {
    let library = expect_context::<Library>();
    let id = StoredValue::new(game.id.clone());
    let text = RwSignal::new(game.launch_args.clone());
    let on_save = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        spawn_local(async move {
            if library.report(api::set_launch_args(&id.get_value(), &text.get_untracked()).await).is_some() {
                library.games.refetch();
                library.toast(tr!("Opções de inicialização salvas."));
            }
        });
    };
    view! {
        <form class="card launch" on:submit=on_save>
            <h3>{tr!("Opções de inicialização")}</h3>
            <p class="muted small">{tr!("Passadas ao jogo quando ele abre, depois das do Cerberus. Use aspas para valores com espaço.")}</p>
            <div class="row">
                <input class="input grow" placeholder=tr!("ex.: --fullscreen --fps 120") prop:value=move || text.get() on:input=move |e| text.set(event_target_value(&e)) />
                <button type="submit" class="btn main">{tr!("Salvar")}</button>
            </div>
        </form>
    }
}

/// The launcher's recent steps for this game, kept across restarts, with the
/// last failure up top so its cause can still be read after the notice is gone.
/// The pictures taken of the game with F12, newest first.
#[component]
fn Photos(game_id: String) -> impl IntoView {
    let library = expect_context::<Library>();
    let id = StoredValue::new(game_id);
    let shots = LocalResource::new(move || async move { api::list_screenshots(&id.get_value()).await });
    api::on_screenshot(move |taken: String| {
        if taken == id.get_value() {
            shots.refetch();
        }
    });
    let open_folder = move |_| {
        spawn_local(async move {
            library.report(api::open_screenshots_folder(&id.get_value()).await);
        });
    };
    view! {
        <section class="card">
            <div class="card-h">
                <h3>{tr!("Fotos")}</h3>
                <button type="button" class="btn sm" on:click=open_folder><Svg icon=Icon::Folder size=16 />{tr!("Abrir pasta")}</button>
            </div>
            <p class="muted small">{tr!("Com o jogo aberto pelo Cerberus e na frente, aperte F12: a foto é só da janela do jogo e aparece aqui. Jogos em tela cheia exclusiva podem sair pretos; use janela sem bordas.")}</p>
            {move || match shots.get() {
                None => view! { <p class="muted">{tr!("Carregando...")}</p> }.into_any(),
                Some(Err(e)) => view! { <p class="problem">{e}</p> }.into_any(),
                Some(Ok(list)) if list.is_empty() => view! {
                    <div class="empty small">
                        <Svg icon=Icon::Photo size=28 />
                        <p class="muted">{tr!("Nenhuma foto ainda.")}</p>
                    </div>
                }.into_any(),
                Some(Ok(list)) => view! {
                    <div class="shots">
                        {list.into_iter().map(|shot| {
                            let path = shot.path.clone();
                            let when = format::ago(shot.taken);
                            view! {
                                <button type="button" class="shot" title=tr!("Abrir · {}", when) on:click=move |_| {
                                    let path = path.clone();
                                    spawn_local(async move { library.report(api::open_screenshot(&path).await); });
                                }>
                                    <img src=api::convert_file_src(&shot.path) alt=tr!("Foto de {}", when) loading="lazy" />
                                    <small>{when.clone()}</small>
                                </button>
                            }
                        }).collect_view()}
                    </div>
                }.into_any(),
            }}
        </section>
    }
}

#[component]
pub fn History(id: String) -> impl IntoView {
    let library = expect_context::<Library>();
    let open = RwSignal::new(false);
    let entries = LocalResource::new(move || {
        // Sessions and backups end by reloading the library.
        library.games.track();
        let id = id.clone();
        async move { api::game_trail(&id).await.unwrap_or_default() }
    });
    move || {
        let list = entries.get().unwrap_or_default();
        if list.is_empty() {
            return None;
        }
        let failure = list.iter().find(|e| !e.ok).cloned();
        let latest_failed = list.first().is_some_and(|e| !e.ok);
        let summary = match &failure {
            Some(f) if latest_failed => view! {
                <p class="problem">{tr!("Última falha · {} · {}", f.step, format::date_time(f.at))}<br/>{f.message.clone()}</p>
            }.into_any(),
            Some(f) => view! {
                <p class="muted">{tr!("Tudo certo desde a última falha ({}, {}).", f.step, format::ago(f.at))}</p>
            }.into_any(),
            None => view! { <p class="muted">{tr!("Nenhuma falha registrada.")}</p> }.into_any(),
        };
        let rows = list.into_iter().map(|e| view! {
            <li class:failed=!e.ok>
                <span class="when">{format::date_time(e.at)}</span>
                <b>{cerberus_shared::i18n::translate(&e.step)}</b>
                <span>{e.message}</span>
            </li>
        }).collect_view();
        Some(view! {
            <div class="card history">
                <div class="card-h">
                    <h3>{tr!("Histórico")}</h3>
                    <button type="button" class="link" on:click=move |_| open.update(|o| *o = !*o)>
                        {move || if open.get() { tr!("Esconder") } else { tr!("Ver tudo") }}
                    </button>
                </div>
                {summary}
                <Show when=move || open.get()>
                    <ul class="trail">{rows.clone()}</ul>
                </Show>
            </div>
        })
    }
}

/// For a recomp not installed by Cerberus: the player already has it somewhere.
#[component]
fn AlreadyInstalled(recomp_id: String) -> impl IntoView {
    let library = expect_context::<Library>();
    let rid = StoredValue::new(recomp_id);
    let on_click = move |_| {
        spawn_local(async move {
            if let Some(game) = library.report(api::adopt_recomp(&rid.get_value(), None).await).flatten() {
                library.games.refetch();
                library.recomps.refetch();
                library.toast(tr!("{} está na Biblioteca, de onde você instalou.", game.name));
                library.go(crate::Route::Game(game.id));
            }
        });
    };
    view! {
        <button type="button" class="btn big" title=tr!("Escolha o programa do recomp que você já tem") on:click=on_click>
            <Svg icon=Icon::Folder size=18 />{tr!("Já tenho instalado")}
        </button>
    }
}

/// A game added by hand that is a catalog recomp: linking it brings its
/// settings, saves, mods and art, and keeps its play time.
#[component]
fn LinkToCatalog(id: String) -> impl IntoView {
    let library = expect_context::<Library>();
    let id = StoredValue::new(id);
    let suggestions = LocalResource::new(move || async move { api::recomp_suggestions(&id.get_value()).await.unwrap_or_default() });
    move || {
        let list = suggestions.get().unwrap_or_default();
        if list.is_empty() {
            return None;
        }
        let buttons = list
            .into_iter()
            .map(|s| {
                let rid = StoredValue::new(s.recomp_id.clone());
                let on_click = move |_| {
                    spawn_local(async move {
                        if let Some(game) = library.report(api::link_to_recomp(&id.get_value(), &rid.get_value()).await) {
                            library.games.refetch();
                            library.recomps.refetch();
                            library.toast(tr!("Vinculado a {}.", game.name));
                        }
                    });
                };
                let note = if s.sure { tr!("Mesmo programa do catálogo") } else { tr!("Nome parecido") };
                view! {
                    <button type="button" class="row-btn" on:click=on_click>
                        <Svg icon=Icon::Check />
                        <span><b>{tr!("É o {}", s.name)}</b><small>{format!("{} · {} · {note}", s.game, s.console)}</small></span>
                    </button>
                }
            })
            .collect_view();
        Some(view! {
            <div class="card">
                <h3>{tr!("É um recomp do catálogo?")}</h3>
                <p class="muted small">{tr!("Vinculado, ele ganha ajustes, mods, backups de saves e a arte do catálogo. O tempo de jogo continua.")}</p>
                <div class="manage">{buttons}</div>
            </div>
        })
    }
}

/// "2017-03-18" -> "18/03/2017" ("03/18/2017" in English); anything else as it is.
fn day(text: &str) -> String {
    let parts: Vec<&str> = text.get(..10).unwrap_or(text).split('-').collect();
    match parts.as_slice() {
        [y, m, d] if y.len() == 4 && cerberus_shared::i18n::lang() == cerberus_shared::i18n::Lang::En => format!("{m}/{d}/{y}"),
        [y, m, d] if y.len() == 4 => format!("{d}/{m}/{y}"),
        _ => text.to_string(),
    }
}

/// What is known about the game the recomp was made from.
#[component]
fn OriginalGameCard(original: cerberus_shared::OriginalGame, game: String) -> impl IntoView {
    let join = |list: &[String], most: usize| list.iter().take(most).cloned().collect::<Vec<_>>().join(", ");
    let facts: Vec<(&'static str, String)> = [
        (tr!("Lançamento"), if original.released.is_empty() { String::new() } else { day(&original.released) }),
        (tr!("Desenvolvedora"), original.developer.clone()),
        (tr!("Publicadora"), original.publisher.clone()),
        (tr!("Série"), original.series.clone()),
        (tr!("Gêneros"), join(&original.genres, 4)),
        ("Modos", join(&original.modes, 4)),
        (tr!("Lançado para"), join(&original.platforms, 5)),
        (tr!("Nota da crítica"), original.rating.map(|r| format!("{:.0}/100", r)).unwrap_or_default()),
    ]
    .into_iter()
    .filter(|(_, v)| !v.trim().is_empty())
    .collect();
    let rows = facts.into_iter().map(|(k, v)| view! { <div><dt>{k}</dt><dd>{v}</dd></div> }).collect_view();
    // The Wikipedia summary in the player's language, when there is one; IGDB's English text otherwise.
    let library = expect_context::<Library>();
    let wiki = LocalResource::new({
        let game = game.clone();
        move || api::wiki_about(game.clone())
    });
    let igdb = original.summary.clone();
    let summary = move || match wiki.get().and_then(Result::ok).flatten() {
        Some(w) => {
            let url = w.url.clone();
            view! {
                <p class="og-summary">{w.extract.clone()}</p>
                <button type="button" class="link og-wiki" on:click=move |_| {
                    let url = url.clone();
                    spawn_local(async move { library.report(api::open_wiki(url).await); });
                }><Svg icon=Icon::External size=14 />{tr!("Ler na Wikipedia")}</button>
            }
            .into_any()
        }
        None => (!igdb.is_empty()).then(|| view! { <p class="og-summary">{igdb.clone()}</p> }).into_any(),
    };
    view! {
        <section class="card original">
            <h3>{tr!("Sobre {}", game)}</h3>
            {summary}
            <dl class="og-facts">{rows}</dl>
            <p class="muted small">{tr!("Dados do jogo original pelo IGDB e pela Wikipedia.")}</p>
        </section>
    }
}

/// Other recomps someone who likes this one might like: shared tags count
/// twice, then the same series, genre and console. Other versions of the same
/// game are left out.
pub fn similar(entry: &RecompEntry, all: &[RecompEntry], most: usize) -> Vec<RecompEntry> {
    let series = entry.original.as_ref().map(|o| o.series.clone()).filter(|s| !s.is_empty());
    let mut scored: Vec<(f32, &RecompEntry)> = crate::versions::one_each(all)
        .into_iter()
        .map(|(lead, _)| lead)
        .filter(|x| x.title != entry.title && x.game != entry.game && x.latest.is_some())
        .filter_map(|x| {
            let shared = x.tags.iter().filter(|t| entry.tags.contains(t)).count() as f32;
            let mut score = shared * 2.0;
            if !entry.genre.is_empty() && x.genre == entry.genre {
                score += 3.0;
            }
            if x.origin == entry.origin {
                score += 1.5;
            }
            if series.is_some() && x.original.as_ref().map(|o| &o.series) == series.as_ref() {
                score += 4.0;
            }
            (score >= 3.0).then_some((score, all.iter().find(|e| e.id == x.id)?))
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(b.1.downloads.cmp(&a.1.downloads)));
    scored.into_iter().take(most).map(|(_, e)| e.clone()).collect()
}

#[component]
fn SimilarGames(entry: RecompEntry) -> impl IntoView {
    let library = expect_context::<Library>();
    move || {
        let all = library.recomps.get().and_then(Result::ok).unwrap_or_default();
        let list = similar(&entry, &all, 12);
        (!list.is_empty()).then(|| {
            let tiles = list.iter().map(crate::tiles::recomp_tile).collect::<Vec<_>>();
            view! { <crate::tiles::Shelf title=tr!("Jogos parecidos").to_string() tiles /> }
        })
    }
}
