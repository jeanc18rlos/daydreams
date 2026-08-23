//! EXT: a glTF model as a scene object -- every part of a [`Load`] drawn at one `Object`, with
//! the shader each part's materials want, optional triangle collision, and the file's own
//! clips played on the parts that move. Not part of the C++ port.
//!
//! `Backrooms` (`ext/backrooms.rs`) is the hand-built version of this for one unlit scan. This
//! is the general one, for the PBR levels and the animated elevator: a scene names the parts
//! it wants (`PartSpec`), which of them follow an animated node, and whether the player
//! collides with the lot, and gets back something it can push into the object vector.
//!
//! # Animation
//!
//! A part that follows a node is drawn at the prop's transform plus that node's
//! [`GltfModel::node_delta`] -- a rigid slide, which is what a translation channel is. The
//! part must have been gathered under `Frame::Scene` from that node (and left out of the body
//! with `PartSpec::skip`), so that at `t = 0` it sits exactly where the body has a gap for
//! it. `play` starts a clip; `update` advances it by the fixed step, looping or holding at
//! the end.
//!
//! The collider is built from the parts at rest and does not follow the animation: an open
//! elevator door still blocks like a closed one. A scene that needs otherwise builds its own
//! (`ext::trimesh`).

use crate::camera::Camera;
use crate::ext::gltf_model::{GltfModel, Load};
use crate::ext::trimesh::TriMeshCollider;
use crate::game_header::GH_DT;
use crate::object::{Object, ObjectT, RenderCtx, UpdateCtx};
use crate::resources::Resources;
use crate::shader::Shader;
use crate::vector::Vector3;
use std::rc::Rc;

/// A part that rides an animated node.
struct Follower {
    part: String,
    node: String,
}

struct Playing {
    clip: String,
    time: f32,
    looping: bool,
}

pub struct GltfProp {
    base: Object,
    model: Rc<GltfModel>,
    /// Every part of the load, in its order; drawn at `base`.
    parts: Vec<String>,
    followers: Vec<Follower>,
    playing: Option<Playing>,
    pbr: Rc<Shader>,
    unlit: Rc<Shader>,
    /// What the unlit shader fades to with distance (see `Shaders/gltfunlit.frag`).
    fog_color: [f32; 4],
    collider: Option<Rc<TriMeshCollider>>,
}

impl GltfProp {
    /// Load `spec` and stand it at `pos` with yaw `yaw`. `followers` pairs a part name with the
    /// node whose animated translation it is drawn at (see the module docs). With `solid`, every
    /// part's triangles become one world-space collider, built here and fixed: move the prop
    /// afterwards and the collision stays behind.
    pub fn new(
        gl: &Rc<glow::Context>,
        res: &Resources,
        spec: &Load,
        pos: Vector3,
        yaw: f32,
        followers: &[(&str, &str)],
        solid: bool,
    ) -> GltfProp {
        let model = GltfModel::acquire(gl, spec);
        let mut base = Object::new();
        base.pos = pos;
        base.euler.y = yaw;
        let parts: Vec<String> = spec.parts.iter().map(|p| p.name.to_string()).collect();

        let collider = solid.then(|| {
            let mut pos: Vec<[f32; 3]> = Vec::new();
            let mut idx: Vec<u32> = Vec::new();
            for part in &parts {
                let (p, i) = model.triangles(part);
                let off = pos.len() as u32;
                pos.extend_from_slice(p);
                idx.extend(i.iter().map(|k| k + off));
            }
            Rc::new(TriMeshCollider::new(&pos, &idx, &base.local_to_world()))
        });

        GltfProp {
            base,
            model,
            parts,
            followers: followers
                .iter()
                .map(|(part, node)| Follower { part: part.to_string(), node: node.to_string() })
                .collect(),
            playing: None,
            pbr: res.acquire_shader("gltfpbr"),
            unlit: res.acquire_shader("gltfunlit"),
            fog_color: [0.0, 0.0, 0.0, 1.0],
            collider,
        }
    }

    pub fn model(&self) -> &Rc<GltfModel> {
        &self.model
    }

    /// What the unlit parts fade to with distance. Black until set.
    pub fn set_fog_color(&mut self, color: [f32; 4]) {
        self.fog_color = color;
    }

    /// Start the named clip from its beginning; `looping` restarts it at the end, otherwise it
    /// holds its last frame. `false` when the file has no such clip, and nothing plays.
    pub fn play(&mut self, clip: &str, looping: bool) -> bool {
        if self.model.animation(clip).is_none() {
            self.playing = None;
            return false;
        }
        self.playing = Some(Playing { clip: clip.to_string(), time: 0.0, looping });
        true
    }

    /// The `Object` a part is drawn at: `base`, slid by its node's animated delta if it follows
    /// one and a clip is playing.
    fn placement(&self, part: &str) -> Object {
        let mut obj = Object::new();
        obj.pos = self.base.pos;
        obj.euler = self.base.euler;
        obj.scale = self.base.scale;
        let Some(playing) = &self.playing else { return obj };
        let Some(f) = self.followers.iter().find(|f| f.part == part) else { return obj };
        let delta = self
            .model
            .animation(&playing.clip)
            .and_then(|a| self.model.node_delta(a, &f.node, playing.time))
            .unwrap_or_else(Vector3::zero);
        // The delta is in the model's object space; the prop's yaw and scale carry it to the
        // world like any other local direction.
        obj.pos += self.base.local_to_world().mul_direction(delta);
        obj
    }
}

impl ObjectT for GltfProp {
    fn base(&self) -> &Object {
        &self.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base
    }

    fn update(&mut self, _ctx: &UpdateCtx) {
        let Some(p) = &mut self.playing else { return };
        let duration = self.model.animation(&p.clip).map_or(0.0, |a| a.duration());
        p.time += GH_DT;
        if p.time > duration {
            p.time = if p.looping && duration > 0.0 { p.time % duration } else { duration };
        }
    }

    fn draw(&self, ctx: &RenderCtx, cam: &Camera, _fbo: Option<glow::Framebuffer>) {
        for part in &self.parts {
            let obj = self.placement(part);
            if self.model.unlit(part) {
                self.unlit.use_program();
                self.unlit.set_vec4("fog_color", self.fog_color);
                self.model.draw_part(part, &obj, &self.unlit, cam, ctx);
            } else {
                self.model.draw_part(part, &obj, &self.pbr, cam, ctx);
            }
        }
    }

    fn trimesh(&self) -> Option<Rc<TriMeshCollider>> {
        self.collider.clone()
    }
}
