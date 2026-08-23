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
//! Drawn last, in the main pass only -- inside `Engine::render` it would be painted into every
//! portal's framebuffer as well.

use crate::ext::ui::{Ui, WHITE};
use crate::ext::ui_atlas::{CURSOR_CLOSED, CURSOR_DOT, CURSOR_OPEN};

/// Dot diameter in pixels (the sprite is a 128 px disc, scaled down).
const DOT_PX: f32 = 9.0;
/// Hand height in pixels.
const HAND_PX: f32 = 44.0;

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
