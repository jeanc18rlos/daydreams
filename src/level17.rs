//! EXT: Scene `,` -- "Pool Rooms". Not part of the C++ port.
//!
//! Blenderust's "Level 37 flooded tiled complex" (`Meshes/level_37_flooded_tiled_complex.glb`,
//! CC-BY-4.0; licence beside it, credit in `THIRD_PARTY.md`): a white-tiled hall of pillars
//! and ceiling lamps, flooded knee-deep, with a second storey above, marble slides through
//! both and a tiled basement under the floor. Loaded through `ext/interior.rs`: solid from
//! the tiles up, the water a true translucent pass the player wades through.
//!
//! # What the file holds, measured (model metres, Y up)
//!
//! Bounds x[-8.0, 28.9], y[-7.2, 7.3], z[-23.2, 10.2]. The lower hall -- the level -- is a
//! flat `Mosaic__white_tiles` floor at **y = 0** (904 m^2, no holes: the basement under it is
//! sealed) inside walls whose inner faces are x = -3.15 and 27.62, z = -22.13 and 3.3, so
//! about 31 x 25 m; its ceiling is the upper floor's slab at 4.15, with lamps hanging to 3.64.
//! The water sheet lies at **y = 0.78**: 0.78 m deep over the whole hall, which is wading --
//! the eye at 1.5 stays 0.72 m above the surface and the camera never dips under it. The
//! pillars stand on a 6 m grid (the first row at x[-0.74, 0.74]). Above 1.82 m the outer
//! walls are pierced by arched windows, 2.75 m wide on a 4.5 m pitch, open to the dark
//! outside (the interior sky and the ground cap); the piers between them are 1.75 m.
//!
//! The upper storey (floor 4.2, its own water at 4.54) is reached only by the spiral stair
//! at the hall's south-west corner (x 0, z -0.2), and the engine cannot climb stairs -- the
//! foot sphere stops at every riser, see `level13.rs` -- so it stays scenery, as do the
//! marble slides (they leave the building through the south wall) and the basement. What is
//! reachable is the hall, end to end.
//!
//! # Placement
//!
//! The hall's length runs along model +x, and the arrival looks down it: the model is turned
//! a quarter turn so model +x is world -z (the default heading), and shifted so the arrival
//! spot is the origin with the tiles at y = 0. The elevator is reserved on the west wall's
//! inner face at its mid-length, between two pillar rows -- 25 m of flat wall at head
//! height, the nearest pillars 2.4 m out and flanking the cabin rather than facing it --
//! its doorway looking down the 31 m of hall between them. The pillar rows line up with the
//! wall's piers, so between two rows there is a window over the doorway, from 1.82 m up: a
//! 4.4 m cabin panel covers a window wherever it goes on this wall, and the cabin, being
//! opaque and 3.3 m tall, is what hides the hole.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ext::elevator::{self, Elevator, PROUD};
use crate::ext::gltf_model::{Anchor, Fit, Frame, Load, PartSpec};
use crate::ext::interior::{self, Openings, SPAWN_AHEAD};
use crate::game_header::GH_PI;
use crate::object::{Object, ObjectT};
use crate::player::Player;
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec, Scene};
use crate::vector::Vector3;

pub struct Level17;

const MODEL: &str = "Meshes/level_37_flooded_tiled_complex.glb";
/// The whole file, from the scene root (its Z-up to Y-up rotation applied).
const PART: &str = "all";
/// Every map in the file is 1024 square or smaller: nothing is resampled.
const MAP: u32 = 1024;
/// The one material drawn blended (see `ext/gltf_model.rs`, "Alpha"): both water sheets.
const WATER: &str = "Water.002";

const PARTS: [PartSpec<'static>; 1] =
    [PartSpec { name: PART, roots: &[], skip: &[], frame: Frame::Scene, anchor: Anchor::Hinge }];

fn load_spec() -> Load<'static> {
    Load {
        path: MODEL,
        parts: &PARTS,
        fit: Fit::Identity,
        max_map: MAP,
        translucent: &[WATER],
        metallic_override: &[],
        cut_boxes: &[],
    }
}

/// The lower hall's tiles, in the model.
const FLOOR_Y: f32 = 0.0;

/// The hall's west wall, inner face, where the elevator's doorway is reserved: the wall
/// runs z[-22.13, 3.3] at this x, flat up to the windows at 1.82 (and 1.05 m thick: the
/// outer face is at -4.2).
const WALL_X: f32 = -3.15;
/// Mid-length of the hall, centred between two pillar rows (their faces at z = -11.48 and
/// -6.86, so the cabin's 4.4 m panel clears both by a hand); the window over it spans
/// z[-10.75, -8.0], between the piers at z[-12.75, -11.0] and z[-7.75, -6.0].
const DOORWAY_Z: f32 = -9.15;
/// The doorway point in the model -- the slab's face stands `elevator::PROUD` off the wall
/// -- and the way it faces: down the hall.
const DOORWAY_MODEL: Vector3 = Vector3 { x: WALL_X + PROUD, y: FLOOR_Y, z: DOORWAY_Z };
const FACING_MODEL: Vector3 = Vector3 { x: 1.0, y: 0.0, z: 0.0 };
/// The turn that takes model +x to world -z (`Matrix4::rot_y`: local +x goes to
/// (cos a, 0, -sin a)).
const MODEL_YAW: f32 = GH_PI / 2.0;

/// World coordinates of the elevator: its threshold -- the cabin's floor point on the slab's
/// face, [`PROUD`] off the wall's inner face and [`SPAWN_AHEAD`] behind the arrival along
/// +z -- and the yaw its doorway faces -- -z, down the hall, as an `Object::euler.y`
/// (`interior::facing`, `door::yaw_facing`). The spawn is derived from these
/// (`interior::arrival`), and the tests below measure the wall and the floor of the file
/// against them.
pub const ELEVATOR_SPOT: Vector3 = Vector3 { x: 0.0, y: 0.0, z: SPAWN_AHEAD };
pub const ELEVATOR_YAW: f32 = GH_PI;

/// Where the model stands: turned by [`MODEL_YAW`] and shifted so the arrival spot --
/// [`SPAWN_AHEAD`] in front of the doorway -- lands on the origin.
fn placement() -> Object {
    let mut obj = Object::new();
    obj.euler.y = MODEL_YAW;
    let arrival_model = DOORWAY_MODEL + FACING_MODEL * SPAWN_AHEAD;
    obj.pos = -obj.local_to_world().mul_direction(arrival_model);
    obj
}

impl Scene for Level17 {
    fn load(
        &self,
        gl: &Rc<glow::Context>,
        res: &Resources,
        objs: &mut PObjectVec,
        _portals: &mut PPortalVec,
        player: &mut Player,
    ) {
        // The elevator first, set into the hall's west wall (`ext/elevator.rs`, "Set into a wall"):
        // the model is carved round its doorway as it loads, and the fence takes in the
        // cabin, which stands outside the model behind the wall.
        let arrived = elevator::take_arrival();
        let lift = Elevator::new(gl, res, ELEVATOR_SPOT, ELEVATOR_YAW, arrived);
        let openings = Openings { cut: &[lift.wall_cut()], also_inside: &[lift.world_bounds()] };
        let arrival = interior::arrival(ELEVATOR_SPOT, ELEVATOR_YAW);
        interior::load(gl, res, objs, player, &load_spec(), &placement(), openings, 0.0, arrival);
        if arrived.is_some() {
            // Delivered by a ride: in the cabin, facing its doors, which are about to open.
            lift.board(player);
        }
        objs.push(Rc::new(RefCell::new(lift.doors())) as Rc<RefCell<dyn ObjectT>>);
        objs.push(Rc::new(RefCell::new(lift)) as Rc<RefCell<dyn ObjectT>>);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ext::door::yaw_facing;
    use crate::ext::interior::probe::{
        ceiling_over, floor_under, nearest_hit, world_triangles, world_triangles_all,
    };
    use crate::ext::interior::{facing, FENCE_MARGIN};
    use crate::game_header::GH_PLAYER_HEIGHT;

    /// The doorway's facing in the world: -z.
    const ELEVATOR_FACING: Vector3 = Vector3 { x: 0.0, y: 0.0, z: -1.0 };

    fn tris() -> Vec<[Vector3; 3]> {
        world_triangles(&load_spec(), PART, &placement())
    }

    fn arrival() -> Vector3 {
        interior::arrival(ELEVATOR_SPOT, ELEVATOR_YAW).pos
    }

    /// The placement's promise: the doorway lands on `ELEVATOR_SPOT`, its facing on the
    /// yaw's, and the arrival on the origin.
    #[test]
    fn placement_puts_the_doorway_and_the_arrival_where_the_constants_say() {
        let m = placement().local_to_world();
        let doorway = m.mul_point(DOORWAY_MODEL);
        assert!((doorway - ELEVATOR_SPOT).mag() < 1e-4, "doorway at {doorway:?}");
        let dir = m.mul_direction(FACING_MODEL);
        assert!((dir - ELEVATOR_FACING).mag() < 1e-5, "facing {dir:?}");
        assert!((facing(ELEVATOR_YAW) - ELEVATOR_FACING).mag() < 1e-6);
        assert!((yaw_facing(ELEVATOR_FACING) - ELEVATOR_YAW).abs() < 1e-6);
        let spot = m.mul_point(DOORWAY_MODEL + FACING_MODEL * SPAWN_AHEAD);
        assert!(spot.mag() < 1e-4, "arrival spot at {spot:?}");
        assert!((arrival() - Vector3::new(0.0, GH_PLAYER_HEIGHT, 0.0)).mag() < 1e-5);
    }

    /// The arrival stands on the tiles at world y = 0 with the water line 0.78 m over them
    /// -- the first thing under the eye is water, the first solid thing the tiles -- and the
    /// hall's ceiling (a lamp, actually) well over head height.
    #[test]
    fn arrival_is_on_the_tiles_under_the_water_line_and_the_ceiling() {
        let t = tris();
        let eye = arrival();
        let floor = floor_under(&t, eye).expect("a floor");
        assert!(floor.abs() < 0.01, "floor at {floor}");
        assert!((eye.y - floor - GH_PLAYER_HEIGHT).abs() < 0.01);
        let ceiling = ceiling_over(&t, eye).expect("a ceiling");
        assert!(ceiling > 2.4, "ceiling at {ceiling}");
        let water = floor_under(&world_triangles_all(&load_spec(), PART, &placement()), eye)
            .expect("a surface");
        assert!((water - 0.78).abs() < 0.01, "water line at {water}");
        // Wading, not swimming: the water line is below the eye and above the feet.
        assert!(water < eye.y - 0.5 && water > floor + 0.3);
    }

    /// The reserved wall: flat at head height where the cabin's 4.4 m panel goes, a window
    /// over the doorway from 1.82 up (the asset's, see the module docs), the pillar rows no
    /// nearer than 2.3 m anywhere in front, and the doorway's own width looking down the
    /// hall past the stride out.
    #[test]
    fn elevator_wall_is_flat_and_clear_in_front() {
        let t = tris();
        let mid = Vector3::new(0.0, 1.2, 0.0);
        let back = -ELEVATOR_FACING;
        // The window: nothing behind the doorway at 2.3 m up, a pier 1.75 m to either side.
        let high = Vector3::new(0.0, 2.3, 0.0);
        assert!(nearest_hit(&t, ELEVATOR_SPOT + high + ELEVATOR_FACING * 0.1, back).is_none());
        for side in [-2.0, 2.0] {
            let from = ELEVATOR_SPOT + Vector3::new(side, 0.0, 0.0) + high + ELEVATOR_FACING * 0.1;
            let d = nearest_hit(&t, from, back).expect("a pier");
            assert!((d - 0.1 - PROUD).abs() < 0.02, "pier {d} behind at {side}");
        }
        let right = ELEVATOR_FACING.cross(Vector3::new(0.0, 1.0, 0.0));
        for i in 0..=18 {
            let side = -2.25 + 0.25 * i as f32;
            let off = right * side;
            // From just inside the doorway, the wall is a hair behind the slab's face...
            let from = ELEVATOR_SPOT + off + mid + ELEVATOR_FACING * 0.1;
            let d = nearest_hit(&t, from, back).expect("a wall behind");
            assert!((d - 0.1 - PROUD).abs() < 0.02, "wall {d} behind at offset {side}");
            // ...and nothing in front nearer than the pillar rows, which flank the cabin.
            let ahead = nearest_hit(&t, from, ELEVATOR_FACING).unwrap_or(f32::MAX);
            assert!(ahead > 2.3, "{ahead} m clear at offset {side}");
            // The doorway's width looks down the whole hall.
            if side.abs() <= 1.0 {
                assert!(ahead > 25.0, "{ahead} m of hall at offset {side}");
            }
        }
        // And the wall's inner face is PROUD behind ELEVATOR_SPOT exactly.
        let face = placement().local_to_world().mul_point(Vector3::new(WALL_X, 1.2, DOORWAY_Z));
        let d = nearest_hit(&t, face + ELEVATOR_FACING * 0.05, back).expect("the face");
        assert!((d - 0.05).abs() < 0.005, "face {d}");
        assert!(((face - ELEVATOR_SPOT).dot(ELEVATOR_FACING) + PROUD).abs() < 1e-4);
    }

    /// The arrival is inside the fence, and the fence is outside the model.
    #[test]
    fn arrival_is_inside_the_fence() {
        let spec = load_spec();
        let b = crate::ext::gltf_model::GltfModel::probe_bounds(&spec, PART);
        let (lo, hi) = interior::world_bounds(b, &placement());
        let p = arrival();
        for (v, l, h) in [(p.x, lo.x, hi.x), (p.y, lo.y, hi.y), (p.z, lo.z, hi.z)] {
            assert!(v > l - FENCE_MARGIN && v < h + FENCE_MARGIN, "{p:?} outside {lo:?}..{hi:?}");
        }
        // The whole hall is inside too: the model is 37 x 33 m on the ground, turned.
        assert!((hi.x - lo.x - 33.4).abs() < 0.1 && (hi.z - lo.z - 36.9).abs() < 0.1);
    }

    #[test]
    fn registered_on_comma() {
        let e = crate::ext::scenes::SCENES.iter().find(|e| e.name == "Pool Rooms").expect("listed");
        assert_eq!(e.key, b',');
    }
}
