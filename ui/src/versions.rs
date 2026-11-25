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

/// Set while the page swaps to another version of the same game: the game's
/// art stays put and only what belongs to the version changes.
#[derive(Clone, Copy)]
pub struct Switching(pub RwSignal<bool>);

/// A game's versions in the order to choose from: the one you have, then the
/// one the catalog recommends, then the most downloaded.
pub fn ordered(mut group: Vec<RecompEntry>) -> Vec<RecompEntry> {
    group.sort_by_key(|e| (e.installed.is_none(), !e.recommended, std::cmp::Reverse(e.downloads)));
    group
}

fn author_of(entry: &RecompEntry) -> String {
    if entry.author.is_empty() { entry.repo.split('/').next().unwrap_or_default().to_string() } else { entry.author.clone() }
}

/// Under the game's title: which version this is ("SpaghettiKart · Nintendo 64 ·
/// por HarbourMasters"). With more than one, it opens the others to switch, in
/// place: arrows move, Enter picks, Escape closes.
#[component]
pub fn VersionLine(current: RecompEntry, #[prop(optional)] covers: Vec<String>) -> impl IntoView {
    let library = expect_context::<Library>();
    let switching = use_context::<Switching>();
    // 0 closed, 1 open, 2 closing (still drawn while it fades out).
    let phase = RwSignal::new(0u8);
    let up = RwSignal::new(false);
    let room = RwSignal::new(480.0f64);
    let trigger = NodeRef::<leptos::html::Button>::new();
    let current_id = current.id.clone();
    let title = StoredValue::new(current.title.clone());
    let covers = StoredValue::new(covers);

    let group = move || {
        let all = library.recomps.get().and_then(Result::ok).unwrap_or_default();
        ordered(all.into_iter().filter(|e| e.title == title.get_value()).collect())
    };
    let count = move || group().len();

    let close = move |focus_back: bool| {
        if phase.get_untracked() != 1 {
            return;
        }
        phase.set(2);
        set_timeout(move || phase.set(0), std::time::Duration::from_millis(140));
        if focus_back {
            if let Some(b) = trigger.get_untracked() {
                let _ = b.focus();
            }
        }
    };
    let open_menu = move || {
        // Opens downward when there's room, else upward, and never past the window.
        if let (Some(b), Some(w)) = (trigger.get_untracked(), web_sys::window()) {
            let rect = b.get_bounding_client_rect();
            let height = w.inner_height().ok().and_then(|h| h.as_f64()).unwrap_or(800.0);
            let below = height - rect.bottom() - 16.0;
            let above = rect.top() - 16.0;
            up.set(below < 320.0 && above > below);
            room.set(if up.get_untracked() { above } else { below }.max(200.0));
        }
        phase.set(1);
        request_animation_frame(|| {
            if let Some(row) = document().query_selector(".vermenu .vm-row.on").ok().flatten().or_else(|| document().query_selector(".vermenu .vm-row").ok().flatten()) {
                let row: web_sys::HtmlElement = wasm_bindgen::JsCast::unchecked_into(row);
                let _ = row.focus();
            }
        });
    };
    let keys = move |e: leptos::ev::KeyboardEvent| {
        let rows = document().query_selector_all(".vermenu .vm-row").ok();
        let Some(rows) = rows else { return };
        let n = rows.length();
        let active = document().active_element();
        let at = (0..n).find(|i| rows.item(*i).zip(active.clone()).is_some_and(|(r, a)| r == wasm_bindgen::JsCast::unchecked_into::<web_sys::Node>(a)));
        let go = |i: u32| {
            if let Some(r) = rows.item(i) {
                let r: web_sys::HtmlElement = wasm_bindgen::JsCast::unchecked_into(r);
                let _ = r.focus();
            }
        };
        match e.key().as_str() {
            "Escape" => {
                e.prevent_default();
                close(true);
            }
            "ArrowDown" if n > 0 => {
                e.prevent_default();
                go(at.map_or(0, |i| (i + 1) % n));
            }
            "ArrowUp" if n > 0 => {
                e.prevent_default();
                go(at.map_or(n - 1, |i| (i + n - 1) % n));
            }
            "Home" if n > 0 => go(0),
            "End" if n > 0 => go(n - 1),
            "Tab" => close(false),
            _ => {}
        }
    };

    let menu = move || {
        if phase.get() == 0 {
            return None;
        }
        let list = group();
        // Two versions with the same name are told apart by their project.
        let same_name = |name: &str| list.iter().filter(|e| e.name == name).count() > 1;
        let rows = list
            .iter()
            .cloned()
            .map(|entry| {
                let here = entry.id == current_id;
                let route = Route::Game(format!("recomp:{}", entry.id));
                let pick = move |_| {
                    if here {
                        close(true);
                        return;
                    }
                    if let Some(Switching(s)) = switching {
                        s.set(true);
                        set_timeout(move || s.set(false), std::time::Duration::from_millis(700));
                    }
                    phase.set(0);
                    library.replace(route.clone());
                };
                let installed = entry.installed.as_ref().map(|v| view! { <span class="vtag inst">{tr!("Instalada · {}", v)}</span> });
                let recommended = entry.recommended.then(|| view! { <span class="vtag rec">{tr!("Recomendada")}</span> });
                let what = if !entry.adds.is_empty() { entry.adds.clone() } else { entry.why.clone() };
                let project = same_name(&entry.name).then(|| entry.repo.clone()).filter(|r| !r.is_empty());
                let mut facts = vec![tr!("por {}", author_of(&entry))];
                if let Some(release) = &entry.latest {
                    facts.push(release.version.clone());
                    if release.size > 0 {
                        facts.push(crate::format::megabytes(release.size));
                    }
                }
                facts.push(tr!("{} downloads", crate::format::count(entry.downloads)));
                let mut art = recomp_covers(&entry);
                art.extend(covers.get_value());
                view! {
                    <button type="button" class="vm-row" class:on=here role="option" aria-selected=here.to_string() tabindex="-1" on:click=pick>
                        <span class="v-ic"><Poster sources=art title=entry.name.clone() /></span>
                        <span class="v-txt">
                            <span class="v-n"><b>{entry.name.clone()}</b>{recommended}{installed}</span>
                            {(!what.is_empty()).then(|| view! { <small class="v-what">{what}</small> })}
                            {project.map(|p| view! { <small class="v-repo">{p}</small> })}
                            <small class="v-facts">{facts.join(" · ")}</small>
                        </span>
                        <span class="v-here" aria-hidden="true">{here.then(|| view! { <Svg icon=Icon::Check size=18 /> })}</span>
                    </button>
                }
            })
            .collect_view();
        let style = format!("max-height: {}px", room.get().min(520.0));
        Some(view! {
            <div class="vm-scrim" on:click=move |_| close(false)></div>
            <div class="vermenu" class:up=move || up.get() class:out=move || phase.get() == 2 role="listbox" aria-label=tr!("Versões deste jogo") style=style on:keydown=keys>
                <div class="vm-h">{tr!("{} versões deste jogo", count())}</div>
                {rows}
            </div>
        })
    };

    let console = current.console.clone();
    let author = author_of(&current);
    let name = current.name.clone();
    let many = move || count() > 1;
    view! {
        <div class="ver-wrap">
            <button type="button" class="gp-ver" node_ref=trigger class:static=move || !many() disabled=move || !many()
                aria-haspopup="listbox" aria-expanded=move || (phase.get() == 1).to_string()
                title=move || if many() { tr!("Trocar a versão").to_string() } else { String::new() }
                on:click=move |_| if phase.get_untracked() == 1 { close(false) } else { open_menu() }
                on:keydown=move |e| if e.key() == "ArrowDown" && many() { e.prevent_default(); open_menu() }>
                <b>{name}</b>
                <span class="gp-ver-meta">{format!("{console} · ")}{tr!("por {}", author)}</span>
                {move || many().then(|| view! { <span class="gp-ver-more">{tr!("{} versões", count())}<Svg icon=Icon::Chevron size=16 /></span> })}
            </button>
            {menu}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, installed: bool, recommended: bool, downloads: u64) -> RecompEntry {
        serde_json::from_value(serde_json::json!({
            "id": id, "title": "mk64", "name": id, "game": "Mario Kart 64", "console": "Nintendo 64", "origin": "n64",
            "stars": 0, "icon": null, "cover": null, "hero": null, "adds": "", "needs": "", "tags": [], "features": [], "setup_steps": [], "why": "", "released": "", "added": "", "owned_copy": null, "handoff": null, "accepts_folder": false, "latest": null, "problem": null, "author": "", "repo": "", "about": "", "downloads": downloads,
            "installed": if installed { serde_json::json!("1.0") } else { serde_json::Value::Null }, "recommended": recommended
        }))
        .unwrap()
    }

    #[test]
    fn versions_come_installed_then_recommended_then_most_downloaded() {
        let list = ordered(vec![entry("few", false, false, 10), entry("rec", false, true, 5), entry("mine", true, false, 1), entry("many", false, false, 900)]);
        let ids: Vec<&str> = list.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["mine", "rec", "many", "few"]);
    }
}
