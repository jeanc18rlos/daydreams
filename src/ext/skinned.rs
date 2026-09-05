//! EXT: skinned glTF characters -- Elbolillo's rigged cast, played on the CPU. Not part
//! of the C++ port.
//!
//! `gltf_model.rs` deliberately bakes node transforms into vertices at load and keeps only
//! translation channels; skeletal animation needs the opposite (vertices in bind space,
//! posed per frame by a joint hierarchy with rotation channels), so characters get their
//! own loader rather than a fifth code path through the static one.
//!
//! # Why CPU skinning
//!
//! A character here is 700-1200 vertices of PSX-era geometry, and a scene fields a dozen
//! of them. Skinning on the CPU at a reduced cadence (about 31 Hz; see `SKIN_EVERY`) costs
//! well under a millisecond for the whole cast, and buys the one thing that matters for
//! fitting into this engine: the posed mesh is ordinary vertex data drawn by the ordinary
//! `gltfpbr` shader, so scene fog, the mood grade and the portal passes all apply without
//! one line of new GLSL, and `draw` stays `&self`-clean for portal re-entry (the upload is
//! a version-guarded `glBufferSubData`, GL state only).
//!
//! The `surface` and `light` units the PBR shader expects are fed 1x1 stand-ins (flat
//! normal, rough dielectric, no emissive, full occlusion) -- exactly what these low-poly
//! colour-mapped characters are.
//!
//! # Files
//!
//! `Meshes/mannequin.glb` -- Mixamo's Mannequin on its own 65-bone skeleton with the 31
//! clips downloaded beside it (`tools/build_mannequin.py`). The loader is asset-agnostic:
//! give it any GLB with a skin and named clips and it plays them.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use glow::HasContext;

use crate::app::assets;
use crate::app::crash::fatal;
use crate::app::error::AssetError;
use crate::camera::Camera;
use crate::game_header::GH_DT;
use crate::object::{Object, RenderCtx};
use crate::resources::Resources;
use crate::shader::Shader;
use crate::vector::Vector3;

/// Re-skin every this many 500 Hz steps: 16 steps is ~31 Hz, twice PSX cartoon rate.
const SKIN_EVERY: u32 = 16;

thread_local! {
    /// Staggers the cast: consecutive instances land in different `SKIN_EVERY`
    /// residue classes, so a street of NPCs re-poses one or two per step
    /// instead of all thirteen on the same one (a periodic hitch otherwise).
    static PHASE: Cell<u32> = const { Cell::new(0) };
}

// ─────────────────────────────────────────────────────────────────────────────
// Column-major 4x4, the glTF convention, kept apart from the engine's row-major
// Matrix4 exactly as gltf_model.rs keeps its own `M`: convert only at the end
// (here never -- skinned vertices leave in model space, plain data).
// ─────────────────────────────────────────────────────────────────────────────

type M4 = [f32; 16];

const ID4: M4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

fn m_mul(a: &M4, b: &M4) -> M4 {
    let mut o = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            o[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    o
}

fn m_point(m: &M4, p: [f32; 3]) -> [f32; 3] {
    [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
    ]
}

fn m_dir(m: &M4, p: [f32; 3]) -> [f32; 3] {
    [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2],
    ]
}

/// T * R * S from a glTF node decomposition (quaternion x,y,z,w).
fn trs(t: [f32; 3], q: [f32; 4], s: [f32; 3]) -> M4 {
    let [x, y, z, w] = q;
    let (x2, y2, z2) = (x + x, y + y, z + z);
    let (xx, yy, zz) = (x * x2, y * y2, z * z2);
    let (xy, xz, yz) = (x * y2, x * z2, y * z2);
    let (wx, wy, wz) = (w * x2, w * y2, w * z2);
    [
        (1.0 - (yy + zz)) * s[0],
        (xy + wz) * s[0],
        (xz - wy) * s[0],
        0.0,
        (xy - wz) * s[1],
        (1.0 - (xx + zz)) * s[1],
        (yz + wx) * s[1],
        0.0,
        (xz + wy) * s[2],
        (yz - wx) * s[2],
        (1.0 - (xx + yy)) * s[2],
        0.0,
        t[0],
        t[1],
        t[2],
        1.0,
    ]
}

fn nlerp(a: [f32; 4], b: [f32; 4], u: f32) -> [f32; 4] {
    // Shortest arc: a quaternion and its negation are the same rotation.
    let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    let sign = if dot < 0.0 { -1.0 } else { 1.0 };
    let mut o = [0.0; 4];
    for i in 0..4 {
        o[i] = a[i] + (b[i] * sign - a[i]) * u;
    }
    let m = (o[0] * o[0] + o[1] * o[1] + o[2] * o[2] + o[3] * o[3]).sqrt().max(1e-6);
    o.map(|c| c / m)
}

fn lerp3(a: [f32; 3], b: [f32; 3], u: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * u, a[1] + (b[1] - a[1]) * u, a[2] + (b[2] - a[2]) * u]
}

// ─────────────────────────────────────────────────────────────────────────────
// Model data
// ─────────────────────────────────────────────────────────────────────────────

struct NodeT {
    parent: Option<usize>,
    t: [f32; 3],
    r: [f32; 4],
    s: [f32; 3],
}

struct SkinT {
    /// Node index per joint, in the skin's joint order (what JOINTS_0 indexes).
    joints: Vec<usize>,
    ibm: Vec<M4>,
}

/// One clip's sampled channels, keyed by node index. The exporter runs with
/// force-sampling, so times are dense and uniform; linear interpolation between
/// neighbouring keys reproduces the authored curves.
struct Track<V> {
    times: Vec<f32>,
    values: Vec<V>,
}

impl<V: Copy> Track<V> {
    fn sample(&self, t: f32, mix: impl Fn(V, V, f32) -> V) -> V {
        let last = self.times.len() - 1;
        if t.is_nan() || t <= self.times[0] {
            return self.values[0];
        }
        if t >= self.times[last] {
            return self.values[last];
        }
        let i = self.times.partition_point(|&k| k <= t) - 1;
        let u = (t - self.times[i]) / (self.times[i + 1] - self.times[i]).max(1e-6);
        mix(self.values[i], self.values[i + 1], u)
    }
}

struct Clip {
    duration: f32,
    rot: HashMap<usize, Track<[f32; 4]>>,
    trans: HashMap<usize, Track<[f32; 3]>>,
    scale: HashMap<usize, Track<[f32; 3]>>,
}

struct PrimCpu {
    pos: Vec<[f32; 3]>,
    nrm: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    joints: Vec<[u16; 4]>,
    weights: Vec<[f32; 4]>,
    /// Source winding plus its reverse: costumes are open surfaces (a sheet, a
    /// dress) and the engine keeps CULL_FACE on.
    idx: Vec<u32>,
    image: Option<usize>,
    alpha_cutoff: f32,
}

struct Part {
    skin: usize,
    prims: Vec<PrimCpu>,
    /// Model-space bounds at rest pose, after skinning: `[min_xyz, max_xyz]`.
    bounds: [f32; 6],
}

pub struct SkinnedModel {
    gl: Rc<glow::Context>,
    path: String,
    nodes: Vec<NodeT>,
    /// Node indices, every parent before any of its children.
    order: Vec<usize>,
    skins: Vec<SkinT>,
    clips: HashMap<String, Clip>,
    parts: HashMap<String, Part>,
    /// Encoded (png/jpeg) bytes per glTF image, decoded to GL on first use.
    images: Vec<Vec<u8>>,
    textures: RefCell<HashMap<usize, glow::Texture>>,
    /// 1x1 stand-ins for the PBR shader's `surface` and `light` units.
    flat_surface: glow::Texture,
    dark_light: glow::Texture,
}

thread_local! {
    static CACHE: RefCell<HashMap<String, std::rc::Weak<SkinnedModel>>> =
        RefCell::new(HashMap::new());
}

impl SkinnedModel {
    /// Infallible like `GltfModel::acquire`, and for the same reason: a missing
    /// character file is a packaging bug, not a runtime condition.
    pub fn acquire(gl: &Rc<glow::Context>, path: &str) -> Rc<SkinnedModel> {
        if let Some(hit) = CACHE.with(|c| c.borrow().get(path).and_then(|w| w.upgrade())) {
            return hit;
        }
        let m = Rc::new(SkinnedModel::load(gl, path).unwrap_or_else(|e| fatal(&e)));
        CACHE.with(|c| c.borrow_mut().insert(path.to_string(), Rc::downgrade(&m)));
        m
    }

    pub fn load(gl: &Rc<glow::Context>, path: &str) -> Result<SkinnedModel, AssetError> {
        let full = assets::path(path);
        let bad = |reason: String| AssetError::Gltf { path: full.clone(), reason };
        let bytes =
            std::fs::read(&full).map_err(|source| AssetError::Io { path: full.clone(), source })?;
        let gltf = gltf::Gltf::from_slice(&bytes).map_err(|e| bad(format!("parse: {e}")))?;
        let blob = gltf.blob.clone().ok_or_else(|| bad("no BIN chunk".to_string()))?;
        let doc = gltf.document;

        // ── nodes and a parent-first ordering
        let mut nodes: Vec<NodeT> = doc
            .nodes()
            .map(|n| {
                let (t, r, s) = n.transform().decomposed();
                NodeT { parent: None, t, r, s }
            })
            .collect();
        for n in doc.nodes() {
            for c in n.children() {
                nodes[c.index()].parent = Some(n.index());
            }
        }
        let mut order = Vec::with_capacity(nodes.len());
        let mut queue: Vec<usize> =
            (0..nodes.len()).filter(|&i| nodes[i].parent.is_none()).collect();
        while let Some(i) = queue.pop() {
            order.push(i);
            queue.extend(
                doc.nodes().nth(i).into_iter().flat_map(|n| n.children().map(|c| c.index())),
            );
        }

        // ── skins
        let skins: Vec<SkinT> = doc
            .skins()
            .map(|s| {
                let joints: Vec<usize> = s.joints().map(|j| j.index()).collect();
                let reader = s.reader(|_| Some(blob.as_slice()));
                let ibm: Vec<M4> = match reader.read_inverse_bind_matrices() {
                    Some(it) => it
                        .map(|m| {
                            let mut o = [0.0; 16];
                            for c in 0..4 {
                                for r in 0..4 {
                                    o[c * 4 + r] = m[c][r];
                                }
                            }
                            o
                        })
                        .collect(),
                    None => vec![ID4; joints.len()],
                };
                SkinT { joints, ibm }
            })
            .collect();

        // ── clips
        let mut clips = HashMap::new();
        for a in doc.animations() {
            let name = a.name().unwrap_or("clip").to_string();
            let mut clip = Clip {
                duration: 0.0,
                rot: HashMap::new(),
                trans: HashMap::new(),
                scale: HashMap::new(),
            };
            for ch in a.channels() {
                let node = ch.target().node().index();
                let reader = ch.reader(|_| Some(blob.as_slice()));
                let times: Vec<f32> = match reader.read_inputs() {
                    Some(t) => t.collect(),
                    None => continue,
                };
                if times.is_empty() {
                    continue;
                }
                clip.duration = clip.duration.max(*times.last().unwrap());
                match reader.read_outputs() {
                    Some(gltf::animation::util::ReadOutputs::Rotations(r)) => {
                        let values: Vec<[f32; 4]> = r.into_f32().collect();
                        clip.rot.insert(node, Track { times, values });
                    }
                    Some(gltf::animation::util::ReadOutputs::Translations(t)) => {
                        let values: Vec<[f32; 3]> = t.collect();
                        clip.trans.insert(node, Track { times, values });
                    }
                    Some(gltf::animation::util::ReadOutputs::Scales(s)) => {
                        let values: Vec<[f32; 3]> = s.collect();
                        clip.scale.insert(node, Track { times, values });
                    }
                    _ => {}
                }
            }
            clips.insert(name, clip);
        }

        // ── images (bytes only; GL upload is lazy, most of the 113 crowd
        // textures are never instantiated in a given scene)
        let images: Vec<Vec<u8>> = doc
            .images()
            .map(|img| match img.source() {
                gltf::image::Source::View { view, .. } => {
                    let s = view.offset();
                    match blob.get(s..s + view.length()) {
                        Some(bytes) => bytes.to_vec(),
                        None => {
                            log::warn!("[skinned] {path}: image view past the BIN chunk");
                            Vec::new()
                        }
                    }
                }
                gltf::image::Source::Uri { .. } => Vec::new(),
            })
            .collect();

        // ── skinned parts
        let mut parts = HashMap::new();
        for node in doc.nodes() {
            let (Some(mesh), Some(skin)) = (node.mesh(), node.skin()) else { continue };
            let name = node.name().unwrap_or("part").to_string();
            let mut prims = Vec::new();
            for p in mesh.primitives() {
                if p.mode() != gltf::mesh::Mode::Triangles {
                    continue;
                }
                let r = p.reader(|_| Some(blob.as_slice()));
                let Some(pos) = r.read_positions().map(|i| i.collect::<Vec<_>>()) else {
                    continue;
                };
                let n = pos.len();
                let nrm: Vec<[f32; 3]> = match r.read_normals() {
                    Some(i) => i.collect(),
                    None => vec![[0.0, 1.0, 0.0]; n],
                };
                let uv: Vec<[f32; 2]> = match r.read_tex_coords(0) {
                    Some(i) => i.into_f32().collect(),
                    None => vec![[0.0, 0.0]; n],
                };
                let joints: Vec<[u16; 4]> = match r.read_joints(0) {
                    Some(i) => i.into_u16().collect(),
                    None => vec![[0; 4]; n],
                };
                let weights: Vec<[f32; 4]> = match r.read_weights(0) {
                    Some(i) => i.into_f32().collect(),
                    None => vec![[1.0, 0.0, 0.0, 0.0]; n],
                };
                let front: Vec<u32> = match r.read_indices() {
                    Some(i) => i.into_u32().collect(),
                    None => (0..n as u32).collect(),
                };
                let mut idx = front.clone();
                for t in front.chunks_exact(3) {
                    idx.extend_from_slice(&[t[0], t[2], t[1]]);
                }
                let mat = p.material();
                let image = mat
                    .pbr_metallic_roughness()
                    .base_color_texture()
                    .map(|t| t.texture().source().index());
                let alpha_cutoff = match mat.alpha_mode() {
                    gltf::material::AlphaMode::Mask => mat.alpha_cutoff().unwrap_or(0.5),
                    _ => -1.0,
                };
                prims.push(PrimCpu { pos, nrm, uv, joints, weights, idx, image, alpha_cutoff });
            }
            if prims.is_empty() {
                continue;
            }
            parts.insert(name, Part { skin: skin.index(), prims, bounds: [0.0; 6] });
        }
        if parts.is_empty() {
            return Err(bad("no skinned parts".to_string()));
        }

        let (flat_surface, dark_light) = unsafe {
            (
                tex1x1(gl, [128, 128, 204, 0]).map_err(AssetError::Gl)?,
                tex1x1(gl, [0, 0, 0, 255]).map_err(AssetError::Gl)?,
            )
        };

        let mut model = SkinnedModel {
            gl: gl.clone(),
            path: path.to_string(),
            nodes,
            order,
            skins,
            clips,
            parts,
            images,
            textures: RefCell::new(HashMap::new()),
            flat_surface,
            dark_light,
        };

        // Bounds per part in the REST pose, through the same path an instance
        // skins with. The rest pose is authoritative again now that the rig
        // comes from Mixamo's auto-rigger: the bind pose IS the character
        // standing as the artist modelled it (tools/rig_from_mixamo.py).
        let globals = model.pose_globals(None, 0.0);
        let names: Vec<String> = model.parts.keys().cloned().collect();
        for name in names {
            let palette = model.palette(model.parts[&name].skin, &globals);
            let mut lo = [f32::MAX; 3];
            let mut hi = [f32::MIN; 3];
            for prim in &model.parts[&name].prims {
                for (i, p) in prim.pos.iter().enumerate() {
                    let v = skin_one(&palette, prim.joints[i], prim.weights[i], *p, m_point);
                    for k in 0..3 {
                        lo[k] = lo[k].min(v[k]);
                        hi[k] = hi[k].max(v[k]);
                    }
                }
            }
            let part = model.parts.get_mut(&name).unwrap();
            part.bounds = [lo[0], hi[0], lo[1], hi[1], lo[2], hi[2]];
        }
        log::info!(
            "[skinned] {} loaded: {} parts, {} clips, {} joints max",
            path,
            model.parts.len(),
            model.clips.len(),
            model.skins.iter().map(|s| s.joints.len()).max().unwrap_or(0)
        );
        Ok(model)
    }

    /// The named part, or the part whose name starts with `name` -- the packs
    /// suffix node names (`Character_01_Character_01_0`), and callers say
    /// `Character_01_`.
    fn resolve(&self, name: &str) -> Option<&str> {
        // Shortest match wins, so an exact key beats exact-plus-suffix and
        // `Character_1_` cannot steal `Character_1_`'s tail from `_16_`.
        let mut best: Option<&str> = None;
        for k in self.parts.keys() {
            if k.starts_with(name) && best.is_none_or(|b| k.len() < b.len()) {
                best = Some(k);
            }
        }
        best
    }

    /// Every node's model-space matrix under `clip` at `t` (rest pose when `None`).
    fn pose_globals(&self, clip: Option<&Clip>, t: f32) -> Vec<M4> {
        let mut globals = vec![ID4; self.nodes.len()];
        for &i in &self.order {
            let n = &self.nodes[i];
            let (mut tr, mut ro, mut sc) = (n.t, n.r, n.s);
            if let Some(c) = clip {
                if let Some(track) = c.trans.get(&i) {
                    tr = track.sample(t, lerp3);
                }
                if let Some(track) = c.rot.get(&i) {
                    ro = track.sample(t, nlerp);
                }
                if let Some(track) = c.scale.get(&i) {
                    sc = track.sample(t, lerp3);
                }
            }
            let local = trs(tr, ro, sc);
            globals[i] = match n.parent {
                Some(p) => m_mul(&globals[p], &local),
                None => local,
            };
        }
        globals
    }

    fn palette(&self, skin: usize, globals: &[M4]) -> Vec<M4> {
        let s = &self.skins[skin];
        s.joints.iter().zip(&s.ibm).map(|(&j, ibm)| m_mul(&globals[j], ibm)).collect()
    }

    /// GL texture for a glTF image index, decoded and uploaded on first use.
    fn texture(&self, image: Option<usize>) -> glow::Texture {
        let Some(i) = image else { return self.flat_surface };
        if let Some(t) = self.textures.borrow().get(&i) {
            return *t;
        }
        let rgba =
            image::load_from_memory(&self.images[i]).map(|d| d.to_rgba8()).unwrap_or_else(|e| {
                log::warn!("[skinned] {}: image {i} undecodable ({e}); grey", self.path);
                image::RgbaImage::from_pixel(1, 1, image::Rgba([128, 128, 128, 255]))
            });
        let (w, h) = rgba.dimensions();
        let t =
            unsafe { tex_upload(&self.gl, &rgba, w as i32, h as i32).unwrap_or(self.flat_surface) };
        self.textures.borrow_mut().insert(i, t);
        t
    }
}

impl Drop for SkinnedModel {
    fn drop(&mut self) {
        unsafe {
            for (_, t) in self.textures.borrow_mut().drain() {
                self.gl.delete_texture(t);
            }
            self.gl.delete_texture(self.flat_surface);
            self.gl.delete_texture(self.dark_light);
        }
    }
}

fn skin_one(
    palette: &[M4],
    joints: [u16; 4],
    weights: [f32; 4],
    v: [f32; 3],
    apply: impl Fn(&M4, [f32; 3]) -> [f32; 3],
) -> [f32; 3] {
    let mut o = [0.0; 3];
    let mut total = 0.0;
    for k in 0..4 {
        let w = weights[k];
        if w <= 0.0 {
            continue;
        }
        let Some(m) = palette.get(joints[k] as usize) else { continue };
        let p = apply(m, v);
        for c in 0..3 {
            o[c] += p[c] * w;
        }
        total += w;
    }
    if total > 1e-6 {
        for c in &mut o {
            *c /= total;
        }
        o
    } else {
        v
    }
}

unsafe fn tex1x1(gl: &glow::Context, rgba: [u8; 4]) -> Result<glow::Texture, String> {
    let t = unsafe { gl.create_texture() }?;
    unsafe {
        gl.bind_texture(glow::TEXTURE_2D, Some(t));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            1,
            1,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(&rgba)),
        );
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
        gl.bind_texture(glow::TEXTURE_2D, None);
    }
    Ok(t)
}

unsafe fn tex_upload(
    gl: &glow::Context,
    rgba: &[u8],
    w: i32,
    h: i32,
) -> Result<glow::Texture, String> {
    let t = unsafe { gl.create_texture() }?;
    unsafe {
        gl.bind_texture(glow::TEXTURE_2D, Some(t));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            w,
            h,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(rgba)),
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR_MIPMAP_LINEAR as i32,
        );
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
        gl.generate_mipmap(glow::TEXTURE_2D);
        gl.bind_texture(glow::TEXTURE_2D, None);
    }
    Ok(t)
}

// ─────────────────────────────────────────────────────────────────────────────
// Instance
// ─────────────────────────────────────────────────────────────────────────────

struct PrimGpu {
    vao: glow::VertexArray,
    bufs: [glow::Buffer; 5],
    /// bufs[0] = pos (dynamic), [1] = uv, [2] = nrm (dynamic), [3] = tangent
    /// (constant), [4] = the index buffer. Order mirrors gltfpbr.vert.
    count: i32,
    tex: glow::Texture,
    alpha_cutoff: f32,
    /// (part key, prim index) back into the model's CPU data.
    src: (String, usize),
}

/// One character on screen: a set of parts from a [`SkinnedModel`], an animation
/// state, and the GPU buffers the posed vertices stream into. Owned by whatever
/// game object stands it somewhere (`ext::npc`); this type draws at any `Object`
/// it is handed and never positions itself.
pub struct Instance {
    model: Rc<SkinnedModel>,
    prims: Vec<PrimGpu>,
    shader: Rc<Shader>,
    clip: String,
    time: f32,
    speed: f32,
    tick: u32,
    /// Skinned pos/nrm per prim, staged by `update`, uploaded by the first
    /// `draw` that sees a newer version (portal passes re-draw; upload once).
    staged: RefCell<Vec<(Vec<f32>, Vec<f32>)>>,
    staged_version: Cell<u64>,
    uploaded: Cell<u64>,
    bounds: [f32; 6],
}

impl Instance {
    /// `part`: a part-name prefix for one character of the crowd file, or
    /// `None` for every skinned part (the monster files). Starts in `idle`.
    pub fn new(
        gl: &Rc<glow::Context>,
        res: &Resources,
        model: &Rc<SkinnedModel>,
        part: Option<&str>,
    ) -> Instance {
        let keys: Vec<String> = match part {
            Some(p) => model.resolve(p).map(|k| vec![k.to_string()]).unwrap_or_else(|| {
                log::warn!("[skinned] {}: no part matching {p:?}", model.path);
                Vec::new()
            }),
            None => model.parts.keys().cloned().collect(),
        };
        let mut prims = Vec::new();
        let mut staged = Vec::new();
        let mut bounds = [f32::MAX, f32::MIN, f32::MAX, f32::MIN, f32::MAX, f32::MIN];
        if keys.is_empty() {
            // A neutral human box, not the MAX/MIN sentinels: height math on an
            // unresolved part would otherwise scale by infinity (npc.rs divides
            // by these bounds), and one bad prefix in a cast table is a missing
            // character, not a broken scene.
            bounds = [-0.3, 0.3, 0.0, 1.7, -0.3, 0.3];
        }
        for key in &keys {
            let part = &model.parts[key];
            for k in 0..3 {
                bounds[k * 2] = bounds[k * 2].min(part.bounds[k * 2]);
                bounds[k * 2 + 1] = bounds[k * 2 + 1].max(part.bounds[k * 2 + 1]);
            }
            for (pi, prim) in part.prims.iter().enumerate() {
                match upload_prim(gl, model, prim) {
                    Ok(mut g) => {
                        g.src = (key.clone(), pi);
                        prims.push(g);
                        staged.push((Vec::new(), Vec::new()));
                    }
                    Err(e) => log::warn!("[skinned] {}: prim upload failed: {e}", model.path),
                }
            }
        }
        let mut inst = Instance {
            model: model.clone(),
            prims,
            shader: res.acquire_shader("gltfpbr"),
            // No clip until the owner sets one: an unset name poses the rest
            // stance, which for a Mixamo rig is the character standing.
            clip: String::new(),
            time: 0.0,
            speed: 1.0,
            tick: PHASE.with(|c| {
                let v = c.get();
                c.set(v.wrapping_add(1));
                v
            }),
            staged: RefCell::new(staged),
            staged_version: Cell::new(0),
            uploaded: Cell::new(0),
            bounds,
        };
        inst.skin_now();
        inst
    }

    /// Switch clips, keeping phase only when the clip is unchanged (so a state
    /// machine may set its clip every step without restart pops). A name the
    /// file does not carry poses the rest stance and says so once.
    pub fn set_clip(&mut self, clip: &str, speed: f32) {
        if self.clip != clip {
            if !clip.is_empty() && !self.model.clips.contains_key(clip) {
                log::warn!("[skinned] {}: no clip named {clip:?}", self.model.path);
            }
            self.clip = clip.to_string();
            self.time = 0.0;
            self.skin_now();
        }
        self.speed = speed;
    }

    /// Model-space rest bounds over this instance's parts.
    pub fn bounds(&self) -> [f32; 6] {
        self.bounds
    }

    /// Advance one fixed step; re-pose the vertices every `SKIN_EVERY` steps.
    pub fn update(&mut self) {
        let duration = self.model.clips.get(&self.clip).map_or(0.0, |c| c.duration);
        self.time += GH_DT * self.speed;
        if duration > 0.0 && self.time > duration {
            self.time %= duration;
        }
        self.tick = self.tick.wrapping_add(1);
        if self.tick % SKIN_EVERY == 0 {
            self.skin_now();
        }
    }

    fn skin_now(&mut self) {
        let clip = self.model.clips.get(&self.clip);
        let globals = self.model.pose_globals(clip, self.time);
        let mut palettes: HashMap<usize, Vec<M4>> = HashMap::new();
        let mut staged = self.staged.borrow_mut();
        for (g, slot) in self.prims.iter().zip(staged.iter_mut()) {
            let part = &self.model.parts[&g.src.0];
            let palette = palettes
                .entry(part.skin)
                .or_insert_with(|| self.model.palette(part.skin, &globals));
            let prim = &part.prims[g.src.1];
            slot.0.clear();
            slot.1.clear();
            slot.0.reserve(prim.pos.len() * 3);
            slot.1.reserve(prim.pos.len() * 3);
            for i in 0..prim.pos.len() {
                let p = skin_one(palette, prim.joints[i], prim.weights[i], prim.pos[i], m_point);
                slot.0.extend_from_slice(&p);
                let n = skin_one(palette, prim.joints[i], prim.weights[i], prim.nrm[i], m_dir);
                let m = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
                slot.1.extend_from_slice(&[n[0] / m, n[1] / m, n[2] / m]);
            }
        }
        self.staged_version.set(self.staged_version.get() + 1);
    }

    /// Draw at `obj`. `&self` like every draw in this engine (portal re-entry);
    /// the buffer upload mutates GL alone, guarded to run once per pose.
    pub fn draw(&self, obj: &Object, ctx: &RenderCtx, cam: &Camera) {
        if self.prims.is_empty() {
            return;
        }
        let local_to_world = obj.local_to_world();
        let b = &self.bounds;
        let centre = local_to_world.mul_point(Vector3::new(
            (b[0] + b[1]) / 2.0,
            (b[2] + b[3]) / 2.0,
            (b[4] + b[5]) / 2.0,
        ));
        let radius =
            0.75 * ((b[1] - b[0]).powi(2) + (b[3] - b[2]).powi(2) + (b[5] - b[4]).powi(2)).sqrt();
        let stretch = local_to_world
            .x_axis()
            .mag()
            .max(local_to_world.y_axis().mag())
            .max(local_to_world.z_axis().mag());
        if !ctx.frustum.sphere(centre, radius * stretch) {
            return;
        }

        let gl = &self.model.gl;
        if self.uploaded.get() != self.staged_version.get() {
            let staged = self.staged.borrow();
            for (g, (pos, nrm)) in self.prims.iter().zip(staged.iter()) {
                unsafe {
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(g.bufs[0]));
                    gl.buffer_sub_data_u8_slice(glow::ARRAY_BUFFER, 0, bytemuck::cast_slice(pos));
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(g.bufs[2]));
                    gl.buffer_sub_data_u8_slice(glow::ARRAY_BUFFER, 0, bytemuck::cast_slice(nrm));
                }
            }
            unsafe { gl.bind_buffer(glow::ARRAY_BUFFER, None) };
            self.uploaded.set(self.staged_version.get());
        }

        let shader = &self.shader;
        let mv = obj.world_to_local().transposed();
        let mvp = cam.matrix() * local_to_world;
        let eye = ctx.eye;
        shader.use_program();
        shader.set_mvp(Some(&mvp), Some(&mv));
        shader.set_mat4("model", &local_to_world);
        shader.set_f32("time", crate::ext::view::time());
        shader.set_vec4("cam_pos", [eye.x, eye.y, eye.z, 1.0]);
        shader.set_f32("mood", crate::ext::view::mood_for(eye));
        shader.set_vec4("glow", crate::ext::view::glow());
        shader.set_f32("detail", crate::ext::view::detail());
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

        for g in &self.prims {
            shader.set_f32("alpha_cutoff", g.alpha_cutoff);
            unsafe {
                gl.active_texture(glow::TEXTURE2);
                gl.bind_texture(glow::TEXTURE_2D, Some(self.model.dark_light));
                gl.active_texture(glow::TEXTURE1);
                gl.bind_texture(glow::TEXTURE_2D, Some(self.model.flat_surface));
                // Unit 0 last and left active, as draw_prim documents: objects
                // drawn after this one bind their texture with no active_texture.
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(g.tex));
                gl.bind_vertex_array(Some(g.vao));
                gl.draw_elements(glow::TRIANGLES, g.count, glow::UNSIGNED_INT, 0);
            }
        }
        unsafe { gl.bind_vertex_array(None) };
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        let gl = &self.model.gl;
        unsafe {
            for g in &self.prims {
                gl.delete_vertex_array(g.vao);
                for b in g.bufs {
                    gl.delete_buffer(b);
                }
            }
        }
    }
}

fn upload_prim(
    gl: &Rc<glow::Context>,
    model: &SkinnedModel,
    prim: &PrimCpu,
) -> Result<PrimGpu, String> {
    let n = prim.pos.len();
    unsafe {
        let vao = gl.create_vertex_array()?;
        gl.bind_vertex_array(Some(vao));
        let mut bufs = [gl.create_buffer()?; 5];
        for b in bufs.iter_mut().skip(1) {
            *b = gl.create_buffer()?;
        }
        // location 0: positions, streamed per pose
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(bufs[0]));
        gl.buffer_data_size(glow::ARRAY_BUFFER, (n * 12) as i32, glow::DYNAMIC_DRAW);
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 0, 0);
        // location 1: uvs, fixed
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(bufs[1]));
        gl.buffer_data_u8_slice(
            glow::ARRAY_BUFFER,
            bytemuck::cast_slice(&prim.uv),
            glow::STATIC_DRAW,
        );
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, 0, 0);
        // location 2: normals, streamed per pose
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(bufs[2]));
        gl.buffer_data_size(glow::ARRAY_BUFFER, (n * 12) as i32, glow::DYNAMIC_DRAW);
        gl.enable_vertex_attrib_array(2);
        gl.vertex_attrib_pointer_f32(2, 3, glow::FLOAT, false, 0, 0);
        // location 3: tangents. The surface unit holds a flat normal map, so any
        // orthonormal-ish constant will do; (1,0,0,1) keeps the shader honest.
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(bufs[3]));
        let tan: Vec<f32> = std::iter::repeat([1.0f32, 0.0, 0.0, 1.0]).take(n).flatten().collect();
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytemuck::cast_slice(&tan), glow::STATIC_DRAW);
        gl.enable_vertex_attrib_array(3);
        gl.vertex_attrib_pointer_f32(3, 4, glow::FLOAT, false, 0, 0);
        // indices, both windings (see PrimCpu::idx)
        gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(bufs[4]));
        gl.buffer_data_u8_slice(
            glow::ELEMENT_ARRAY_BUFFER,
            bytemuck::cast_slice(&prim.idx),
            glow::STATIC_DRAW,
        );
        gl.bind_vertex_array(None);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
        gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, None);
        Ok(PrimGpu {
            vao,
            bufs,
            count: prim.idx.len() as i32,
            tex: model.texture(prim.image),
            alpha_cutoff: prim.alpha_cutoff,
            src: (String::new(), 0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trs_translation_lands_in_the_last_column() {
        let m = trs([1.0, 2.0, 3.0], [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0]);
        assert_eq!(m_point(&m, [0.0, 0.0, 0.0]), [1.0, 2.0, 3.0]);
    }

    #[test]
    fn a_quarter_turn_about_y_sends_x_to_minus_z() {
        let s = (0.5f32).sqrt();
        let m = trs([0.0; 3], [0.0, s, 0.0, s], [1.0; 3]);
        let p = m_point(&m, [1.0, 0.0, 0.0]);
        assert!((p[0]).abs() < 1e-5 && (p[2] + 1.0).abs() < 1e-5, "{p:?}");
    }

    #[test]
    fn nlerp_walks_the_shorter_arc() {
        let a = [0.0, 0.0, 0.0, 1.0];
        let b = [0.0, 0.0, 0.0, -1.0]; // same rotation, negated
        let q = nlerp(a, b, 0.5);
        assert!((q[3] - 1.0).abs() < 1e-5, "{q:?}");
    }

    #[test]
    fn track_sampling_clamps_and_interpolates() {
        let t = Track { times: vec![0.0, 1.0], values: vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0]] };
        assert_eq!(t.sample(-1.0, lerp3)[0], 0.0);
        assert_eq!(t.sample(0.5, lerp3)[0], 1.0);
        assert_eq!(t.sample(9.0, lerp3)[0], 2.0);
        assert_eq!(t.sample(f32::NAN, lerp3)[0], 0.0);
    }

    #[test]
    fn skin_one_blends_by_weight_and_survives_zero_weights() {
        let a = trs([1.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]);
        let b = trs([3.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]);
        let palette = vec![a, b];
        let v = skin_one(&palette, [0, 1, 0, 0], [0.5, 0.5, 0.0, 0.0], [0.0; 3], m_point);
        assert!((v[0] - 2.0).abs() < 1e-5, "{v:?}");
        let v = skin_one(&palette, [0, 0, 0, 0], [0.0; 4], [7.0, 0.0, 0.0], m_point);
        assert_eq!(v, [7.0, 0.0, 0.0], "all-zero weights fall back to the bind position");
    }

    #[test]
    fn a_skinned_glb_round_trips_if_one_is_present() {
        // No cast ships yet; when one does, drop it here and this checks the
        // loader still finds a skin in it.
        let path = assets::path("Meshes/characters_rigged.glb");
        let Ok(bytes) = std::fs::read(path) else { return };
        let gltf = gltf::Gltf::from_slice(&bytes).unwrap();
        assert!(gltf.document.skins().count() >= 1, "a cast file ships skinned");
    }
}
