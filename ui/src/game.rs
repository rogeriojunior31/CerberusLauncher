//! A game's page: the art across the top, the main action, and tabs.
//!
//! The same page serves a game in the library and a catalog recomp not yet
//! installed (`recomp:<id>`), so Discover and the library lead to one place.

use cerberus_shared::{CopyHandoff, Game, RecompEntry, Source};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::art::{Backdrop, Poster, cover_sources, hero_sources, is_low_res, recomp_covers, recomp_heroes};
use crate::backups::Backups;
use crate::discover::{progress_view, start_install};
use crate::format::{self, megabytes};
use crate::icons::{Icon, Svg};
use crate::tiles::source_name;
use crate::versions::{VersionPill, versions_of};
use crate::{Library, api, play_game};

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Overview,
    Saves,
    Mods,
    Photos,
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
            let text = if loading { "Carregando..." } else { "Esse jogo não está mais na biblioteca." };
            return view! { <div class="page"><p class="muted">{text}</p></div> }.into_any();
        }
        view! { <Page game entry tab /> }.into_any()
    };
    view! {
        <div class="gp">
            <button type="button" class="btn glass back" on:click=move |_| library.back()>
                <Svg icon=Icon::Back size=18 />"Voltar"
            </button>
            {content}
        </div>
    }
}

#[component]
fn Page(game: Option<Game>, entry: Option<RecompEntry>, tab: RwSignal<Tab>) -> impl IntoView {
    let title = game.as_ref().map(|g| g.name.clone()).or_else(|| entry.as_ref().map(|e| e.name.clone())).unwrap_or_default();
    let covers = game.as_ref().map(cover_sources).filter(|c| !c.is_empty()).or_else(|| entry.as_ref().map(recomp_covers)).unwrap_or_default();
    let heroes = game.as_ref().map(hero_sources).filter(|h| !h.is_empty()).or_else(|| entry.as_ref().map(recomp_heroes)).unwrap_or_default();
    let kicker = match (&game, &entry) {
        (_, Some(e)) => format!("Recomp · {}", e.console),
        (Some(g), None) => source_name(&g.source).to_string(),
        (None, None) => String::new(),
    };
    let based = entry.as_ref().map(|e| view! { <span class="based">"Baseado em " <b>{e.game.clone()}</b></span> });
    let library = expect_context::<Library>();
    let siblings = entry.as_ref().map_or(0, |e| {
        library.recomps.with_untracked(|list| match list {
            Some(Ok(all)) => versions_of(all, &e.title).len(),
            _ => 0,
        })
    });
    let pill = entry.clone().filter(|_| siblings > 1).map(|current| view! { <VersionPill current /> });
    let heart = game.as_ref().map(|g| {
        let (id, on) = (g.id.clone(), g.favorite);
        let label = if on { "Tirar dos favoritos" } else { "Adicionar aos favoritos" };
        view! {
            <button type="button" class="btn glass fav-btn" class:on=on title=label aria-label=label aria-pressed=on.to_string()
                on:click=move |_| crate::toggle_favorite(library, id.clone(), !on)>
                <Svg icon=if on { Icon::HeartFill } else { Icon::Heart } size=18 />
                {if on { "Favorito" } else { "Favoritar" }}
            </button>
        }
    });
    let has_saves = matches!(game.as_ref().map(|g| &g.source), Some(Source::Recomp { .. }));
    // Every game Cerberus starts gets the tab: picture filters, and the catalog's settings when it has some.
    let has_settings = game.as_ref().is_some_and(|g| g.is_tracked());
    let manageable = game.is_some() || entry.is_some();
    // F12 takes pictures of the games Cerberus opens itself.
    let photos = matches!(game.as_ref().map(|g| &g.source), Some(Source::Recomp { .. } | Source::Local { .. }));

    if matches!(tab.get_untracked(), Tab::Saves | Tab::Mods) && !has_saves
        || tab.get_untracked() == Tab::Settings && !has_settings
        || tab.get_untracked() == Tab::Manage && !manageable
    {
        tab.set(Tab::Overview);
    }
    let action = primary_action(game.clone(), entry.clone());
    let tab_button = move |which: Tab, label: &'static str| {
        let icon = match which {
            Tab::Overview => Icon::Info,
            Tab::Saves => Icon::Saves,
            Tab::Settings => Icon::Settings,
            Tab::Mods => Icon::Puzzle,
            Tab::Photos => Icon::Photo,
            Tab::Manage => Icon::Edit,
        };
        view! {
            <button type="button" class="tab with-icon" class:on=move || tab.get() == which on:click=move |_| tab.set(which)>
                <Svg icon size=16 />{label}
            </button>
        }
    };
    let tabs = view! {
        <nav class="tabs">
            {tab_button(Tab::Overview, "Visão geral")}
            {has_saves.then(|| tab_button(Tab::Saves, "Saves"))}
            {has_settings.then(|| tab_button(Tab::Settings, "Ajustes"))}
            {has_saves.then(|| tab_button(Tab::Mods, "Mods"))}
            {photos.then(|| tab_button(Tab::Photos, "Fotos"))}
            {manageable.then(|| tab_button(Tab::Manage, "Gerenciar"))}
        </nav>
    };

    let body_game = game.clone();
    let body_entry = entry.clone();
    let body = move || match tab.get() {
        Tab::Overview => view! { <Overview game=body_game.clone() entry=body_entry.clone() /> }.into_any(),
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

    view! {
        <div class="gp-hero">
            <Backdrop sources=heroes soft=entry.as_ref().and_then(|e| e.hero.as_deref()).is_some_and(is_low_res) />
            <div class="gp-shade"></div>
        </div>
        <div class="page gp-page">
            <section class="gp-head">
                <div class="gp-cover"><Poster sources=covers title=title.clone() /></div>
                <div class="gp-info">
                    <span class="kick">{kicker}</span>
                    {match game.as_ref().and_then(|g| g.logo_path.clone()).or_else(|| entry.as_ref().and_then(|e| e.logo.clone())) {
                        Some(logo) => view! { <img class="gp-logo" src=crate::art::src(&logo) alt=title.clone() /> }.into_any(),
                        None => view! { <h1>{title.clone()}</h1> }.into_any(),
                    }}
                    {based}
                    <div class="gp-tags">{pill}{heart}</div>
                </div>
                <div class="gp-act">{action}</div>
            </section>
            {tabs}
            {body}
        </div>
    }
}

/// Play, Playing, Install with its size, the install's progress, or Update.
fn primary_action(game: Option<Game>, entry: Option<RecompEntry>) -> impl IntoView {
    let library = expect_context::<Library>();
    let recomp_id = entry.as_ref().map(|e| e.id.clone());
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
                        {format!("Atualizar para {version}")}
                    </button>
                }
            });
            let (label, icon) = if running { ("Jogando", Icon::Play) } else { crate::launch_action(&game) };
            // The copy the recomp was given, else the one found in the ROM folders.
            let original = game
                .game_copy
                .clone()
                .or_else(|| entry.as_ref().and_then(|e| e.owned_copy.clone()))
                .filter(|_| matches!(game.source, Source::Recomp { .. }))
                .and_then(|copy| crate::classics::emulated_console(&copy).map(|c| (copy, c.to_string())))
                .map(|(copy, console)| view! {
                    <button type="button" class="btn big" title="Abrir a sua cópia original no RetroArch"
                        on:click=move |_| crate::classics::play_original(library, copy.clone(), console.clone())>
                        "Original no RetroArch"
                    </button>
                });
            return view! {
                {update_button}
                {original}
                <button type="button" class="btn main big play" disabled=running on:click=move |_| play_game(library, game.clone())>
                    <Svg icon size=20 />{label}
                </button>
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
                        <Svg icon=Icon::Download size=20 />{format!("Instalar · {}", megabytes(release.size))}
                    </button>
                }
                .into_any()
            }
            None => view! { <button type="button" class="btn big" disabled=true>"Indisponível"</button> }.into_any(),
        }
    }
}

#[component]
fn Overview(game: Option<Game>, entry: Option<RecompEntry>) -> impl IntoView {
    let mut stats: Vec<(&'static str, String)> = Vec::new();
    if let Some(g) = &game {
        // Steam and Epic games count too, once Cerberus has seen them played.
        if g.is_tracked() || g.last_played.is_some() {
            let played = if g.last_played.is_none() && g.playtime_secs == 0 { "Nunca jogado".to_string() } else { format::playtime(g.playtime_secs) };
            stats.push(("Tempo de jogo", played));
            if let Some(seconds) = g.last_session_secs {
                stats.push(("Última sessão", format::session_duration(seconds)));
            }
            stats.push(("Última vez", g.last_played.map(format::ago).unwrap_or_else(|| "—".to_string())));
        }
        if let Source::Recomp { version, .. } = &g.source {
            let shown = if g.external && version.is_empty() { "Instalado por você".to_string() } else { version.clone() };
            stats.push(("Versão", shown));
        }
        stats.push(("Fonte", source_name(&g.source).to_string()));
    } else if let Some(e) = &entry {
        if let Some(release) = &e.latest {
            stats.push(("Última versão", release.version.clone()));
            if release.size > 0 {
                stats.push(("Tamanho", megabytes(release.size)));
            }
        }
        stats.push(("Console", e.console.clone()));
    }
    if let Some(e) = &entry {
        if e.downloads > 0 {
            stats.push(("Downloads", format::count(e.downloads)));
        }
        if e.stars > 0 {
            stats.push(("Estrelas", format::count(e.stars)));
        }
        if e.players > 0 {
            stats.push(("Jogadores recentes", format::count(e.players)));
        }
        if e.releases > 0 {
            stats.push(("Versões lançadas", e.releases.to_string()));
        }
        if !e.first_release.is_empty() {
            stats.push(("Primeira versão", day(&e.first_release)));
        }
    }
    let cards = stats
        .into_iter()
        .map(|(label, value)| view! { <div class="stat"><span>{label}</span><b>{value}</b></div> })
        .collect_view();
    let untracked = game.as_ref().is_some_and(|g| !g.is_tracked()).then(|| {
        view! { <p class="muted small">"O tempo dos jogos da Steam e da Epic é contado enquanto o Cerberus está aberto, mesmo escondido na bandeja."</p> }
    });

    let about = entry.as_ref().map(|e| e.about.clone()).filter(|a| !a.is_empty()).map(|text| view! { <p class="about">{text}</p> });
    let problem = entry.as_ref().and_then(|e| e.problem.clone()).map(|text| view! { <p class="problem">{text}</p> });
    let tags = entry.as_ref().filter(|e| !e.tags.is_empty() || !e.multiplayer.is_empty()).map(|e| {
        let players = match e.multiplayer.as_str() {
            "local" => Some("Multijogador local"),
            "online" => Some("Multijogador online"),
            "both" => Some("Multijogador local e online"),
            _ => None,
        };
        let players = players.map(|p| view! { <span class="tag players">{p}</span> });
        let chips = e.tags.iter().map(|tag| view! { <span class="tag">{tag.clone()}</span> }).collect_view();
        view! { <div class="tags">{players}{chips}</div> }
    });
    let note = entry.as_ref().map(|e| e.player_note.clone()).filter(|n| !n.is_empty()).map(|text| {
        view! {
            <div class="note">
                <Svg icon=Icon::Check size=18 />
                <div><b>"Como começar"</b><p>{text}</p></div>
            </div>
        }
    });
    let features = entry.as_ref().filter(|e| !e.features.is_empty()).map(|e| {
        let items = e.features.iter().map(|f| view! { <li><Svg icon=Icon::Check size=16 />{f.clone()}</li> }).collect_view();
        view! { <ul class="features">{items}</ul> }
    });
    let setup = entry.as_ref().filter(|e| !e.setup_steps.is_empty()).map(|e| {
        let steps = e.setup_steps.iter().map(|s| view! { <li>{s.clone()}</li> }).collect_view();
        view! {
            <section class="card setup">
                <h3>"Precisa de configuração"</h3>
                <p class="muted small">"Esse recomp precisa de alguns passos feitos à mão antes de jogar:"</p>
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
                        <b>"Sua cópia foi encontrada"</b>
                        <span class="path">{path.clone()}</span>
                    </div>
                </div>
            }
            .into_any(),
            None => view! {
                <div class="need">
                    <Svg icon=Icon::Check size=18 />
                    <div>
                        <b>"Sua cópia do jogo"</b>
                        <span>{if e.needs.is_empty() { format!("Sua cópia de {}.", e.game) } else { e.needs.clone() }}</span>
                    </div>
                </div>
            }
            .into_any(),
        };
        view! {
            <aside class="card ready">
                <h3>"Antes de jogar"</h3>
                {copy}
                <p class="muted small">"O Cerberus nunca baixa arquivos de jogo. Aponte suas pastas de ROMs em Configurações e ele avisa quais jogos você já tem."</p>
            </aside>
        }
    });

    view! {
        <div class="ov">
            <div class="ov-main">
                <div class="stats">{cards}</div>
                {untracked}
                {about}
                {note}
                {tags}
                {problem}
                {setup}
                {features}
                {entry.clone().and_then(|e| e.original.clone().map(|o| view! { <OriginalGameCard original=o game=e.game.clone() /> }))}
                {entry.clone().map(|e| view! { <SimilarGames entry=e /> })}
            </div>
            {setup_card}
            {needs}
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
                library.toast("Pronto: o jogo vai abrir com a sua cópia.");
            }
        });
    };
    // A recomp that takes only extracted folders never gets a file picker.
    let choose = move |use_found: bool| choose_from(use_found, only_folder && !use_found);
    let pick = if only_folder { "Escolher pasta..." } else { "Escolher arquivo..." };
    let change = if only_folder { "Trocar pasta..." } else { "Trocar arquivo..." };
    let folder_button = move || {
        (accepts_folder && !only_folder).then(|| {
            view! { <button type="button" class="btn sm" disabled=move || busy.get() on:click=move |_| choose_from(false, true)>"Escolher pasta..."</button> }
        })
    };

    let found = entry.owned_copy.clone();
    let what = if entry.needs.is_empty() { format!("Sua cópia de {}.", entry.game) } else { entry.needs.clone() };
    let warning = move || lost.get().then(|| view! {
        <p class="problem small">"A cópia escolhida não está mais onde o jogo procura. Escolha de novo."</p>
    });
    let body = move || match (current.get().filter(|_| !lost.get()), found.clone()) {
        (Some(path), _) => view! {
            <div class="need found">
                <Svg icon=Icon::Check size=18 />
                <div><b>"Cópia selecionada"</b><span class="path">{path}</span></div>
            </div>
            <button type="button" class="btn sm" disabled=move || busy.get() on:click=move |_| choose(false)>{change}</button>
        }
        .into_any(),
        (None, Some(path)) => view! {
            <div class="need">
                <Svg icon=Icon::Check size=18 />
                <div><b>"Encontramos sua cópia"</b><span class="path">{path}</span></div>
            </div>
            <div class="row">
                <button type="button" class="btn sm main" disabled=move || busy.get() on:click=move |_| choose(true)>"Usar esta cópia"</button>
                <button type="button" class="btn sm" disabled=move || busy.get() on:click=move |_| choose(false)>"Escolher outra..."</button>
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
            <h3>"Easy Setup"</h3>
            {warning}
            {body}
            <p class="muted small">"O Cerberus entrega sua cópia ao jogo do jeito que ele espera. Nada sai do seu PC."</p>
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
                library.toast(if install { "Mod instalado. Ele vale na próxima vez que o jogo abrir." } else { "Mod removido." });
            }
            refresh.update(|n| *n += 1);
        });
    };

    let rows = move || match list.get() {
        None => view! { <p class="muted">"Carregando os mods..."</p> }.into_any(),
        Some(Err(error)) => view! { <p class="problem">{error}</p> }.into_any(),
        Some(Ok(None)) => view! { <p class="muted">"Esse jogo não tem mods no Thunderstore nem no GameBanana."</p> }.into_any(),
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
                            <button type="button" class="btn sm" disabled=working on:click=move |_| act(full_remove.clone(), false)>"Remover"</button>
                        }
                        .into_any(),
                        Some(_) => view! {
                            <button type="button" class="btn sm main" disabled=working.clone() on:click=move |_| act(full.clone(), true)>"Atualizar"</button>
                            <button type="button" class="btn sm" disabled=working on:click=move |_| act(full_remove.clone(), false)>"Remover"</button>
                        }
                        .into_any(),
                        None => view! {
                            <button type="button" class="btn sm main" disabled=working on:click=move |_| act(full.clone(), true)>"Instalar"</button>
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
                <p class="muted small">{format!("De {} · instalados em {}", mods.from, mods.folder)}</p>
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
                    <input type="search" placeholder="Buscar mods" prop:value=move || search.get() on:input=move |e| search.set(event_target_value(&e)) />
                </label>
            </div>
            {rows}
        </section>
    }
}

#[component]
fn Manage(game: Option<Game>, entry: Option<RecompEntry>) -> impl IntoView {
    let library = expect_context::<Library>();
    let armed = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let is_recomp = matches!(game.as_ref().map(|g| &g.source), Some(Source::Recomp { .. }));
    let editable = game.as_ref().is_some_and(|g| matches!(g.source, Source::Local { .. } | Source::Recomp { .. }));

    let folder = game.as_ref().map(|g| {
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
                <span><b>"Abrir pasta do jogo"</b><small>{g.install_dir.clone()}</small></span>
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
                    library.toast(format!("{} foi movido para {}.", game.name, game.install_dir));
                }
            });
        };
        view! {
            <button type="button" class="row-btn" on:click=on_move disabled=move || moving.get()>
                <Svg icon=Icon::Folder />
                <span>
                    <b>{move || if moving.get() { "Movendo… pode levar alguns minutos" } else { "Mover para outra pasta ou disco" }}</b>
                    <small>"Leva a pasta inteira, com saves, mods e configurações, e o Cerberus passa a abrir de lá"</small>
                </span>
            </button>
        }
    });
    let shortcut = game.as_ref().map(|g| {
        let id = g.id.clone();
        let on_shortcut = move |_| {
            let id = id.clone();
            spawn_local(async move {
                if library.report(api::create_shortcut(&id).await).is_some() {
                    library.toast("Atalho criado na área de trabalho.");
                }
            });
        };
        view! {
            <button type="button" class="row-btn" on:click=on_shortcut>
                <Svg icon=Icon::External />
                <span><b>"Criar atalho na área de trabalho"</b><small>"Abre o jogo pelo Cerberus: tempo de jogo, backups e filtros continuam valendo"</small></span>
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
                        library.toast("Tirado da Steam.");
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
                    <b>{move || if added.get() { "Tirar da Steam" } else { "Adicionar à Steam" }}</b>
                    <small>{move || if added.get() {
                        "Está na biblioteca da Steam como jogo de fora; tirar não apaga nada do jogo"
                    } else {
                        "Aparece na biblioteca da Steam, com a capa, e abre pelo Cerberus (Big Picture e Steam Deck). Feche a Steam antes"
                    }}</small>
                </span>
            </button>
        }
    });
    let customize = game.clone().map(|g| {
        view! {
            <button type="button" class="row-btn" on:click=move |_| library.customizing.set(Some(g.clone()))>
                <Svg icon=Icon::Edit />
                <span><b>"Personalizar arte"</b><small>"Capa, fundo e logo do SteamGridDB ou de um arquivo seu, com recorte e zoom"</small></span>
            </button>
        }
    });
    let edit = game.clone().filter(|_| editable).map(|g| {
        view! {
            <button type="button" class="row-btn" on:click=move |_| library.editing.set(Some(g.clone()))>
                <Svg icon=Icon::Edit />
                <span><b>"Editar nome e capa"</b><small>"Renomear, buscar outra capa ou escolher uma imagem sua"</small></span>
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
                <span><b>{format!("Projeto no {host}")}</b><small>{repo}</small></span>
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
            "Baixa o jogo de novo por cima; saves, configurações e mods ficam".to_string()
        } else {
            format!("Baixa a versão {version} de novo por cima; saves, configurações e mods ficam")
        };
        view! {
            <button type="button" class="row-btn" disabled=downloading on:click=move |_| crate::discover::start_install(library, rid.clone())>
                <Svg icon=Icon::Download />
                <span><b>"Reparar instalação"</b><small>{detail}</small></span>
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
            ("Remover da biblioteca", "A pasta é sua e continua no PC. Os backups de saves continuam guardados.")
        } else if is_recomp {
            ("Desinstalar", "Apaga a pasta do jogo. Os backups de saves continuam guardados.")
        } else {
            ("Remover da biblioteca", "Os arquivos do jogo continuam no seu PC.")
        };
        let label = move || if armed.get() { "Clique de novo para confirmar" } else { title };
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
                <h3>{if g.setup_pending { "Instalador de primeira execução" } else { "Programa de abertura" }}</h3>
                <p class="path">{exe.clone()}</p>
                {(!g.setup_pending).then(|| view! { <TrackingProgram id=g.id.clone() selected=g.tracking_exe.is_some() /> })}
            </div>
        }),
        _ => None,
    });
    let history = game.as_ref().filter(|g| g.is_tracked()).map(|g| view! { <History id=g.id.clone() /> });
    let link = game.as_ref().filter(|g| matches!(g.source, Source::Local { .. })).map(|g| view! { <LinkToCatalog id=g.id.clone() /> });
    let outside = game.as_ref().filter(|g| g.external).map(|_| view! {
        <div class="card">
            <h3>"Instalado fora do Cerberus"</h3>
            <p class="muted small">"Você trouxe esse recomp de uma pasta sua. O Cerberus abre, conta o tempo e faz backup dos saves, mas não atualiza nem apaga os arquivos dele."</p>
        </div>
    });
    view! { <div class="manage">{outside}{link}{program}{options}{history}{folder}{relocate}{shortcut}{steam}{customize}{edit}{project}{repair}{remove}</div> }
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
                library.toast("A medição escolhida vale a partir da próxima abertura.");
            }
            busy.set(false);
        });
    };
    move || mode.get().and_then(Result::ok).flatten().map(|label| view! {
        <h3>"Processo acompanhado"</h3>
        <p class="path">{label}</p>
        <div class="row">
            <button class="btn sm" disabled=move || busy.get() on:click=move |_| choose(false)>"Escolher executável"</button>
            {selected.then(|| view! { <button class="btn sm" disabled=move || busy.get() on:click=move |_| choose(true)>"Usar automático"</button> })}
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
                library.toast("Opções de inicialização salvas.");
            }
        });
    };
    view! {
        <form class="card launch" on:submit=on_save>
            <h3>"Opções de inicialização"</h3>
            <p class="muted small">"Passadas ao jogo quando ele abre, depois das do Cerberus. Use aspas para valores com espaço."</p>
            <div class="row">
                <input class="input grow" placeholder="ex.: --fullscreen --fps 120" prop:value=move || text.get() on:input=move |e| text.set(event_target_value(&e)) />
                <button type="submit" class="btn main">"Salvar"</button>
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
                <h3>"Fotos"</h3>
                <button type="button" class="btn sm" on:click=open_folder><Svg icon=Icon::Folder size=16 />"Abrir pasta"</button>
            </div>
            <p class="muted small">"Com o jogo aberto pelo Cerberus e na frente, aperte F12: a foto é só da janela do jogo e aparece aqui. Jogos em tela cheia exclusiva podem sair pretos; use janela sem bordas."</p>
            {move || match shots.get() {
                None => view! { <p class="muted">"Carregando..."</p> }.into_any(),
                Some(Err(e)) => view! { <p class="problem">{e}</p> }.into_any(),
                Some(Ok(list)) if list.is_empty() => view! {
                    <div class="empty small">
                        <Svg icon=Icon::Photo size=28 />
                        <p class="muted">"Nenhuma foto ainda."</p>
                    </div>
                }.into_any(),
                Some(Ok(list)) => view! {
                    <div class="shots">
                        {list.into_iter().map(|shot| {
                            let path = shot.path.clone();
                            let when = format::ago(shot.taken);
                            view! {
                                <button type="button" class="shot" title=format!("Abrir · {when}") on:click=move |_| {
                                    let path = path.clone();
                                    spawn_local(async move { library.report(api::open_screenshot(&path).await); });
                                }>
                                    <img src=api::convert_file_src(&shot.path) alt=format!("Foto de {when}") loading="lazy" />
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
                <p class="problem">{format!("Última falha · {} · {}", f.step, format::date_time(f.at))}<br/>{f.message.clone()}</p>
            }.into_any(),
            Some(f) => view! {
                <p class="muted">{format!("Tudo certo desde a última falha ({}, {}).", f.step, format::ago(f.at))}</p>
            }.into_any(),
            None => view! { <p class="muted">"Nenhuma falha registrada."</p> }.into_any(),
        };
        let rows = list.into_iter().map(|e| view! {
            <li class:failed=!e.ok>
                <span class="when">{format::date_time(e.at)}</span>
                <b>{e.step}</b>
                <span>{e.message}</span>
            </li>
        }).collect_view();
        Some(view! {
            <div class="card history">
                <div class="card-h">
                    <h3>"Histórico"</h3>
                    <button type="button" class="link" on:click=move |_| open.update(|o| *o = !*o)>
                        {move || if open.get() { "Esconder" } else { "Ver tudo" }}
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
                library.toast(format!("{} está na Biblioteca, de onde você instalou.", game.name));
                library.go(crate::Route::Game(game.id));
            }
        });
    };
    view! {
        <button type="button" class="btn big" title="Escolha o programa do recomp que você já tem" on:click=on_click>
            <Svg icon=Icon::Folder size=18 />"Já tenho instalado"
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
                            library.toast(format!("Vinculado a {}.", game.name));
                        }
                    });
                };
                let note = if s.sure { "Mesmo programa do catálogo" } else { "Nome parecido" };
                view! {
                    <button type="button" class="row-btn" on:click=on_click>
                        <Svg icon=Icon::Check />
                        <span><b>{format!("É o {}", s.name)}</b><small>{format!("{} · {} · {note}", s.game, s.console)}</small></span>
                    </button>
                }
            })
            .collect_view();
        Some(view! {
            <div class="card">
                <h3>"É um recomp do catálogo?"</h3>
                <p class="muted small">"Vinculado, ele ganha ajustes, mods, backups de saves e a arte do catálogo. O tempo de jogo continua."</p>
                <div class="manage">{buttons}</div>
            </div>
        })
    }
}

/// "2017-03-18" -> "18/03/2017"; anything else as it is.
fn day(text: &str) -> String {
    let parts: Vec<&str> = text.get(..10).unwrap_or(text).split('-').collect();
    match parts.as_slice() {
        [y, m, d] if y.len() == 4 => format!("{d}/{m}/{y}"),
        _ => text.to_string(),
    }
}

/// What is known about the game the recomp was made from.
#[component]
fn OriginalGameCard(original: cerberus_shared::OriginalGame, game: String) -> impl IntoView {
    let join = |list: &[String], most: usize| list.iter().take(most).cloned().collect::<Vec<_>>().join(", ");
    let facts: Vec<(&'static str, String)> = [
        ("Lançamento", if original.released.is_empty() { String::new() } else { day(&original.released) }),
        ("Desenvolvedora", original.developer.clone()),
        ("Publicadora", original.publisher.clone()),
        ("Série", original.series.clone()),
        ("Gêneros", join(&original.genres, 4)),
        ("Modos", join(&original.modes, 4)),
        ("Lançado para", join(&original.platforms, 5)),
        ("Nota da crítica", original.rating.map(|r| format!("{:.0}/100", r)).unwrap_or_default()),
    ]
    .into_iter()
    .filter(|(_, v)| !v.trim().is_empty())
    .collect();
    let rows = facts.into_iter().map(|(k, v)| view! { <div><dt>{k}</dt><dd>{v}</dd></div> }).collect_view();
    let summary = (!original.summary.is_empty()).then(|| view! { <p class="og-summary">{original.summary.clone()}</p> });
    view! {
        <section class="card original">
            <h3>{format!("Sobre {game}")}</h3>
            {summary}
            <dl class="og-facts">{rows}</dl>
            <p class="muted small">"Dados do jogo original pelo IGDB, em inglês."</p>
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
            view! { <crate::tiles::Shelf title="Jogos parecidos".to_string() tiles /> }
        })
    }
}
