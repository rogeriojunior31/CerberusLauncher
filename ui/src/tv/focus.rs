//! Where the focus is on a TV page: a row and a column. Pages are rows of
//! items (a shelf of covers, a row of buttons, one line of a list). Up and down
//! go to the next row that has items, landing on the item that row had last
//! (shelves) or on the same column (grids). Left and right stay in the row and
//! stop at its ends. The focused item is scrolled smoothly into view.

use std::collections::HashMap;

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use super::input::Btn;

#[derive(Clone, Copy)]
pub struct Focus {
    pub row: RwSignal<usize>,
    pub col: RwSignal<usize>,
    memory: StoredValue<HashMap<usize, usize>>,
    /// Grids keep the column when moving between rows; shelves remember each row's own.
    grid: bool,
    prefix: &'static str,
}

impl Focus {
    pub fn new(prefix: &'static str, grid: bool) -> Self {
        let focus = Self { row: RwSignal::new(0), col: RwSignal::new(0), memory: StoredValue::new(HashMap::new()), grid, prefix };
        // Scroll the focused item into view after it is drawn.
        Effect::new(move |_| {
            let key = focus.key(focus.row.get(), focus.col.get());
            request_animation_frame(move || scroll_to(&key));
        });
        focus
    }

    /// The `data-tvf` value of an item.
    pub fn key(&self, row: usize, col: usize) -> String {
        format!("{}-{row}-{col}", self.prefix)
    }

    pub fn is(&self, row: usize, col: usize) -> bool {
        self.row.get() == row && self.col.get() == col
    }

    pub fn set(&self, row: usize, col: usize) {
        self.row.set(row);
        self.col.set(col);
    }

    /// Moves for a direction; `rows` holds each row's length. False when it couldn't move.
    pub fn step(&self, btn: Btn, rows: &[usize]) -> bool {
        let (r, c) = (self.row.get_untracked(), self.col.get_untracked());
        let len = |i: usize| rows.get(i).copied().unwrap_or(0);
        match btn {
            Btn::Left if c > 0 => self.col.set(c - 1),
            Btn::Right if c + 1 < len(r) => self.col.set(c + 1),
            Btn::Up | Btn::Down => {
                let target = if btn == Btn::Up { (0..r).rev().find(|&i| len(i) > 0) } else { (r + 1..rows.len()).find(|&i| len(i) > 0) };
                let Some(to) = target else { return false };
                self.memory.update_value(|m| {
                    m.insert(r, c);
                });
                let col = if self.grid { c } else { self.memory.with_value(|m| m.get(&to).copied()).unwrap_or(0) };
                self.row.set(to);
                self.col.set(col.min(len(to).saturating_sub(1)));
            }
            _ => return false,
        }
        true
    }

    /// Keeps the focus on an item that exists after the page's items changed.
    pub fn clamp(&self, rows: &[usize]) {
        if rows.iter().all(|&n| n == 0) {
            return;
        }
        let r = self.row.get_untracked();
        let row = if rows.get(r).copied().unwrap_or(0) > 0 { r } else { rows.iter().position(|&n| n > 0).unwrap_or(0) };
        let col = self.col.get_untracked().min(rows[row].saturating_sub(1));
        if row != r {
            self.row.set(row);
        }
        if col != self.col.get_untracked() {
            self.col.set(col);
        }
    }
}

fn scroll_to(key: &str) {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else { return };
    let Ok(Some(element)) = document.query_selector(&format!("[data-tvf='{key}']")) else { return };
    let options = web_sys::ScrollIntoViewOptions::new();
    options.set_behavior(web_sys::ScrollBehavior::Smooth);
    options.set_block(web_sys::ScrollLogicalPosition::Center);
    options.set_inline(web_sys::ScrollLogicalPosition::Center);
    if let Ok(element) = element.dyn_into::<web_sys::Element>() {
        element.scroll_into_view_with_scroll_into_view_options(&options);
    }
}
