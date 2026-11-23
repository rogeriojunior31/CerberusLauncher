//! Achievements on the TV: the totals, then rows of games: the ones nearly at
//! 100%, the platinums, and the ones with unlocks lately. Read from what's
//! kept on this PC, so it opens at once. A opens the game's page (Steam games
//! in the library), where its achievements are listed.

use cerberus_shared::tr;
use cerberus_shared::{AchievementGame, AchievementSource};
use leptos::prelude::*;

use super::{Btn, Focus, Sound, Tv, View, play};
use crate::art::Poster;
use crate::{Library, api, format};

/// The library id of the game, when it can be opened on the TV.
fn library_id(library: Library, g: &AchievementGame) -> Option<String> {
    if g.source != AchievementSource::Steam {
        return None;
    }
    let id = format!("steam:{}", g.key);
    library.games.with(|all| all.as_ref().and_then(|r| r.as_ref().ok()).and_then(|list| list.iter().any(|x| x.id == id).then_some(id)))
}

#[component]
pub fn Achievements() -> impl IntoView {
    let library = expect_context::<Library>();
    let tv = expect_context::<Tv>();
    let focus = Focus::new("ach", false);
    tv.keep_position(focus);
    let hub = LocalResource::new(|| api::achievements_hub(false));

    let shelves = Memo::new(move |_| {
        let Some(Ok(hub)) = hub.get() else { return Vec::new() };
        let mut near: Vec<AchievementGame> = hub.games.iter().filter(|g| g.unlocked > 0 && g.unlocked < g.total && g.percent() >= 40).cloned().collect();
        near.sort_by(|a, b| b.percent().cmp(&a.percent()));
        let mut plats: Vec<AchievementGame> = hub.games.iter().filter(|g| g.total > 0 && g.unlocked == g.total).cloned().collect();
        plats.sort_by(|a, b| b.last_unlock.cmp(&a.last_unlock));
        let mut lately: Vec<AchievementGame> = hub.games.iter().filter(|g| g.last_unlock.is_some()).cloned().collect();
        lately.sort_by(|a, b| b.last_unlock.cmp(&a.last_unlock));
        vec![
            (tr!("Perto dos 100%"), near.into_iter().take(14).collect::<Vec<_>>()),
            (tr!("Platinados"), plats.into_iter().take(14).collect()),
            (tr!("Conquistas recentes"), lately.into_iter().take(14).collect()),
        ]
        .into_iter()
        .filter(|(_, list)| !list.is_empty())
        .collect::<Vec<_>>()
    });
    let rows = move || shelves.with(|s| s.iter().map(|(_, l)| l.len()).collect::<Vec<_>>());
    Effect::new(move |_| focus.clamp(&rows()));
    let focused = move || shelves.with(|s| s.get(focus.row.get()).and_then(|(_, l)| l.get(focus.col.get()).cloned()));
    Effect::new(move |_| {
        if let Some(g) = focused() {
            tv.show_backdrop(g.art.iter().map(|a| crate::art::src(a)).collect());
        }
    });
    Effect::new(move |_| {
        let can_open = focused().is_some_and(|g| library_id(library, &g).is_some());
        tv.hints.set(if can_open { vec![(Btn::A, tr!("Página do jogo"))] } else { Vec::new() });
    });
    tv.handle_page(move |btn| match btn {
        Btn::Up | Btn::Down | Btn::Left | Btn::Right => {
            if focus.step(btn, &rows()) {
                play(Sound::Move);
            }
            true
        }
        Btn::A => {
            if let Some(id) = focused().and_then(|g| library_id(library, &g)) {
                play(Sound::Select);
                tv.open(View::Game(id));
            }
            true
        }
        _ => false,
    });

    let summary = move || {
        let Some(Ok(hub)) = hub.get() else { return None };
        let unlocked: u32 = hub.games.iter().map(|g| g.unlocked).sum();
        let total: u32 = hub.games.iter().map(|g| g.total).sum();
        let plats = hub.games.iter().filter(|g| g.total > 0 && g.unlocked == g.total).count();
        let pct = if total == 0 { 0 } else { unlocked as u64 * 100 / total as u64 };
        Some(view! {
            <div class="tv-ach-sum">
                <span><b>{format::count(unlocked as u64)}</b>{tr!(" de {} conquistas", format::count(total as u64))}</span>
                <span><b>{hub.games.len()}</b>{tr!(" jogos")}</span>
                <span class="gold"><b>{plats}</b>{tr!(" platinados")}</span>
                <div class="tv-ach-bar"><i style=format!("width: {}%", pct.max(1))></i></div>
            </div>
        })
    };
    let body = move || {
        match hub.get() {
            None => return view! { <p class="tv-empty">{tr!("Lendo as conquistas…")}</p> }.into_any(),
            Some(Err(e)) => return view! { <p class="tv-empty">{e}</p> }.into_any(),
            Some(Ok(_)) => {}
        }
        let list = shelves.get();
        if list.is_empty() {
            return view! { <p class="tv-empty">{tr!("Nenhuma conquista ainda. Jogue um jogo da Steam ou conecte o RetroAchievements em Configurações.")}</p> }.into_any();
        }
        list.into_iter()
            .enumerate()
            .map(|(r, (title, games))| {
                let tiles = games
                    .into_iter()
                    .enumerate()
                    .map(|(c, g)| {
                        let pct = g.percent();
                        let done = g.total > 0 && g.unlocked == g.total;
                        let target = library_id(library, &g);
                        view! {
                            <div class="tv-tile" class:on=move || focus.is(r, c) data-tvf=focus.key(r, c)
                                on:mouseenter=move |_| focus.set(r, c)
                                on:click=move |_| if let Some(id) = target.clone() { play(Sound::Select); tv.open(View::Game(id)); }>
                                <div class="tv-cover" class:plat=done>
                                    <Poster sources=g.art.iter().map(|a| crate::art::src(a)).collect() title=g.name.clone() />
                                    <span class="tv-tag">{format!("{}/{}", g.unlocked, g.total)}</span>
                                    <div class="tv-bar" class:gold=done><i style=format!("width:{pct}%")></i></div>
                                </div>
                                <span class="tv-name">{g.name.clone()}</span>
                            </div>
                        }
                    })
                    .collect_view();
                view! {
                    <section class="tv-shelf">
                        <h2>{title}</h2>
                        <div class="tv-row">{tiles}</div>
                    </section>
                }
            })
            .collect_view()
            .into_any()
    };
    view! {
        <div class="tv-page">
            <h1 class="tv-title">{tr!("Conquistas")}</h1>
            {summary}
            {body}
        </div>
    }
}
