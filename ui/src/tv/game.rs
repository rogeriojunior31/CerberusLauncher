//! A game's page on the TV: its art across the screen, its logo or name, a row
//! of buttons led by the one thing to do now (play, install, update, the
//! download's progress), and the details beside it.

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
        Act::Running => "Jogando".into(),
        Act::Install(_) => match entry.as_ref().and_then(|e| e.latest.clone()) {
            Some(r) if r.size > 0 => format!("Instalar · {}", format::megabytes(r.size)),
            _ => "Instalar".into(),
        },
        Act::Cancel(rid) if crate::discover::is_paused(library, rid) => "Cancelar download".into(),
        Act::Cancel(rid) => format!("Baixando {}% · cancelar", progress_of(library, rid).unwrap_or(0)),
        Act::Pause(_) => "Pausar download".into(),
        Act::Resume(rid) => format!("Continuar download · {}%", progress_of(library, rid).unwrap_or(0)),
        Act::Update(_) => format!("Atualizar para {}", entry.as_ref().and_then(|e| e.latest.as_ref()).map(|l| l.version.clone()).unwrap_or_default()),
        Act::Backup(_) => "Fazer backup dos saves".into(),
        Act::Options => "Opções".into(),
        Act::Unavailable => "Indisponível".into(),
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
                }
                let update = e.as_ref().filter(|_| !g.external && !downloading).and_then(|e| {
                    let newest = e.latest.as_ref()?;
                    (e.installed.as_ref() != Some(&newest.version)).then(|| e.id.clone())
                });
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
    let rows = move || vec![actions().len()];
    Effect::new(move |_| focus.clamp(&rows()));

    tv.hints.set(vec![(Btn::A, "Escolher"), (Btn::X, "Opções"), (Btn::B, "Voltar")]);
    let run = move |act: Act| match act {
        Act::Play(g) => {
            play(Sound::Select);
            play_game(library, g);
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
                    Some(Some(b)) => library.toast(format!("Backup feito: {} arquivos.", b.files)),
                    Some(None) => library.toast("Nada novo para copiar: o backup mais recente já tem esses saves."),
                    None => {}
                }
            });
        }
        Act::Options => {
            play(Sound::Select);
            tv.sheet.set(Some(Sheet::Options(id.get_value())));
        }
        Act::Running | Act::Unavailable => {}
    };
    tv.handle_page(move |btn| match btn {
        Btn::Left | Btn::Right | Btn::Up | Btn::Down => {
            if focus.step(btn, &rows()) {
                play(Sound::Move);
            }
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
            Some(e) => format!("Recomp · {}", e.console),
            None => g.as_ref().map(|g| crate::tiles::source_name(&g.source).to_string()).unwrap_or_default(),
        };
        let title = match g.as_ref().and_then(|g| g.logo_path.clone()).or_else(|| e.as_ref().and_then(|e| e.logo.clone())) {
            Some(path) => view! { <img class="tv-logo" src=crate::art::src(&path) alt=i.name() /> }.into_any(),
            None => view! { <h1>{i.name()}</h1> }.into_any(),
        };
        let based = e.as_ref().filter(|e| !e.game.is_empty()).map(|e| view! { <p class="tv-based">"Baseado em "<b>{e.game.clone()}</b></p> });
        let note = e.as_ref().map(|e| e.player_note.clone()).filter(|n| !n.is_empty()).map(|n| view! { <p class="tv-note-box"><b>"Como começar"</b>{n}</p> });
        let about = e.as_ref().map(|e| e.about.clone()).filter(|a| !a.is_empty()).map(|a| view! { <p class="tv-about">{a}</p> });
        Some(view! {
            <div class="tv-game-main">
                <span class="tv-kicker">{kicker}</span>
                {title}
                {based}
                <div class="tv-actions">{buttons}</div>
                {about}
                {note}
            </div>
        })
    };

    let details = move || {
        let mut facts: Vec<(&'static str, String)> = Vec::new();
        if let Some(g) = game.get() {
            facts.push(("Tempo de jogo", if g.playtime_secs == 0 { "Nunca jogado".into() } else { format::playtime(g.playtime_secs) }));
            if let Some(t) = g.last_played {
                facts.push(("Última vez", format::ago(t)));
            }
            if let Source::Recomp { version, .. } = &g.source {
                facts.push(("Versão", if version.is_empty() { "Instalado por você".into() } else { version.clone() }));
            }
        }
        if let Some(e) = entry.get() {
            facts.push(("Console", e.console.clone()));
            if e.downloads > 0 {
                facts.push(("Downloads", format::count(e.downloads)));
            }
            if e.stars > 0 {
                facts.push(("Estrelas no GitHub", format::count(e.stars)));
            }
            if let Some(copy) = e.owned_copy.clone() {
                facts.push(("Sua cópia", copy.rsplit(['\\', '/']).next().unwrap_or_default().to_string()));
            }
        }
        let cover = item().map(|i| view! { <div class="tv-side-cover"><Poster sources=i.covers() title=i.name() /></div> });
        let rows = facts.into_iter().map(|(k, v)| view! { <div class="tv-fact"><span>{k}</span><b>{v}</b></div> }).collect_view();
        view! { <aside class="tv-side">{cover}{rows}</aside> }
    };

    view! {
        <div class="tv-page game">
            {move || if item().is_some() {
                view! { {head} {details} }.into_any()
            } else {
                view! { <p class="tv-empty">"Esse jogo não está mais aqui."</p> }.into_any()
            }}
        </div>
    }
}
