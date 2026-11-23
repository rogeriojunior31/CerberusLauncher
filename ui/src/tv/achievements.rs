//! Achievements on the TV: the totals, then rows of games: the ones nearly at
//! 100%, the platinums, and the ones with unlocks lately. Read from what's
//! kept on this PC, so it opens at once. A opens the game's page (Steam games
//! in the library), or else the game's own list of achievements.

use cerberus_shared::tr;
use cerberus_shared::{AchievementGame, AchievementSource};
use leptos::prelude::*;

use super::{Btn, Focus, Sound, Tv, View, play};
use crate::art::Poster;
use crate::icons::{Icon, Svg};
use crate::{Library, api, format};

/// The library id of the game, when it can be opened on the TV.
fn library_id(library: Library, g: &AchievementGame) -> Option<String> {
    if g.source != AchievementSource::Steam {
        return None;
    }
    let id = format!("steam:{}", g.key);
    library.games.with(|all| all.as_ref().and_then(|r| r.as_ref().ok()).and_then(|list| list.iter().any(|x| x.id == id).then_some(id)))
}

/// Where A takes a game: its page in the library, or else its own list of achievements.
fn target(library: Library, g: &AchievementGame) -> View {
    library_id(library, g).map_or_else(|| View::Achievements(g.source.code().to_string(), g.key.clone()), View::Game)
}

/// One game's achievements on the TV: unlocked first (latest on top), then the
/// rest from the most common, so the next easy one is near. Secret ones stay
/// hidden until A shows the focused one.
#[component]
pub fn GameList(source: String, key: String) -> impl IntoView {
    let tv = expect_context::<Tv>();
    let focus = Focus::new("achl", false);
    let revealed = RwSignal::new(std::collections::HashSet::<String>::new());
    let data = LocalResource::new(move || {
        let (source, key) = (source.clone(), key.clone());
        async move { api::game_achievements(&source, &key).await }
    });
    let list = Memo::new(move |_| {
        let Some(Ok(d)) = data.get() else { return Vec::new() };
        let mut all = d.achievements.clone();
        all.sort_by(|a, b| match (a.unlocked_at, b.unlocked_at) {
            (Some(x), Some(y)) => y.cmp(&x),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => b.rarity.unwrap_or(-1.0).total_cmp(&a.rarity.unwrap_or(-1.0)),
        });
        all
    });
    Effect::new(move |_| {
        if let Some(Ok(d)) = data.get() {
            tv.show_backdrop(d.game.art.iter().map(|a| crate::art::src(a)).collect());
        }
    });
    let rows = move || list.with(|l| vec![1; l.len()]);
    Effect::new(move |_| focus.clamp(&rows()));
    let concealed = move |a: &cerberus_shared::Achievement| a.hidden && a.unlocked_at.is_none() && !revealed.with(|r| r.contains(&a.id));
    Effect::new(move |_| {
        let secret = list.with(|l| l.get(focus.row.get()).is_some_and(|a| concealed(a)));
        tv.hints.set(if secret { vec![(Btn::A, tr!("Mostrar"))] } else { Vec::new() });
    });
    tv.handle_page(move |btn| match btn {
        Btn::Up | Btn::Down => {
            if focus.step(btn, &rows()) {
                play(Sound::Move);
            }
            true
        }
        Btn::A => {
            if let Some(a) = list.with_untracked(|l| l.get(focus.row.get_untracked()).cloned()).filter(|a| concealed(a)) {
                play(Sound::Select);
                revealed.update(|r| {
                    r.insert(a.id);
                });
            }
            true
        }
        _ => false,
    });

    let head = move || {
        let Some(Ok(d)) = data.get() else { return None };
        let g = d.game;
        let pct = g.percent();
        let done = g.total > 0 && g.unlocked == g.total;
        let points = (g.points_total > 0).then(|| view! { <span>{tr!("{} de {} pontos", g.points, g.points_total)}</span> });
        Some(view! {
            <header class="tv-al-head">
                <div class="tv-cover" class:plat=done><Poster sources=g.art.iter().map(|a| crate::art::src(a)).collect() title=g.name.clone() /></div>
                <div class="tv-al-t">
                    <small>{g.platform.clone()}</small>
                    <h1>{g.name.clone()}</h1>
                    <div class="tv-ach-sum">
                        <span><b>{format!("{}/{}", g.unlocked, g.total)}</b>{format!(" · {pct}%")}</span>
                        {done.then(|| view! { <span class="gold"><b>{tr!("Platinado")}</b></span> })}
                        {points}
                        <div class="tv-ach-bar"><i class:gold=done style=format!("width: {}%", pct.max(1))></i></div>
                    </div>
                </div>
            </header>
        })
    };
    let body = move || {
        match data.get() {
            None => return view! { <p class="tv-empty">{tr!("Lendo as conquistas…")}</p> }.into_any(),
            Some(Err(e)) => return view! { <p class="tv-empty">{e}</p> }.into_any(),
            Some(Ok(_)) => {}
        }
        list.get()
            .into_iter()
            .enumerate()
            .map(|(r, a)| {
                let secret = concealed(&a);
                let got = a.unlocked_at.is_some();
                let icon = if got { a.icon.clone() } else { a.icon_locked.clone().or_else(|| a.icon.clone()) };
                let (name, about) = if secret { (tr!("Conquista secreta").to_string(), tr!("Escondida até você desbloquear.").to_string()) } else { (a.name.clone(), a.description.clone()) };
                let when = match a.unlocked_at {
                    Some(t) => format::ago(t),
                    None => a.rarity.map(|p| tr!("{}% dos jogadores", format!("{p:.1}"))).unwrap_or_default(),
                };
                let points = (a.points > 0).then(|| view! { <em>{tr!("{} pts", a.points)}</em> });
                view! {
                    <div class="tv-al-row" class:on=move || focus.is(r, 0) class:got=got data-tvf=focus.key(r, 0) on:mouseenter=move |_| focus.set(r, 0)>
                        {match icon {
                            Some(src) if !secret => view! { <img src=crate::art::src(&src) alt="" /> }.into_any(),
                            _ => view! { <span class="tv-al-icon"><Svg icon=Icon::Trophy size=28 /></span> }.into_any(),
                        }}
                        <div class="tv-al-text"><b>{name}</b><span>{about}</span></div>
                        <div class="tv-al-when">{points}<small>{when}</small></div>
                    </div>
                }
            })
            .collect_view()
            .into_any()
    };
    view! {
        <div class="tv-page">
            {head}
            <div class="tv-al">{body}</div>
        </div>
    }
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
        let label = focused().map(|g| if library_id(library, &g).is_some() { tr!("Página do jogo") } else { tr!("Ver conquistas") });
        tv.hints.set(label.map(|l| vec![(Btn::A, l)]).unwrap_or_default());
    });
    tv.handle_page(move |btn| match btn {
        Btn::Up | Btn::Down | Btn::Left | Btn::Right => {
            if focus.step(btn, &rows()) {
                play(Sound::Move);
            }
            true
        }
        Btn::A => {
            if let Some(g) = focused() {
                play(Sound::Select);
                tv.open(target(library, &g));
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
                        let target = target(library, &g);
                        view! {
                            <div class="tv-tile" class:on=move || focus.is(r, c) data-tvf=focus.key(r, c)
                                on:mouseenter=move |_| focus.set(r, c)
                                on:click=move |_| { play(Sound::Select); tv.open(target.clone()); }>
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
