//! A recomp's save backups: list, make one now, restore.

use cerberus_shared::Backup;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::icons::{Icon, Svg};
use crate::{Library, api, format};

#[component]
pub fn Backups(game_id: String) -> impl IntoView {
    let library = expect_context::<Library>();
    let id = StoredValue::new(game_id);
    let refresh = RwSignal::new(0u32);
    let busy = RwSignal::new(false);
    let backups = LocalResource::new(move || {
        refresh.track();
        // A backup is made when the game closes, which also reloads the library.
        library.games.track();
        let id = id.get_value();
        async move { api::list_backups(&id).await }
    });

    let on_backup = move |_| {
        busy.set(true);
        spawn_local(async move {
            let made = library.report(api::backup_now(&id.get_value()).await);
            busy.set(false);
            match made {
                Some(Some(_)) => library.toast("Backup criado."),
                Some(None) => library.toast("Nada novo para copiar: o backup mais recente já tem esses saves, ou o jogo ainda não salvou."),
                None => {}
            }
            refresh.update(|n| *n += 1);
        });
    };
    let on_open = move |_| {
        spawn_local(async move {
            library.report(api::open_backups(&id.get_value()).await);
        });
    };

    let list = move || match backups.get() {
        None => view! { <p class="muted">"Carregando..."</p> }.into_any(),
        Some(Err(error)) => view! { <p class="problem">{error}</p> }.into_any(),
        Some(Ok(list)) if list.is_empty() => view! {
            <div class="empty small">
                <h3>"Nenhum backup ainda"</h3>
                <p class="muted">"Um é feito sempre que você fecha o jogo, antes de atualizar e antes de desinstalar."</p>
            </div>
        }
        .into_any(),
        Some(Ok(list)) => {
            let rows = list.into_iter().map(|backup| view! { <BackupRow backup game_id=id.get_value() refresh busy /> }).collect_view();
            view! { <ul class="backups">{rows}</ul> }.into_any()
        }
    };

    view! {
        <section class="card">
            <div class="card-h">
                <div>
                    <h3>"Backups de saves"</h3>
                    <p class="muted small">"Os 10 mais recentes ficam guardados, mesmo se você desinstalar o jogo."</p>
                </div>
                <div class="row">
                    <button type="button" class="btn sm" on:click=on_open>
                        <Svg icon=Icon::Folder size=16 />"Abrir pasta"
                    </button>
                    <button type="button" class="btn sm main" on:click=on_backup disabled=move || busy.get()>"Fazer backup agora"</button>
                </div>
            </div>
            {list}
        </section>
    }
}

#[component]
fn BackupRow(backup: Backup, game_id: String, refresh: RwSignal<u32>, busy: RwSignal<bool>) -> impl IntoView {
    let library = expect_context::<Library>();
    // Restoring replaces the current saves (after copying them), so it takes a second click.
    let armed = RwSignal::new(false);
    let name = StoredValue::new(backup.name.clone());
    let id = StoredValue::new(game_id);

    let on_restore = move |_| {
        if !armed.get_untracked() {
            armed.set(true);
            return;
        }
        armed.set(false);
        busy.set(true);
        spawn_local(async move {
            let restored = library.report(api::restore_backup(&id.get_value(), &name.get_value()).await);
            busy.set(false);
            if let Some(files) = restored {
                library.toast(format!("{files} arquivo(s) restaurado(s). Os saves de antes ficaram num backup novo."));
            }
            refresh.update(|n| *n += 1);
        });
    };

    let when = format::date_time(backup.created);
    let detail = format!("{} · {} arquivo(s) · {}", backup.reason, backup.files, format::megabytes(backup.bytes));
    let label = move || if armed.get() { "Confirmar: substituir os saves atuais" } else { "Restaurar" };

    view! {
        <li>
            <span class="ic"><Svg icon=Icon::Clock size=18 /></span>
            <div>
                <b>{when}</b>
                <span class="muted small">{detail}</span>
            </div>
            <button type="button" class="btn sm" class:warn=move || armed.get() on:click=on_restore disabled=move || busy.get()>
                {label}
            </button>
        </li>
    }
}
