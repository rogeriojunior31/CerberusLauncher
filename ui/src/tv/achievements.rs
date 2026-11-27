//! Achievements on the TV: the totals, then rows of games: the ones nearly at
//! 100%, the platinums, and the ones with unlocks lately. Read from what's
//! kept on this PC, so it opens at once, then the accounts are asked (when
//! due). A opens the game's page (Steam and Epic games in the library), or
//! else the game's own list of achievements.

use cerberus_shared::tr;
use cerberus_shared::{AchievementGame, AchievementSource};
use leptos::prelude::*;

use super::{Btn, Focus, Sound, Tv, View, play};
use crate::art::Poster;
use crate::icons::{Icon, Svg};
use crate::{Library, api, format};

/// The library id of the game, when it can be opened on the TV.
fn library_id(library: Library, g: &AchievementGame) -> Option<String> {
    library.games.with(|all| {
        let list = all.as_ref().and_then(|r| r.as_ref().ok())?;
        match g.source {
            AchievementSource::Steam => {
                let id = format!("steam:{}", g.key);
                list.iter().any(|x| x.id == id).then_some(id)
            }
            // Epic's achievements are by namespace; the library's game has it in its source.
            AchievementSource::Epic => list.iter().find(|x| matches!(&x.source, cerberus_shared::Source::Epic { namespace, .. } if *namespace == g.key)).map(|x| x.id.clone()),
            _ => None,
        }
    })
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
        tv.hints.set(if secret { vec![(Btn::A, tr!("Mostrar")), (Btn::B, tr!("Voltar"))] } else { vec![(Btn::B, tr!("Voltar"))] });
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
                    Some(t) => format::unlocked_when(t),
                    None => a.rarity.map(|p| tr!("{}% dos jogadores", crate::format::decimal(format!("{p:.1}")))).unwrap_or_default(),
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
    // What's on this PC at once, then the accounts (when due): the TV used to never update.
    let hub = RwSignal::new(None::<Result<cerberus_shared::AchievementsHub, String>>);
    leptos::task::spawn_local(async move {
        let kept = api::achievements_hub(false).await;
        if hub.try_set(Some(kept)).is_some() {
            return;
        }
        let fresh = api::achievements_hub(true).await;
        // A failed update keeps what's on screen; the page may be gone by now.
        if fresh.is_ok() {
            let _ = hub.try_set(Some(fresh));
        }
    });
    // "All games" can hold hundreds: drawn a stretch at a time, growing as the focus moves along it.
    let shown = RwSignal::new(30usize);

    let shelves = Memo::new(move |_| {
        let Some(Ok(hub)) = hub.get() else { return Vec::new() };
        let mut near: Vec<AchievementGame> = hub.games.iter().filter(|g| g.unlocked > 0 && g.unlocked < g.total && g.percent() >= 40).cloned().collect();
        near.sort_by(|a, b| b.percent().cmp(&a.percent()));
        let mut plats: Vec<AchievementGame> = hub.games.iter().filter(|g| g.total > 0 && g.unlocked == g.total).cloned().collect();
        plats.sort_by(|a, b| b.last_unlock.cmp(&a.last_unlock));
        let mut lately: Vec<AchievementGame> = hub.games.iter().filter(|g| g.last_unlock.is_some()).cloned().collect();
        lately.sort_by(|a, b| b.last_unlock.cmp(&a.last_unlock));
        // Every game reachable: the shelves above only show some.
        let mut all: Vec<AchievementGame> = hub.games.clone();
        all.sort_by(|a, b| b.last_unlock.cmp(&a.last_unlock).then_with(|| super::pages::fold(&a.name).cmp(&super::pages::fold(&b.name))));
        vec![
            (tr!("Perto dos 100%"), near.into_iter().take(14).collect::<Vec<_>>()),
            (tr!("Platinados"), plats.into_iter().take(14).collect()),
            (tr!("Conquistas recentes"), lately.into_iter().take(14).collect()),
            (tr!("Todos os jogos"), all.into_iter().take(shown.get()).collect()),
        ]
        .into_iter()
        .filter(|(_, list)| !list.is_empty())
        .collect::<Vec<_>>()
    });
    let rows = move || shelves.with(|s| s.iter().map(|(_, l)| l.len()).collect::<Vec<_>>());
    Effect::new(move |_| focus.clamp(&rows()));
    Effect::new(move |_| {
        let (r, c) = (focus.row.get(), focus.col.get());
        let last = shelves.with_untracked(|s| s.len().saturating_sub(1));
        if r == last && c + 10 >= shown.get_untracked() {
            shown.set(c + 40);
        }
    });
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
        // The account synced longest ago tells how fresh this is.
        let oldest = hub.sources.iter().filter(|s| s.wait_until.is_none()).filter_map(|s| s.synced_at).min();
        let synced = match (hub.sources.iter().filter_map(|s| s.wait_until).max(), oldest) {
            (Some(w), _) => tr!("Uma loja pediu para esperar; tenta de novo às {}", format::clock(w)),
            (None, Some(at)) => tr!("Contas atualizadas {}", format::synced_ago(at)),
            (None, None) => String::new(),
        };
        Some(view! {
            <div class="tv-ach-sum">
                <span><b>{format::count(unlocked as u64)}</b>{tr!(" de {} conquistas", format::count(total as u64))}</span>
                <span><b>{hub.games.len()}</b>{tr!(" jogos")}</span>
                <span class="gold"><b>{plats}</b>{tr!(" platinados")}</span>
                <div class="tv-ach-bar"><i style=format!("width: {}%", pct.max(1))></i></div>
            </div>
            <p class="tv-ach-sync">{synced}</p>
            {hub.problems.iter().map(|p| view! { <p class="tv-ach-problem">{p.clone()}</p> }).collect_view()}
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
            return view! { <p class="tv-empty">{tr!("Nenhum jogo com conquistas ainda. Jogue um jogo da Steam, do Xbox ou da Epic, ou conecte o RetroAchievements em Configurações.")}</p> }.into_any();
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
