//! Port of Object.h / Object.cpp.

use std::rc::Rc;

use crate::camera::Camera;
use crate::mesh::Mesh;
use crate::shader::Shader;
use crate::texture::Texture;
use crate::vector::{Matrix4, Vector3};

// PORT: the C++ globals GH_ENGINE / GH_INPUT (GameHeader.h:48-50) are not ported as
// statics; they are threaded through as these two context structs instead.
pub struct RenderCtx<'a> {
    pub gl: &'a glow::Context,
    pub engine: &'a crate::engine::Engine,
    // EXT: this pass's view frustum and eye position, computed once in Engine::render. Every
    // draw used to invert the camera's world_view for the eye on its own (one 4x4 inverse per
    // object per pass), and nothing culled; see src/ext/cull.rs.
    pub frustum: crate::ext::cull::Frustum,
    pub eye: Vector3,
    // EXT: the engine's portal framebuffers, one per recursion level; `Portal::draw` takes
    // `[rec_level - 1]`. Shared rather than per-portal -- see `Engine::portal_fbos`.
    pub portal_fbos: &'a [crate::frame_buffer::FrameBuffer],
}

pub struct UpdateCtx<'a> {
    pub input: &'a crate::input::Input,
    // EXT: the player's eye transform, sampled once before the update loop. Room-logic objects
    // (src/ext/room.rs) need to know where the player is looking from, and cannot borrow the
    // player themselves -- the player is in the same object vector being iterated, so borrowing
    // it during the loop would alias its RefCell.
    pub cam_to_world: crate::vector::Matrix4,
    pub player_pos: crate::vector::Vector3,
}

// PORT: `typedef std::vector<std::shared_ptr<Object>> PObjectVec` (Object.h:46) lives in
// scene.rs, next to PPortalVec, so that the two aliases sit together.

pub struct Object {
    pub pos: Vector3,
    pub euler: Vector3,
    pub scale: Vector3,

    // Physical scale, only updated by portal scale changes
    pub p_scale: f32,

    // PORT: std::shared_ptr<T> -> Option<Rc<T>>; a null shared_ptr is None
    // (was: std::shared_ptr<Mesh> mesh; ..., Object.h:42-44).
    pub mesh: Option<Rc<Mesh>>,
    pub texture: Option<Rc<Texture>>,
    pub shader: Option<Rc<Shader>>,

    // EXT: a rotation that stands in for `euler` when set. A rigid body under a physics
    // engine turns freely, and a general orientation does not round-trip through the
    // Y-X-Z Euler product below without gimbal trouble, so such an object hands its rotation
    // over whole (a pure rotation matrix: orthonormal axes, no translation, no scale) and
    // `local_to_world` / `world_to_local` / `forward` use it INSTEAD of `euler`. `None` is the
    // ported path, bit for bit; `Physical::try_portal` still rewrites `euler.y` and so only
    // reorients objects on that path.
    pub rot: Option<Matrix4>,
}

// PORT: written by hand, NOT derived -- the C++ default ctor sets scale to 1 and
// p_scale to 1, which #[derive(Default)] would get wrong
// (was: Object::Object() : pos(0.0f), euler(0.0f), scale(1.0f), p_scale(1.0f) {}, Object.cpp:6-11).
impl Default for Object {
    fn default() -> Object {
        Object::new()
    }
}

impl Object {
    pub fn new() -> Object {
        Object {
            pos: Vector3::splat(0.0),
            euler: Vector3::splat(0.0),
            scale: Vector3::splat(1.0),
            p_scale: 1.0,
            mesh: None,
            texture: None,
            shader: None,
            rot: None,
        }
    }

    pub fn reset(&mut self) {
        self.pos.set_zero();
        self.euler.set_zero();
        self.scale.set_ones();
        self.p_scale = 1.0;
        // EXT: back to the Euler path, as a reset orientation should be.
        self.rot = None;
    }

    // PORT: this is the body of `Object::Draw` (Object.cpp:20-31), split out under a
    // separate name so that ObjectT::draw (the virtual) can call it as its default.
    // The `curFBO` parameter is dropped here -- the base implementation never used it;
    // only Portal::Draw does.
    pub fn draw_impl(&self, ctx: &RenderCtx, cam: &Camera) {
        // PORT: `ctx.gl` is unused -- Mesh/Shader/Texture each own an Rc<glow::Context>,
        // so the GL handle does not have to be passed down like the C++ global did.
        if let (Some(shader), Some(mesh)) = (&self.shader, &self.mesh) {
            // EXT: bounding-sphere cull against this pass's frustum, before any GL work. An
            // empty or collider-only mesh has no sphere and is left alone -- it draws nothing.
            if let Some((c, r)) = crate::ext::cull::object_sphere(self) {
                if !ctx.frustum.sphere(c, r) {
                    return;
                }
            }
            let mv = self.world_to_local().transposed();
            let mvp = cam.matrix() * self.local_to_world();
            shader.use_program();
            if let Some(texture) = &self.texture {
                texture.use_texture();
            }
            shader.set_mvp(Some(&mvp), Some(&mv));
            // EXT: frame clock for animated materials; a silent no-op for shaders that do not
            // declare `time` (every original shader).
            shader.set_f32("time", crate::ext::view::time());
            // EXT: eye position for view-dependent materials (grass sheen, distance haze).
            let eye = ctx.eye;
            shader.set_vec4("cam_pos", [eye.x, eye.y, eye.z, 1.0]);
            // EXT: weather grade for this render pass and the door's light pool (intro level).
            shader.set_f32("mood", crate::ext::view::mood_for(eye));
            shader.set_vec4("glow", crate::ext::view::glow());
            shader.set_f32("detail", crate::ext::view::detail());
            // EXT: world period for scenes that wrap (the toroidal meadow). 0 elsewhere, which
            // every shader reads as "do not quantise anything".
            shader.set_f32("wrap", crate::ext::view::wrap());
            mesh.draw();
        }
    }

    pub fn forward(&self) -> Vector3 {
        // EXT: the override's -Z axis; the Euler product below otherwise (see `rot`).
        if let Some(rot) = &self.rot {
            return -rot.z_axis();
        }
        -(Matrix4::rot_z(self.euler.z)
            * Matrix4::rot_x(self.euler.x)
            * Matrix4::rot_y(self.euler.y))
        .z_axis()
    }

    pub fn local_to_world(&self) -> Matrix4 {
        // EXT: see `rot`.
        if let Some(rot) = &self.rot {
            return Matrix4::trans(self.pos) * *rot * Matrix4::scale(self.scale * self.p_scale);
        }
        Matrix4::trans(self.pos)
            * Matrix4::rot_y(self.euler.y)
            * Matrix4::rot_x(self.euler.x)
            * Matrix4::rot_z(self.euler.z)
            * Matrix4::scale(self.scale * self.p_scale)
    }

    pub fn world_to_local(&self) -> Matrix4 {
        // EXT: a pure rotation's inverse is its transpose (see `rot`).
        if let Some(rot) = &self.rot {
            return Matrix4::scale(1.0 / (self.scale * self.p_scale))
                * rot.transposed()
                * Matrix4::trans(-self.pos);
        }
        Matrix4::scale(1.0 / (self.scale * self.p_scale))
            * Matrix4::rot_z(-self.euler.z)
            * Matrix4::rot_x(-self.euler.x)
            * Matrix4::rot_y(-self.euler.y)
            * Matrix4::trans(-self.pos)
    }

    // PORT: Object::DebugDraw (Object.cpp:45-49) is dropped -- it forwards to
    // Mesh::DebugDraw -> Collider::DebugDraw, which is glBegin/glVertex4f/glColor3f
    // immediate mode (Collider.cpp:47-67). Immediate mode does not exist in a 3.3+ core
    // profile, and its only caller is the commented-out debug path in Engine.cpp:267.
}

/// The virtual half of `class Object` (Object.h:20-27).
///
/// PORT: `virtual void OnHit(Object& other, Vector3& push)` (Object.h:23) loses its
/// `other` argument -- every implementation in the codebase ignores it, and passing it
/// would force borrowing two `RefCell<dyn ObjectT>` cells mutably at once.
///
/// IMPORTANT: `draw` takes `&self`, never `&mut self`. Portal::draw recurses back into
/// Engine::render, which can reach the SAME portal object again (skipPortal is
/// warp->toPortal, the portal on the far side, not the one currently being drawn). With
/// Rc<RefCell<..>> a nested borrow_mut() would panic at runtime; a nested borrow() is
/// fine, so the whole render path must stay immutable.
#[allow(dead_code)]
pub trait ObjectT {
    fn base(&self) -> &Object;
    fn base_mut(&mut self) -> &mut Object;

    fn draw(&self, ctx: &RenderCtx, cam: &Camera, _cur_fbo: Option<glow::Framebuffer>) {
        self.base().draw_impl(ctx, cam)
    }

    fn update(&mut self, _ctx: &UpdateCtx) {}

    fn on_hit(&mut self, _push: Vector3) {}

    // PORT: added (not in Object.h). `Physical::OnCollide` is virtual and Player overrides
    // it (Physical.h:13, Player.h:12), but the contract's `as_physical_mut()` hands out a
    // `&mut Physical`, on which Rust would statically resolve `on_collide` to the base
    // implementation and silently lose Player's override (its onGround handling). Engine
    // must therefore call `obj.on_collide(push)` through this trait method -- which
    // dispatches virtually -- instead of `as_physical_mut().unwrap().on_collide(push)`.
    // The default reproduces the C++ behaviour for every non-Player Physical.
    fn on_collide(&mut self, push: Vector3) {
        if let Some(p) = self.as_physical_mut() {
            p.on_collide(push);
        }
    }

    // EXT: a triangle-mesh collider (src/ext/trimesh.rs), consulted by Engine::update's
    // collision pass beside the rectangle colliders on `base().mesh`. Scenery built from a
    // scanned model has no rectangles to declare and returns its mesh here instead.
    fn trimesh(&self) -> Option<Rc<crate::ext::trimesh::TriMeshCollider>> {
        None
    }

    // EXT: whether the engine moves this object. When false, `Engine::update`'s collision
    // pass skips it as the SUBJECT (its hit spheres are never pushed; it still blocks others
    // through its mesh and trimesh) and the portal pass never warps it -- something else,
    // a rigid-body simulation, owns its motion and writes `pos`/`rot` itself.
    fn engine_collision(&self) -> bool {
        true
    }

    // EXT: the grab's hooks (src/ext/grab.rs). `on_grab` at the moment of pickup;
    // `on_release` at the drop, with the hand's velocity -- the held object's displacement
    // over the last rendered frame divided by that frame's length, so a thrown object can
    // keep its momentum; `on_rescale` whenever the carry changes `p_scale`, with the new
    // value, after it has been written. All three default to nothing.
    fn on_grab(&mut self) {}
    fn on_release(&mut self, _velocity: Vector3) {}
    fn on_rescale(&mut self, _p_scale: f32) {}

    // EXT: how the grab places this object against what the crosshair hits. False (the
    // default) stands it off the surface by its bounding radius, as a ball would rest; true
    // lays it flush -- a picture or a window on a wall, a mat on a floor -- and turns it to
    // face out of the surface (`grab::flat_euler`).
    fn place_flat(&self) -> bool {
        false
    }

    // EXT: a prompt for the HUD while the crosshair is on this object (`ext::hint`): what
    // E would do, or why it will not ("LOCKED", "TOO SMALL", "E  USE KEY"). None is silent.
    fn pick_hint(&self) -> Option<&'static str> {
        None
    }

    //Casts
    fn as_physical(&self) -> Option<&crate::physical::Physical> {
        None
    }
    fn as_physical_mut(&mut self) -> Option<&mut crate::physical::Physical> {
        None
    }

    // PORT: named `reset_obj` rather than `reset` so it does not collide with the
    // inherent `Object::reset` / `Physical::reset` / `Player::reset`
    // (was: virtual void Reset(), Object.h:20).
    fn reset_obj(&mut self) {
        self.base_mut().reset()
    }
}

// A plain prop is just an Object with no overrides.
impl ObjectT for Object {
    fn base(&self) -> &Object {
        self
    }
    fn base_mut(&mut self) -> &mut Object {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_header::GH_PI;

    fn approx_m(a: &Matrix4, b: &Matrix4) -> bool {
        a.m.iter().zip(b.m.iter()).all(|(x, y)| (x - y).abs() < 1e-5)
    }

    /// EXT: the rotation override reproduces the Euler path for the one rotation both can
    /// express, and its inverse really is the inverse.
    #[test]
    fn rot_override_matches_the_euler_path_for_a_yaw() {
        let mut euler = Object::new();
        euler.pos = Vector3::new(1.0, 2.0, 3.0);
        euler.scale = Vector3::new(2.0, 3.0, 4.0);
        euler.p_scale = 0.5;
        euler.euler.y = 0.7 * GH_PI;
        let mut over = Object { rot: Some(Matrix4::rot_y(0.7 * GH_PI)), ..Object::new() };
        over.pos = euler.pos;
        over.scale = euler.scale;
        over.p_scale = euler.p_scale;
        // A different yaw in `euler`, to prove it is ignored.
        over.euler.y = -1.0;
        assert!(approx_m(&over.local_to_world(), &euler.local_to_world()));
        assert!(approx_m(&over.world_to_local(), &euler.world_to_local()));
        assert!((over.forward() - euler.forward()).mag() < 1e-5);
        assert!(approx_m(&(over.local_to_world() * over.world_to_local()), &Matrix4::identity()));
        // And a rotation the Euler path cannot name still round-trips.
        over.rot = Some(Matrix4::rot_x(0.3) * Matrix4::rot_y(1.1) * Matrix4::rot_z(-2.0));
        assert!(approx_m(&(over.local_to_world() * over.world_to_local()), &Matrix4::identity()));
        over.reset();
        assert!(over.rot.is_none(), "reset returns to the Euler path");
    }
}
