//! Big Picture Mode: the launcher full screen, for a TV and a controller.
//!
//! Five sections in tabs along the top (LB/RB): Início, Biblioteca, Descobrir,
//! Downloads and Configurações. A game opens its own page; B goes back to the
//! very item you came from, and on Início it asks whether to leave. X opens a
//! game's options anywhere. A button bar at the bottom shows what each button
//! does, drawn for the controller in use (Xbox, PlayStation or Nintendo).

mod achievements;
mod focus;
mod game;
pub(crate) mod input;
mod pages;
mod sheets;
mod sound;

use cerberus_shared::{tk, tr};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use leptos::prelude::*;

use crate::art::Backdrop;
use crate::icons::{Icon, Svg};
use crate::{Library, Route, api};
pub use focus::Focus;
pub use input::{Btn, PadKind};
pub use sound::{Sound, play};
pub use sound::{enabled as sound_enabled, set_enabled as set_sound_enabled};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Tab {
    Home,
    Library,
    Discover,
    Achievements,
    Downloads,
    Settings,
}

const TABS: [(Tab, &str, Icon); 6] = [
    (Tab::Home, tk!("Início"), Icon::Home),
    (Tab::Library, tk!("Biblioteca"), Icon::Library),
    (Tab::Discover, tk!("Descobrir"), Icon::Discover),
    (Tab::Achievements, tk!("Conquistas"), Icon::Trophy),
    (Tab::Downloads, tk!("Downloads"), Icon::Download),
    (Tab::Settings, tk!("Configurações"), Icon::Settings),
];

#[derive(Clone, PartialEq, Debug)]
pub enum View {
    Tab(Tab),
    /// A game in the library (its id) or in the catalog (`recomp:<id>`).
    Game(String),
    /// One game's achievements (source code and key), for games not in the library.
    Achievements(String, String),
}

impl View {
    fn key(&self) -> String {
        match self {
            View::Tab(t) => format!("{t:?}"),
            View::Game(id) => format!("game:{id}"),
            View::Achievements(source, key) => format!("ach:{source}:{key}"),
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Sheet {
    Leave,
    Options(String),
    ConfirmRemove(String),
    Keyboard,
}

type Handler = Rc<dyn Fn(Btn) -> bool>;

/// The TV screen's shared state, in context for its pages and sheets.
#[derive(Clone, Copy)]
pub struct Tv {
    pub view: RwSignal<View>,
    stack: RwSignal<Vec<View>>,
    pub sheet: RwSignal<Option<Sheet>>,
    pub pad: RwSignal<PadKind>,
    pub search: RwSignal<String>,
    /// The Library's own search, apart from Discover's.
    pub lib_search: RwSignal<String>,
    /// The on-screen keyboard types into the Library's search (else Discover's).
    pub kb_lib: RwSignal<bool>,
    pub hints: RwSignal<Vec<(Btn, &'static str)>>,
    pub sheet_hints: RwSignal<Vec<(Btn, &'static str)>>,
    backdrop: RwSignal<Vec<String>>,
    pending_backdrop: StoredValue<Vec<String>>,
    page_handler: StoredValue<Option<(u64, Handler)>, LocalStorage>,
    sheet_handler: StoredValue<Option<(u64, Handler)>, LocalStorage>,
    next_id: StoredValue<u64>,
    /// Where the focus was on each page, to come back to it.
    positions: StoredValue<HashMap<String, (usize, usize)>>,
}

impl Tv {
    pub fn open(&self, view: View) {
        let current = self.view.get_untracked();
        if current == view {
            return;
        }
        self.stack.update(|s| s.push(current));
        self.view.set(view);
        play(Sound::Select);
    }

    /// Back one page; false when there is nothing behind.
    pub fn back(&self) -> bool {
        let Some(previous) = self.stack.try_update(|s| s.pop()).flatten() else { return false };
        self.view.set(previous);
        play(Sound::Back);
        true
    }

    pub fn tab(&self, tab: Tab) {
        self.stack.set(Vec::new());
        self.view.set(View::Tab(tab));
    }

    fn register(&self, slot: StoredValue<Option<(u64, Handler)>, LocalStorage>, handler: Handler) {
        let id = self.next_id.get_value() + 1;
        self.next_id.set_value(id);
        slot.set_value(Some((id, handler)));
        on_cleanup(move || {
            slot.update_value(|s| {
                if s.as_ref().is_some_and(|(mine, _)| *mine == id) {
                    *s = None;
                }
            });
        });
    }

    /// The current page's buttons; return true when a press was used.
    pub fn handle_page(&self, handler: impl Fn(Btn) -> bool + 'static) {
        self.register(self.page_handler, Rc::new(handler));
    }

    /// An open sheet takes every press.
    pub fn handle_sheet(&self, handler: impl Fn(Btn) -> bool + 'static) {
        self.register(self.sheet_handler, Rc::new(handler));
    }

    /// The page's focus starts where it was left, and is kept when the page goes.
    pub fn keep_position(&self, focus: Focus) {
        let key = self.view.get_untracked().key();
        if let Some((row, col)) = self.positions.with_value(|p| p.get(&key).copied()) {
            focus.set(row, col);
        }
        let positions = self.positions;
        on_cleanup(move || {
            let at = (focus.row.get_untracked(), focus.col.get_untracked());
            positions.update_value(|p| {
                p.insert(key, at);
            });
        });
    }

    /// The picture behind everything follows the focus after a short pause, so
    /// scrolling quickly doesn't flicker through every game's art.
    pub fn show_backdrop(&self, sources: Vec<String>) {
        self.pending_backdrop.set_value(sources.clone());
        let pending = self.pending_backdrop;
        let backdrop = self.backdrop;
        set_timeout(
            // The TV may have closed in the meantime: only `try_` reads, never a panic.
            move || {
                let still = pending.try_with_value(|p| *p == sources).unwrap_or(false);
                if still && backdrop.try_with_untracked(|b| *b != sources).unwrap_or(false) {
                    let _ = backdrop.try_set(sources);
                }
            },
            Duration::from_millis(220),
        );
    }
}

/// One controller button, drawn the way the player's pad prints it.
pub fn glyph(btn: Btn, kind: PadKind) -> impl IntoView {
    let (text, class) = match (btn, kind) {
        (Btn::A, PadKind::PlayStation) => ("✕", "ps-cross"),
        (Btn::B, PadKind::PlayStation) => ("○", "ps-circle"),
        (Btn::X, PadKind::PlayStation) => ("□", "ps-square"),
        (Btn::Y, PadKind::PlayStation) => ("△", "ps-triangle"),
        (Btn::Lb, PadKind::PlayStation) => ("L1", "shoulder"),
        (Btn::Rb, PadKind::PlayStation) => ("R1", "shoulder"),
        (Btn::Lb, PadKind::Nintendo) => ("L", "shoulder"),
        (Btn::Rb, PadKind::Nintendo) => ("R", "shoulder"),
        (Btn::A, PadKind::Nintendo) => ("A", "nin"),
        (Btn::B, PadKind::Nintendo) => ("B", "nin"),
        (Btn::X, PadKind::Nintendo) => ("X", "nin"),
        (Btn::Y, PadKind::Nintendo) => ("Y", "nin"),
        (Btn::A, _) => ("A", "xb-a"),
        (Btn::B, _) => ("B", "xb-b"),
        (Btn::X, _) => ("X", "xb-x"),
        (Btn::Y, _) => ("Y", "xb-y"),
        (Btn::Lb, _) => ("LB", "shoulder"),
        (Btn::Lt, PadKind::PlayStation) => ("L2", "shoulder"),
        (Btn::Rt, PadKind::PlayStation) => ("R2", "shoulder"),
        (Btn::Lt, PadKind::Nintendo) => ("ZL", "shoulder"),
        (Btn::Rt, PadKind::Nintendo) => ("ZR", "shoulder"),
        (Btn::Lt, _) => ("LT", "shoulder"),
        (Btn::Rt, _) => ("RT", "shoulder"),
        (Btn::Rb, _) => ("RB", "shoulder"),
        (Btn::Menu | Btn::MenuHold, PadKind::PlayStation) => ("☰", "shoulder"),
        (Btn::Menu | Btn::MenuHold, _) => ("☰", "shoulder"),
        _ => ("✥", "shoulder"),
    };
    view! { <span class=format!("tv-btn {class}")>{text}</span> }
}

fn clock() -> String {
    let now = js_sys::Date::new_0();
    format!("{:02}:{:02}", now.get_hours(), now.get_minutes())
}

#[component]
pub fn TvMode() -> impl IntoView {
    let library = expect_context::<Library>();
    // The classics' play times, fresh once on entering (not on every section that passes by).
    library.roms.refetch();
    let tv = Tv {
        view: RwSignal::new(View::Tab(Tab::Home)),
        stack: RwSignal::new(Vec::new()),
        sheet: RwSignal::new(None),
        pad: RwSignal::new(PadKind::default()),
        search: RwSignal::new(String::new()),
        lib_search: RwSignal::new(String::new()),
        kb_lib: RwSignal::new(false),
        hints: RwSignal::new(Vec::new()),
        sheet_hints: RwSignal::new(Vec::new()),
        backdrop: RwSignal::new(Vec::new()),
        pending_backdrop: StoredValue::new(Vec::new()),
        page_handler: StoredValue::new_local(None),
        sheet_handler: StoredValue::new_local(None),
        next_id: StoredValue::new(0),
        positions: StoredValue::new(HashMap::new()),
    };
    provide_context(tv);
    // On the monitor the player picked, then full screen; leaving before that is done stays windowed.
    let here = StoredValue::new(true);
    leptos::task::spawn_local(async move {
        if let Ok(Some(note)) = api::to_chosen_monitor().await {
            library.toast(note);
        }
        if here.try_get_value().unwrap_or(false) {
            api::window_toggle("setFullscreen", true);
        }
    });
    on_cleanup(move || {
        here.try_set_value(false);
        api::window_toggle("setFullscreen", false);
        // Back to the monitor (and place) it was on before Big Picture.
        leptos::task::spawn_local(async move {
            let _ = api::back_from_tv().await;
        });
    });

    let leave = move || library.go(Route::Home);
    let dispatch = move |btn: Btn| {
        let typing = tv.sheet.get_untracked() == Some(Sheet::Keyboard);
        // Letters are buttons, except on the search keyboard.
        let btn = match btn {
            Btn::Type(' ') if !typing => Btn::A,
            Btn::Type(c) if !typing => match c.to_ascii_lowercase() {
                'q' => Btn::Lb,
                'e' => Btn::Rb,
                'x' => Btn::X,
                'y' => Btn::Y,
                _ => return,
            },
            Btn::Erase if !typing => Btn::B,
            other => other,
        };
        if btn == Btn::MenuHold {
            leave();
            return;
        }
        if tv.sheet.get_untracked().is_some() {
            if let Some((_, handler)) = tv.sheet_handler.get_value() {
                handler(btn);
            }
            return;
        }
        if let Some((_, handler)) = tv.page_handler.get_value() {
            if handler(btn) {
                return;
            }
        }
        match btn {
            Btn::Lb | Btn::Rb => {
                // On a game's page, from the section it was opened from (Discover → game → RB is Achievements).
                let tab_index = |t: Tab| TABS.iter().position(|(x, _, _)| *x == t).unwrap_or(0);
                let current = match tv.view.get_untracked() {
                    View::Tab(t) => tab_index(t),
                    View::Game(_) | View::Achievements(..) => tv.stack.with_untracked(|s| s.iter().find_map(|v| if let View::Tab(t) = v { Some(tab_index(*t)) } else { None })).unwrap_or(0),
                };
                let next = if btn == Btn::Rb { (current + 1) % TABS.len() } else { (current + TABS.len() - 1) % TABS.len() };
                tv.tab(TABS[next].0);
                play(Sound::Tab);
            }
            Btn::B => {
                if !tv.back() {
                    if tv.view.get_untracked() == View::Tab(Tab::Home) {
                        tv.sheet.set(Some(Sheet::Leave));
                        play(Sound::Select);
                    } else {
                        tv.tab(Tab::Home);
                        play(Sound::Back);
                    }
                }
            }
            Btn::Menu => {
                tv.tab(Tab::Settings);
                play(Sound::Tab);
            }
            _ => {}
        }
    };
    input::listen(dispatch, tv.pad);

    let time = RwSignal::new(clock());
    let ticker = set_interval_with_handle(move || time.set(clock()), Duration::from_secs(15));
    on_cleanup(move || {
        if let Ok(t) = ticker {
            t.clear();
        }
    });

    let tabs = move || {
        let current = tv.view.get();
        match current {
            View::Game(_) | View::Achievements(..) => view! {
                <div class="tv-back" on:click=move |_| { tv.back(); }>{move || glyph(Btn::B, tv.pad.get())}<span>{tr!("Voltar")}</span></div>
            }
            .into_any(),
            View::Tab(now) => {
                let items = TABS
                    .iter()
                    .map(|(tab, label, icon)| {
                        let on = *tab == now;
                        let badge = (*tab == Tab::Downloads).then(|| {
                            move || {
                                let n = library.progress.with(|p| p.len());
                                (n > 0).then(|| view! { <i class="tv-badge">{n}</i> })
                            }
                        });
                        let target = *tab;
                        view! { <span class="tv-tab" class:on=on on:click=move |_| tv.tab(target)><Svg icon=*icon size=22 />{cerberus_shared::i18n::t(label)}{badge}</span> }
                    })
                    .collect_view();
                view! {
                    <nav class="tv-tabs">
                        {move || glyph(Btn::Lb, tv.pad.get())}
                        {items}
                        {move || glyph(Btn::Rb, tv.pad.get())}
                    </nav>
                }
                .into_any()
            }
        }
    };

    let page = move || match tv.view.get() {
        View::Tab(Tab::Home) => view! { <pages::Home /> }.into_any(),
        View::Tab(Tab::Library) => view! { <pages::LibraryGrid /> }.into_any(),
        View::Tab(Tab::Discover) => view! { <pages::Discover /> }.into_any(),
        View::Tab(Tab::Achievements) => view! { <achievements::Achievements /> }.into_any(),
        View::Tab(Tab::Downloads) => view! { <pages::Downloads /> }.into_any(),
        View::Tab(Tab::Settings) => view! { <pages::Settings /> }.into_any(),
        View::Game(id) => view! { <game::GamePage id /> }.into_any(),
        View::Achievements(source, key) => view! { <achievements::GameList source key /> }.into_any(),
    };

    let hints = move || {
        let list = if tv.sheet.get().is_some() { tv.sheet_hints.get() } else { tv.hints.get() };
        let kind = tv.pad.get();
        let sections = (tv.sheet.get().is_none() && matches!(tv.view.get(), View::Tab(_))).then(|| {
            view! { <span class="tv-hint">{glyph(Btn::Lb, kind)}<i>"+"</i>{glyph(Btn::Rb, kind)}<b>{tr!("Seções")}</b></span> }
        });
        // On a section, what B does: back to Home, or (from Home) the leave question.
        let back = (tv.sheet.get().is_none() && !list.iter().any(|(b, _)| *b == Btn::B)).then(|| match tv.view.get() {
            View::Tab(Tab::Home) => Some(tr!("Sair")),
            View::Tab(_) => Some(tr!("Início")),
            _ => None,
        }).flatten().map(|label| view! { <span class="tv-hint">{glyph(Btn::B, kind)}<b>{label}</b></span> });
        let items = list.into_iter().map(|(btn, label)| view! { <span class="tv-hint">{glyph(btn, kind)}<b>{label}</b></span> }).collect_view();
        view! { {items}{back}{sections} }
    };

    let sheet = move || tv.sheet.get().map(|s| view! { <sheets::SheetView sheet=s /> });

    // The app's notices, sized for the TV (the window's own are under this screen).
    let toasts = move || {
        library
            .toasts
            .get()
            .into_iter()
            .map(|t| {
                let icon = if t.error { Icon::Info } else { Icon::Check };
                view! { <div class="tv-toast" class:bad=t.error role=if t.error { "alert" } else { "status" }><Svg icon size=26 /><span>{t.text}</span></div> }
            })
            .collect_view()
    };

    view! {
        <div class="tv">
            <div class="tv-toasts" aria-live="polite">{toasts}</div>
            <div class="tv-bg">{move || { let s = tv.backdrop.get(); (!s.is_empty()).then(|| view! { <Backdrop sources=s /> }) }}</div>
            <div class="tv-shade"></div>
            <header class="tv-top">
                <div class="tv-brand">
                    <img class="mark" src="brand/cerberus-pixel-icone.svg" alt="" draggable="false" />
                    <img class="word" src="brand/cerberus-launcher-pixel.svg#wordmark" alt=tr!("Cerberus Launcher") draggable="false" />
                </div>
                {tabs}
                <div class="tv-top-end">
                    {move || {
                        time.track();
                        let id = library.running.with(|r| r.iter().next().cloned())?;
                        let name = library.games.with(|g| g.as_ref().and_then(|r| r.as_ref().ok()).and_then(|all| all.iter().find(|x| x.id == id).map(|x| x.name.clone())))?;
                        let mins = library.started.with(|s| s.get(&id).map(|t| ((js_sys::Date::now() - t) / 60000.0) as u64)).unwrap_or(0);
                        Some(view! {
                            <span class="tv-playing"><i></i>{tr!("Jogando {}", name)}<small>{if mins == 0 { tr!("agora").to_string() } else { format!("{mins} min") }}</small></span>
                        })
                    }}
                    <div class="tv-clock">{move || time.get()}</div>
                </div>
            </header>
            <main class="tv-main">{page}</main>
            <footer class="tv-hints">{hints}</footer>
            {sheet}
        </div>
    }
}

/// Outside the TV screen: holding a controller's Menu button for about a
/// second, or pressing F11, opens it.
pub fn watch_for_tv(library: Library) {
    let since = StoredValue::new(None::<f64>);
    // After the TV (left by holding Menu), Menu must be let go before it counts again.
    let wait_release = StoredValue::new(false);
    let poll = set_interval_with_handle(
        move || {
            if library.route.get_untracked() == Route::BigPicture {
                since.set_value(None);
                wait_release.set_value(true);
                return;
            }
            // A game in front owns the pad.
            if !web_sys::window().and_then(|w| w.document()).is_some_and(|d| d.has_focus().unwrap_or(true)) {
                since.set_value(None);
                return;
            }
            let held = web_sys::window()
                .and_then(|w| w.navigator().get_gamepads().ok())
                .is_some_and(|pads| {
                    pads.iter().any(|p| {
                        use wasm_bindgen::JsCast;
                        p.dyn_into::<web_sys::Gamepad>().is_ok_and(|pad| {
                            pad.connected() && pad.buttons().get(9).dyn_into::<web_sys::GamepadButton>().is_ok_and(|b| b.pressed())
                        })
                    })
                });
            if wait_release.get_value() {
                if !held {
                    wait_release.set_value(false);
                }
                return;
            }
            let now = js_sys::Date::now();
            match (held, since.get_value()) {
                (true, None) => since.set_value(Some(now)),
                (true, Some(t)) if now - t >= 900.0 => {
                    since.set_value(None);
                    library.go(Route::BigPicture);
                }
                (false, _) => since.set_value(None),
                _ => {}
            }
        },
        Duration::from_millis(100),
    );
    let keys = window_event_listener(leptos::ev::keydown, move |e| {
        if e.key() == "F11" && !e.repeat() && library.route.get_untracked() != Route::BigPicture {
            e.prevent_default();
            library.go(Route::BigPicture);
        }
    });
    let keys = StoredValue::new_local(Some(keys));
    on_cleanup(move || {
        if let Ok(p) = poll {
            p.clear();
        }
        keys.update_value(|k| {
            if let Some(k) = k.take() {
                k.remove();
            }
        });
    });
}
