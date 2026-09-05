//! EXT: the third-person boom camera. Not part of the C++ port.
//!
//! Hide 'N Dream is a game about *being seen* -- your own silhouette is the thing you are
//! trying to keep out of somebody's cone. That is unplayable from inside your own skull, so
//! the render camera steps back onto a boom and the player gets a body ([`ext/avatar.rs`]).
//!
//! # The one rule: the RENDER camera moves, the EYE does not
//!
//! Two transforms come out of `Player` and they must not be confused:
//!
//! * [`Player::cam_to_world`] is the **eye** -- where the player's head actually is. Every
//!   gameplay question keeps using it: the observation cone (`ext/visibility.rs`), the grab
//!   ray (`ext/grab.rs`), the flashlight's beam (`ext/tool.rs`), footstep audio. A boom that
//!   fed those would let you grab through walls from three metres behind yourself and would
//!   quietly widen the Chameleon Rule's cone by the boom length.
//! * [`Player::render_world_to_cam`] is what the **renderer** uses (`Engine::render`), and
//!   only that.
//!
//! Because the split is exactly the difference between "what the player knows" and "what the
//! screen shows", it is also the correct split for the multiplayer round: a hider is caught
//! by where their body is, never by where their camera happens to hang.
//!
//! # Portals
//!
//! The boom is a camera-space offset, so it rides `cam_to_world` and inherits the player's
//! `p_scale` and every portal warp the body has taken -- a shrunk player gets a
//! proportionally short boom, and crossing a doorway takes the camera with it in the same
//! step. What it deliberately does NOT do is cross a portal on its own: a boom that reached
//! through a portal plane would have to be re-warped independently, and the ported render
//! path has one camera per pass. The pull-in below keeps it out of geometry, and portal
//! quads are geometry, so the camera stops short of a doorway rather than poking through it.

use crate::vector::Vector3;
use std::cell::Cell;

thread_local! {
    /// Whether the render camera is on the boom. Ambient rather than a `Player` field
    /// because it must survive the scene loads that rebuild the player (`Player::reset`),
    /// exactly like `ext::view`'s mood: a round that reloads must not silently drop the
    /// player back into their own head.
    static ENABLED: Cell<bool> = const { Cell::new(false) };
}

/// Is the render camera currently on the boom?
pub fn enabled() -> bool {
    ENABLED.with(|c| c.get())
}

/// Put the render camera on the boom, or back in the eye.
pub fn set_enabled(on: bool) {
    ENABLED.with(|c| c.set(on));
}

/// Swap first and third person; returns the new state.
pub fn toggle() -> bool {
    ENABLED.with(|c| {
        let next = !c.get();
        c.set(next);
        next
    })
}

/// The boom, in CAMERA space at `p_scale == 1`: `x` right of the head, `y` above it, `z`
/// behind it (the camera looks down its own -Z, so +Z is backwards).
///
/// Over the right shoulder rather than dead centre, so the body does not sit on the
/// crosshair: the aim ray still leaves the eye, and a centred boom would put the avatar's
/// head exactly where the player is trying to look.
pub const BOOM: Vector3 = Vector3 { x: 0.42, y: 0.30, z: 2.75 };

/// How far off an obstruction the pulled-in camera stops, in metres at `p_scale == 1`.
/// Smaller than the player's own radius so the camera can tuck into a corner the body
/// cannot enter.
pub const CLEARANCE: f32 = 0.18;

/// The shortest the boom is allowed to get before it may as well be first person, as a
/// fraction of [`BOOM`]. Below this the pull-in would put the camera inside the avatar's
/// head; the avatar hides itself there instead (`ext/avatar.rs`).
pub const MIN_FRACTION: f32 = 0.22;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_round_trips() {
        set_enabled(false);
        assert!(toggle());
        assert!(enabled());
        assert!(!toggle());
        assert!(!enabled());
    }

    /// The boom must sit behind the eye and above it, or the avatar is drawn in front of
    /// the camera and the shot looks up its own nose.
    #[test]
    fn the_boom_is_behind_and_above() {
        assert!(BOOM.z > 0.0, "+Z is behind the camera");
        assert!(BOOM.y > 0.0, "raised, so the body does not fill the lower half");
        assert!(BOOM.x.abs() > 0.0, "off-centre, so the body is not on the crosshair");
        assert!(MIN_FRACTION > 0.0 && MIN_FRACTION < 1.0);
    }
}
