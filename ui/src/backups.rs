//! A recomp's save backups: list, make one now, restore.

use cerberus_shared::tr;
use cerberus_shared::Backup;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::icons::{Icon, Svg};
use crate::{Library, api, format};

#[component]
pub fn Backups(
    game_id: String,
    /// `game_id` is an emulator's ("duckstation"): its memory cards and states.
    #[prop(optional)] emulator: bool,
    /// The card's title and line under it, for an emulator.
    #[prop(optional)] title: Option<String>,
    #[prop(optional)] note: Option<String>,
) -> impl IntoView {
    let library = expect_context::<Library>();
    let id = StoredValue::new(game_id);
    let refresh = RwSignal::new(0u32);
    let busy = RwSignal::new(false);
    let backups = LocalResource::new(move || {
        refresh.track();
        // A backup is made when the game closes, which also reloads the library.
        library.games.track();
        let id = id.get_value();
        async move {
            if emulator {
                api::emulator_backups().await.map(|all| all.into_iter().find(|e| e.id == id).map(|e| e.backups).unwrap_or_default())
            } else {
                api::list_backups(&id).await
            }
        }
    });

    let on_backup = move |_| {
        busy.set(true);
        spawn_local(async move {
            let made = library.report(if emulator { api::backup_emulator_saves(&id.get_value()).await } else { api::backup_now(&id.get_value()).await });
            busy.set(false);
            match made {
                Some(Some(_)) => library.toast(tr!("Backup criado.")),
                Some(None) => library.toast(tr!("Nada novo para copiar: o backup mais recente já tem esses saves, ou o jogo ainda não salvou.")),
                None => {}
            }
            refresh.update(|n| *n += 1);
        });
    };
    let on_open = move |_| {
        spawn_local(async move {
            library.report(if emulator { api::open_emulator_backups(&id.get_value()).await } else { api::open_backups(&id.get_value()).await });
        });
    };

    let list = move || match backups.get() {
        None => view! { <p class="muted">{tr!("Carregando...")}</p> }.into_any(),
        Some(Err(error)) => view! { <p class="problem">{error}</p> }.into_any(),
        Some(Ok(list)) if list.is_empty() => view! {
            <div class="empty small">
                <h3>{tr!("Nenhum backup ainda")}</h3>
                <p class="muted">{if emulator { tr!("Um é feito sempre que você fecha um clássico nesse emulador.") } else { tr!("Um é feito sempre que você fecha o jogo, antes de atualizar e antes de desinstalar.") }}</p>
            </div>
        }
        .into_any(),
        Some(Ok(list)) => {
            let rows = list.into_iter().map(|backup| view! { <BackupRow backup game_id=id.get_value() emulator refresh busy /> }).collect_view();
            view! { <ul class="backups">{rows}</ul> }.into_any()
        }
    };

    view! {
        <section class="card">
            <div class="card-h">
                <div>
                    <h3>{title.unwrap_or_else(|| tr!("Backups de saves").to_string())}</h3>
                    <p class="muted small">{note.unwrap_or_else(|| tr!("Os 10 mais recentes ficam guardados, e mais as cópias de antes de cada restauração, mesmo se você desinstalar o jogo.").to_string())}</p>
                </div>
                <div class="row">
                    <button type="button" class="btn sm" on:click=on_open>
                        <Svg icon=Icon::Folder size=16 />{tr!("Abrir pasta")}
                    </button>
                    <button type="button" class="btn sm" class:main=!emulator on:click=on_backup disabled=move || busy.get()>{tr!("Fazer backup agora")}</button>
                </div>
            </div>
            {list}
        </section>
    }
}

#[component]
fn BackupRow(backup: Backup, game_id: String, emulator: bool, refresh: RwSignal<u32>, busy: RwSignal<bool>) -> impl IntoView {
    let library = expect_context::<Library>();
    // Restoring replaces the current saves (after copying them), so it takes a second click.
    let armed = RwSignal::new(false);
    let name = StoredValue::new(backup.name.clone());
    let id = StoredValue::new(game_id);

    let on_restore = move |_| {
        if !armed.get_untracked() {
            armed.set(true);
            // Not armed forever: a click much later is a new first click.
            set_timeout(move || armed.set(false), std::time::Duration::from_secs(5));
            return;
        }
        armed.set(false);
        busy.set(true);
        spawn_local(async move {
            let restored = library.report(if emulator {
                api::restore_emulator_backup(&id.get_value(), &name.get_value()).await
            } else {
                api::restore_backup(&id.get_value(), &name.get_value()).await
            });
            busy.set(false);
            if let Some(files) = restored {
                library.toast(if files == 1 {
                    tr!("1 arquivo restaurado. Os saves de antes estão guardados nos backups.").to_string()
                } else {
                    tr!("{} arquivos restaurados. Os saves de antes estão guardados nos backups.", files)
                });
            }
            refresh.update(|n| *n += 1);
        });
    };

    let when = format::date_time(backup.created);
    let files = if backup.files == 1 { tr!("1 arquivo").to_string() } else { tr!("{} arquivos", backup.files) };
    let damaged = backup.damaged;
    let detail = if damaged {
        tr!("Danificado ({}): o arquivo não pode ser lido, então não dá para restaurar.", format::megabytes(backup.bytes))
    } else {
        format!("{} · {} · {}", cerberus_shared::i18n::translate(&backup.reason), files, format::megabytes(backup.bytes))
    };
    let label = move || if armed.get() { tr!("Confirmar: substituir os saves atuais") } else { tr!("Restaurar") };

    view! {
        <li>
            <span class="ic"><Svg icon=Icon::Clock size=18 /></span>
            <div>
                <b>{when}</b>
                <span class="muted small">{detail}</span>
            </div>
            {(!damaged).then(|| view! {
                <button type="button" class="btn sm" class:warn=move || armed.get() on:click=on_restore on:blur=move |_| armed.set(false) disabled=move || busy.get()>
                    {label}
                </button>
            })}
        </li>
    }
}
