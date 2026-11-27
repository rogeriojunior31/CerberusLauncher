//! The achievement notice over the game: its own small see-through window.
//! It shows each queued notice for a few seconds, then asks for the next one;
//! when there's none, the backend hides the window.

use cerberus_shared::{UnlockKind, UnlockNotice};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::icons::{Icon, Svg};

/// How long each notice stays, in milliseconds (the slide in and out included).
const SHOWN_MS: i32 = 6000;

/// Whether this window is the achievement notice, not the launcher.
pub fn is_unlock() -> bool {
    js_sys::Reflect::get(&window(), &"__CERBERUS_UNLOCK".into()).is_ok_and(|v| v.is_truthy())
}

fn after(ms: i32, f: impl FnOnce() + 'static) {
    let f = wasm_bindgen::closure::Closure::once_into_js(f);
    let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(f.unchecked_ref(), ms);
}

use wasm_bindgen::JsCast;

/// A short two-note chime, made on the spot (no sound file), quiet.
fn chime(kind: UnlockKind) {
    let Ok(ctx) = web_sys::AudioContext::new() else { return };
    let notes: &[f32] = match kind {
        UnlockKind::Normal => &[880.0, 1318.5],
        UnlockKind::Rare => &[987.8, 1479.98],
        UnlockKind::Complete => &[783.99, 1046.5, 1567.98],
    };
    let now = ctx.current_time();
    for (i, f) in notes.iter().enumerate() {
        let (Ok(osc), Ok(gain)) = (ctx.create_oscillator(), ctx.create_gain()) else { return };
        osc.set_type(web_sys::OscillatorType::Sine);
        osc.frequency().set_value(*f);
        let start = now + i as f64 * 0.11;
        let g = gain.gain();
        let _ = g.set_value_at_time(0.0, start);
        let _ = g.linear_ramp_to_value_at_time(0.12, start + 0.02);
        let _ = g.exponential_ramp_to_value_at_time(0.0001, start + 0.5);
        let _ = osc.connect_with_audio_node(&gain);
        let _ = gain.connect_with_audio_node(&ctx.destination());
        let _ = osc.start_with_when(start);
        let _ = osc.stop_with_when(start + 0.55);
    }
}

#[component]
pub fn UnlockToast() -> impl IntoView {
    let current = RwSignal::new(None::<UnlockNotice>);
    let leaving = RwSignal::new(false);
    let busy = StoredValue::new(false);
    // A notice queued while the last answer ("nothing left") was on its way.
    let missed = StoredValue::new(false);

    fn next(current: RwSignal<Option<UnlockNotice>>, leaving: RwSignal<bool>, busy: StoredValue<bool>, missed: StoredValue<bool>) {
        busy.set_value(true);
        spawn_local(async move {
            match api::unlock_next().await.ok().flatten() {
                Some(n) => {
                    chime(n.kind);
                    leaving.set(false);
                    current.set(Some(n));
                    after(SHOWN_MS - 450, move || leaving.set(true));
                    after(SHOWN_MS, move || next(current, leaving, busy, missed));
                }
                None => {
                    current.set(None);
                    busy.set_value(false);
                    // One arrived meanwhile: asked for again, not left until the next unlock.
                    if missed.get_value() {
                        missed.set_value(false);
                        next(current, leaving, busy, missed);
                    }
                }
            }
        });
    }

    api::on_unlock_queued(move || {
        if busy.get_value() {
            missed.set_value(true);
        } else {
            next(current, leaving, busy, missed);
        }
    });
    next(current, leaving, busy, missed);

    move || {
        current.get().map(|n| {
            let kind = match n.kind {
                UnlockKind::Normal => "",
                UnlockKind::Rare => "rare",
                UnlockKind::Complete => "complete",
            };
            let icon = match n.icon.clone() {
                Some(src) => view! { <img src=src alt="" /> }.into_any(),
                None => view! { <Svg icon=Icon::Trophy size=34 /> }.into_any(),
            };
            view! {
                <div class=format!("unlock {kind}") class:out=move || leaving.get() role="status">
                    <span class="unlock-icon">{icon}</span>
                    <span class="unlock-t">
                        <small>{n.title.clone()}</small>
                        <b>{n.name.clone()}</b>
                        {(!n.description.is_empty()).then(|| view! { <span>{n.description.clone()}</span> })}
                    </span>
                </div>
            }
        })
    }
}
