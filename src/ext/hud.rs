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
//! Drawn last, in the main pass only -- inside `Engine::render` it would be painted into every
//! portal's framebuffer as well.

use crate::ext::ui::{Align, Ui, WHITE};
use crate::ext::ui_atlas::{CURSOR_CLOSED, CURSOR_DOT, CURSOR_OPEN};

/// Dot diameter in pixels (the sprite is a 128 px disc, scaled down).
const DOT_PX: f32 = 9.0;
/// Hand height in pixels.
const HAND_PX: f32 = 44.0;
/// The hint line: its top as a fraction of the height, and its size -- the pause menu's own
/// hint size, so the two read as one voice.
const HINT_Y: f32 = 0.86;
const HINT_SIZE: f32 = 0.022;

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
