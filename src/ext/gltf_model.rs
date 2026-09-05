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
//!   * one material per primitive, with base colour, normal, occlusion, metallic-roughness and
//!     emissive maps -- so a brass handle is brass and the painted slab is paint, in one model;
//!   * the file's **translation animations**, sampled by node name ([`Animation`]), so an
//!     asset's own door motion can drive a part drawn at rest.
//!
//! # What the file may contain
//!
//! GLB only, with every image embedded (PNG or JPEG); an image referenced by URI is an error
//! rather than a silently missing map. `KHR_materials_unlit`, `KHR_materials_emissive_strength`
//! and `KHR_texture_transform` are honoured; the texture transform is **baked into the UVs at
//! load** (see `walk`), so the shaders never see it -- and only the base colour texture's
//! transform is read, applied to every map of that material, since a primitive has one UV
//! stream here. Clearcoat, specular and the other material extensions are ignored, as is the
//! second UV set. A primitive that names no material gets the spec's default one (white,
//! opaque, fully metallic) rather than the file's first.
//!
//! # Alpha
//!
//! `alphaMode` becomes one of two things, chosen per material by [`Load::translucent`]:
//!
//! * an **alpha test** -- `MASK` at the file's cutoff, `BLEND` at 0.5 -- which discards the
//!   fragment in the shader and otherwise draws like an opaque one: depth-correct from every
//!   angle, crisp-edged, no sorting. Foliage cards want exactly this, whatever the exporter
//!   wrote, because sorted blending over a field of bushes is a mess of pop and halo;
//! * a **translucent pass** for the materials the caller names: drawn after every other
//!   primitive of the part, blended `SRC_ALPHA, ONE_MINUS_SRC_ALPHA`, depth-tested but not
//!   depth-writing, both faces. A real see-through surface -- a pool's water -- is the case.
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

use crate::app::assets;
use crate::app::crash::fatal;
use crate::app::error::AssetError;
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// The space a part's root nodes are walked in -- what, if anything, of the nodes ABOVE them is
/// applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frame<'a> {
    /// The roots' own transforms only. Starting below the model's own root skips any transform
    /// that poses the part in the source scene -- for the door, the `Door` node's 55-degree
    /// rotation that models it hanging ajar, and the exporter's pose of the whole model.
    Local,
    /// The named node's TRANSLATION, then the roots' own transforms; its rotation and scale
    /// are ignored.
    ///
    /// This is how a part skips a pose rotation without also leaving its siblings' coordinate
    /// system. The door's leaf hangs under a node carrying both the ajar rotation and the
    /// leaf's position in the assembly; walking from below that node drops the rotation but
    /// also the position, which is what separated leaf and frame into unrelated spaces.
    // EXT: unused since the intro door stopped needing it -- the door it was written for hung
    // its leaf under a node carrying a 55-degree "ajar" pose that had to be dropped while its
    // translation was kept. Left in place because that is a shape glTF exports fall into
    // routinely, and the next model posed in its source scene will want it back.
    #[allow(dead_code)]
    Translated(&'a str),
    /// Every ancestor's full transform, from the scene root down: the part lands exactly where
    /// the file puts it, in the same space as a part gathered from the scene root -- and in
    /// the space [`GltfModel::node_delta`] reports in. For a sub-assembly that is to be drawn
    /// separately but at rest in its model: the elevator's two door leaves, whose ancestors
    /// carry the exporter's centimetres-to-metres scale and its Z-up to Y-up rotation.
    Scene,
}

/// One drawable group, named by the caller and gathered from one or more node sub-trees.
///
/// Every material inside a part must agree on `unlit`: [`GltfModel::draw_part`] draws a whole
/// part with ONE shader, and the PBR and unlit shaders read different uniforms and samplers.
/// A part that mixed the two would have half its primitives shaded by the wrong program;
/// `load` asserts it never happens.
#[derive(Debug)]
pub struct PartSpec<'a> {
    pub name: &'a str,
    /// Node names to walk from, each in the space [`Frame`] says. **Empty means the whole
    /// file**: every root node of the default scene, which is how a level model is loaded as
    /// one part without knowing what its exporter called the root.
    pub roots: &'a [&'a str],
    /// Node names whose sub-trees are left out of the walk: what is going to move on its own
    /// -- the elevator's door leaves, gathered as parts of their own under [`Frame::Scene`]
    /// and drawn at `node_delta` -- must not also be baked into the body at rest.
    pub skip: &'a [&'a str],
    pub frame: Frame<'a>,
    pub anchor: Anchor<'a>,
}

/// The two halves of drawing a part (see [`GltfModel::draw_part`]): everything opaque or
/// alpha-tested, and then what blends over it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pass {
    Opaque,
    Translucent,
}

/// How the gathered geometry is placed in the model's object space.
#[derive(Clone, Copy, Debug)]
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

/// Everything `GltfModel::acquire` needs to know about a file. `Debug` is load-bearing: its
/// output is the cache key (see `acquire`). `Copy`, so a scene can take a level's constant
/// spec and add the boxes it carves out of it (`Load { cut_boxes: &cut, ..spec }`).
#[derive(Debug, Clone, Copy)]
pub struct Load<'a> {
    /// Relative to the asset root (`app::assets`): `"Meshes/door.glb"`.
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
    /// Material names drawn in the translucent pass (see the module docs): blended over
    /// everything else in their part, with the base colour map's alpha, whatever their
    /// `alphaMode` says. Every other material is alpha-tested or opaque. A name the file does
    /// not have is an error, so a typo cannot leave a pool without its water.
    pub translucent: &'a [&'a str],
    /// `(material name, metalness)` pairs that replace the named materials' `metallicFactor`.
    /// A name ending in `*` matches every material with that prefix (`"Bush_*"`). A pattern
    /// that matches nothing is an error, like a translucent name the file lacks.
    ///
    /// Why this exists: glTF's default `metallicFactor` is **1.0** -- a material that ships
    /// no factor and no metallic-roughness map is a fully metallic one, and a metal has no
    /// diffuse term, so a foliage card exported that way (the overgrown room's bushes, with
    /// nothing but a base colour map) renders as a dark, roughly-reflective cut-out of itself
    /// under any lighting model, this engine's included. The fix belongs to the asset, but
    /// the asset is someone else's: naming the materials here renders them as the dielectric
    /// (0.0) they were meant to be without re-exporting the file. Applied to PBR materials
    /// only; an unlit material has no metalness to override.
    pub metallic_override: &'a [(&'a str, f32)],
    /// Axis-aligned boxes, `(min, max)` in the MODEL's space -- the fitted space
    /// [`GltfModel::triangles`] reports in -- carved out of every part as it is parsed
    /// (`ext/carve.rs`): triangles inside a box are dropped, triangles crossing its faces are
    /// clipped so exactly the part outside survives, with its UVs, normals and tangents
    /// interpolated along the cut. What is drawn and what collides are the same triangles, so
    /// a wall is gone from both. This is how a thing set into a model's wall -- the elevator
    /// -- gets a real doorway through it rather than a wall 3 cm behind its leaves. A scene
    /// converts the world-space box it wants gone into the model's space first
    /// (`bounds::model_box`).
    pub cut_boxes: &'a [(Vector3, Vector3)],
}

struct Prim {
    vao: glow::VertexArray,
    bufs: Vec<glow::Buffer>,
    /// Every index: the source winding plus, for a double-sided material, its reverse.
    count: i32,
    /// The source winding alone. The translucent pass draws only this, with face culling off
    /// -- blending both windings would lay the surface over itself.
    front: i32,
    material: usize,
}

/// What an unlit material carries besides its map: the factors the shader applies as
/// uniforms, so a 1024-square map is never walked on the CPU to bake in what the GPU
/// multiplies for free. A PBR material bakes its factors into the packed maps instead
/// (`build_material`) and has none of this.
struct Unlit {
    base_color: [f32; 4],
    /// `emissiveFactor` x `KHR_materials_emissive_strength`, left unclamped here and clamped
    /// in the shader -- there is no HDR target for a strength of 10 to mean anything more
    /// than "saturate".
    emissive: [f32; 3],
}

/// One glTF material's GPU state. A part's materials are all PBR or all unlit -- see
/// [`PartSpec`] -- because the part is drawn with one shader, and `Shaders/gltfpbr.*` wants
/// `surface` and `light` on units 1 and 2 where `Shaders/gltfunlit.*` wants the `Unlit`
/// uniforms instead.
///
/// # The PBR packing
///
/// Five single- or three-channel maps interleaved into three RGBA textures, each packed at the
/// size of the largest source map feeding it (1x1 when none does -- a factor-only material
/// costs four texels, not a megabyte of constant):
///
/// | texture   | unit | RGB                                                  | A |
/// |-----------|------|------------------------------------------------------|---|
/// | `albedo`  | 0    | base colour x `baseColorFactor`                      | base alpha x factor, or 1 for an `OPAQUE` material |
/// | `surface` | 1    | RG = tangent-space normal xy, B = roughness x factor | metalness x factor |
/// | `light`   | 2    | emissive map x `emissiveFactor` x strength, clamped  | occlusion, `strength` applied |
///
/// Occlusion used to ride in `albedo.A`; it moved when the base colour's alpha had to reach
/// the shader for the alpha test and the translucent pass.
///
/// # The metalness default
///
/// A glTF material that writes no `metallicFactor` is **metallic** -- the spec's default is
/// 1.0, and the default `roughnessFactor` is 1.0 too, so an exporter that only wrote a base
/// colour map has described a rough metal. That is what `surface.A` packs, and a metal has no
/// diffuse term in any shader that takes the word seriously, so such a material renders dark
/// and faintly reflective: the overgrown room's `Bush_Texture_1` and `_2` ship exactly this
/// way. The `image` of the leaf is fine; the lighting response is wrong in the file.
/// [`Load::metallic_override`] names the materials to render as dielectrics instead.
struct Material {
    /// PBR: packed as above. Unlit: the base colour map exactly as shipped (or 1x1 white when
    /// there is none).
    albedo: glow::Texture,
    /// PBR only.
    surface: Option<glow::Texture>,
    /// PBR only.
    light: Option<glow::Texture>,
    /// `KHR_materials_unlit`: the map already contains its lighting, and these are the
    /// per-primitive uniforms drawn with it instead of the packed-surface PBR path.
    unlit: Option<Unlit>,
    /// Fragments whose alpha is below this are discarded; -1 turns the test off.
    alpha_cutoff: f32,
    /// Drawn in the part's translucent pass (see the module docs).
    translucent: bool,
}

/// CPU copy of a part's fitted triangles, for building colliders from.
struct Geometry {
    pos: Vec<[f32; 3]>,
    /// Every triangle, three indices each.
    idx: Vec<u32>,
    /// The subset of `idx` belonging to opaque materials -- see [`GltfModel::solid_triangles`].
    solid: Vec<u32>,
}

/// One translation channel of an [`Animation`], keyed by the node it moves.
struct Channel {
    node: String,
    interpolation: Interp,
    /// Key times, ascending, and one value per key.
    times: Vec<f32>,
    values: Vec<[f32; 3]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Interp {
    Linear,
    Step,
}

impl Channel {
    /// The channel's value at `t`, held at its first and last keys beyond them. A `t` that
    /// is not a number is the first key: it compares false against everything, and the
    /// partition below would otherwise hand back index 0 and wrap on the `- 1`.
    fn sample(&self, t: f32) -> [f32; 3] {
        let last = self.times.len() - 1;
        if t.is_nan() || t <= self.times[0] {
            return self.values[0];
        }
        if t >= self.times[last] {
            return self.values[last];
        }
        // The key at or before `t`; `t` is strictly inside the range, so `1 <= i < last`.
        let i = self.times.partition_point(|&k| k <= t) - 1;
        match self.interpolation {
            Interp::Step => self.values[i],
            Interp::Linear => {
                let (a, b) = (self.values[i], self.values[i + 1]);
                let u = (t - self.times[i]) / (self.times[i + 1] - self.times[i]);
                [a[0] + (b[0] - a[0]) * u, a[1] + (b[1] - a[1]) * u, a[2] + (b[2] - a[2]) * u]
            }
        }
    }
}

/// One of the file's animation clips: its translation channels, by node name.
///
/// Only translation is kept. `LINEAR` and `STEP` samplers are honoured as written; a
/// `CUBICSPLINE` sampler is reduced to its key values and played linearly (its tangents are
/// dropped), which passes through every key exactly and is more than a sliding door needs.
/// Rotation, scale and morph-weight channels are not read.
pub struct Animation {
    name: String,
    /// The last key time over the kept channels: the clip's length in seconds.
    duration: f32,
    channels: Vec<Channel>,
}

impl Animation {
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The clip's length in seconds: the last key time over the translation channels that
    /// were kept (see the type docs). A rotation or scale channel that ran on longer in the
    /// file does not count, because it is not read; the first key is not subtracted, so a
    /// clip whose keys start late is as long as its last key says.
    pub fn duration(&self) -> f32 {
        self.duration
    }

    /// The nodes this clip moves, in channel order.
    pub fn nodes(&self) -> impl Iterator<Item = &str> {
        self.channels.iter().map(|c| c.node.as_str())
    }

    /// The node's animated translation at `t` seconds, in the node's own local space (the
    /// units and axes of its parent -- what the file's `translation` property is in). `t` is
    /// clamped to `[0, duration]`; a `t` that is not a number (a clip time poisoned by a
    /// zero-length division somewhere upstream) reads as 0, the rest pose, rather than
    /// indexing the keys with it. `None` when the clip has no translation channel for `node`.
    pub fn translation(&self, node: &str, t: f32) -> Option<[f32; 3]> {
        let ch = self.channels.iter().find(|c| c.node == node)?;
        let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, self.duration) };
        Some(ch.sample(t))
    }
}

/// Where a named node sits at rest: what [`Rig::node_delta`] needs to turn a local animated
/// translation into a displacement in the fitted model.
struct NodeRest {
    /// The node's own `translation`, in its parent's space.
    translation: [f32; 3],
    /// The parent's world matrix under the default scene (identity for a root), whose linear
    /// part carries a local displacement into the scene's space.
    parent: M,
}

/// The GL-free half of a model's motion: its clips, its nodes' rest poses and the fit scale.
/// Split from `GltfModel` so a test can sample a file's animation without a context.
struct Rig {
    animations: Vec<Animation>,
    nodes: HashMap<String, NodeRest>,
    /// What the fit multiplied lengths by: `Fit::Part`'s scale, or 1.
    fit_scale: f32,
}

impl Rig {
    fn animation(&self, name: &str) -> Option<&Animation> {
        self.animations.iter().find(|a| a.name == name)
    }

    /// See [`GltfModel::node_delta`].
    fn node_delta(&self, anim: &Animation, node: &str, t: f32) -> Option<Vector3> {
        let now = anim.translation(node, t)?;
        let rest = self.nodes.get(node)?;
        let r = rest.translation;
        // The translation lives in the parent's frame (world = parent x T x R x S), so moving
        // it by `d` moves every point of the sub-tree by the parent's linear part of `d`; the
        // node's own rotation and scale never touch it.
        let d = linear(&rest.parent, [now[0] - r[0], now[1] - r[1], now[2] - r[2]]);
        Some(Vector3::new(d[0], d[1], d[2]) * self.fit_scale)
    }
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
    rig: Rig,
}

// ── glTF matrices are column-major `[[f32; 4]; 4]`, indexed m[col][row]. The engine's own
// Matrix4 is row-major and unrelated; keep the two apart and only convert at the end.
type M = [[f32; 4]; 4];

const IDENT: M =
    [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

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

/// The linear part alone: a displacement, which translation does not move and whose length
/// the matrix's scale is meant to change.
fn linear(m: &M, v: [f32; 3]) -> [f32; 3] {
    let mut o = [0.0f32; 3];
    for (r, slot) in o.iter_mut().enumerate() {
        *slot = m[0][r] * v[0] + m[1][r] * v[1] + m[2][r] * v[2];
    }
    o
}

/// A unit direction through the linear part: a normal or tangent.
fn direction(m: &M, p: [f32; 3]) -> [f32; 3] {
    let mut o = linear(m, p);
    let len = (o[0] * o[0] + o[1] * o[1] + o[2] * o[2]).sqrt();
    if len > 1e-12 {
        for v in o.iter_mut() {
            *v /= len;
        }
    }
    o
}

/// `KHR_texture_transform` on a base colour texture, as the spec gives it: `uv' = offset +
/// R(rotation) * (scale * uv)`, rotation in radians, R's first column `(cos, sin)`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct UvTransform {
    offset: [f32; 2],
    rotation: f32,
    scale: [f32; 2],
}

impl UvTransform {
    fn apply(&self, uv: [f32; 2]) -> [f32; 2] {
        let (s, c) = self.rotation.sin_cos();
        let (u, v) = (uv[0] * self.scale[0], uv[1] * self.scale[1]);
        [c * u - s * v + self.offset[0], s * u + c * v + self.offset[1]]
    }
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
    /// Keyed on the WHOLE `Load` (its `Debug` rendering), not the path: the parts gathered,
    /// the fit and the map cap all shape what is built, so a second scene asking for the same
    /// file under another fit must get its own model rather than silently the first one's.
    ///
    /// A thread-local rather than a field on `Resources`, because `Resources` is ported code
    /// with no extension point, and the engine is single-threaded throughout (see view.rs).
    static CACHE: std::cell::RefCell<HashMap<String, std::rc::Weak<GltfModel>>> =
        std::cell::RefCell::new(HashMap::new());
}

/// What the alpha policy decided for one material (see the module docs), plus the one fact
/// about its geometry the texture upload needs.
struct MaterialPolicy {
    /// Shader alpha test threshold; -1 is off.
    alpha_cutoff: f32,
    translucent: bool,
    /// Whether the packed albedo keeps the base colour's alpha (a masked or translucent
    /// material) or forces it opaque, as glTF says an `OPAQUE` material's alpha is to be read.
    keeps_alpha: bool,
    /// Some primitive using this material has UVs outside [0, 1] -- it tiles its maps (the
    /// pool's marble, scaled x5 by a texture transform), so the packed textures must take the
    /// sampler's wrap mode rather than the clamp an atlas-addressed material gets.
    tiles: bool,
    /// `Load::metallic_override` for this material, if a pattern named it: what the packed
    /// surface map's metalness is scaled by in place of the file's `metallicFactor`.
    metallic: Option<f32>,
}

/// The GL-free half of a load: the file parsed, its parts gathered and fitted. Split out so
/// that a test can measure a model's geometry without a context (see `probe_triangles`).
struct Parsed {
    doc: gltf::Document,
    blob: Vec<u8>,
    raw: HashMap<String, Vec<Raw>>,
    bounds: HashMap<String, [f32; 6]>,
    policies: Vec<MaterialPolicy>,
    rig: Rig,
}

impl GltfModel {
    /// Load `spec` once per scene, sharing it with every later caller that asks for the same
    /// spec. Two specs are the same when their `Debug` renderings are (see `CACHE`).
    pub fn acquire(gl: &Rc<glow::Context>, spec: &Load) -> Rc<GltfModel> {
        let key = format!("{spec:?}");
        if let Some(hit) = CACHE.with(|c| c.borrow().get(&key).and_then(|w| w.upgrade())) {
            return hit;
        }
        // Infallible like `Resources::acquire_*`, and for the same reason: the scenes that
        // place a model have no `Result` to carry a failure, and a model that is asked for
        // and not there is a packaging bug.
        let m = Rc::new(GltfModel::load(gl, spec).unwrap_or_else(|e| fatal(&e)));
        CACHE.with(|c| c.borrow_mut().insert(key, Rc::downgrade(&m)));
        m
    }

    /// Load `spec.path`, gathering the requested parts and placing them per `spec.fit`.
    pub fn load(gl: &Rc<glow::Context>, spec: &Load) -> Result<GltfModel, AssetError> {
        let Parsed { doc, blob, raw, bounds, policies, rig } = parse(spec)?;
        let Load { path, parts, max_map, .. } = *spec;
        let bad = |reason: String| AssetError::Gltf { path: assets::path(path), reason };

        // One shader per part (see `PartSpec`): every material a part uses must be the same
        // kind. Checked here, on the materials' own flags, so a re-exported model that mixed
        // them fails at load rather than drawing half of itself with the wrong program.
        for spec in parts {
            let mut kinds = raw[spec.name]
                .iter()
                .map(|r| material(&doc, r.material).is_some_and(|m| m.unlit()));
            let first = kinds.next().expect("part gathered geometry");
            if !kinds.all(|k| k == first) {
                return Err(bad(format!(
                    "part {:?} mixes unlit and PBR materials, and is drawn with one shader",
                    spec.name
                )));
            }
        }

        // ── Materials, then geometry. Every map is decoded first, all at once, so the packing
        // loops below are pure CPU work over images that are already in hand.
        let maps = decode_maps(&doc, &blob, max_map).map_err(bad)?;
        let materials = policies
            .iter()
            .enumerate()
            .map(|(i, policy)| build_material(gl, &doc, &maps, i, policy))
            .collect::<Result<Vec<_>, _>>()?;

        let mut built: HashMap<String, Vec<Prim>> = HashMap::new();
        let mut geometry: HashMap<String, Geometry> = HashMap::new();
        for (name, list) in raw.iter() {
            built.insert(
                name.clone(),
                list.iter().map(|r| upload(gl, r)).collect::<Result<_, _>>()?,
            );
            geometry.insert(name.clone(), gather(list, &policies));
        }

        Ok(GltfModel { gl: gl.clone(), materials, parts: built, bounds, geometry, rig })
    }

    /// A part's fitted triangles straight from the file, with no GL context: what a test uses
    /// to measure a model against the constants a scene places things by. Same gather and fit
    /// as `load`, so the answer is the one `triangles` would give.
    #[cfg(test)]
    pub fn probe_triangles(spec: &Load, part: &str) -> (Vec<[f32; 3]>, Vec<u32>) {
        let parsed = parse(spec).unwrap_or_else(|e| panic!("{e}"));
        let g = gather(&parsed.raw[part], &parsed.policies);
        (g.pos, g.idx)
    }

    /// A part's fitted bounds straight from the file, with no GL context (see `bounds`).
    #[cfg(test)]
    pub fn probe_bounds(spec: &Load, part: &str) -> [f32; 6] {
        parse(spec).unwrap_or_else(|e| panic!("{e}")).bounds[part]
    }

    /// What `spec.metallic_override` made of the material called `name`: the metalness it
    /// will be packed with, or `None` where the file's own factor stands.
    #[cfg(test)]
    pub fn probe_metallic(spec: &Load, name: &str) -> Option<f32> {
        let parsed = parse(spec).unwrap_or_else(|e| panic!("{e}"));
        let i = parsed
            .doc
            .materials()
            .position(|m| m.name() == Some(name))
            .unwrap_or_else(|| panic!("no material {name:?}"));
        parsed.policies[i].metallic
    }

    /// `probe_triangles` for the part's solid triangles only (see `solid_triangles`): what a
    /// test measures a floor or a wall from, with the foliage cards out of the way.
    #[cfg(test)]
    pub fn probe_solid_triangles(spec: &Load, part: &str) -> (Vec<[f32; 3]>, Vec<u32>) {
        let parsed = parse(spec).unwrap_or_else(|e| panic!("{e}"));
        let g = gather(&parsed.raw[part], &parsed.policies);
        (g.pos, g.solid)
    }

    /// The file's clips and the nodes each one moves, `(clip, nodes)` in file order, read
    /// without loading anything else: what a scene needs before it can build a `Load` that
    /// splits the moving nodes into parts of their own (`PartSpec::skip`, [`Frame::Scene`]).
    pub fn clips(path: &str) -> Result<Vec<(String, Vec<String>)>, AssetError> {
        let path = assets::path(path);
        let bad = |reason: String| AssetError::Gltf { path: path.clone(), reason };
        let bytes =
            std::fs::read(&path).map_err(|source| AssetError::Io { path: path.clone(), source })?;
        let gltf = gltf::Gltf::from_slice(&bytes).map_err(|e| bad(format!("parse: {e}")))?;
        let blob = gltf.blob.as_deref().ok_or_else(|| bad("no BIN chunk".to_string()))?;
        Ok(parse_animations(&gltf.document, blob)
            .map_err(bad)?
            .into_iter()
            .map(|a| (a.name, a.channels.into_iter().map(|c| c.node).collect()))
            .collect())
    }

    /// Fitted bounds of a part: `[minx, maxx, miny, maxy, minz, maxz]`.
    pub fn bounds(&self, part: &str) -> [f32; 6] {
        *self.bounds.get(part).unwrap_or_else(|| panic!("no part {part:?}"))
    }

    /// A part's fitted triangles in the model's object space, as `(positions, indices)` with
    /// three indices per triangle and each triangle listed once.
    pub fn triangles(&self, part: &str) -> (&[[f32; 3]], &[u32]) {
        let g = self.geometry.get(part).unwrap_or_else(|| panic!("no part {part:?}"));
        (&g.pos, &g.idx)
    }

    /// [`GltfModel::triangles`] restricted to the triangles of **opaque** materials: neither
    /// alpha-tested (`MASK`, or `BLEND` under the alpha policy) nor translucent. What a
    /// collider is built from. A foliage card is a picture of a bush on a quad, and a player
    /// must walk through the quad's transparent corners as freely as its leaves; a pool's
    /// water is something to wade through, not a floor. Positions are the whole part's --
    /// the indices simply never name the others.
    pub fn solid_triangles(&self, part: &str) -> (&[[f32; 3]], &[u32]) {
        let g = self.geometry.get(part).unwrap_or_else(|| panic!("no part {part:?}"));
        (&g.pos, &g.solid)
    }

    /// Whether a part's materials are `KHR_materials_unlit` -- and so whether it is drawn with
    /// `gltfunlit` or `gltfpbr`. A part is all one or all the other (`load` checks).
    pub fn unlit(&self, part: &str) -> bool {
        let prims = self.parts.get(part).unwrap_or_else(|| panic!("no part {part:?}"));
        prims.first().is_some_and(|p| self.materials[p.material].unlit.is_some())
    }

    /// The clip called `name`, if the file has one.
    pub fn animation(&self, name: &str) -> Option<&Animation> {
        self.rig.animation(name)
    }

    /// Every clip in the file, in its order.
    pub fn animations(&self) -> impl Iterator<Item = &Animation> {
        self.rig.animations.iter()
    }

    /// How far `anim` has moved `node` from its rest pose at `t` seconds, as a displacement in
    /// the model's fitted object space -- the space a part gathered under [`Frame::Scene`] is
    /// in. The parent chain's linear part (the exporter's unit scale and up-axis rotation) and
    /// the fit scale are applied, so a caller draws the `Door1` part at the elevator's
    /// `Object` plus this and gets the asset's own door motion. Zero at `t = 0` for a clip
    /// that starts at rest; `None` when the clip does not move `node` or the file has no node
    /// by that name.
    pub fn node_delta(&self, anim: &Animation, node: &str, t: f32) -> Option<Vector3> {
        self.rig.node_delta(anim, node, t)
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
    /// lights and grades like every other surface in the scene, then adds the material
    /// samplers on top.
    ///
    /// Opaque and alpha-tested primitives go first, then the part's translucent ones blended
    /// over them (see the module docs), in file order within each group. Translucent surfaces
    /// are ordered against the rest of the scene only by what was drawn before this part: a
    /// scene that wants its water over everything draws the model that owns it last. A model
    /// of several parts wants every part's opaque pass before any part's translucent one --
    /// [`GltfModel::draw_part_pass`] draws the two separately for that.
    ///
    /// Takes `&self`: `ObjectT::draw` is re-entrant through portal recursion (Portal::Draw
    /// re-enters Engine::Render), so a draw path must never mutate.
    pub fn draw_part(
        &self,
        part: &str,
        obj: &Object,
        shader: &Shader,
        cam: &Camera,
        ctx: &RenderCtx,
    ) {
        self.draw_part_pass(part, obj, shader, cam, ctx, Pass::Opaque);
        self.draw_part_pass(part, obj, shader, cam, ctx, Pass::Translucent);
    }

    /// One of the two passes of [`GltfModel::draw_part`]. [`Pass::Translucent`] leaves the
    /// blend state as the ported renderer expects it (BLEND off, depth writes on, back faces
    /// culled) before returning, and costs nothing for a part with no translucent material.
    ///
    /// Either pass is skipped outright when the part's bounding sphere lies wholly outside
    /// the pass frustum (`ext::cull`). The backrooms sits 1,000 units from the meadow and is
    /// drawn by the meadow's main pass too; without this its 70k triangles would be
    /// transformed and clipped in every pass that cannot see it.
    pub fn draw_part_pass(
        &self,
        part: &str,
        obj: &Object,
        shader: &Shader,
        cam: &Camera,
        ctx: &RenderCtx,
        pass: Pass,
    ) {
        let Some(prims) = self.parts.get(part) else { return };
        let wanted =
            |p: &&Prim| self.materials[p.material].translucent == (pass == Pass::Translucent);
        if !prims.iter().any(|p| wanted(&p)) {
            return;
        }
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
        // EXT: the scene's own fog, if it set one (`view::Fog`). Zeroes mean "the weather's",
        // which is what the shader does with them and what GL would have held anyway.
        let (over, shape) = match crate::ext::view::fog() {
            Some(f) => {
                ([f.color[0], f.color[1], f.color[2], 1.0], [f.density, f.squareness, f.cap, 0.0])
            }
            None => ([0.0; 4], [0.0; 4]),
        };
        shader.set_vec4("fog_over", over);
        shader.set_vec4("fog_shape", shape);
        // EXT: the flashlight's cone (src/ext/view.rs).
        crate::ext::view::upload_spot(shader);
        shader.set_i32("tex", 0);
        shader.set_i32("tex2", 1);
        shader.set_i32("tex3", 2);

        unsafe {
            match pass {
                Pass::Opaque => {
                    for p in prims.iter().filter(wanted) {
                        self.draw_prim(p, shader, p.count);
                    }
                }
                Pass::Translucent => {
                    let gl = &self.gl;
                    gl.enable(glow::BLEND);
                    gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
                    gl.depth_mask(false);
                    // Both sides of a see-through surface show, and the double-sided copy of
                    // the winding must not draw: culling off, source winding only.
                    gl.disable(glow::CULL_FACE);
                    for p in prims.iter().filter(wanted) {
                        self.draw_prim(p, shader, p.front);
                    }
                    gl.enable(glow::CULL_FACE);
                    gl.depth_mask(true);
                    gl.disable(glow::BLEND);
                }
            }
            self.gl.bind_vertex_array(None);
        }
    }

    /// Bind one primitive's material and issue its draw, for the first `count` indices.
    fn draw_prim(&self, p: &Prim, shader: &Shader, count: i32) {
        let gl = &self.gl;
        let m = &self.materials[p.material];
        if let Some(u) = &m.unlit {
            shader.set_vec4("base_color", u.base_color);
            let [er, eg, eb] = u.emissive;
            shader.set_vec4("emissive", [er, eg, eb, 0.0]);
        }
        shader.set_f32("alpha_cutoff", m.alpha_cutoff);
        unsafe {
            if let Some(light) = m.light {
                gl.active_texture(glow::TEXTURE2);
                gl.bind_texture(glow::TEXTURE_2D, Some(light));
            }
            if let Some(surface) = m.surface {
                gl.active_texture(glow::TEXTURE1);
                gl.bind_texture(glow::TEXTURE_2D, Some(surface));
            }
            // Leave unit 0 active on the way out: Object::draw_impl binds its texture with no
            // active_texture call of its own (object.rs:86-88), so any object drawn after this
            // one would otherwise land its texture on the last unit used here and sample black.
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(m.albedo));
            gl.bind_vertex_array(Some(p.vao));
            gl.draw_elements(glow::TRIANGLES, count, glow::UNSIGNED_INT, 0);
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
                for t in [m.surface, m.light].into_iter().flatten() {
                    self.gl.delete_texture(t);
                }
            }
        }
    }
}

/// Read, walk and fit. See `Parsed`. Anything wrong with the file, or with what the spec
/// asks of it, is a `Gltf` error naming the file.
fn parse(spec: &Load) -> Result<Parsed, AssetError> {
    let path = assets::path(spec.path);
    let bytes =
        std::fs::read(&path).map_err(|source| AssetError::Io { path: path.clone(), source })?;
    parse_bytes(&bytes, spec, path)
}

/// `parse` on a GLB already in memory; `path` only names it in errors.
fn parse_bytes(bytes: &[u8], spec: &Load, path: std::path::PathBuf) -> Result<Parsed, AssetError> {
    let Load { parts, fit, translucent, metallic_override, cut_boxes, .. } = *spec;
    let bad = |reason: String| AssetError::Gltf { path: path.clone(), reason };
    let gltf = gltf::Gltf::from_slice(bytes).map_err(|e| bad(format!("parse: {e}")))?;
    let blob = gltf.blob.clone().ok_or_else(|| bad("no BIN chunk".to_string()))?;
    let doc = gltf.document;

    // ── Every node's parent matrix under the default scene: what `Frame::Scene` walks from
    // and what a node's animated translation is carried through.
    let scene = doc.default_scene().or_else(|| doc.scenes().next());
    let mut parents = vec![IDENT; doc.nodes().count()];
    if let Some(scene) = &scene {
        for root in scene.nodes() {
            record_parents(&root, IDENT, &mut parents);
        }
    }
    let find = |name: &str| {
        doc.nodes()
            .find(|n| n.name() == Some(name))
            .ok_or_else(|| bad(format!("no node named {name:?}")))
    };

    // ── Walk each part's sub-trees into CPU buffers. A primitive without a material takes
    // the slot one past the file's materials (see `material`).
    let n_materials = doc.materials().count();
    let mut raw: HashMap<String, Vec<Raw>> = HashMap::new();
    for spec in parts {
        let mut out: Vec<Raw> = Vec::new();
        let roots: Vec<gltf::Node> = if spec.roots.is_empty() {
            scene.as_ref().map(|s| s.nodes().collect()).unwrap_or_default()
        } else {
            spec.roots.iter().map(|r| find(r)).collect::<Result<_, _>>()?
        };
        for root in &roots {
            let base = match spec.frame {
                Frame::Local => IDENT,
                // Translation only: this brings the part back into its siblings' space
                // without re-applying the pose rotation that walking from below the node
                // was meant to drop.
                Frame::Translated(pre) => {
                    let t = find(pre)?.transform().decomposed().0;
                    let mut m = IDENT;
                    m[3] = [t[0], t[1], t[2], 1.0];
                    m
                }
                Frame::Scene => parents[root.index()],
            };
            walk(root, base, spec.skip, &blob, n_materials, &mut out);
        }
        if out.is_empty() {
            return Err(bad(format!("part {:?} gathered no geometry", spec.name)));
        }
        raw.insert(spec.name.to_string(), out);
    }

    // ── Fit. Under `Fit::Part`: ONE scale and ONE origin for the whole model, both taken
    // from the hinge part. Every other part keeps its true offset from it, so the assembly
    // comes out of the file rather than being reassembled from constants. Under
    // `Fit::Identity` nothing moves, and the bounds simply report where the file put things.
    let mut fit_scale = 1.0;
    if let Fit::Part { part: scale_part, height: scale_height } = fit {
        let sb = bbox(&raw[scale_part]);
        let scale = scale_height / (sb[3] - sb[2]);
        fit_scale = scale;
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
                if !raw.contains_key(h) {
                    return Err(bad(format!(
                        "part {:?} is anchored around {h:?}, which is not a declared part",
                        spec.name
                    )));
                }
            }
        }
    }
    let bounds: HashMap<String, [f32; 6]> =
        raw.iter().map(|(name, list)| (name.clone(), bbox(list))).collect();

    // ── The cuts, in the fitted space the bounds were just taken in. A part left with no
    // triangle at all was wholly inside a box, which no scene means.
    for &(lo, hi) in cut_boxes {
        for (name, list) in raw.iter_mut() {
            for r in list.iter_mut() {
                *r = cut_raw(r, lo, hi);
            }
            list.retain(|r| !r.idx.is_empty());
            if list.is_empty() {
                return Err(bad(format!(
                    "part {name:?} lies wholly inside the cut box {lo:?}..{hi:?}"
                )));
            }
        }
    }

    // ── Alpha policy and metalness per material, and whether its UVs tile. One slot past
    // the file's materials is the default material, for primitives that declare none; it is
    // only built when some primitive uses it.
    for name in translucent {
        if !doc.materials().any(|m| m.name() == Some(name)) {
            return Err(bad(format!("no material named {name:?} to draw translucent")));
        }
    }
    for (pattern, _) in metallic_override {
        if !doc.materials().any(|m| m.name().is_some_and(|n| pattern_matches(pattern, n))) {
            return Err(bad(format!("no material matches {pattern:?} to override its metalness")));
        }
    }
    let mut tiles = vec![false; n_materials + 1];
    for r in raw.values().flatten() {
        if r.uv.iter().any(|uv| uv.iter().any(|&c| !(-1e-4..=1.0 + 1e-4).contains(&c))) {
            tiles[r.material] = true;
        }
    }
    let uses_default = raw.values().flatten().any(|r| r.material == n_materials);
    let policies = (0..n_materials + usize::from(uses_default))
        .map(|i| {
            let m = material(&doc, i).expect("in range, or the default some primitive uses");
            let named = m.name().is_some_and(|n| translucent.contains(&n));
            let metallic = m.name().and_then(|n| {
                metallic_override.iter().find(|(p, _)| pattern_matches(p, n)).map(|&(_, f)| f)
            });
            material_policy(&m, named, tiles[i], metallic)
        })
        .collect();

    // ── Motion: the clips, and every named node's rest pose.
    let animations = parse_animations(&doc, &blob).map_err(bad)?;
    let nodes = doc
        .nodes()
        .filter_map(|n| {
            let name = n.name()?;
            let translation = n.transform().decomposed().0;
            Some((name.to_string(), NodeRest { translation, parent: parents[n.index()] }))
        })
        .collect();

    Ok(Parsed { doc, blob, raw, bounds, policies, rig: Rig { animations, nodes, fit_scale } })
}

/// The alpha policy (module docs) for one material, plus its metalness override.
fn material_policy(
    m: &gltf::Material,
    named_translucent: bool,
    tiles: bool,
    metallic: Option<f32>,
) -> MaterialPolicy {
    use gltf::material::AlphaMode;
    let policy = |alpha_cutoff: f32, translucent: bool, keeps_alpha: bool| MaterialPolicy {
        alpha_cutoff,
        translucent,
        keeps_alpha,
        tiles,
        metallic,
    };
    if named_translucent {
        return policy(-1.0, true, true);
    }
    match m.alpha_mode() {
        AlphaMode::Opaque => policy(-1.0, false, false),
        // The spec's default cutoff is 0.5 when the file leaves it out.
        AlphaMode::Mask => policy(m.alpha_cutoff().unwrap_or(0.5), false, true),
        AlphaMode::Blend => policy(0.5, false, true),
    }
}

/// Whether a `Load::metallic_override` pattern names `material`: exact, or a prefix when the
/// pattern ends in `*`.
fn pattern_matches(pattern: &str, material: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => material.starts_with(prefix),
        None => pattern == material,
    }
}

/// The file's material `i`, or the default material for `i` one past the file's count --
/// the slot `walk` gives a primitive that declares none (see `default_material`).
fn material(doc: &gltf::Document, i: usize) -> Option<gltf::Material<'_>> {
    doc.materials().nth(i).or_else(|| default_material(doc))
}

/// The spec's default material, as the `gltf` crate hands it to a primitive without one:
/// white, metalness 1, roughness 1, opaque, no maps. There is no way to construct it
/// directly, so it is borrowed from the first such primitive; `None` when every primitive
/// names a material and nothing needs it.
fn default_material(doc: &gltf::Document) -> Option<gltf::Material<'_>> {
    doc.meshes().flat_map(|m| m.primitives()).map(|p| p.material()).find(|m| m.index().is_none())
}

/// Fill `parents[i]` with the world matrix of node `i`'s parent, for every node under `node`.
fn record_parents(node: &gltf::Node, parent: M, parents: &mut [M]) {
    parents[node.index()] = parent;
    let world = mul(parent, node.transform().matrix());
    for c in node.children() {
        record_parents(&c, world, parents);
    }
}

/// Every clip's translation channels. The error string names what was wrong with the file.
fn parse_animations(doc: &gltf::Document, blob: &[u8]) -> Result<Vec<Animation>, String> {
    use gltf::animation::util::ReadOutputs;
    use gltf::animation::{Interpolation, Property};
    let mut out = Vec::new();
    for anim in doc.animations() {
        let name = anim.name().unwrap_or("").to_string();
        let mut channels = Vec::new();
        for ch in anim.channels() {
            if ch.target().property() != Property::Translation {
                continue;
            }
            // An unnamed node cannot be asked for by name, and that is the only way out.
            let Some(node) = ch.target().node().name() else { continue };
            let reader = ch.reader(|b| if b.index() == 0 { Some(blob) } else { None });
            let times: Vec<f32> = reader
                .read_inputs()
                .ok_or_else(|| format!("animation {name:?}: channel input unreadable"))?
                .collect();
            let Some(ReadOutputs::Translations(values)) = reader.read_outputs() else {
                return Err(format!("animation {name:?}: {node} translation output unreadable"));
            };
            let mut values: Vec<[f32; 3]> = values.collect();
            let interpolation = match ch.sampler().interpolation() {
                Interpolation::Linear => Interp::Linear,
                Interpolation::Step => Interp::Step,
                // Three outputs per key: in-tangent, value, out-tangent. Keep the values.
                Interpolation::CubicSpline => {
                    values = values.chunks_exact(3).map(|k| k[1]).collect();
                    Interp::Linear
                }
            };
            if times.is_empty() || times.len() != values.len() {
                return Err(format!(
                    "animation {name:?}: {node} has {} keys and {} values",
                    times.len(),
                    values.len()
                ));
            }
            if times.windows(2).any(|w| w[1] < w[0]) {
                return Err(format!("animation {name:?}: {node} keys are not in time order"));
            }
            channels.push(Channel { node: node.to_string(), interpolation, times, values });
        }
        let duration = channels.iter().map(|c| c.times[c.times.len() - 1]).fold(0.0, f32::max);
        out.push(Animation { name, duration, channels });
    }
    Ok(out)
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

/// One vertex of a [`Raw`], for the cut: what a clipped edge interpolates.
#[derive(Clone, Copy)]
struct Vertex {
    pos: [f32; 3],
    uv: [f32; 2],
    nrm: [f32; 3],
    tan: [f32; 4],
}

/// `r` with the box `[lo, hi]` carved out (`ext/carve.rs`): triangles clear of the box keep
/// their vertices and indices, triangles crossing it are clipped and their outside pieces
/// fanned from new vertices whose UV, normal and tangent are blended along the cut edge --
/// the normal and tangent renormalised, the tangent's handedness the edge's first vertex's
/// (a triangle's three share it). The double-sided copy of the winding, if the material has
/// one, is made again from the cut front faces.
fn cut_raw(r: &Raw, lo: Vector3, hi: Vector3) -> Raw {
    let mut pos = r.pos.clone();
    let mut uv = r.uv.clone();
    let mut nrm = r.nrm.clone();
    let mut tan = r.tan.clone();
    let mut idx: Vec<u32> = Vec::with_capacity(r.front);
    let vertex = |i: u32| {
        let i = i as usize;
        Vertex { pos: r.pos[i], uv: r.uv[i], nrm: r.nrm[i], tan: r.tan[i] }
    };
    let at = |v: &Vertex| Vector3::from_slice(&v.pos);
    let lerp = |a: &Vertex, b: &Vertex, t: f32| {
        let mix = |x: f32, y: f32| x + (y - x) * t;
        let unit = |v: [f32; 3]| {
            let m = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            if m > 1e-12 {
                [v[0] / m, v[1] / m, v[2] / m]
            } else {
                v
            }
        };
        let n = unit([mix(a.nrm[0], b.nrm[0]), mix(a.nrm[1], b.nrm[1]), mix(a.nrm[2], b.nrm[2])]);
        let t3 = unit([mix(a.tan[0], b.tan[0]), mix(a.tan[1], b.tan[1]), mix(a.tan[2], b.tan[2])]);
        Vertex {
            pos: [mix(a.pos[0], b.pos[0]), mix(a.pos[1], b.pos[1]), mix(a.pos[2], b.pos[2])],
            uv: [mix(a.uv[0], b.uv[0]), mix(a.uv[1], b.uv[1])],
            nrm: n,
            tan: [t3[0], t3[1], t3[2], a.tan[3]],
        }
    };
    for t in r.idx[..r.front].chunks_exact(3) {
        let tri = [vertex(t[0]), vertex(t[1]), vertex(t[2])];
        if crate::ext::carve::clear_of_box(&[at(&tri[0]), at(&tri[1]), at(&tri[2])], lo, hi) {
            idx.extend_from_slice(t);
            continue;
        }
        crate::ext::carve::clip_outside_box(tri, lo, hi, at, lerp, |poly| {
            let base = pos.len() as u32;
            for v in poly {
                pos.push(v.pos);
                uv.push(v.uv);
                nrm.push(v.nrm);
                tan.push(v.tan);
            }
            for i in 1..poly.len() as u32 - 1 {
                idx.extend_from_slice(&[base, base + i, base + i + 1]);
            }
        });
    }
    let front = idx.len();
    if r.idx.len() > r.front {
        let back: Vec<u32> = idx.chunks_exact(3).flat_map(|t| [t[2], t[1], t[0]]).collect();
        idx.extend(back);
    }
    Raw { pos, uv, nrm, tan, idx, front, material: r.material }
}

/// The base colour texture's `KHR_texture_transform`, if the primitive's material has one.
/// Assumed to apply to the material's normal map too -- true of every asset shipped, where
/// the exporter wrote the same transform on both, and the only way it could be, since a
/// normal map sampled on other coordinates than its colour would shade the wrong bumps.
fn uv_transform(prim: &gltf::Primitive) -> Option<UvTransform> {
    let info = prim.material().pbr_metallic_roughness().base_color_texture()?;
    let t = info.texture_transform()?;
    Some(UvTransform { offset: t.offset(), rotation: t.rotation(), scale: t.scale() })
}

/// Gather the primitives under `node` into `out`; `default_material` is the material slot a
/// primitive that declares none is given (one past the file's own -- see `material`), so it
/// never aliases the file's first material, and a file with no materials at all still loads.
fn walk(
    node: &gltf::Node,
    parent: M,
    skip: &[&str],
    blob: &[u8],
    default_material: usize,
    out: &mut Vec<Raw>,
) {
    if node.name().is_some_and(|n| skip.contains(&n)) {
        return;
    }
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
            // The texture transform is baked here rather than applied in the shader: every
            // primitive gets its own UV copy anyway, so the bake costs nothing per frame and
            // the shaders stay ignorant of the extension.
            let transform = uv_transform(&prim);
            let uv: Vec<[f32; 2]> = match r.read_tex_coords(0) {
                Some(it) => it.into_f32().map(|uv| transform.map_or(uv, |t| t.apply(uv))).collect(),
                None => vec![[0.0, 0.0]; n],
            };
            let mut idx: Vec<u32> = match r.read_indices() {
                Some(it) => it.into_u32().collect(),
                None => (0..n as u32).collect(),
            };
            // TANGENT is VEC4: xyz is the tangent, w the bitangent handedness. The transform
            // moves xyz; w is a sign and must be carried through untouched. A file without
            // tangents (the Blender exports: every Sketchfab room, tangents or not a normal
            // map is there) gets them from its UVs, as the spec asks of a client -- a fixed
            // stand-in is parallel to the normal on some face of every model, and there the
            // shader's frame collapses to NaN and the surface goes black.
            let tan: Vec<[f32; 4]> = match r.read_tangents() {
                Some(it) => it
                    .map(|t| {
                        let d = direction(&world, [t[0], t[1], t[2]]);
                        [d[0], d[1], d[2], t[3]]
                    })
                    .collect(),
                None => tangents(&pos, &nrm, &uv, &idx),
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
                let back: Vec<u32> = idx.chunks_exact(3).flat_map(|t| [t[2], t[1], t[0]]).collect();
                idx.extend(back);
            }
            out.push(Raw {
                pos,
                uv,
                nrm,
                tan,
                idx,
                front,
                material: prim.material().index().unwrap_or(default_material),
            });
        }
    }
    for c in node.children() {
        walk(&c, world, skip, blob, default_material, out);
    }
}

/// Per-vertex tangents from the UV layout: the surface direction along which `u` grows,
/// summed over each vertex's triangles and made orthogonal to its normal, with `w` the
/// handedness that puts `cross(normal, tangent) * w` along growing `v` -- the frame a glTF
/// normal map is expressed in. A vertex whose triangles have no usable UV area (a degenerate
/// mapping) gets any direction perpendicular to its normal, so the shader's frame never
/// collapses.
fn tangents(pos: &[[f32; 3]], nrm: &[[f32; 3]], uv: &[[f32; 2]], idx: &[u32]) -> Vec<[f32; 4]> {
    let n = pos.len();
    let mut t_sum = vec![[0.0f32; 3]; n];
    let mut b_sum = vec![[0.0f32; 3]; n];
    for tri in idx.chunks_exact(3) {
        let [i0, i1, i2] = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
        let (p0, p1, p2) = (pos[i0], pos[i1], pos[i2]);
        let (w0, w1, w2) = (uv[i0], uv[i1], uv[i2]);
        let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let (du1, dv1) = (w1[0] - w0[0], w1[1] - w0[1]);
        let (du2, dv2) = (w2[0] - w0[0], w2[1] - w0[1]);
        let det = du1 * dv2 - du2 * dv1;
        if det.abs() < 1e-12 {
            continue;
        }
        let r = 1.0 / det;
        for i in [i0, i1, i2] {
            for k in 0..3 {
                t_sum[i][k] += (e1[k] * dv2 - e2[k] * dv1) * r;
                b_sum[i][k] += (e2[k] * du1 - e1[k] * du2) * r;
            }
        }
    }
    (0..n)
        .map(|i| {
            let nv = nrm[i];
            let t = t_sum[i];
            // Gram-Schmidt against the normal, then unit length.
            let d = t[0] * nv[0] + t[1] * nv[1] + t[2] * nv[2];
            let mut t = [t[0] - nv[0] * d, t[1] - nv[1] * d, t[2] - nv[2] * d];
            let len = (t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt();
            if len > 1e-8 {
                for v in t.iter_mut() {
                    *v /= len;
                }
            } else {
                t = perpendicular(nv);
            }
            let c = [
                nv[1] * t[2] - nv[2] * t[1],
                nv[2] * t[0] - nv[0] * t[2],
                nv[0] * t[1] - nv[1] * t[0],
            ];
            let b = b_sum[i];
            let w = if c[0] * b[0] + c[1] * b[1] + c[2] * b[2] < 0.0 { -1.0 } else { 1.0 };
            [t[0], t[1], t[2], w]
        })
        .collect()
}

/// Any unit vector perpendicular to `n`: the cross with whichever axis `n` leans least on.
fn perpendicular(n: [f32; 3]) -> [f32; 3] {
    let axis = if n[0].abs() < n[1].abs() && n[0].abs() < n[2].abs() {
        [1.0, 0.0, 0.0]
    } else if n[1].abs() < n[2].abs() {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    direction(
        &IDENT,
        [
            n[1] * axis[2] - n[2] * axis[1],
            n[2] * axis[0] - n[0] * axis[2],
            n[0] * axis[1] - n[1] * axis[0],
        ],
    )
}

/// Concatenate a part's primitives into one triangle list, each triangle once, and note
/// which of them are solid (`GltfModel::solid_triangles`: the material neither alpha-tested
/// nor translucent).
fn gather(list: &[Raw], policies: &[MaterialPolicy]) -> Geometry {
    let mut pos = Vec::new();
    let mut idx = Vec::new();
    let mut solid = Vec::new();
    for r in list {
        let base = pos.len() as u32;
        pos.extend_from_slice(&r.pos);
        let own = r.idx[..r.front].iter().map(|i| i + base);
        let policy = &policies[r.material];
        if policy.alpha_cutoff < 0.0 && !policy.translucent {
            solid.extend(own.clone());
        }
        idx.extend(own);
    }
    Geometry { pos, idx, solid }
}

fn upload(gl: &Rc<glow::Context>, r: &Raw) -> Result<Prim, AssetError> {
    let gl_error = |e: String| AssetError::Gl(format!("glTF buffer allocation failed: {e}"));
    unsafe {
        let vao = gl.create_vertex_array().map_err(gl_error)?;
        gl.bind_vertex_array(Some(vao));
        let mut bufs = Vec::new();

        // Locations 0..3 match the order `Shader` binds them in: it scrapes the vertex source
        // for "\nin " and assigns 0, 1, 2, ... in declaration order (shader.rs:88-96). The door
        // shader therefore declares in_pos, in_uv, in_normal, in_tangent in exactly this order.
        let mut attrib = |loc: u32, comps: i32, data: &[u8]| -> Result<(), AssetError> {
            let b = gl.create_buffer().map_err(gl_error)?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(b));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, data, glow::STATIC_DRAW);
            gl.enable_vertex_attrib_array(loc);
            gl.vertex_attrib_pointer_f32(loc, comps, glow::FLOAT, false, 0, 0);
            bufs.push(b);
            Ok(())
        };
        attrib(0, 3, as_bytes(&r.pos))?;
        attrib(1, 2, as_bytes(&r.uv))?;
        attrib(2, 3, as_bytes(&r.nrm))?;
        attrib(3, 4, as_bytes(&r.tan))?;

        let ib = gl.create_buffer().map_err(gl_error)?;
        gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ib));
        gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, as_bytes(&r.idx), glow::STATIC_DRAW);
        bufs.push(ib);

        gl.bind_vertex_array(None);
        Ok(Prim {
            vao,
            bufs,
            count: r.idx.len() as i32,
            front: r.front as i32,
            material: r.material,
        })
    }
}

/// A slice of `[f32; N]` or `u32` as the bytes `buffer_data_u8_slice` takes, the same way
/// `mesh.rs` does for the ported meshes.
fn as_bytes<T: bytemuck::Pod>(v: &[T]) -> &[u8] {
    bytemuck::cast_slice(v)
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
///
/// An image the file does not embed -- a `uri` reference, which the GLB-only contract of this
/// loader has no way to fetch -- or one that does not decode is an error: a map quietly missing
/// is a model quietly wrong.
fn decode_maps(
    doc: &gltf::Document,
    blob: &[u8],
    max_map: u32,
) -> Result<HashMap<usize, image::RgbaImage>, String> {
    let mut wanted: Vec<usize> =
        doc.materials().flat_map(|m| material_sources(&m)).flatten().collect();
    wanted.sort_unstable();
    wanted.dedup();

    // Resolve each index to its slice of the blob up front: `gltf::Document` is not `Sync`, so
    // the workers must not touch it.
    let mut jobs: Vec<(usize, &[u8])> = Vec::with_capacity(wanted.len());
    for &i in &wanted {
        let img = doc.images().nth(i).ok_or_else(|| format!("image {i} is out of range"))?;
        match img.source() {
            gltf::image::Source::View { view, .. } => {
                let off = view.offset();
                jobs.push((i, &blob[off..off + view.length()]));
            }
            gltf::image::Source::Uri { uri, .. } => {
                return Err(format!("image {i} is a URI reference ({uri:?}); embed it (GLB only)"))
            }
        }
    }

    // Biggest first. The lanes below pull from a shared queue, and starting the long jobs
    // early is what keeps the last lane from finishing alone: the door's maps range from 380 KB
    // to 20 MB, and a wave that happens to draw four small ones then a large one takes as long
    // as the large one all by itself.
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
        out.insert(i, img.map_err(|e| format!("image {i} does not decode: {e}"))?);
    }
    Ok(out)
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

/// The five maps a glTF metallic-roughness material can carry, as image indices, in the order
/// `build_material` reads them: base colour, metallic-roughness, normal, occlusion, emissive.
/// An unlit material reads only its base colour map: the other four describe a lighting
/// response it does not have, so they are never decoded for it.
fn material_sources(m: &gltf::Material) -> [Option<usize>; 5] {
    let pbr = m.pbr_metallic_roughness();
    let base = pbr.base_color_texture().map(|t| t.texture().source().index());
    if m.unlit() {
        return [base, None, None, None, None];
    }
    [
        base,
        pbr.metallic_roughness_texture().map(|t| t.texture().source().index()),
        m.normal_texture().map(|t| t.texture().source().index()),
        m.occlusion_texture().map(|t| t.texture().source().index()),
        m.emissive_texture().map(|t| t.texture().source().index()),
    ]
}

/// Decode one embedded image (PNG or JPEG, told apart by their magic bytes) to RGBA, downscaled
/// so its longer side is `max_map` if it is larger than that.
///
/// A source already at or under the cap is handed back as decoded: no resample, and so no
/// softening of maps that were baked at exactly the size they are shown. `into_rgba8` also
/// expands paletted PNGs (the backrooms ships one) and greyscale JPEGs, so every map arrives
/// as straight RGBA.
fn decode_one(bytes: &[u8], max_map: u32) -> image::ImageResult<image::RgbaImage> {
    // `into_rgba8`, not `to_rgba8`: the latter copies the decoded image into a second buffer of
    // the same size, which at 4096 square is another 67 MB held for the length of the resize.
    let rgba = image::load_from_memory(bytes)?.into_rgba8();
    // A map already at or under the cap -- every one in the shipped door, and all of the
    // backrooms' -- is used as decoded. Resampling at 1:1 would be a copy through the filter
    // that changes nothing but the time.
    let (w, h) = rgba.dimensions();
    if w <= max_map && h <= max_map {
        return Ok(rgba);
    }
    // Shrink about the longer side so a 2:1 map stays 2:1 rather than being squashed square.
    let (nw, nh) = if w >= h {
        (max_map, (h * max_map / w).max(1))
    } else {
        ((w * max_map / h).max(1), max_map)
    };
    Ok(image::imageops::resize(&rgba, nw, nh, MAP_FILTER))
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

/// The size a packed texture takes: that of the largest map feeding it, or 1x1 when nothing
/// does and every texel would be the same factor.
fn pack_size(maps: &[Option<&image::RgbaImage>]) -> (u32, u32) {
    maps.iter()
        .flatten()
        .map(|im| im.dimensions())
        .max_by_key(|&(w, h)| w as u64 * h as u64)
        .unwrap_or((1, 1))
}

/// Fill a `w` x `h` RGBA texture from a per-texel function of `(x, y)`, where `px(map)` is
/// that texel of any of the source maps -- point-sampled up when a source is smaller than the
/// pack (identity when, as in the door, every map is the pack's own size).
fn pack(
    w: u32,
    h: u32,
    texel: impl Fn(&dyn Fn(Option<&image::RgbaImage>) -> Option<image::Rgba<u8>>) -> [u8; 4],
) -> Vec<u8> {
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let px = |img: Option<&image::RgbaImage>| {
                img.map(|im| *im.get_pixel(x * im.width() / w, y * im.height() / h))
            };
            out.extend_from_slice(&texel(&px));
        }
    }
    out
}

/// Build a material's textures: the unlit map as shipped, or a glTF metallic-roughness
/// material's maps packed into the three RGBA textures [`Material`] describes.
///
/// `Object` has a single texture slot and the ported `Texture` can only be built from a file on
/// disk, so a multi-map material needs its own upload path anyway. Packing is not just
/// convenience: occlusion, roughness and metalness are single-channel data and base colour,
/// emissive and normal are three-channel, and interleaving them costs three samplers instead
/// of five with no loss.
fn build_material(
    gl: &Rc<glow::Context>,
    doc: &gltf::Document,
    maps: &HashMap<usize, image::RgbaImage>,
    i: usize,
    policy: &MaterialPolicy,
) -> Result<Material, AssetError> {
    let m = material(doc, i).expect("material index in range, or the default");
    let pbr = m.pbr_metallic_roughness();

    let [base_src, mr_src, nrm_src, occ_src, emis_src] = material_sources(&m);
    let get = |src: Option<usize>| src.and_then(|s| maps.get(&s));
    let (base, mr, nrm, occ, emis) =
        (get(base_src), get(mr_src), get(nrm_src), get(occ_src), get(emis_src));

    let bf = pbr.base_color_factor();
    let strength = m.emissive_strength().unwrap_or(1.0);
    let ef = m.emissive_factor();
    let &MaterialPolicy { alpha_cutoff, translucent, keeps_alpha, tiles, metallic } = policy;
    let sampler_wrap = pbr
        .base_color_texture()
        .map(|t| {
            let s = t.texture().sampler();
            (wrap_mode(s.wrap_s()), wrap_mode(s.wrap_t()))
        })
        .unwrap_or((glow::REPEAT, glow::REPEAT));

    if m.unlit() {
        // The map goes up untouched, at its own size and with its own wrap mode; the factor
        // and the emissive term are uniforms. No pack, no per-texel loop.
        let albedo = match base {
            Some(img) => {
                let (ws, wt) = sampler_wrap;
                tex2d(gl, img.as_raw(), img.width(), img.height(), ws, wt)?
            }
            // No map: 1x1 white, so `texture(tex, uv) * base_color` is the factor alone.
            None => tex2d(gl, &[255, 255, 255, 255], 1, 1, glow::REPEAT, glow::REPEAT)?,
        };
        return Ok(Material {
            albedo,
            surface: None,
            light: None,
            unlit: Some(Unlit {
                base_color: bf,
                emissive: [ef[0] * strength, ef[1] * strength, ef[2] * strength],
            }),
            alpha_cutoff,
            translucent,
        });
    }

    // The file's factor unless the load overrides it (`Load::metallic_override`).
    let metal_f = metallic.unwrap_or_else(|| pbr.metallic_factor());
    let rough_f = pbr.roughness_factor();
    let occ_str = m.occlusion_texture().map(|t| t.strength()).unwrap_or(1.0);
    let unit = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u8;

    // Base colour: the map where there is one, the factor otherwise. Both stay in raw byte
    // space -- the engine has no sRGB decode anywhere, so a gamma-correct upload here would
    // make the door the only surface in the game shaded in a different space. Alpha is the
    // map's times the factor's for a material whose alpha means something, opaque otherwise.
    let (aw, ah) = pack_size(&[base]);
    let albedo = pack(aw, ah, |px| {
        let b = px(base);
        let ch = |c: usize| unit(b.map(|p| p[c] as f32 / 255.0).unwrap_or(1.0) * bf[c]);
        [ch(0), ch(1), ch(2), if keeps_alpha { ch(3) } else { 255 }]
    });

    // Normal xy; z is reconstructed in the shader, so the blue channel is free for roughness.
    // glTF metallic-roughness: G = roughness, B = metalness, each scaled by its factor.
    // The normal map's `scale` is baked into xy here (the spec: scale the xy components,
    // then renormalise -- which the shader's reconstruction of z does). A file can wear a
    // normal map at scale 0, which is how the overgrown room turns the moss's bumps off on
    // the wallpaper that shares them; ignoring it grained every wall.
    let (sw, sh) = pack_size(&[nrm, mr]);
    let nscale = m.normal_texture().map_or(1.0, |t| t.scale());
    let surface = pack(sw, sh, |px| {
        let nv = px(nrm);
        let mrv = px(mr);
        let rough = mrv.map(|p| p[1] as f32 / 255.0).unwrap_or(1.0) * rough_f;
        let metal = mrv.map(|p| p[2] as f32 / 255.0).unwrap_or(1.0) * metal_f;
        [normal_xy(nv, 0, nscale), normal_xy(nv, 1, nscale), unit(rough), unit(metal)]
    });

    // Emissive, factor and strength applied and saturated here -- there is no HDR target for
    // a strength of 10 to mean anything more than "clip to the colour" -- and glTF's
    // occlusion, in R, with `strength` as a lerp toward 1 (no occlusion).
    let (lw, lh) = pack_size(&[emis, occ]);
    let light = pack(lw, lh, |px| {
        let e = px(emis);
        let ch = |c: usize| unit(e.map(|p| p[c] as f32 / 255.0).unwrap_or(1.0) * ef[c] * strength);
        let o = px(occ).map(|p| p[0] as f32 / 255.0).unwrap_or(1.0);
        [ch(0), ch(1), ch(2), unit(1.0 + occ_str * (o - 1.0))]
    });

    // A material addressed by UV islands inside [0,1] -- the door's atlases -- is clamped:
    // it keeps a mip's edge from bleeding the opposite side of the atlas into the seam, and
    // the sampler's REPEAT would only ever have shown at that seam. One whose geometry tiles
    // past [0,1] takes the sampler's word for it, or every tile but the first is a smear of
    // the edge texel.
    let (ws, wt) = if tiles { sampler_wrap } else { (glow::CLAMP_TO_EDGE, glow::CLAMP_TO_EDGE) };
    Ok(Material {
        albedo: tex2d(gl, &albedo, aw, ah, ws, wt)?,
        surface: Some(tex2d(gl, &surface, sw, sh, ws, wt)?),
        light: Some(tex2d(gl, &light, lw, lh, ws, wt)?),
        unlit: None,
        alpha_cutoff,
        translucent,
    })
}

/// One of a normal map texel's xy components, its `scale` applied about the flat value 128:
/// the byte untouched at scale 1, so an unscaled map packs bit for bit, and flat at 0. A
/// missing map is flat.
fn normal_xy(texel: Option<image::Rgba<u8>>, c: usize, scale: f32) -> u8 {
    let Some(p) = texel else { return 128 };
    if scale == 1.0 {
        return p[c];
    }
    let v = (p[c] as f32 / 255.0 * 2.0 - 1.0) * scale;
    ((v * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0).round() as u8
}

fn tex2d(
    gl: &Rc<glow::Context>,
    rgba: &[u8],
    w: u32,
    h: u32,
    wrap_s: u32,
    wrap_t: u32,
) -> Result<glow::Texture, AssetError> {
    unsafe {
        let t = gl
            .create_texture()
            .map_err(|e| AssetError::Gl(format!("glGenTextures for a glTF map failed: {e}")))?;
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
        Ok(t)
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
        // A displacement through the same matrix scales and does not translate.
        assert_eq!(linear(&m, [1.0, 1.0, 1.0]), [2.0, 2.0, 2.0]);
    }

    #[test]
    fn direction_ignores_translation_and_normalizes() {
        let mut t = IDENT;
        t[3] = [5.0, 5.0, 5.0, 1.0];
        let d = direction(&t, [0.0, 3.0, 0.0]);
        assert!((d[1] - 1.0).abs() < 1e-6, "got {d:?}");
    }

    // ── The cut (`Load::cut_boxes`, `cut_raw`).

    /// A 2 x 2 quad in the plane z = 0, facing +z, u along +x and v along +y, with a
    /// double-sided material (the back winding after `front`).
    fn quad_raw() -> Raw {
        let pos = vec![[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [1.0, 1.0, 0.0], [-1.0, 1.0, 0.0]];
        let uv = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let nrm = vec![[0.0, 0.0, 1.0]; 4];
        let tan = vec![[1.0, 0.0, 0.0, 1.0]; 4];
        let front = vec![0u32, 1, 2, 0, 2, 3];
        let back: Vec<u32> = front.chunks_exact(3).flat_map(|t| [t[2], t[1], t[0]]).collect();
        let idx = [front, back].concat();
        Raw { pos, uv, nrm, tan, idx, front: 6, material: 0 }
    }

    fn raw_area(r: &Raw, idx: &[u32]) -> f32 {
        idx.chunks_exact(3)
            .map(|t| {
                let v = |i: u32| Vector3::from_slice(&r.pos[i as usize]);
                (v(t[1]) - v(t[0])).cross(v(t[2]) - v(t[0])).mag() * 0.5
            })
            .sum()
    }

    /// A box straddling the quad's lower-right quarter: that quarter goes, the rest stays
    /// with its winding, and every vertex the cut made has the UV the quad's mapping gives
    /// its position -- the texture does not tear at the hole.
    #[test]
    fn cut_raw_keeps_the_outside_with_interpolated_attributes_and_winding() {
        let r = quad_raw();
        let cut = cut_raw(&r, Vector3::new(0.0, -2.0, -0.5), Vector3::new(2.0, 0.0, 0.5));
        // Front faces first, the back copy after: still double-sided, still mirrored.
        assert_eq!(cut.idx.len(), 2 * cut.front);
        let front = &cut.idx[..cut.front];
        assert!((raw_area(&cut, front) - 3.0).abs() < 1e-5, "area {}", raw_area(&cut, front));
        for (t, b) in front.chunks_exact(3).zip(cut.idx[cut.front..].chunks_exact(3)) {
            let v = |i: u32| Vector3::from_slice(&cut.pos[i as usize]);
            assert!((v(t[1]) - v(t[0])).cross(v(t[2]) - v(t[0])).z > 0.0, "front faces +z");
            assert_eq!(b, [t[2], t[1], t[0]], "the back copy is the front reversed");
        }
        // Nothing of the cut quarter survives, nothing outside it is lost.
        for t in front.chunks_exact(3) {
            let c = (0..3)
                .fold(Vector3::zero(), |c, k| c + Vector3::from_slice(&cut.pos[t[k] as usize]))
                * (1.0 / 3.0);
            assert!(!(c.x > 0.0 && c.y < 0.0), "a piece inside the box at {c:?}");
        }
        // The original four vertices are still the first four; the new ones carry the UVs
        // the quad's mapping gives their positions (u = (x + 1) / 2, v = (y + 1) / 2), the
        // normal, the tangent and its handedness.
        assert_eq!(cut.pos[..4], r.pos[..]);
        assert!(cut.pos.len() > 4, "the cut made vertices");
        for i in 4..cut.pos.len() {
            let [x, y, z] = cut.pos[i];
            assert!(z.abs() < 1e-6);
            let [u, v] = cut.uv[i];
            assert!((u - (x + 1.0) * 0.5).abs() < 1e-5, "u at {x}: {u}");
            assert!((v - (y + 1.0) * 0.5).abs() < 1e-5, "v at {y}: {v}");
            assert_eq!(cut.nrm[i], [0.0, 0.0, 1.0]);
            assert_eq!(cut.tan[i], [1.0, 0.0, 0.0, 1.0]);
            // On the box's faces, or on the quad's edge.
            assert!(
                x.abs() < 1e-5 || y.abs() < 1e-5 || x.abs() > 1.0 - 1e-5 || y.abs() > 1.0 - 1e-5
            );
        }
    }

    /// A box clear of the quad leaves it untouched -- same vertices, same indices -- and one
    /// around it empties it.
    #[test]
    fn cut_raw_leaves_the_clear_and_empties_the_contained() {
        let r = quad_raw();
        let clear = cut_raw(&r, Vector3::new(5.0, 5.0, 5.0), Vector3::new(6.0, 6.0, 6.0));
        assert_eq!((clear.pos.len(), clear.idx, clear.front), (4, r.idx.clone(), 6));
        let gone = cut_raw(&r, Vector3::splat(-2.0), Vector3::splat(2.0));
        assert!(gone.idx.is_empty() && gone.front == 0);
    }

    /// Top of the elevator's cabin floor, as `ext/elevator.rs` measures it.
    const FLOOR_CHECK: f32 = -0.022;

    /// The cut reaches the loader's output: the elevator file with a box over a patch of its
    /// floor loses exactly the triangles there, and a box around everything is an error.
    #[test]
    fn cut_boxes_carve_the_parsed_model() {
        let whole_spec = |cut: &'static [(Vector3, Vector3)]| Load {
            path: "Meshes/elevator_with_animation_lowpoly.glb",
            parts: &WHOLE,
            fit: Fit::Identity,
            max_map: 256,
            translucent: &[],
            metallic_override: &[],
            cut_boxes: cut,
        };
        // A box round a patch of the cabin's floor, at its middle: a ray down onto the patch
        // finds the floor before the cut and nothing at all after it (the file has nothing
        // under its floor), while the rest of the model is as it was.
        static PATCH: [(Vector3, Vector3); 1] =
            [(Vector3 { x: -1.2, y: -0.5, z: -0.5 }, Vector3 { x: -0.6, y: 0.5, z: 0.3 })];
        let whole = GltfModel::probe_triangles(&whole_spec(&[]), "all");
        let cut = GltfModel::probe_triangles(&whole_spec(&PATCH), "all");
        let tris = |(pos, idx): &(Vec<[f32; 3]>, Vec<u32>)| -> Vec<[Vector3; 3]> {
            let v = |i: u32| Vector3::from_slice(&pos[i as usize]);
            idx.chunks_exact(3).map(|t| [v(t[0]), v(t[1]), v(t[2])]).collect()
        };
        let (whole_t, cut_t) = (tris(&whole), tris(&cut));
        let over = Vector3::new(-0.9, 0.4, -0.1);
        let floor = crate::ext::interior::probe::floor_under(&whole_t, over).expect("a floor");
        assert!((floor - FLOOR_CHECK).abs() < 0.01, "the cabin floor at {floor}");
        assert!(crate::ext::interior::probe::floor_under(&cut_t, over).is_none(), "cut away");
        // Beside the patch the floor is still there, and above it nothing changed.
        let beside = Vector3::new(-0.3, 0.4, -0.1);
        let still = crate::ext::interior::probe::floor_under(&cut_t, beside).expect("floor");
        assert!((still - floor).abs() < 1e-4);
        let high = |t: &[[Vector3; 3]]| t.iter().filter(|t| t.iter().all(|p| p.y > 0.5)).count();
        assert_eq!(high(&whole_t), high(&cut_t));
        static ALL: [(Vector3, Vector3); 1] =
            [(Vector3 { x: -10.0, y: -10.0, z: -10.0 }, Vector3 { x: 10.0, y: 10.0, z: 10.0 })];
        let err = parse(&whole_spec(&ALL)).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(err.contains("wholly inside"), "{err}");
    }

    // ── KHR_texture_transform: `uv' = offset + R(rotation) * (scale * uv)`.

    fn close2(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-5 && (a[1] - b[1]).abs() < 1e-5
    }

    #[test]
    fn uv_transform_scales_then_offsets() {
        // The moss: offset [0, -2], scale [3, 3].
        let t = UvTransform { offset: [0.0, -2.0], rotation: 0.0, scale: [3.0, 3.0] };
        assert!(close2(t.apply([0.0, 0.0]), [0.0, -2.0]));
        assert!(close2(t.apply([0.5, 0.5]), [1.5, -0.5]));
        assert!(close2(t.apply([1.0, 1.0]), [3.0, 1.0]));
    }

    #[test]
    fn uv_transform_rotates_after_scaling_and_before_offsetting() {
        // A quarter turn takes u onto v: the spec's R has (cos, sin) as its first column.
        let q = std::f32::consts::FRAC_PI_2;
        let t = UvTransform { offset: [0.0, 0.0], rotation: q, scale: [1.0, 1.0] };
        assert!(close2(t.apply([1.0, 0.0]), [0.0, 1.0]));
        assert!(close2(t.apply([0.0, 1.0]), [-1.0, 0.0]));
        // Scale first, rotate, then offset.
        let t = UvTransform { offset: [10.0, 20.0], rotation: q, scale: [2.0, 3.0] };
        assert!(close2(t.apply([1.0, 1.0]), [10.0 - 3.0, 20.0 + 2.0]));
    }

    #[test]
    fn identity_uv_transform_changes_nothing() {
        let t = UvTransform { offset: [0.0, 0.0], rotation: 0.0, scale: [1.0, 1.0] };
        assert!(close2(t.apply([0.25, 0.75]), [0.25, 0.75]));
    }

    // ── Tangents generated from UVs, for files that ship none.

    /// A quad in the xz plane, facing up, with u along +x and v along +z.
    fn quad_tangents(u_axis: [f32; 2], v_axis: [f32; 2]) -> Vec<[f32; 4]> {
        let pos = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 0.0, 1.0]];
        let nrm = [[0.0, 1.0, 0.0]; 4];
        let uv: Vec<[f32; 2]> = pos
            .iter()
            .map(|p| [p[0] * u_axis[0] + p[2] * u_axis[1], p[0] * v_axis[0] + p[2] * v_axis[1]])
            .collect();
        tangents(&pos, &nrm, &uv, &[0, 2, 1, 0, 3, 2])
    }

    fn close3(a: [f32; 3], b: [f32; 3]) -> bool {
        (0..3).all(|k| (a[k] - b[k]).abs() < 1e-5)
    }

    #[test]
    fn tangent_follows_u_and_handedness_follows_v() {
        // u along +x, v along +z: tangent +x; cross(n, t) = cross(+y, +x) = -z, against +v,
        // so w = -1 puts the bitangent back along +z.
        for t in quad_tangents([1.0, 0.0], [0.0, 1.0]) {
            assert!(close3([t[0], t[1], t[2]], [1.0, 0.0, 0.0]), "{t:?}");
            assert_eq!(t[3], -1.0);
        }
        // v along -z: the mirror image, w = +1.
        for t in quad_tangents([1.0, 0.0], [0.0, -1.0]) {
            assert!(close3([t[0], t[1], t[2]], [1.0, 0.0, 0.0]), "{t:?}");
            assert_eq!(t[3], 1.0);
        }
        // u along +z: the tangent turns with it.
        for t in quad_tangents([0.0, 1.0], [1.0, 0.0]) {
            assert!(close3([t[0], t[1], t[2]], [0.0, 0.0, 1.0]), "{t:?}");
        }
    }

    #[test]
    fn degenerate_uvs_still_give_a_unit_tangent_perpendicular_to_the_normal() {
        // Every vertex at the same UV: no area to derive from.
        for t in quad_tangents([0.0, 0.0], [0.0, 0.0]) {
            let len = (t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt();
            assert!((len - 1.0).abs() < 1e-5 && t[1].abs() < 1e-5, "{t:?}");
        }
        // ...and a normal along any axis gets a perpendicular, never a zero vector.
        for n in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.6, 0.8, 0.0]] {
            let p = perpendicular(n);
            let dot = p[0] * n[0] + p[1] * n[1] + p[2] * n[2];
            let len = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            assert!(dot.abs() < 1e-5 && (len - 1.0).abs() < 1e-5, "n {n:?} -> {p:?}");
        }
    }

    // ── Channel sampling.

    fn ramp(interpolation: Interp) -> Channel {
        Channel {
            node: "n".into(),
            interpolation,
            times: vec![1.0, 2.0, 4.0],
            values: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [3.0, 0.0, 0.0]],
        }
    }

    #[test]
    fn linear_channel_interpolates_between_keys_and_holds_outside() {
        let c = ramp(Interp::Linear);
        assert_eq!(c.sample(0.0), [0.0, 0.0, 0.0], "before the first key");
        assert_eq!(c.sample(1.0), [0.0, 0.0, 0.0]);
        assert_eq!(c.sample(1.5), [0.5, 0.0, 0.0]);
        assert_eq!(c.sample(2.0), [1.0, 0.0, 0.0]);
        assert_eq!(c.sample(3.0), [2.0, 0.0, 0.0]);
        assert_eq!(c.sample(4.0), [3.0, 0.0, 0.0]);
        assert_eq!(c.sample(9.0), [3.0, 0.0, 0.0], "after the last key");
    }

    #[test]
    fn step_channel_holds_each_key_until_the_next() {
        let c = ramp(Interp::Step);
        assert_eq!(c.sample(1.5), [0.0, 0.0, 0.0]);
        assert_eq!(c.sample(2.0), [1.0, 0.0, 0.0]);
        assert_eq!(c.sample(3.99), [1.0, 0.0, 0.0]);
        assert_eq!(c.sample(4.0), [3.0, 0.0, 0.0]);
    }

    #[test]
    fn animation_clamps_to_its_duration_and_knows_its_nodes() {
        let a =
            Animation { name: "clip".into(), duration: 4.0, channels: vec![ramp(Interp::Linear)] };
        assert_eq!(a.translation("n", -1.0), Some([0.0, 0.0, 0.0]));
        assert_eq!(a.translation("n", 100.0), Some([3.0, 0.0, 0.0]));
        assert_eq!(a.translation("other", 1.0), None);
        assert_eq!(a.nodes().collect::<Vec<_>>(), vec!["n"]);
    }

    /// A time that is not a number, or infinite, must not wrap an index: NaN is the rest
    /// pose, the infinities are the ends.
    #[test]
    fn animation_survives_nan_and_infinite_times() {
        let a =
            Animation { name: "clip".into(), duration: 4.0, channels: vec![ramp(Interp::Linear)] };
        assert_eq!(a.translation("n", f32::NAN), Some([0.0, 0.0, 0.0]));
        assert_eq!(a.translation("n", f32::INFINITY), Some([3.0, 0.0, 0.0]));
        assert_eq!(a.translation("n", f32::NEG_INFINITY), Some([0.0, 0.0, 0.0]));
        // And the channel itself, which a clip could hand an unclamped value.
        assert_eq!(ramp(Interp::Step).sample(f32::NAN), [0.0, 0.0, 0.0]);
        // A clip whose keys start late is as long as its last key: no first-key subtraction.
        assert_eq!(a.duration(), 4.0);
    }

    #[test]
    fn normal_scale_is_baked_about_flat() {
        let px = |r: u8, g: u8| Some(image::Rgba([r, g, 255, 255]));
        // Scale 1: bit for bit.
        assert_eq!(normal_xy(px(37, 200), 0, 1.0), 37);
        assert_eq!(normal_xy(px(37, 200), 1, 1.0), 200);
        // Scale 0: flat, whatever the map says.
        assert_eq!(normal_xy(px(37, 200), 0, 0.0), 128);
        assert_eq!(normal_xy(px(0, 255), 1, 0.0), 128);
        // Scale 0.5 halves the tilt; 2 doubles it and saturates.
        assert_eq!(normal_xy(px(255, 0), 0, 0.5), 191);
        assert_eq!(normal_xy(px(255, 0), 1, 0.5), 64);
        assert_eq!(normal_xy(px(200, 0), 0, 2.0), 255);
        // No map: flat.
        assert_eq!(normal_xy(None, 0, 1.0), 128);
    }

    #[test]
    fn metallic_patterns_match_exactly_or_by_prefix() {
        assert!(pattern_matches("Bush_*", "Bush_Texture_1"));
        assert!(pattern_matches("Thick_Moss", "Thick_Moss"));
        assert!(!pattern_matches("Bush_*", "Grass_Realistic_1"));
        assert!(!pattern_matches("Bush", "Bush_Texture_1"), "no star, no prefix");
        assert!(!pattern_matches("Thick_Moss", "Thick_Moss_2"));
    }

    /// The rig carries a local displacement through the parent's linear part and the fit.
    #[test]
    fn node_delta_applies_the_parent_chain_and_the_fit_scale() {
        let mut parent = IDENT;
        // Parent: scale 0.01 and a swap of y and z, like Sketchfab's FBX wrapper nodes.
        parent[0] = [0.01, 0.0, 0.0, 0.0];
        parent[1] = [0.0, 0.0, 0.01, 0.0];
        parent[2] = [0.0, -0.01, 0.0, 0.0];
        parent[3] = [7.0, 8.0, 9.0, 1.0];
        let rig = Rig {
            animations: vec![Animation {
                name: "clip".into(),
                duration: 4.0,
                channels: vec![Channel {
                    node: "n".into(),
                    interpolation: Interp::Linear,
                    times: vec![0.0, 4.0],
                    values: vec![[10.0, 20.0, 30.0], [110.0, 20.0, 30.0]],
                }],
            }],
            nodes: HashMap::from([(
                "n".into(),
                NodeRest { translation: [10.0, 20.0, 30.0], parent },
            )]),
            fit_scale: 2.0,
        };
        let a = rig.animation("clip").unwrap();
        let at = |t: f32| rig.node_delta(a, "n", t).unwrap();
        assert!(at(0.0).mag() < 1e-6, "at rest at t = 0");
        // +100 along the node's local x, through 0.01 and the fit's 2: +2 along world x.
        let d = at(4.0);
        assert!((d - Vector3::new(2.0, 0.0, 0.0)).mag() < 1e-5, "{d:?}");
        // A displacement is never translated by the parent.
        assert!((at(2.0) - Vector3::new(1.0, 0.0, 0.0)).mag() < 1e-5);
        assert!(rig.node_delta(a, "absent", 1.0).is_none());
    }

    // ── The shipped files, parsed without a context.

    /// The whole file as one part, at the file's own scale.
    const WHOLE: [PartSpec<'static>; 1] = [PartSpec {
        name: "all",
        roots: &[],
        skip: &[],
        frame: Frame::Scene,
        anchor: Anchor::Hinge,
    }];

    fn whole(path: &'static str, translucent: &'static [&'static str]) -> Parsed {
        let spec = Load {
            path,
            parts: &WHOLE,
            fit: Fit::Identity,
            max_map: 1024,
            translucent,
            metallic_override: &[],
            cut_boxes: &[],
        };
        parse(&spec).unwrap_or_else(|e| panic!("{e}"))
    }

    fn triangles(p: &Parsed, part: &str) -> usize {
        p.raw[part].iter().map(|r| r.front / 3).sum()
    }

    fn material_index(p: &Parsed, name: &str) -> usize {
        p.doc.materials().position(|m| m.name() == Some(name)).unwrap_or_else(|| panic!("{name}"))
    }

    fn assert_bounds(got: [f32; 6], want: [f32; 6], tol: f32) {
        for k in 0..6 {
            assert!((got[k] - want[k]).abs() < tol, "bounds {got:?}, expected {want:?}");
        }
    }

    #[test]
    fn elevator_parses_with_its_doors_and_their_clip() {
        let p = whole("Meshes/elevator_with_animation_lowpoly.glb", &[]);
        assert_eq!(triangles(&p, "all"), 1054);
        assert_bounds(p.bounds["all"], [-3.05, 1.35, -0.08, 3.21, -1.49, 1.21], 0.02);
        // No alpha anywhere; every material opaque and none tiling.
        assert!(p.policies.iter().all(|m| m.alpha_cutoff < 0.0 && !m.translucent));

        let a = p.rig.animation("Doors open").expect("the clip");
        assert!((a.duration() - 10.42).abs() < 0.05, "{}", a.duration());
        let mut nodes: Vec<&str> = a.nodes().collect();
        nodes.sort_unstable();
        assert_eq!(nodes, ["Door1", "Door2"]);

        // At rest at the start of the clip...
        for door in ["Door1", "Door2"] {
            let d0 = p.rig.node_delta(a, door, 0.0).unwrap();
            assert!(d0.mag() < 1e-4, "{door} starts {d0:?} from rest");
        }
        // ...wide open near four seconds: a horizontal slide of well over a metre for the
        // outer leaf, in the model's metres, and a shorter one for the inner -- the two
        // telescope -- and closed again by the end.
        let open = p.rig.node_delta(a, "Door2", 3.9).unwrap();
        assert!(open.y.abs() < 1e-3 && open.mag() > 1.5, "Door2 open: {open:?}");
        let inner = p.rig.node_delta(a, "Door1", 3.9).unwrap();
        assert!(inner.y.abs() < 1e-3 && inner.mag() > 0.8 && inner.mag() < open.mag());
        assert!(open.x < 0.0 && inner.x < 0.0, "both slide the same way");
        let end = p.rig.node_delta(a, "Door2", a.duration()).unwrap();
        assert!(end.mag() < 0.05, "closed again: {end:?}");
        // Outside the clip the ends hold.
        let at = |t: f32| p.rig.node_delta(a, "Door2", t).unwrap();
        assert!((at(-5.0) - at(0.0)).mag() < 1e-6);
        assert!((at(99.0) - at(a.duration())).mag() < 1e-6);
        assert!(p.rig.node_delta(a, "Wall", 1.0).is_none(), "the wall does not move");
    }

    /// A door leaf gathered on its own under `Frame::Scene` lands where the whole model has
    /// it: inside the elevator's bounds, the height of an elevator door, in metres.
    #[test]
    fn elevator_door_part_is_at_rest_in_the_models_space() {
        const PARTS: [PartSpec<'static>; 2] = [
            PartSpec {
                name: "body",
                roots: &[],
                skip: &["Door1"],
                frame: Frame::Scene,
                anchor: Anchor::Hinge,
            },
            PartSpec {
                name: "door1",
                roots: &["Door1"],
                skip: &[],
                frame: Frame::Scene,
                anchor: Anchor::Hinge,
            },
        ];
        let spec = Load {
            path: "Meshes/elevator_with_animation_lowpoly.glb",
            parts: &PARTS,
            fit: Fit::Identity,
            max_map: 1024,
            translucent: &[],
            metallic_override: &[],
            cut_boxes: &[],
        };
        let p = parse(&spec).unwrap_or_else(|e| panic!("{e}"));
        // The leaf's triangles are in one part or the other, never both.
        assert_eq!(triangles(&p, "body") + triangles(&p, "door1"), 1054);
        assert!(triangles(&p, "door1") > 0);
        let (all, door) = (p.bounds["body"], p.bounds["door1"]);
        for k in 0..3 {
            assert!(door[2 * k] >= all[2 * k] - 1e-3 && door[2 * k + 1] <= all[2 * k + 1] + 1e-3);
        }
        let height = door[3] - door[2];
        assert!((2.0..3.0).contains(&height), "door leaf {height} m tall");
        // Sliding it by the clip's open delta keeps it inside the model too: the leaf goes
        // behind the wall, not out of the building.
        let a = p.rig.animation("Doors open").unwrap();
        let d = p.rig.node_delta(a, "Door1", 3.9).unwrap();
        assert!(door[0] + d.x >= all[0] - 1e-3 && door[1] + d.x <= all[1] + 1e-3, "{d:?}");
    }

    #[test]
    fn overgrown_room_parses_with_tested_foliage_and_tiling_moss() {
        let p = whole("Meshes/backrooms_room_with_plants_overgrown.glb", &[]);
        assert_eq!(triangles(&p, "all"), 8566);
        let b = p.bounds["all"];
        assert!((b[1] - b[0] - 21.2).abs() < 0.1 && (b[3] - b[2] - 2.5).abs() < 0.1);
        assert!((b[5] - b[4] - 21.3).abs() < 0.1);
        assert!(p.rig.animations.is_empty());
        // BLEND foliage is alpha-tested at 0.5, not blended.
        let bush = &p.policies[material_index(&p, "Bush_Texture_1")];
        assert!((bush.alpha_cutoff - 0.5).abs() < 1e-6 && !bush.translucent && bush.keeps_alpha);
        // The moss's x3 texture transform is baked into its UVs, which now tile.
        let moss = material_index(&p, "Thick_Moss");
        assert!(p.policies[moss].tiles);
        let max_uv = p.raw["all"]
            .iter()
            .filter(|r| r.material == moss)
            .flat_map(|r| r.uv.iter())
            .fold(f32::MIN, |m, uv| m.max(uv[0]).max(uv[1]));
        assert!(max_uv > 1.5, "moss UVs reach {max_uv}");
        // The wallpaper has no transform and stays inside its map.
        assert!(!p.policies[material_index(&p, "Backrooms_Wallpaper")].tiles);
    }

    #[test]
    fn flooded_complex_parses_with_translucent_water_when_asked() {
        let p = whole("Meshes/level_37_flooded_tiled_complex.glb", &["Water.002"]);
        assert_eq!(triangles(&p, "all"), 15095);
        let b = p.bounds["all"];
        assert!((b[1] - b[0] - 36.9).abs() < 0.1 && (b[3] - b[2] - 14.5).abs() < 0.1);
        assert!((b[5] - b[4] - 33.4).abs() < 0.1);
        let water = material_index(&p, "Water.002");
        assert_eq!(
            p.doc.materials().nth(water).unwrap().alpha_mode(),
            gltf::material::AlphaMode::Blend
        );
        let w = &p.policies[water];
        assert!(w.translucent && w.alpha_cutoff < 0.0 && w.keeps_alpha);
        // The marble's x5 transform tiles; the tiles material does not.
        assert!(p.policies[material_index(&p, "Marble_8")].tiles);
        assert!(!p.policies[material_index(&p, "Bath_Tile")].tiles);

        // Not asked for: the water is alpha-tested like any other BLEND material.
        let p = whole("Meshes/level_37_flooded_tiled_complex.glb", &[]);
        let w = &p.policies[water];
        assert!(!w.translucent && (w.alpha_cutoff - 0.5).abs() < 1e-6);
    }

    /// The smallest GLB there is: one triangle, no materials, no indices. What an exporter
    /// writes for a bare mesh, and what used to alias material 0 -- which here does not
    /// exist.
    fn bare_triangle_glb() -> Vec<u8> {
        triangle_glb(false)
    }

    /// The same triangle, `blended` giving it the file's one material, a `BLEND` one.
    fn triangle_glb(blended: bool) -> Vec<u8> {
        let (materials, material) = if blended {
            (r#","materials":[{"name":"glass","alphaMode":"BLEND"}]"#, r#","material":0"#)
        } else {
            ("", "")
        };
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0,"name":"tri"}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}}{material}}}]}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}}],"bufferViews":[{{"buffer":0,"byteLength":36}}],"buffers":[{{"byteLength":36}}]{materials}}}"#
        );
        let mut json = json.into_bytes();
        while json.len() % 4 != 0 {
            json.push(b' ');
        }
        let mut bin = Vec::new();
        for v in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            bin.extend_from_slice(&v.to_le_bytes());
        }
        let total = 12 + 8 + json.len() + 8 + bin.len();
        let mut out = Vec::new();
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(json.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&json);
        out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&bin);
        out
    }

    #[test]
    fn a_primitive_without_a_material_gets_the_default_one() {
        let spec = Load {
            path: "bare.glb",
            parts: &WHOLE,
            fit: Fit::Identity,
            max_map: 1024,
            translucent: &[],
            metallic_override: &[],
            cut_boxes: &[],
        };
        let p = parse_bytes(&bare_triangle_glb(), &spec, "bare.glb".into())
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(p.doc.materials().count(), 0);
        // The one primitive names the slot past the file's (none) materials, and a policy
        // was built for it: opaque, as the spec's default material is.
        assert_eq!(p.raw["all"].len(), 1);
        assert_eq!(p.raw["all"][0].material, 0);
        assert_eq!(p.policies.len(), 1);
        assert!(p.policies[0].alpha_cutoff < 0.0 && !p.policies[0].translucent);
        assert!(!p.policies[0].keeps_alpha);
        // It is PBR (not unlit), and has the default's maps: none.
        let m = material(&p.doc, 0).expect("the default");
        assert!(m.index().is_none() && !m.unlit());
        assert_eq!(material_sources(&m), [None; 5]);
        // A file whose primitives all name a material builds no default slot.
        let q = whole("Meshes/backrooms_room_with_plants_overgrown.glb", &[]);
        assert_eq!(q.policies.len(), q.doc.materials().count());
    }

    /// A file whose only material is `BLEND` -- a glass or a bush on its own -- has every
    /// triangle drawn and none solid: what `GltfProp` builds no collider from, and what
    /// `--view-glb` must still open.
    #[test]
    fn a_file_of_nothing_solid_has_no_solid_triangles() {
        let spec = Load {
            path: "glass.glb",
            parts: &WHOLE,
            fit: Fit::Identity,
            max_map: 1024,
            translucent: &[],
            metallic_override: &[],
            cut_boxes: &[],
        };
        let p = parse_bytes(&triangle_glb(true), &spec, "glass.glb".into())
            .unwrap_or_else(|e| panic!("{e}"));
        let g = gather(&p.raw["all"], &p.policies);
        assert_eq!((g.pos.len(), g.idx.len()), (3, 3));
        assert!(g.solid.is_empty());
    }

    /// The elevator embeds both formats: nine images, JPEG and PNG (two of them paletted),
    /// every one decoded to RGBA at its own size under a cap it fits in, and shrunk about its
    /// longer side under one it does not.
    #[test]
    fn decode_maps_reads_the_elevators_jpeg_and_png_images() {
        let path = assets::path("Meshes/elevator_with_animation_lowpoly.glb");
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let gltf = gltf::Gltf::from_slice(&bytes).unwrap_or_else(|e| panic!("{e}"));
        let blob = gltf.blob.as_deref().expect("BIN");
        let doc = &gltf.document;
        let kinds: Vec<&str> = doc
            .images()
            .map(|i| match i.source() {
                gltf::image::Source::View { mime_type, .. } => mime_type,
                gltf::image::Source::Uri { .. } => "uri",
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "image/jpeg",
                "image/jpeg",
                "image/jpeg",
                "image/jpeg",
                "image/png",
                "image/jpeg",
                "image/png",
                "image/png",
                "image/jpeg"
            ]
        );
        let maps = decode_maps(doc, blob, 1024).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(maps.len(), 9, "every image is referenced by a material");
        let sizes = [
            (1024, 512),
            (256, 128),
            (512, 256),
            (512, 1024),
            (512, 1024),
            (512, 1024),
            (512, 256),
            (512, 512),
            (256, 256),
        ];
        for (i, &size) in sizes.iter().enumerate() {
            assert_eq!(maps[&i].dimensions(), size, "image {i}");
        }
        // The paletted PNG (image 7, the wallpaper) expanded to straight RGBA: not one colour.
        let wallpaper = &maps[&7];
        let first = wallpaper.get_pixel(0, 0);
        assert!(wallpaper.pixels().any(|p| p != first));
        // Under a smaller cap the oversize maps shrink about their longer side and the rest
        // are untouched.
        let small = decode_maps(doc, blob, 256).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(small[&0].dimensions(), (256, 128));
        assert_eq!(small[&3].dimensions(), (128, 256));
        assert_eq!(small[&1].dimensions(), (256, 128));
        assert_eq!(small[&8].dimensions(), (256, 256));
    }

    /// The solid subset leaves out what is alpha-tested or translucent: the pool's water, the
    /// overgrown room's foliage cards.
    #[test]
    fn solid_triangles_leave_out_water_and_foliage() {
        let p = whole("Meshes/level_37_flooded_tiled_complex.glb", &["Water.002"]);
        let g = gather(&p.raw["all"], &p.policies);
        assert_eq!(g.idx.len() / 3, 15095);
        assert_eq!(g.solid.len() / 3, 15095 - 10, "the two water sheets are ten triangles");
        // Water alpha-tested instead (not named) is still not solid.
        let p = whole("Meshes/level_37_flooded_tiled_complex.glb", &[]);
        let g = gather(&p.raw["all"], &p.policies);
        assert_eq!(g.solid.len() / 3, 15095 - 10);

        let p = whole("Meshes/backrooms_room_with_plants_overgrown.glb", &[]);
        let g = gather(&p.raw["all"], &p.policies);
        let foliage: usize = p.raw["all"]
            .iter()
            .filter(|r| p.policies[r.material].alpha_cutoff >= 0.0)
            .map(|r| r.front / 3)
            .sum();
        assert!(foliage > 7000, "the grass and bushes are most of the file: {foliage}");
        assert_eq!(g.solid.len() / 3, 8566 - foliage);
        // The elevator is all opaque: nothing left out.
        let p = whole("Meshes/elevator_with_animation_lowpoly.glb", &[]);
        let g = gather(&p.raw["all"], &p.policies);
        assert_eq!(g.solid.len(), g.idx.len());
    }

    #[test]
    fn metallic_override_names_materials_by_pattern() {
        let spec = Load {
            path: "Meshes/backrooms_room_with_plants_overgrown.glb",
            parts: &WHOLE,
            fit: Fit::Identity,
            max_map: 1024,
            translucent: &[],
            metallic_override: &[("Bush_*", 0.0), ("Thick_Moss", 0.25)],
            cut_boxes: &[],
        };
        let p = parse(&spec).unwrap_or_else(|e| panic!("{e}"));
        let at = |name: &str| p.policies[material_index(&p, name)].metallic;
        // The file's own factors: bushes 1 and 2 ship none, so the spec's default of 1.0 --
        // the gotcha the override exists for.
        let bush = p.doc.materials().find(|m| m.name() == Some("Bush_Texture_1")).unwrap();
        assert_eq!(bush.pbr_metallic_roughness().metallic_factor(), 1.0);
        assert_eq!(at("Bush_Texture_1"), Some(0.0));
        assert_eq!(at("Bush_Texture_4"), Some(0.0));
        assert_eq!(at("Thick_Moss"), Some(0.25));
        assert_eq!(at("Grass_Realistic_1"), None);
        assert_eq!(at("Backrooms_Wallpaper"), None);
        // Nothing named: nothing overridden.
        let q = whole("Meshes/backrooms_room_with_plants_overgrown.glb", &[]);
        assert!(q.policies.iter().all(|m| m.metallic.is_none()));
    }

    #[test]
    fn a_metallic_pattern_matching_nothing_is_an_error() {
        let spec = Load {
            path: "Meshes/backrooms_room_with_plants_overgrown.glb",
            parts: &WHOLE,
            fit: Fit::Identity,
            max_map: 1024,
            translucent: &[],
            metallic_override: &[("Shrub_*", 0.0)],
            cut_boxes: &[],
        };
        let err = parse(&spec).err().expect("rejected");
        assert!(err.to_string().contains("Shrub_*"), "{err}");
    }

    #[test]
    fn a_translucent_name_the_file_lacks_is_an_error() {
        let spec = Load {
            path: "Meshes/level_37_flooded_tiled_complex.glb",
            parts: &WHOLE,
            fit: Fit::Identity,
            max_map: 1024,
            translucent: &["Water.003"],
            metallic_override: &[],
            cut_boxes: &[],
        };
        let err = parse(&spec).err().expect("rejected");
        assert!(err.to_string().contains("Water.003"), "{err}");
    }
}
