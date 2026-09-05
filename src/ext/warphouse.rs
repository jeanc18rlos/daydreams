//! EXT: the non-Euclidean fittings of the Open House -- a mouth that changes your size, and
//! pockets in the backs of things. Not part of the C++ port.
//!
//! Everything here is one `Portal` pair and one product of matrices. `connect(a, b)` bakes
//! `a_local_to_world * b_world_to_local` into the warp, which means a traveller keeps its
//! coordinates RELATIVE TO THE QUAD as it crosses. Every trick in this module is a choice
//! about how the two quads differ:
//!
//! * identical size, identical facing, a pure translation apart -- a seam (level31's street);
//! * identical size, turned -- a doorway that faces somewhere else (the pockets below);
//! * **different size** -- the crossing multiplies the traveller's `p_scale` by the ratio,
//!   which is the scale mouth. `Physical::try_portal` does it in one line, and because
//!   `p_scale` is the engine's whole notion of how big you are, walk speed, jump, gravity,
//!   eye height, head bob and your own collision spheres all follow from that one number.
//!   Nothing is scaled up: YOU get smaller, so the ordinary living room you step out into is
//!   twice the room it was.
//!
//! # The rules a portal here has to obey
//!
//! * **Vertical.** `Portal::draw` asserts `euler.x == 0 && euler.z == 0`.
//! * **Uniform ratio.** The warp must be a similarity, so a scaling pair's half-extents must
//!   differ by the same factor in all three components -- the z half-extent too, even though
//!   the quad is flat. Getting that wrong shears the world.
//! * **Proud of the wall.** The player's head is a sphere of `GH_PLAYER_RADIUS` = 0.2 m; a
//!   quad flush with a wall can never be reached, because the eye stops 0.2 m short of it
//!   and the crossing test never sees the segment cross the plane. Every mouth here stands
//!   [`STAND_OFF`] into the room.
//! * **Sixteen.** `GH_MAX_PORTALS` is 16 and the render path indexes fixed-size arrays by
//!   portal index -- a seventeenth is an out-of-bounds panic every frame in release, not a
//!   debug assert. The street loop spends 2; [`PORTAL_BUDGET`] is what is left, and
//!   `level32` asserts it stays within.

use std::cell::RefCell;
use std::rc::Rc;

use crate::portal::{connect, Portal};
use crate::resources::Resources;
use crate::scene::PPortalVec;
use crate::vector::Vector3;

/// How far a mouth stands off the wall it belongs to, in metres: clear of the player's own
/// head sphere with room to spare (module docs).
pub const STAND_OFF: f32 = 0.45;

/// Portals the street loop already spends, and what a scene may add on top before it hits
/// `GH_MAX_PORTALS`.
pub const LOOP_PORTALS: usize = 2;
pub const PORTAL_BUDGET: usize = crate::game_header::GH_MAX_PORTALS - LOOP_PORTALS;

/// Half-extents of the ordinary, walk-in-sized mouth: 1.2 m across and 2.0 m tall, a little
/// wider than a door so it reads as an opening rather than a doorway.
pub const MOUTH_BIG: Vector3 = Vector3 { x: 0.6, y: 1.0, z: 1.0 };
/// What crossing the big mouth does to the player's size.
pub const SCALE_RATIO: f32 = 0.5;
/// The far end of a scale mouth. DERIVED from the ratio in all three axes, the flat one
/// included: the warp has to be a similarity or the world shears at the crossing, and a
/// hand-typed triple is exactly how that goes wrong (module docs).
pub const MOUTH_SMALL: Vector3 = Vector3 {
    x: MOUTH_BIG.x * SCALE_RATIO,
    y: MOUTH_BIG.y * SCALE_RATIO,
    z: MOUTH_BIG.z * SCALE_RATIO,
};

/// Half-extents of a pocket mouth. Wide enough that a player walking at it goes THROUGH
/// it: a mouth the width of a wardrobe reads better but is missed constantly, because the
/// approach is never a straight line -- furniture nudges you, and slipping past the edge of
/// a portal looks exactly like the portal being broken.
pub const MOUTH_POCKET: Vector3 = Vector3 { x: 0.75, y: 1.0, z: 1.0 };

/// One end of a warp: where its quad stands, which way it faces, and how big it is.
///
/// `yaw` is the direction the mouth LOOKS -- a player walking along `-facing` walks into it.
/// The quad's centre is `foot` raised by its own half-height, so callers give floor
/// positions and the mouth stands on the floor like a doorway.
#[derive(Clone, Copy, Debug)]
pub struct Mouth {
    pub foot: Vector3,
    pub yaw: f32,
    pub half: Vector3,
}

impl Mouth {
    pub fn new(foot: Vector3, yaw: f32, half: Vector3) -> Mouth {
        Mouth { foot, yaw, half }
    }

    /// The quad's centre: its own half-height above the floor point.
    pub fn centre(&self) -> Vector3 {
        self.foot + Vector3::new(0.0, self.half.y, 0.0)
    }

    fn build(&self, res: &Resources) -> Rc<RefCell<Portal>> {
        let mut p = Portal::new(res);
        p.base.pos = self.centre();
        p.base.euler = Vector3::new(0.0, self.yaw, 0.0);
        p.base.scale = self.half;
        Rc::new(RefCell::new(p))
    }
}

/// Stand a connected pair of mouths in the world and hand them to the scene.
///
/// Crossing `a` puts the traveller at `b` with `p_scale` multiplied by `b.half / a.half` --
/// so a big `a` and a small `b` shrink whoever walks in, and walking back out grows them
/// again. A pair of equal mouths is an ordinary doorway to somewhere else.
pub fn pair(
    res: &Resources,
    portals: &mut PPortalVec,
    a: Mouth,
    b: Mouth,
) -> (Rc<RefCell<Portal>>, Rc<RefCell<Portal>>) {
    let pa = a.build(res);
    let pb = b.build(res);
    portals.push(pa.clone());
    portals.push(pb.clone());
    connect(&pa, &pb);
    (pa, pb)
}

/// A mouth that halves whoever walks into it, and the far end they step out of.
///
/// `into` is the wall the mouth stands against, `out_at` the floor point they arrive at.
/// The far mouth faces `out_yaw`, which is the way they will be walking when they arrive.
pub fn scale_mouth(
    res: &Resources,
    portals: &mut PPortalVec,
    into: Mouth,
    out_at: Vector3,
    out_yaw: f32,
) -> (Rc<RefCell<Portal>>, Rc<RefCell<Portal>>) {
    let big = Mouth::new(into.foot, into.yaw, MOUTH_BIG);
    let small = Mouth::new(out_at, out_yaw, MOUTH_SMALL);
    pair(res, portals, big, small)
}

/// A pocket: the back of a piece of furniture opening onto somewhere that is not in this
/// house at all. Both mouths are the same size, so nothing about the traveller changes but
/// where they are.
pub fn pocket(
    res: &Resources,
    portals: &mut PPortalVec,
    here: Vector3,
    here_yaw: f32,
    there: Vector3,
    there_yaw: f32,
) -> (Rc<RefCell<Portal>>, Rc<RefCell<Portal>>) {
    pair(
        res,
        portals,
        Mouth::new(here, here_yaw, MOUTH_POCKET),
        Mouth::new(there, there_yaw, MOUTH_POCKET),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_header::{GH_MAX_PORTALS, GH_PLAYER_RADIUS};

    /// The warp must be a similarity: the two mouths of a scale pair differ by the SAME
    /// factor in all three axes, the flat one included. A pair that scales x and y but not
    /// z shears the world at the crossing.
    #[test]
    fn the_scale_pair_is_a_uniform_similarity() {
        let rx = MOUTH_SMALL.x / MOUTH_BIG.x;
        let ry = MOUTH_SMALL.y / MOUTH_BIG.y;
        let rz = MOUTH_SMALL.z / MOUTH_BIG.z;
        assert!((rx - SCALE_RATIO).abs() < 1e-6, "x ratio {rx}");
        assert!((ry - SCALE_RATIO).abs() < 1e-6, "y ratio {ry}");
        assert!((rz - SCALE_RATIO).abs() < 1e-6, "z ratio {rz} -- the flat axis counts too");
    }

    /// A mouth must be wide enough and tall enough for a player to walk into, and it must
    /// stand clear of the wall behind it: a quad flush with a wall can never be crossed,
    /// because the head sphere stops short of the plane.
    #[test]
    fn a_mouth_is_walk_in_sized_and_stands_proud() {
        assert!(MOUTH_BIG.x > GH_PLAYER_RADIUS * 2.0, "too narrow to walk into");
        assert!(MOUTH_BIG.y * 2.0 > crate::game_header::GH_PLAYER_HEIGHT, "too low");
        // Comfortably wider than the player, not merely wider: an approach is never a
        // straight line, and a mouth you can slip past the edge of reads as a bug.
        assert!(MOUTH_POCKET.x > GH_PLAYER_RADIUS * 3.0, "the pocket is too easy to miss");
        assert!(MOUTH_POCKET.y * 2.0 > crate::game_header::GH_PLAYER_HEIGHT, "too low");
        assert!(STAND_OFF > GH_PLAYER_RADIUS * 2.0, "the mouth would be unreachable");
        // The far end of a scale mouth is crossed at half size, so it only has to clear a
        // halved player.
        assert!(MOUTH_SMALL.x > GH_PLAYER_RADIUS * SCALE_RATIO * 2.0);
    }

    /// A mouth stands on the floor: its centre is its own half-height up, so callers may
    /// give floor points and never think about it.
    #[test]
    fn a_mouth_stands_on_the_floor_it_was_given() {
        let foot = Vector3::new(3.0, 0.84, -2.0);
        let m = Mouth::new(foot, 0.0, MOUTH_BIG);
        assert!((m.centre().y - (foot.y + MOUTH_BIG.y)).abs() < 1e-6);
        assert!((m.centre().x - foot.x).abs() < 1e-6);
        // And its bottom edge is exactly the floor.
        assert!((m.centre().y - MOUTH_BIG.y - foot.y).abs() < 1e-6);
    }

    /// The budget is real: the render path indexes fixed-size arrays by portal index, so a
    /// seventeenth portal is a panic every frame in release, not a debug assert.
    #[test]
    fn the_budget_is_what_is_left_after_the_street_loop() {
        assert_eq!(PORTAL_BUDGET, GH_MAX_PORTALS - LOOP_PORTALS);
        assert_eq!(GH_MAX_PORTALS, 16);
        assert!(PORTAL_BUDGET >= 8, "no room for the fittings");
    }
}
