//! EXT: Scene `.` -- "Overgrown". Not part of the C++ port.
//!
//! Blenderust's "Backrooms room with plants, overgrown"
//! (`Meshes/backrooms_room_with_plants_overgrown.glb`, CC-BY-4.0; licence beside it, credit in
//! `THIRD_PARTY.md`): a 20 x 20 m maze of yellow wallpaper under a 2.43 m plaster ceiling,
//! its floor moss, its corners gone to grass and bushes, lit by ceiling panels and two red
//! EXIT signs over two rusted doors. Loaded through `ext/interior.rs`.
//!
//! # What the file holds, measured (model metres, Y up)
//!
//! Bounds x[-10.5, 10.6], y[0, 2.43], z[-10.7, 10.6], the overhang being foliage. The floor
//! (`Thick_Moss`) is one quad at **y = 0**, 20 m square; the outer walls are single quads
//! on x = +-10.01 and z = +-10.01 facing inward, the maze inside them thin slabs of the same
//! `Backrooms_Wallpaper`; the ceiling is a single quad at 2.43, facing down. The doors are
//! shut leaves on the z = +-10 walls at x[-7.7, -6.5], an exit sign over each. Everything
//! green -- `Bush_*`, `Grass_*`, `Grass_Realistic_5` on the walls -- is alpha-tested cards,
//! and none of it collides (`GltfModel::solid_triangles`); the bushes also ship with glTF's
//! default metalness of 1.0, which would draw them as dark metal, so the load overrides it
//! (`Load::metallic_override`, and see the gotcha in `ext/gltf_model.rs`).
//!
//! The whole room is reachable: one floor, no stairs.
//!
//! The room is also the far side of the Backrooms' window (`ext/window.rs`): `level16.rs`
//! loads it a second time, by this file's `load_spec` and `placement` shifted east, and a
//! player who walks through the window arrives here at the same spot (`window::take_arrival`
//! below). There is no window back; the elevator is the way out.
//!
//! # Placement
//!
//! The elevator is reserved on the south wall (z = +10.01, facing -z): the one stretch of
//! outer wall 4.5 m wide with 4 m of open floor in front of it that is not a door -- from
//! x = -3.7, where a maze wall meets the outer one, to x = 1.9, the end of the central block;
//! the cabin's 4.4 m centred at x = -0.9 leaves 0.6 m either side. The block's wall stands
//! 3.9 m in front. The arrival looks -z into the maze, which is the default heading, so the
//! model is not turned -- only shifted so the arrival spot is the origin.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ext::elevator::{self, Elevator, PROUD};
use crate::ext::gltf_model::{Anchor, Fit, Frame, Load, PartSpec};
use crate::ext::interior::{self, Openings, SPAWN_AHEAD};
use crate::ext::window;
use crate::game_header::GH_PI;
use crate::object::{Object, ObjectT};
use crate::player::Player;
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec, Scene};
use crate::vector::Vector3;

pub struct Level18;

const MODEL: &str = "Meshes/backrooms_room_with_plants_overgrown.glb";
/// The whole file, from the scene root.
pub(crate) const PART: &str = "all";
/// Every map in the file is 1024 square or smaller: nothing is resampled.
const MAP: u32 = 1024;
/// The foliage, rendered as the dielectric it is (module docs): the bushes ship metalness 1
/// by omission, the grass 0.5 by mistake; the moss floor's 0.5 goes too, a floor being no
/// more a metal than a leaf.
const DIELECTRIC: [(&str, f32); 3] = [("Bush_*", 0.0), ("Grass_*", 0.0), ("Thick_Moss", 0.0)];

const PARTS: [PartSpec<'static>; 1] =
    [PartSpec { name: PART, roots: &[], skip: &[], frame: Frame::Scene, anchor: Anchor::Hinge }];

/// The file and its overrides. Shared with the Backrooms (`level16.rs`), which loads the room
/// a second time as the far side of its window (`ext/window.rs`).
pub(crate) fn load_spec() -> Load<'static> {
    Load {
        path: MODEL,
        parts: &PARTS,
        fit: Fit::Identity,
        max_map: MAP,
        translucent: &[],
        metallic_override: &DIELECTRIC,
        cut_boxes: &[],
    }
}

/// The moss, in the model.
const FLOOR_Y: f32 = 0.0;

/// The south wall's inner face, where the elevator's doorway is reserved.
const WALL_Z: f32 = 10.01;
/// Centre of the clear stretch x[-3.7, 1.9] (module docs).
const DOORWAY_X: f32 = -0.9;
/// The doorway point in the model -- the slab's face stands `elevator::PROUD` off the wall
/// -- and the way it faces: into the maze, -z.
const DOORWAY_MODEL: Vector3 = Vector3 { x: DOORWAY_X, y: FLOOR_Y, z: WALL_Z - PROUD };
const FACING_MODEL: Vector3 = Vector3 { x: 0.0, y: 0.0, z: -1.0 };

/// World coordinates of the elevator: its threshold -- the cabin's floor point on the slab's
/// face, [`PROUD`] off the wall's inner face and [`SPAWN_AHEAD`] behind the arrival along
/// +z -- and the yaw its doorway faces -- -z, into the maze, as an `Object::euler.y`
/// (`interior::facing`, `door::yaw_facing`). The spawn is derived from these
/// (`interior::arrival`), and the tests below measure the wall and the floor of the file
/// against them.
pub const ELEVATOR_SPOT: Vector3 = Vector3 { x: 0.0, y: 0.0, z: SPAWN_AHEAD };
/// The room's plaster ceiling, above its floor at y = 0 (module docs; a test measures it):
/// what the window's opening has to fit under on the far side (`ext/window.rs`).
pub const CEILING: f32 = 2.43;
pub const ELEVATOR_YAW: f32 = GH_PI;

/// Where the model stands: unturned, shifted so the arrival spot -- [`SPAWN_AHEAD`] in
/// front of the doorway -- lands on the origin. Shared with the Backrooms, as `load_spec` is.
pub(crate) fn placement() -> Object {
    let mut obj = Object::new();
    obj.pos = -(DOORWAY_MODEL + FACING_MODEL * SPAWN_AHEAD);
    obj
}

impl Scene for Level18 {
    fn load(
        &self,
        gl: &Rc<glow::Context>,
        res: &Resources,
        objs: &mut PObjectVec,
        _portals: &mut PPortalVec,
        player: &mut Player,
    ) {
        // EXT: what the footsteps land on (src/ext/audio.rs).
        crate::ext::audio::set_surface(crate::ext::audio::Surface::Moss);
        // The elevator first, set into the south wall (`ext/elevator.rs`, "Set into a wall"):
        // the model is carved round its doorway as it loads, and the fence takes in the
        // cabin, which stands outside the model behind the wall.
        let arrived = elevator::take_arrival();
        // And the other way in: through the Backrooms' window. Taken whatever the elevator
        // said, so a stale one cannot wait for a later load; a ride wins, there being no
        // way to have done both.
        let through = window::take_arrival();
        let lift = Elevator::new(gl, res, ELEVATOR_SPOT, ELEVATOR_YAW, arrived);
        let openings = Openings { cut: &[lift.wall_cut()], also_inside: &[lift.world_bounds()] };
        let arrival = interior::arrival(ELEVATOR_SPOT, ELEVATOR_YAW);
        interior::load(gl, res, objs, player, &load_spec(), &placement(), openings, 0.0, arrival);
        if arrived.is_some() {
            // Delivered by a ride: in the cabin, facing its doors, which are about to open.
            lift.board(player);
        } else if let Some(through) = through {
            // Walked in through the window (`ext/window.rs`): the same spot and the same look
            // as in the copy the step before, so nothing is seen to change.
            player.base.set_position(through.pos);
            player.set_look(through.yaw, through.pitch);
        }
        objs.push(Rc::new(RefCell::new(lift.doors())) as Rc<RefCell<dyn ObjectT>>);
        objs.push(Rc::new(RefCell::new(lift)) as Rc<RefCell<dyn ObjectT>>);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ext::door::yaw_facing;
    use crate::ext::interior::probe::{ceiling_over, floor_under, nearest_hit, world_triangles};
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

    #[test]
    fn placement_puts_the_doorway_and_the_arrival_where_the_constants_say() {
        let m = placement().local_to_world();
        let doorway = m.mul_point(DOORWAY_MODEL);
        assert!((doorway - ELEVATOR_SPOT).mag() < 1e-4, "doorway at {doorway:?}");
        assert!((m.mul_direction(FACING_MODEL) - ELEVATOR_FACING).mag() < 1e-6);
        assert!((facing(ELEVATOR_YAW) - ELEVATOR_FACING).mag() < 1e-6);
        assert!((yaw_facing(ELEVATOR_FACING) - ELEVATOR_YAW).abs() < 1e-6);
        let spot = m.mul_point(DOORWAY_MODEL + FACING_MODEL * SPAWN_AHEAD);
        assert!(spot.mag() < 1e-4, "arrival spot at {spot:?}");
        assert!((arrival() - Vector3::new(0.0, GH_PLAYER_HEIGHT, 0.0)).mag() < 1e-5);
    }

    /// The arrival stands on the moss at world y = 0 under the 2.43 m ceiling -- measured
    /// from the solid triangles, so a grass card lying flat at 0.33 cannot pass for a floor.
    #[test]
    fn arrival_is_on_the_moss_under_the_ceiling() {
        let t = tris();
        let eye = arrival();
        let floor = floor_under(&t, eye).expect("a floor");
        assert!(floor.abs() < 0.01, "floor at {floor}");
        assert!((eye.y - floor - GH_PLAYER_HEIGHT).abs() < 0.01);
        let ceiling = ceiling_over(&t, eye).expect("a ceiling");
        assert!((ceiling - CEILING).abs() < 0.02, "ceiling at {ceiling}");
    }

    /// The reserved wall: flat for the cabin's 4.4 m panel, nothing in front of it for the
    /// cabin's depth and the stride out, and the maze's first wall further than that.
    #[test]
    fn elevator_wall_is_flat_and_clear_in_front() {
        let t = tris();
        let mid = Vector3::new(0.0, 1.2, 0.0);
        let back = -ELEVATOR_FACING;
        let right = ELEVATOR_FACING.cross(Vector3::new(0.0, 1.0, 0.0));
        for i in 0..=18 {
            let off = right * (-2.25 + 0.25 * i as f32);
            let from = ELEVATOR_SPOT + off + mid + ELEVATOR_FACING * 0.1;
            let d = nearest_hit(&t, from, back).expect("a wall behind");
            assert!((d - 0.1 - PROUD).abs() < 0.02, "wall {d} behind at offset {off:?}");
            // ...and nothing in front for the stride out and two more.
            let ahead = nearest_hit(&t, from, ELEVATOR_FACING).unwrap_or(f32::MAX);
            assert!(ahead > SPAWN_AHEAD + 2.0, "{ahead} m clear at offset {off:?}");
        }
        // Straight ahead: the central block's wall, 3.9 m off the wall (module docs).
        let ahead = nearest_hit(&t, ELEVATOR_SPOT + mid, ELEVATOR_FACING).expect("the block");
        assert!((ahead + PROUD - 3.9).abs() < 0.15, "{ahead} m to the first wall");
        // Nothing of the doors: they are 5.6 m and more to the left.
        let door = placement().local_to_world().mul_point(Vector3::new(-7.1, 1.2, WALL_Z));
        assert!((door - ELEVATOR_SPOT).mag() > 5.5);
    }

    /// The bushes are the reason the override exists: the file says metal.
    #[test]
    fn foliage_is_overridden_to_dielectric() {
        let spec = load_spec();
        let metal = |name: &str| crate::ext::gltf_model::GltfModel::probe_metallic(&spec, name);
        for name in ["Bush_Texture_1", "Bush_Texture_4", "Grass_Realistic_1", "Grass_Realistic_5"] {
            assert_eq!(metal(name), Some(0.0), "{name}");
        }
        assert_eq!(metal("Thick_Moss"), Some(0.0));
        for name in ["Backrooms_Wallpaper", "Rusted_Metal", "Emission_EXIT", "Lights_Emission"] {
            assert_eq!(metal(name), None, "{name} keeps the file's factor");
        }
    }

    #[test]
    fn arrival_is_inside_the_fence() {
        let spec = load_spec();
        let b = crate::ext::gltf_model::GltfModel::probe_bounds(&spec, PART);
        let (lo, hi) = interior::world_bounds(b, &placement());
        let p = arrival();
        for (v, l, h) in [(p.x, lo.x, hi.x), (p.y, lo.y, hi.y), (p.z, lo.z, hi.z)] {
            assert!(v > l - FENCE_MARGIN && v < h + FENCE_MARGIN, "{p:?} outside {lo:?}..{hi:?}");
        }
        // The doorway is on the model's south face, the fence a margin past the foliage
        // overhang beyond it.
        assert!(hi.z > ELEVATOR_SPOT.z && hi.z < ELEVATOR_SPOT.z + FENCE_MARGIN);
    }

    #[test]
    fn registered_on_period() {
        let e = crate::ext::scenes::SCENES.iter().find(|e| e.name == "Overgrown").expect("listed");
        assert_eq!(e.key, b'.');
    }
}
