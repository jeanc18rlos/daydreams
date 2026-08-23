//! EXT: a runtime glTF/GLB loader. Not part of the C++ port.
//!
//! The ported `Mesh` reads the original's custom OBJ dialect and deliberately **discards vertex
//! normals**, recomputing flat per-face ones (Mesh.cpp:187). That is right for the demo's own
//! assets -- the faceted look is the original's -- and wrong for a scanned model, whose entire
//! appearance lives in data that dialect cannot carry: smooth normals, tangents, and a set of
//! material maps per primitive. Converting such a model to OBJ throws all of it away and leaves
//! a faceted white slab.
//!
//! So EXT geometry loads through here instead, straight from the GLB, keeping:
//!   * per-vertex **normals** (attribute 2) and **tangents** (attribute 3, `vec4`, w = handedness);
//!   * one material per primitive, with base colour, normal, occlusion and metallic-roughness
//!     maps -- so a brass handle is brass and the painted slab is paint, in one model.
//!
//! # How it coexists with the port
//!
//! Nothing here touches the ported renderer. A `GltfModel` owns its own VAOs and textures and is
//! drawn by an `ObjectT` implementation that calls [`GltfModel::draw_part`] in place of
//! `Object::draw_impl`. The matrices, the uniform set and the draw order are all copied from
//! `Object::draw_impl` (object.rs:79-101) so a glTF part shades exactly like everything else.
//!
//! Colliders are *not* handled here: glTF has no such concept, and the engine's collision loop
//! reads them off `base().mesh` (engine.rs:676-691). A scene that needs the player to bump into
//! glTF geometry keeps a small collider-only OBJ in `Object::mesh` and simply never draws it --
//! or, for scanned interiors, builds an `ext::trimesh::TriMeshCollider` from the fitted
//! triangles this loader keeps per part ([`GltfModel::triangles`]).
//!
//! # Two kinds of model
//!
//! The door is a lit PBR asset that has to be re-fitted to the game's door size; the backrooms
//! is a baked, unlit scan already in metres. The loader serves both through [`Fit`] (one
//! similarity from a named part, or the source placement as-is) and per-material `unlit`: an
//! unlit material keeps its base colour map as shipped and is drawn by a shader that adds
//! nothing to it, because its lighting is already painted in.

use crate::camera::Camera;
use crate::object::{Object, RenderCtx};
use crate::shader::Shader;
use crate::vector::Vector3;
use glow::HasContext;
use std::collections::HashMap;
use std::rc::Rc;

/// Where a part's origin goes once the model is fitted. Consulted only under [`Fit::Part`];
/// [`Fit::Identity`] keeps every part exactly where the file puts it.
///
/// Every part is fitted with the SAME scale and from the SAME origin -- the one belonging to the
/// hinge part -- so the model's true relative placement survives. Anchoring each part to its own
/// bounding box instead is the obvious thing to do and it silently throws that placement away:
/// on this door it lost a 9.7 mm sideways offset, a 37 mm door-to-rebate gap and a 3.6 mm floor
/// undercut, each of which then had to be guessed back as a hand-tuned constant.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Anchor<'a> {
    /// Origin at this part's low-x edge, its foot, and its z centre -- the hinge of a door leaf,
    /// which is the point `Object::euler` rotates about. Exactly one part uses this, and it
    /// defines the origin every other part is measured from.
    Hinge,
    /// Keep this part's true offset from the named hinge part, then shift x by that part's
    /// half-width. The effect is that the hinge part, hung at local `(-half_width, 0, 0)`, lands
    /// exactly where the model puts it -- so a door frame ends up with its opening around x=0
    /// and its leaf correctly seated in the rebate, both straight from the source.
    Around(&'a str),
}

/// One drawable group, named by the caller and gathered from one or more node sub-trees.
pub struct PartSpec<'a> {
    pub name: &'a str,
    /// Node names to walk from. Starting below the model's own root skips any transform that
    /// poses the part in the source scene -- for the door, the `Door` node's 55-degree rotation
    /// that models it hanging ajar.
    pub roots: &'a [&'a str],
    /// A node whose TRANSLATION is applied before walking, its rotation and scale ignored.
    ///
    /// This is how a part skips a pose rotation without also leaving its siblings' coordinate
    /// system. The door's leaf hangs under a node carrying both the ajar rotation and the
    /// leaf's position in the assembly; walking from below that node drops the rotation but
    /// also the position, which is what separated leaf and frame into unrelated spaces.
    pub pre: Option<&'a str>,
    pub anchor: Anchor<'a>,
}

/// How the gathered geometry is placed in the model's object space.
#[derive(Clone, Copy)]
pub enum Fit<'a> {
    /// ONE uniform scale and ONE origin for the whole model, both taken from `part`: scaled so
    /// that part is `height` tall, with its hinge anchor (see [`Anchor::Hinge`]) at the origin.
    /// A single similarity keeps the parts consistent with each other -- a door leaf must still
    /// fit its frame afterwards.
    Part { part: &'a str, height: f32 },
    /// Source units and source origin: only the node transforms are applied. For a model
    /// authored to scale in the engine's metres, which the scene then places through its
    /// `Object` like any other prop.
    Identity,
}

/// Everything `GltfModel::acquire` needs to know about a file.
pub struct Load<'a> {
    pub path: &'a str,
    pub parts: &'a [PartSpec<'a>],
    pub fit: Fit<'a>,
    /// Largest texture side kept, per load. Maps larger than this are downscaled to it; maps
    /// already at or under it are uploaded as decoded. The door asks for 512 (see `MAP_FILTER`
    /// for why its 4096 sources were an absurd budget -- the shipped GLB is pre-shrunk to that
    /// size by `tools/shrink_glb.py`, 79 MB -> 1.3 MB, so in practice nothing is resampled; keep
    /// `door::MAP` and the tool agreeing, or the loader quietly goes back to resizing at load
    /// time); the backrooms asks for 1024, the size its wall and carpet maps were baked at, so
    /// nothing there is resampled either.
    pub max_map: u32,
}

struct Prim {
    vao: glow::VertexArray,
    bufs: Vec<glow::Buffer>,
    count: i32,
    material: usize,
}

struct Material {
    /// PBR: RGB = base colour, A = ambient occlusion, packed at `max_map` square.
    /// Unlit: the base colour map exactly as shipped (or 1x1 white when there is none).
    albedo: glow::Texture,
    /// PBR only: RG = tangent-space normal xy, B = roughness, A = metalness.
    surface: Option<glow::Texture>,
    /// `KHR_materials_unlit`: the map already contains its lighting. Drawn with per-primitive
    /// `base_color` / `emissive` uniforms instead of the packed-surface PBR path.
    unlit: bool,
    /// Unlit only. Applied in the shader as uniforms, so a 1024-square map is never walked on
    /// the CPU to bake a factor in that the GPU multiplies for free.
    base_color: [f32; 4],
    /// Unlit only: `emissiveFactor` x `KHR_materials_emissive_strength`, left unclamped here
    /// and clamped in the shader -- there is no HDR target for a strength of 10 to mean
    /// anything more than "saturate".
    emissive: [f32; 3],
}

/// CPU copy of a part's fitted triangles, for building colliders from.
struct Geometry {
    pos: Vec<[f32; 3]>,
    idx: Vec<u32>,
}

pub struct GltfModel {
    gl: Rc<glow::Context>,
    materials: Vec<Material>,
    parts: HashMap<String, Vec<Prim>>,
    /// Fitted bounds per part, `[minx, maxx, miny, maxy, minz, maxz]`. Scenes assert against
    /// these rather than pasting numbers from a converter's printout.
    bounds: HashMap<String, [f32; 6]>,
    /// Fitted triangles per part, each triangle once (the double-sided duplicate windings that
    /// go to the GPU are not repeated here -- a collider wants each surface exactly once).
    geometry: HashMap<String, Geometry>,
}

// ── glTF matrices are column-major `[[f32; 4]; 4]`, indexed m[col][row]. The engine's own
// Matrix4 is row-major and unrelated; keep the two apart and only convert at the end.
type M = [[f32; 4]; 4];

const IDENT: M = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

fn mul(a: M, b: M) -> M {
    let mut o = [[0.0f32; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            o[c][r] = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    o
}

fn point(m: &M, p: [f32; 3]) -> [f32; 3] {
    let mut o = [0.0f32; 3];
    for (r, slot) in o.iter_mut().enumerate() {
        *slot = m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2] + m[3][r];
    }
    o
}

fn direction(m: &M, p: [f32; 3]) -> [f32; 3] {
    let mut o = [0.0f32; 3];
    for (r, slot) in o.iter_mut().enumerate() {
        *slot = m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2];
    }
    let len = (o[0] * o[0] + o[1] * o[1] + o[2] * o[2]).sqrt();
    if len > 1e-12 {
        for v in o.iter_mut() {
            *v /= len;
        }
    }
    o
}

/// CPU-side geometry, before fitting and upload.
struct Raw {
    pos: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    nrm: Vec<[f32; 3]>,
    tan: Vec<[f32; 4]>,
    idx: Vec<u32>,
    /// How many of `idx` are the source's own winding; the rest is the double-sided copy.
    front: usize,
    material: usize,
}

/// Resampling filter for the downscale to `max_map` (4096 -> 512 on the door).
///
/// Triangle, not Lanczos3. `image` scales a filter's kernel by the resampling ratio, so at 8:1
/// Lanczos3's support of 3 becomes a 24-pixel radius -- roughly 2,300 taps per output pixel,
/// which measured at ~145 ms per map and made resizing alone 870 ms of a 1.5 s door load.
/// Triangle's support of 1 gives an 8-pixel radius, which at this ratio is very nearly a box
/// average over the source footprint: the right answer for a pure downscale, and the difference
/// on a normal map read at a few hundred pixels is not visible.
const MAP_FILTER: image::imageops::FilterType = image::imageops::FilterType::Triangle;

thread_local! {
    /// Same acquire-or-load shape as `Resources` (resources.rs:41-56): the cache holds only a
    /// `Weak`, so a model lives exactly as long as the scene objects holding it. A level with
    /// two doors decodes the GLB's ten images once rather than twice.
    ///
    /// A thread-local rather than a field on `Resources`, because `Resources` is ported code
    /// with no extension point, and the engine is single-threaded throughout (see view.rs).
    static CACHE: std::cell::RefCell<HashMap<String, std::rc::Weak<GltfModel>>> =
        std::cell::RefCell::new(HashMap::new());
}

impl GltfModel {
    /// Load `spec.path` once per scene, sharing it with every later caller that asks for the
    /// same file. The rest of `spec` is only consulted on a cache miss.
    pub fn acquire(gl: &Rc<glow::Context>, spec: &Load) -> Rc<GltfModel> {
        if let Some(hit) = CACHE.with(|c| c.borrow().get(spec.path).and_then(|w| w.upgrade())) {
            return hit;
        }
        let m = Rc::new(GltfModel::load(gl, spec));
        CACHE.with(|c| c.borrow_mut().insert(spec.path.to_string(), Rc::downgrade(&m)));
        m
    }

    /// Load `spec.path`, gathering the requested parts and placing them per `spec.fit`.
    pub fn load(gl: &Rc<glow::Context>, spec: &Load) -> GltfModel {
        let Load { path, parts, fit, max_map } = *spec;
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("open {path}: {e}"));
        let gltf = gltf::Gltf::from_slice(&bytes).unwrap_or_else(|e| panic!("parse {path}: {e}"));
        let blob = gltf.blob.clone().unwrap_or_else(|| panic!("{path}: no BIN chunk"));
        let doc = gltf.document;

        // ── Walk each part's sub-trees into CPU buffers.
        let mut raw: HashMap<String, Vec<Raw>> = HashMap::new();
        for spec in parts {
            let mut out: Vec<Raw> = Vec::new();
            let find = |name: &str| {
                doc.nodes()
                    .find(|n| n.name() == Some(name))
                    .unwrap_or_else(|| panic!("{path}: no node named {name:?}"))
            };
            // Translation only: this brings the part back into its siblings' space without
            // re-applying the pose rotation that walking from below the node was meant to drop.
            let mut base = IDENT;
            if let Some(pre) = spec.pre {
                let t = find(pre).transform().decomposed().0;
                base[3] = [t[0], t[1], t[2], 1.0];
            }
            for root in spec.roots {
                walk(&find(root), base, &blob, &mut out);
            }
            assert!(!out.is_empty(), "{path}: part {:?} gathered no geometry", spec.name);
            raw.insert(spec.name.to_string(), out);
        }

        // ── Fit. Under `Fit::Part`: ONE scale and ONE origin for the whole model, both taken
        // from the hinge part. Every other part keeps its true offset from it, so the assembly
        // comes out of the file rather than being reassembled from constants. Under
        // `Fit::Identity` nothing moves, and the bounds simply report where the file put things.
        if let Fit::Part { part: scale_part, height: scale_height } = fit {
            let sb = bbox(&raw[scale_part]);
            let scale = scale_height / (sb[3] - sb[2]);
            // The hinge part's own anchor: low-x edge, foot, z centre.
            let origin = [sb[0], sb[2], 0.5 * (sb[4] + sb[5])];
            let half_w = 0.5 * (sb[1] - sb[0]) * scale;

            for spec in parts {
                let list = raw.get_mut(spec.name).expect("part present");
                // `Around` shifts x so the hinge part, hung by door.rs at local (-half_w, 0, 0),
                // lands back on the origin -- i.e. exactly where the model has it.
                let shift = match spec.anchor {
                    Anchor::Hinge => 0.0,
                    Anchor::Around(_) => -half_w,
                };
                for r in list.iter_mut() {
                    for p in r.pos.iter_mut() {
                        p[0] = (p[0] - origin[0]) * scale + shift;
                        p[1] = (p[1] - origin[1]) * scale;
                        p[2] = (p[2] - origin[2]) * scale;
                    }
                }
                // A uniform scale plus a translation leaves normals and tangents untouched,
                // which is the whole reason for insisting on one similarity for the model.
            }
            for spec in parts {
                if let Anchor::Around(h) = spec.anchor {
                    assert!(
                        raw.contains_key(h),
                        "{path}: part {:?} is anchored around {h:?}, which is not a declared part",
                        spec.name
                    );
                }
            }
        }
        let bounds: HashMap<String, [f32; 6]> =
            raw.iter().map(|(name, list)| (name.clone(), bbox(list))).collect();

        // ── Materials, then geometry. Every map is decoded first, all at once, so the packing
        // loops below are pure CPU work over images that are already in hand.
        let maps = decode_maps(&doc, &blob, max_map);
        let materials = (0..doc.materials().count())
            .map(|i| build_material(gl, &doc, &maps, i, max_map))
            .collect();

        let mut built: HashMap<String, Vec<Prim>> = HashMap::new();
        let mut geometry: HashMap<String, Geometry> = HashMap::new();
        for (name, list) in raw.iter() {
            built.insert(name.clone(), list.iter().map(|r| upload(gl, r)).collect());
            geometry.insert(name.clone(), gather(list));
        }

        GltfModel { gl: gl.clone(), materials, parts: built, bounds, geometry }
    }

    /// Fitted bounds of a part: `[minx, maxx, miny, maxy, minz, maxz]`.
    pub fn bounds(&self, part: &str) -> [f32; 6] {
        *self.bounds.get(part).unwrap_or_else(|| panic!("no part {part:?}"))
    }

    /// A part's fitted triangles in the model's object space, as `(positions, indices)` with
    /// three indices per triangle and each triangle listed once. What a collider is built from.
    pub fn triangles(&self, part: &str) -> (&[[f32; 3]], &[u32]) {
        let g = self.geometry.get(part).unwrap_or_else(|| panic!("no part {part:?}"));
        (&g.pos, &g.idx)
    }

    /// The part's bounding sphere in object space, `(centre, radius)`, from its fitted bounds.
    fn bounding_sphere(&self, part: &str) -> (Vector3, f32) {
        let b = self.bounds(part);
        let lo = Vector3::new(b[0], b[2], b[4]);
        let hi = Vector3::new(b[1], b[3], b[5]);
        ((lo + hi) * 0.5, (hi - lo).mag() * 0.5)
    }

    /// Draw one part positioned by `obj`, in the pass described by `ctx` (its frustum and eye,
    /// computed once per pass in `Engine::render`). This mirrors `Object::draw_impl`
    /// (object.rs:79-101) exactly -- same matrices, same EXT uniform set -- so a glTF part
    /// lights and grades like every other surface in the scene, then adds the two material
    /// samplers on top.
    ///
    /// Takes `&self`: `ObjectT::draw` is re-entrant through portal recursion (Portal::Draw
    /// re-enters Engine::Render), so a draw path must never mutate.
    ///
    /// Skipped outright when the part's bounding sphere lies wholly outside the pass frustum
    /// (`ext::cull`). The backrooms sits 1,000 units from the meadow and is drawn by the
    /// meadow's main pass too; without this its 70k triangles would be transformed and clipped
    /// in every pass that cannot see it.
    pub fn draw_part(&self, part: &str, obj: &Object, shader: &Shader, cam: &Camera, ctx: &RenderCtx) {
        let Some(prims) = self.parts.get(part) else { return };
        let local_to_world = obj.local_to_world();
        let (centre, radius) = self.bounding_sphere(part);
        let world_centre = local_to_world.mul_point(centre);
        // The largest axis stretch bounds how far any local point can move from the centre.
        let stretch = local_to_world
            .x_axis()
            .mag()
            .max(local_to_world.y_axis().mag())
            .max(local_to_world.z_axis().mag());
        if !ctx.frustum.sphere(world_centre, radius * stretch) {
            return;
        }
        let eye = ctx.eye;

        let mv = obj.world_to_local().transposed();
        let mvp = cam.matrix() * local_to_world;

        shader.use_program();
        shader.set_mvp(Some(&mvp), Some(&mv));
        // local_to_world as its own uniform: the unlit shader wants world positions for its
        // fog and has no reason to invert `mv` per vertex to get them.
        shader.set_mat4("model", &local_to_world);
        shader.set_f32("time", crate::ext::view::time());
        shader.set_vec4("cam_pos", [eye.x, eye.y, eye.z, 1.0]);
        shader.set_f32("mood", crate::ext::view::mood_for(eye));
        shader.set_vec4("glow", crate::ext::view::glow());
        shader.set_f32("detail", crate::ext::view::detail());
        shader.set_i32("tex", 0);
        shader.set_i32("tex2", 1);

        unsafe {
            for p in prims {
                let m = &self.materials[p.material];
                if let Some(surface) = m.surface {
                    self.gl.active_texture(glow::TEXTURE1);
                    self.gl.bind_texture(glow::TEXTURE_2D, Some(surface));
                    // Leave unit 0 active on the way out: Object::draw_impl binds its texture
                    // with no active_texture call of its own (object.rs:86-88), so any object
                    // drawn after this one would otherwise land its texture on unit 1 and
                    // sample black.
                    self.gl.active_texture(glow::TEXTURE0);
                }
                if m.unlit {
                    let [r, g, b, a] = m.base_color;
                    shader.set_vec4("base_color", [r, g, b, a]);
                    let [er, eg, eb] = m.emissive;
                    shader.set_vec4("emissive", [er, eg, eb, 0.0]);
                }
                self.gl.bind_texture(glow::TEXTURE_2D, Some(m.albedo));
                self.gl.bind_vertex_array(Some(p.vao));
                self.gl.draw_elements(glow::TRIANGLES, p.count, glow::UNSIGNED_INT, 0);
            }
            self.gl.bind_vertex_array(None);
        }
    }
}

impl Drop for GltfModel {
    fn drop(&mut self) {
        unsafe {
            for prims in self.parts.values() {
                for p in prims {
                    self.gl.delete_vertex_array(p.vao);
                    for b in &p.bufs {
                        self.gl.delete_buffer(*b);
                    }
                }
            }
            for m in &self.materials {
                self.gl.delete_texture(m.albedo);
                if let Some(surface) = m.surface {
                    self.gl.delete_texture(surface);
                }
            }
        }
    }
}

fn bbox(list: &[Raw]) -> [f32; 6] {
    let mut b = [f32::MAX, f32::MIN, f32::MAX, f32::MIN, f32::MAX, f32::MIN];
    for r in list {
        for p in &r.pos {
            for k in 0..3 {
                b[2 * k] = b[2 * k].min(p[k]);
                b[2 * k + 1] = b[2 * k + 1].max(p[k]);
            }
        }
    }
    b
}

fn walk(node: &gltf::Node, parent: M, blob: &[u8], out: &mut Vec<Raw>) {
    let world = mul(parent, node.transform().matrix());
    if let Some(mesh) = node.mesh() {
        for prim in mesh.primitives() {
            if prim.mode() != gltf::mesh::Mode::Triangles {
                continue;
            }
            let r = prim.reader(|b| if b.index() == 0 { Some(blob) } else { None });
            let Some(pos) = r.read_positions() else { continue };
            let pos: Vec<[f32; 3]> = pos.map(|p| point(&world, p)).collect();
            let n = pos.len();
            let nrm: Vec<[f32; 3]> = match r.read_normals() {
                Some(it) => it.map(|v| direction(&world, v)).collect(),
                None => vec![[0.0, 1.0, 0.0]; n],
            };
            // TANGENT is VEC4: xyz is the tangent, w the bitangent handedness. The transform
            // moves xyz; w is a sign and must be carried through untouched.
            let tan: Vec<[f32; 4]> = match r.read_tangents() {
                Some(it) => it
                    .map(|t| {
                        let d = direction(&world, [t[0], t[1], t[2]]);
                        [d[0], d[1], d[2], t[3]]
                    })
                    .collect(),
                None => vec![[1.0, 0.0, 0.0, 1.0]; n],
            };
            let uv: Vec<[f32; 2]> = match r.read_tex_coords(0) {
                Some(it) => it.into_f32().collect(),
                None => vec![[0.0, 0.0]; n],
            };
            let mut idx: Vec<u32> = match r.read_indices() {
                Some(it) => it.into_u32().collect(),
                None => (0..n as u32).collect(),
            };

            // glTF's `doubleSided` has no counterpart here: the engine enables CULL_FACE/BACK
            // once at init and a scene cannot turn it off (engine.rs:114-116). Left culled, any
            // surface whose winding faces away simply is not drawn -- which is why the door
            // frame's inner reveal was see-through, and why an open leaf showed nothing on its
            // back. The established workaround in this codebase is to emit BOTH windings and
            // flip the normal on gl_FrontFacing (Shaders/grassblade.frag:28-35); the door's
            // fragment shader does the flip, so the second winding is all that is missing.
            // It doubles this model to ~5,300 triangles, which is nothing.
            let front = idx.len();
            if prim.material().double_sided() {
                let back: Vec<u32> =
                    idx.chunks_exact(3).flat_map(|t| [t[2], t[1], t[0]]).collect();
                idx.extend(back);
            }
            out.push(Raw {
                pos,
                uv,
                nrm,
                tan,
                idx,
                front,
                material: prim.material().index().unwrap_or(0),
            });
        }
    }
    for c in node.children() {
        walk(&c, world, blob, out);
    }
}

/// Concatenate a part's primitives into one triangle list, each triangle once.
fn gather(list: &[Raw]) -> Geometry {
    let mut pos = Vec::new();
    let mut idx = Vec::new();
    for r in list {
        let base = pos.len() as u32;
        pos.extend_from_slice(&r.pos);
        idx.extend(r.idx[..r.front].iter().map(|i| i + base));
    }
    Geometry { pos, idx }
}

fn upload(gl: &Rc<glow::Context>, r: &Raw) -> Prim {
    unsafe {
        let vao = gl.create_vertex_array().expect("create_vertex_array");
        gl.bind_vertex_array(Some(vao));
        let mut bufs = Vec::new();

        // Locations 0..3 match the order `Shader` binds them in: it scrapes the vertex source
        // for "\nin " and assigns 0, 1, 2, ... in declaration order (shader.rs:88-96). The door
        // shader therefore declares in_pos, in_uv, in_normal, in_tangent in exactly this order.
        let mut attrib = |loc: u32, comps: i32, data: &[u8]| {
            let b = gl.create_buffer().expect("create_buffer");
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(b));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, data, glow::STATIC_DRAW);
            gl.enable_vertex_attrib_array(loc);
            gl.vertex_attrib_pointer_f32(loc, comps, glow::FLOAT, false, 0, 0);
            bufs.push(b);
        };
        attrib(0, 3, as_bytes(&r.pos));
        attrib(1, 2, as_bytes(&r.uv));
        attrib(2, 3, as_bytes(&r.nrm));
        attrib(3, 4, as_bytes(&r.tan));

        let ib = gl.create_buffer().expect("create_buffer");
        gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ib));
        gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, as_bytes(&r.idx), glow::STATIC_DRAW);
        bufs.push(ib);

        gl.bind_vertex_array(None);
        Prim { vao, bufs, count: r.idx.len() as i32, material: r.material }
    }
}

/// Reinterpret a slice of POD arrays as bytes for `buffer_data_u8_slice`, the same way
/// `mesh.rs:554` does for the ported meshes.
fn as_bytes<T>(v: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

/// Decode and downscale every image the model's materials reference, **in parallel**, keyed by
/// glTF image index.
///
/// PNG decode is what is left of this loader's cost once the resize is sensible: the door
/// carried six 4096-square maps totalling ~79 MB of PNG, and decoding them one after another
/// was half a second in which the game showed nothing. They are wholly independent -- each
/// reads its own slice of the shared, immutable BIN chunk and produces an owned image -- so
/// they scale almost perfectly across cores. (The shipped door is now pre-shrunk, see `Load::max_map`,
/// and decodes in a few milliseconds either way; the lanes cost nothing and keep a full-size
/// model loadable.)
///
/// This is the only threaded code in the engine, and it stays that way by construction:
/// `thread::scope` joins every worker before returning, nothing here touches GL (the uploads in
/// `build_material` happen afterwards, on the main thread), and no state outlives the scope.
///
/// Decoding by *image* rather than by material also de-duplicates: `SM_Door4_Parts` is worn by
/// three of the door's five primitives, and its four maps are now decoded once between them.
fn decode_maps(doc: &gltf::Document, blob: &[u8], max_map: u32) -> HashMap<usize, image::RgbaImage> {
    let mut wanted: Vec<usize> = doc
        .materials()
        .flat_map(|m| material_sources(&m))
        .flatten()
        .collect();
    wanted.sort_unstable();
    wanted.dedup();

    // Resolve each index to its slice of the blob up front: `gltf::Document` is not `Sync`, so
    // the workers must not touch it.
    let jobs: Vec<(usize, &[u8])> = wanted
        .iter()
        .filter_map(|&i| {
            let img = doc.images().nth(i)?;
            let gltf::image::Source::View { view, .. } = img.source() else { return None };
            let off = view.offset();
            Some((i, &blob[off..off + view.length()]))
        })
        .collect();

    // Biggest first. The lanes below pull from a shared queue, and starting the long jobs
    // early is what keeps the last lane from finishing alone: the door's maps range from 380 KB
    // to 20 MB, and a wave that happens to draw four small ones then a large one takes as long
    // as the large one all by itself.
    let mut jobs = jobs;
    jobs.sort_by_key(|(_, b)| std::cmp::Reverse(b.len()));

    // A bounded pool rather than one thread per image. A 4096-square source is ~67 MB of RGBA
    // while it is being resized, so unbounded fan-out buys its speed with a peak resident set
    // several hundred megabytes taller -- and with only six large maps in the file there is
    // little left to win past a handful of lanes.
    let next = std::sync::atomic::AtomicUsize::new(0);
    let done = std::sync::Mutex::new(Vec::new());
    let lanes = DECODE_LANES.min(jobs.len().max(1));
    std::thread::scope(|scope| {
        for _ in 0..lanes {
            scope.spawn(|| loop {
                let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(&(i, bytes)) = jobs.get(k) else { break };
                let img = decode_one(bytes, max_map);
                done.lock().expect("decode queue").push((i, img));
            });
        }
    });

    let mut out = HashMap::new();
    for (i, img) in done.into_inner().expect("decode queue") {
        if let Some(img) = img {
            out.insert(i, img);
        }
    }
    out
}

/// How many images may be decoding at once. See `decode_maps`.
///
/// Three, chosen by measurement rather than by core count -- the machine has ten performance
/// cores and using them all is the wrong trade. The door carries six large maps, so three lanes
/// and four both finish in two rounds and reach the first frame in the same 1.62 s; six finishes
/// in one round and saves 0.12 s, but holds twice as many 67 MB intermediates and pushes peak
/// resident memory 210 MB past where the old sequential loader sat. Raising this buys time with
/// memory, and at this asset's shape there is very little time left to buy.
const DECODE_LANES: usize = 3;

/// The four maps a glTF metallic-roughness material can carry, as image indices. An unlit
/// material reads only its base colour map: the other three describe a lighting response it
/// does not have, so they are never decoded for it.
fn material_sources(m: &gltf::Material) -> [Option<usize>; 4] {
    let pbr = m.pbr_metallic_roughness();
    let base = pbr.base_color_texture().map(|t| t.texture().source().index());
    if m.unlit() {
        return [base, None, None, None];
    }
    [
        base,
        pbr.metallic_roughness_texture().map(|t| t.texture().source().index()),
        m.normal_texture().map(|t| t.texture().source().index()),
        m.occlusion_texture().map(|t| t.texture().source().index()),
    ]
}

/// Decode one embedded image to RGBA, downscaled to `max_map` square if it is larger.
///
/// A source already at or under the cap is handed back as decoded: no resample, and so no
/// softening of maps that were baked at exactly the size they are shown. `into_rgba8` also
/// expands paletted PNGs (the backrooms ships one), so every map arrives as straight RGBA.
fn decode_one(bytes: &[u8], max_map: u32) -> Option<image::RgbaImage> {
    // `into_rgba8`, not `to_rgba8`: the latter copies the decoded image into a second buffer of
    // the same size, which at 4096 square is another 67 MB held for the length of the resize.
    let rgba = image::load_from_memory(bytes).ok()?.into_rgba8();
    // A map already at or under the cap -- every one in the shipped door, and all of the
    // backrooms' -- is used as decoded. Resampling at 1:1 would be a copy through the filter
    // that changes nothing but the time.
    if rgba.width() <= max_map && rgba.height() <= max_map {
        return Some(rgba);
    }
    Some(image::imageops::resize(&rgba, max_map, max_map, MAP_FILTER))
}

/// GL wrap mode for a glTF sampler's, so tiling carpet tiles and clamped decals stay as authored.
fn wrap_mode(w: gltf::texture::WrappingMode) -> u32 {
    use gltf::texture::WrappingMode;
    match w {
        WrappingMode::ClampToEdge => glow::CLAMP_TO_EDGE,
        WrappingMode::MirroredRepeat => glow::MIRRORED_REPEAT,
        WrappingMode::Repeat => glow::REPEAT,
    }
}

/// Build a material's textures: the unlit map as shipped, or a glTF metallic-roughness
/// material's four maps packed into TWO RGBA textures.
///
/// `Object` has a single texture slot and the ported `Texture` can only be built from a file on
/// disk, so a multi-map material needs its own upload path anyway. Packing is not just
/// convenience: base colour, occlusion, roughness and metalness are all single- or
/// three-channel data, and interleaving them costs two samplers instead of four with no loss.
fn build_material(
    gl: &Rc<glow::Context>,
    doc: &gltf::Document,
    maps: &HashMap<usize, image::RgbaImage>,
    i: usize,
    max_map: u32,
) -> Material {
    let m = doc.materials().nth(i).expect("material index in range");
    let pbr = m.pbr_metallic_roughness();

    let [base_src, mr_src, nrm_src, occ_src] = material_sources(&m);
    let get = |src: Option<usize>| src.and_then(|s| maps.get(&s));
    let (base, mr, nrm, occ) = (get(base_src), get(mr_src), get(nrm_src), get(occ_src));

    let bf = pbr.base_color_factor();

    if m.unlit() {
        // The map goes up untouched, at its own size and with its own wrap mode; the factor
        // and the emissive term are uniforms. No pack, no per-texel loop.
        let albedo = match (base, pbr.base_color_texture()) {
            (Some(img), Some(info)) => {
                let s = info.texture().sampler();
                let (ws, wt) = (wrap_mode(s.wrap_s()), wrap_mode(s.wrap_t()));
                tex2d(gl, img.as_raw(), img.width(), img.height(), ws, wt)
            }
            // No map: 1x1 white, so `texture(tex, uv) * base_color` is the factor alone.
            _ => tex2d(gl, &[255, 255, 255, 255], 1, 1, glow::REPEAT, glow::REPEAT),
        };
        let strength = m.emissive_strength().unwrap_or(1.0);
        let ef = m.emissive_factor();
        return Material {
            albedo,
            surface: None,
            unlit: true,
            base_color: bf,
            emissive: [ef[0] * strength, ef[1] * strength, ef[2] * strength],
        };
    }

    let metal_f = pbr.metallic_factor();
    let rough_f = pbr.roughness_factor();
    let occ_str = m.occlusion_texture().map(|t| t.strength()).unwrap_or(1.0);

    let map = max_map;
    let n = (map * map) as usize;
    let mut albedo = vec![0u8; n * 4];
    let mut surface = vec![0u8; n * 4];
    for k in 0..n {
        let (x, y) = ((k as u32) % map, (k as u32) / map);
        // Maps smaller than the pack square (a source already under `max_map`) are point-sampled
        // up to it; the door's are all downscaled to exactly `max_map`, where this is identity.
        let px = |img: Option<&image::RgbaImage>| {
            img.map(|im| *im.get_pixel(x * im.width() / map, y * im.height() / map))
        };

        // Base colour: the map where there is one, the factor otherwise. Both stay in raw byte
        // space -- the engine has no sRGB decode anywhere, so a gamma-correct upload here would
        // make the door the only surface in the game shaded in a different space.
        let b = px(base);
        for c in 0..3 {
            let t = b.map(|p| p[c] as f32 / 255.0).unwrap_or(1.0);
            albedo[k * 4 + c] = ((t * bf[c]).clamp(0.0, 1.0) * 255.0) as u8;
        }
        // glTF puts occlusion in R and applies `strength` as a lerp toward 1 (no occlusion).
        let o = px(occ).map(|p| p[0] as f32 / 255.0).unwrap_or(1.0);
        albedo[k * 4 + 3] = ((1.0 + occ_str * (o - 1.0)).clamp(0.0, 1.0) * 255.0) as u8;

        // Normal xy; z is reconstructed in the shader, so the blue channel is free.
        let nv = px(nrm);
        surface[k * 4] = nv.map(|p| p[0]).unwrap_or(128);
        surface[k * 4 + 1] = nv.map(|p| p[1]).unwrap_or(128);
        // glTF metallic-roughness: G = roughness, B = metalness, each scaled by its factor.
        let mrv = px(mr);
        let rough = mrv.map(|p| p[1] as f32 / 255.0).unwrap_or(1.0) * rough_f;
        let metal = mrv.map(|p| p[2] as f32 / 255.0).unwrap_or(1.0) * metal_f;
        surface[k * 4 + 2] = (rough.clamp(0.0, 1.0) * 255.0) as u8;
        surface[k * 4 + 3] = (metal.clamp(0.0, 1.0) * 255.0) as u8;
    }

    // Packed maps are addressed by the door's own UV islands, which never leave [0,1]: clamping
    // keeps a mip's edge from bleeding the opposite side of the atlas into the seam.
    Material {
        albedo: tex2d(gl, &albedo, map, map, glow::CLAMP_TO_EDGE, glow::CLAMP_TO_EDGE),
        surface: Some(tex2d(gl, &surface, map, map, glow::CLAMP_TO_EDGE, glow::CLAMP_TO_EDGE)),
        unlit: false,
        base_color: bf,
        emissive: [0.0; 3],
    }
}

fn tex2d(gl: &Rc<glow::Context>, rgba: &[u8], w: u32, h: u32, wrap_s: u32, wrap_t: u32) -> glow::Texture {
    unsafe {
        let t = gl.create_texture().expect("create_texture");
        gl.bind_texture(glow::TEXTURE_2D, Some(t));
        // LINEAR + mipmaps, unlike the ported 24-bit BMP path's NEAREST (texture.rs:167-177):
        // these are continuous material maps on curved joinery, and point sampling a normal map
        // produces exactly the stair-stepped facets this loader exists to avoid.
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR_MIPMAP_LINEAR as i32,
        );
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, wrap_s as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, wrap_t as i32);
        // RGBA, not the BMP path's BGRA: `image` decodes to RGBA byte order. Rows go up in
        // decode order (row 0 first), which is what puts glTF's v=0 -- the TOP of the image --
        // at GL t=0, matching this engine's convention (texture.rs:27-41). No flip, anywhere.
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            w as i32,
            h as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(rgba)),
        );
        gl.generate_mipmap(glow::TEXTURE_2D);
        gl.bind_texture(glow::TEXTURE_2D, None);
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Column-major multiply must compose the way glTF nests nodes: parent then child.
    #[test]
    fn matrices_compose_parent_then_child() {
        let mut t = IDENT;
        t[3] = [1.0, 2.0, 3.0, 1.0];
        let mut s = IDENT;
        s[0][0] = 2.0;
        s[1][1] = 2.0;
        s[2][2] = 2.0;
        // parent translate, child scale: the child's points scale first, then translate.
        let m = mul(t, s);
        assert_eq!(point(&m, [1.0, 1.0, 1.0]), [3.0, 4.0, 5.0]);
    }

    #[test]
    fn direction_ignores_translation_and_normalizes() {
        let mut t = IDENT;
        t[3] = [5.0, 5.0, 5.0, 1.0];
        let d = direction(&t, [0.0, 3.0, 0.0]);
        assert!((d[1] - 1.0).abs() < 1e-6, "got {d:?}");
    }
}
