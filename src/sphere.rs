//! Port of Sphere.h.

use crate::vector::{Matrix4, Vector3};

#[derive(Clone, Copy)]
pub struct Sphere {
    pub center: Vector3,
    pub radius: f32,
}

// Preserved verbatim from the C++ API surface; not every member is reachable from the
// scenes this port ships. Kept for fidelity rather than deleted.
#[allow(dead_code)]
impl Sphere {
    // PORT: `Sphere(float r=1.0f)` (default argument) and `Sphere(const Vector3&, float)`
    // become two named ctors (was: Sphere(float r=1.0f) : center(0.0f), radius(r), Sphere.h:7-8).
    pub fn new(r: f32) -> Sphere {
        Sphere { center: Vector3::splat(0.0), radius: r }
    }
    pub fn new_at(pos: Vector3, r: f32) -> Sphere {
        Sphere { center: pos, radius: r }
    }

    //Transformations to and frpom sphere coordinates
    // PORT: C++ `assert` (compiled out in release) -> debug_assert! (was: assert(radius > 0.0f), Sphere.h:12).
    pub fn unit_to_local(&self) -> Matrix4 {
        debug_assert!(self.radius > 0.0);
        Matrix4::trans(self.center) * Matrix4::scale_uniform(self.radius)
    }
    pub fn local_to_unit(&self) -> Matrix4 {
        debug_assert!(self.radius > 0.0);
        Matrix4::scale_uniform(1.0 / self.radius) * Matrix4::trans(-self.center)
    }
}
