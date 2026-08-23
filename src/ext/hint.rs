//! EXT: the one HUD hint line. Not part of the C++ port.
//!
//! Anything in the world that offers the player an action -- the elevator's "E  RIDE TO ...",
//! a locked window's "LOCKED", a key's "E  USE KEY" -- says so here, and the engine's overlay
//! block draws whatever was said through `hud::draw_hint`. One line, one channel: the HUD has
//! a single slot for a prompt (`hud::HINT_Y`), so two writers could only ever fight over it.
//!
//! # One writer per frame
//!
//! The channel is a single slot, written with [`set`] and emptied by [`take`] once per rendered
//! frame. The last writer before the take wins and everything before it is lost, so the rule
//! for writers is: set the hint every frame it applies, from exactly one place per situation.
//! Object updates run at the 500 Hz step, so a writer there sets it several times a frame --
//! harmless, the slot just holds the latest string -- and a writer that stops writing makes
//! the line disappear on the next frame without having to clear anything. Nothing is cleared
//! on a scene load either, for the same reason: the next take empties it.
//!
//! The first caller is the elevator (`ext::elevator`), which sets it each step the player
//! stands in an idle cabin; the grab's `ObjectT::pick_hint` sets it once per frame for the
//! object under the crosshair. Those two never apply at once (the elevator takes the E press
//! before the grab looks), so the ordering between them has not had to be chosen.

use std::cell::RefCell;

thread_local! {
    /// The hint set since the last take, if any.
    static HINT: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Offer a hint line for this frame. A later call before the frame's [`take`] replaces it.
pub fn set(text: impl Into<String>) {
    HINT.with(|h| *h.borrow_mut() = Some(text.into()));
}

/// The hint for this frame, consumed. Called by the engine's overlay block once per rendered
/// frame; a second call in the same frame gets `None`.
pub fn take() -> Option<String> {
    HINT.with(|h| h.borrow_mut().take())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_writer_wins_and_a_take_empties_the_slot() {
        assert_eq!(take(), None);
        set("LOCKED");
        set(String::from("E  USE KEY"));
        assert_eq!(take().as_deref(), Some("E  USE KEY"));
        assert_eq!(take(), None, "consumed");
    }
}
