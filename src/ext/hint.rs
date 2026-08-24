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
//!
//! One pair does apply at once: the held key aimed at the locked window. The window's
//! `pick_hint` says "LOCKED", the key's step says "E  USE THE KEY", and the key's must show
//! -- but the grab sets the window's line once per rendered frame, AFTER the fixed steps in
//! which the key set its. So a second slot, [`insist`], outranks [`set`] whatever the order:
//! what the thing in your hand can do beats what the thing under the crosshair is.
//!
//! A third slot sits between them. [`notice`] is for a line about what the player just *tried*
//! -- the inventory's "POCKETS FULL" and its two siblings, which stay up for a second and a half
//! because a key press is over in a frame. It beats a description of whatever the crosshair is
//! on, and loses to the prompt for the thing in the hand: an action still available outranks an
//! explanation of one that was not, and a refusal that blanked "E  USE THE KEY" for 1.6 s would
//! read as the key having stopped working.

use std::cell::RefCell;

thread_local! {
    /// The hint set since the last take, if any.
    static HINT: RefCell<Option<String>> = const { RefCell::new(None) };
    /// The insisted hint since the last take, if any; shown in preference to `NOTICE` and `HINT`.
    static INSISTED: RefCell<Option<String>> = const { RefCell::new(None) };
    /// The noticed hint since the last take, if any; between the two.
    static NOTICE: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Offer a hint line for this frame. A later call before the frame's [`take`] replaces it.
pub fn set(text: impl Into<String>) {
    HINT.with(|h| *h.borrow_mut() = Some(text.into()));
}

/// Offer a hint line that outranks any [`set`] this frame, whichever came first: the prompt
/// for the object in hand (see the module docs). A later `insist` replaces an earlier one.
pub fn insist(text: impl Into<String>) {
    INSISTED.with(|h| *h.borrow_mut() = Some(text.into()));
}

/// Offer a hint line that outranks a plain [`set`] but yields to an [`insist`]: a line about what
/// the player just tried and could not do (see the module docs). A later `notice` replaces an
/// earlier one.
pub fn notice(text: impl Into<String>) {
    NOTICE.with(|h| *h.borrow_mut() = Some(text.into()));
}

/// The hint for this frame, consumed -- the insisted one if there is one, then a notice, then the
/// set one; all three slots are emptied. Called by the engine's overlay block once per rendered
/// frame; a second call in the same frame gets `None`.
pub fn take() -> Option<String> {
    let plain = HINT.with(|h| h.borrow_mut().take());
    let notice = NOTICE.with(|h| h.borrow_mut().take());
    INSISTED.with(|h| h.borrow_mut().take()).or(notice).or(plain)
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

    #[test]
    fn an_insisted_hint_outranks_a_set_one_in_either_order_and_both_are_consumed() {
        insist("E  USE THE KEY");
        set("LOCKED");
        assert_eq!(take().as_deref(), Some("E  USE THE KEY"));
        assert_eq!(take(), None, "the outranked line must not show on the next frame");
        set("LOCKED");
        insist("E  USE THE KEY");
        assert_eq!(take().as_deref(), Some("E  USE THE KEY"));
        assert_eq!(take(), None);
        // With nothing insisted, a set line shows as before.
        set("LOCKED");
        assert_eq!(take().as_deref(), Some("LOCKED"));
    }

    /// A notice sits between the two, in either order, and every slot is emptied by one take.
    #[test]
    fn a_notice_beats_a_set_line_and_loses_to_an_insisted_one() {
        set("LOCKED");
        notice("POCKETS FULL");
        assert_eq!(take().as_deref(), Some("POCKETS FULL"));
        assert_eq!(take(), None);

        notice("POCKETS FULL");
        insist("E  USE THE KEY");
        assert_eq!(take().as_deref(), Some("E  USE THE KEY"), "the key must not go dark");
        assert_eq!(take(), None, "and the outranked notice does not show a frame later");

        insist("E  USE THE KEY");
        notice("POCKETS FULL");
        set("LOCKED");
        assert_eq!(take().as_deref(), Some("E  USE THE KEY"), "whichever order they came in");
        assert_eq!(take(), None);
    }
}
