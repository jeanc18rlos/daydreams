//! EXT: many named parts of one GLB, each at its own placement, as one scene object. Not
//! part of the C++ port.
//!
//! `GltfProp` (ext/gltf_prop.rs) draws a whole spec at ONE base transform -- right for a
//! level shell, wrong for a furnished house, where forty pieces of one furniture pack stand
//! at forty different spots. Forty `GltfProp`s would work but cost forty `Load`s and forty
//! parses of the same file (the acquire cache keys on the whole spec, gltf_model.rs); a
//! [`PackProps`] instead holds one shared [`GltfModel`] and a list of `(part, Object)`
//! instances, so the file is parsed once and each copy costs one `draw_part` call --
//! per-part sphere culling included.
//!
//! Collision is one merged [`TriMeshCollider`], built at construction from the solid
//! triangles of every instance marked collidable, each transformed by its own placement.
//! Small clutter (a plate, a book) is drawn but not collided; walls, floors and the big
//! furniture are. The mesh is fixed in world space -- a `PackProps` cannot move, which is
//! what a house is.
//!
//! # Placement helper
//!
//! The pack keeps its authored showroom layout (`Fit::Identity` + `Frame::Scene`), so a
//! part's geometry sits wherever the author parked it, not at the origin. [`PackProps::add`]
//! cancels that: it reads the part's fitted bounds and offsets the instance so the FOOT
//! CENTRE (x/z middle of the bounding box, at its bottom face) lands exactly on the `pos` it
//! was asked for. Rotation happens about that foot centre, not the pack origin.

use crate::camera::Camera;
use crate::ext::gltf_model::{GltfModel, Load, Pass};
use crate::ext::trimesh::TriMeshCollider;
use crate::object::{Object, ObjectT, RenderCtx};
use crate::resources::Resources;
use crate::shader::Shader;
use crate::vector::{Matrix4, Vector3};
use std::rc::Rc;

pub struct PackProps {
    model: Rc<GltfModel>,
    pbr: Rc<Shader>,
    unlit: Rc<Shader>,
    /// Part name, world placement, and whether its triangles joined the collider.
    items: Vec<(String, Object)>,
    /// One mesh for every collidable instance, points pre-transformed to world space.
    collider: Option<Rc<TriMeshCollider>>,
    tri_pos: Vec<[f32; 3]>,
    tri_idx: Vec<u32>,
    /// Whole-group bounding sphere: one test before the per-part culls.
    centre: Vector3,
    radius: f32,
    /// Skip drawing altogether beyond this distance from the eye: seventeen furnished
    /// houses line the loop and the fog has shut long before them. Frustum culling alone
    /// would still draw every lot down a street-length look.
    draw_range: f32,
    /// What the unlit shader fades its materials to (`gltfunlit.frag` wants it set every
    /// draw); black until the scene says otherwise, exactly as `GltfProp` defaults.
    fog_color: [f32; 4],
    base: Object,
}

impl PackProps {
    pub fn new(gl: &Rc<glow::Context>, res: &Resources, spec: &Load) -> PackProps {
        PackProps {
            model: GltfModel::acquire(gl, spec),
            pbr: res.acquire_shader("gltfpbr"),
            unlit: res.acquire_shader("gltfunlit"),
            items: Vec::new(),
            collider: None,
            tri_pos: Vec::new(),
            tri_idx: Vec::new(),
            centre: Vector3::zero(),
            radius: 0.0,
            draw_range: f32::INFINITY,
            fog_color: [0.0, 0.0, 0.0, 1.0],
            base: Object::new(),
        }
    }

    /// The part's model-space anchor: x/z centre of its fitted bounds, at their floor.
    pub fn anchor(&self, part: &str) -> Vector3 {
        let b = self.model.bounds(part);
        Vector3::new((b[0] + b[1]) * 0.5, b[2], (b[4] + b[5]) * 0.5)
    }

    /// Fitted model-space size of a part, `(dx, dy, dz)`.
    pub fn size(&self, part: &str) -> Vector3 {
        let b = self.model.bounds(part);
        Vector3::new(b[1] - b[0], b[3] - b[2], b[5] - b[4])
    }

    /// Stand `part` so its foot centre is at `pos`, turned `yaw` about that point. With
    /// `collide`, its solid triangles join the merged collider.
    pub fn add(&mut self, part: &str, pos: Vector3, yaw: f32, collide: bool) {
        self.add_lifted(part, pos, yaw, collide, 0.0);
    }

    /// [`PackProps::add`], with the foot floated `lift` above `pos` -- food on a table.
    pub fn add_lifted(&mut self, part: &str, pos: Vector3, yaw: f32, collide: bool, lift: f32) {
        let anchor = self.anchor(part);
        let mut obj = Object::new();
        obj.euler.y = yaw;
        obj.pos = pos + Vector3::new(0.0, lift, 0.0) - Matrix4::rot_y(yaw).mul_direction(anchor);
        if collide {
            debug_assert!(
                self.collider.is_none(),
                "PackProps::add({part}, collide) after seal(): the triangles would never collide"
            );
            let world = obj.local_to_world();
            let (p, i) = self.model.solid_triangles(part);
            let off = self.tri_pos.len() as u32;
            self.tri_pos.extend(p.iter().map(|v| {
                let w = world.mul_point(Vector3::new(v[0], v[1], v[2]));
                [w.x, w.y, w.z]
            }));
            self.tri_idx.extend(i.iter().map(|k| k + off));
        }
        // Track the group sphere as instances arrive, from each part's world-placed bounds.
        let b = self.model.bounds(part);
        for corner in 0..8 {
            let c = Vector3::new(
                b[(corner & 1) as usize],
                b[2 + ((corner >> 1) & 1) as usize],
                b[4 + ((corner >> 2) & 1) as usize],
            );
            let w = obj.local_to_world().mul_point(c);
            if self.items.is_empty() && corner == 0 {
                self.centre = w;
                self.radius = 0.0;
            } else {
                // Grow-to-fit: shift the centre toward the stray point.
                let d = (w - self.centre).mag();
                if d > self.radius {
                    let grow = (d - self.radius) * 0.5;
                    self.centre += (w - self.centre) * (grow / d);
                    self.radius += grow;
                }
            }
        }
        self.items.push((part.to_string(), obj));
    }

    pub fn set_draw_range(&mut self, range: f32) {
        self.draw_range = range;
    }

    /// For packs that carry unlit materials in a fogged scene (this game's two do not, but
    /// the drawer should not quietly render the next one black).
    #[allow(dead_code)]
    pub fn set_fog_color(&mut self, color: [f32; 4]) {
        self.fog_color = color;
    }

    /// Seal the collider once every instance is in. Idempotent; called by the scene after
    /// the last `add`.
    pub fn seal(&mut self) {
        if !self.tri_idx.is_empty() && self.collider.is_none() {
            self.collider = Some(Rc::new(TriMeshCollider::new(
                &self.tri_pos,
                &self.tri_idx,
                &Matrix4::identity(),
            )));
        }
    }
}

impl ObjectT for PackProps {
    fn base(&self) -> &Object {
        &self.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base
    }

    fn draw(&self, ctx: &RenderCtx, cam: &Camera, _fbo: Option<glow::Framebuffer>) {
        if (ctx.eye - self.centre).mag() > self.draw_range + self.radius {
            return;
        }
        if !ctx.frustum.sphere(self.centre, self.radius) {
            return;
        }
        for pass in [Pass::Opaque, Pass::Translucent] {
            for (part, obj) in &self.items {
                let shader = if self.model.unlit(part) {
                    // gltfunlit fades to fog_color and trusts the caller to set it, as
                    // GltfProp::draw does; scene fog does not reach unlit materials.
                    self.unlit.use_program();
                    self.unlit.set_vec4("fog_color", self.fog_color);
                    &self.unlit
                } else {
                    &self.pbr
                };
                self.model.draw_part_pass(part, obj, shader, cam, ctx, pass);
            }
        }
    }

    fn trimesh(&self) -> Option<Rc<TriMeshCollider>> {
        debug_assert!(
            self.collider.is_some() || self.tri_idx.is_empty(),
            "PackProps::seal() was never called: {} collidable triangles unbuilt",
            self.tri_idx.len() / 3
        );
        self.collider.clone()
    }

    /// The pieces are scenery: the collision pass reads the trimesh and the rigid-body
    /// snapshot takes it (`static_collision` stays true), but nothing pushes them.
    fn engine_collision(&self) -> bool {
        false
    }
}
