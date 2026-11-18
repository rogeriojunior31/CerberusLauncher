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
    Manage,
}

#[component]
pub fn GamePage(id: String) -> impl IntoView {
    let library = expect_context::<Library>();
    let id = StoredValue::new(id);
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
        view! { <Page game entry /> }.into_any()
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
fn Page(game: Option<Game>, entry: Option<RecompEntry>) -> impl IntoView {
    let tab = RwSignal::new(Tab::Overview);

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
    let has_saves = matches!(game.as_ref().map(|g| &g.source), Some(Source::Recomp { .. }));
    let manageable = game.as_ref().is_some_and(|g| matches!(g.source, Source::Local { .. } | Source::Recomp { .. })) || entry.is_some();

    let action = primary_action(game.clone(), entry.clone());
    let tab_button = move |which: Tab, label: &'static str| {
        view! {
            <button type="button" class="tab" class:on=move || tab.get() == which on:click=move |_| tab.set(which)>{label}</button>
        }
    };
    let tabs = view! {
        <nav class="tabs">
            {tab_button(Tab::Overview, "Visão geral")}
            {has_saves.then(|| tab_button(Tab::Saves, "Saves"))}
            {has_saves.then(|| tab_button(Tab::Mods, "Mods"))}
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
                    <h1>{title.clone()}</h1>
                    {based}
                    {pill}
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
            let update = entry.as_ref().and_then(|e| {
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
            return view! {
                {update_button}
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
        if g.is_tracked() {
            let played = if g.last_played.is_none() && g.playtime_secs == 0 { "Nunca jogado".to_string() } else { format::playtime(g.playtime_secs) };
            stats.push(("Tempo de jogo", played));
            if let Some(seconds) = g.last_session_secs {
                stats.push(("Última sessão", format::session_duration(seconds)));
            }
            stats.push(("Última vez", g.last_played.map(format::ago).unwrap_or_else(|| "—".to_string())));
        }
        if let Source::Recomp { version, .. } = &g.source {
            stats.push(("Versão", version.clone()));
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
            stats.push(("Estrelas no GitHub", format::count(e.stars)));
        }
    }
    let cards = stats
        .into_iter()
        .map(|(label, value)| view! { <div class="stat"><span>{label}</span><b>{value}</b></div> })
        .collect_view();
    let untracked = game.as_ref().is_some_and(|g| !g.is_tracked()).then(|| {
        view! { <p class="muted small">"Tempo de jogo só é contado para jogos que o Cerberus abre sozinho."</p> }
    });

    let about = entry.as_ref().map(|e| e.about.clone()).filter(|a| !a.is_empty()).map(|text| view! { <p class="about">{text}</p> });
    let problem = entry.as_ref().and_then(|e| e.problem.clone()).map(|text| view! { <p class="problem">{text}</p> });
    let tags = entry.as_ref().filter(|e| !e.tags.is_empty()).map(|e| {
        let chips = e.tags.iter().map(|tag| view! { <span class="tag">{tag.clone()}</span> }).collect_view();
        view! { <div class="tags">{chips}</div> }
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
                {tags}
                {problem}
                {setup}
                {features}
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

    let accepts_folder = entry.accepts_folder;
    let choose_from = move |use_found: bool, folder: bool| {
        busy.set(true);
        spawn_local(async move {
            let set = library.report(api::set_game_copy(&id.get_value(), use_found, folder).await).flatten();
            busy.set(false);
            if let Some(game) = set {
                current.set(game.game_copy.clone());
                library.games.refetch();
                library.toast("Pronto: o jogo vai abrir com a sua cópia.");
            }
        });
    };
    let choose = move |use_found: bool| choose_from(use_found, false);
    let folder_button = move || {
        accepts_folder.then(|| {
            view! { <button type="button" class="btn sm" disabled=move || busy.get() on:click=move |_| choose_from(false, true)>"Escolher pasta..."</button> }
        })
    };

    let found = entry.owned_copy.clone();
    let what = if entry.needs.is_empty() { format!("Sua cópia de {}.", entry.game) } else { entry.needs.clone() };
    let body = move || match (current.get(), found.clone()) {
        (Some(path), _) => view! {
            <div class="need found">
                <Svg icon=Icon::Check size=18 />
                <div><b>"Cópia selecionada"</b><span class="path">{path}</span></div>
            </div>
            <button type="button" class="btn sm" disabled=move || busy.get() on:click=move |_| choose(false)>"Trocar arquivo..."</button>
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
                <button type="button" class="btn sm main" disabled=move || busy.get() on:click=move |_| choose(false)>"Escolher arquivo..."</button>
                {folder_button}
            </div>
        }
        .into_any(),
    };

    view! {
        <aside class="card ready">
            <h3>"Easy Setup"</h3>
            {body}
            <p class="muted small">"O Cerberus entrega sua cópia ao jogo do jeito que ele espera. Nada sai do seu PC."</p>
        </aside>
    }
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
        None => view! { <p class="muted">"Carregando os mods do Thunderstore..."</p> }.into_any(),
        Some(Err(error)) => view! { <p class="problem">{error}</p> }.into_any(),
        Some(Ok(None)) => view! { <p class="muted">"Esse jogo não tem mods no Thunderstore."</p> }.into_any(),
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
                                <span class="muted small">{format!("por {} · v{} · {} downloads", m.owner, m.version, format::count(m.downloads))}</span>
                                <span class="small">{m.description.clone()}</span>
                            </div>
                            <div class="row">{action}</div>
                        </li>
                    }
                })
                .collect_view();
            view! {
                <p class="muted small">{format!("Da comunidade thunderstore.io/c/{} · instalados em {}", mods.community, mods.folder)}</p>
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
        let on_project = move |_| {
            let rid = rid.clone();
            spawn_local(async move {
                library.report(api::open_project(&rid).await);
            });
        };
        view! {
            <button type="button" class="row-btn" on:click=on_project>
                <Svg icon=Icon::External />
                <span><b>"Projeto no GitHub"</b><small>{repo}</small></span>
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
        let (title, detail) = if is_recomp {
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
            </div>
        }),
        _ => None,
    });
    view! { <div class="manage">{program}{options}{folder}{edit}{project}{remove}</div> }
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
