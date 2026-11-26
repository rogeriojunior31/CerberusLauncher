//! The parts of a game's Overview tab that every kind of game shares: your
//! play time (with the last two weeks drawn), the store's pictures and
//! trailers, the description, and the side rail (achievements, details,
//! controller, requirements, languages, links).

use cerberus_shared::{PlaySession, StoreDetails, StoreMovie, tr};
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;

use crate::icons::{Icon, Svg};
use crate::{Library, api, format};

const DAY: f64 = 86_400_000.0;

/// Midnight (local time) of the day a JS timestamp falls on, in ms.
fn day_start(ms: f64) -> f64 {
    let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ms));
    d.set_hours(0);
    d.set_minutes(0);
    d.set_seconds(0);
    d.set_milliseconds(0);
    d.get_time()
}

fn weekday_letter(ms: f64) -> String {
    let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ms));
    let options = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&options, &"weekday".into(), &"narrow".into());
    d.to_locale_date_string(if crate::i18n::is_english() { "en-US" } else { "pt-BR" }, &options).into()
}

fn day_label(ms: f64) -> String {
    let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ms));
    let options = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&options, &"weekday".into(), &"short".into());
    let _ = js_sys::Reflect::set(&options, &"day".into(), &"numeric".into());
    let _ = js_sys::Reflect::set(&options, &"month".into(), &"short".into());
    d.to_locale_date_string(if crate::i18n::is_english() { "en-US" } else { "pt-BR" }, &options).into()
}

/// "Your play time": the last 14 days as bars, and the numbers beside them.
#[component]
pub fn PlayCard(id: String, total: u64, last: Option<u64>, #[prop(optional)] title: Option<&'static str>) -> impl IntoView {
    let title = title.unwrap_or(tr!("Sua jogatina"));
    let sessions = LocalResource::new(move || {
        let id = id.clone();
        async move { api::game_sessions(&id).await.unwrap_or_default() }
    });
    move || {
        let Some(list) = sessions.get() else {
            return view! { <section class="card play-card"><h3>{title}</h3><div class="sk sk-play"></div></section> }.into_any();
        };
        let list: Vec<PlaySession> = list;
        let now = js_sys::Date::now();
        let today = day_start(now);
        let mut days = [0u64; 14];
        let (mut week, mut month, mut longest) = (0u64, 0u64, 0u64);
        for s in &list {
            let at = s.ended as f64 * 1000.0;
            let age = now - at;
            if age <= 7.0 * DAY {
                week += s.seconds;
            }
            if age <= 30.0 * DAY {
                month += s.seconds;
            }
            longest = longest.max(s.seconds);
            let back = ((today - day_start(at)) / DAY).round() as i64;
            if (0..14).contains(&back) {
                days[13 - back as usize] += s.seconds;
            }
        }
        let busiest = days.iter().copied().max().unwrap_or(0).max(60);
        let any_recent = days.iter().any(|d| *d > 0);
        let bars = (0..14)
            .map(|i| {
                let ms = today - (13 - i) as f64 * DAY;
                let secs = days[i];
                let height = if secs == 0 { 0.0 } else { (secs as f64 / busiest as f64 * 100.0).max(6.0) };
                let tip = if secs == 0 { tr!("{}: não jogou", day_label(ms)) } else { format!("{}: {}", day_label(ms), format::playtime(secs)) };
                view! {
                    <div class="pc-day" title=tip.clone() aria-label=tip>
                        <div class="pc-col">
                            <i class:zero=secs == 0 style=format!("height: {height:.1}%")></i>
                        </div>
                        <small>{weekday_letter(ms)}</small>
                    </div>
                }
            })
            .collect_view();
        let rows: Vec<(Icon, String, String)> = [
            (Icon::Clock, tr!("Total").to_string(), // Zero with a last time: played, just under a minute (Steam counts whole minutes).
            if total == 0 && last.is_none() { tr!("Nunca jogado").to_string() } else { format::playtime(total) }),
            (Icon::Play, tr!("Última vez").to_string(), last.map(format::ago).unwrap_or_else(|| "—".into())),
            (Icon::List, tr!("Esta semana").to_string(), if week == 0 { "—".into() } else { format::playtime(week) }),
            (Icon::Layers, tr!("Este mês").to_string(), if month == 0 { "—".into() } else { format::playtime(month) }),
            (Icon::Gamepad, tr!("Sessões").to_string(), if list.is_empty() { "—".into() } else { list.len().to_string() }),
            (Icon::Star, tr!("Mais longa").to_string(), if longest == 0 { "—".into() } else { format::playtime(longest) }),
        ]
        .into_iter()
        .collect();
        let rows = rows
            .into_iter()
            .map(|(icon, label, value)| view! { <li><Svg icon size=16 /><span>{label}</span><b>{value}</b></li> })
            .collect_view();
        // The total also holds time from before sessions were written down one by one.
        let logged: u64 = list.iter().map(|s| s.seconds).sum();
        let older = (total > logged + 60).then(|| view! {
            <p class="muted small pc-older">{tr!("O total inclui partidas mais antigas, de antes do registro por sessão.")}</p>
        });
        // One shape for every game. With no sessions written down in two weeks, the
        // chart says why, and doesn't contradict a recent "Última vez" (a Steam game
        // played outside Cerberus counts only in the total).
        let played_lately = last.is_some_and(|t| now - (t as f64 * 1000.0) <= 14.0 * DAY);
        let empty_note = (!any_recent).then(|| {
            let text = if played_lately {
                tr!("Jogou nesses dias, mas fora do Cerberus: só o total conta essas partidas.")
            } else {
                tr!("Não jogou nas últimas duas semanas.")
            };
            view! { <p class="pc-empty muted small">{text}</p> }
        });
        view! {
            <section class="card pc">
                <div class="pc-h">
                    <h3>{title}</h3>
                    <small class="muted">{tr!("Últimas duas semanas")}</small>
                </div>
                <div class="pc-body">
                    <div class="pc-chart" class:empty=!any_recent><div class="pc-bars">{bars}</div>{empty_note}</div>
                    <ul class="pc-list">{rows}</ul>
                </div>
                {older}
            </section>
        }
        .into_any()
    }
}

/// Loads hls.js once (bundled with the app) and plays an HLS stream in `video`,
/// through `vendor/hls-play.js` (no code built from text, so the page's CSP
/// needs no 'unsafe-eval').
fn play_hls(video: &web_sys::HtmlElement, url: &str) {
    use wasm_bindgen::JsCast;
    let Some(window) = web_sys::window() else { return };
    let call = move |window: &web_sys::Window| {
        if let Ok(f) = js_sys::Reflect::get(window, &"cerberusPlayHls".into()).and_then(|f| f.dyn_into::<js_sys::Function>()) {
            let _ = f.call2(&wasm_bindgen::JsValue::NULL, video, &url.into());
            return true;
        }
        false
    };
    if call(&window) {
        return;
    }
    // First trailer: the helper comes in, then plays.
    let Some(document) = window.document() else { return };
    let Ok(script) = document.create_element("script") else { return };
    let _ = script.set_attribute("src", "vendor/hls-play.js");
    let (video, url) = (video.clone(), url.to_string());
    let loaded = wasm_bindgen::closure::Closure::once_into_js(move || {
        if let Some(window) = web_sys::window() {
            if let Ok(f) = js_sys::Reflect::get(&window, &"cerberusPlayHls".into()).and_then(|f| f.dyn_into::<js_sys::Function>()) {
                let _ = f.call2(&wasm_bindgen::JsValue::NULL, &video, &url.into());
            }
        }
    });
    let _ = script.add_event_listener_with_callback("load", loaded.unchecked_ref());
    if let Some(head) = document.head() {
        let _ = head.append_child(&script);
    }
}

#[derive(Clone, PartialEq)]
enum Media {
    Movie(StoreMovie),
    Shot { thumb: String, full: String },
}

/// Trailers first, then pictures: a 16:9 viewer, a strip of thumbnails, and a
/// full-screen view for pictures (arrows and Esc).
#[component]
pub fn Gallery(details: StoreDetails) -> impl IntoView {
    let mut items: Vec<Media> = details.movies.iter().cloned().map(Media::Movie).collect();
    items.extend(details.screenshots.iter().map(|s| Media::Shot { thumb: s.thumb.clone(), full: s.full.clone() }));
    if items.is_empty() {
        return ().into_any();
    }
    let count = items.len();
    // Steam's trailer thumbnails are small; a full-size picture of the game stands in.
    let poster = details.screenshots.first().map(|s| s.full.clone());
    let items = StoredValue::new(items);
    let at = RwSignal::new(0usize);
    let playing = RwSignal::new(false);
    let full = RwSignal::new(false);
    let video_ref = NodeRef::<leptos::html::Video>::new();
    let go = move |to: usize| {
        playing.set(false);
        at.set(to.min(count - 1));
        let strip = document().query_selector(&format!(".gal-strip button:nth-child({})", to + 1)).ok().flatten();
        if let Some(el) = strip {
            let opts = web_sys::ScrollIntoViewOptions::new();
            opts.set_behavior(web_sys::ScrollBehavior::Smooth);
            opts.set_block(web_sys::ScrollLogicalPosition::Nearest);
            opts.set_inline(web_sys::ScrollLogicalPosition::Center);
            el.scroll_into_view_with_scroll_into_view_options(&opts);
        }
    };
    let step = move |d: i32| go(((at.get_untracked() as i32 + d).rem_euclid(count as i32)) as usize);
    // Leaving the page stops the trailer's stream too.
    on_cleanup(move || {
        if let Some(video) = video_ref.get_untracked() {
            let el: &wasm_bindgen::JsValue = video.as_ref();
            if let Ok(h) = js_sys::Reflect::get(el, &"__hls".into()) {
                if let Ok(f) = js_sys::Reflect::get(&h, &"destroy".into()).and_then(|f| f.dyn_into::<js_sys::Function>().map_err(|e| e.into())) {
                    let _ = f.call0(&h);
                }
            }
        }
    });
    let start_movie = move |m: StoreMovie| {
        playing.set(true);
        request_animation_frame(move || {
            let Some(video) = video_ref.get() else { return };
            let el: &web_sys::HtmlElement = video.unchecked_ref();
            match (&m.mp4, &m.hls) {
                (Some(mp4), _) => {
                    let _ = el.set_attribute("src", mp4);
                    let _ = js_sys::Reflect::get(el, &"play".into()).ok().and_then(|f| f.dyn_into::<js_sys::Function>().ok()).map(|f| f.call0(el));
                }
                (None, Some(hls)) => play_hls(el, hls),
                _ => {}
            }
        });
    };
    let stage = move || {
        let item = items.with_value(|i| i[at.get()].clone());
        match item {
            Media::Movie(m) => {
                if playing.get() {
                    view! { <video node_ref=video_ref class="gal-media" controls=true playsinline=true poster=m.thumb.clone()></video> }.into_any()
                } else {
                    let m2 = m.clone();
                    let src = poster.clone().unwrap_or_else(|| m.thumb.clone());
                    view! {
                        <button type="button" class="gal-poster" aria-label=tr!("Ver trailer") on:click=move |_| start_movie(m2.clone())>
                            <img class="gal-media dim" src=src alt="" />
                            <span class="gal-play"><Svg icon=Icon::Play size=30 /></span>
                            <span class="gal-cap">{m.name.clone()}</span>
                        </button>
                    }
                    .into_any()
                }
            }
            Media::Shot { full: src, .. } => view! {
                <button type="button" class="gal-poster" aria-label=tr!("Ver em tela cheia") on:click=move |_| full.set(true)>
                    <img class="gal-media" src=src alt="" />
                </button>
            }
            .into_any(),
        }
    };
    let strip = move || {
        items.with_value(|all| {
            all.iter()
                .enumerate()
                .map(|(i, item)| {
                    let (thumb, movie) = match item {
                        Media::Movie(m) => (m.thumb.clone(), true),
                        Media::Shot { thumb, .. } => (thumb.clone(), false),
                    };
                    view! {
                        <button type="button" class="gal-thumb" class:on=move || at.get() == i on:click=move |_| go(i)>
                            <img src=thumb alt="" loading="lazy" />
                            {movie.then(|| view! { <span class="gal-tplay"><Svg icon=Icon::Play size=16 /></span> })}
                        </button>
                    }
                })
                .collect_view()
        })
    };
    let lightbox = move || {
        full.get().then(|| {
            let src = items.with_value(|i| match &i[at.get()] {
                Media::Shot { full, .. } => full.clone(),
                Media::Movie(m) => m.thumb.clone(),
            });
            view! {
                <div class="lightbox" role="dialog" aria-modal="true" tabindex="-1"
                    on:click=move |_| full.set(false)
                    on:keydown=move |e| match e.key().as_str() {
                        "Escape" => full.set(false),
                        "ArrowRight" => step(1),
                        "ArrowLeft" => step(-1),
                        _ => {}
                    }>
                    <img src=src alt="" on:click=|e| e.stop_propagation() />
                    <button type="button" class="lb-nav prev" aria-label=tr!("Anterior") on:click=move |e| { e.stop_propagation(); step(-1) }><Svg icon=Icon::Left size=22 /></button>
                    <button type="button" class="lb-nav next" aria-label=tr!("Próxima") on:click=move |e| { e.stop_propagation(); step(1) }><Svg icon=Icon::Right size=22 /></button>
                    <span class="lb-count">{move || format!("{} / {}", at.get() + 1, count)}</span>
                    <button type="button" class="lb-close" aria-label=tr!("Fechar") on:click=move |_| full.set(false)><Svg icon=Icon::Close size=20 /></button>
                </div>
            }
        })
    };
    Effect::new(move || {
        if full.get() {
            request_animation_frame(|| {
                if let Some(el) = document().query_selector(".lightbox").ok().flatten() {
                    let _ = el.unchecked_ref::<web_sys::HtmlElement>().focus();
                }
            });
        }
    });
    view! {
        <section class="gal" aria-label=tr!("Imagens e trailers")>
            <div class="gal-stage">
                {stage}
                {(count > 1).then(|| view! {
                    <button type="button" class="gal-nav prev" aria-label=tr!("Anterior") on:click=move |_| step(-1)><Svg icon=Icon::Left size=20 /></button>
                    <button type="button" class="gal-nav next" aria-label=tr!("Próxima") on:click=move |_| step(1)><Svg icon=Icon::Right size=20 /></button>
                })}
            </div>
            {(count > 1).then(|| view! { <div class="gal-strip">{strip}</div> })}
            {lightbox}
        </section>
    }
    .into_any()
}

/// The store's description: clamped, with "Show more".
#[component]
pub fn About(details: StoreDetails) -> impl IntoView {
    if details.about.is_empty() && details.short.is_empty() {
        return ().into_any();
    }
    let open = RwSignal::new(false);
    let long = details.about.iter().map(|b| b.text.len()).sum::<usize>() > 700 || details.about.iter().any(|b| b.kind == "img");
    let blocks = if details.about.is_empty() {
        view! { <p>{details.short.clone()}</p> }.into_any()
    } else {
        details
            .about
            .iter()
            .map(|b| match b.kind.as_str() {
                "h" => view! { <h4>{b.text.clone()}</h4> }.into_any(),
                "li" => view! { <p class="li">{b.text.clone()}</p> }.into_any(),
                "img" => view! { <img src=b.src.clone().unwrap_or_default() alt="" loading="lazy" /> }.into_any(),
                _ => view! { <p>{b.text.clone()}</p> }.into_any(),
            })
            .collect_view()
            .into_any()
    };
    view! {
        <section class="card about-card">
            <h3>{tr!("Sobre o jogo")}</h3>
            <div class="about-body" class:clamped=move || long && !open.get()>{blocks}</div>
            {long.then(|| view! {
                <button type="button" class="btn sm" on:click=move |_| open.update(|o| *o = !*o)>
                    {move || if open.get() { tr!("Mostrar menos") } else { tr!("Mostrar mais") }}
                </button>
            })}
            <small class="muted">{tr!("Da página da Steam.")}</small>
        </section>
    }
    .into_any()
}

/// The rail's achievements card: progress and the latest unlocks.
#[component]
pub fn AchievementsCard(data: LocalResource<Result<cerberus_shared::GameAchievements, String>>, #[prop(into)] on_all: Callback<()>) -> impl IntoView {
    move || match data.get() {
        None => view! {
            <section class="card rail-card">
                <h3>{tr!("Conquistas")}</h3>
                <div class="sk sk-line"></div><div class="sk sk-row"></div><div class="sk sk-row"></div><div class="sk sk-row"></div>
            </section>
        }
        .into_any(),
        Some(Err(_)) => ().into_any(),
        Some(Ok(d)) if d.achievements.is_empty() => ().into_any(),
        Some(Ok(d)) => {
            let g = d.game.clone();
            let mut list = d.achievements.clone();
            list.sort_by(|a, b| b.unlocked_at.cmp(&a.unlocked_at));
            let latest: Vec<_> = list.into_iter().take(4).collect();
            let pct = g.percent();
            let rows = latest
                .into_iter()
                .map(|a| {
                    let unlocked = a.unlocked_at.is_some();
                    let secret = a.hidden && !unlocked;
                    let icon = if unlocked { a.icon.clone() } else { a.icon_locked.clone().or(a.icon.clone()) };
                    view! {
                        <li class:locked=!unlocked>
                            {match icon.filter(|_| !secret) {
                                Some(src) => view! { <img src=src alt="" loading="lazy" /> }.into_any(),
                                None => view! { <span class="ach-ph"><Svg icon=Icon::EyeOff size=18 /></span> }.into_any(),
                            }}
                            <span>
                                <b>{if secret { tr!("Conquista secreta").to_string() } else { a.name.clone() }}</b>
                                <small>{match (a.unlocked_at, a.rarity) {
                                    (Some(t), Some(r)) => format!("{} · {}", format::ago(t), tr!("{}% dos jogadores", format::decimal(format!("{r:.1}")))),
                                    (Some(t), None) => format::ago(t),
                                    (None, Some(r)) => tr!("Bloqueada · {}% dos jogadores", format::decimal(format!("{r:.1}"))),
                                    (None, None) => tr!("Bloqueada").to_string(),
                                }}</small>
                            </span>
                        </li>
                    }
                })
                .collect_view();
            view! {
                <section class="card rail-card">
                    <div class="rail-h"><h3>{tr!("Conquistas")}</h3><b class="rail-n">{format!("{}/{}", g.unlocked, g.total)}</b></div>
                    <div class="ach-bar"><i style=format!("width: {pct}%") class:done=pct == 100></i></div>
                    <ul class="rail-ach">{rows}</ul>
                    <button type="button" class="btn sm" on:click=move |_| on_all.run(())>{tr!("Ver todas")}</button>
                </section>
            }
            .into_any()
        }
    }
}

/// Facts from the store: release, studio, genres, score.
#[component]
pub fn StoreFacts(details: StoreDetails, #[prop(optional)] extra: Vec<(String, String)>) -> impl IntoView {
    let mut rows: Vec<(String, String)> = Vec::new();
    if !details.released.is_empty() {
        let released = store_day(&details.released);
        rows.push((tr!("Lançamento").to_string(), if details.coming_soon { tr!("{} (em breve)", released) } else { released }));
    }
    if !details.developers.is_empty() {
        rows.push((tr!("Desenvolvedora").to_string(), details.developers.iter().take(3).cloned().collect::<Vec<_>>().join(", ")));
    }
    if !details.publishers.is_empty() {
        rows.push((tr!("Publicadora").to_string(), details.publishers.iter().take(3).cloned().collect::<Vec<_>>().join(", ")));
    }
    if !details.genres.is_empty() {
        rows.push((tr!("Gêneros").to_string(), details.genres.iter().take(4).cloned().collect::<Vec<_>>().join(", ")));
    }
    if let Some(score) = details.metacritic {
        rows.push(("Metacritic".to_string(), format!("{score}/100")));
    }
    if let Some(n) = details.recommendations {
        rows.push((tr!("Análises na Steam").to_string(), format::count(n)));
    }
    rows.extend(extra);
    let features = details
        .features
        .iter()
        .take(6)
        .map(|f| view! { <span class="tag">{f.clone()}</span> })
        .collect_view();
    view! {
        <section class="card rail-card">
            <h3>{tr!("Detalhes")}</h3>
            <dl class="facts">{rows.into_iter().map(|(k, v)| view! { <div><dt>{k}</dt><dd>{v}</dd></div> }).collect_view()}</dl>
            {(!details.features.is_empty()).then(|| view! { <div class="tags">{features}</div> })}
        </section>
    }
}

/// Controller support, from the store.
#[component]
pub fn Controller(details: StoreDetails) -> impl IntoView {
    let (label, text, class) = match details.controller.as_str() {
        "full" => (tr!("Suporte total"), tr!("Dá para jogar do começo ao fim com o controle."), "ok"),
        "partial" => (tr!("Suporte parcial"), tr!("Algumas partes pedem teclado e mouse."), "soon"),
        _ => return ().into_any(),
    };
    view! {
        <section class="card rail-card">
            <div class="rail-h"><h3>{tr!("Controle")}</h3><span class=format!("pill {class}")><Svg icon=Icon::Gamepad size=14 />{label}</span></div>
            <p class="muted small">{text}</p>
        </section>
    }
    .into_any()
}

/// PC requirements, minimum and recommended.
#[component]
pub fn Requirements(details: StoreDetails) -> impl IntoView {
    if details.minimum.is_empty() && details.recommended.is_empty() {
        return ().into_any();
    }
    let recommended = RwSignal::new(false);
    let has_rec = !details.recommended.is_empty();
    let (min, rec) = (details.minimum.clone(), details.recommended.clone());
    let rows = move || {
        let list = if recommended.get() && has_rec { rec.clone() } else { min.clone() };
        list.into_iter()
            .map(|(k, v)| {
                if k.is_empty() {
                    view! { <div class="req-note">{v}</div> }.into_any()
                } else {
                    view! { <div><dt>{k}</dt><dd>{v}</dd></div> }.into_any()
                }
            })
            .collect_view()
    };
    view! {
        <section class="card rail-card">
            <div class="rail-h">
                <h3>{tr!("Requisitos")}</h3>
                {has_rec.then(|| view! {
                    <div class="segs mini" role="tablist">
                        <button type="button" class="seg" class:on=move || !recommended.get() on:click=move |_| recommended.set(false)>{tr!("Mínimos")}</button>
                        <button type="button" class="seg" class:on=move || recommended.get() on:click=move |_| recommended.set(true)>{tr!("Recomendados")}</button>
                    </div>
                })}
            </div>
            <dl class="facts req">{rows}</dl>
        </section>
    }
    .into_any()
}

/// Languages: interface for all, a mark for full audio.
#[component]
pub fn Languages(details: StoreDetails) -> impl IntoView {
    if details.languages.is_empty() {
        return ().into_any();
    }
    let all = details.languages.clone();
    let open = RwSignal::new(false);
    let more = all.len() > 6;
    let rows = move || {
        let shown = if open.get() { all.len() } else { all.len().min(6) };
        all.iter()
            .take(shown)
            .map(|l| {
                view! {
                    <tr>
                        <td>{l.name.clone()}</td>
                        <td class="ok"><Svg icon=Icon::Check size=15 /></td>
                        <td class:ok=l.audio>{if l.audio { view! { <Svg icon=Icon::Check size=15 /> }.into_any() } else { view! { <span class="dim">"—"</span> }.into_any() }}</td>
                    </tr>
                }
            })
            .collect_view()
    };
    view! {
        <section class="card rail-card">
            <div class="rail-h"><h3>{tr!("Idiomas")}</h3><small class="muted">{all_count(&details)}</small></div>
            <table class="langs">
                <thead><tr><th>{tr!("Idioma")}</th><th>{tr!("Texto")}</th><th>{tr!("Áudio")}</th></tr></thead>
                <tbody>{rows}</tbody>
            </table>
            {more.then(|| view! {
                <button type="button" class="link" on:click=move |_| open.update(|o| *o = !*o)>
                    {move || if open.get() { tr!("Mostrar menos") } else { tr!("Mostrar todos") }}
                </button>
            })}
        </section>
    }
    .into_any()
}

fn all_count(d: &StoreDetails) -> String {
    tr!("{} idiomas", d.languages.len())
}

/// Links that open in the browser: the store page, SteamDB, PCGamingWiki, the site.
#[component]
pub fn Links(app: u32, has_site: bool) -> impl IntoView {
    let library = expect_context::<Library>();
    let open = move |kind: &'static str| spawn_local(async move { library.report(api::open_store_link(app, kind).await); });
    view! {
        <section class="card rail-card">
            <h3>{tr!("Links")}</h3>
            <div class="rail-links">
                <button type="button" class="btn sm" on:click=move |_| open("store")><Svg icon=Icon::External size=15 />{tr!("Loja da Steam")}</button>
                <button type="button" class="btn sm" on:click=move |_| open("pcgw")><Svg icon=Icon::External size=15 />"PCGamingWiki"</button>
                <button type="button" class="btn sm" on:click=move |_| open("steamdb")><Svg icon=Icon::External size=15 />"SteamDB"</button>
                {has_site.then(|| view! { <button type="button" class="btn sm" on:click=move |_| open("site")><Svg icon=Icon::External size=15 />{tr!("Site oficial")}</button> })}
            </div>
        </section>
    }
}

/// Placeholders at the final height while the store page loads.
#[component]
pub fn StoreSkeleton() -> impl IntoView {
    view! {
        <div class="sk sk-media" role="status" aria-label=tr!("Carregando a página da loja")></div>
        <div class="sk sk-strip"></div>
        <div class="sk sk-card"></div>
    }
}

/// Something still loading, at its final size: placeholder rows, the words for
/// screen readers, and after a while a note that it's still going.
#[component]
pub fn Waiting(text: &'static str, #[prop(default = 3)] rows: usize, #[prop(optional)] retry: Option<Callback<()>>) -> impl IntoView {
    // Seconds since this read (or its retry) began: slow at 8, given up at 30.
    let elapsed = RwSignal::new(0u32);
    let tick = set_interval_with_handle(move || elapsed.set(elapsed.get_untracked() + 1), std::time::Duration::from_secs(1)).ok();
    on_cleanup(move || {
        if let Some(t) = tick {
            t.clear();
        }
    });
    let stuck = move || elapsed.get() >= 30;
    let again = move || {
        elapsed.set(0);
        if let Some(r) = retry {
            r.run(());
        }
    };
    view! {
        <div class="waiting" role="status">
            <span class="vh">{text}</span>
            <Show when=move || !stuck()>
                {(0..rows).map(|_| view! { <div class="sk sk-row"></div> }).collect_view()}
            </Show>
            <Show when=move || { elapsed.get() >= 8 }>
                <div class="wait-note">
                    <p class="muted small">{move || if stuck() {
                        tr!("Não deu para ler agora. A pasta do jogo pode estar ocupada (uma atualização, o próprio jogo aberto).")
                    } else {
                        tr!("Está demorando mais que o normal.")
                    }}</p>
                    {retry.is_some().then(|| view! { <button type="button" class="btn sm" on:click=move |_| again()>{tr!("Tentar de novo")}</button> })}
                </div>
            </Show>
        </div>
    }
}


/// The store's date ("10/set./2019", "10 Sep, 2019") in the same numbers as the
/// rest of the app ("10/09/2019"); anything else ("Em breve") as the store wrote it.
fn store_day(text: &str) -> String {
    const MONTHS: [[&str; 2]; 12] = [
        ["jan", "jan"], ["fev", "feb"], ["mar", "mar"], ["abr", "apr"], ["mai", "may"], ["jun", "jun"],
        ["jul", "jul"], ["ago", "aug"], ["set", "sep"], ["out", "oct"], ["nov", "nov"], ["dez", "dec"],
    ];
    let words: Vec<String> = text
        .split(|c: char| c == '/' || c == ' ' || c == ',' || c == '.')
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    let month_of = |w: &str| MONTHS.iter().position(|m| m.iter().any(|n| w.starts_with(n))).map(|i| i + 1);
    let (day, month, year) = match words.as_slice() {
        [d, m, y] if d.parse::<u32>().is_ok() => (d.clone(), month_of(m), y.clone()),
        [m, d, y] if d.parse::<u32>().is_ok() => (d.clone(), month_of(m), y.clone()),
        _ => return text.to_string(),
    };
    match (day.parse::<u32>(), month, year.len() == 4 && year.parse::<u32>().is_ok()) {
        (Ok(d), Some(m), true) if cerberus_shared::i18n::lang() == cerberus_shared::i18n::Lang::En => format!("{m:02}/{d:02}/{year}"),
        (Ok(d), Some(m), true) => format!("{d:02}/{m:02}/{year}"),
        _ => text.to_string(),
    }
}
