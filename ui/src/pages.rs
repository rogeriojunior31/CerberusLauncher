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
        // The game's name first, as everywhere else; the recomp's follows it.
        entry_of(rid)
            .map(|e| crate::tiles::entry_names(&e))
            .or_else(|| library.games.with_untracked(|g| g.as_ref().and_then(|r| r.as_ref().ok()).and_then(|all| all.iter().find(|x| x.id == id).map(|x| (x.name.clone(), None)))))
            .unwrap_or_else(|| (rid.to_string(), None))
    };

    // Installing now: the first one big, with speed and time left; the rest quiet.
    let active = move || {
        let mut jobs: Vec<(String, InstallStep)> = library.progress.get().into_iter().collect();
        jobs.sort_by_key(|(id, step)| (matches!(step, InstallStep::Paused { .. }), id.clone()));
        if jobs.is_empty() {
            return None;
        }
        let count = jobs.len();
        // With more than one: pause every download that has bytes to keep, or carry on with every paused one.
        let running: Vec<String> = jobs.iter().filter(|(_, s)| matches!(s, InstallStep::Downloading { done, .. } if *done > 0)).map(|(id, _)| id.clone()).collect();
        let waiting: Vec<String> = jobs.iter().filter(|(_, s)| matches!(s, InstallStep::Paused { .. })).map(|(id, _)| id.clone()).collect();
        let all_buttons = (count > 1).then(|| {
            let pause = (!running.is_empty()).then(|| {
                view! { <button type="button" class="btn sm" on:click=move |_| { for id in running.clone() { crate::discover::pause_install(library, id); } }>
                    <Svg icon=Icon::Pause size=16 />{tr!("Pausar tudo")}</button> }
            });
            let resume = (!waiting.is_empty()).then(|| {
                view! { <button type="button" class="btn sm main" on:click=move |_| { for id in waiting.clone() { crate::discover::start_install(library, id); } }>
                    <Svg icon=Icon::Play size=16 />{tr!("Continuar tudo")}</button> }
            });
            view! { <div class="row">{pause}{resume}</div> }
        });
        let rows = jobs
            .into_iter()
            .enumerate()
            .map(|(i, (id, step))| {
                let entry = entry_of(&id);
                let name = entry.as_ref().map(|e| crate::tiles::entry_names(e).0).unwrap_or_else(|| id.clone());
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
                // Waiting in line: it can go first.
                let first = matches!(step, InstallStep::Queued).then(|| {
                    let id = id.clone();
                    view! {
                        <button type="button" class="btn sm" on:click=move |_| {
                            let id = id.clone();
                            leptos::task::spawn_local(async move {
                                if library.report(api::download_first(&id).await) == Some(true) {
                                    library.toast(tr!("Esse baixa logo depois do download de agora."));
                                }
                            });
                        }>{tr!("Baixar primeiro")}</button>
                    }
                });
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
                        InstallStep::Queued => tr!("Na fila: começa quando o download de agora terminar").to_string(),
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
                                {first}
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
                <div class="shelf-h"><h2>{tr!("Baixando")}{(count > 1).then(|| view! { <span class="chip-n">{count}</span> })}</h2>{all_buttons}</div>
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
            let Some(entry) = entries.iter().find(|e| e.is(recomp_id)) else { continue };
            let Some(latest) = entry.latest.as_ref().filter(|l| cerberus_shared::is_update(version, &l.version)) else { continue };
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
                        <b>{crate::tiles::game_names(&g, library).0}</b>
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
                    let name = name_of(&id).0;
                    let (again, dismiss) = (id.clone(), id.clone());
                    view! {
                        <div class="dl-row card dl-failed">
                            <Svg icon=Icon::Info size=22 />
                            <div class="dl-info"><b>{name}</b><span class="problem">{cerberus_shared::i18n::translate(&why)}</span></div>
                            <button type="button" class="btn sm main" on:click=move |_| crate::discover::start_install(library, again.clone())>{tr!("Tentar de novo")}</button>
                            <button type="button" class="btn sm icon-btn" aria-label=tr!("Dispensar") on:click=move |_| {
                                library.failed.update(|m| { m.remove(&dismiss); });
                                let id = dismiss.clone();
                                leptos::task::spawn_local(async move { let _ = api::forget_download(id).await; });
                            }>
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
                                {let (game, recomp) = name_of(&e.game_id); view! { <b>{game}{recomp.map(|v| view! { <small class="sv-ver">{v}</small> })}</b> }}
                                <span>{cerberus_shared::i18n::translate(&e.message)}</span>
                                <small>{time}</small>
                            </button>
                        }
                    })
                    .collect_view();
                view! { <h3 class="dl-day">{day}</h3><div class="dl-hist-list">{rows}</div> }
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
    let settings = LocalResource::new(api::get_settings);
    let open = RwSignal::new(None::<String>);
    // Where copies go besides this PC, and anything still waiting to get there.
    let cloud = move || {
        let s = settings.get().and_then(Result::ok)?;
        let banner = match (&s.cloud_dir, s.pending_uploads) {
            (None, _) => view! {
                <div class="card sv-cloud off">
                    <Svg icon=Icon::Saves size=24 />
                    <div>
                        <b>{tr!("Só neste PC")}</b>
                        <p class="muted small">{tr!("Escolha uma pasta do OneDrive, Google Drive ou Dropbox e as cópias vão também para lá, prontas para outro PC.")}</p>
                    </div>
                    <button type="button" class="btn sm main" on:click=move |_| spawn_local(async move {
                        if library.report(api::set_cloud_dir(false).await).is_some() {
                            settings.refetch();
                        }
                    })>{tr!("Escolher pasta")}</button>
                </div>
            }
            .into_any(),
            (Some(dir), 0) => view! {
                <div class="card sv-cloud ok">
                    <Svg icon=Icon::Check size=24 />
                    <div>
                        <b>{tr!("Na nuvem")}</b>
                        <p class="muted small path">{dir.clone()}</p>
                    </div>
                    <span class="pill ok" title=tr!("Copiado para a pasta sincronizada; o aplicativo dela (OneDrive, Google Drive, Dropbox) leva para a internet.")>{tr!("Tudo copiado para a pasta")}</span>
                </div>
            }
            .into_any(),
            (Some(dir), n) => view! {
                <div class="card sv-cloud wait">
                    <Svg icon=Icon::Info size=24 />
                    <div>
                        <b>{if n == 1 { tr!("1 cópia esperando a pasta da nuvem").to_string() } else { tr!("{} cópias esperando a pasta da nuvem", n) }}</b>
                        <p class="muted small path">{dir.clone()}</p>
                    </div>
                    <button type="button" class="btn sm" on:click=move |_| spawn_local(async move {
                        if library.report(api::retry_save_uploads().await).is_some() {
                            settings.refetch();
                        }
                    })>{tr!("Tentar enviar")}</button>
                </div>
            }
            .into_any(),
        };
        Some(banner)
    };
    let rows = move || {
        let games = library.games.get().and_then(Result::ok).unwrap_or_default();
        let recomps: Vec<Game> = games.into_iter().filter(|g| matches!(g.source, Source::Recomp { .. })).collect();
        if recomps.is_empty() {
            return view! {
                <div class="card dl-empty">
                    <Svg icon=Icon::Saves size=28 />
                    <div>
                        <b>{tr!("Nenhum recomp instalado")}</b>
                        <p class="muted small">{tr!("Os saves dos recomps são copiados sozinhos: quando você fecha o jogo, antes de atualizar e antes de desinstalar.")}</p>
                    </div>
                </div>
            }
            .into_any();
        }
        let cloud_on = settings.get().and_then(Result::ok).is_some_and(|s| s.cloud_dir.is_some());
        let list = recomps.into_iter().map(|game| view! { <SaveRow game open cloud_on /> }).collect_view();
        view! { <div class="sv-list">{list}</div> }.into_any()
    };
    view! {
        <div class="page dl-page">
            <header class="page-h">
                <div>
                    <h1>{tr!("Saves")}</h1>
                    <p class="muted">{tr!("Cópias automáticas dos saves: dos recomps ao fechar, antes de atualizar e antes de desinstalar; dos emuladores sempre que um clássico fecha. As 10 mais recentes ficam guardadas. Jogos da Steam guardam os saves na nuvem da Steam.")}</p>
                </div>
            </header>
            {cloud}
            {rows}
            <EmulatorSaves />
        </div>
    }
}

/// The classics' saves: one card per emulator on this PC, with the copies made
/// after each classic closed.
#[component]
fn EmulatorSaves() -> impl IntoView {
    let found = LocalResource::new(|| async { api::emulator_backups().await.unwrap_or_default() });
    move || {
        let list = found.get().unwrap_or_default();
        (!list.is_empty()).then(|| {
            let cards = list
                .into_iter()
                .map(|e| {
                    let note = tr!("Memory cards e estados salvos de {}. Os 10 backups mais recentes ficam guardados.", e.folder);
                    view! { <crate::backups::Backups game_id=e.id emulator=true title=e.name note /> }
                })
                .collect_view();
            view! {
                <section class="shelf">
                    <div class="shelf-h"><h2>{tr!("Clássicos")}</h2></div>
                    <p class="muted small sv-emu-note">{tr!("Cada emulador guarda os saves de todos os seus jogos juntos; o Cerberus copia essa pasta sempre que um clássico fecha.")}</p>
                    <div class="sv-emu">{cards}</div>
                </section>
            }
        })
    }
}

/// A game's saves in one line (status, last copy, size); opens into its history.
#[component]
fn SaveRow(game: Game, open: RwSignal<Option<String>>, cloud_on: bool) -> impl IntoView {
    let library = expect_context::<Library>();
    let (title, version) = crate::tiles::game_names(&game, library);
    let id = game.id.clone();
    let backups = LocalResource::new(move || {
        let id = id.clone();
        async move { api::list_backups(&id).await }
    });
    let gid = StoredValue::new(game.id.clone());
    let is_open = move || open.get().as_deref() == Some(gid.get_value().as_str());
    let summary = move || match backups.get() {
        None => (tr!("Carregando…").to_string(), "", String::new()),
        Some(Err(error)) => (error, "bad", String::new()),
        Some(Ok(list)) => match list.first() {
            None => (tr!("Ainda sem backup").to_string(), "none", String::new()),
            Some(newest) => {
                let bytes: u64 = list.iter().map(|b| b.bytes).sum();
                let count = if list.len() == 1 { tr!("1 cópia").to_string() } else { tr!("{} cópias", list.len()) };
                // What the newest copy really is, as the backend found it.
                let (where_, tone) = match (cloud_on, newest.in_cloud, newest.waiting) {
                    (true, Some(true), _) => (tr!("Na pasta da nuvem"), "ok"),
                    (true, _, true) => (tr!("Esperando a pasta da nuvem"), "wait"),
                    _ => (tr!("Só neste PC"), "local"),
                };
                (tr!("{} · última {}", where_, format::ago(newest.created)), tone, format!("{count} · {}", megabytes(bytes)))
            }
        },
    };
    view! {
        <div class="card sv-row" class:open=is_open>
            <button type="button" class="sv-head" aria-expanded=move || is_open().to_string()
                on:click=move |_| open.update(|o| *o = if o.as_deref() == Some(gid.get_value().as_str()) { None } else { Some(gid.get_value()) })>
                <div class="thumb"><Poster sources=cover_sources(&game) title=title.clone() /></div>
                <div class="dl-info">
                    <b>{title.clone()}{version.map(|v| view! { <small class="sv-ver">{v}</small> })}</b>
                    {move || { let (text, tone, _) = summary(); view! { <span class=format!("sv-status {tone}")><i></i>{text}</span> } }}
                </div>
                <small class="muted">{move || summary().2}</small>
                <span class="sv-chev"><Svg icon=Icon::Right size=18 /></span>
            </button>
            {move || is_open().then(|| view! { <div class="sv-body"><crate::backups::Backups game_id=gid.get_value() /></div> })}
        </div>
    }
}
