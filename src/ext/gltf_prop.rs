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
//! (`ext::trimesh`). Only solid triangles go into it (`GltfModel::solid_triangles`): a
//! foliage card or a water surface is walked through, not into.
//!
//! # Drawing
//!
//! Every part's opaque pass is drawn before any part's translucent pass, so that a pool's
//! water blends over the whole model and not just over the part it happens to belong to.

use crate::camera::Camera;
use crate::ext::gltf_model::{GltfModel, Load, Pass};
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

impl Playing {
    /// Advance the clip by `dt` seconds: past the end, a looping clip wraps (keeping the
    /// overshoot, so a 10 s clip stepped at 0.4 s does not drift) and a one-shot holds its
    /// last frame. A clip of no length stays at 0 either way.
    fn advance(&mut self, dt: f32, duration: f32) {
        self.time += dt;
        if self.time > duration {
            self.time =
                if self.looping && duration > 0.0 { self.time % duration } else { duration };
        }
    }
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
    /// part's solid triangles become one world-space collider, built here and fixed: move the
    /// prop afterwards and the collision stays behind. A model with nothing solid in it -- a
    /// file of foliage cards alone, under `--view-glb` -- gets no collider and a warning.
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
                let (p, i) = model.solid_triangles(part);
                let off = pos.len() as u32;
                pos.extend_from_slice(p);
                idx.extend(i.iter().map(|k| k + off));
            }
            if idx.is_empty() {
                log::warn!("[gltf] {} has no solid triangles: nothing to collide with", spec.path);
                return None;
            }
            Some(Rc::new(TriMeshCollider::new(&pos, &idx, &base.local_to_world())))
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
            collider: collider.flatten(),
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
        p.advance(GH_DT, duration);
    }

    fn draw(&self, ctx: &RenderCtx, cam: &Camera, _fbo: Option<glow::Framebuffer>) {
        for pass in [Pass::Opaque, Pass::Translucent] {
            for part in &self.parts {
                let obj = self.placement(part);
                if self.model.unlit(part) {
                    self.unlit.use_program();
                    self.unlit.set_vec4("fog_color", self.fog_color);
                    self.model.draw_part_pass(part, &obj, &self.unlit, cam, ctx, pass);
                } else {
                    self.model.draw_part_pass(part, &obj, &self.pbr, cam, ctx, pass);
                }
            }
        }
    }

    fn trimesh(&self) -> Option<Rc<TriMeshCollider>> {
        self.collider.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playing(looping: bool) -> Playing {
        Playing { clip: "clip".into(), time: 0.0, looping }
    }

    #[test]
    fn a_looping_clip_wraps_and_keeps_its_overshoot() {
        let mut p = playing(true);
        for _ in 0..3 {
            p.advance(0.4, 1.0);
        }
        assert!((p.time - 0.2).abs() < 1e-6, "1.2 s into a 1 s loop is 0.2 s: {}", p.time);
        // Many steps never drift past the clip.
        for _ in 0..10_000 {
            p.advance(0.013, 1.0);
            assert!((0.0..=1.0).contains(&p.time));
        }
    }

    #[test]
    fn a_one_shot_clip_holds_its_last_frame() {
        let mut p = playing(false);
        for _ in 0..5 {
            p.advance(0.4, 1.0);
        }
        assert_eq!(p.time, 1.0);
        p.advance(0.4, 1.0);
        assert_eq!(p.time, 1.0);
    }

    #[test]
    fn a_clip_of_no_length_stays_at_zero_without_dividing_by_it() {
        for looping in [true, false] {
            let mut p = playing(looping);
            p.advance(0.4, 0.0);
            assert_eq!(p.time, 0.0);
        }
    }

    #[test]
    fn stepping_at_the_fixed_rate_reaches_the_end_on_time() {
        let mut p = playing(false);
        let steps = (2.0 / GH_DT).round() as usize;
        for _ in 0..steps {
            p.advance(GH_DT, 10.0);
        }
        assert!((p.time - 2.0).abs() < 1e-3, "{}", p.time);
    }
}
