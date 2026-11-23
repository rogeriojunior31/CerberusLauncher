//! Novidades: what changed since the player last looked. Updates waiting for
//! their recomps, and what happened lately across every game (installs,
//! sessions, achievements, problems), from the same history each game's page
//! shows.

use cerberus_shared::tr;
use cerberus_shared::{Game, RecompEntry, Source, TrailEntry};
use leptos::prelude::*;

use crate::art::{Poster, cover_sources};
use crate::icons::{Icon, Svg};
use crate::{Library, Route, api, format};

/// A recomp of the library with a newer release than the one installed.
pub struct Update {
    pub game: Game,
    pub recomp_id: String,
    pub from: String,
    pub to: String,
}

/// Recomps installed by Cerberus whose newest release isn't the one installed.
pub fn updates(games: &[Game], recomps: &[RecompEntry]) -> Vec<Update> {
    let newest = |id: &str| recomps.iter().find(|e| e.id == id).and_then(|e| e.latest.as_ref()).map(|r| r.version.clone());
    behind(games, newest)
}

fn behind(games: &[Game], newest: impl Fn(&str) -> Option<String>) -> Vec<Update> {
    games
        .iter()
        .filter(|g| !g.external)
        .filter_map(|game| {
            let Source::Recomp { recomp_id, version, .. } = &game.source else { return None };
            let to = newest(recomp_id)?;
            (to != *version).then(|| Update { game: game.clone(), recomp_id: recomp_id.clone(), from: version.clone(), to })
        })
        .collect()
}

pub fn update_count(library: Library) -> usize {
    let games = library.games.get().and_then(Result::ok).unwrap_or_default();
    let recomps = library.recomps.get().and_then(Result::ok).unwrap_or_default();
    updates(&games, &recomps).len()
}

/// The name a history line belongs to, as the player knows it.
fn whose(id: &str, games: &[Game], recomps: &[RecompEntry]) -> String {
    if id == "emulation" {
        return "RetroArch".into();
    }
    if let Some(path) = id.strip_prefix("classic:") {
        let file = path.rsplit(['\\', '/']).next().unwrap_or(path);
        let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
        return stem.split(" (").next().unwrap_or(stem).trim().to_string();
    }
    if let Some(game) = games.iter().find(|g| g.id == id) {
        return game.name.clone();
    }
    let rid = id.strip_prefix("recomp:").unwrap_or(id);
    recomps.iter().find(|e| e.id == rid).map(|e| e.name.clone()).unwrap_or_else(|| id.to_string())
}

/// Where a history line leads: its game's page, when there is one.
fn page_of(id: &str, games: &[Game]) -> Option<Route> {
    if games.iter().any(|g| g.id == id) {
        return Some(Route::Game(id.to_string()));
    }
    if let Some(path) = id.strip_prefix("classic:") {
        return Some(Route::Classic(path.to_string()));
    }
    id.starts_with("recomp:").then(|| Route::Game(id.to_string()))
}

/// The Steam games to ask news for: the ones played most lately first, then by play time.
fn news_games(games: &[Game]) -> Vec<(u32, String)> {
    let mut steam: Vec<&Game> = games.iter().filter(|g| !g.hidden && matches!(g.source, Source::Steam { .. })).collect();
    steam.sort_by(|a, b| b.last_played.cmp(&a.last_played).then(b.playtime_secs.cmp(&a.playtime_secs)));
    steam
        .into_iter()
        .filter_map(|g| match g.source {
            Source::Steam { app_id } => Some((app_id, g.name.clone())),
            _ => None,
        })
        .take(12)
        .collect()
}

/// Posts by the developers of the player's Steam games.
#[component]
fn SteamNews() -> impl IntoView {
    let library = expect_context::<Library>();
    let wanted = Memo::new(move |_| library.games.get().and_then(Result::ok).map(|g| news_games(&g)));
    let news = LocalResource::new(move || {
        let games = wanted.get();
        async move {
            match games {
                Some(list) if !list.is_empty() => api::steam_news(list).await.ok(),
                Some(_) => Some(Vec::new()),
                None => None,
            }
        }
    });
    move || {
        let Some(list) = news.get().flatten() else {
            return view! { <p class="muted" role="status">{tr!("Procurando notícias…")}</p> }.into_any();
        };
        if list.is_empty() {
            return view! { <p class="muted">{tr!("Nenhuma notícia recente dos seus jogos da Steam.")}</p> }.into_any();
        }
        let games = library.games.get().and_then(Result::ok).unwrap_or_default();
        let cards = list
            .into_iter()
            .map(|n| {
                let game = games.iter().find(|g| matches!(g.source, Source::Steam { app_id } if app_id == n.app_id)).cloned();
                let art = game.as_ref().map(cover_sources).unwrap_or_default();
                let url = n.url.clone();
                let open = move |_| {
                    let url = url.clone();
                    leptos::task::spawn_local(async move {
                        library.report(api::open_news(url).await);
                    });
                };
                view! {
                    <button type="button" class="news-card" on:click=open title=tr!("Abrir no navegador")>
                        <div class="news-art"><Poster sources=art title=n.game.clone() /></div>
                        <div class="news-t">
                            <small>{n.game.clone()}" · "{format::ago(n.date)}</small>
                            <b>{n.title.clone()}</b>
                            <span>{n.summary.clone()}</span>
                        </div>
                    </button>
                }
            })
            .collect_view();
        view! { <div class="news-grid">{cards}</div> }.into_any()
    }
}

#[component]
pub fn NewsPage() -> impl IntoView {
    let library = expect_context::<Library>();
    let activity = LocalResource::new(|| async { api::recent_activity().await.unwrap_or_default() });

    let waiting = move || {
        let games = library.games.get().and_then(Result::ok).unwrap_or_default();
        let recomps = library.recomps.get().and_then(Result::ok).unwrap_or_default();
        let list = updates(&games, &recomps);
        if list.is_empty() {
            return view! { <p class="muted">{tr!("Todos os seus recomps estão na versão mais nova.")}</p> }.into_any();
        }
        let rows = list
            .into_iter()
            .map(|u| {
                let route = Route::Game(u.game.id.clone());
                let watching = StoredValue::new(u.recomp_id.clone());
                let busy = move || library.progress.with(|p| p.contains_key(&watching.get_value()));
                let rid = u.recomp_id.clone();
                view! {
                    <div class="dl-row card">
                        <div class="thumb"><Poster sources=cover_sources(&u.game) title=u.game.name.clone() /></div>
                        <div class="dl-info">
                            <b>{u.game.name.clone()}</b>
                            <span class="muted">{format!("{} → {}", u.from, u.to)}</span>
                        </div>
                        <div class="row">
                            <button type="button" class="btn sm" on:click=move |_| library.go(route.clone())>{tr!("Ver o que mudou")}</button>
                            <button type="button" class="btn sm main" disabled=busy
                                on:click=move |_| crate::discover::start_install(library, rid.clone())>
                                {move || if busy() { tr!("Atualizando...") } else { tr!("Atualizar") }}
                            </button>
                        </div>
                    </div>
                }
            })
            .collect_view();
        view! { <div class="dl-list">{rows}</div> }.into_any()
    };

    let lately = move || {
        let Some(list) = activity.get() else {
            return view! { <p class="muted" role="status">{tr!("Carregando...")}</p> }.into_any();
        };
        if list.is_empty() {
            return view! { <p class="muted">{tr!("Nada por aqui ainda. Instalações, partidas e conquistas aparecem aqui.")}</p> }.into_any();
        }
        let games = library.games.get().and_then(Result::ok).unwrap_or_default();
        let recomps = library.recomps.get().and_then(Result::ok).unwrap_or_default();
        let rows = list
            .into_iter()
            .map(|e: TrailEntry| {
                let name = whose(&e.game_id, &games, &recomps);
                let icon = if !e.ok { Icon::Info } else if e.step == "Conquista" { Icon::Trophy } else { Icon::Check };
                let open = page_of(&e.game_id, &games);
                let clickable = open.is_some();
                let go = move |_| {
                    if let Some(route) = open.clone() {
                        library.go(route);
                    }
                };
                view! {
                    <li class="act" class:bad=!e.ok class:clickable=clickable on:click=go>
                        <Svg icon size=18 />
                        <div>
                            <b>{format!("{} · {}", name, cerberus_shared::i18n::translate(&e.step))}</b>
                            <span>{e.message.clone()}</span>
                        </div>
                        <time>{format::ago(e.at)}</time>
                    </li>
                }
            })
            .collect_view();
        view! { <ul class="activity card">{rows}</ul> }.into_any()
    };

    view! {
        <div class="page">
            <header class="page-h">
                <div>
                    <h1>{tr!("Novidades")}</h1>
                    <p class="muted">{tr!("Atualizações dos seus recomps e o que aconteceu com os seus jogos.")}</p>
                </div>
            </header>
            <section class="news-sec">
                <h2><Svg icon=Icon::Download />{tr!("Atualizações disponíveis")}</h2>
                {waiting}
            </section>
            <section class="news-sec">
                <h2><Svg icon=Icon::Bell />{tr!("Notícias dos seus jogos")}</h2>
                <SteamNews />
            </section>
            <section class="news-sec">
                <h2><Svg icon=Icon::Clock />{tr!("Atividade recente")}</h2>
                {lately}
            </section>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(id: &str, version: &str, external: bool) -> Game {
        serde_json::from_value(serde_json::json!({
            "id": format!("recomp:{id}"), "name": id,
            "source": {"kind": "recomp", "recomp_id": id, "version": version, "exe": "x.exe", "generation": 0},
            "install_dir": "x", "external": external
        }))
        .unwrap()
    }


    #[test]
    fn only_cerberus_installs_behind_their_newest_release_are_updates() {
        let games = vec![game("a", "1.0", false), game("b", "2.0", false), game("c", "1.0", true)];
        let newest = |id: &str| Some(match id { "a" => "1.1", "b" => "2.0", _ => "9.9" }.to_string());
        let list = behind(&games, newest);
        let recomps: Vec<RecompEntry> = Vec::new();
        assert_eq!(list.len(), 1);
        assert_eq!((list[0].recomp_id.as_str(), list[0].from.as_str(), list[0].to.as_str()), ("a", "1.0", "1.1"));
        assert_eq!(whose("classic:D:/ROMS/N64/Mario Kart 64 (USA).z64", &games, &recomps), "Mario Kart 64");
        assert_eq!(whose("emulation", &games, &recomps), "RetroArch");
    }
}
