//! EXT: Scene `]` -- "Relativity". Not part of the C++ port.
//!
//! **"Escher Relativity" by Benoit Gagnier** (CC-BY-4.0, via Sketchfab; licence ships as
//! `Meshes/escher_relativity.LICENSE.txt`) -- as a *playable level*, not an exhibit. You spawn
//! on a landing inside the structure and walk its floors and staircases.
//!
//! # How a 660k-triangle sculpture becomes walkable
//!
//! The engine collides hit spheres against rectangle colliders at 500 Hz, so the raw triangles
//! can't be the collision. `tools/gen_walkable.py` extracts a shell instead: it keeps the
//! up-facing surfaces, rasterises them into a multi-layer heightfield (multi-layer because the
//! building has overlapping storeys), and emits ~530 gradient-tilted rectangle colliders baked
//! straight into the mesh file as `c` lines -- the engine's own dialect for exactly this.
//!
//! Two judgement calls worth knowing:
//!
//! - **Stairs are ramps.** The engine cannot climb steps -- the foot sphere hits each riser and
//!   stops, which is why the original demo only ever uses slopes (Level4) and never stairs. The
//!   shell blurs each stair run into an incline, and the model is auto-scaled so the implied
//!   riser (~0.17) reads correctly against the player's height.
//! - **One gravity of three.** Relativity's joke is three interleaved gravity systems. The
//!   player has one; the shell only covers surfaces that are "up" under it. The other two stair
//!   systems are genuinely present and genuinely unwalkable -- they are the walls and ceilings
//!   of your world, exactly as they are for two of the three populations in the print.
//!
//! Falling off lands you on the gallery floor at the structure's own ground level, so you can
//! walk back in rather than respawn.

use std::cell::RefCell;
use std::rc::Rc;

use crate::game_header::GH_PLAYER_HEIGHT;
use crate::object::Object;
use crate::player::Player;
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec, Scene};
use crate::vector::Vector3;

pub struct Level13;

/// World bbox of the rescaled model, from tools/gen_walkable.py:
///   x[-4.7, 4.7]  y[0.0, 9.8]  z[-5.1, 5.1]
#[allow(dead_code)] // read by the containment test only
const MODEL_HALF_XZ: f32 = 5.1;
#[allow(dead_code)] // read by the containment test only
const MODEL_TOP: f32 = 9.8;
/// Room shell half-extents: generous margin around the structure, headroom above it.
const ROOM_HALF: Vector3 = Vector3 { x: 12.0, y: 13.0, z: 12.0 };
/// Spawn: the flattest low landing the shell generator found (world coords).
const SPAWN: Vector3 = Vector3 { x: -0.63, y: 2.15 + GH_PLAYER_HEIGHT, z: -3.15 };

impl Scene for Level13 {
    fn load(
        &self,
        gl: &Rc<glow::Context>,
        res: &Resources,
        objs: &mut PObjectVec,
        _portals: &mut PPortalVec,
        player: &mut Player,
    ) {
        // ── Shell room: keeps the player in-bounds on all sides ───────────────────────────
        let mut room = Object::new();
        room.mesh = Some(res.acquire_mesh("room.obj"));
        room.shader = Some(res.acquire_shader("texture"));
        room.texture = Some(res.acquire_texture("checker_gray.bmp", 1, 1));
        room.scale = ROOM_HALF;
        objs.push(Rc::new(RefCell::new(room)));

        let mut floor = crate::props::ground(gl, res, false);
        floor.scale = Vector3::new(1.2, 1.0, 1.2);
        objs.push(Rc::new(RefCell::new(floor)));

        // ── The structure. Colliders ship inside the mesh file, so this one object is both
        //    the visual and the walkable geometry -- no tracking, no logic, just a building. ──
        let mut monument = Object::new();
        monument.mesh = Some(res.acquire_mesh("escher_relativity.obj"));
        monument.shader = Some(res.acquire_shader("texture"));
        // Untextured source model + 1x1 white texture = flat clay under the per-face lighting.
        monument.texture = Some(res.acquire_texture("white.bmp", 1, 1));
        objs.push(Rc::new(RefCell::new(monument)));

        player.base.set_position(SPAWN);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The collision shell must actually be inside the mesh file -- this is the regression
    /// guard against regenerating the model without its `c` lines.
    #[test]
    fn model_carries_a_collision_shell() {
        let text = std::fs::read_to_string("Meshes/escher_relativity.obj")
            .expect("Meshes/escher_relativity.obj missing");
        let parsed = crate::mesh::parse_obj(&text);
        assert!(
            parsed.colliders.len() > 300,
            "expected a walk shell (>300 colliders), found {}",
            parsed.colliders.len()
        );
    }

    /// Spawn and structure must fit inside the shell room.
    #[test]
    fn scene_fits_inside_the_room() {
        assert!(MODEL_HALF_XZ < ROOM_HALF.x && MODEL_HALF_XZ < ROOM_HALF.z);
        assert!(MODEL_TOP < ROOM_HALF.y);
        assert!(SPAWN.x.abs() < ROOM_HALF.x && SPAWN.z.abs() < ROOM_HALF.z);
        assert!(SPAWN.y > 0.0 && SPAWN.y < ROOM_HALF.y);
    }
}
