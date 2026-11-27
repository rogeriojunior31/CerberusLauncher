//! Navigation sounds for the TV screen, made on the spot with Web Audio (no
//! sound files): a soft tick when the focus moves, a rising chime when
//! something is chosen, a falling one going back, a swish between tabs.

use std::cell::RefCell;

thread_local! {
    static CONTEXT: RefCell<Option<web_sys::AudioContext>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy)]
pub enum Sound {
    Move,
    Select,
    Back,
    Tab,
}

const KEY: &str = "cerberus.tv.sounds";

pub fn enabled() -> bool {
    let stored = web_sys::window().and_then(|w| w.local_storage().ok().flatten()).and_then(|s| s.get_item(KEY).ok().flatten());
    stored.as_deref() != Some("off")
}

pub fn set_enabled(on: bool) {
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(KEY, if on { "on" } else { "off" });
    }
}

/// One short tone: start and end pitch, length in seconds, loudness.
fn tone(ctx: &web_sys::AudioContext, from: f32, to: f32, start: f64, length: f64, volume: f32) -> Option<()> {
    let osc = ctx.create_oscillator().ok()?;
    let gain = ctx.create_gain().ok()?;
    osc.set_type(web_sys::OscillatorType::Sine);
    let t = ctx.current_time() + start;
    osc.frequency().set_value_at_time(from, t).ok()?;
    osc.frequency().exponential_ramp_to_value_at_time(to, t + length).ok()?;
    gain.gain().set_value_at_time(0.0001, t).ok()?;
    gain.gain().exponential_ramp_to_value_at_time(volume, t + 0.008).ok()?;
    gain.gain().exponential_ramp_to_value_at_time(0.0001, t + length).ok()?;
    osc.connect_with_audio_node(&gain).ok()?;
    gain.connect_with_audio_node(&ctx.destination()).ok()?;
    osc.start_with_when(t).ok()?;
    osc.stop_with_when(t + length + 0.02).ok()?;
    Some(())
}

pub fn play(sound: Sound) {
    if !enabled() {
        return;
    }
    CONTEXT.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = web_sys::AudioContext::new().ok();
        }
        let Some(ctx) = slot.as_ref() else { return };
        // Made by a controller press (not a gesture the page counts), it can start
        // suspended; resuming one that already runs does nothing.
        let _ = ctx.resume();
        let _ = match sound {
            Sound::Move => tone(ctx, 1320.0, 1180.0, 0.0, 0.045, 0.05),
            Sound::Select => tone(ctx, 660.0, 660.0, 0.0, 0.07, 0.08).and_then(|_| tone(ctx, 990.0, 990.0, 0.06, 0.12, 0.08)),
            Sound::Back => tone(ctx, 700.0, 420.0, 0.0, 0.11, 0.07),
            Sound::Tab => tone(ctx, 500.0, 900.0, 0.0, 0.09, 0.05),
        };
    });
}
