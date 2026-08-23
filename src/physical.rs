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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_header::GH_PI;
    use crate::portal::connect;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn approx(a: Vector3, b: Vector3) -> bool {
        (a - b).mag() < 1e-4
    }

    /// Portal A at the origin facing -z, portal B at x = 10 turned a quarter turn and twice
    /// the size, connected. Walking into A from its front (+z side) comes out of B's front.
    fn portals() -> (Portal, Portal) {
        let a = Rc::new(RefCell::new(Portal::detached()));
        let b = Rc::new(RefCell::new(Portal::detached()));
        {
            let mut b = b.borrow_mut();
            b.base.pos = Vector3::new(10.0, 0.0, 0.0);
            b.base.euler.y = GH_PI / 2.0;
            b.base.scale = Vector3::splat(2.0);
        }
        connect(&a, &b);
        let a = Rc::try_unwrap(a).ok().unwrap().into_inner();
        let b = Rc::try_unwrap(b).ok().unwrap().into_inner();
        (a, b)
    }

    /// A physical whose last step went from `from` to `to`, looking along -z.
    fn stepped(from: Vector3, to: Vector3) -> Physical {
        let mut p = Physical::new();
        p.set_position(from);
        p.base.pos = to;
        p.velocity = (to - from) * (1.0 / GH_DT);
        p
    }

    #[test]
    fn crossing_the_plane_teleports_into_the_other_frame() {
        let (a, b) = portals();
        let mut p = stepped(Vector3::new(0.0, 0.0, 1.0), Vector3::new(0.0, 0.0, -1.0));
        let v0 = p.velocity;
        assert!(p.try_portal(&a));
        // Position: A-local (0, 0, -1) minus the doubled bump, scaled by two and turned into
        // B's frame, so 2.004 out along B's forward (-x in world) from B's centre.
        let depth = 1.0 + 2.0 * (2.0 * GH_NEAR_MIN);
        assert!(approx(p.base.pos, Vector3::new(10.0 - 2.0 * depth, 0.0, 0.0)), "{:?}", p.base.pos);
        assert!(approx(
            b.base.world_to_local().mul_point(p.base.pos),
            Vector3::new(0.0, 0.0, -depth)
        ));
        // prev_pos is reset to the new position so the next step cannot re-cross.
        assert!(approx(p.prev_pos, p.base.pos));
        // Velocity turns with the frame and doubles with it: -z at speed s becomes -x at 2s.
        assert!(approx(p.velocity, Vector3::new(-2.0 * v0.mag(), 0.0, 0.0)), "{:?}", p.velocity);
        // Looking along -z (euler.y = 0) now looks along -x, which is euler.y = +pi/2
        // (forward = (-sin, 0, -cos)).
        assert!((p.base.euler.y - GH_PI / 2.0).abs() < 1e-5, "{}", p.base.euler.y);
        // And the traveller is twice the size.
        assert!((p.base.p_scale - 2.0).abs() < 1e-5);
    }

    #[test]
    fn the_return_trip_undoes_the_scale_and_the_turn() {
        let (a, b) = portals();
        let mut p = stepped(Vector3::new(0.0, 0.0, 1.0), Vector3::new(0.0, 0.0, -1.0));
        assert!(p.try_portal(&a));
        // Step back through B, now at twice the scale: from 2 out along its forward to 2 past.
        let here = p.base.pos;
        let back = Vector3::new(10.0 + 2.0, 0.0, 0.0);
        p.prev_pos = here;
        p.base.pos = back;
        p.velocity = (back - here) * (1.0 / GH_DT);
        assert!(p.try_portal(&b));
        assert!((p.base.p_scale - 1.0).abs() < 1e-5);
        assert!(p.base.euler.y.abs() < 1e-5, "{}", p.base.euler.y);
        assert!(p.base.pos.z > 0.0 && p.base.pos.x.abs() < 1e-4, "{:?}", p.base.pos);
        assert!(p.velocity.z > 0.0 && p.velocity.x.abs() < 1e-2, "{:?}", p.velocity);
    }

    #[test]
    fn a_step_that_does_not_cross_changes_nothing() {
        let (a, _) = portals();
        let mut p = stepped(Vector3::new(0.0, 0.0, 1.0), Vector3::new(0.0, 0.0, 0.5));
        let (pos, prev, vel, ey, ps) =
            (p.base.pos, p.prev_pos, p.velocity, p.base.euler.y, p.base.p_scale);
        assert!(!p.try_portal(&a));
        assert!(approx(p.base.pos, pos) && approx(p.prev_pos, prev) && approx(p.velocity, vel));
        assert!(p.base.euler.y == ey && p.base.p_scale == ps);
        // Crossing the plane outside the quad is not a crossing either.
        let mut p = stepped(Vector3::new(3.0, 0.0, 1.0), Vector3::new(3.0, 0.0, -1.0));
        assert!(!p.try_portal(&a));
        assert!(approx(p.base.pos, Vector3::new(3.0, 0.0, -1.0)));
    }

    #[test]
    fn the_bump_lands_the_object_past_the_far_plane() {
        let (a, b) = portals();
        // A step that ends exactly on A's plane still counts -- the plane is bumped
        // 2*GH_NEAR_MIN towards the traveller -- and the arrival is 4*GH_NEAR_MIN (times the
        // new scale) beyond B's plane, never on it, so the next step cannot bounce back.
        let mut p = stepped(Vector3::new(0.0, 0.0, 0.01), Vector3::new(0.0, 0.0, 0.0));
        assert!(p.try_portal(&a));
        let clearance = (p.base.pos - b.base.pos).dot(b.base.forward());
        let expect = 4.0 * GH_NEAR_MIN * 2.0;
        assert!((clearance - expect).abs() < 1e-5, "clearance {clearance} vs {expect}");
        assert!(clearance > GH_NEAR_MIN);
        // The bump scales with the traveller: at p_scale 2 it is twice as far.
        let mut big = stepped(Vector3::new(0.0, 0.0, 0.01), Vector3::new(0.0, 0.0, 0.0));
        big.base.p_scale = 2.0;
        assert!(big.try_portal(&a));
        let clearance = (big.base.pos - b.base.pos).dot(b.base.forward());
        assert!((clearance - 2.0 * expect).abs() < 1e-5, "clearance {clearance}");
        assert!((big.base.p_scale - 4.0).abs() < 1e-5);
    }

    #[test]
    fn on_collide_removes_the_velocity_into_the_push() {
        let mut p = Physical::new();
        p.velocity = Vector3::new(1.0, -2.0, 0.0);
        p.on_collide(Vector3::new(0.0, 0.5, 0.0));
        assert!(approx(p.base.pos, Vector3::new(0.0, 0.5, 0.0)));
        assert!(approx(p.velocity, Vector3::new(1.0, 0.0, 0.0)), "{:?}", p.velocity);
        // Bounce reflects the component into the push; friction scales the rest.
        let mut p = Physical::new();
        p.bounce = 0.5;
        p.friction = 0.25;
        p.velocity = Vector3::new(1.0, -2.0, 0.0);
        p.on_collide(Vector3::new(0.0, 0.5, 0.0));
        assert!(approx(p.velocity, Vector3::new(0.75, 1.0, 0.0)), "{:?}", p.velocity);
        // A push below the 1e-8 * p_scale threshold moves the object but leaves its velocity.
        let mut p = Physical::new();
        p.velocity = Vector3::new(1.0, -2.0, 0.0);
        p.on_collide(Vector3::new(0.0, 1e-5, 0.0));
        assert!(approx(p.velocity, Vector3::new(1.0, -2.0, 0.0)));
        assert!((p.base.pos.y - 1e-5).abs() < 1e-9);
    }
}
