//! EXT: view-frustum culling. Not part of the C++ port.
//!
//! The ported renderer draws every object in every pass and lets the GPU clip (Engine.cpp:256-259).
//! That is fine for the original's few-hundred-triangle rooms, and wrong once a scene carries a
//! 70,000-triangle model that is 1,000 units from the camera for most of the frame: the vertex
//! work is paid in full before the rasteriser throws it all away, and paid again in every portal
//! framebuffer. A bounding-sphere test against the pass camera's six planes costs six dot
//! products and removes the whole draw.
//!
//! # Why the planes come from the matrix and not from the FOV
//!
//! `Camera::clip_oblique` (Camera.cpp:68-85) rewrites the projection's third row so the near
//! plane coincides with the portal being looked through. A frustum assembled from the nominal
//! FOV and near/far would be wrong in exactly those passes. Extracting the planes from
//! `cam.matrix()` instead reproduces the clip volume the GPU actually uses -- the six
//! inequalities `-w <= x,y,z <= w` -- oblique near plane included, so the test can never cull
//! something the rasteriser would have drawn.

use crate::vector::{Matrix4, Vector3};

/// Six clip planes `[a, b, c, d]` with `a*x + b*y + c*z + d >= 0` on the inside, each
/// normalised so the left-hand side is a signed distance in world units.
pub struct Frustum {
    planes: [[f32; 4]; 6],
}

impl Frustum {
    /// Planes of the clip volume of a row-major view-projection matrix `m` (clip = m * p).
    pub fn from_view_proj(m: &Matrix4) -> Frustum {
        // Rows of the matrix, as [x, y, z, w] coefficients.
        let row = |r: usize| [m.m[r * 4], m.m[r * 4 + 1], m.m[r * 4 + 2], m.m[r * 4 + 3]];
        let (r0, r1, r2, r3) = (row(0), row(1), row(2), row(3));
        let combine = |sign: f32, r: [f32; 4]| {
            let mut p = [0.0f32; 4];
            for k in 0..4 {
                p[k] = r3[k] + sign * r[k];
            }
            let len = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            // A zero normal cannot happen for a real projection; guard rather than divide.
            if len > 1e-20 {
                for v in p.iter_mut() {
                    *v /= len;
                }
            }
            p
        };
        // x >= -w, x <= w, y >= -w, y <= w, z >= -w, z <= w: left, right, bottom, top, near, far.
        Frustum {
            planes: [
                combine(1.0, r0),
                combine(-1.0, r0),
                combine(1.0, r1),
                combine(-1.0, r1),
                combine(1.0, r2),
                combine(-1.0, r2),
            ],
        }
    }

    /// `true` if any part of the sphere may be inside the volume. Conservative: a sphere
    /// straddling a corner can pass while being outside, which only costs a draw, never drops one.
    pub fn sphere(&self, c: Vector3, r: f32) -> bool {
        self.planes
            .iter()
            .all(|p| p[0] * c.x + p[1] * c.y + p[2] * c.z + p[3] >= -r)
    }

    /// `true` if any part of the axis-aligned box `[min, max]` may be inside the volume. Same
    /// conservative reading as `sphere`: tests the box's most-inside corner against each plane.
    ///
    /// The box half of the helper, for things that are boxes -- terrain tiles and grass cells,
    /// whose sphere would be so much larger than them that it would rarely cull. glTF parts use
    /// `sphere`; nothing else in this branch of the tree is drawn in cells yet.
    #[allow(dead_code)] // The tile/cell test; exercised by the tests below.
    pub fn aabb(&self, min: Vector3, max: Vector3) -> bool {
        self.planes.iter().all(|p| {
            let x = if p[0] >= 0.0 { max.x } else { min.x };
            let y = if p[1] >= 0.0 { max.y } else { min.y };
            let z = if p[2] >= 0.0 { max.z } else { min.z };
            p[0] * x + p[1] * y + p[2] * z + p[3] >= 0.0
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::Camera;
    use crate::game_header::{GH_FAR, GH_NEAR_MAX};

    /// A camera at the origin looking down -z, as `Camera::set_position_orientation` builds it.
    fn camera() -> Camera {
        let mut cam = Camera::new();
        cam.set_size(1280, 720, GH_NEAR_MAX, GH_FAR);
        cam.set_position_orientation(Vector3::zero(), 0.0, 0.0);
        cam
    }

    #[test]
    fn sphere_ahead_is_kept_and_sphere_behind_is_culled() {
        let f = Frustum::from_view_proj(&camera().matrix());
        assert!(f.sphere(Vector3::new(0.0, 0.0, -10.0), 1.0));
        assert!(!f.sphere(Vector3::new(0.0, 0.0, 10.0), 1.0));
    }

    #[test]
    fn far_plane_is_the_camera_far() {
        let f = Frustum::from_view_proj(&camera().matrix());
        assert!(f.sphere(Vector3::new(0.0, 0.0, -(GH_FAR - 2.0)), 1.0));
        assert!(!f.sphere(Vector3::new(0.0, 0.0, -(GH_FAR + 2.0)), 1.0));
        // A sphere centred past the far plane but reaching back across it is kept.
        assert!(f.sphere(Vector3::new(0.0, 0.0, -(GH_FAR + 2.0)), 3.0));
    }

    #[test]
    fn side_planes_follow_the_field_of_view() {
        let f = Frustum::from_view_proj(&camera().matrix());
        // Vertical FOV is 60 degrees: at z=-10 the view is ~5.77 tall either side.
        assert!(f.sphere(Vector3::new(0.0, 5.0, -10.0), 0.1));
        assert!(!f.sphere(Vector3::new(0.0, 7.0, -10.0), 0.1));
        // ...and 16:9 wider: ~10.3 either side horizontally.
        assert!(f.sphere(Vector3::new(9.0, 0.0, -10.0), 0.1));
        assert!(!f.sphere(Vector3::new(12.0, 0.0, -10.0), 0.1));
    }

    #[test]
    fn aabb_matches_sphere_on_the_easy_cases() {
        let f = Frustum::from_view_proj(&camera().matrix());
        assert!(f.aabb(
            Vector3::new(-1.0, -1.0, -11.0),
            Vector3::new(1.0, 1.0, -9.0)
        ));
        assert!(!f.aabb(Vector3::new(-1.0, -1.0, 9.0), Vector3::new(1.0, 1.0, 11.0)));
        // A huge box that surrounds the camera is inside.
        assert!(f.aabb(Vector3::splat(-1000.0), Vector3::splat(1000.0)));
        // Off to one side, entirely past the right plane.
        assert!(!f.aabb(
            Vector3::new(50.0, -1.0, -11.0),
            Vector3::new(52.0, 1.0, -9.0)
        ));
    }

    /// The planes must move with the camera, not sit at the origin.
    #[test]
    fn follows_the_camera_transform() {
        let mut cam = camera();
        cam.set_position_orientation(Vector3::new(1000.0, 0.0, 0.0), 0.0, 0.0);
        let f = Frustum::from_view_proj(&cam.matrix());
        assert!(f.sphere(Vector3::new(1000.0, 0.0, -10.0), 1.0));
        assert!(!f.sphere(Vector3::new(0.0, 0.0, -10.0), 1.0));
    }
}
