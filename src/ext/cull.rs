//! EXT: view-frustum culling. Not part of the C++ port.
//!
//! The ported renderer draws every object in every pass (Engine.cpp:225-228), which was fine for
//! seven small rooms and is not fine for a meadow: the intro's terrain lattice is nine tiles of
//! 32k triangles each, and a portal pass whose camera stands in the sea world a thousand units
//! away still pushed all nine through the vertex stage to be discarded at the far plane. This is
//! the standard remedy -- six planes pulled out of the view-projection matrix once per pass, and
//! a bounding volume tested against them per thing drawn.
//!
//! # Plane extraction on this engine's matrices
//!
//! `Matrix4` is row-major with translation at `m[3], m[7], m[11]` (Vector.h), and a point maps
//! to clip space as `clip = M * [x, y, z, 1]`, so clip.x is row 0 of `M` dotted with the point,
//! and so on. Gribb & Hartmann's planes are then sums and differences of rows: a point is inside
//! the left plane when `clip.w + clip.x >= 0`, i.e. `(row3 + row0) . p >= 0`. Each plane is
//! normalised so its dot product is a true signed distance, which is what the sphere test needs.
//!
//! This holds for the portal cameras too. `Camera::clip_oblique` rewrites the projection's third
//! row so the GPU clips against the portal plane instead of the near plane; the "far" plane
//! extracted from that matrix is equally odd (Lengyel's trick tilts it to keep the depth range
//! usable), but it is exactly the plane the GPU clips against, so a volume outside it really is
//! not drawn.

use crate::object::Object;
use crate::vector::{Matrix4, Vector3};

/// World-space bounding sphere of an object: its mesh's unit-scale radius (`Mesh::bound_radius`,
/// the largest vertex magnitude) grown by the largest axis of its scale and its portal scale,
/// about its position -- which is where the local origin lands, so the sphere needs no rotation.
/// `None` for an object with no mesh, or one whose mesh is empty or collider-only (zero radius):
/// there is nothing to bound, and nothing to draw either.
pub fn object_sphere(obj: &Object) -> Option<(Vector3, f32)> {
    let mesh = obj.mesh.as_ref()?;
    if mesh.bound_radius <= 0.0 {
        return None;
    }
    let s = obj.scale * obj.p_scale;
    Some((obj.pos, mesh.bound_radius * s.x.abs().max(s.y.abs()).max(s.z.abs())))
}

/// Six planes as `(a, b, c, d)`, inside where `a x + b y + c z + d >= 0`.
#[derive(Clone, Copy, Debug)]
pub struct Frustum {
    planes: [[f32; 4]; 6],
}

impl Frustum {
    /// Extract the planes from a combined projection * view matrix (`Camera::matrix()`).
    pub fn from_view_proj(m: &Matrix4) -> Frustum {
        let row = |r: usize| [m.m[r * 4], m.m[r * 4 + 1], m.m[r * 4 + 2], m.m[r * 4 + 3]];
        let (r0, r1, r2, r3) = (row(0), row(1), row(2), row(3));
        let add = |a: [f32; 4], b: [f32; 4]| [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]];
        let sub = |a: [f32; 4], b: [f32; 4]| [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]];
        let mut planes = [
            add(r3, r0), // left:   w + x >= 0
            sub(r3, r0), // right:  w - x >= 0
            add(r3, r1), // bottom: w + y >= 0
            sub(r3, r1), // top:    w - y >= 0
            add(r3, r2), // near:   w + z >= 0
            sub(r3, r2), // far:    w - z >= 0
        ];
        for p in planes.iter_mut() {
            let len = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            // A degenerate plane (zero normal) can only come from a degenerate matrix; leave it
            // as-is rather than divide by zero, and it will simply never reject anything.
            if len > 1e-12 {
                for v in p.iter_mut() {
                    *v /= len;
                }
            }
        }
        Frustum { planes }
    }

    /// Signed distance of a point from one plane; positive is inside.
    #[inline]
    fn dist(p: &[f32; 4], c: Vector3) -> f32 {
        p[0] * c.x + p[1] * c.y + p[2] * c.z + p[3]
    }

    /// Whether a sphere touches the frustum. Conservative: a sphere that sits in the region
    /// past a corner, outside the frustum but inside every plane, is reported visible -- that
    /// is the usual trade, and it costs a draw, never a hole.
    pub fn sphere(&self, c: Vector3, r: f32) -> bool {
        self.planes.iter().all(|p| Self::dist(p, c) >= -r)
    }

    /// Whether an axis-aligned box touches the frustum. Tests the box corner furthest along each
    /// plane's normal (the "p-vertex"); if that is outside, the whole box is. Same conservative
    /// corner case as `sphere`.
    pub fn aabb(&self, min: Vector3, max: Vector3) -> bool {
        self.planes.iter().all(|p| {
            let v = Vector3::new(
                if p[0] >= 0.0 { max.x } else { min.x },
                if p[1] >= 0.0 { max.y } else { min.y },
                if p[2] >= 0.0 { max.z } else { min.z },
            );
            Self::dist(p, v) >= 0.0
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::Camera;

    /// A camera at the origin looking down -z, the engine's default orientation, 16:9 at the
    /// intro's clip range.
    fn cam() -> Camera {
        let mut c = Camera::new();
        c.set_size(1600, 900, 0.1, 100.0);
        c
    }

    #[test]
    fn point_in_front_is_inside_and_behind_is_outside() {
        let f = Frustum::from_view_proj(&cam().matrix());
        assert!(f.sphere(Vector3::new(0.0, 0.0, -10.0), 0.0));
        assert!(!f.sphere(Vector3::new(0.0, 0.0, 10.0), 0.0));
        // Past the far plane, and before the near plane.
        assert!(!f.sphere(Vector3::new(0.0, 0.0, -101.0), 0.0));
        assert!(f.sphere(Vector3::new(0.0, 0.0, -99.0), 0.0));
        assert!(!f.sphere(Vector3::new(0.0, 0.0, -0.05), 0.0));
    }

    #[test]
    fn sphere_reaching_back_across_the_far_plane_is_kept() {
        let f = Frustum::from_view_proj(&cam().matrix());
        assert!(f.sphere(Vector3::new(0.0, 0.0, -98.0), 1.0));
        assert!(!f.sphere(Vector3::new(0.0, 0.0, -102.0), 1.0));
        // Centred past the far plane but reaching back across it: a draw, not a hole.
        assert!(f.sphere(Vector3::new(0.0, 0.0, -102.0), 3.0));
    }

    #[test]
    fn side_planes_follow_the_field_of_view() {
        let f = Frustum::from_view_proj(&cam().matrix());
        // GH_FOV is 60 degrees vertical: at z = -10 the half-height is 10 tan(30) = 5.77, and
        // at 16:9 the half-width is 10.26.
        assert!(f.sphere(Vector3::new(0.0, 5.5, -10.0), 0.0));
        assert!(!f.sphere(Vector3::new(0.0, 6.0, -10.0), 0.0));
        assert!(f.sphere(Vector3::new(10.0, 0.0, -10.0), 0.0));
        assert!(!f.sphere(Vector3::new(10.5, 0.0, -10.0), 0.0));
        // A radius rescues a centre that is just outside.
        assert!(f.sphere(Vector3::new(0.0, 6.0, -10.0), 1.0));
        assert!(!f.sphere(Vector3::new(0.0, 8.0, -10.0), 1.0));
    }

    #[test]
    fn aabb_uses_the_nearest_corner() {
        let f = Frustum::from_view_proj(&cam().matrix());
        // A box straddling the left plane is visible...
        assert!(f.aabb(Vector3::new(-30.0, -1.0, -11.0), Vector3::new(-9.0, 1.0, -9.0)));
        // ...one entirely beyond it is not...
        assert!(!f.aabb(Vector3::new(-30.0, -1.0, -11.0), Vector3::new(-12.0, 1.0, -9.0)));
        // ...and one behind the camera is not, however big.
        assert!(!f.aabb(Vector3::new(-100.0, -100.0, 1.0), Vector3::new(100.0, 100.0, 200.0)));
        // A box enclosing the whole frustum is visible.
        assert!(f.aabb(Vector3::new(-200.0, -200.0, -200.0), Vector3::new(200.0, 200.0, 200.0)));
    }

    #[test]
    fn follows_the_view_matrix() {
        // Turn the camera to look down +x: what was in front is now off to the side.
        let mut c = cam();
        c.set_position_orientation(Vector3::new(5.0, 0.0, 0.0), 0.0, std::f32::consts::FRAC_PI_2);
        let f = Frustum::from_view_proj(&c.matrix());
        let fwd = c.world_view.inverse().mul_direction(Vector3::new(0.0, 0.0, -1.0));
        assert!(f.sphere(Vector3::new(5.0, 0.0, 0.0) + fwd * 10.0, 0.0));
        assert!(!f.sphere(Vector3::new(5.0, 0.0, 0.0) - fwd * 10.0, 0.0));
        assert!(!f.sphere(Vector3::new(5.0, 0.0, -10.0), 0.0));
    }
}
