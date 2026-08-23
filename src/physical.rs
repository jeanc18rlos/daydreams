//! Port of Physical.h / Physical.cpp.

use crate::game_header::{gh_min, GH_DT, GH_GRAVITY, GH_NEAR_MIN};
use crate::object::{Object, ObjectT, UpdateCtx};
use crate::portal::Portal;
use crate::sphere::Sphere;
use crate::vector::{Matrix4, Vector3};

// PORT: `class Physical : public Object` -> composition; the base class becomes the
// `base` field and the virtuals are re-exposed through `impl ObjectT for Physical`
// (was: class Physical : public Object, Physical.h:6).
pub struct Physical {
    pub base: Object,

    pub gravity: Vector3,
    pub velocity: Vector3,
    pub bounce: f32,
    pub friction: f32,
    pub high_friction: f32,
    pub drag: f32,

    pub prev_pos: Vector3,

    // PORT: snake_case (was: std::vector<Sphere> hitSpheres, Physical.h:33).
    pub hit_spheres: Vec<Sphere>,
}

impl Physical {
    pub fn new() -> Physical {
        // PORT: C++ default-constructs the members and then calls Reset() (Physical.cpp:4-6);
        // Rust needs every field initialized first, so they are zeroed and Reset() overwrites
        // them exactly as before.
        let mut p = Physical {
            base: Object::new(),
            gravity: Vector3::splat(0.0),
            velocity: Vector3::splat(0.0),
            bounce: 0.0,
            friction: 0.0,
            high_friction: 0.0,
            drag: 0.0,
            prev_pos: Vector3::splat(0.0),
            hit_spheres: Vec::new(),
        };
        p.reset();
        p
    }

    pub fn reset(&mut self) {
        self.base.reset();
        self.velocity.set_zero();
        self.gravity.set(0.0, GH_GRAVITY, 0.0);
        self.bounce = 0.0;
        self.friction = 0.0;
        self.high_friction = 0.0;
        self.drag = 0.0;
        self.prev_pos.set_zero();
    }

    pub fn update(&mut self) {
        self.prev_pos = self.base.pos;
        self.velocity += self.gravity * self.base.p_scale * GH_DT;
        self.velocity *= 1.0 - self.drag;
        self.base.pos += self.velocity * GH_DT;
    }

    pub fn set_position(&mut self, _pos: Vector3) {
        self.base.pos = _pos;
        self.prev_pos = _pos;
    }

    // PORT: dropped the unused `Object& other` first argument
    // (was: void Physical::OnCollide(Object& other, const Vector3& push), Physical.cpp:26).
    pub fn on_collide(&mut self, push: Vector3) {
        //Update position to avoid collision
        self.base.pos += push;

        //Ignore push if delta is too small
        if push.mag_sq() < 1e-8 * self.base.p_scale {
            return;
        }

        //Calculate kinetic friction
        let mut kinetic_friction = self.friction;
        if self.high_friction > 0.0 {
            let vel_ratio = self.velocity.mag() / (self.high_friction * self.base.p_scale);
            kinetic_friction = gh_min(self.friction * (vel_ratio + 5.0) / (vel_ratio + 1.0), 1.0);
        }

        //Update velocity to react to collision
        let push_proj = push * (self.velocity.dot(push) / push.dot(push));
        self.velocity =
            (self.velocity - push_proj) * (1.0 - kinetic_friction) - push_proj * self.bounce;
    }

    pub fn try_portal(&mut self, portal: &Portal) -> bool {
        let bump = portal.get_bump(self.prev_pos) * (2.0 * GH_NEAR_MIN * self.base.p_scale);
        // PORT: `const Portal::Warp*` (nullable pointer) -> Option<&Warp>
        // (was: const Warp* warp = portal.Intersects(...), Physical.cpp:49).
        let warp = portal.intersects(self.prev_pos, self.base.pos, bump);
        if let Some(warp) = warp {
            //Teleport object
            self.base.pos = warp.delta_inv.mul_point(self.base.pos - bump * 2.0);
            self.velocity = warp.delta_inv.mul_direction(self.velocity);
            self.prev_pos = self.base.pos;

            //Update camera direction
            let forward = Vector3::new(-self.base.euler.y.sin(), 0.0, -self.base.euler.y.cos());
            let new_dir = warp.delta_inv.mul_direction(forward);
            self.base.euler.y = -new_dir.x.atan2(-new_dir.z);

            //Update object scale
            self.base.p_scale *= warp.delta_inv.x_axis().mag();
            return true;
        }
        false
    }

    // Inherited from Object (Object.h:31-32), forwarded for convenience.
    pub fn world_to_local(&self) -> Matrix4 {
        self.base.world_to_local()
    }
    pub fn local_to_world(&self) -> Matrix4 {
        self.base.local_to_world()
    }
}

impl ObjectT for Physical {
    fn base(&self) -> &Object {
        &self.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base
    }

    fn update(&mut self, _ctx: &UpdateCtx) {
        Physical::update(self)
    }

    // virtual Physical* AsPhysical() override { return this; }   (Physical.h:22)
    fn as_physical(&self) -> Option<&Physical> {
        Some(self)
    }
    fn as_physical_mut(&mut self) -> Option<&mut Physical> {
        Some(self)
    }

    fn reset_obj(&mut self) {
        Physical::reset(self)
    }
}
