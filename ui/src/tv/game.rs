//! A game's page on the TV: its art across the screen, its logo or name, a row
//! of buttons led by the one thing to do now (play, install, update, the
//! download's progress), and the details beside it.

use cerberus_shared::tr;
use cerberus_shared::{Game, RecompEntry, Source};
use leptos::prelude::*;
use leptos::task::spawn_local;

use super::pages::{Item, progress_of};
use super::{Btn, Focus, Sheet, Sound, Tv, play};
use crate::art::Poster;
use crate::discover::{cancel_install, start_install};
use crate::{Library, api, format, play_game};

#[derive(Clone)]
enum Act {
    Play(Game),
    /// The player's own copy in its emulator: path, console, emulator name.
    PlayOriginal(String, String, String),
    Running,
    Install(String),
    Cancel(String),
    Pause(String),
    Resume(String),
    Update(String),
    Backup(String),
    Options,
    Unavailable,
}

fn label(act: &Act, library: Library, entry: &Option<RecompEntry>) -> String {
    match act {
        Act::Play(g) => crate::launch_action(g).0.to_string(),
        Act::PlayOriginal(_, _, name) => tr!("Jogar o original no {}", name),
        Act::Running => tr!("Voltar ao jogo").into(),
        Act::Install(_) => match entry.as_ref().and_then(|e| e.latest.clone()) {
            Some(r) if r.size > 0 => tr!("Instalar · {}", format::megabytes(r.size)),
            _ => tr!("Instalar").into(),
        },
        Act::Cancel(rid) if crate::discover::is_paused(library, rid) => tr!("Cancelar download").into(),
        Act::Cancel(rid) => tr!("Baixando {}% · cancelar", progress_of(library, rid).unwrap_or(0)),
        Act::Pause(_) => tr!("Pausar download").into(),
        Act::Resume(rid) => tr!("Continuar download · {}%", progress_of(library, rid).unwrap_or(0)),
        Act::Update(_) => tr!("Atualizar para {}", entry.as_ref().and_then(|e| e.latest.as_ref()).map(|l| l.version.clone()).unwrap_or_default()),
        Act::Backup(_) => tr!("Fazer backup dos saves").into(),
        Act::Options => tr!("Opções").into(),
        Act::Unavailable => tr!("Indisponível").into(),
    }
}

/// While downloading: cancel, then pause; once paused: carry on, then cancel.
fn download_acts(library: Library, rid: String) -> Vec<Act> {
    if crate::discover::is_paused(library, &rid) {
        vec![Act::Resume(rid.clone()), Act::Cancel(rid)]
    } else {
        vec![Act::Cancel(rid.clone()), Act::Pause(rid)]
    }
}

#[component]
pub fn GamePage(id: String) -> impl IntoView {
    let library = expect_context::<Library>();
    let tv = expect_context::<Tv>();
    let focus = Focus::new("game", false);
    tv.keep_position(focus);
    let id = StoredValue::new(id);

    let game = Memo::new(move |_| library.games.get().and_then(Result::ok).and_then(|all| all.into_iter().find(|g| g.id == id.get_value())));
    let entry = Memo::new(move |_| {
        let rid = match game.get() {
            Some(Game { source: Source::Recomp { recomp_id, .. }, .. }) => Some(recomp_id),
            Some(_) => None,
            None => id.get_value().strip_prefix("recomp:").map(str::to_string),
        }?;
        library.recomps.get().and_then(Result::ok)?.into_iter().find(|e| e.id == rid)
    });
    let item = move || game.get().map(Item::Game).or_else(|| entry.get().map(Item::Entry));
    Effect::new(move |_| {
        if let Some(i) = item() {
            tv.show_backdrop(i.heroes());
        }
    });

    let actions = move || {
        let mut list = Vec::new();
        let rid = entry.get().map(|e| e.id);
        let downloading = rid.as_deref().is_some_and(|r| library.progress.with(|p| p.contains_key(r)));
        match (game.get(), entry.get()) {
            (Some(g), e) => {
                if downloading {
                    list.extend(download_acts(library, rid.clone().unwrap_or_default()));
                } else if library.running.with(|r| r.contains(&g.id)) {
                    list.push(Act::Running);
                } else {
                    list.push(Act::Play(g.clone()));
                    // A merged classic: its original plays from here too, as on the desktop.
                    let games = library.games.get_untracked().and_then(Result::ok).unwrap_or_default();
                    let recomps = library.recomps.get_untracked().and_then(Result::ok).unwrap_or_default();
                    let players = library.players.get_untracked().unwrap_or_default();
                    let original = library.roms.get().unwrap_or_default().into_iter()
                        .find(|r| crate::classics::recomp_game(r, &games, &recomps).is_some_and(|m| m.id == g.id))
                        .and_then(|r| {
                            let console = crate::classics::console_of(&r);
                            crate::classics::ready_player(&players, console).map(|name| Act::PlayOriginal(r.path.clone(), console.to_string(), name))
                        });
                    list.extend(original);
                }
                let update = e.as_ref().filter(|_| !g.external && !downloading).and_then(|e| e.update_to().map(|_| e.id.clone()));
                if let Some(u) = update {
                    list.push(Act::Update(u));
                }
                if let Source::Recomp { recomp_id, .. } = &g.source {
                    let _ = recomp_id;
                    list.push(Act::Backup(g.id.clone()));
                }
            }
            (None, Some(e)) if downloading => list.extend(download_acts(library, e.id.clone())),
            (None, Some(e)) => list.push(if e.latest.is_some() { Act::Install(e.id.clone()) } else { Act::Unavailable }),
            (None, None) => {}
        }
        list.push(Act::Options);
        list
    };
    // A Steam game's store page and achievements, both kept on disk once read.
    let steam_app = Memo::new(move |_| match game.get().map(|g| g.source) {
        Some(Source::Steam { app_id }) => Some(app_id),
        _ => None,
    });
    let store = LocalResource::new(move || {
        let app = steam_app.get();
        async move {
            match app {
                Some(a) => api::steam_details(a).await.ok(),
                None => None,
            }
        }
    });
    let achievements = LocalResource::new(move || {
        let app = steam_app.get();
        async move {
            match app {
                Some(a) => api::game_achievements("steam", &a.to_string()).await.ok(),
                None => None,
            }
        }
    });
    let shots = move || store.get().flatten().map(|d| d.screenshots).unwrap_or_default();
    let viewing = RwSignal::new(None::<usize>);
    let rows = move || {
        let n = shots().len();
        if n > 0 { vec![actions().len(), n] } else { vec![actions().len()] }
    };
    Effect::new(move |_| focus.clamp(&rows()));

    Effect::new(move |_| {
        tv.hints.set(if viewing.get().is_some() {
            vec![(Btn::Left, tr!("Trocar imagem")), (Btn::B, tr!("Fechar"))]
        } else {
            vec![(Btn::A, tr!("Escolher")), (Btn::X, tr!("Opções")), (Btn::B, tr!("Voltar"))]
        });
    });
    let run = move |act: Act| match act {
        Act::Play(g) => {
            play(Sound::Select);
            play_game(library, g);
        }
        Act::PlayOriginal(path, console, _) => {
            play(Sound::Select);
            crate::classics::play_original(library, path, console);
        }
        Act::Install(rid) | Act::Update(rid) | Act::Resume(rid) => {
            play(Sound::Select);
            start_install(library, rid);
        }
        Act::Cancel(rid) => {
            play(Sound::Back);
            cancel_install(library, rid);
        }
        Act::Pause(rid) => {
            play(Sound::Back);
            crate::discover::pause_install(library, rid);
        }
        Act::Backup(gid) => {
            play(Sound::Select);
            spawn_local(async move {
                match library.report(api::backup_now(&gid).await) {
                    Some(Some(b)) => library.toast(tr!("Backup feito: {} arquivos.", b.files)),
                    Some(None) => library.toast(tr!("Nada novo para copiar: o backup mais recente já tem esses saves.")),
                    None => {}
                }
            });
        }
        Act::Options => {
            play(Sound::Select);
            tv.sheet.set(Some(Sheet::Options(id.get_value())));
        }
        // The game is open: Cerberus steps aside so it's in front again.
        Act::Running => {
            play(Sound::Select);
            crate::api::window_action("minimize");
        }
        Act::Unavailable => {
            play(Sound::Back);
            library.toast(tr!("Ainda não há uma versão deste jogo para baixar."));
        }
    };
    tv.handle_page(move |btn| {
        // The full-screen picture: left and right step, B or A closes.
        if let Some(i) = viewing.get_untracked() {
            let n = shots().len().max(1);
            match btn {
                Btn::Left => viewing.set(Some((i + n - 1) % n)),
                Btn::Right => viewing.set(Some((i + 1) % n)),
                Btn::B | Btn::A => viewing.set(None),
                _ => {}
            }
            play(Sound::Move);
            return true;
        }
        match btn {
        Btn::Left | Btn::Right | Btn::Up | Btn::Down => {
            if focus.step(btn, &rows()) {
                play(Sound::Move);
            } else if matches!(btn, Btn::Up | Btn::Down) {
                // Nothing further to focus: the page's text (a long description,
                // "How to start") scrolls instead, so all of it can be read.
                if let Some(main) = document().query_selector(".tv-main").ok().flatten() {
                    let step = f64::from(main.client_height()) * 0.4;
                    main.scroll_by_with_x_and_y(0.0, if btn == Btn::Down { step } else { -step });
                }
            }
            true
        }
        Btn::A if focus.row.get_untracked() == 1 => {
            play(Sound::Select);
            viewing.set(Some(focus.col.get_untracked()));
            true
        }
        Btn::A => {
            if let Some(act) = actions().get(focus.col.get_untracked()).cloned() {
                run(act);
            }
            true
        }
        Btn::X => {
            run(Act::Options);
            true
        }
        _ => false,
        }
    });

    let buttons = move || {
        let e = entry.get();
        actions()
            .into_iter()
            .enumerate()
            .map(|(c, act)| {
                let main = c == 0;
                let text = label(&act, library, &e);
                let progress = match &act {
                    Act::Cancel(rid) | Act::Resume(rid) => progress_of(library, rid),
                    _ => None,
                };
                let clicked = act.clone();
                view! {
                    <span class="tv-action" class:main=main class:on=move || focus.is(0, c) data-tvf=focus.key(0, c)
                        on:mouseenter=move |_| focus.set(0, c) on:click=move |_| run(clicked.clone())>
                        {progress.map(|p| view! { <i class="tv-action-fill" style=format!("width:{p}%")></i> })}
                        <b>{text}</b>
                    </span>
                }
            })
            .collect_view()
    };

    let head = move || {
        let i = item()?;
        let e = entry.get();
        let g = game.get();
        let kicker = match &e {
            // As on the desktop page: which native version, and its console.
            Some(e) => format!("{} · {}", e.name, e.console),
            None => g.as_ref().map(|g| crate::tiles::source_name(&g.source).to_string()).unwrap_or_default(),
        };
        let title = match g.as_ref().and_then(|g| g.logo_path.clone()).or_else(|| e.as_ref().and_then(|e| e.logo.clone())) {
            Some(path) => view! { <img class="tv-logo" src=crate::art::src(&path) alt=i.name() /> }.into_any(),
            None => view! { <h1>{i.name()}</h1> }.into_any(),
        };
        let based = e.as_ref().filter(|e| !e.game.is_empty()).map(|e| view! { <p class="tv-based">{tr!("Baseado em ")}<b>{e.game.clone()}</b></p> });
        let note = e.as_ref().map(|e| e.player_note.clone()).filter(|n| !n.is_empty()).map(|n| view! { <p class="tv-note-box"><b>{tr!("Como começar")}</b>{n}</p> });
        let about = e.as_ref().map(|e| e.about.clone()).filter(|a| !a.is_empty()).map(|a| view! { <p class="tv-about">{a}</p> });
        // Steam games: the store's short description.
        let store_about = move || store.get().flatten().map(|d| d.short).filter(|s| !s.is_empty()).map(|s| view! { <p class="tv-about">{s}</p> });
        let media = move || {
            let list = shots();
            (!list.is_empty()).then(|| {
                let thumbs = list
                    .into_iter()
                    .enumerate()
                    .map(|(c, s)| view! {
                        <span class="tv-shot" class:on=move || focus.is(1, c) data-tvf=focus.key(1, c)
                            on:mouseenter=move |_| focus.set(1, c) on:click=move |_| viewing.set(Some(c))>
                            <img src=s.thumb alt="" loading="lazy" />
                        </span>
                    })
                    .collect_view();
                view! { <div class="tv-shots">{thumbs}</div> }
            })
        };
        Some(view! {
            <div class="tv-game-main">
                <span class="tv-kicker">{kicker}</span>
                {title}
                {based}
                <div class="tv-actions">{buttons}</div>
                {about}
                {store_about}
                {note}
                {media}
            </div>
        })
    };

    let details = move || {
        let mut facts: Vec<(&'static str, String)> = Vec::new();
        if let Some(g) = game.get() {
            facts.push((tr!("Tempo de jogo"), if g.playtime_secs == 0 { tr!("Nunca jogado").into() } else { format::playtime(g.playtime_secs) }));
            if let Some(t) = g.last_played {
                facts.push((tr!("Última vez"), format::ago(t)));
            }
            if let Source::Recomp { version, .. } = &g.source {
                facts.push((tr!("Versão"), if version.is_empty() { tr!("Instalado por você").into() } else { version.clone() }));
            }
        }
        if let Some(e) = entry.get() {
            facts.push((tr!("Console"), e.console.clone()));
            if e.downloads > 0 {
                facts.push((tr!("Downloads"), format::count(e.downloads)));
            }
            if e.stars > 0 {
                facts.push((tr!("Estrelas"), format::count(e.stars)));
            }
            if let Some(copy) = e.owned_copy.clone() {
                facts.push((tr!("Sua cópia"), copy.rsplit(['\\', '/']).next().unwrap_or_default().to_string()));
            }
        }
        if let Some(d) = store.get().flatten() {
            if let Some(dev) = d.developers.first() {
                facts.push((tr!("Desenvolvedora"), dev.clone()));
            }
            if !d.released.is_empty() {
                facts.push((tr!("Lançamento"), d.released.clone()));
            }
            if !d.genres.is_empty() {
                facts.push((tr!("Gêneros"), d.genres.iter().take(3).cloned().collect::<Vec<_>>().join(", ")));
            }
            match d.controller.as_str() {
                "full" => facts.push((tr!("Controle"), tr!("Suporte total").to_string())),
                "partial" => facts.push((tr!("Controle"), tr!("Suporte parcial").to_string())),
                _ => {}
            }
            if let Some(m) = d.metacritic {
                facts.push(("Metacritic", format!("{m}/100")));
            }
        }
        let cover = item().map(|i| view! { <div class="tv-side-cover"><Poster sources=i.covers() title=i.name() /></div> });
        let rows = facts.into_iter().map(|(k, v)| view! { <div class="tv-fact"><span>{k}</span><b>{v}</b></div> }).collect_view();
        // Achievements: progress and the latest unlocks.
        let ach = achievements.get().flatten().filter(|d| d.game.total > 0).map(|d| {
            let g = d.game.clone();
            let pct = g.percent();
            let mut list = d.achievements.clone();
            list.sort_by(|a, b| b.unlocked_at.cmp(&a.unlocked_at));
            let icons = list
                .into_iter()
                .filter(|a| a.unlocked_at.is_some())
                .take(5)
                .map(|a| view! { <img src=a.icon.clone().unwrap_or_default() alt=a.name.clone() title=a.name.clone() /> })
                .collect_view();
            view! {
                <div class="tv-ach">
                    <div class="tv-ach-h"><span>{tr!("Conquistas")}</span><b>{format!("{}/{}", g.unlocked, g.total)}</b></div>
                    <div class="tv-ach-bar"><i style=format!("width: {pct}%")></i></div>
                    <div class="tv-ach-icons">{icons}</div>
                </div>
            }
        });
        view! { <aside class="tv-side">{cover}{ach}{rows}</aside> }
    };

    let viewer = move || {
        let i = viewing.get()?;
        let list = shots();
        let shot = list.get(i)?.clone();
        Some(view! {
            <div class="tv-viewer" on:click=move |_| viewing.set(None)>
                <img src=shot.full alt="" />
                <span class="tv-viewer-n">{format!("{} / {}", i + 1, list.len())}</span>
            </div>
        })
    };
    view! {
        <div class="tv-page game">
            {move || if item().is_some() {
                view! { {head} {details} }.into_any()
            } else {
                view! { <p class="tv-empty">{tr!("Esse jogo não está mais aqui.")}</p> }.into_any()
            }}
        </div>
        {viewer}
    }
}
