mod api;
mod art;
mod backups;
mod game_overview;
mod game_notes;
mod tv;
mod discover;
mod format;
mod game;
mod classics;
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
mod news;
mod game_menu;
mod versions;
mod achievements;
mod i18n;
mod overlay;
mod unlock;
mod updates;

use cerberus_shared::tr;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

use cerberus_shared::{Game, InstallStep, NextStep, RecompEntry};
use leptos::prelude::*;
use leptos::task::spawn_local;

use art::{Poster, cover_sources};

fn main() {
    console_error_panic_hook::set_once();
    i18n::init();
    if unlock::is_unlock() {
        if let Some(root) = document().document_element() {
            let _ = root.class_list().add_1("overlay-mode");
        }
        leptos::mount::mount_to_body(unlock::UnlockToast);
        return;
    }
    if overlay::is_overlay() {
        leptos::mount::mount_to_body(overlay::OverlayMenu);
        return;
    }
    leptos::mount::mount_to_body(App);
}

#[derive(Clone, PartialEq, Debug)]
pub enum Route {
    Home,
    Library,
    Discover,
    Downloads,
    Saves,
    /// Updates waiting and what happened lately.
    News,
    Settings,
    /// The full-screen view for a TV and a controller.
    BigPicture,
    /// A game's page, by library id; `recomp:<id>` works for recomps not yet installed.
    Game(String),
    /// A classic from the ROM folders, by its file.
    Classic(String),
    /// The achievements hub.
    Achievements,
    /// One game's achievements: its source ("steam", "retro") and id there.
    AchievementGame(String, String),
}

impl Route {
    /// The sidebar item a page belongs to.
    fn section(&self) -> Route {
        match self {
            Route::Game(id) if id.starts_with("recomp:") => Route::Discover,
            Route::Game(_) | Route::Classic(_) => Route::Library,
            Route::AchievementGame(..) => Route::Achievements,
            other => other.clone(),
        }
    }
}

/// Marks or unmarks a game as a favorite (only that game changes on screen).
pub fn toggle_favorite(library: Library, id: String, on: bool) {
    leptos::task::spawn_local(async move {
        if library.report(api::set_favorite(id.clone(), on).await).is_some() {
            library.patch_game(&id, |g| g.favorite = on);
            library.toast(if on { tr!("Adicionado aos favoritos.") } else { tr!("Tirado dos favoritos.") });
        }
    });
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
    /// The classics in the player's ROM folders (refetched when a page shows them).
    roms: LocalResource<Vec<cerberus_shared::RomView>>,
    /// Who opens each console's classics on this PC.
    players: LocalResource<Vec<cerberus_shared::ClassicPlayer>>,
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
    /// Installs that didn't finish this session: recomp id -> why.
    failed: RwSignal<std::collections::BTreeMap<String, String>>,
    /// Download speed samples (bytes per second), newest last, by recomp id.
    speed: RwSignal<HashMap<String, Vec<f64>>>,
    /// When each running game was seen starting (ms since 1970).
    started: RwSignal<HashMap<String, f64>>,
    /// The same game in several stores: each copy's id to the whole group,
    /// the copy shown first leading.
    copies: Memo<HashMap<String, Vec<String>>>,
    /// The recomp catalog is shown (the player can turn it off in Settings).
    catalog_on: RwSignal<bool>,
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

    /// The game's copies in every store (its own first when it leads), or none.
    pub fn copies_of(self, id: &str) -> Vec<String> {
        self.copies.with(|all| all.get(id).cloned().unwrap_or_default())
    }

    /// A copy shown under another one: left out of lists, which show the lead copy with every store's logo.
    pub fn is_extra_copy(self, id: &str) -> bool {
        self.copies.with(|all| all.get(id).is_some_and(|group| group.first().is_some_and(|lead| lead != id)))
    }

    pub fn back(self) {
        let previous = self.history.try_update(|h| h.pop()).flatten();
        self.route.set(previous.unwrap_or(Route::Home));
    }

    /// Shows an error as a toast that stays until closed; passes a success through.
    /// Changes one game as listed, without listing the library again (a
    /// listing reads every store: a heart shouldn't cost that).
    pub fn patch_game(self, id: &str, change: impl FnOnce(&mut Game)) {
        self.games.update(|listed| {
            if let Some(Ok(games)) = listed {
                if let Some(game) = games.iter_mut().find(|g| g.id == id) {
                    change(game);
                }
            }
        });
    }

    pub fn report<T>(self, result: Result<T, String>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(error) => {
                let (_, message) = cerberus_shared::uninstall::split(&error);
                self.push_toast(message.to_string(), true);
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
        // Messages kept in Portuguese (like a cancelled install) show in the player's language.
        let text = cerberus_shared::i18n::translate(&text);
        // A counter: two notices in the same millisecond never share an id (and a timer).
        thread_local! { static NEXT: std::cell::Cell<u64> = const { std::cell::Cell::new(1) }; }
        let id = NEXT.with(|n| {
            let id = n.get();
            n.set(id + 1);
            id
        });
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
pub fn launch_action(game: &Game) -> (&'static str, icons::Icon) {
    match &game.next_step {
        NextStep::Play => (tr!("Jogar"), icons::Icon::Play),
        NextStep::RunInstaller => (tr!("Abrir instalador"), icons::Icon::External),
        NextStep::ChooseCopy { found: Some(_) } => (tr!("Usar minha cópia"), icons::Icon::Check),
        NextStep::ChooseCopy { found: None } => (tr!("Escolher cópia do jogo"), icons::Icon::Folder),
        NextStep::ChooseInput { .. } => (tr!("Escolher o que falta"), icons::Icon::Folder),
        NextStep::Locate { .. } => (tr!("Localizar programa"), icons::Icon::Search),
        NextStep::Install => (tr!("Instalar"), icons::Icon::Download),
    }
}

/// The main button everywhere: takes the next step, and plays once nothing is left.
pub fn play_game(library: Library, game: Game) {
    // Wherever it's launched from, the native way was the last one used.
    if matches!(game.next_step, NextStep::Play) {
        classics::remember_way(&game.id, classics::Way::Native);
    }
    spawn_local(async move {
        let ready = match &game.next_step {
            NextStep::Play | NextStep::RunInstaller => Some(game),
            // The store's own install window opens; the tile turns into Play when it's done.
            NextStep::Install => {
                library.toast(match game.source {
                    cerberus_shared::Source::Xbox { .. } => tr!("O app Xbox abre na página de {}: instale por lá. Quando terminar, ele aparece pronto para jogar.", game.name),
                    cerberus_shared::Source::Epic { .. } => tr!("A Epic vai instalar {}. Quando terminar, ele aparece pronto para jogar.", game.name),
                    _ => tr!("A Steam vai instalar {}. Quando terminar, ele aparece pronto para jogar.", game.name),
                });
                Some(game)
            }
            NextStep::ChooseCopy { found } => {
                let set = library.report(api::set_game_copy(&game.id, found.is_some(), false).await).flatten();
                library.games.refetch();
                library.recomps.refetch();
                if set.is_some() {
                    library.toast(tr!("Cópia de {} configurada.", game.name));
                }
                // Plays right away; Play itself refuses if anything is still missing.
                set
            }
            // A second disc or a key: picked on the game's page, under "The game also needs".
            NextStep::ChooseInput { label } => {
                library.toast(tr!("Falta: {}. Escolha em \"O jogo também precisa\".", label));
                library.go(Route::Game(game.id.clone()));
                None
            }
            NextStep::Locate { reason } => {
                library.toast(reason.clone());
                let found = library.report(api::locate_game(&game.id).await).flatten();
                library.games.refetch();
                if let Some(found) = &found {
                    library.toast(tr!("{} está pronto para jogar.", found.name));
                }
                None
            }
        };
        if let Some(game) = ready {
            library.report(api::play(&game, library.route.get_untracked() == Route::BigPicture).await);
        }
    });
}

#[component]
fn App() -> impl IntoView {
    let games = LocalResource::new(api::list_games);
    let copies = Memo::new(move |_| {
        games.with(|listed| {
            let mut map = HashMap::new();
            if let Some(Ok(all)) = listed {
                for group in cerberus_shared::names::copies(all) {
                    for id in &group {
                        map.insert(id.clone(), group.clone());
                    }
                }
            }
            map
        })
    });
    let library = Library {
        games,
        copies,
        recomps: LocalResource::new(api::list_recomps),
        roms: LocalResource::new(|| async { api::get_settings().await.map(|s| s.roms).unwrap_or_default() }),
        players: LocalResource::new(api::classic_players),
        progress: RwSignal::new(HashMap::new()),
        running: RwSignal::new(HashSet::new()),
        route: RwSignal::new(Route::Home),
        history: RwSignal::new(Vec::new()),
        editing: RwSignal::new(None),
        toasts: RwSignal::new(Vec::new()),
        adopting: RwSignal::new(false),
        customizing: RwSignal::new(None),
        failed: RwSignal::new(Default::default()),
        speed: RwSignal::new(HashMap::new()),
        started: RwSignal::new(HashMap::new()),
        catalog_on: RwSignal::new(true),
    };
    provide_context(library);
    provide_context(game_menu::GameMenu(RwSignal::new(None)));
    // Setup steps running now, by game: one listener for the whole app.
    let preparing = game::Preparing(RwSignal::new(HashMap::new()));
    api::on_prepare_progress(move |p| {
        preparing.0.update(|all| {
            if p.finished {
                all.remove(&p.game_id);
            } else {
                all.insert(p.game_id.clone(), p);
            }
        })
    });
    provide_context(preparing);
    provide_context(updates::Updates::start());
    provide_context(news::Activity::start());
    provide_context(library_page::LibraryView::default());

    // Games may already be open if the window was reloaded.
    spawn_local(async move {
        if let Ok(ids) = api::running_games().await {
            library.running.set(ids.into_iter().collect());
        }
    });
    api::on_session(move |event| {
        library.started.update(|all| {
            if event.running {
                all.entry(event.game_id.clone()).or_insert_with(js_sys::Date::now);
            } else {
                all.remove(&event.game_id);
            }
        });
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
    // A burst of these (several stores finishing at once) is read once, 300 ms after the last.
    let waiting = StoredValue::new(None::<leptos::prelude::TimeoutHandle>);
    api::on_catalog_changed(move || {
        if let Some(Some(old)) = waiting.try_get_value() {
            old.clear();
        }
        let handle = leptos::prelude::set_timeout_with_handle(
            move || {
                library.recomps.refetch();
                library.games.refetch();
                library.roms.refetch();
                library.players.refetch();
            },
            Duration::from_millis(300),
        )
        .ok();
        let _ = waiting.try_set_value(handle);
    });
    api::on_toast(move |text| library.toast(text));
    tv::watch_for_tv(library);
    settings_page::apply_calm(settings_page::calm_saved());
    settings_page::apply_size(settings_page::size_saved());
    settings_page::apply_look(settings_page::look_saved());

    // The first time: the setup. Later, straight into the TV screen when that is how the player plays.
    let first_run = RwSignal::new(None::<cerberus_shared::SettingsView>);
    let first_run_open = RwSignal::new(false);
    spawn_local(async move {
        let Ok(settings) = api::get_settings().await else { return };
        i18n::follow(settings.language.as_deref());
        library.catalog_on.set(settings.recomp_catalog);
        if !settings.setup_done {
            first_run_open.set(true);
            first_run.set(Some(settings));
        } else if settings.start_in_tv {
            library.go(Route::BigPicture);
        } else {
            match settings.start_page.as_str() {
                "library" => library.go(Route::Library),
                "discover" => library.go(Route::Discover),
                _ => {}
            }
        }
    });
    let setup = move || {
        first_run.get().filter(|_| first_run_open.get()).map(|settings| view! { <first_run::FirstRun settings open=first_run_open /> })
    };
    // An installer finished but its game wasn't found: ask where it went.
    let locating: RwSignal<Option<String>> = RwSignal::new(None);
    api::on_needs_location(move |id| locating.set(Some(id)));
    // Speed from how many bytes arrived since the last sample, at most twice a second.
    let last_sample = StoredValue::new(HashMap::<String, (f64, u64)>::new());
    // What was paused or didn't finish last time comes back to Downloads.
    leptos::task::spawn_local(async move {
        if let Ok(kept) = api::download_queue().await {
            for p in kept {
                match p.step {
                    InstallStep::Failed { message } => library.failed.update(|all| {
                        all.entry(p.recomp_id).or_insert(message);
                    }),
                    step => library.progress.update(|all| {
                        all.entry(p.recomp_id).or_insert(step);
                    }),
                }
            }
        }
    });
    api::on_install_progress(move |progress| {
        if let InstallStep::Downloading { done, .. } = progress.step {
            let now = js_sys::Date::now();
            let id = progress.recomp_id.clone();
            let before = last_sample.with_value(|m| m.get(&id).copied());
            match before {
                Some((t, d)) if now - t >= 500.0 => {
                    let rate = done.saturating_sub(d) as f64 / ((now - t) / 1000.0);
                    last_sample.update_value(|m| {
                        m.insert(id.clone(), (now, done));
                    });
                    library.speed.update(|all| {
                        let list = all.entry(id).or_default();
                        list.push(rate);
                        if list.len() > 60 {
                            list.remove(0);
                        }
                    });
                }
                Some(_) => {}
                None => last_sample.update_value(|m| {
                    m.insert(id, (now, done));
                }),
            }
        } else {
            last_sample.update_value(|m| {
                m.remove(&progress.recomp_id);
            });
        }
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
        Route::News => view! { <news::NewsPage /> }.into_any(),
        Route::Achievements => view! { <achievements::AchievementsPage /> }.into_any(),
        Route::AchievementGame(source, key) => view! { <achievements::AchievementGamePage source key /> }.into_any(),
        Route::Classic(path) => view! { <classics::ClassicPage path /> }.into_any(),
        Route::Settings => view! { <settings_page::SettingsPage /> }.into_any(),
        // Drawn over the whole window instead, below.
        Route::BigPicture => ().into_any(),
        Route::Game(id) => view! { <game::GamePage id /> }.into_any(),
    };
    // Every page opens at its top, not where the last one was scrolled to.
    let main_ref = NodeRef::<leptos::html::Main>::new();
    Effect::new(move |_| {
        library.route.track();
        if let Some(main) = main_ref.get_untracked() {
            main.set_scroll_top(0);
        }
    });
    // A desktop shortcut's game: played once the library is listed.
    let wanted = RwSignal::new(None::<String>);
    spawn_local(async move {
        if let Some(id) = api::take_play_request().await {
            wanted.set(Some(id));
        }
    });
    // The request is kept in the backend: taken from there, so a game is played once.
    api::on_play_request(move |_| {
        spawn_local(async move {
            if let Some(id) = api::take_play_request().await {
                let _ = wanted.try_set(Some(id));
            }
        });
    });
    api::on_show_game(move |id: String| library.go(Route::Game(id)));
    Effect::new(move |_| {
        let Some(id) = wanted.get() else { return };
        let Some(Ok(games)) = library.games.get() else { return };
        wanted.set(None);
        match games.into_iter().find(|g| g.id == id) {
            Some(game) => {
                library.go(Route::Game(game.id.clone()));
                play_game(library, game);
            }
            None => library.toast(tr!("O jogo desse atalho não está mais na biblioteca.")),
        }
    });
    let editor = move || library.editing.get().map(|game| view! { <EditDialog game /> });
    let locate = move || {
        let id = locating.get()?;
        let pick = move |_| {
            let id = id.clone();
            spawn_local(async move {
                if library.report(api::locate_game(&id).await).flatten().is_some() {
                    locating.set(None);
                    library.games.refetch();
                    library.toast(tr!("Pronto: o jogo está na biblioteca e abre pelo Cerberus."));
                }
            });
        };
        Some(view! {
            <div class="backdrop-dim">
                <div class="dialog">
                    <h2>{tr!("Onde o jogo foi instalado?")}</h2>
                    <p class="muted">{tr!("O instalador terminou, mas o Cerberus não achou o jogo sozinho. Escolha o programa do jogo (o .exe) na pasta onde o instalador o colocou.")}</p>
                    <div class="dialog-footer">
                        <button type="button" class="btn" on:click=move |_| locating.set(None)>{tr!("Depois")}</button>
                        <button type="button" class="btn main" on:click=pick>{tr!("Escolher programa...")}</button>
                    </div>
                </div>
            </div>
        })
    };
    let big_picture = move || (library.route.get() == Route::BigPicture).then(|| view! { <tv::TvMode /> });
    // Outside Big Picture Mode the window is never full screen, however the TV
    // screen was left (its own cleanup can be skipped when it closes mid-event).
    Effect::new(move |_| {
        if library.route.get() != Route::BigPicture {
            api::window_toggle("setFullscreen", false);
        }
    });

    view! {
        <div class="app">
            <shell::Sidebar />
            <div class="mainwrap">
                <shell::TitleBar />
                <main class="main" node_ref=main_ref>{page}</main>
            </div>
        </div>
        {big_picture}
        {setup}
        <Show when=move || library.adopting.get()><adopt::AdoptDialog /></Show>
        {move || library.customizing.get().map(|game| view! { <customize::CustomizeDialog game /> })}
        <shell::Toasts />
        <updates::UpdateDialog />
        <updates::WhatsNewDialog />
        {editor}
        {locate}
        <game_menu::GameMenuLayer />
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
                <h2>{tr!("Editar jogo")}</h2>
                <div class="dialog-body">
                    <div class="preview">{preview}</div>
                    <div class="fields">
                        <label for="game-name">{tr!("Nome")}</label>
                        <input
                            id="game-name"
                            class="input"
                            prop:value=move || name.get()
                            on:input=move |event| name.set(event_target_value(&event))
                        />
                        <div class="row">
                            <button type="button" class="btn sm" on:click=on_find disabled=move || busy.get()>
                                {tr!("Buscar capa na Steam")}
                            </button>
                            <button type="button" class="btn sm" on:click=on_pick disabled=move || busy.get()>
                                {tr!("Escolher imagem...")}
                            </button>
                        </div>
                        <p class="muted small">{tr!("A busca usa o nome acima. Mude o nome e busque de novo se a capa vier errada.")}</p>
                    </div>
                </div>
                <div class="dialog-footer">
                    <button type="button" class="btn" on:click=move |_| close()>{tr!("Cancelar")}</button>
                    <button type="submit" class="btn main" disabled=move || busy.get()>{tr!("Salvar")}</button>
                </div>
            </form>
        </div>
    }
}
