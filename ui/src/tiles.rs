//! Game tiles (poster, title, one line under it) and shelves of them.

use cerberus_shared::tr;
use cerberus_shared::{Game, InstallStep, RecompEntry, Source};
use leptos::html;
use leptos::prelude::*;

use crate::art::{Poster, cover_sources, recomp_covers};
use crate::format;
use crate::icons::{Icon, Svg};
use crate::{Library, Route};

#[derive(Clone, PartialEq)]
pub struct TileInfo {
    /// What the corner chip means: its color says it before its words do.
    pub tone: Tone,
    pub title: String,
    pub sub: String,
    pub covers: Vec<String>,
    /// A corner chip: "Atualização", "Jogando"...
    pub badge: Option<String>,
    /// What the chip means and what to do, on hover and for screen readers.
    pub hint: Option<String>,
    /// The player's favorite: a heart on the cover.
    pub favorite: bool,
    /// The library game behind the tile, for its right-click menu.
    pub game: Option<Game>,
    pub open: Route,
    /// Catalog tiles follow installation events without rebuilding the surrounding shelf.
    recomp_id: Option<String>,
    /// Owned but not on this PC: the cover is dimmed.
    pub absent: bool,
    /// Achievements unlocked and in all, for a bar on the cover.
    pub progress: Option<(u32, u32)>,
    /// Where the game comes from and what runs it: the store's logo, "Nativo"
    /// for a recomp, the emulator and console for a classic.
    pub marks: Vec<SourceMark>,
}

/// One sign in the cover's corner.
#[derive(Clone, PartialEq, Debug)]
pub enum Mark {
    /// A store's logo, drawn in one color (ui/brand/stores/<name>.svg).
    Store(&'static str),
    /// An emulator's icon.
    Image(&'static str),
    /// A few letters: "Nativo", "PS2".
    Text(String),
}

#[derive(Clone, PartialEq, Debug)]
pub struct SourceMark {
    pub mark: Mark,
    /// What it means, on hover and for screen readers.
    pub label: String,
}

/// The marks of a library game: its store, or "Nativo" for a recomp.
pub fn source_marks(source: &Source) -> Vec<SourceMark> {
    let one = |mark: Mark, label: &str| vec![SourceMark { mark, label: label.to_string() }];
    match source {
        Source::Steam { .. } => one(Mark::Store("steam"), "Steam"),
        Source::Epic { .. } => one(Mark::Store("epic"), "Epic Games"),
        Source::Xbox { game_pass: true, .. } => one(Mark::Store("xbox"), "Game Pass"),
        Source::Xbox { .. } => one(Mark::Store("xbox"), "Xbox"),
        Source::Recomp { .. } => one(Mark::Text("Recomp".to_string()), tr!("Recomp: versão nativa para PC")),
        Source::Local { .. } => Vec::new(),
    }
}

/// The corner signs, top left of the cover.
fn marks_view(marks: &[SourceMark]) -> Option<impl IntoView + use<>> {
    (!marks.is_empty()).then(|| {
        let items = marks
            .iter()
            .map(|m| {
                let body = match &m.mark {
                    Mark::Store(name) => view! { <i class=format!("store-logo {name}")></i> }.into_any(),
                    Mark::Image(src) => view! { <img src=*src alt="" /> }.into_any(),
                    Mark::Text(text) => view! { <b>{text.clone()}</b> }.into_any(),
                };
                view! { <span class="src-mark" title=m.label.clone() aria-label=m.label.clone()>{body}</span> }
            })
            .collect_view();
        view! { <span class="src-marks">{items}</span> }
    })
}

pub fn source_name(source: &Source) -> &'static str {
    match source {
        Source::Steam { .. } => tr!("Steam"),
        Source::Epic { .. } => tr!("Epic Games"),
        Source::Xbox { game_pass: true, .. } => "Game Pass",
        Source::Xbox { .. } => "Xbox",
        Source::Local { .. } => tr!("Adicionado à mão"),
        Source::Recomp { .. } => tr!("Recomp"),
    }
}

/// Whether a newer release is out for an installed recomp.
fn has_update(game: &Game, library: Library) -> bool {
    let Source::Recomp { recomp_id, version, .. } = &game.source else {
        return false;
    };
    if game.external || game.updates_paused {
        return false;
    }
    library.recomps.with_untracked(|list| {
        let Some(Ok(entries)) = list else {
            return false;
        };
        let entry = entries.iter().find(|e| e.is(recomp_id));
        entry.and_then(|e| e.latest.as_ref()).is_some_and(|latest| cerberus_shared::is_update(version, &latest.version))
    })
}

/// A recomp's game and version: the original game's name players know
/// ("Mario Kart 64"), and the recomp's own ("SpaghettiKart") when it differs.
pub fn names_of(name: &str, entry: Option<&RecompEntry>) -> (String, Option<String>) {
    let Some(entry) = entry.filter(|e| !e.game.trim().is_empty()) else { return (name.to_string(), None) };
    let game = entry.game.trim().to_string();
    // A mod that is another number of the series ("Tenkaichi 4" on "Tenkaichi 3") is its own game.
    if another_number(name, &game) {
        return (name.to_string(), None);
    }
    let version = (!name.trim().eq_ignore_ascii_case(&game)).then(|| name.to_string());
    (game, version)
}

/// The numbers in a name ("3", "64").
fn numbers(text: &str) -> Vec<&str> {
    text.split(|c: char| !c.is_ascii_digit()).filter(|n| !n.is_empty()).collect()
}

/// The recomp names a number of a numbered series that its game doesn't have.
fn another_number(name: &str, game: &str) -> bool {
    let theirs = numbers(game);
    !theirs.is_empty() && numbers(name).iter().any(|n| !theirs.contains(n))
}

/// The names a library game shows: the original game's first, the recomp after.
pub fn game_names(game: &Game, library: Library) -> (String, Option<String>) {
    // One game of a several-game release is that game.
    if game.part_of.is_some() {
        return (game.name.clone(), None);
    }
    let Source::Recomp { recomp_id, .. } = &game.source else { return (game.name.clone(), None) };
    // Tracked: lists drawn before the catalog arrives show the game's name once it does.
    library.recomps.with(|r| {
        let entry = r.as_ref().and_then(|r| r.as_ref().ok()).and_then(|all| all.iter().find(|e| e.is(recomp_id)));
        names_of(&game.name, entry)
    })
}

/// A recomp of the catalog: its game first, the recomp after.
pub fn entry_names(entry: &RecompEntry) -> (String, Option<String>) {
    names_of(&entry.name, Some(entry))
}

pub fn game_tile(game: &Game, library: Library) -> TileInfo {
    let running = library.running.with_untracked(|all| all.contains(&game.id));
    let sub = match game.last_played {
        _ if running => tr!("Jogando agora").to_string(),
        // Not on this PC: said plainly, where it comes from (installs from there).
        _ if game.next_step == cerberus_shared::NextStep::Install => tr!("{} · não instalado", source_name(&game.source)),
        Some(when) => tr!("Jogado {}", format::ago(when)),
        None => source_name(&game.source).to_string(),
    };
    // A recomp shows its game's name; which recomp it is comes first on the line under it.
    let (title, version) = game_names(game, library);
    let sub = match version {
        Some(v) => format!("{v} · {sub}"),
        None => sub,
    };
    let hint = step_hint(game, library);
    let (badge, tone) = if running {
        (Some(tr!("Jogando").to_string()), Tone::Ok)
    } else if let Some(waiting) = waiting_for(game) {
        (Some(waiting.to_string()), Tone::Warn)
    } else if has_update(game, library) {
        (Some(tr!("Atualização").to_string()), Tone::Accent)
    } else {
        (None, Tone::Accent)
    };
    TileInfo {
        tone,
        title,
        sub,
        covers: cover_sources(game),
        badge,
        hint,
        favorite: game.favorite,
        game: Some(game.clone()),
        open: Route::Game(game.id.clone()),
        recomp_id: None,
        absent: game.next_step == cerberus_shared::NextStep::Install,
        progress: None,
        marks: source_marks(&game.source),
    }
}

/// A classic from the ROM folders, as a tile: it opens its own page.
/// `playable`: some emulator here opens it; the others are dimmed.
pub fn classic_tile(title: String, sub: String, covers: Vec<String>, path: String, badge: Option<(String, Tone)>, playable: bool, hint: Option<String>, marks: Vec<SourceMark>) -> TileInfo {
    let (badge, tone) = badge.map_or((None, Tone::Accent), |(text, tone)| (Some(text), tone));
    TileInfo { tone, title, sub, covers, badge, hint, favorite: false, game: None, open: Route::Classic(path), recomp_id: None, absent: !playable, progress: None, marks }
}

/// The meaning of a cover's corner chip.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tone {
    /// Something to do or that just happened: an update, a download.
    Accent,
    /// Being played now.
    Ok,
    /// It won't play until something is fixed.
    Warn,
    /// Good to know, nothing to do.
    Info,
}

impl Tone {
    fn class(self) -> &'static str {
        match self {
            Tone::Accent => "chip",
            Tone::Ok => "chip ok",
            Tone::Warn => "chip warn",
            Tone::Info => "chip info",
        }
    }
}

pub fn recomp_tile(entry: &RecompEntry) -> TileInfo {
    let update = entry.update_to().is_some();
    let (badge, tone) = if update {
        (Some(tr!("Atualização").to_string()), Tone::Accent)
    } else if entry.installed.is_some() {
        (Some(tr!("Instalado").to_string()), Tone::Info)
    } else {
        (None, Tone::Accent)
    };
    let (title, version) = entry_names(entry);
    TileInfo {
        tone,
        title,
        sub: version.map_or_else(|| entry.console.clone(), |v| format!("{v} · {}", entry.console)),
        covers: recomp_covers(entry),
        badge,
        hint: None,
        favorite: false,
        game: None,
        open: Route::Game(format!("recomp:{}", entry.id)),
        recomp_id: Some(entry.id.clone()),
        absent: false,
        progress: None,
        marks: vec![SourceMark { mark: Mark::Text("Recomp".to_string()), label: tr!("Recomp: versão nativa para PC").to_string() }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sequel_mod_keeps_its_own_name() {
        assert!(another_number("Dragon Ball Z Budokai Tenkaichi 4 Tag Team Decomp", "Dragon Ball Z: Budokai Tenkaichi 3"));
        assert!(!another_number("Tenkaichi 3 Decomp", "Dragon Ball Z: Budokai Tenkaichi 3"));
        assert!(!another_number("Zelda 64: Recompiled", "The Legend of Zelda: Majora's Mask"), "an unnumbered game");
        assert!(!another_number("SpaghettiKart", "Mario Kart 64"));
    }

    fn entry(name: &str, game: &str) -> RecompEntry {
        serde_json::from_value(serde_json::json!({
            "id": "x", "title": game, "name": name, "game": game, "console": "Nintendo 64", "origin": "n64",
            "stars": 0, "recommended": false, "installed": null, "icon": null, "cover": null, "hero": null, "adds": "", "needs": "", "tags": [], "features": [], "setup_steps": [], "why": "", "released": "", "added": "", "owned_copy": null, "handoff": null, "accepts_folder": false, "latest": null, "problem": null, "author": "", "repo": "", "about": "", "downloads": 0
        }))
        .unwrap()
    }

    #[test]
    fn a_recomp_is_shown_by_its_game_with_the_recomp_after() {
        assert_eq!(names_of("SpaghettiKart", Some(&entry("SpaghettiKart", "Mario Kart 64"))), ("Mario Kart 64".into(), Some("SpaghettiKart".into())));
        assert_eq!(names_of("StepMania", Some(&entry("StepMania", "StepMania"))), ("StepMania".into(), None), "a PC port named like its game");
        assert_eq!(names_of("My Port", Some(&entry("My Port", " "))), ("My Port".into(), None), "no game in the catalog: its own name");
        assert_eq!(names_of("Hollow Knight", None), ("Hollow Knight".into(), None));
    }
}

#[component]
pub fn Tile(info: TileInfo) -> impl IntoView {
    let library = expect_context::<Library>();
    let open = info.open.clone();
    let on_open = move |_| library.go(open.clone());
    let menu = expect_context::<crate::game_menu::GameMenu>();
    let menu_game = info.game.clone();
    let on_menu = move |e: leptos::ev::MouseEvent| {
        if let Some(game) = menu_game.clone() {
            menu.open(game, &e);
        }
    };
    let step = Memo::new(move |_| {
        info.recomp_id.as_ref().and_then(|id| library.progress.with(|all| all.get(id).cloned()))
    });
    let overlay = move || {
        let step = step.get();
        let (text, tone) = match step {
            Some(InstallStep::Downloading { .. }) => (Some(tr!("Baixando")), Tone::Accent),
            Some(InstallStep::Unpacking) => (Some(tr!("Preparando")), Tone::Accent),
            Some(InstallStep::Paused { .. }) => (Some(tr!("Pausado")), Tone::Accent),
            Some(InstallStep::Queued) => (Some(tr!("Na fila")), Tone::Accent),
            _ => (info.badge.as_deref(), info.tone),
        };
        // Only the game's own chip has an explanation (not "Baixando").
        let hint = step.is_none().then(|| info.hint.clone()).flatten();
        let badge = text.map(|text| view! {
            <span class=tone.class() title=hint.clone() aria-label=hint.clone().map(|h| format!("{text}: {h}"))>
                {(tone == Tone::Warn).then(|| view! { <Svg icon=Icon::Info size=12 /> })}
                {text.to_owned()}
            </span>
        });
        let bar = match step {
            Some(InstallStep::Downloading { done, total } | InstallStep::Paused { done, total }) if total > 0 => {
                let percent = u128::from(done.min(total)) * 100 / u128::from(total);
                Some(view! { <span class="bar-p"><i style:width=format!("{percent}%")></i></span> })
            }
            _ => None,
        };
        (badge, bar)
    };
    // Play (or Install) right on the cover, without opening the page first.
    let quick = info.game.clone().filter(|g| !matches!(g.next_step, cerberus_shared::NextStep::ChooseCopy { .. } | cerberus_shared::NextStep::Locate { .. })).map(|g| {
        let (label, icon) = crate::launch_action(&g);
        let label = format!("{label} · {}", g.name);
        view! {
            <div class="qp-zone">
                // Out of the Tab order: one stop per cover (the keyboard plays from the page or the menu).
                <button type="button" class="qp" tabindex="-1" title=label.clone() aria-label=label
                    on:click=move |e| { e.stop_propagation(); crate::play_game(library, g.clone()); }>
                    <Svg icon size=20 />
                </button>
            </div>
        }
    });
    view! {
        <div class="tile" class:absent=info.absent>
            {quick}
            <button type="button" class="tile-b" on:click=on_open on:contextmenu=on_menu title=info.title.clone()>
                <div class="poster-wrap">
                    <Poster sources=info.covers.clone() title=info.title.clone() />
                    {marks_view(&info.marks)}
                    {info.favorite.then(|| view! { <span class="fav-mark" title=tr!("Favorito")><Svg icon=Icon::HeartFill size=16 /></span> })}
                    {info.progress.filter(|(_, total)| *total > 0).map(|(got, total)| {
                        let pct = got * 100 / total;
                        let tip = if got == total { tr!("Platinado · {} conquistas", total) } else { tr!("{} de {} conquistas", got, total) };
                        view! {
                            <span class="ach-mini" class:done=got == total title=tip.clone() aria-label=tip>
                                <i style=format!("width: {pct}%")></i>
                            </span>
                        }
                    })}
                    {overlay}
                </div>
                <span class="t">{info.title.clone()}</span>
                <span class="s">{info.sub.clone()}</span>
            </button>
        </div>
    }
}

/// A titled row of tiles that scrolls sideways, with arrows.
#[component]
pub fn Shelf(
    title: String,
    tiles: Vec<TileInfo>,
    #[prop(optional)] note: Option<String>,
    #[prop(optional)] see_all: Option<Route>,
) -> impl IntoView {
    let library = expect_context::<Library>();
    let track = NodeRef::<html::Div>::new();
    let scroll = move |direction: f64| {
        let Some(row) = track.get() else {
            return;
        };
        let step = row.client_width() as f64 * 0.85;
        row.scroll_by_with_x_and_y(direction * step, 0.0);
    };
    let see = see_all.map(|route| {
        view! { <button type="button" class="link" on:click=move |_| library.go(route.clone())>{tr!("Ver tudo")}</button> }
    });
    let note = note.map(|text| view! { <p>{text}</p> });
    let cards = tiles.into_iter().map(|info| view! { <Tile info /> }).collect_view();
    // The arrows only when the row scrolls, and each one off at its end.
    let at_start = RwSignal::new(true);
    let at_end = RwSignal::new(false);
    // Also runs from a frame callback or a window resize, which can come after
    // the shelf is gone (a page change): then there is nothing to measure.
    let measure = move || {
        if let Some(row) = track.try_get_untracked().flatten() {
            let (left, width, full) = (row.scroll_left(), row.client_width(), row.scroll_width());
            at_start.try_set(left <= 2);
            at_end.try_set(left + width >= full - 2);
        }
    };
    Effect::new(move || {
        if track.get().is_some() {
            request_animation_frame(measure);
        }
    });
    let resized = window_event_listener(leptos::ev::resize, move |_| measure());
    on_cleanup(move || resized.remove());

    view! {
        <section class="shelf">
            <div class="shelf-h">
                <div>
                    <h2>{title}</h2>
                    {note}
                </div>
                <div class="pager">
                    {see}
                    <Show when=move || !(at_start.get() && at_end.get())>
                        <button type="button" class="round" aria-label=tr!("Anterior") disabled=move || at_start.get() on:click=move |_| scroll(-1.0)>
                            <Svg icon=Icon::Left size=18 />
                        </button>
                        <button type="button" class="round" aria-label=tr!("Próximo") disabled=move || at_end.get() on:click=move |_| scroll(1.0)>
                            <Svg icon=Icon::Right size=18 />
                        </button>
                    </Show>
                </div>
            </div>
            <div class="track" node_ref=track on:scroll=move |_| measure()>{cards}</div>
        </section>
    }
}

/// What a step before playing asks of the player, in a line.
fn step_hint(game: &Game, library: Library) -> Option<String> {
    match &game.next_step {
        cerberus_shared::NextStep::ChooseCopy { found: Some(_) } => Some(tr!("Sua cópia do jogo foi achada nas suas pastas de ROMs: é só abrir o jogo e confirmar.").to_string()),
        cerberus_shared::NextStep::ChooseCopy { found: None } => {
            let Source::Recomp { recomp_id, .. } = &game.source else { return None };
            let needs = library.recomps.with_untracked(|r| {
                r.as_ref().and_then(|r| r.as_ref().ok()).and_then(|all| all.iter().find(|e| e.is(recomp_id)).map(|e| e.needs.clone()))
            });
            Some(match needs.filter(|n| !n.is_empty()) {
                Some(needs) => tr!("{} Abra o jogo para escolher onde ela está.", needs),
                None => tr!("O recomp precisa do jogo original (a sua cópia). Abra o jogo para escolher onde ela está.").to_string(),
            })
        }
        cerberus_shared::NextStep::RunInstaller => Some(tr!("O instalador do recomp ainda não rodou: abra o jogo para terminar.").to_string()),
        cerberus_shared::NextStep::Locate { reason } => Some(reason.clone()),
        _ => None,
    }
}

/// The tile's note when a step comes before playing.
fn waiting_for(game: &Game) -> Option<&'static str> {
    match game.next_step {
        cerberus_shared::NextStep::Play => None,
        cerberus_shared::NextStep::RunInstaller => Some(tr!("Instalação pendente")),
        cerberus_shared::NextStep::ChooseCopy { .. } => Some(tr!("Falta sua cópia")),
        cerberus_shared::NextStep::Locate { .. } => Some(tr!("Programa não encontrado")),
        // Dimmed already, and under its own filter: no chip on every cover.
        cerberus_shared::NextStep::Install => None,
    }
}
