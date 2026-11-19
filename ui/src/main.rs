mod api;
mod art;
mod backups;
mod tv;
mod discover;
mod format;
mod game;
mod looks;
mod customize;
mod adopt;
mod first_run;
mod game_settings;
mod home;
mod icons;
mod library_page;
mod pages;
mod settings_page;
mod shell;
mod tiles;
mod versions;

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use cerberus_shared::{Game, InstallStep, NextStep, RecompEntry};
use leptos::prelude::*;
use leptos::task::spawn_local;

use art::{Poster, cover_sources};

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}

#[derive(Clone, PartialEq, Debug)]
pub enum Route {
    Home,
    Library,
    Discover,
    Downloads,
    Saves,
    Settings,
    /// The full-screen view for a TV and a controller.
    BigPicture,
    /// A game's page, by library id; `recomp:<id>` works for recomps not yet installed.
    Game(String),
}

impl Route {
    /// The sidebar item a page belongs to.
    fn section(&self) -> Route {
        match self {
            Route::Game(id) if id.starts_with("recomp:") => Route::Discover,
            Route::Game(_) => Route::Library,
            other => other.clone(),
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct Toast {
    id: u64,
    text: String,
    error: bool,
}

/// Shared by every component: what's listed, what's happening, and where we are.
#[derive(Clone, Copy)]
pub struct Library {
    games: LocalResource<Result<Vec<Game>, String>>,
    recomps: LocalResource<Result<Vec<RecompEntry>, String>>,
    /// Recomps installing right now, by recomp id.
    progress: RwSignal<HashMap<String, InstallStep>>,
    /// Ids of games the launcher started that are still open.
    running: RwSignal<HashSet<String>>,
    route: RwSignal<Route>,
    /// Pages to go back to.
    history: RwSignal<Vec<Route>>,
    editing: RwSignal<Option<Game>>,
    toasts: RwSignal<Vec<Toast>>,
    /// The "Trazer seus recomps" window is open.
    adopting: RwSignal<bool>,
    /// The game whose art is being customized.
    customizing: RwSignal<Option<Game>>,
}

impl Library {
    pub fn go(self, route: Route) {
        let now = self.route.get_untracked();
        if now == route {
            return;
        }
        self.history.update(|h| h.push(now));
        self.route.set(route);
    }

    /// Goes somewhere without leaving a step to come back to, like switching versions of a game.
    pub fn replace(self, route: Route) {
        self.route.set(route);
    }

    pub fn back(self) {
        let previous = self.history.try_update(|h| h.pop()).flatten();
        self.route.set(previous.unwrap_or(Route::Home));
    }

    /// Shows an error as a toast that stays until closed; passes a success through.
    pub fn report<T>(self, result: Result<T, String>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(error) => {
                self.push_toast(error, true);
                None
            }
        }
    }

    /// Good news, gone on its own after a few seconds.
    pub fn toast(self, text: impl Into<String>) {
        let id = self.push_toast(text.into(), false);
        let toasts = self.toasts;
        set_timeout(move || toasts.update(|all| all.retain(|t| t.id != id)), Duration::from_secs(5));
    }

    fn push_toast(self, text: String, error: bool) -> u64 {
        let id = js_sys::Date::now() as u64 ^ (text.len() as u64);
        self.toasts.update(|all| all.push(Toast { id, text, error }));
        id
    }

    pub fn add_game(self) {
        spawn_local(async move {
            let added = self.report(api::add_local_game().await).flatten();
            if let Some(game) = added {
                self.games.refetch();
                // The editor opens right away, so a wrong name or cover is easy to fix.
                self.editing.set(Some(game));
            }
        });
    }
}

/// The main button's label and icon: the step the backend says comes next.
fn launch_action(game: &Game) -> (&'static str, icons::Icon) {
    match &game.next_step {
        NextStep::Play => ("Jogar", icons::Icon::Play),
        NextStep::RunInstaller => ("Abrir instalador", icons::Icon::External),
        NextStep::ChooseCopy { found: Some(_) } => ("Usar minha cópia", icons::Icon::Check),
        NextStep::ChooseCopy { found: None } => ("Escolher cópia do jogo", icons::Icon::Folder),
        NextStep::Locate { .. } => ("Localizar programa", icons::Icon::Search),
    }
}

/// The main button everywhere: takes the next step, and plays once nothing is left.
pub fn play_game(library: Library, game: Game) {
    spawn_local(async move {
        let ready = match &game.next_step {
            NextStep::Play | NextStep::RunInstaller => Some(game),
            NextStep::ChooseCopy { found } => {
                let set = library.report(api::set_game_copy(&game.id, found.is_some(), false).await).flatten();
                library.games.refetch();
                library.recomps.refetch();
                if set.is_some() {
                    library.toast(format!("Cópia de {} configurada.", game.name));
                }
                // Plays right away; Play itself refuses if anything is still missing.
                set
            }
            NextStep::Locate { reason } => {
                library.toast(reason.clone());
                let found = library.report(api::locate_game(&game.id).await).flatten();
                library.games.refetch();
                if let Some(found) = &found {
                    library.toast(format!("{} está pronto para jogar.", found.name));
                }
                None
            }
        };
        if let Some(game) = ready {
            library.report(api::play(&game).await);
        }
    });
}

#[component]
fn App() -> impl IntoView {
    let library = Library {
        games: LocalResource::new(api::list_games),
        recomps: LocalResource::new(api::list_recomps),
        progress: RwSignal::new(HashMap::new()),
        running: RwSignal::new(HashSet::new()),
        route: RwSignal::new(Route::Home),
        history: RwSignal::new(Vec::new()),
        editing: RwSignal::new(None),
        toasts: RwSignal::new(Vec::new()),
        adopting: RwSignal::new(false),
        customizing: RwSignal::new(None),
    };
    provide_context(library);
    provide_context(library_page::LibraryView::default());

    // Games may already be open if the window was reloaded.
    spawn_local(async move {
        if let Ok(ids) = api::running_games().await {
            library.running.set(ids.into_iter().collect());
        }
    });
    api::on_session(move |event| {
        library.running.update(|all| {
            if event.running {
                all.insert(event.game_id.clone());
            } else {
                all.remove(&event.game_id);
            }
        });
    });
    api::on_playtime_updated(move || library.games.refetch());
    // Steps arrive while `install_recomp` is still running; its own result reports
    // failures, so the events only move the progress bar.
    // A newer catalog arrived, or the ROM folders were looked through again.
    api::on_catalog_changed(move || {
        library.recomps.refetch();
        library.games.refetch();
    });
    api::on_toast(move |text| library.toast(text));
    tv::watch_for_tv(library);

    // The first time: the setup. Later, straight into the TV screen when that is how the player plays.
    let first_run = RwSignal::new(None::<cerberus_shared::SettingsView>);
    let first_run_open = RwSignal::new(false);
    spawn_local(async move {
        let Ok(settings) = api::get_settings().await else { return };
        if !settings.setup_done {
            first_run_open.set(true);
            first_run.set(Some(settings));
        } else if settings.start_in_tv {
            library.go(Route::BigPicture);
        }
    });
    let setup = move || {
        first_run.get().filter(|_| first_run_open.get()).map(|settings| view! { <first_run::FirstRun settings open=first_run_open /> })
    };
    // An installer finished but its game wasn't found: ask where it went.
    let locating: RwSignal<Option<String>> = RwSignal::new(None);
    api::on_needs_location(move |id| locating.set(Some(id)));
    api::on_install_progress(move |progress| {
        library.progress.update(|all| match progress.step {
            InstallStep::Failed { .. } => {
                all.remove(&progress.recomp_id);
            }
            step => {
                all.insert(progress.recomp_id, step);
            }
        });
    });

    let page = move || match library.route.get() {
        Route::Home => view! { <home::Home /> }.into_any(),
        Route::Library => view! { <library_page::LibraryPage /> }.into_any(),
        Route::Discover => view! { <discover::DiscoverPage /> }.into_any(),
        Route::Downloads => view! { <pages::DownloadsPage /> }.into_any(),
        Route::Saves => view! { <pages::SavesPage /> }.into_any(),
        Route::Settings => view! { <settings_page::SettingsPage /> }.into_any(),
        // Drawn over the whole window instead, below.
        Route::BigPicture => ().into_any(),
        Route::Game(id) => view! { <game::GamePage id /> }.into_any(),
    };
    let editor = move || library.editing.get().map(|game| view! { <EditDialog game /> });
    let locate = move || {
        let id = locating.get()?;
        let pick = move |_| {
            let id = id.clone();
            spawn_local(async move {
                if library.report(api::locate_game(&id).await).flatten().is_some() {
                    locating.set(None);
                    library.games.refetch();
                    library.toast("Pronto: o jogo está na biblioteca e abre pelo Cerberus.");
                }
            });
        };
        Some(view! {
            <div class="backdrop-dim">
                <div class="dialog">
                    <h2>"Onde o jogo foi instalado?"</h2>
                    <p class="muted">"O instalador terminou, mas o Cerberus não achou o jogo sozinho. Escolha o programa do jogo (o .exe) na pasta onde o instalador o colocou."</p>
                    <div class="dialog-footer">
                        <button type="button" class="btn" on:click=move |_| locating.set(None)>"Depois"</button>
                        <button type="button" class="btn main" on:click=pick>"Escolher programa..."</button>
                    </div>
                </div>
            </div>
        })
    };
    let big_picture = move || (library.route.get() == Route::BigPicture).then(|| view! { <tv::TvMode /> });

    view! {
        <div class="app">
            <shell::Sidebar />
            <div class="mainwrap">
                <shell::TitleBar />
                <main class="main">{page}</main>
            </div>
        </div>
        {big_picture}
        {setup}
        <Show when=move || library.adopting.get()><adopt::AdoptDialog /></Show>
        {move || library.customizing.get().map(|game| view! { <customize::CustomizeDialog game /> })}
        <shell::Toasts />
        {editor}
        {locate}
    }
}

#[component]
fn EditDialog(game: Game) -> impl IntoView {
    let library = expect_context::<Library>();
    let id = StoredValue::new(game.id.clone());
    let name = RwSignal::new(game.name.clone());
    let current = RwSignal::new(game);
    let busy = RwSignal::new(false);

    let close = move || library.editing.set(None);

    // Runs one backend change; while it runs every button is disabled.
    let run = move |task: std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<Game>, String>>>>| {
        busy.set(true);
        spawn_local(async move {
            let updated = library.report(task.await).flatten();
            busy.set(false);
            if let Some(game) = updated {
                current.set(game);
                library.games.refetch();
            }
        });
    };
    let on_find = move |_| {
        let id = id.get_value();
        run(Box::pin(async move { api::find_cover(&id).await.map(Some) }));
    };
    let on_pick = move |_| {
        let id = id.get_value();
        run(Box::pin(async move { api::pick_cover(&id).await }));
    };
    let on_save = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        busy.set(true);
        spawn_local(async move {
            let renamed = library.report(api::rename_game(&id.get_value(), &name.get_untracked()).await);
            busy.set(false);
            if renamed.is_some() {
                library.games.refetch();
                close();
            }
        });
    };

    let preview = move || {
        let game = current.get();
        view! { <Poster sources=cover_sources(&game) title=game.name.clone() /> }
    };

    view! {
        <div class="backdrop-dim" on:click=move |_| close()>
            <form class="dialog" on:click=|event| event.stop_propagation() on:submit=on_save>
                <h2>"Editar jogo"</h2>
                <div class="dialog-body">
                    <div class="preview">{preview}</div>
                    <div class="fields">
                        <label for="game-name">"Nome"</label>
                        <input
                            id="game-name"
                            class="input"
                            prop:value=move || name.get()
                            on:input=move |event| name.set(event_target_value(&event))
                        />
                        <div class="row">
                            <button type="button" class="btn sm" on:click=on_find disabled=move || busy.get()>
                                "Buscar capa na Steam"
                            </button>
                            <button type="button" class="btn sm" on:click=on_pick disabled=move || busy.get()>
                                "Escolher imagem..."
                            </button>
                        </div>
                        <p class="muted small">"A busca usa o nome acima. Mude o nome e busque de novo se a capa vier errada."</p>
                    </div>
                </div>
                <div class="dialog-footer">
                    <button type="button" class="btn" on:click=move |_| close()>"Cancelar"</button>
                    <button type="submit" class="btn main" disabled=move || busy.get()>"Salvar"</button>
                </div>
            </form>
        </div>
    }
}
