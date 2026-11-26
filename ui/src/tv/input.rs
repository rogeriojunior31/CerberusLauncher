//! Controller and keyboard for the TV screen.
//!
//! Controllers are read through the browser's Gamepad API at 30 Hz. Buttons are
//! named by the label printed on them: a Nintendo pad's A sits on the right, so
//! its face buttons are swapped back. A held direction repeats after 400 ms,
//! then every 120 ms. The left stick only counts once it has been seen resting,
//! so a drifting stick can't scroll on its own. Menu tapped opens Settings;
//! held for 0.9 s it leaves the TV screen.

use leptos::prelude::*;
use wasm_bindgen::JsCast;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Btn {
    Up,
    Down,
    Left,
    Right,
    A,
    B,
    X,
    Y,
    Lb,
    Rb,
    /// The triggers: in the Library, a jump to the next letter (or page).
    Lt,
    Rt,
    Menu,
    MenuHold,
    /// A letter or digit typed on a physical keyboard (for the search keyboard).
    Type(char),
    Erase,
}

/// Which controller the player uses, for its button pictures.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PadKind {
    #[default]
    Xbox,
    PlayStation,
    Nintendo,
}

/// The controller plugged in, for a game told about it: "playstation" or "xbox"
/// (any other kind plays with Xbox glyphs). `None` with none connected.
pub fn connected_pad() -> Option<&'static str> {
    let pads = web_sys::window()?.navigator().get_gamepads().ok()?;
    pads.iter().filter_map(|p| p.dyn_into::<web_sys::Gamepad>().ok()).find(|p| p.connected()).map(|p| match kind_of(&p.id()) {
        PadKind::PlayStation => "playstation",
        _ => "xbox",
    })
}

fn kind_of(id: &str) -> PadKind {
    let id = id.to_lowercase();
    if id.contains("054c") || id.contains("playstation") || id.contains("dualsense") || id.contains("dualshock") || id.contains("wireless controller") {
        PadKind::PlayStation
    } else if id.contains("057e") || id.contains("nintendo") || id.contains("pro controller") || id.contains("joy-con") {
        PadKind::Nintendo
    } else {
        PadKind::Xbox
    }
}

/// A controller as the player knows it: "Xbox 360 Controller", not the driver's long id.
pub fn pad_name(id: &str) -> String {
    let name = id.split(" (").next().unwrap_or(id).trim();
    if name.is_empty() { id.to_string() } else { name.to_string() }
}

/// The controllers Windows passes to Cerberus now: name, kind, and the button
/// held (by its printed label), for the check in Settings.
pub fn connected_pads() -> Vec<(String, PadKind, Option<&'static str>)> {
    let Some(pads) = web_sys::window().and_then(|w| w.navigator().get_gamepads().ok()) else { return Vec::new() };
    pads.iter()
        .filter_map(|p| p.dyn_into::<web_sys::Gamepad>().ok())
        .filter(|p| p.connected())
        .map(|pad| {
            let kind = kind_of(&pad.id());
            let buttons = pad.buttons();
            let down = |i: u32| buttons.get(i).dyn_into::<web_sys::GamepadButton>().is_ok_and(|b| b.pressed());
            let labels: [&'static str; 4] = match kind {
                PadKind::PlayStation => ["\u{2715}", "\u{25CB}", "\u{25A1}", "\u{25B3}"],
                PadKind::Nintendo => ["B", "A", "Y", "X"],
                PadKind::Xbox => ["A", "B", "X", "Y"],
            };
            let held = (0..4u32).find(|&i| down(i)).map(|i| labels[i as usize]).or_else(|| {
                [(4, "LB"), (5, "RB"), (8, "View"), (9, "Menu"), (12, "\u{2191}"), (13, "\u{2193}"), (14, "\u{2190}"), (15, "\u{2192}")]
                    .into_iter()
                    .find(|(i, _)| down(*i))
                    .map(|(_, l)| l)
            });
            (pad_name(&pad.id()), kind, held)
        })
        .collect()
}

const REPEAT_FIRST: f64 = 400.0;
const REPEAT_NEXT: f64 = 120.0;
const MENU_HOLD: f64 = 900.0;
const STICK_ON: f64 = 0.6;
const STICK_REST: f64 = 0.3;

#[derive(Default)]
struct PadState {
    /// The first snapshot was taken: buttons already held then don't count as presses.
    primed: bool,
    /// Menu was held when reading began (the hold that opened the TV): it counts once released.
    menu_blocked: bool,
    pressed: [bool; 8],
    dir: Option<Btn>,
    next_repeat: f64,
    menu_since: Option<f64>,
    menu_fired: bool,
    stick_seen_resting: bool,
}

type Snapshot = ([bool; 8], bool, Option<Btn>, (f64, f64), PadKind);

thread_local! {
    /// The controller used last (its index): a virtual one sitting idle in the
    /// first slot (Steam Input, DS4Windows) doesn't block the real one.
    static ACTIVE: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
}

/// Whether a snapshot has anything pressed or the stick pushed.
fn in_use(s: &Snapshot) -> bool {
    s.0.iter().any(|b| *b) || s.1 || s.2.is_some() || s.3.0.abs() >= STICK_ON || s.3.1.abs() >= STICK_ON
}

/// The controller to read: one in use now (the one used last if it still is),
/// else the one used last while connected, else the first connected.
fn choose(active: Option<u32>, pads: &[(u32, bool)]) -> Option<u32> {
    let busy: Vec<u32> = pads.iter().filter(|(_, b)| *b).map(|(i, _)| *i).collect();
    if let Some(a) = active.filter(|a| busy.contains(a)) {
        return Some(a);
    }
    busy.first().copied().or_else(|| active.filter(|a| pads.iter().any(|(i, _)| i == a))).or_else(|| pads.first().map(|(i, _)| *i))
}

/// One snapshot of the controller in use: face and shoulder buttons
/// (A, B, X, Y, LB, RB, LT, RT), Menu, the direction held, and its kind.
fn read_pad() -> Option<Snapshot> {
    let pads = web_sys::window()?.navigator().get_gamepads().ok()?;
    let mut all: Vec<(u32, Snapshot)> = Vec::new();
    for (index, pad) in pads.iter().enumerate() {
        let Ok(pad) = pad.dyn_into::<web_sys::Gamepad>() else { continue };
        if !pad.connected() {
            continue;
        }
        if let Some(s) = snapshot(&pad) {
            all.push((index as u32, s));
        }
    }
    let flags: Vec<(u32, bool)> = all.iter().map(|(i, s)| (*i, in_use(s))).collect();
    let picked = choose(ACTIVE.with(std::cell::Cell::get), &flags)?;
    ACTIVE.with(|a| a.set(Some(picked)));
    all.into_iter().find(|(i, _)| *i == picked).map(|(_, s)| s)
}

fn snapshot(pad: &web_sys::Gamepad) -> Option<Snapshot> {
    {
        let kind = kind_of(&pad.id());
        let buttons = pad.buttons();
        let down = |i: u32| buttons.get(i).dyn_into::<web_sys::GamepadButton>().is_ok_and(|b| b.pressed());
        // Standard layout: 0 bottom, 1 right, 2 left, 3 top. Nintendo prints A on the right, B at the bottom.
        let (a, b, x, y) = if kind == PadKind::Nintendo { (down(1), down(0), down(3), down(2)) } else { (down(0), down(1), down(2), down(3)) };
        let face = [a, b, x, y, down(4), down(5), down(6), down(7)];
        let dpad = if down(12) {
            Some(Btn::Up)
        } else if down(13) {
            Some(Btn::Down)
        } else if down(14) {
            Some(Btn::Left)
        } else if down(15) {
            Some(Btn::Right)
        } else {
            None
        };
        let axes = pad.axes();
        let stick = (axes.get(0).as_f64().unwrap_or(0.0), axes.get(1).as_f64().unwrap_or(0.0));
        Some((face, down(9), dpad, stick, kind))
    }
}

fn now() -> f64 {
    js_sys::Date::now()
}

/// Starts reading the controller and the keyboard; `on` gets every press.
/// Everything stops when the calling component goes away.
pub fn listen(on: impl Fn(Btn) + Clone + 'static, pad_kind: RwSignal<PadKind>) {
    let state = StoredValue::new_local(PadState::default());
    let on_pad = on.clone();
    let poll = set_interval_with_handle(
        move || {
            // Only while Cerberus is the window in front: with a game in front the
            // pad is the game's. Coming back, what is held is read afresh.
            let focused = web_sys::window().and_then(|w| w.document()).is_some_and(|d| d.has_focus().unwrap_or(true));
            if !focused {
                state.update_value(|s| s.primed = false);
                return;
            }
            let Some((face, menu, dpad, stick, kind)) = read_pad() else { return };
            if pad_kind.get_untracked() != kind {
                pad_kind.set(kind);
            }
            let t = now();
            let mut fire = Vec::new();
            state.update_value(|s| {
                if !s.primed {
                    s.primed = true;
                    s.pressed = face;
                    s.menu_blocked = menu;
                    s.menu_since = None;
                    s.stick_seen_resting = false;
                }
                if s.menu_blocked && !menu {
                    s.menu_blocked = false;
                }
                const FACE: [Btn; 8] = [Btn::A, Btn::B, Btn::X, Btn::Y, Btn::Lb, Btn::Rb, Btn::Lt, Btn::Rt];
                for (i, btn) in FACE.iter().enumerate() {
                    if face[i] && !s.pressed[i] {
                        fire.push(*btn);
                    }
                }
                s.pressed = face;

                // Menu: a tap opens Settings, a long hold leaves.
                match (menu && !s.menu_blocked, s.menu_since) {
                    (true, None) => {
                        s.menu_since = Some(t);
                        s.menu_fired = false;
                    }
                    (true, Some(since)) if !s.menu_fired && t - since >= MENU_HOLD => {
                        s.menu_fired = true;
                        fire.push(Btn::MenuHold);
                    }
                    (false, Some(_)) => {
                        if !s.menu_fired {
                            fire.push(Btn::Menu);
                        }
                        s.menu_since = None;
                    }
                    _ => {}
                }

                // The stick counts only after it has been seen at rest.
                let (sx, sy) = stick;
                if sx.abs() < STICK_REST && sy.abs() < STICK_REST {
                    s.stick_seen_resting = true;
                }
                let from_stick = if !s.stick_seen_resting {
                    None
                } else if sx.abs() > sy.abs() && sx.abs() > STICK_ON {
                    Some(if sx < 0.0 { Btn::Left } else { Btn::Right })
                } else if sy.abs() > STICK_ON {
                    Some(if sy < 0.0 { Btn::Up } else { Btn::Down })
                } else {
                    None
                };
                let dir = dpad.or(from_stick);
                match dir {
                    Some(d) if s.dir != Some(d) => {
                        fire.push(d);
                        s.next_repeat = t + REPEAT_FIRST;
                    }
                    Some(d) if t >= s.next_repeat => {
                        fire.push(d);
                        s.next_repeat = t + REPEAT_NEXT;
                    }
                    _ => {}
                }
                s.dir = dir;
            });
            for btn in fire {
                on_pad(btn);
            }
        },
        std::time::Duration::from_millis(33),
    );

    let keys = window_event_listener(leptos::ev::keydown, move |e| {
        let key = e.key();
        let btn = match key.as_str() {
            "ArrowUp" => Btn::Up,
            "ArrowDown" => Btn::Down,
            "ArrowLeft" => Btn::Left,
            "ArrowRight" => Btn::Right,
            "Enter" => Btn::A,
            // Space types a space on the search keyboard; elsewhere it is A.
            " " => Btn::Type(' '),
            "Escape" => Btn::B,
            "Backspace" => Btn::Erase,
            // Q/E and X/Y arrive as letters: the TV screen reads them as LB/RB and X/Y
            // except while the search keyboard is open, where they type.
            "PageUp" => Btn::Lb,
            "PageDown" => Btn::Rb,
            "Home" => Btn::Lt,
            "End" => Btn::Rt,
            "F10" => Btn::Menu,
            // Held, F11 repeats: only the first press counts.
            "F11" if e.repeat() => return,
            "F11" => Btn::MenuHold,
            k if k.chars().count() == 1 => {
                let c = k.chars().next().unwrap_or(' ');
                if c.is_ascii_alphanumeric() { Btn::Type(c) } else { return }
            }
            _ => return,
        };
        e.prevent_default();
        on(btn);
    });
    let keys = StoredValue::new_local(Some(keys));

    on_cleanup(move || {
        if let Ok(handle) = poll {
            handle.clear();
        }
        keys.update_value(|k| {
            if let Some(k) = k.take() {
                k.remove();
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_controller_in_use_is_read_not_an_idle_one_first() {
        // A virtual pad idle in slot 0, the real one in 1 pressing a button.
        assert_eq!(choose(None, &[(0, false), (1, true)]), Some(1));
        // The real one stays chosen while idle; the idle virtual one doesn't take over.
        assert_eq!(choose(Some(1), &[(0, false), (1, false)]), Some(1));
        // Unplugged: back to what's there.
        assert_eq!(choose(Some(1), &[(0, false)]), Some(0));
        // Two in use: the one used last keeps it.
        assert_eq!(choose(Some(1), &[(0, true), (1, true)]), Some(1));
        assert_eq!(choose(None, &[]), None);
    }

    #[test]
    fn pad_names_drop_the_driver_details() {
        assert_eq!(pad_name("Xbox 360 Controller (XInput STANDARD GAMEPAD)"), "Xbox 360 Controller");
        assert_eq!(pad_name("Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 09cc)"), "Wireless Controller");
    }

    #[test]
    fn pads_are_told_apart_by_their_names() {
        assert_eq!(kind_of("Xbox 360 Controller (XInput STANDARD GAMEPAD)"), PadKind::Xbox);
        assert_eq!(kind_of("DualSense Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 0ce6)"), PadKind::PlayStation);
        assert_eq!(kind_of("Pro Controller (STANDARD GAMEPAD Vendor: 057e Product: 2009)"), PadKind::Nintendo);
    }
}
