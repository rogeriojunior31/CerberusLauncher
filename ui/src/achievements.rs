//! The achievements hub: every game with achievements on Steam and
//! RetroAchievements, the latest unlocks, and each game's own list.

use cerberus_shared::{Achievement, AchievementGame, AchievementSource, AchievementsHub, GameAchievements, GameAward, tr};
use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::art::Poster;
use crate::icons::{Icon, Svg};
use crate::{Library, Route, api, format};

#[derive(Clone, Copy, PartialEq)]
enum Platform {
    All,
    Steam,
    Retro,
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    All,
    Started,
    Beaten,
    Done,
    NotStarted,
}

fn award_label(award: GameAward) -> &'static str {
    match award {
        GameAward::BeatenSoftcore => tr!("Zerado"),
        GameAward::BeatenHardcore => tr!("Zerado no hardcore"),
        GameAward::Completed => tr!("Completo"),
        GameAward::Mastered => tr!("Dominado"),
    }
}

/// The game's award as a badge; hardcore ones are gold and shine.
fn award_badge(award: Option<GameAward>, at: Option<u64>) -> Option<impl IntoView + use<>> {
    let award = award?;
    let title = at.map(|t| tr!("{} em {}", award_label(award), format::date_time(t))).unwrap_or_else(|| award_label(award).to_string());
    Some(view! {
        <span class="award" class:hard=award.hardcore() class:full=award.full() title=title>
            <Svg icon=Icon::Trophy size=13 />{award_label(award)}
        </span>
    })
}

#[derive(Clone, Copy, PartialEq)]
enum Order {
    Recent,
    Percent,
    Most,
    Name,
    Played,
}

fn source_label(source: AchievementSource) -> &'static str {
    match source {
        AchievementSource::Steam => "Steam",
        AchievementSource::Retro => "RetroAchievements",
    }
}

fn platform_chip(source: AchievementSource) -> impl IntoView {
    let class = match source {
        AchievementSource::Steam => "ach-src steam",
        AchievementSource::Retro => "ach-src retro",
    };
    view! { <span class=class>{source_label(source)}</span> }
}

/// The console for RetroAchievements games; nothing for Steam (the chip says it).
fn platform_note(g: &AchievementGame) -> Option<impl IntoView + use<>> {
    (g.platform != source_label(g.source)).then(|| view! { <small>{g.platform.clone()}</small> })
}

/// A progress bar with "X/Y · Z%".
fn progress(unlocked: u32, total: u32) -> impl IntoView {
    let pct = if total == 0 { 0 } else { unlocked * 100 / total };
    view! {
        <div class="ach-progress" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow=pct.to_string()>
            <div class="ach-bar"><i style=format!("width: {pct}%") class:done=pct == 100></i></div>
            <span>{format!("{unlocked}/{total} · {pct}%")}</span>
        </div>
    }
}

#[component]
pub fn AchievementsPage() -> impl IntoView {
    // What's on this PC shows at once; Steam and RetroAchievements are asked
    // right after, and their answer replaces it.
    let hub = RwSignal::new(None::<Result<AchievementsHub, String>>);
    let refreshing = RwSignal::new(false);
    let search = RwSignal::new(String::new());
    let platform = RwSignal::new(Platform::All);
    let state = RwSignal::new(State::All);
    let order = RwSignal::new(Order::Recent);
    let refresh_later = StoredValue::new(None::<Callback<()>>);
    let refresh = move || {
        if refreshing.get_untracked() {
            return;
        }
        refreshing.set(true);
        leptos::task::spawn_local(async move {
            let got = api::achievements_hub(true).await;
            let again = got.as_ref().is_ok_and(|h| h.retry_soon);
            // A failed refresh keeps what's already on screen.
            if got.is_ok() || hub.get_untracked().is_none() {
                hub.set(Some(got));
            }
            refreshing.set(false);
            // Steam asked to wait: carry on by itself in a few minutes, if the page is still open.
            if again {
                let f = wasm_bindgen::closure::Closure::once_into_js(move || {
                    if hub.try_get_untracked().is_some() {
                        if let Some(again) = refresh_later.try_get_value().flatten() {
                            again.run(());
                        }
                    }
                });
                let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(f.unchecked_ref(), 180_000);
            }
        });
    };
    refresh_later.set_value(Some(Callback::new(move |_| refresh())));
    leptos::task::spawn_local(async move {
        let got = api::achievements_hub(false).await;
        if hub.get_untracked().is_none() {
            hub.set(Some(got));
        }
        refresh();
    });

    let body = move || match hub.get() {
        None => view! {
            <div class="ach-loading" role="status"><span class="spin"></span>{tr!("Lendo suas conquistas…")}</div>
        }
        .into_any(),
        Some(Err(e)) => view! { <p class="problem">{e}</p> }.into_any(),
        Some(Ok(h)) => view! { <HubBody hub=h search platform state order /> }.into_any(),
    };
    view! {
        <div class="page ach-page">
            <header class="page-h">
                <h1>{tr!("Conquistas")}</h1>
                <div class="ach-top-a">
                    <Show when=move || refreshing.get()>
                        <span class="ach-sync" role="status"><span class="spin"></span>{tr!("Atualizando")}</span>
                    </Show>
                    <button type="button" class="btn" disabled=move || refreshing.get() on:click=move |_| refresh()>
                        <Svg icon=Icon::Refresh size=16 />{tr!("Atualizar")}
                    </button>
                </div>
            </header>
            {body}
        </div>
    }
}

#[component]
fn HubBody(hub: AchievementsHub, search: RwSignal<String>, platform: RwSignal<Platform>, state: RwSignal<State>, order: RwSignal<Order>) -> impl IntoView {
    let library = expect_context::<Library>();
    let unlocked: u32 = hub.games.iter().map(|g| g.unlocked).sum();
    let total: u32 = hub.games.iter().map(|g| g.total).sum();
    let complete = hub.games.iter().filter(|g| g.total > 0 && g.unlocked == g.total).count();
    let beaten = hub.games.iter().filter(|g| g.award.is_some_and(|a| !a.full())).count();
    let hardcore_awards = hub.games.iter().filter(|g| g.award.is_some_and(GameAward::hardcore)).count();

    // One line of numbers instead of a wall of boxes.
    let mut facts = vec![
        tr!("{} de {} conquistas", format::count(unlocked as u64), format::count(total as u64)),
        if hub.games.len() == 1 { tr!("1 jogo").to_string() } else { tr!("{} jogos", hub.games.len()) },
    ];
    if complete > 0 {
        facts.push(tr!("{} com 100%", complete));
    }
    if beaten > 0 {
        facts.push(if beaten == 1 { tr!("1 zerado").to_string() } else { tr!("{} zerados", beaten) });
    }
    if hardcore_awards > 0 {
        facts.push(tr!("{} no hardcore", hardcore_awards));
    }
    if hub.retro && hub.retro_points + hub.retro_softcore > 0 {
        facts.push(tr!("{} pontos no RetroAchievements", format::count(hub.retro_points + hub.retro_softcore)));
    }
    let pct = if total == 0 { 0 } else { (unlocked as u64 * 100 / total as u64) as u32 };
    let summary = view! {
        <section class="ach-sum" aria-label=tr!("Resumo")>
            <p>{facts.into_iter().enumerate().map(|(i, f)| view! { {(i > 0).then(|| view! { <i aria-hidden="true">"·"</i> })}<span>{f}</span> }).collect_view()}</p>
            <div class="ach-sum-bar" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow=pct.to_string()>
                <i style=format!("width: {}%", pct.max(1))></i>
            </div>
        </section>
    };

    let notes = view! {
        {(!hub.steam).then(|| view! {
            <div class="ach-note"><Svg icon=Icon::Info size=18 /><span>{tr!("A Steam não foi encontrada neste PC, ou ainda não guardou conquistas. Abra um jogo pela Steam e volte aqui.")}</span></div>
        })}
        {(!hub.retro).then(|| view! {
            <div class="ach-note">
                <Svg icon=Icon::Trophy size=18 />
                <span>{tr!("Conecte sua conta do RetroAchievements para ver as conquistas dos clássicos.")}</span>
                <button type="button" class="btn sm" on:click=move |_| library.go(Route::Settings)>{tr!("Conectar")}</button>
            </div>
        })}
        {hub.problems.iter().map(|p| view! {
            <div class="ach-note warn" role="alert"><Svg icon=Icon::Info size=18 /><span>{p.clone()}</span></div>
        }).collect_view()}
    };

    // The games closest to 100%: what's worth playing next.
    let mut near: Vec<AchievementGame> = hub.games.iter().filter(|g| g.unlocked > 0 && g.unlocked < g.total && g.percent() >= 40).cloned().collect();
    near.sort_by(|a, b| b.percent().cmp(&a.percent()).then_with(|| (a.total - a.unlocked).cmp(&(b.total - b.unlocked))));
    near.truncate(4);
    let near = (!near.is_empty()).then(|| view! {
        <section class="shelf">
            <div class="shelf-h"><h2>{tr!("Perto dos 100%")}</h2></div>
            <div class="ach-near">{near.into_iter().map(|g| view! { <NearCard g /> }).collect_view()}</div>
        </section>
    });

    let recent = (!hub.recent.is_empty()).then(|| {
        let items = hub.recent.iter().take(24).map(|r| {
            let a = r.achievement.clone();
            let route = Route::AchievementGame(r.source.code().to_string(), r.game_key.clone());
            let title = a.description.clone();
            view! {
                <button type="button" class="ach-unlock" title=title on:click=move |_| library.go(route.clone())>
                    <AchIcon a=a.clone() unlocked=true />
                    <b>{a.name.clone()}</b>
                    <small>{r.game_name.clone()}</small>
                    <small class="ach-ago">{a.unlocked_at.map(format::ago).unwrap_or_default()}</small>
                </button>
            }
        }).collect_view();
        view! {
            <section class="shelf">
                <div class="shelf-h"><h2>{tr!("Últimas conquistas")}</h2></div>
                <div class="ach-strip">{items}</div>
            </section>
        }
    });

    let games = hub.games.clone();
    let list = move || {
        let needle = search.get().trim().to_lowercase();
        let mut shown: Vec<AchievementGame> = games
            .iter()
            .filter(|g| match platform.get() {
                Platform::All => true,
                Platform::Steam => g.source == AchievementSource::Steam,
                Platform::Retro => g.source == AchievementSource::Retro,
            })
            .filter(|g| match state.get() {
                State::All => true,
                State::Started => g.unlocked > 0 && g.unlocked < g.total,
                State::Beaten => g.award.is_some(),
                State::Done => g.total > 0 && g.unlocked == g.total,
                State::NotStarted => g.unlocked == 0,
            })
            .filter(|g| needle.is_empty() || g.name.to_lowercase().contains(&needle) || g.platform.to_lowercase().contains(&needle))
            .cloned()
            .collect();
        match order.get() {
            Order::Recent => shown.sort_by(|a, b| b.last_unlock.cmp(&a.last_unlock).then_with(|| b.last_played.cmp(&a.last_played))),
            Order::Percent => shown.sort_by(|a, b| b.percent().cmp(&a.percent()).then_with(|| b.unlocked.cmp(&a.unlocked))),
            Order::Most => shown.sort_by(|a, b| b.unlocked.cmp(&a.unlocked)),
            Order::Name => shown.sort_by_key(|g| g.name.to_lowercase()),
            Order::Played => shown.sort_by(|a, b| b.playtime_secs.cmp(&a.playtime_secs)),
        }
        if shown.is_empty() {
            return view! {
                <div class="empty small">
                    <Svg icon=Icon::Trophy size=28 />
                    <p class="muted">{tr!("Nenhum jogo com esses filtros.")}</p>
                </div>
            }
            .into_any();
        }
        view! { <div class="ach-lines" role="list">{shown.into_iter().map(|g| view! { <GameLine g /> }).collect_view()}</div> }.into_any()
    };

    let seg = |label: &'static str, on: Signal<bool>, set: Callback<()>| {
        view! { <button type="button" class="seg" role="tab" class:on=on aria-selected=move || on.get().to_string() on:click=move |_| set.run(())>{label}</button> }
    };
    let platforms = [(Platform::All, tr!("Todas")), (Platform::Steam, "Steam"), (Platform::Retro, "RetroAchievements")]
        .into_iter()
        .map(|(p, label)| seg(label, Signal::derive(move || platform.get() == p), Callback::new(move |_| platform.set(p))))
        .collect_view();
    let states = [(State::All, tr!("Todos")), (State::Started, tr!("Em andamento")), (State::Beaten, tr!("Zerados")), (State::Done, tr!("100%")), (State::NotStarted, tr!("Não começados"))]
        .into_iter()
        .map(|(s, label)| {
            view! {
                <button type="button" class="chip-btn" class:on=move || state.get() == s aria-pressed=move || (state.get() == s).to_string()
                    on:click=move |_| state.set(s)>{label}</button>
            }
        })
        .collect_view();

    view! {
        {summary}
        {notes}
        {near}
        {recent}
        <section class="shelf">
            <div class="shelf-h"><h2>{tr!("Todos os jogos")}</h2></div>
            <div class="ach-tools" aria-label=tr!("Filtros das conquistas")>
                <div class="segs" role="tablist" aria-label=tr!("Plataforma")>{platforms}</div>
                <div class="chips" role="group" aria-label=tr!("Situação")>{states}</div>
                <label class="search">
                    <Svg icon=Icon::Search size=18 />
                    <input type="search" placeholder=tr!("Buscar jogo") aria-label=tr!("Buscar jogo")
                        prop:value=move || search.get() on:input=move |e| search.set(event_target_value(&e)) />
                </label>
                <select class="select" aria-label=tr!("Ordenar")
                    on:change=move |e| order.set(match event_target_value(&e).as_str() {
                        "percent" => Order::Percent, "most" => Order::Most, "name" => Order::Name, "played" => Order::Played, _ => Order::Recent,
                    })>
                    <option value="recent">{tr!("Desbloqueadas recentemente")}</option>
                    <option value="percent">{tr!("Mais perto de completar")}</option>
                    <option value="most">{tr!("Mais conquistas")}</option>
                    <option value="played">{tr!("Mais jogados")}</option>
                    <option value="name">{tr!("A a Z")}</option>
                </select>
            </div>
            {list}
        </section>
    }
}

/// A game close to 100%, with what's left.
#[component]
fn NearCard(g: AchievementGame) -> impl IntoView {
    let library = expect_context::<Library>();
    let route = Route::AchievementGame(g.source.code().to_string(), g.key.clone());
    let left = g.total - g.unlocked;
    let pct = g.percent();
    view! {
        <button type="button" class="ach-near-c" on:click=move |_| library.go(route.clone())>
            <span class="ach-art"><Poster sources=g.art.iter().map(|a| crate::art::src(a)).collect() title=g.name.clone() /></span>
            <span class="ach-near-t">
                <b>{g.name.clone()}</b>
                <small>{if left == 1 { tr!("Falta 1 conquista").to_string() } else { tr!("Faltam {} conquistas", left) }}</small>
                <span class="ach-near-p"><em>{pct}</em>"%"</span>
                <span class="ach-bar"><i style=format!("width: {pct}%")></i></span>
            </span>
        </button>
    }
}

/// A game in the hub's list: one line, scannable down the columns.
#[component]
fn GameLine(g: AchievementGame) -> impl IntoView {
    let library = expect_context::<Library>();
    let route = Route::AchievementGame(g.source.code().to_string(), g.key.clone());
    let hard = g.award.is_some_and(GameAward::hardcore);
    let pct = g.percent();
    let done = g.total > 0 && g.unlocked == g.total;
    let mut meta = vec![g.platform.clone()];
    if let Some(t) = g.playtime_secs {
        meta.push(tr!("{} jogados", format::playtime(t)));
    }
    view! {
        <button type="button" role="listitem" class="ach-line" class:award-hard=hard class:done=done
            on:click=move |_| library.go(route.clone())>
            <span class="ach-art"><Poster sources=g.art.iter().map(|a| crate::art::src(a)).collect() title=g.name.clone() /></span>
            <span class="ach-line-t">
                <span class="ach-line-h"><b>{g.name.clone()}</b>{award_badge(g.award, g.award_at)}</span>
                <small>{meta.join(" · ")}</small>
            </span>
            <span class="ach-line-p">
                <span class="ach-bar"><i style=format!("width: {pct}%") class:done=pct == 100></i></span>
                <span class="ach-line-n"><b>{format!("{pct}%")}</b><small>{format!("{}/{}", g.unlocked, g.total)}</small></span>
            </span>
            <small class="ach-line-w">{g.last_unlock.map(format::ago).unwrap_or_default()}</small>
        </button>
    }
}

/// An achievement's picture: in color when unlocked, grey otherwise, and gold
/// with a shine when unlocked in hardcore.
#[component]
fn AchIcon(a: Achievement, unlocked: bool) -> impl IntoView {
    let src = if unlocked { a.icon.clone() } else { a.icon_locked.clone().or(a.icon.clone()) };
    let hard = unlocked && a.hardcore_at.is_some();
    view! {
        <span class="ach-icon" class:locked=!unlocked class:hard=hard title=hard.then(|| tr!("Desbloqueada no hardcore"))>
            {src.map(|s| view! { <img src=s alt="" loading="lazy" /> })}
        </span>
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Show {
    All,
    Unlocked,
    Locked,
}

#[derive(Clone, Copy, PartialEq)]
enum ListOrder {
    Game,
    Recent,
    Rare,
    Common,
}

/// One game's achievements.
#[component]
pub fn AchievementGamePage(source: String, key: String) -> impl IntoView {
    let library = expect_context::<Library>();
    let source = StoredValue::new(source);
    let key = StoredValue::new(key);
    let data = LocalResource::new(move || async move { api::game_achievements(&source.get_value(), &key.get_value()).await });
    let body = move || match data.get() {
        None => view! { <p class="muted" role="status">{tr!("Lendo as conquistas…")}</p> }.into_any(),
        Some(Err(e)) => view! { <p class="problem">{e}</p> }.into_any(),
        Some(Ok(d)) => view! { <GameBody d /> }.into_any(),
    };
    view! {
        <div class="page">
            <div>
                <button type="button" class="btn" on:click=move |_| library.back()><Svg icon=Icon::Back size=16 />{tr!("Voltar")}</button>
            </div>
            {body}
        </div>
    }
}

/// A game's achievements inside its page in the Library.
#[component]
pub fn AchievementsTab(source: String, key: String) -> impl IntoView {
    let source = StoredValue::new(source);
    let key = StoredValue::new(key);
    let data = LocalResource::new(move || async move { api::game_achievements(&source.get_value(), &key.get_value()).await });
    move || match data.get() {
        None => view! { <div class="ach-loading" role="status"><span class="spin"></span>{tr!("Lendo as conquistas…")}</div> }.into_any(),
        Some(Err(e)) => view! {
            <div class="empty small"><Svg icon=Icon::Trophy size=28 /><p class="muted">{e}</p></div>
        }
        .into_any(),
        Some(Ok(d)) if d.achievements.is_empty() => view! {
            <div class="empty small"><Svg icon=Icon::Trophy size=28 /><p class="muted">{tr!("Esse jogo não tem conquistas.")}</p></div>
        }
        .into_any(),
        Some(Ok(d)) => view! { <div class="ach-tab"><GameBody d embedded=true /></div> }.into_any(),
    }
}

#[component]
fn GameBody(d: GameAchievements, #[prop(optional)] embedded: bool) -> impl IntoView {
    let library = expect_context::<Library>();
    let g = d.game.clone();
    let search = RwSignal::new(String::new());
    let show = RwSignal::new(Show::All);
    let order = RwSignal::new(ListOrder::Game);
    let reveal_all = RwSignal::new(false);
    let revealed = RwSignal::new(std::collections::HashSet::<String>::new());
    let any_rarity = d.achievements.iter().any(|a| a.rarity.is_some());
    let any_hidden = d.achievements.iter().any(|a| a.hidden && a.unlocked_at.is_none());
    let in_library = format!("steam:{}", g.key);
    let library_game = (g.source == AchievementSource::Steam)
        .then(|| library.games.get().and_then(Result::ok).and_then(|list| list.into_iter().find(|x| x.id == in_library)))
        .flatten();

    let mut stats = vec![(tr!("Progresso").to_string(), format!("{}/{} · {}%", g.unlocked, g.total, g.percent()))];
    if let Some(t) = g.playtime_secs {
        stats.push((tr!("Tempo de jogo (Steam)").to_string(), format::playtime(t)));
    }
    if let Some(t) = g.last_played {
        stats.push((tr!("Última vez").to_string(), format::ago(t)));
    }
    if let Some(t) = g.last_unlock {
        stats.push((tr!("Última conquista").to_string(), format::ago(t)));
    }
    if g.points_total > 0 {
        stats.push((tr!("Pontos").to_string(), format!("{} / {}", g.points, g.points_total)));
    }
    if g.source == AchievementSource::Retro {
        stats.push((tr!("Hardcore").to_string(), format!("{}/{}", g.hardcore, g.total)));
    }

    let unlocked_count = d.achievements.iter().filter(|a| a.unlocked_at.is_some()).count();
    let locked_count = d.achievements.len() - unlocked_count;
    let all = d.achievements.clone();
    let list = move || {
        let needle = search.get().trim().to_lowercase();
        let mut shown: Vec<Achievement> = all
            .iter()
            .filter(|a| match show.get() {
                Show::All => true,
                Show::Unlocked => a.unlocked_at.is_some(),
                Show::Locked => a.unlocked_at.is_none(),
            })
            .filter(|a| {
                let concealed = a.hidden && a.unlocked_at.is_none() && !reveal_all.get() && !revealed.with(|r| r.contains(&a.id));
                needle.is_empty() || (!concealed && (a.name.to_lowercase().contains(&needle) || a.description.to_lowercase().contains(&needle)))
            })
            .cloned()
            .collect();
        let rarity = |a: &Achievement| a.rarity.unwrap_or(101.0);
        match order.get() {
            ListOrder::Game => {}
            ListOrder::Recent => shown.sort_by(|a, b| b.unlocked_at.cmp(&a.unlocked_at)),
            ListOrder::Rare => shown.sort_by(|a, b| rarity(a).total_cmp(&rarity(b))),
            ListOrder::Common => shown.sort_by(|a, b| b.rarity.unwrap_or(-1.0).total_cmp(&a.rarity.unwrap_or(-1.0))),
        }
        if shown.is_empty() {
            return view! { <p class="muted">{tr!("Nenhuma conquista com esses filtros.")}</p> }.into_any();
        }
        shown.into_iter().map(|a| view! { <AchRow a reveal_all revealed /> }).collect_view().into_any()
    };

    let tab = move |which: Show, label: String| {
        view! {
            <button type="button" class="seg" role="tab" class:on=move || show.get() == which aria-selected=move || (show.get() == which).to_string()
                on:click=move |_| show.set(which)>{label}</button>
        }
    };
    let head = if embedded {
        view! {
            <div class="ach-tab-h">
                {award_badge(g.award, g.award_at)}
                {progress(g.unlocked, g.total)}
                {g.last_unlock.map(|t| view! { <small class="muted">{tr!("Última conquista {}", format::ago(t))}</small> })}
            </div>
        }
        .into_any()
    } else {
        view! {
        <header class="ach-head">
            <span class="ach-art big">
                <Poster sources=g.art.iter().map(|a| crate::art::src(a)).collect() title=g.name.clone() />
            </span>
            <div class="ach-head-t">
                <span class="ach-game-m">{platform_chip(g.source)}{platform_note(&g)}</span>
                <h1>{g.name.clone()}</h1>
                {award_badge(g.award, g.award_at)}
                {progress(g.unlocked, g.total)}
                {library_game.map(|game| {
                    let id = game.id.clone();
                    view! { <button type="button" class="btn sm" on:click=move |_| library.go(Route::Game(id.clone()))>{tr!("Página do jogo")}</button> }
                })}
            </div>
        </header>
        }
        .into_any()
    };
    // In the game's own page the header already says the time and the progress.
    let stats_view = (!embedded).then(|| view! {
        <div class="stats">
            {stats.into_iter().map(|(label, value)| view! { <div class="stat"><span>{label}</span><b>{value}</b></div> }).collect_view()}
        </div>
    });
    view! {
        {head}
        {stats_view}
        <section class="catalog-filters" aria-label=tr!("Filtros das conquistas")>
            <div class="toolbar">
                <label class="search">
                    <Svg icon=Icon::Search size=18 />
                    <input type="search" placeholder=tr!("Buscar conquista") aria-label=tr!("Buscar conquista")
                        prop:value=move || search.get() on:input=move |e| search.set(event_target_value(&e)) />
                </label>
                <select class="select" aria-label=tr!("Ordenar")
                    on:change=move |e| order.set(match event_target_value(&e).as_str() {
                        "recent" => ListOrder::Recent, "rare" => ListOrder::Rare, "common" => ListOrder::Common, _ => ListOrder::Game,
                    })>
                    <option value="game">{tr!("Ordem do jogo")}</option>
                    <option value="recent">{tr!("Desbloqueadas recentemente")}</option>
                    {any_rarity.then(|| view! {
                        <option value="rare">{tr!("Mais raras primeiro")}</option>
                        <option value="common">{tr!("Mais comuns primeiro")}</option>
                    })}
                </select>
            </div>
            <div class="lib-tabs">
                <div class="segs big" role="tablist" aria-label=tr!("Mostrar")>
                    {tab(Show::All, tr!("Todas ({})", unlocked_count + locked_count))}
                    {tab(Show::Unlocked, tr!("Desbloqueadas ({})", unlocked_count))}
                    {tab(Show::Locked, tr!("Bloqueadas ({})", locked_count))}
                </div>
                {any_hidden.then(|| view! {
                    <button type="button" class="chip-btn" class:on=move || reveal_all.get() aria-pressed=move || reveal_all.get().to_string()
                        on:click=move |_| reveal_all.update(|r| *r = !*r)>{tr!("Mostrar as secretas")}</button>
                })}
            </div>
        </section>
        <div class="ach-list">{list}</div>
    }
}

/// One achievement in a game's list.
#[component]
fn AchRow(a: Achievement, reveal_all: RwSignal<bool>, revealed: RwSignal<std::collections::HashSet<String>>) -> impl IntoView {
    let unlocked = a.unlocked_at.is_some();
    let id = a.id.clone();
    let concealed = {
        let id = id.clone();
        move || a.hidden && !unlocked && !reveal_all.get() && !revealed.with(|r| r.contains(&id))
    };
    let name = a.name.clone();
    let description = a.description.clone();
    let text = {
        let concealed = concealed.clone();
        move || {
            if concealed() {
                let id = id.clone();
                view! {
                    <b>{tr!("Conquista secreta")}</b>
                    <button type="button" class="link" on:click=move |_| revealed.update(|r| { r.insert(id.clone()); })>{tr!("Clique para revelar")}</button>
                }
                .into_any()
            } else {
                view! { <b>{name.clone()}</b><span class="muted">{description.clone()}</span> }.into_any()
            }
        }
    };
    let rare = a.rarity.is_some_and(|r| r < 10.0);
    let rarity = a.rarity.map(|r| {
        let shown = if r < 0.1 { "<0.1".to_string() } else { format::decimal(format!("{r:.1}")) };
        tr!("{}% dos jogadores", shown)
    });
    let when = a.unlocked_at.map(|t| if t == 0 { tr!("Desbloqueada").to_string() } else { format::date_time(t) });
    view! {
        <div class="ach-row" class:unlocked=unlocked>
            <AchIcon a=a.clone() unlocked />
            <div class="ach-row-t">{text}</div>
            <div class="ach-row-m">
                {match when {
                    Some(w) => view! { <span class="ach-when"><Svg icon=Icon::Check size=14 />{w}</span> }.into_any(),
                    None => view! { <span class="muted small">{tr!("Bloqueada")}</span> }.into_any(),
                }}
                <span class="ach-tags">
                    {a.hardcore_at.map(|_| view! { <span class="ach-tag hard">{tr!("Hardcore")}</span> })}
                    {(a.points > 0).then(|| view! { <span class="ach-tag">{tr!("{} pts", a.points)}</span> })}
                    {rarity.map(|r| view! { <span class="ach-tag" class:rare=rare>{r}</span> })}
                </span>
            </div>
        </div>
    }
}
