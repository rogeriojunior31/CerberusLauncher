//! Downloads and Saves: the two lists the sidebar gives a page of their own.

use cerberus_shared::tr;
use cerberus_shared::{Game, InstallStep, Source};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::art::{Poster, cover_sources};
use crate::discover::progress_view;
use crate::icons::{Icon, Svg};
use crate::format::megabytes;
use crate::{Library, Route, api, format};

/// Bytes per second in words: "48 MB/s", "820 KB/s".
fn rate_text(bps: f64) -> String {
    if bps >= 1_048_576.0 {
        format!("{:.1} MB/s", bps / 1_048_576.0)
    } else {
        format!("{:.0} KB/s", bps / 1024.0)
    }
}

/// Seconds left in words: "2 min", "40 s", "1 h 5 min".
fn left_text(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    match s {
        0..60 => format!("{s} s"),
        60..3600 => format!("{} min", s / 60),
        _ => format!("{} h {} min", s / 3600, s % 3600 / 60),
    }
}

/// The speed over the last half minute as a small line.
fn sparkline(samples: &[f64]) -> impl IntoView + use<> {
    let (w, h) = (160.0, 40.0);
    let top = samples.iter().copied().fold(1.0, f64::max);
    let n = samples.len().max(2) as f64 - 1.0;
    let points: Vec<String> = samples.iter().enumerate().map(|(i, v)| format!("{:.1},{:.1}", i as f64 / n * w, h - v / top * (h - 4.0) - 2.0)).collect();
    let area = format!("0,{h} {} {w},{h}", points.join(" "));
    view! {
        <svg class="dl-spark" viewBox=format!("0 0 {w} {h}") preserveAspectRatio="none" aria-hidden="true">
            <polygon points=area></polygon>
            <polyline points=points.join(" ")></polyline>
        </svg>
    }
}

/// What's installing, ready to update, didn't finish, and what was done lately.
#[component]
pub fn DownloadsPage() -> impl IntoView {
    let library = expect_context::<Library>();
    let recent = LocalResource::new(move || {
        library.games.track();
        api::recent_activity()
    });
    let entry_of = move |id: &str| library.recomps.with(|r| r.as_ref().and_then(|r| r.as_ref().ok()).and_then(|all| all.iter().find(|e| e.id == id).cloned()));
    let name_of = move |id: &str| {
        let rid = id.strip_prefix("recomp:").unwrap_or(id);
        entry_of(rid)
            .map(|e| e.name)
            .or_else(|| library.games.with_untracked(|g| g.as_ref().and_then(|r| r.as_ref().ok()).and_then(|all| all.iter().find(|x| x.id == id).map(|x| x.name.clone()))))
            .unwrap_or_else(|| rid.to_string())
    };

    // Installing now: the first one big, with speed and time left; the rest quiet.
    let active = move || {
        let mut jobs: Vec<(String, InstallStep)> = library.progress.get().into_iter().collect();
        jobs.sort_by_key(|(id, step)| (matches!(step, InstallStep::Paused { .. }), id.clone()));
        if jobs.is_empty() {
            return None;
        }
        let count = jobs.len();
        let rows = jobs
            .into_iter()
            .enumerate()
            .map(|(i, (id, step))| {
                let entry = entry_of(&id);
                let name = entry.as_ref().map(|e| e.name.clone()).unwrap_or_else(|| id.clone());
                if i > 0 {
                    let covers = entry.as_ref().map(crate::art::recomp_covers).unwrap_or_default();
                    return view! {
                        <div class="dl-row card">
                            <div class="thumb"><Poster sources=covers title=name.clone() /></div>
                            <div class="dl-info"><b>{name}</b>{progress_view(step, id)}</div>
                        </div>
                    }
                    .into_any();
                }
                let heroes = entry.as_ref().map(crate::art::recomp_heroes).filter(|h| !h.is_empty()).or_else(|| entry.as_ref().map(crate::art::recomp_covers)).unwrap_or_default();
                let speed_id = id.clone();
                let samples = move || library.speed.with(|all| all.get(&speed_id).cloned().unwrap_or_default());
                let stats = {
                    let samples = samples.clone();
                    move || {
                        let s = samples();
                        let now = s.iter().rev().take(4).sum::<f64>() / s.iter().rev().take(4).count().max(1) as f64;
                        let peak = s.iter().copied().fold(0.0, f64::max);
                        (now, peak)
                    }
                };
                let paused = matches!(step, InstallStep::Paused { .. });
                let pct = match step {
                    InstallStep::Downloading { done, total } | InstallStep::Paused { done, total } if total > 0 => Some(done.min(total) * 100 / total),
                    _ => None,
                };
                let line_step = step.clone();
                let line = {
                    let stats = stats.clone();
                    move || match line_step.clone() {
                        InstallStep::Downloading { done, total } if total > 0 => {
                            let (now, _) = stats();
                            let mut parts = vec![tr!("{} de {}", megabytes(done), megabytes(total))];
                            if now > 0.0 {
                                parts.push(rate_text(now));
                                parts.push(tr!("{} restantes", left_text(total.saturating_sub(done) as f64 / now)));
                            }
                            parts.join(" · ")
                        }
                        InstallStep::Downloading { .. } => tr!("Iniciando download…").to_string(),
                        InstallStep::Paused { done, total } => tr!("Pausado · {} de {}", megabytes(done), megabytes(total)),
                        InstallStep::Unpacking => tr!("Preparando arquivos…").to_string(),
                        _ => String::new(),
                    }
                };
                view! {
                    <div class="dl-hero">
                        <div class="dl-hero-bg"><crate::art::Backdrop sources=heroes soft=true /></div>
                        <div class="dl-hero-body">
                            <div class="dl-hero-main">
                                <span class="kick">{if paused { tr!("Pausado") } else { tr!("Baixando agora") }}</span>
                                <h2>{name}</h2>
                                <p class="dl-line">{line}</p>
                                <div class="dl-bar" class:busy=pct.is_none()><i style=pct.map(|p| format!("width: {p}%")).unwrap_or_default()></i></div>
                                {progress_view(step, id.clone())}
                            </div>
                            <div class="dl-hero-side">
                                <b class="dl-pct">{pct.map(|p| format!("{p}%")).unwrap_or_else(|| "…".into())}</b>
                                {move || { let s = samples(); (s.len() > 2).then(|| sparkline(&s)) }}
                                {move || { let (_, peak) = stats(); (peak > 0.0).then(|| view! { <small class="muted">{tr!("Pico {}", rate_text(peak))}</small> }) }}
                            </div>
                        </div>
                    </div>
                }
                .into_any()
            })
            .collect_view();
        Some(view! {
            <section class="shelf">
                <div class="shelf-h"><h2>{tr!("Baixando")}{(count > 1).then(|| view! { <span class="chip-n">{count}</span> })}</h2></div>
                <div class="dl-list">{rows}</div>
            </section>
        })
    };

    // Installed recomps with a newer release; paused ones apart.
    let updates = move || {
        let games = library.games.get().and_then(Result::ok).unwrap_or_default();
        let entries = library.recomps.get().and_then(Result::ok).unwrap_or_default();
        let busy = library.progress.get();
        let mut ready = Vec::new();
        let mut held = Vec::new();
        for g in games {
            let Source::Recomp { recomp_id, version, .. } = &g.source else { continue };
            if g.external || busy.contains_key(recomp_id) {
                continue;
            }
            let Some(entry) = entries.iter().find(|e| &e.id == recomp_id) else { continue };
            let Some(latest) = entry.latest.as_ref().filter(|l| &l.version != version) else { continue };
            let row = (g.clone(), entry.clone(), version.clone(), latest.clone());
            if g.updates_paused { held.push(row) } else { ready.push(row) }
        }
        if ready.is_empty() && held.is_empty() {
            return None;
        }
        let ids: Vec<String> = ready.iter().map(|(_, e, _, _)| e.id.clone()).collect();
        let update_all = (ids.len() >= 2).then(|| {
            let ids = ids.clone();
            view! {
                <button type="button" class="btn sm main" on:click=move |_| {
                    let ids = ids.clone();
                    spawn_local(async move {
                        for id in ids {
                            crate::discover::install_one(library, id).await;
                        }
                    });
                }><Svg icon=Icon::Download size=16 />{tr!("Atualizar todos")}</button>
            }
        });
        let row = move |(g, e, from, to): (Game, cerberus_shared::RecompEntry, String, cerberus_shared::Release), held: bool| {
            let (rid, gid, page) = (e.id.clone(), g.id.clone(), Route::Game(g.id.clone()));
            let action = if held {
                view! {
                    <button type="button" class="btn sm" on:click=move |_| {
                        let gid = gid.clone();
                        spawn_local(async move {
                            if library.report(api::set_updates_paused(&gid, false).await).is_some() {
                                library.games.refetch();
                            }
                        });
                    }>{tr!("Retomar atualizações")}</button>
                }
                .into_any()
            } else {
                view! { <button type="button" class="btn sm main" on:click=move |_| crate::discover::start_install(library, rid.clone())>{tr!("Atualizar")}</button> }.into_any()
            };
            view! {
                <div class="dl-row card">
                    <button type="button" class="thumb" on:click=move |_| library.go(page.clone())><Poster sources=crate::art::recomp_covers(&e) title=g.name.clone() /></button>
                    <div class="dl-info">
                        <b>{g.name.clone()}</b>
                        <span class="dl-ver">{from}<Svg icon=Icon::Right size=14 /><em>{to.version.clone()}</em>
                            {(to.size > 0).then(|| view! { <small class="muted">{format!("· {}", megabytes(to.size))}</small> })}
                        </span>
                    </div>
                    {action}
                </div>
            }
        };
        let ready_rows = ready.into_iter().map(|r| row(r, false)).collect_view();
        let held_rows = (!held.is_empty()).then(|| {
            let rows = held.into_iter().map(|r| row(r, true)).collect_view();
            view! { <p class="muted small dl-sub">{tr!("Atualizações pausadas: o jogo fica na versão atual até você retomar.")}</p>{rows} }
        });
        Some(view! {
            <section class="shelf">
                <div class="shelf-h"><h2>{tr!("Atualizações prontas")}<span class="chip-n">{ids.len()}</span></h2>{update_all}</div>
                <div class="dl-list">{ready_rows}{held_rows}</div>
            </section>
        })
    };

    // Didn't finish this session: why, and try again.
    let failed = move || {
        let all = library.failed.get();
        (!all.is_empty()).then(|| {
            let rows = all
                .into_iter()
                .map(|(id, why)| {
                    let name = name_of(&id);
                    let (again, dismiss) = (id.clone(), id.clone());
                    view! {
                        <div class="dl-row card dl-failed">
                            <Svg icon=Icon::Info size=22 />
                            <div class="dl-info"><b>{name}</b><span class="problem">{cerberus_shared::i18n::translate(&why)}</span></div>
                            <button type="button" class="btn sm main" on:click=move |_| crate::discover::start_install(library, again.clone())>{tr!("Tentar de novo")}</button>
                            <button type="button" class="btn sm icon-btn" aria-label=tr!("Dispensar") on:click=move |_| library.failed.update(|m| { m.remove(&dismiss); })>
                                <Svg icon=Icon::Close size=16 />
                            </button>
                        </div>
                    }
                })
                .collect_view();
            view! {
                <section class="shelf">
                    <div class="shelf-h"><h2>{tr!("Não terminou")}</h2></div>
                    <div class="dl-list">{rows}</div>
                </section>
            }
        })
    };

    // Installs and updates lately, by day.
    let history = move || {
        let list: Vec<cerberus_shared::TrailEntry> = recent.get().and_then(Result::ok).unwrap_or_default().into_iter().filter(|e| e.step == "Instalação").take(30).collect();
        if list.is_empty() {
            return None;
        }
        let mut groups: Vec<(String, Vec<cerberus_shared::TrailEntry>)> = Vec::new();
        for e in list {
            let day = day_label(e.at);
            match groups.last_mut() {
                Some((d, items)) if *d == day => items.push(e),
                _ => groups.push((day, vec![e])),
            }
        }
        let body = groups
            .into_iter()
            .map(|(day, items)| {
                let rows = items
                    .into_iter()
                    .map(|e| {
                        let page = Route::Game(e.game_id.clone());
                        let time = format::date_time(e.at).split(' ').last().unwrap_or_default().to_string();
                        view! {
                            <button type="button" class="dl-hist" class:bad=!e.ok on:click=move |_| library.go(page.clone())>
                                <Svg icon=if e.ok { Icon::Check } else { Icon::Info } size=16 />
                                <b>{name_of(&e.game_id)}</b>
                                <span>{cerberus_shared::i18n::translate(&e.message)}</span>
                                <small>{time}</small>
                            </button>
                        }
                    })
                    .collect_view();
                view! { <h4 class="dl-day">{day}</h4><div class="dl-hist-list">{rows}</div> }
            })
            .collect_view();
        Some(view! {
            <section class="shelf">
                <div class="shelf-h"><h2>{tr!("Recentes")}</h2></div>
                <div class="card dl-history">{body}</div>
            </section>
        })
    };

    let nothing = move || {
        let empty = library.progress.with(|p| p.is_empty()) && library.failed.with(|f| f.is_empty());
        empty.then(|| view! {
            <div class="card dl-empty">
                <Svg icon=Icon::Download size=28 />
                <div>
                    <b>{tr!("Nada baixando agora")}</b>
                    <p class="muted small">{tr!("Instale um recomp em Descobrir e o download aparece aqui, com velocidade e tempo restante.")}</p>
                </div>
                <button type="button" class="btn" on:click=move |_| library.go(Route::Discover)>{tr!("Ir para Descobrir")}</button>
            </div>
        })
    };

    view! {
        <div class="page dl-page">
            <header class="page-h">
                <div>
                    <h1>{tr!("Downloads")}</h1>
                    <p class="muted">{tr!("Instalações e atualizações dos recomps, conferidas pelo arquivo publicado na release.")}</p>
                </div>
            </header>
            {nothing}
            {active}
            {failed}
            {updates}
            {history}
        </div>
    }
}

/// "Hoje", "Ontem" or the date, for grouping by day.
fn day_label(at: u64) -> String {
    let ms = at as f64 * 1000.0;
    let start = |t: f64| {
        let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(t));
        d.set_hours(0);
        d.set_minutes(0);
        d.set_seconds(0);
        d.set_milliseconds(0);
        d.get_time()
    };
    let today = start(js_sys::Date::now());
    let days = ((today - start(ms)) / 86_400_000.0).round() as i64;
    match days {
        0 => tr!("Hoje").to_string(),
        1 => tr!("Ontem").to_string(),
        _ => format::date_time(at).split(' ').next().unwrap_or_default().trim_end_matches(',').to_string(),
    }
}

#[component]
pub fn SavesPage() -> impl IntoView {
    let library = expect_context::<Library>();
    let rows = move || {
        let games = library.games.get().and_then(Result::ok).unwrap_or_default();
        let recomps: Vec<Game> = games.into_iter().filter(|g| matches!(g.source, Source::Recomp { .. })).collect();
        if recomps.is_empty() {
            return view! {
                <div class="empty">
                    <Svg icon=Icon::Saves size=40 />
                    <h2>{tr!("Nenhum recomp instalado")}</h2>
                    <p class="muted">{tr!("Os saves dos recomps são copiados sozinhos: quando você fecha o jogo, antes de atualizar e antes de desinstalar.")}</p>
                </div>
            }
            .into_any();
        }
        let list = recomps.into_iter().map(|game| view! { <SaveRow game /> }).collect_view();
        view! { <div class="dl-list">{list}</div> }.into_any()
    };
    view! {
        <div class="page">
            <header class="page-h">
                <div>
                    <h1>{tr!("Saves")}</h1>
                    <p class="muted">{tr!("Backups automáticos dos recomps. Os 10 mais recentes de cada jogo ficam guardados.")}</p>
                </div>
            </header>
            {rows}
        </div>
    }
}

#[component]
fn SaveRow(game: Game) -> impl IntoView {
    let library = expect_context::<Library>();
    let id = game.id.clone();
    let backups = LocalResource::new(move || {
        let id = id.clone();
        async move { api::list_backups(&id).await }
    });
    let summary = move || match backups.get() {
        None => tr!("Carregando...").to_string(),
        Some(Err(error)) => error,
        Some(Ok(list)) => match list.first() {
            None => tr!("Nenhum backup ainda").to_string(),
            Some(newest) => tr!("{} backup(s) · último {}", list.len(), format::ago(newest.created)),
        },
    };
    let route = Route::Game(game.id.clone());
    view! {
        <button type="button" class="dl-row card clickable" on:click=move |_| library.go(route.clone())>
            <div class="thumb"><Poster sources=cover_sources(&game) title=game.name.clone() /></div>
            <div class="dl-info">
                <b>{game.name.clone()}</b>
                <span class="muted">{summary}</span>
            </div>
            <Svg icon=Icon::Right />
        </button>
    }
}
