//! Panels that slide in from the right over the TV screen: leaving, a game's
//! options, confirming a removal, and the on-screen keyboard for search. While
//! one is open it takes every button; B closes it.

use cerberus_shared::{tk, tr};
use std::rc::Rc;

use cerberus_shared::Source;
use leptos::prelude::*;
use leptos::task::spawn_local;

use super::{Btn, Focus, Sheet, Sound, Tab, Tv, View, play};
use crate::art::Poster;
use crate::discover::start_install;
use crate::{Library, Route, api, play_game};

struct Choice {
    label: String,
    danger: bool,
    run: Rc<dyn Fn()>,
}

fn choice(label: impl Into<String>, run: impl Fn() + 'static) -> Choice {
    Choice { label: label.into(), danger: false, run: Rc::new(run) }
}

fn danger(label: impl Into<String>, run: impl Fn() + 'static) -> Choice {
    Choice { label: label.into(), danger: true, run: Rc::new(run) }
}

#[component]
pub fn SheetView(sheet: Sheet) -> impl IntoView {
    match sheet {
        Sheet::Keyboard => view! { <Keyboard /> }.into_any(),
        other => view! { <ChoiceSheet sheet=other /> }.into_any(),
    }
}

/// A list of choices; A picks one, B closes.
#[component]
fn ChoiceSheet(sheet: Sheet) -> impl IntoView {
    let library = expect_context::<Library>();
    let tv = expect_context::<Tv>();
    let focus = Focus::new("sheet", true);
    let close = move || tv.sheet.set(None);

    let games = library.games.get_untracked().and_then(Result::ok).unwrap_or_default();
    let recomps = library.recomps.get_untracked().and_then(Result::ok).unwrap_or_default();
    let mut header: Option<(String, Vec<String>)> = None;

    let (title, choices): (String, Vec<Choice>) = match &sheet {
        Sheet::Leave => (
            tr!("Sair do Big Picture Mode?").into(),
            vec![
                choice(tr!("Voltar para a janela normal"), move || library.go(Route::Home)),
                danger(tr!("Fechar o Cerberus"), || api::window_action("close")),
                choice(tr!("Cancelar"), move || tv.sheet.set(None)),
            ],
        ),
        Sheet::Options(id) => {
            let game = games.iter().find(|g| g.id == *id).cloned();
            let rid = match &game {
                Some(g) => match &g.source {
                    Source::Recomp { recomp_id, .. } => Some(recomp_id.clone()),
                    _ => None,
                },
                None => id.strip_prefix("recomp:").map(str::to_string),
            };
            let entry = rid.as_ref().and_then(|r| recomps.iter().find(|e| e.id == *r).cloned());
            let mut list = Vec::new();
            match &game {
                Some(g) => {
                    header = Some((g.name.clone(), crate::art::cover_sources(g)));
                    let play_it = g.clone();
                    list.push(choice(crate::launch_action(g).0, move || play_game(library, play_it.clone())));
                }
                None => {
                    if let Some(e) = &entry {
                        header = Some((e.name.clone(), crate::art::recomp_covers(e)));
                        if e.latest.is_some() && e.installed.is_none() {
                            let rid = e.id.clone();
                            list.push(choice(tr!("Instalar"), move || start_install(library, rid.clone())));
                        }
                    }
                }
            }
            if tv.view.get_untracked() != View::Game(id.clone()) {
                let open = id.clone();
                list.push(choice(tr!("Página do jogo"), move || tv.open(View::Game(open.clone()))));
            }
            if let Some(g) = &game {
                let (gid, on) = (g.id.clone(), g.favorite);
                list.push(choice(if on { tr!("Tirar dos favoritos") } else { tr!("Adicionar aos favoritos") }, move || {
                    crate::toggle_favorite(library, gid.clone(), !on)
                }));
                if matches!(g.source, Source::Recomp { .. }) {
                    let gid = g.id.clone();
                    list.push(choice(tr!("Fazer backup dos saves"), move || {
                        let gid = gid.clone();
                        spawn_local(async move {
                            match library.report(api::backup_now(&gid).await) {
                                Some(Some(b)) => library.toast(tr!("Backup feito: {} arquivos.", b.files)),
                                Some(None) => library.toast(tr!("Nada novo para copiar.")),
                                None => {}
                            }
                        });
                    }));
                }
                let gid = g.id.clone();
                list.push(choice(tr!("Abrir a pasta do jogo"), move || {
                    let gid = gid.clone();
                    spawn_local(async move {
                        library.report(api::open_folder(&gid).await);
                    });
                }));
                if matches!(g.source, Source::Recomp { .. } | Source::Local { .. }) {
                    let gid = g.id.clone();
                    let label = if matches!(g.source, Source::Recomp { .. }) && !g.external { tr!("Desinstalar") } else { tr!("Remover da biblioteca") };
                    list.push(danger(label, move || tv.sheet.set(Some(Sheet::ConfirmRemove(gid.clone())))));
                }
            }
            (tr!("Opções").into(), list)
        }
        Sheet::ConfirmRemove(id) => {
            let game = games.iter().find(|g| g.id == *id).cloned();
            let name = game.as_ref().map(|g| g.name.clone()).unwrap_or_default();
            let uninstall = game.as_ref().is_some_and(|g| matches!(g.source, Source::Recomp { .. }) && !g.external);
            let gid = id.clone();
            let remove = move || {
                let gid = gid.clone();
                spawn_local(async move {
                    if library.report(api::remove_game(&gid).await).is_some() {
                        library.games.refetch();
                        library.recomps.refetch();
                        library.toast(tr!("Removido. Os backups de saves continuam guardados."));
                        // The TV may have closed while the files were deleted.
                        if tv.view.try_get_untracked() == Some(View::Game(gid.clone())) && !tv.back() {
                            tv.tab(Tab::Home);
                        }
                    }
                });
            };
            (
                if uninstall { tr!("Desinstalar {}?", name) } else { tr!("Remover {} da biblioteca?", name) },
                vec![danger(if uninstall { tr!("Sim, desinstalar") } else { tr!("Sim, remover") }, remove), choice(tr!("Cancelar"), move || tv.sheet.set(None))],
            )
        }
        Sheet::Keyboard => (String::new(), Vec::new()),
    };

    let count = choices.len();
    let runs: Vec<Rc<dyn Fn()>> = choices.iter().map(|c| c.run.clone()).collect();
    let runs = StoredValue::new_local(runs);
    tv.sheet_hints.set(vec![(Btn::A, tr!("Escolher")), (Btn::B, tr!("Fechar"))]);
    let is_options = matches!(sheet, Sheet::Options(_));
    // A on the focused choice, or a click on any choice.
    let pick = move |i: usize| {
        play(Sound::Select);
        // Taken before closing: closing drops this sheet, and its choices with it.
        let run = runs.try_with_value(|r| r.get(i).cloned()).flatten();
        // Close first: a choice may open another sheet.
        close();
        if let Some(run) = run {
            run();
        }
    };
    tv.handle_sheet(move |btn| {
        match btn {
            Btn::Up | Btn::Down => {
                if focus.step(btn, &vec![1; count]) {
                    play(Sound::Move);
                }
            }
            Btn::A => pick(focus.row.get_untracked()),
            Btn::B => {
                play(Sound::Back);
                close();
            }
            Btn::X if is_options => {
                play(Sound::Back);
                close();
            }
            _ => {}
        }
        true
    });

    let items = choices
        .into_iter()
        .enumerate()
        .map(|(r, c)| {
            view! {
                <div class="tv-choice" class:danger=c.danger class:on=move || focus.is(r, 0) data-tvf=focus.key(r, 0)
                    on:mouseenter=move |_| focus.set(r, 0) on:click=move |_| pick(r)>{c.label}</div>
            }
        })
        .collect_view();
    let head = header.map(|(name, covers)| {
        view! { <div class="tv-sheet-head"><div class="tv-sheet-cover"><Poster sources=covers title=name.clone() /></div><b>{name}</b></div> }
    });
    view! {
        <div class="tv-scrim" on:click=move |_| { play(Sound::Back); close(); }>
            <aside class="tv-sheet" on:click=|e| e.stop_propagation()>
                {head}
                <h2>{title}</h2>
                <div class="tv-choices">{items}</div>
            </aside>
        </div>
    }
}

const KEYS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const KEY_COLUMNS: usize = 6;
const EXTRA: [&str; 4] = [tk!("Espaço"), tk!("Apagar"), tk!("Limpar"), tk!("Pronto")];

/// The search keyboard: letters and digits in a grid, Y types a space and X
/// deletes; a physical keyboard types too. Results filter as you type.
#[component]
fn Keyboard() -> impl IntoView {
    let tv = expect_context::<Tv>();
    let focus = Focus::new("kb", true);
    let keys: Vec<char> = KEYS.chars().collect();
    let key_rows = keys.len().div_ceil(KEY_COLUMNS);
    let mut rows: Vec<usize> = (0..key_rows).map(|r| (keys.len() - r * KEY_COLUMNS).min(KEY_COLUMNS)).collect();
    rows.push(EXTRA.len());
    let rows = StoredValue::new(rows);
    let close = move || tv.sheet.set(None);
    let typed = move |c: char| tv.search.update(|s| s.push(c.to_ascii_lowercase()));
    let erase = move || tv.search.update(|s| {
        s.pop();
    });
    tv.sheet_hints.set(vec![(Btn::A, tr!("Digitar")), (Btn::Y, tr!("Espaço")), (Btn::X, tr!("Apagar")), (Btn::B, tr!("Pronto"))]);
    let key_list = keys.clone();
    tv.handle_sheet(move |btn| {
        match btn {
            Btn::Up | Btn::Down | Btn::Left | Btn::Right => {
                if rows.with_value(|r| focus.step(btn, r)) {
                    play(Sound::Move);
                }
            }
            Btn::A => {
                let (r, c) = (focus.row.get_untracked(), focus.col.get_untracked());
                play(Sound::Move);
                if r < key_rows {
                    if let Some(k) = key_list.get(r * KEY_COLUMNS + c) {
                        typed(*k);
                    }
                } else {
                    match c {
                        0 => typed(' '),
                        1 => erase(),
                        2 => tv.search.set(String::new()),
                        _ => {
                            play(Sound::Select);
                            close();
                        }
                    }
                }
            }
            Btn::Y => typed(' '),
            Btn::X | Btn::Erase => erase(),
            Btn::Type(c) => typed(c),
            Btn::B => {
                play(Sound::Back);
                close();
            }
            _ => {}
        }
        true
    });
    let grid = keys
        .into_iter()
        .enumerate()
        .map(|(i, k)| {
            let (r, c) = (i / KEY_COLUMNS, i % KEY_COLUMNS);
            view! { <span class="tv-key" class:on=move || focus.is(r, c) data-tvf=focus.key(r, c)>{k.to_string()}</span> }
        })
        .collect_view();
    let extra = EXTRA
        .iter()
        .enumerate()
        .map(|(c, label)| view! { <span class="tv-key wide" class:done=c == 3 class:on=move || focus.is(key_rows, c) data-tvf=focus.key(key_rows, c)>{cerberus_shared::i18n::t(label)}</span> })
        .collect_view();
    view! {
        <div class="tv-scrim">
            <aside class="tv-sheet keyboard">
                <h2>{tr!("Buscar")}</h2>
                <div class="tv-field">{move || { let q = tv.search.get(); if q.is_empty() { view! { <span class="ph">{tr!("Nome do jogo")}</span> }.into_any() } else { view! { <span>{q}</span> }.into_any() } }}<i class="caret"></i></div>
                <div class="tv-keys">{grid}</div>
                <div class="tv-keys extra">{extra}</div>
            </aside>
        </div>
    }
}
