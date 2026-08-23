//! EXT: Scene `'` -- "Backrooms". Not part of the C++ port.
//!
//! The intro's meadow and its white door, exactly as in `level15` (both are `ext/meadow.rs`)
//! -- but the door no longer opens onto a sunset sea. It opens onto the Backrooms: a scanned,
//! light-baked office interior (`ext/backrooms.rs`) with yellowed walls, buzzing lamps and a
//! maze of rooms behind the hall you arrive in. Same weather trick, same linked door; only the
//! far world differs.
//!
//! # What is different about this far world
//!
//! * **It is solid.** The sea was a quad with one ground collider. The backrooms is 70,000
//!   triangles, and the player has to walk on its carpet and be stopped by its walls, skirting
//!   and armchairs -- so the whole model is a triangle-mesh collider (`ext/trimesh.rs`),
//!   consulted by the engine beside the rectangle colliders it was born with.
//! * **It is its own weather.** Every other surface takes the `mood` grade of the pass camera;
//!   the backrooms' lighting is painted into its maps and its shader ignores the grade. What
//!   does take it -- the return door's paint, and the sky through any gap in the scan's
//!   single-sided walls -- is told the far world is an *interior* (`view::MOOD_INTERIOR`):
//!   the door stays the white it is, and the sky is the near-black of an unlit building going
//!   on past its walls. A dark ground cap (`backrooms::GroundCap`) does the same for the
//!   ground below the horizon. The intro's sunset grade would have turned the door pink.
//! * **It is placed by its door.** The level says where the return door stands (`FAR`) and
//!   `Backrooms::new` puts the model's own door spot on that point, carpet at world y = 0.
//! * **It can be fallen out of.** Single-sided walls push a sphere caught inside them to
//!   whichever side its centre is on, and the wrong side has no floor. A player under the
//!   carpet is put back at the arrival point (`ARRIVAL`), facing down the hall, by a
//!   `RoomLogic` -- see `ext::backrooms::fell_out` for the rule.
//!
//! Returning is symmetrical: the door on the carpet leads back to the meadow.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ext::backrooms::{fell_out, Backrooms, GroundCap, DOOR_FACING};
use crate::ext::bounds::bounds_box;
use crate::ext::meadow::{door_with_portal, load_meadow, FAR};
use crate::ext::room::{request_respawn, Respawn, RoomLogic};
use crate::ext::view;
use crate::game_header::GH_PLAYER_HEIGHT;
use crate::object::ObjectT;
use crate::player::Player;
use crate::portal::connect;
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec, Scene};
use crate::vector::Vector3;

pub struct Level16;

/// Clearance the invisible fence keeps around the model. The scan has doorways in its outer
/// walls that lead nowhere; a metre of margin lets the player stand in one without the fence
/// showing through it, and not walk out of it.
const FENCE_MARGIN: f32 = 1.0;
/// Where a player who has fallen out of the building is put back: a stride inside the arrival
/// door, at standing height on the carpet, which is where walking through it lands them. They
/// face down the hall, away from the door, as they would have on arrival.
const ARRIVAL: Vector3 = Vector3 { x: FAR.x - 1.0, y: FAR.y + GH_PLAYER_HEIGHT, z: FAR.z };

impl Scene for Level16 {
    fn load(
        &self,
        gl: &Rc<glow::Context>,
        res: &Resources,
        objs: &mut PObjectVec,
        portals: &mut PPortalVec,
        player: &mut Player,
    ) {
        let meadow = load_meadow(gl, res, objs, portals, player);
        // Past the split is a building, not a sunset: see the module docs. After `load_meadow`,
        // which turns the split on (and every load resets this to sunset).
        view::set_far_mood(view::MOOD_INTERIOR);

        // ── The backrooms: the model, placed so its door spot is FAR with the carpet at y = 0.
        let rooms = Backrooms::new(gl, res, FAR);
        let (lo, hi) = rooms.world_bounds();
        // Darkness under and around the building, for wherever its walls let the outside show.
        objs.push(Rc::new(RefCell::new(GroundCap::new(res, &rooms))) as Rc<RefCell<dyn ObjectT>>);
        objs.push(Rc::new(RefCell::new(rooms)) as Rc<RefCell<dyn ObjectT>>);

        // An invisible fence round the model's full extent, so nothing leaves through a
        // doorway in an outer wall. bounds_box takes the floor level and the wall height (not
        // a half-extent) on y: the floor is dropped a margin below the model and the walls
        // rise a margin above it.
        objs.push(Rc::new(RefCell::new(bounds_box(
            res,
            Vector3::new(0.5 * (lo.x + hi.x), lo.y - FENCE_MARGIN, 0.5 * (lo.z + hi.z)),
            Vector3::new(
                0.5 * (hi.x - lo.x) + FENCE_MARGIN,
                hi.y - lo.y + 2.0 * FENCE_MARGIN,
                0.5 * (hi.z - lo.z) + FENCE_MARGIN,
            ),
        ))) as Rc<RefCell<dyn ObjectT>>);

        // Under the carpet there is nothing, on purpose: a player who ends up there (see the
        // module docs) is put back at the arrival point rather than caught by a net and left
        // standing in the dark. The rule is checked every step against the fenced footprint
        // and the carpet level, which is FAR.y by construction (`carpet_lands_at_the_doors_foot`).
        let footprint = (
            Vector3::new(lo.x - FENCE_MARGIN, lo.y, lo.z - FENCE_MARGIN),
            Vector3::new(hi.x + FENCE_MARGIN, hi.y, hi.z + FENCE_MARGIN),
        );
        let arrival = Respawn::facing(ARRIVAL, -DOOR_FACING);
        objs.push(Rc::new(RefCell::new(RoomLogic::new(move |ctx| {
            if fell_out(ctx.player_pos, FAR.y, footprint) {
                request_respawn(arrival);
            }
        }))) as Rc<RefCell<dyn ObjectT>>);

        // The same door, from the carpet. It faces the hall's end wall, so stepping out of it
        // means looking down the hall; it leads back.
        let there =
            door_with_portal(gl, res, FAR, DOOR_FACING, false, meadow.link_there, objs, portals);

        // Walk in here, walk out there -- and back.
        connect(&meadow.here, &there);

        // ── Portraits along the hall, watching (`ext/painting.rs`). Five on the north wall,
        // three on the south, each a hair off its wall face so nothing is coplanar with the
        // scan. The watch is taken now, after both doors exist, so a painting knows it is
        // being looked at through the meadow door as well as from the carpet.
        {
            use crate::ext::painting::{Painting, Watch};
            /// The hall's wall faces in world z: `backrooms::DOOR_SPOT` puts model z = 3.48 and
            /// 7.07 here (the scan's walls are planes; a ray probe along the hall finds them
            /// at -1.5214 and 2.0721 at every x).
            const HALL_SOUTH_Z: f32 = -1.52;
            const HALL_NORTH_Z: f32 = 2.07;
            /// Clearance a painting's back keeps from the wall, so the two never z-fight.
            const WALL_GAP: f32 = 0.02;
            /// Centre height, and width x height. Eye level for a standing player is 1.5, so
            /// the sitter's eyes -- a little above the canvas centre -- are just above theirs.
            const HEIGHT: f32 = 1.6;
            const SIZE: (f32, f32) = (0.8, 1.0);

            let watch = Watch::new(objs, portals);
            let hang = |x: f32, wall_z: f32, facing_z: f32, seed: u32| {
                let centre = Vector3::new(x, HEIGHT, wall_z + facing_z * WALL_GAP);
                let facing = Vector3::new(0.0, 0.0, facing_z);
                Painting::new(res, centre, facing, SIZE, seed, watch.clone())
            };
            let north =
                [981.0, 985.0, 989.0, 993.0, 997.0].into_iter().map(|x| (x, HALL_NORTH_Z, -1.0));
            let south = [983.0, 991.0, 999.0].into_iter().map(|x| (x, HALL_SOUTH_Z, 1.0));
            for (seed, (x, wall_z, facing_z)) in north.chain(south).enumerate() {
                objs.push(Rc::new(RefCell::new(hang(x, wall_z, facing_z, seed as u32)))
                    as Rc<RefCell<dyn ObjectT>>);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ext::backrooms::DOOR_SPOT;
    use crate::ext::door::{portal_placement, yaw_facing};
    use crate::ext::meadow::DOOR_POS;
    use crate::game_header::GH_PI;
    use crate::object::Object;

    /// The carpet must be at world y = 0 where the door stands: the door's foot is at FAR.y
    /// and the model is shifted so its floor level lands there.
    #[test]
    fn carpet_lands_at_the_doors_foot() {
        let model_pos = FAR - DOOR_SPOT;
        assert!((model_pos.y + crate::ext::backrooms::FLOOR_Y - FAR.y).abs() < 1e-6);
    }

    /// The two doors have different yaws (the meadow's faces +z, the carpet's faces +x), and
    /// `connect` must carry a player across that turn. This reproduces the transform
    /// `connect_warps` builds for walking in through `here` (portal.rs: `there_l2w * here_w2l`)
    /// and the re-aim `Physical::try_portal` applies to it, on the transforms the two doors'
    /// portals actually get -- `door_with_portal` and this both call `portal_placement`.
    #[test]
    fn walking_through_lands_in_the_hall_facing_down_it() {
        let portal = |pos: Vector3, facing: Vector3| {
            let (centre, euler, scale) = portal_placement(pos, yaw_facing(facing));
            let mut p = Object::new();
            p.pos = centre;
            p.euler = euler;
            p.scale = scale;
            p
        };
        let here = portal(DOOR_POS, Vector3::new(0.0, 0.0, 1.0));
        let there = portal(FAR, DOOR_FACING);
        let delta_inv = there.local_to_world() * here.world_to_local();

        // The relative yaw is a quarter turn.
        assert!((there.euler.y - here.euler.y - GH_PI / 2.0).abs() < 1e-6);

        // A player a hair past the meadow door's plane, having walked in along -z...
        let p = DOOR_POS + Vector3::new(0.0, GH_PLAYER_HEIGHT, -0.01);
        let q = delta_inv.mul_point(p);
        // ...lands the same hair past the carpet door on its -x side, at the same height.
        let expect = FAR + Vector3::new(-0.01, GH_PLAYER_HEIGHT, 0.0);
        assert!((q - expect).mag() < 1e-3, "landed at {q:?}, expected {expect:?}");
        // The respawn point is a stride further along the same line: where an arrival would
        // be a moment later, not somewhere else in the building.
        let along = ARRIVAL - expect;
        assert!(along.x < -0.5 && along.y.abs() < 1e-6 && along.z.abs() < 1e-6, "{along:?}");

        // Physical::try_portal re-aims the camera yaw from the warped forward vector.
        let yaw_in = 0.0f32; // looking -z at the meadow door, as the spawn does
        let forward = Vector3::new(-yaw_in.sin(), 0.0, -yaw_in.cos());
        let new_dir = delta_inv.mul_direction(forward);
        let yaw_out = -new_dir.x.atan2(-new_dir.z);
        let forward_out = Vector3::new(-yaw_out.sin(), 0.0, -yaw_out.cos());
        // Facing -x: down the hall, away from the end wall the door faces.
        assert!(forward_out.x < -0.999, "came out facing {forward_out:?}");
    }
}
