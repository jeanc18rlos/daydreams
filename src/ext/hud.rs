//! EXT: the cursor. Not part of the C++ port.
//!
//! Superliminal's reticle language, on the sprite sheet the project ships in
//! `Textures/ui_cursors.bmp`:
//!
//! | State                    | Cursor                    |
//! |--------------------------|---------------------------|
//! | nothing under the reticle| a small dot               |
//! | grabbable under it       | an open hand              |
//! | holding something        | a closed hand             |
//!
//! Plus one line of text, when something in the world offers an action -- the elevator's
//! "E  RIDE TO ..." -- low on the screen, in the menus' hint style, so the middle stays clear.
//!
//! And the inventory's row of slots along the bottom (`ext/inventory.rs`), below the hint line.
//! It is drawn on every gameplay frame rather than faded in on a change and out a few seconds
//! later: `F` and `G` act on the *selected* slot, so the row has to be readable at the moment
//! the player decides to press one, which is exactly the moment a fade would have hidden it.
//! It is small, dim and out at the edge, and it costs one draw per slot.
//!
//! Drawn last, in the main pass only -- inside `Engine::render` it would be painted into every
//! portal's framebuffer as well.

use crate::ext::inventory::CAPACITY;
use crate::ext::ui::{Align, Color, Ui, GOLD, WHITE};
use crate::ext::ui_atlas::{CURSOR_CLOSED, CURSOR_DOT, CURSOR_OPEN};

/// Dot diameter in pixels (the sprite is a 128 px disc, scaled down).
const DOT_PX: f32 = 9.0;
/// Hand height in pixels.
const HAND_PX: f32 = 44.0;
/// The hint line: its top as a fraction of the height, and its size -- the pause menu's own
/// hint size, so the two read as one voice.
const HINT_Y: f32 = 0.86;
const HINT_SIZE: f32 = 0.022;

/// The inventory row, all as fractions of the drawable's height so the row keeps its
/// proportions on any window: one slot's size, the gap between two, where the row's bottom
/// edge sits, how much of a corner is taken off, the label's size and the border's thickness.
/// The row's top lands at 0.913 of the height, clear of the hint line's box (0.86 + 0.022).
const SLOT_W: f32 = 0.115;
const SLOT_H: f32 = 0.046;
const SLOT_GAP: f32 = 0.010;
const SLOT_BOTTOM: f32 = 0.959;
const SLOT_CORNER: f32 = 0.010;
const SLOT_LABEL: f32 = 0.017;
const SLOT_BORDER: f32 = 0.0030;
/// Horizontal strips per corner in the rounded outline (`corner_bands`). Three is where the
/// staircase stops being one at a slot's size, and it is 7 rectangles a tile.
const CORNER_BANDS: u32 = 3;
/// Slot fills: empty, filled, and the selected one.
///
/// Dark and translucent rather than white and translucent, because the row hangs over whatever
/// the level's floor happens to be -- the Backrooms' carpet is very nearly the gold the border
/// is drawn in, and a pale tile disappeared into it. The selected one is a lighter grey, more
/// opaque: brighter than its neighbours whatever is behind them.
const FILL_EMPTY: Color = [0.0, 0.0, 0.0, 0.30];
const FILL_FULL: Color = [0.0, 0.0, 0.0, 0.52];
const FILL_PICKED: Color = [0.34, 0.34, 0.34, 0.66];
/// Label colours: an item's name, and the slot number an empty slot shows instead.
const LABEL_FULL: Color = [0.97, 0.97, 0.97, 1.0];
const LABEL_EMPTY: Color = [1.0, 1.0, 1.0, 0.32];

#[derive(Clone, Copy, PartialEq)]
pub enum Cursor {
    Dot,
    Open,
    Closed,
}

pub fn draw(ui: &Ui, cursor: Cursor) {
    let (w, h) = ui.size();
    let (cx, cy) = (w * 0.5, h * 0.5);
    match cursor {
        Cursor::Dot => ui.draw_cursor(&CURSOR_DOT, cx, cy, DOT_PX, WHITE),
        Cursor::Open => ui.draw_cursor(&CURSOR_OPEN, cx, cy, HAND_PX, WHITE),
        Cursor::Closed => ui.draw_cursor(&CURSOR_CLOSED, cx, cy, HAND_PX, WHITE),
    }
}

/// One line of prompt, centred low on the screen.
pub fn draw_hint(ui: &Ui, text: &str) {
    let (w, h) = ui.size();
    ui.draw_text(text, w * 0.5, h * HINT_Y, h * HINT_SIZE, WHITE, Align::Center);
}

/// One quarter of a rounded corner of radius `r`, as horizontal strips: for each, how far its
/// top is from the rectangle's flat edge, how tall it is, and how far its ends are inset.
///
/// The 2D layer has exactly one primitive, a filled rectangle (`Ui::fill_rect`), so a rounded
/// corner is a staircase. Each strip is inset by the circle's own chord at the strip's middle,
/// `r - sqrt(r^2 - (r - d)^2)`, which is what makes it read as a curve rather than a chamfer.
fn corner_bands(r: f32) -> impl Iterator<Item = (f32, f32, f32)> {
    let step = r / CORNER_BANDS as f32;
    (0..CORNER_BANDS).map(move |i| {
        let top = i as f32 * step;
        let k = r - (top + 0.5 * step);
        (top, step, r - (r * r - k * k).sqrt())
    })
}

/// A rectangle with rounded corners.
fn fill_round_rect(ui: &Ui, x: f32, y: f32, w: f32, h: f32, r: f32, color: Color) {
    let r = r.min(0.5 * w).min(0.5 * h);
    ui.fill_rect(x, y + r, w, h - 2.0 * r, color);
    for (top, bh, ins) in corner_bands(r) {
        ui.fill_rect(x + ins, y + top, w - 2.0 * ins, bh, color);
        ui.fill_rect(x + ins, y + h - top - bh, w - 2.0 * ins, bh, color);
    }
}

/// The same outline, `t` thick, drawn just outside it.
///
/// Each corner strip's side piece is widened by the step down from the strip above it, so the
/// staircase has no gaps in it; the flat caps span the first strip's width, not the full one.
fn stroke_round_rect(ui: &Ui, x: f32, y: f32, w: f32, h: f32, r: f32, t: f32, color: Color) {
    let r = r.min(0.5 * w).min(0.5 * h);
    ui.fill_rect(x - t, y + r, t, h - 2.0 * r, color);
    ui.fill_rect(x + w, y + r, t, h - 2.0 * r, color);
    let mut prev: Option<f32> = None;
    for (top, bh, ins) in corner_bands(r) {
        if prev.is_none() {
            ui.fill_rect(x + ins, y - t, w - 2.0 * ins, t, color);
            ui.fill_rect(x + ins, y + h, w - 2.0 * ins, t, color);
        }
        let bar = t + prev.map_or(0.0, |p| (p - ins).max(0.0));
        for y0 in [y + top, y + h - top - bh] {
            ui.fill_rect(x + ins - t, y0, bar, bh, color);
            ui.fill_rect(x + w - ins - bar + t, y0, bar, bh, color);
        }
        prev = Some(ins);
    }
}

/// The inventory row: one tile per slot along the bottom centre, the selected one brighter and
/// inside a gold border, each item's name inside its tile and an empty slot showing only its
/// number (module docs).
///
/// `labels` is the inventory's own `labels()`, in slot order; anything shorter or longer than
/// [`CAPACITY`] draws what it has, so the caller and this cannot disagree about the count.
pub fn draw_inventory(ui: &Ui, labels: &[Option<&'static str>], selected: usize) {
    let (w, h) = ui.size();
    let (sw, sh, gap) = (h * SLOT_W, h * SLOT_H, h * SLOT_GAP);
    let n = labels.len().min(CAPACITY);
    if n == 0 {
        return;
    }
    let total = n as f32 * sw + (n - 1) as f32 * gap;
    let top = h * SLOT_BOTTOM - sh;
    // The label is centred on the cap height rather than the line box: `draw_text` puts the
    // baseline at FONT_ASCENT of the size below `y`, and capitals stand about 0.71 of the size
    // above the baseline, so their middle is 0.575 of the size below the top of the box.
    let cap_middle = 0.575;
    let pad = sw * 0.12;

    for (i, label) in labels.iter().take(n).enumerate() {
        let x = w * 0.5 - total * 0.5 + i as f32 * (sw + gap);
        let picked = i == selected;
        let fill = match (label, picked) {
            (_, true) => FILL_PICKED,
            (Some(_), false) => FILL_FULL,
            (None, false) => FILL_EMPTY,
        };
        let r = h * SLOT_CORNER;
        fill_round_rect(ui, x, top, sw, sh, r, fill);
        if picked {
            stroke_round_rect(ui, x, top, sw, sh, r, h * SLOT_BORDER, GOLD);
        }
        // The props name themselves in their own voice ("apple", "chess king"); the HUD speaks
        // in capitals, as every other line of it does.
        let (text, color) = match label {
            Some(l) => (l.to_ascii_uppercase(), LABEL_FULL),
            None => ((i + 1).to_string(), LABEL_EMPTY),
        };
        let mut size = h * SLOT_LABEL;
        let room = sw - 2.0 * pad;
        let width = ui.measure(&text, size);
        if width > room {
            size *= room / width;
        }
        ui.draw_text(
            &text,
            x + sw * 0.5,
            top + sh * 0.5 - size * cap_middle,
            size,
            color,
            Align::Center,
        );
    }
}
