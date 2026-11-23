//! The monitor Cerberus opens on (and Big Picture Mode fills), when the player
//! picked one. Monitors are known by Windows' name for them (`\\.\DISPLAY2`);
//! one that isn't plugged in now is ignored and the window stays where it is.

use cerberus_shared::MonitorView;
use tauri::{Monitor, PhysicalPosition, Runtime, WebviewWindow};

fn name_of(monitor: &Monitor) -> String {
    monitor.name().cloned().unwrap_or_default()
}

/// The monitors plugged in, numbered as the player sees them.
pub fn list<R: Runtime>(window: &WebviewWindow<R>) -> Vec<MonitorView> {
    let primary = window.primary_monitor().ok().flatten().map(|m| name_of(&m));
    let all = window.available_monitors().unwrap_or_default();
    all.iter()
        .enumerate()
        .map(|(i, m)| {
            let name = name_of(m);
            let is_primary = primary.as_deref() == Some(name.as_str());
            let size = m.size();
            let mut label = format!("Monitor {} · {}×{}", i + 1, size.width, size.height);
            if is_primary {
                label.push_str(" (principal)");
            }
            MonitorView { name, label, primary: is_primary }
        })
        .collect()
}

/// Where a window of `size` sits centered on a monitor at `origin` of `area`;
/// a window larger than the monitor starts at its corner.
fn centered(origin: (i32, i32), area: (u32, u32), size: (u32, u32)) -> (i32, i32) {
    let gap = |a: u32, s: u32| (a.saturating_sub(s) / 2) as i32;
    (origin.0 + gap(area.0, size.0), origin.1 + gap(area.1, size.1))
}

/// Moves the window to the chosen monitor, keeping it maximized or full
/// screen if it was. Nothing happens when it's there already.
pub fn place<R: Runtime>(window: &WebviewWindow<R>, chosen: Option<&str>) {
    let Some(chosen) = chosen.filter(|c| !c.is_empty()) else { return };
    let all = window.available_monitors().unwrap_or_default();
    let Some(target) = all.iter().find(|m| name_of(m) == chosen) else { return };
    if window.current_monitor().ok().flatten().is_some_and(|m| name_of(&m) == chosen) {
        return;
    }
    let full = window.is_fullscreen().unwrap_or(false);
    let max = window.is_maximized().unwrap_or(false);
    if full {
        let _ = window.set_fullscreen(false);
    }
    if max {
        let _ = window.unmaximize();
    }
    let size = window.outer_size().map(|s| (s.width, s.height)).unwrap_or((1280, 800));
    let origin = target.position();
    let area = target.size();
    let (x, y) = centered((origin.x, origin.y), (area.width, area.height), size);
    let _ = window.set_position(PhysicalPosition::new(x, y));
    if max {
        let _ = window.maximize();
    }
    if full {
        let _ = window.set_fullscreen(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centers_on_the_monitor() {
        assert_eq!(centered((1920, 0), (2560, 1440), (1280, 800)), (1920 + 640, 320));
        assert_eq!(centered((-1920, 0), (1920, 1080), (2000, 1200)), (-1920, 0), "too big: the corner");
    }
}
