//! Novidades: what changed since the player last looked. Updates waiting for
//! their recomps, what went wrong and wasn't seen yet, the developers' news,
//! and then the history (installs, sessions, achievements) by day, from the
//! same record each game's page shows. Only updates and problems are news: the
//! player's own sessions aren't.

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

/// What happened lately, shared by Novidades and the side menu's counter, and
/// when the player last marked it as read (kept on this PC).
#[derive(Clone, Copy)]
pub struct Activity {
    pub list: LocalResource<Vec<TrailEntry>>,
    pub seen: RwSignal<u64>,
}

const SEEN_KEY: &str = "cerberus-news-seen";

impl Activity {
    /// Reads the history now and again every minute (it's a small local file).
    pub fn start() -> Self {
        let list = LocalResource::new(|| async { api::recent_activity().await.unwrap_or_default() });
        let seen = web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .and_then(|s| s.get_item(SEEN_KEY).ok().flatten())
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let _ = set_interval_with_handle(move || list.refetch(), std::time::Duration::from_secs(60));
        Self { list, seen: RwSignal::new(seen) }
    }

    /// What went wrong after the last "Entendi", newest first.
    pub fn problems(self) -> Vec<Row> {
        let seen = self.seen.get();
        self.list.get().map(|l| fold(&l).into_iter().filter(|r| !r.entry.ok && r.entry.at > seen).collect()).unwrap_or_default()
    }

    /// How many problems the side menu counts.
    pub fn unread(self) -> usize {
        self.problems().len()
    }

    pub fn mark_read(self) {
        let newest = self.list.get_untracked().and_then(|l| l.iter().map(|e| e.at).max()).unwrap_or(0);
        let now = (js_sys::Date::now() / 1000.0) as u64;
        let at = newest.max(now);
        if let Some(s) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
            let _ = s.set_item(SEEN_KEY, &at.to_string());
        }
        self.seen.set(at);
    }
}

/// One line of the activity: a history entry, the backup that came with a
/// session, and how many times the same thing happened in a row.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub entry: TrailEntry,
    pub backup: Option<String>,
    pub count: usize,
    /// The oldest of the repeats, for the time window.
    oldest: u64,
}

const SESSION_STEP: &str = "Partida";
const BACKUP_STEP: &str = "Backup depois de jogar";
const CLASSIC_STEP: &str = "Jogar clássico";
/// A backup that found nothing to copy says nothing worth a line.
const NOTHING_NEW: &str = "Nada novo para copiar";

/// The history (newest first) folded for reading: a session and its backup
/// become one line, and the same result for the same game within a day is
/// counted instead of repeated.
pub fn fold(list: &[TrailEntry]) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    for e in list {
        // A classic opened and then played is the session's line alone.
        let played = |r: &Row| r.entry.game_id == e.game_id && r.entry.step == SESSION_STEP && (e.at..=e.at + 6 * 3600).contains(&r.entry.at);
        if e.ok && e.step == CLASSIC_STEP && rows.iter().any(played) {
            continue;
        }
        if let Some(r) = rows.iter_mut().find(|r| {
            r.entry.game_id == e.game_id && r.entry.step == e.step && r.entry.message == e.message && r.entry.ok == e.ok && r.oldest.saturating_sub(e.at) <= 24 * 3600
        }) {
            r.count += 1;
            r.oldest = e.at;
            continue;
        }
        let pairs = |r: &Row| {
            r.entry.game_id == e.game_id
                && r.entry.at.abs_diff(e.at) <= 120
                && ((r.entry.step == BACKUP_STEP && e.step == SESSION_STEP) || (r.entry.step == SESSION_STEP && e.step == BACKUP_STEP && r.backup.is_none()))
        };
        if let Some(r) = rows.iter_mut().find(|r| pairs(r)) {
            if e.step == SESSION_STEP {
                // The backup line was newer: the session takes its place, keeping it as a detail.
                let backup = std::mem::replace(&mut r.entry, e.clone());
                r.backup = Some(backup.message);
                r.entry.ok = r.entry.ok && backup.ok;
            } else {
                r.backup = Some(e.message.clone());
                r.entry.ok = r.entry.ok && e.ok;
            }
            continue;
        }
        rows.push(Row { entry: e.clone(), backup: None, count: 1, oldest: e.at });
    }
    rows
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
    // A recomp goes by its game's name ("RollerCoaster Tycoon 2", not "OpenRCT2"), as in the library.
    let rid = id.strip_prefix("recomp:").unwrap_or(id);
    let entry = recomps.iter().find(|e| e.id == rid);
    if let Some(game) = games.iter().find(|g| g.id == id) {
        return match game.source {
            Source::Recomp { .. } => crate::tiles::names_of(&game.name, entry).0,
            _ => game.name.clone(),
        };
    }
    entry.map(|e| crate::tiles::entry_names(e).0).unwrap_or_else(|| id.to_string())
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
    let shown = RwSignal::new(6usize);
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
        let total = list.len();
        let cards = list
            .into_iter()
            .take(shown.get())
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
        let more = (total > shown.get()).then(|| view! {
            <button type="button" class="btn sm more" on:click=move |_| shown.set(usize::MAX)>
                {tr!("Mostrar mais ({})", total - shown.get_untracked())}
            </button>
        });
        view! { <div class="news-grid">{cards}</div>{more} }.into_any()
    }
}

/// The local day of a moment, as a key ("Tue Nov 24 2026") and as words
/// ("Hoje", "Ontem", "segunda-feira, 16 de novembro").
fn day_of(seconds: u64) -> (String, String) {
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(seconds as f64 * 1000.0));
    let key: String = date.to_date_string().into();
    let today = js_sys::Date::new_0();
    let yesterday = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(today.get_time() - 86_400_000.0));
    let words = if key == String::from(today.to_date_string()) {
        tr!("Hoje").to_string()
    } else if key == String::from(yesterday.to_date_string()) {
        tr!("Ontem").to_string()
    } else {
        let options = js_sys::Object::new();
        let set = |k: &str, v: &str| js_sys::Reflect::set(&options, &k.into(), &v.into());
        let _ = set("weekday", "long");
        let _ = set("day", "numeric");
        let _ = set("month", "long");
        let locale = if cerberus_shared::i18n::lang() == cerberus_shared::i18n::Lang::En { "en-US" } else { "pt-BR" };
        let text: String = date.to_locale_date_string(locale, &options).into();
        let mut chars = text.chars();
        chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
    };
    (key, words)
}

/// The clock time of a moment, "14:05".
fn clock(seconds: u64) -> String {
    format::date_time(seconds).rsplit(", ").next().unwrap_or_default().to_string()
}

/// One history line; `when` is its time ("14:05", or "há 3 h" outside a day group).
fn activity_row(library: Library, row: Row, when: String, games: &[Game], recomps: &[RecompEntry]) -> impl IntoView {
    let e = row.entry;
    // Lines written before the singular was fixed.
    let mut detail = match e.message.strip_prefix("1 min jogados") {
        Some(rest) => format!("1 min jogado{rest}"),
        None => e.message.clone(),
    };
    if let Some(b) = row.backup.as_ref().filter(|b| b.as_str() != NOTHING_NEW && b.as_str() != tr!("Nada novo para copiar")) {
        detail = tr!("{} · backup: {}", detail, b);
    }
    if row.count > 1 {
        detail = tr!("{} ({}x)", detail, row.count);
    }
    let tip = detail.clone();
    let name = whose(&e.game_id, games, recomps);
    let icon = if !e.ok {
        Icon::Info
    } else if e.step == "Conquista" {
        Icon::Trophy
    } else if e.step == SESSION_STEP || e.step == "Jogar clássico" {
        Icon::Play
    } else if e.step.starts_with("Instal") {
        Icon::Download
    } else {
        Icon::Check
    };
    let open = page_of(&e.game_id, games);
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
                <span title=tip>{detail}</span>
            </div>
            <time>{when}</time>
        </li>
    }
}

/// History lines shown before "Mostrar mais".
const HISTORY_PAGE: usize = 12;

#[component]
pub fn NewsPage() -> impl IntoView {
    let library = expect_context::<Library>();
    let news = expect_context::<Activity>();
    news.list.refetch();
    let activity = news.list;

    let waiting = move || {
        let games = library.games.get().and_then(Result::ok).unwrap_or_default();
        let recomps = library.recomps.get().and_then(Result::ok).unwrap_or_default();
        let list = updates(&games, &recomps);
        if list.is_empty() {
            return ().into_any();
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
        view! {
            <section class="news-sec">
                <h2><Svg icon=Icon::Download />{tr!("Atualizações disponíveis")}</h2>
                <div class="dl-list">{rows}</div>
            </section>
        }
        .into_any()
    };
    let all_current = move || {
        let games = library.games.get().and_then(Result::ok).unwrap_or_default();
        let recomps = library.recomps.get().and_then(Result::ok).unwrap_or_default();
        updates(&games, &recomps).is_empty() && news.problems().is_empty()
    };

    // What went wrong and wasn't seen: the only activity that is news.
    let attention = move || {
        let problems = news.problems();
        if problems.is_empty() {
            return ().into_any();
        }
        let games = library.games.get().and_then(Result::ok).unwrap_or_default();
        let recomps = library.recomps.get().and_then(Result::ok).unwrap_or_default();
        let rows = problems.into_iter().map(|row| {
            let when = format::ago(row.entry.at);
            activity_row(library, row, when, &games, &recomps)
        }).collect_view();
        view! {
            <section class="news-sec">
                <div class="news-h">
                    <h2><Svg icon=Icon::Info />{tr!("Precisa de atenção")}</h2>
                    <button type="button" class="btn sm" on:click=move |_| news.mark_read()><Svg icon=Icon::Check size=16 />{tr!("Entendi")}</button>
                </div>
                <ul class="activity card">{rows}</ul>
            </section>
        }
        .into_any()
    };

    // The history, by day; it's a record to look back on, not news.
    let shown = RwSignal::new(HISTORY_PAGE);
    let lately = move || {
        let Some(list) = activity.get() else {
            return view! { <p class="muted" role="status">{tr!("Carregando...")}</p> }.into_any();
        };
        if list.is_empty() {
            return view! { <p class="muted">{tr!("Nada por aqui ainda. Instalações, partidas e conquistas aparecem aqui.")}</p> }.into_any();
        }
        let games = library.games.get().and_then(Result::ok).unwrap_or_default();
        let recomps = library.recomps.get().and_then(Result::ok).unwrap_or_default();
        let all = fold(&list);
        let total = all.len();
        let mut days: Vec<(String, String, Vec<Row>)> = Vec::new();
        for row in all.into_iter().take(shown.get()) {
            let (key, words) = day_of(row.entry.at);
            match days.last_mut() {
                Some((k, _, rows)) if *k == key => rows.push(row),
                _ => days.push((key, words, vec![row])),
            }
        }
        let groups = days.into_iter().map(|(_, words, rows)| {
            let rows = rows.into_iter().map(|row| {
                let when = clock(row.entry.at);
                activity_row(library, row, when, &games, &recomps)
            }).collect_view();
            view! {
                <div class="act-day">
                    <h3>{words}</h3>
                    <ul class="activity card">{rows}</ul>
                </div>
            }
        }).collect_view();
        let more = (total > shown.get()).then(|| view! {
            <button type="button" class="btn sm more" on:click=move |_| shown.set(shown.get_untracked() + HISTORY_PAGE * 2)>
                {tr!("Mostrar mais ({})", total - shown.get_untracked())}
            </button>
        });
        view! { {groups}{more} }.into_any()
    };

    view! {
        <div class="page">
            <header class="page-h">
                <div>
                    <h1>{tr!("Novidades")}</h1>
                    <p class="muted">{tr!("Atualizações dos seus recomps, o que precisa da sua atenção e as notícias dos seus jogos.")}</p>
                    <Show when=all_current>
                        <p class="news-ok"><Svg icon=Icon::Check size=16 />{tr!("Tudo em dia: seus recomps estão na versão mais nova e nada precisa da sua atenção.")}</p>
                    </Show>
                </div>
            </header>
            {waiting}
            {attention}
            <section class="news-sec">
                <h2><Svg icon=Icon::Bell />{tr!("Notícias dos seus jogos")}</h2>
                <SteamNews />
            </section>
            <section class="news-sec">
                <h2><Svg icon=Icon::Clock />{tr!("Seu histórico")}</h2>
                {lately}
            </section>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(at: u64, game: &str, step: &str, ok: bool, message: &str) -> TrailEntry {
        TrailEntry { at, game_id: game.into(), step: step.into(), ok, message: message.into() }
    }

    #[test]
    fn a_classic_opened_and_played_is_one_line() {
        let list = vec![
            step(2000, "classic:c", "Partida", true, "5 min"),
            step(1700, "classic:c", "Jogar clássico", true, "Aberto no DuckStation"),
            step(900, "classic:x", "Jogar clássico", false, "Sem emulador"),
        ];
        let rows = fold(&list);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].entry.step, "Partida");
        assert!(!rows[1].entry.ok, "a classic that didn't open stays");
    }

    #[test]
    fn a_session_and_its_backup_are_one_line_and_repeats_are_counted() {
        // Newest first, as the history comes.
        let list = vec![
            step(1000, "g", "O jogo fechou com erro", false, "código 2"),
            step(900, "g", "O jogo fechou com erro", false, "código 2"),
            step(800, "g", BACKUP_STEP, true, "50 arquivos copiados"),
            step(798, "g", SESSION_STEP, true, "3 min jogados"),
            step(500, "h", SESSION_STEP, true, "1 h jogada"),
            step(10, "g", BACKUP_STEP, true, "2 arquivos copiados"),
        ];
        let rows = fold(&list);
        assert_eq!(rows.len(), 4);
        assert_eq!((rows[0].entry.step.as_str(), rows[0].count), ("O jogo fechou com erro", 2));
        assert_eq!(rows[1].entry.step, SESSION_STEP);
        assert_eq!(rows[1].backup.as_deref(), Some("50 arquivos copiados"));
        assert_eq!(rows[1].entry.at, 798);
        assert_eq!((rows[2].entry.game_id.as_str(), rows[2].backup.as_deref()), ("h", None));
        assert_eq!(rows[3].entry.step, BACKUP_STEP, "a backup far from any session stays on its own");
    }

    #[test]
    fn the_same_thing_a_day_apart_is_not_folded() {
        let day = 24 * 3600;
        let list = vec![step(3 * day, "g", "Instalação", true, "v1"), step(day, "g", "Instalação", true, "v1")];
        assert_eq!(fold(&list).len(), 2);
    }

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
