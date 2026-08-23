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
use crate::texture::Texture;
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

    /// World height of the carpet: the model's floor level, where the door's foot lands.
    pub fn carpet_y(&self) -> f32 {
        self.base.pos.y + FLOOR_Y
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

    fn draw(&self, ctx: &RenderCtx, cam: &Camera, _fbo: Option<glow::Framebuffer>) {
        self.model.draw_part(PART, &self.base, &self.shader, cam, ctx);
    }

    fn trimesh(&self) -> Option<Rc<TriMeshCollider>> {
        Some(self.collider.clone())
    }
}

/// How far past the building's footprint the ground cap reaches. Its edge must lie beyond the
/// far plane (`GH_FAR` = 100) from anywhere the player can stand, or the sky would show as a
/// bright line along it; the fence keeps them within `FENCE_MARGIN` of the footprint.
const CAP_MARGIN: f32 = 120.0;
/// How far below the carpet the cap sits, so it never z-fights the floor quads yet reads as
/// the floor continuing through any gap in an outer wall.
const CAP_DROP: f32 = 0.05;
/// The cap's colour: near-black, a touch warm, like the walls' own fog tone gone dark.
const CAP_COLOR: [f32; 4] = [0.030, 0.024, 0.018, 1.0];

/// A dark, unlit plane under the whole building and far past it.
///
/// The scan's walls are single-sided and not every outer doorway leads anywhere, so from some
/// spots the player can see out of the model -- and without this they saw the meadow's sky
/// under the horizon as well as above it. The sky above is dealt with by the interior mood
/// (`view::MOOD_INTERIOR`); this is the ground below, in the same dark. Purely cosmetic: it
/// uses `double_quad.obj`, which carries no collider (the level's invisible net is what
/// catches a player who falls out), and is visible from either side because that quad has
/// both windings.
///
/// Drawn with the unlit glTF shader rather than `Object::draw_impl` because that shader wants
/// `model`, `base_color` and `emissive` set, which `draw_impl` does not do; the white texture
/// turns its base map term into the flat colour, and its squared-distance fog then fades the
/// cap toward the same brown the walls fade to.
pub struct GroundCap {
    base: Object,
    shader: Rc<Shader>,
    white: Rc<Texture>,
}

impl GroundCap {
    pub fn new(res: &Resources, rooms: &Backrooms) -> GroundCap {
        let (lo, hi) = rooms.world_bounds();
        let mut base = Object::new();
        base.mesh = Some(res.acquire_mesh("double_quad.obj"));
        base.pos = Vector3::new(0.5 * (lo.x + hi.x), rooms.carpet_y() - CAP_DROP, 0.5 * (lo.z + hi.z));
        // The quad is in its own xy plane; a quarter turn about x lays it flat, and then its
        // local y is world z.
        base.euler.x = -std::f32::consts::FRAC_PI_2;
        base.scale = Vector3::new(
            0.5 * (hi.x - lo.x) + CAP_MARGIN,
            0.5 * (hi.z - lo.z) + CAP_MARGIN,
            1.0,
        );
        GroundCap {
            base,
            shader: res.acquire_shader("gltfunlit"),
            white: res.acquire_texture("white.bmp", 1, 1),
        }
    }
}

impl ObjectT for GroundCap {
    fn base(&self) -> &Object {
        &self.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base
    }

    fn draw(&self, ctx: &RenderCtx, cam: &Camera, _fbo: Option<glow::Framebuffer>) {
        // The same sphere cull draw_impl applies: from the meadow, a thousand units off, this
        // is behind the far plane and costs nothing.
        if let Some((c, r)) = crate::ext::cull::object_sphere(&self.base) {
            if !ctx.frustum.sphere(c, r) {
                return;
            }
        }
        let local_to_world = self.base.local_to_world();
        let mvp = cam.matrix() * local_to_world;
        let eye = ctx.eye;
        self.shader.use_program();
        self.shader.set_mvp(Some(&mvp), None);
        self.shader.set_mat4("model", &local_to_world);
        self.shader.set_vec4("cam_pos", [eye.x, eye.y, eye.z, 1.0]);
        self.shader.set_vec4("base_color", CAP_COLOR);
        self.shader.set_vec4("emissive", [0.0; 4]);
        self.shader.set_i32("tex", 0);
        self.white.use_texture();
        self.base.mesh.as_ref().expect("set in new").draw();
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
