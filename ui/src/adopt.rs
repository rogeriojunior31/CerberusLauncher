//! "Trazer seus recomps": recomps the player installed without Cerberus,
//! found in a folder they pick and brought into the library where they are.

use cerberus_shared::AdoptFound;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::art::Poster;
use crate::icons::{Icon, Svg};
use crate::{Library, api};

#[derive(Clone)]
enum Stage {
    Looking,
    Found(Vec<AdoptFound>),
    Bringing,
}

/// Opens the folder picker, then a window with what was found there.
pub fn start(library: Library) {
    library.adopting.set(true);
}

#[component]
pub fn AdoptDialog() -> impl IntoView {
    let library = expect_context::<Library>();
    let stage = RwSignal::new(Stage::Looking);
    let folder = RwSignal::new(String::new());
    // Which of the found recomps to bring: sure matches start ticked.
    let picked = RwSignal::new(Vec::<String>::new());
    let close = move || library.adopting.set(false);

    spawn_local(async move {
        match api::scan_for_recomps().await {
            Ok(Some(scan)) => {
                picked.set(scan.found.iter().filter(|f| f.sure).map(|f| f.recomp_id.clone()).collect());
                folder.set(scan.folder);
                stage.set(Stage::Found(scan.found));
            }
            Ok(None) => close(),
            Err(error) => {
                library.report::<()>(Err(error));
                close();
            }
        }
    });

    let bring = move || {
        let Stage::Found(found) = stage.get_untracked() else { return };
        let chosen: Vec<AdoptFound> = found.into_iter().filter(|f| picked.get_untracked().contains(&f.recomp_id)).collect();
        stage.set(Stage::Bringing);
        spawn_local(async move {
            let mut brought = 0;
            for f in &chosen {
                if library.report(api::adopt_recomp(&f.recomp_id, Some(f.program.clone())).await).flatten().is_some() {
                    brought += 1;
                }
            }
            library.games.refetch();
            library.recomps.refetch();
            close();
            match brought {
                0 => {}
                1 => library.toast("1 recomp trazido para a Biblioteca."),
                n => library.toast(format!("{n} recomps trazidos para a Biblioteca.")),
            }
            if brought > 0 {
                library.go(crate::Route::Library);
            }
        });
    };

    let body = move || match stage.get() {
        Stage::Looking => view! { <p class="fr-status"><span class="spin"></span>"Procurando recomps na pasta..."</p> }.into_any(),
        Stage::Bringing => view! { <p class="fr-status"><span class="spin"></span>"Trazendo para a Biblioteca..."</p> }.into_any(),
        Stage::Found(found) if found.is_empty() => view! {
            <p class="muted">"Nenhum recomp do catálogo nessa pasta. Se um jogo seu não apareceu, abra a página dele no Descobrir e use \"Já tenho instalado\"."</p>
        }
        .into_any(),
        Stage::Found(found) => {
            let rows = found
                .into_iter()
                .map(|f| {
                    let rid = f.recomp_id.clone();
                    let rid_check = StoredValue::new(f.recomp_id.clone());
                    let on = move || rid_check.with_value(|r| picked.with(|p| p.contains(r)));
                    let toggle = move |_| {
                        picked.update(|p| match p.iter().position(|x| *x == rid) {
                            Some(i) => {
                                p.remove(i);
                            }
                            None => p.push(rid.clone()),
                        })
                    };
                    let covers = f.cover.clone().into_iter().collect::<Vec<_>>();
                    view! {
                        <button type="button" class="adopt-row" class:on=on role="checkbox" aria-checked=move || on().to_string() on:click=toggle>
                            <div class="adopt-cover"><Poster sources=covers title=f.game.clone() /></div>
                            <div class="adopt-text">
                                <b>{f.name.clone()}</b>
                                <small>{format!("{} · {}", f.game, f.console)}</small>
                                <small class="path">{f.program.clone()}</small>
                                {(!f.sure).then(|| view! { <small class="adopt-maybe">"Nome parecido: confira se é esse mesmo"</small> })}
                            </div>
                            <span class="adopt-check"><Svg icon=Icon::Check size=16 /></span>
                        </button>
                    }
                })
                .collect_view();
            view! { <div class="adopt-list">{rows}</div> }.into_any()
        }
    };

    let count = move || picked.with(Vec::len);
    let can_bring = move || matches!(stage.get(), Stage::Found(ref f) if !f.is_empty()) && count() > 0;
    view! {
        <div class="backdrop-dim">
            <div class="dialog adopt">
                <h2>"Trazer seus recomps"</h2>
                <p class="muted">"Recomps que você instalou sem o Cerberus entram na Biblioteca onde estão, sem baixar nada. A pasta continua sua: o Cerberus nunca apaga nem atualiza esses arquivos."</p>
                <Show when=move || !folder.get().is_empty()>
                    <p class="adopt-folder"><Svg icon=Icon::Folder size=16 /><span>{move || folder.get()}</span></p>
                </Show>
                {body}
                <div class="dialog-footer">
                    <button type="button" class="btn" on:click=move |_| close()>"Cancelar"</button>
                    <Show when=can_bring>
                        <button type="button" class="btn main" on:click=move |_| bring()>
                            {move || if count() == 1 { "Trazer 1 recomp".to_string() } else { format!("Trazer {} recomps", count()) }}
                        </button>
                    </Show>
                </div>
            </div>
        </div>
    }
}
