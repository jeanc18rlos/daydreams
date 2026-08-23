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
//!
//! # Falling out
//!
//! The scan's walls are single-sided and its collision is "keep the sphere on the side its
//! centre is on" (`ext/trimesh.rs`). A sphere that starts a step inside a wall slab -- a seam,
//! a `--pos` into a wall, a corner two pushes could not agree on -- is pushed out the wrong
//! side, where there is no carpet, and falls. Nothing under the building would ever stop it,
//! so instead [`fell_out`] says when a player has gone under, and the level respawns them at
//! the arrival point (see `level16.rs`). A net under the carpet was the first answer, and it
//! left the player standing in the dark under the floor for ever.

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
/// The model's east end is an open hall some 23 m long -- x from about -16.5 to the end wall
/// at x = 6.78 -- before the maze of rooms begins to the west. Its wall FACES are at z = 3.48
/// and z = 7.07 (the slabs behind them are thicker than the coarse occupancy plot first
/// suggested, which put the south wall at z = 3), so the clear width is about 3.6 m centred on
/// z = 5.3. The door stands on the carpet near the end wall and faces it, so a player stepping
/// out of the door's far side is looking straight down the length of the hall. A little south
/// of the centre line -- 1.5 m from the south face, 2.1 from the north -- and 1.2 m clear of
/// the end wall: enough to walk round behind the door, not enough to waste the hall. Verified
/// against a screenshot from the spot; the tests below measure the faces from the model
/// itself, so a nudge can never put a frame post inside a wall unnoticed.
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

/// How far under the carpet a player may be before they count as having fallen out of the
/// building (see the module docs). Half a metre: more than any skirting or stray chair leg
/// dips below the carpet level, far less than a fall takes to become a problem.
pub const FALL_DEPTH: f32 = 0.5;

/// The one part this model is loaded as. A constant so the tests can probe the same geometry
/// the scene collides with, without a GL context.
const PARTS: [PartSpec<'static>; 1] = [PartSpec {
    name: PART,
    roots: &["Sketchfab_model"],
    pre: None,
    // Irrelevant under Fit::Identity, which is the point of it.
    anchor: Anchor::Hinge,
}];

fn load_spec() -> Load<'static> {
    Load { path: MODEL, parts: &PARTS, fit: Fit::Identity, max_map: MAP }
}

/// Whether a player whose position (eye height, as `Player` keeps it) is `pos` has fallen
/// out under the building: feet more than [`FALL_DEPTH`] below the carpet at `carpet_y`
/// while still within the fenced footprint `(lo, hi)` on x and z. Outside that footprint
/// nothing is under them either, but nothing of the building's is to blame and the scene's
/// fence is what keeps them from getting there.
pub fn fell_out(pos: Vector3, carpet_y: f32, (lo, hi): (Vector3, Vector3)) -> bool {
    let feet = pos.y - crate::game_header::GH_PLAYER_HEIGHT;
    feet < carpet_y - FALL_DEPTH
        && pos.x >= lo.x
        && pos.x <= hi.x
        && pos.z >= lo.z
        && pos.z <= hi.z
}

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
        let model = GltfModel::acquire(gl, &load_spec());

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
        // The shader's fog tone is the caller's (see Shaders/gltfunlit.frag); the model's is
        // set here, before `draw_part` sets everything else, because `GltfModel` has no idea
        // what colour a building's far end should be.
        self.shader.use_program();
        self.shader.set_vec4("fog_color", WALL_FOG);
        self.model.draw_part(PART, &self.base, &self.shader, cam, ctx);
    }

    fn trimesh(&self) -> Option<Rc<TriMeshCollider>> {
        Some(self.collider.clone())
    }
}

/// What the building's walls fade to with distance: a dark yellow-brown, the maps' own colour
/// gone dim, so the far end of the maze softens rather than popping at the far plane.
const WALL_FOG: [f32; 4] = [0.40, 0.33, 0.16, 1.0];

/// How far past the building's footprint the ground cap reaches. Its edge must lie beyond the
/// far plane (`GH_FAR` = 100) from anywhere the player can stand, or the sky would show as a
/// bright line along it; the fence keeps them within `FENCE_MARGIN` of the footprint.
const CAP_MARGIN: f32 = 120.0;
/// How far below the carpet the cap sits, so it never z-fights the floor quads yet reads as
/// the floor continuing through any gap in an outer wall.
const CAP_DROP: f32 = 0.05;
/// The cap's colour, and what it fogs to: the interior sky's horizon tone, byte for byte
/// (`sky.frag`, the `mood > 1.5` branch, at n.y = 0). The cap's far edge lies beyond the far
/// plane, so what the eye sees along the horizon is the cap at full fog meeting the sky at
/// the horizon -- and if those two differ at all, the seam is a bright line. With the walls'
/// brown as the fog target there was a 1-3 px band of it along the whole horizon.
const CAP_COLOR: [f32; 4] = [0.030, 0.024, 0.016, 1.0];

/// A dark, unlit plane under the whole building and far past it.
///
/// The scan's walls are single-sided and not every outer doorway leads anywhere, so from some
/// spots the player can see out of the model -- and without this they saw the meadow's sky
/// under the horizon as well as above it. The sky above is dealt with by the interior mood
/// (`view::MOOD_INTERIOR`); this is the ground below, in the same dark. Purely cosmetic: it
/// uses `double_quad.obj`, which carries no collider (a player who falls out of the building
/// is meant to keep falling until the level's `fell_out` rule puts them back), and is visible
/// from either side because that quad has both windings.
///
/// Drawn with the unlit glTF shader rather than `Object::draw_impl` because that shader wants
/// `model`, `base_color` and `emissive` set, which `draw_impl` does not do; the white texture
/// turns its base map term into the flat colour, and its squared-distance fog is pointed at
/// that same colour, so the cap is one flat dark from underfoot to the horizon.
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
        self.shader.set_vec4("fog_color", CAP_COLOR);
        self.shader.set_vec4("emissive", [0.0; 4]);
        self.shader.set_i32("tex", 0);
        self.white.use_texture();
        self.base.mesh.as_ref().expect("set in new").draw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ext::door::{HALF_W, POST_DEPTH};
    use crate::game_header::GH_PLAYER_HEIGHT;

    /// The faces of the hall around the door spot, in model coordinates, measured from the
    /// model's own triangles (no GL: `GltfModel::probe_triangles`).
    struct Hall {
        /// z of the south wall face, the nearest one below the spot.
        south: f32,
        /// z of the north wall face.
        north: f32,
        /// x of the end wall face the door faces.
        east: f32,
        /// y of the carpet under the spot.
        floor: f32,
    }

    fn measure_hall() -> Hall {
        let (pos, idx) = GltfModel::probe_triangles(&load_spec(), PART);
        let v = |i: u32| {
            let p = pos[i as usize];
            Vector3::new(p[0], p[1], p[2])
        };
        // Mid-wall height: a wall face has to span it, which rules out skirting, cornice
        // and the strips of ceiling slab that also face sideways.
        let mid_y = FLOOR_Y + 1.0;
        // A metre either side of the spot is the footprint the door and its swing occupy.
        let reach = 1.0;
        let (mut south, mut north, mut east, mut floor) = (f32::MIN, f32::MAX, f32::MAX, f32::MIN);
        for t in idx.chunks_exact(3) {
            let (a, b, c) = (v(t[0]), v(t[1]), v(t[2]));
            let n = (b - a).cross(c - a).normalized_safe();
            let lo = Vector3::new(a.x.min(b.x).min(c.x), a.y.min(b.y).min(c.y), a.z.min(b.z).min(c.z));
            let hi = Vector3::new(a.x.max(b.x).max(c.x), a.y.max(b.y).max(c.y), a.z.max(b.z).max(c.z));
            let centre = (a + b + c) / 3.0;
            let spans_mid = lo.y <= mid_y && hi.y >= mid_y;
            if n.z.abs() > 0.9 && spans_mid && lo.x <= DOOR_SPOT.x + reach && hi.x >= DOOR_SPOT.x - reach {
                if centre.z < DOOR_SPOT.z {
                    south = south.max(hi.z);
                } else {
                    north = north.min(lo.z);
                }
            }
            if n.x.abs() > 0.9 && spans_mid && lo.z <= DOOR_SPOT.z + reach && hi.z >= DOOR_SPOT.z - reach && centre.x > DOOR_SPOT.x {
                east = east.min(lo.x);
            }
            // The carpet: the highest upward face under the spot that is below head height.
            if n.y > 0.9
                && lo.x <= DOOR_SPOT.x
                && hi.x >= DOOR_SPOT.x
                && lo.z <= DOOR_SPOT.z
                && hi.z >= DOOR_SPOT.z
                && hi.y < mid_y
            {
                floor = floor.max(hi.y);
            }
        }
        Hall { south, north, east, floor }
    }

    /// The door spot has to sit on the carpet in the hall the module docs describe, with its
    /// frame posts clear of both walls by at least a player's width -- so a moved constant
    /// fails here rather than on a screenshot, and the numbers in the docs are the model's.
    #[test]
    fn door_spot_is_on_the_carpet_between_the_hall_walls() {
        let h = measure_hall();
        // The facts the docs state, as measured.
        assert!((h.south - 3.48).abs() < 0.01, "south face at z = {}", h.south);
        assert!((h.north - 7.07).abs() < 0.01, "north face at z = {}", h.north);
        assert!((h.east - 6.78).abs() < 0.01, "end wall at x = {}", h.east);
        assert!((h.floor - FLOOR_Y).abs() < 0.01, "carpet at y = {}", h.floor);

        // The frame's posts reach HALF_W + POST_DEPTH either side of the spot along z (the
        // door faces x), and a player must fit between a post and the wall to walk round.
        let post = HALF_W + POST_DEPTH;
        let clearance = 2.0 * crate::game_header::GH_PLAYER_RADIUS;
        assert!(
            DOOR_SPOT.z - post - clearance > h.south,
            "south post at z = {} against the wall face at {}",
            DOOR_SPOT.z - post,
            h.south
        );
        assert!(
            DOOR_SPOT.z + post + clearance < h.north,
            "north post at z = {} against the wall face at {}",
            DOOR_SPOT.z + post,
            h.north
        );
        // Room behind the door to walk round it, and for its leaf to swing.
        assert!(h.east - DOOR_SPOT.x > 1.0, "{} m from the end wall", h.east - DOOR_SPOT.x);
        assert_eq!(DOOR_SPOT.y, FLOOR_Y);
    }

    /// Facing must be horizontal and unit length: it becomes a door yaw via `yaw_facing`.
    #[test]
    fn door_facing_is_a_horizontal_unit_vector() {
        assert_eq!(DOOR_FACING.y, 0.0);
        assert!((DOOR_FACING.mag() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn fell_out_means_under_the_carpet_inside_the_footprint() {
        let bounds = (Vector3::new(-10.0, -2.0, -5.0), Vector3::new(10.0, 3.0, 5.0));
        let standing = Vector3::new(0.0, GH_PLAYER_HEIGHT, 0.0);
        assert!(!fell_out(standing, 0.0, bounds));
        // Dipped a little: a skirting, a step, not a fall.
        assert!(!fell_out(standing - Vector3::new(0.0, FALL_DEPTH * 0.9, 0.0), 0.0, bounds));
        // Past the depth: out.
        let under = standing - Vector3::new(0.0, FALL_DEPTH * 1.1, 0.0);
        assert!(fell_out(under, 0.0, bounds));
        // The same depth outside the footprint is not the building's problem.
        assert!(!fell_out(under + Vector3::new(11.0, 0.0, 0.0), 0.0, bounds));
        assert!(!fell_out(under + Vector3::new(0.0, 0.0, -6.0), 0.0, bounds));
        // Measured against the carpet wherever it is.
        assert!(!fell_out(under, -1.0, bounds));
    }
}
