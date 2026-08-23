//! EXT: the Backrooms -- a scanned, light-baked interior as solid scenery. Not part of the
//! C++ port.
//!
//! `Meshes/backrooms_vr.glb` is a Sketchfab bake: 29 primitives, 70k triangles, 27 maps, every
//! material `KHR_materials_unlit` with its lighting painted in. It is already in metres and
//! already the right way up once the root node's Z-up -> Y-up rotation is applied, so it loads
//! under [`Fit::Identity`] and is placed by its `Object` like any prop. Two things make it more
//! than a prop:
//!
//! * **It is drawn unlit.** `Shaders/gltfunlit.*` puts the maps up as they are, adds the two
//!   emissive materials (red exit lamps, white diffusers) and a little depth fog, and ignores
//!   the weather grade that every other surface takes -- the backrooms is its own weather.
//! * **It collides as triangles.** There are no rectangle colliders to declare on 70k
//!   triangles of walls, skirting and armchairs, so the whole model becomes one
//!   `TriMeshCollider`, built once in world space at load. The engine consults it through
//!   `ObjectT::trimesh`. The object is therefore **static**: moving `base` after `new` would
//!   leave the collision behind.
//!
//! # Where the door goes
//!
//! A scene needs a spot of open carpet to stand the return door on, with the length of the
//! building in front of it. [`DOOR_SPOT`] is that spot in the model's own coordinates, and
//! [`Backrooms::new`] takes the WORLD point it should land on and places the model accordingly,
//! so a level reasons about the door and the placement follows.

use crate::camera::Camera;
use crate::ext::gltf_model::{Anchor, Fit, GltfModel, Load, PartSpec};
use crate::ext::trimesh::TriMeshCollider;
use crate::object::{Object, ObjectT, RenderCtx};
use crate::resources::Resources;
use crate::shader::Shader;
use crate::vector::Vector3;
use std::rc::Rc;

const MODEL: &str = "Meshes/backrooms_vr.glb";
/// The whole model, gathered from the root so its Z-up -> Y-up rotation is applied.
const PART: &str = "all";
/// The wall and carpet maps were baked at 1024 square; at this size nothing is resampled.
const MAP: u32 = 1024;

/// Carpet level in the model: the top of the `Moquette` floor quads.
pub const FLOOR_Y: f32 = -0.12;

/// Where the arrival door stands, in model coordinates, and the way it faces.
///
/// The model's east end is an open hall 23 m long and 4 m wide -- x in [-16.5, 6.8], z in
/// [3, 7] -- before the maze of rooms begins to the west. The door stands on the carpet near
/// the hall's east wall (which is at x = 6.78) and faces it, so a player stepping out of the
/// door's far side is looking straight down the length of the hall. Centred in the hall's
/// width, 1.2 m clear of the end wall: enough to walk round behind the door, not enough to
/// waste the hall. Verified against a screenshot from the spot, not only the occupancy plot.
pub const DOOR_SPOT: Vector3 = Vector3 {
    x: 5.6,
    y: FLOOR_Y,
    z: 5.0,
};
/// The door faces +x in model space (toward the east wall).
pub const DOOR_FACING: Vector3 = Vector3 {
    x: 1.0,
    y: 0.0,
    z: 0.0,
};

pub struct Backrooms {
    base: Object,
    model: Rc<GltfModel>,
    shader: Rc<Shader>,
    collider: Rc<TriMeshCollider>,
}

impl Backrooms {
    /// Load the model and place it so that [`DOOR_SPOT`] lands on `door_world`, unrotated --
    /// the model's axes stay the world's, so [`DOOR_FACING`] is the door's facing in world
    /// space too.
    pub fn new(gl: &Rc<glow::Context>, res: &Resources, door_world: Vector3) -> Backrooms {
        let model = GltfModel::acquire(
            gl,
            &Load {
                path: MODEL,
                parts: &[PartSpec {
                    name: PART,
                    roots: &["Sketchfab_model"],
                    pre: None,
                    // Irrelevant under Fit::Identity, which is the point of it.
                    anchor: Anchor::Hinge,
                }],
                fit: Fit::Identity,
                max_map: MAP,
            },
        );

        let mut base = Object::new();
        // No mesh: nothing to rasterise through draw_impl and no rectangle colliders. The
        // engine still collides with this object, through `trimesh()`.
        base.pos = door_world - DOOR_SPOT;

        let (pos, idx) = model.triangles(PART);
        let collider = Rc::new(TriMeshCollider::new(pos, idx, &base.local_to_world()));

        Backrooms {
            base,
            model,
            shader: res.acquire_shader("gltfunlit"),
            collider,
        }
    }

    /// World-space bounds of the placed model, `(min, max)`, for a scene to fence in.
    pub fn world_bounds(&self) -> (Vector3, Vector3) {
        let b = self.model.bounds(PART);
        let p = self.base.pos;
        (
            Vector3::new(b[0], b[2], b[4]) + p,
            Vector3::new(b[1], b[3], b[5]) + p,
        )
    }
}

impl ObjectT for Backrooms {
    fn base(&self) -> &Object {
        &self.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base
    }

    fn draw(&self, _ctx: &RenderCtx, cam: &Camera, _fbo: Option<glow::Framebuffer>) {
        self.model.draw_part(PART, &self.base, &self.shader, cam);
    }

    fn trimesh(&self) -> Option<Rc<TriMeshCollider>> {
        Some(self.collider.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The door spot has to be inside the hall the module docs describe, or the player arrives
    /// inside a wall. The hall's extent is a property of the asset, stated here so a moved
    /// constant fails loudly rather than on a screenshot.
    #[test]
    fn door_spot_is_in_the_east_hall() {
        assert!(
            DOOR_SPOT.x > -16.5 && DOOR_SPOT.x < 6.78 - 1.0,
            "x = {}",
            DOOR_SPOT.x
        );
        assert!(
            DOOR_SPOT.z > 3.0 + 0.5 && DOOR_SPOT.z < 7.0 - 0.5,
            "z = {}",
            DOOR_SPOT.z
        );
        assert_eq!(DOOR_SPOT.y, FLOOR_Y);
    }

    /// Facing must be horizontal and unit length: it becomes a door yaw via `yaw_facing`.
    #[test]
    fn door_facing_is_a_horizontal_unit_vector() {
        assert_eq!(DOOR_FACING.y, 0.0);
        assert!((DOOR_FACING.mag() - 1.0).abs() < 1e-6);
    }
}
