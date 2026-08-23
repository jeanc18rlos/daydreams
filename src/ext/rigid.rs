//! EXT: a prop that is a rigid body. Not part of the C++ port.
//!
//! A [`RigidProp`] is drawn like any ported object -- an `Object` with a mesh, a texture and
//! a shader, through `Object::draw_impl` -- but its motion belongs to the rapier world in
//! `ext/physics.rs`. The engine's collision pass never pushes it and the portal pass never
//! warps it (`ObjectT::engine_collision` is false); instead, every fixed step, `update` copies
//! the body's pose into `Object::pos` and `Object::rot`, and the prop falls, rolls, topples and
//! comes to rest the way the solver says.
//!
//! # Being grabbed
//!
//! The prop is still a `Physical` with exactly one hit sphere, which is how `ext/grab.rs`
//! recognises a grabbable (`as_grabbable`), so it is picked up, carried, resized by
//! perspective and put down like the bunny or the teapot. What changes is what the two ends
//! of a carry do to the body:
//!
//! * **`on_grab`**: the body becomes kinematic -- it follows the hand and shoves whatever it
//!   meets, but nothing moves it -- and the orientation goes back to the Euler path. While
//!   held, the rotate-with-R1/RMB feature adds to `euler.y` and `euler.x`, so the rotation
//!   matrix the body had is converted to the engine's Euler order ([`Matrix4::to_euler`])
//!   and `rot` is cleared; each step the Euler product is pushed to the body as its next
//!   kinematic pose.
//! * **`on_release`**: the Euler product is frozen back into `rot`, pushed to the body as its
//!   pose, the body goes dynamic with the hand's velocity (capped at [`MAX_THROW`]) and a
//!   tumble about the axis a thrown thing turns on, and rapier takes over again. Letting go
//!   while the view is moving is a throw.
//! * **`on_rescale`**: the collider is rebuilt at the new `p_scale`; its mass follows its
//!   volume through the material's density.
//!
//! Names, shapes and materials are the prop's own: `Shape` says what the collider is about
//! the mesh's origin, `Material` how it slides, bounces and weighs (`ext/physics.rs`).

use crate::camera::Camera;
use crate::ext::backrooms::WALL_FOG;
use crate::ext::physics::{self, BodyId, Material, Shape};
use crate::object::{Object, ObjectT, RenderCtx, UpdateCtx};
use crate::physical::Physical;
use crate::resources::Resources;
use crate::sphere::Sphere;
use crate::vector::{Matrix4, Vector3};

/// Fastest a prop leaves the hand, in units per second. The hand's velocity is measured
/// over one rendered frame, and a frame that hitched would otherwise read as a cannon.
pub const MAX_THROW: f32 = 12.0;
/// Radians per second of tumble per unit of throw speed. A thing let go of turns about the
/// axis across its motion, as it would rolling off the fingers; small, so a gentle drop
/// barely turns and a hard throw visibly does.
pub const THROW_SPIN: f32 = 2.0;

impl Matrix4 {
    /// EXT: the Euler angles whose `rot_y(y) * rot_x(x) * rot_z(z)` product -- the order
    /// `Object::local_to_world` composes (Object.cpp:38) -- is this rotation. Inverse of the
    /// engine's own convention, for handing a free rotation back to the Euler path.
    ///
    /// With `c`/`s` the cosines and sines, the product is
    ///
    /// ```text
    ///   [ cy cz + sy sx sz   -cy sz + sy sx cz   sy cx ]
    ///   [ cx sz               cx cz              -sx   ]
    ///   [ -sy cz + cy sx sz   sy sz + cy sx cz   cy cx ]
    /// ```
    ///
    /// so `z` reads off `m[4]`/`m[5]`, and `x` off `m[6]` against the `cx` those two carry
    /// (`asin(-m[6])` alone loses `cx` to rounding near the gimbal lock, where `m[6]` is
    /// within a float's last bit of 1 while the `cx`-scaled entries still hold it). `y` could
    /// read off `m[2]`/`m[10]` the same way, but near the lock (`cx -> 0`) each angle is then
    /// read through float noise -- while the rotation only depends on `y + z` there, which the
    /// first row carries at full size. So `y` is solved from the first and third rows GIVEN
    /// `z`: `cy = m0 cz - m1 sz`, `sy = m9 sz - m8 cz` (the product's rows, with the `z`
    /// terms cancelled), which needs no division, is exact away from the lock, and at it
    /// returns whatever `y` makes `y + z` right for the `z` read -- so the pair always rebuilds
    /// the matrix. At the lock itself there is no `z` to read and it is taken as zero, so `y`
    /// carries the whole turn, as the rotate feature (which adds to `y`) would want.
    pub fn to_euler(self) -> Vector3 {
        let m = &self.m;
        let cx = m[4].hypot(m[5]);
        let x = (-m[6]).atan2(cx);
        let z = if cx < 1e-6 { 0.0 } else { m[4].atan2(m[5]) };
        let (sz, cz) = z.sin_cos();
        let y = (m[9] * sz - m[8] * cz).atan2(m[0] * cz - m[1] * sz);
        Vector3::new(x, y, z)
    }
}

/// The engine's rotation from its Euler angles: what `local_to_world` composes.
fn euler_matrix(e: Vector3) -> Matrix4 {
    Matrix4::rot_y(e.y) * Matrix4::rot_x(e.x) * Matrix4::rot_z(e.z)
}

/// A prop owned by the rigid-body world. See the module docs.
pub struct RigidProp {
    base: Physical,
    shape: Shape,
    body: BodyId,
    /// Set between `on_grab` and `on_release`: the body is kinematic and follows `base`.
    held: bool,
}

impl RigidProp {
    /// A prop drawn with `mesh` (a `Meshes/` file) in `texture` (a `Textures/` BMP) through
    /// the `prop` shader, standing at `pos` with no rotation, its body awake. `name` is what
    /// the `[prop]` report calls it.
    pub fn new(
        res: &Resources,
        name: &'static str,
        mesh: &str,
        texture: &str,
        shape: Shape,
        material: Material,
        pos: Vector3,
    ) -> RigidProp {
        let mut base = Physical::new();
        let mesh = res.acquire_mesh(mesh);
        // Exactly ONE hit sphere, at the mesh's bounding radius: the marker `grab::as_grabbable`
        // looks for, and what the crosshair pick tests against. The ported collision pass never
        // reads it (`engine_collision` is false).
        base.hit_spheres.push(Sphere::new_at(Vector3::zero(), mesh.bound_radius));
        base.base.mesh = Some(mesh);
        base.base.texture = Some(res.acquire_texture(texture, 1, 1));
        base.base.shader = Some(res.acquire_shader("prop"));
        base.set_position(pos);
        base.base.rot = Some(Matrix4::identity());
        let body = physics::with(|w| w.add_body(name, &shape, material, pos, &Matrix4::identity()));
        RigidProp { base, shape, body, held: false }
    }
}

impl Drop for RigidProp {
    fn drop(&mut self) {
        let body = self.body;
        physics::try_with(|w| w.remove_body(body));
    }
}

impl ObjectT for RigidProp {
    fn base(&self) -> &Object {
        &self.base.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base.base
    }

    fn update(&mut self, _ctx: &UpdateCtx) {
        let b = &mut self.base.base;
        if self.held {
            // The hand (ext/grab.rs) wrote `pos`, and the rotate feature `euler`: the body
            // follows.
            physics::with(|w| w.set_pose(self.body, b.pos, &euler_matrix(b.euler)));
        } else if let Some((pos, rot)) = physics::with(|w| w.pose(self.body)) {
            b.pos = pos;
            b.rot = Some(rot);
            self.base.prev_pos = pos;
        }
    }

    fn draw(&self, ctx: &RenderCtx, cam: &Camera, _fbo: Option<glow::Framebuffer>) {
        // The shader's fog tone and model matrix are the prop's to set (Shaders/prop.frag);
        // everything else `draw_impl` sets as for any ported object.
        let b = &self.base.base;
        if let Some(shader) = &b.shader {
            shader.use_program();
            shader.set_vec4("fog_color", WALL_FOG);
            shader.set_mat4("model", &b.local_to_world());
        }
        b.draw_impl(ctx, cam);
    }

    fn engine_collision(&self) -> bool {
        false
    }

    fn on_grab(&mut self) {
        let b = &mut self.base.base;
        // The free rotation becomes the Euler path's, so R1/RMB can add to it.
        if let Some(rot) = b.rot.take() {
            b.euler = rot.to_euler();
        }
        self.held = true;
        physics::with(|w| w.set_kinematic(self.body));
    }

    fn on_release(&mut self, velocity: Vector3) {
        let b = &mut self.base.base;
        let rot = euler_matrix(b.euler);
        b.rot = Some(rot);
        self.held = false;
        let mut v = velocity;
        v.clip_mag(MAX_THROW);
        let spin = Vector3::unit_y().cross(v) * THROW_SPIN;
        physics::with(|w| {
            w.set_dynamic(self.body);
            w.set_pose(self.body, b.pos, &rot);
            w.set_velocity(self.body, v, spin);
        });
    }

    fn on_rescale(&mut self, p_scale: f32) {
        let shape = self.shape.scaled(p_scale);
        physics::with(|w| w.rescale_collider(self.body, &shape));
    }

    fn as_physical(&self) -> Option<&Physical> {
        Some(&self.base)
    }
    fn as_physical_mut(&mut self) -> Option<&mut Physical> {
        Some(&mut self.base)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_header::GH_PI;

    fn close(a: &Matrix4, b: &Matrix4) -> bool {
        a.m.iter().zip(b.m.iter()).all(|(x, y)| (x - y).abs() < 1e-5)
    }

    /// Any set of Euler angles with the pitch inside its principal range comes back as
    /// itself; any rotation at all comes back as a product that is the same rotation.
    #[test]
    fn to_euler_round_trips() {
        let mut seed = 12345u32;
        let mut rnd = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
        };
        for _ in 0..200 {
            let e = Vector3::new(rnd() * 0.49 * GH_PI, rnd() * GH_PI, rnd() * GH_PI);
            let back = euler_matrix(e).to_euler();
            assert!((back - e).mag() < 1e-4, "{e:?} -> {back:?}");
        }
        for _ in 0..200 {
            // A rotation the Euler path would never have produced: an arbitrary product.
            let m = Matrix4::rot_z(rnd() * GH_PI)
                * Matrix4::rot_x(rnd() * GH_PI)
                * Matrix4::rot_y(rnd() * GH_PI)
                * Matrix4::rot_z(rnd() * GH_PI);
            let back = euler_matrix(m.to_euler());
            assert!(close(&m, &back), "{m:?} -> {back:?}");
        }
        // A whisker off the gimbal lock, where reading every angle through `cx` would be
        // reading float noise: the pair returned still rebuilds the matrix.
        for i in 0..100 {
            let off = 1e-5 * (i + 1) as f32;
            for sign in [1.0, -1.0] {
                let e = Vector3::new(sign * (GH_PI / 2.0 - off), rnd() * GH_PI, rnd() * GH_PI);
                let m = euler_matrix(e);
                let back = euler_matrix(m.to_euler());
                assert!(close(&m, &back), "{e:?}: {m:?} -> {back:?}");
            }
        }
    }

    #[test]
    fn to_euler_at_the_gimbal_lock_keeps_the_yaw() {
        for yaw in [0.0f32, 0.8, -2.3, 3.0] {
            for pitch in [GH_PI / 2.0, -GH_PI / 2.0] {
                let m = euler_matrix(Vector3::new(pitch, yaw, 0.0));
                let e = m.to_euler();
                assert!((e.x - pitch).abs() < 1e-4, "{e:?}");
                assert!(e.z.abs() < 1e-6, "{e:?}");
                assert!(close(&m, &euler_matrix(e)), "{e:?}");
            }
        }
        // The identity is all zeros, exactly.
        let e = Matrix4::identity().to_euler();
        assert!(e.x == 0.0 && e.y == 0.0 && e.z == 0.0, "{e:?}");
    }

    #[test]
    fn a_throw_is_capped_and_tumbles_across_its_motion() {
        let v = Vector3::new(30.0, 0.0, 0.0);
        let mut capped = v;
        capped.clip_mag(MAX_THROW);
        assert!((capped.mag() - MAX_THROW).abs() < 1e-5);
        let spin = Vector3::unit_y().cross(capped) * THROW_SPIN;
        assert!(spin.dot(capped).abs() < 1e-5, "spin axis is across the throw");
        assert!((spin.mag() - MAX_THROW * THROW_SPIN).abs() < 1e-4);
    }
}
