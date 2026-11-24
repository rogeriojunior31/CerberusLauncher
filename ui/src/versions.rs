//! Versions: recomps of the same game (Zelda 64: Recompiled and 2 Ship 2
//! Harkinian are both Majora's Mask). Lists show each game once, and its page
//! has a menu to switch between them.

use cerberus_shared::tr;
use cerberus_shared::RecompEntry;
use leptos::prelude::*;

use crate::art::{Poster, recomp_covers};
use crate::icons::{Icon, Svg};
use crate::{Library, Route};

pub fn versions_of<'a>(entries: &'a [RecompEntry], title: &str) -> Vec<&'a RecompEntry> {
    entries.iter().filter(|e| e.title == title).collect()
}

/// Each game once, in catalog order: the version you have installed, else the
/// one the catalog recommends, else its first. Returns each lead with how many
/// versions its game has.
pub fn one_each(entries: &[RecompEntry]) -> Vec<(RecompEntry, usize)> {
    let mut groups: std::collections::HashMap<&str, Vec<&RecompEntry>> = std::collections::HashMap::new();
    let mut order: Vec<&str> = Vec::new();
    for entry in entries {
        let group = groups.entry(entry.title.as_str()).or_default();
        if group.is_empty() {
            order.push(entry.title.as_str());
        }
        group.push(entry);
    }
    order
        .into_iter()
        .map(|title| {
            let group = &groups[title];
            let installed = group.iter().find(|e| e.installed.is_some());
            let recommended = group.iter().find(|e| e.recommended);
            let lead = installed.or(recommended).unwrap_or(&group[0]);
            ((*lead).clone(), group.len())
        })
        .collect()
}

/// Under a recomp's title: "2 versões", opening the list to switch.
#[component]
pub fn VersionPill(current: RecompEntry) -> impl IntoView {
    let library = expect_context::<Library>();
    let open = RwSignal::new(false);
    let current_id = current.id.clone();
    let title = StoredValue::new(current.title.clone());

    // Only Copy things captured, so these closures can be used in several places.
    let group = move || {
        let all = library.recomps.get().and_then(Result::ok).unwrap_or_default();
        all.into_iter().filter(|e| e.title == title.get_value()).collect::<Vec<_>>()
    };
    let count = move || group().len();
    let installed = move || group().iter().filter(|e| e.installed.is_some()).count();

    let menu = move || {
        if !open.get() {
            return None;
        }
        let rows = group()
            .into_iter()
            .map(|entry| {
                let here = entry.id == current_id;
                let route = Route::Game(format!("recomp:{}", entry.id));
                let pick = move |_| {
                    open.set(false);
                    library.replace(route.clone());
                };
                let tag = entry.installed.as_ref().map(|v| view! { <span class="vtag inst">{tr!("Instalado · {}", v)}</span> });
                let recommended = entry.recommended.then(|| view! { <span class="vtag rec">{tr!("Recomendado")}</span> });
                let why = (entry.recommended && !entry.why.is_empty()).then(|| view! { <small class="why">{entry.why.clone()}</small> });
                let author = if entry.author.is_empty() { entry.repo.split('/').next().unwrap_or_default().to_string() } else { entry.author.clone() };
                view! {
                    <button type="button" class="vm-row" class:on=here role="option" aria-selected=here.to_string() on:click=pick>
                        <span class="v-ic"><Poster sources=recomp_covers(&entry) title=entry.name.clone() /></span>
                        <span class="v-txt">
                            <span class="v-n"><b>{entry.name.clone()}</b>{recommended}{tag}</span>
                            {(!entry.adds.is_empty()).then(|| view! { <small>{entry.adds.clone()}</small> })}
                            {why}
                            <small class="dim">{format!("por {author} · {} downloads", crate::format::count(entry.downloads))}</small>
                        </span>
                        {here.then(|| view! { <span class="v-here"><Svg icon=Icon::Check size=18 /></span> })}
                    </button>
                }
            })
            .collect_view();
        Some(view! {
            <div class="vm-scrim" on:click=move |_| open.set(false)></div>
            <div class="vermenu" role="listbox">
                <div class="vm-h">{tr!("{} versões deste jogo", count())}</div>
                {rows}
            </div>
        })
    };

    view! {
        <div class="ver-wrap">
            <button type="button" class="ver-pill" aria-haspopup="listbox" aria-expanded=move || open.get().to_string() on:click=move |_| open.update(|o| *o = !*o)>
                <Svg icon=Icon::Library size=16 />
                <span>{move || tr!("{} versões", count())}</span>
                {move || (installed() > 1).then(|| view! { <em>{format!("{} instaladas", installed())}</em> })}
            </button>
            {menu}
        </div>
    }
}
