//! EXT: ray casting. Not part of the C++ port.
//!
//! The original engine only ever asks "does this unit sphere overlap that rectangle"
//! (`Collider::Collide`, Collider.cpp:23-45). Forced-perspective grabbing needs the other
//! question -- "where along this ray does the world first block it" -- so this module adds
//! ray/rectangle intersection on top of the ported `Collider`.
//!
//! A `Collider` is a rectangle stored as a matrix: the translation is its centre, and its X
//! and Y axes are the two half-extent vectors (see `Collider::create_sorted`, Collider.cpp:69-75).
//! That makes ray/rectangle intersection a plane hit followed by two clamped axis tests --
//! the same projection the collision routine already does, just solved for `t` instead.

use crate::object::ObjectT;
use crate::vector::{Matrix4, Vector3};
use std::cell::RefCell;
use std::rc::Rc;

/// A single ray/world intersection.
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // EXT: full hit record; later scenes use point/normal/object.
pub struct RayHit {
    /// Distance along the ray, in world units.
    pub dist: f32,
    /// World-space point of contact.
    pub point: Vector3,
    /// World-space surface normal, already flipped to face the incoming ray.
    pub normal: Vector3,
    /// Index into the engine's object vector of whatever was hit.
    pub object: usize,
}

/// Intersect a ray with one collider rectangle that has been placed in the world by `obj_to_world`.
///
/// `dir` must be normalised. Returns the distance along the ray, or `None` when the ray misses,
/// is parallel to the rectangle, or would hit behind the origin.
pub fn ray_collider(
    origin: Vector3,
    dir: Vector3,
    obj_to_world: &Matrix4,
    collider: &crate::collider::Collider,
) -> Option<(f32, Vector3)> {
    // Push the rectangle into world space. Same composition Engine::Update uses to build
    // `localToUnit` (Engine.cpp:170-171), just without the sphere normalisation step.
    let m = *obj_to_world * *collider.mat();

    let centre = m.translation();
    let ex = m.x_axis(); // half-extent along the rectangle's local X
    let ey = m.y_axis(); // half-extent along the rectangle's local Y

    // Plane normal. Not normalised by construction, so do it here.
    let n = ex.cross(ey);
    let n_mag_sq = n.mag_sq();
    if n_mag_sq < 1e-20 {
        return None; // degenerate rectangle
    }
    let n = n / n_mag_sq.sqrt();

    let denom = dir.dot(n);
    if denom.abs() < 1e-9 {
        return None; // ray runs parallel to the plane
    }

    let t = (centre - origin).dot(n) / denom;
    if t <= 1e-5 {
        return None; // behind the ray origin, or touching it
    }

    // Inside the rectangle? Project onto each half-extent; |proj| must be within its own length.
    let d = (origin + dir * t) - centre;
    let ex_mag_sq = ex.mag_sq();
    let ey_mag_sq = ey.mag_sq();
    if ex_mag_sq < 1e-20 || ey_mag_sq < 1e-20 {
        return None;
    }
    if (d.dot(ex) / ex_mag_sq).abs() > 1.0 {
        return None;
    }
    if (d.dot(ey) / ey_mag_sq).abs() > 1.0 {
        return None;
    }

    // Face the normal back toward the ray so callers can offset along it without sign juggling.
    let normal = if denom > 0.0 { -n } else { n };
    Some((t, normal))
}

/// Cast a ray against every collider of every object in the scene and return the nearest hit.
///
/// `skip` lets the caller ignore an object -- used so a held item does not block the ray that
/// is positioning it, and so the player never picks themselves.
pub fn raycast(
    objects: &[Rc<RefCell<dyn ObjectT>>],
    origin: Vector3,
    dir: Vector3,
    max_dist: f32,
    skip: Option<usize>,
) -> Option<RayHit> {
    let mut best: Option<RayHit> = None;

    for (i, obj) in objects.iter().enumerate() {
        if Some(i) == skip {
            continue;
        }
        // `try_borrow` rather than `borrow`: this can run while the engine holds a borrow on
        // an object elsewhere in the update, and a ray miss is far better than a panic.
        let Ok(o) = obj.try_borrow() else { continue };
        let base = o.base();

        // Triangle-mesh scenery (ext/trimesh.rs) first: it is already in world space.
        if let Some(tm) = o.trimesh() {
            if let Some((t, normal)) = tm.cast_ray(origin, dir, max_dist) {
                if best.is_none_or(|b| t < b.dist) {
                    best = Some(RayHit { dist: t, point: origin + dir * t, normal, object: i });
                }
            }
        }

        let Some(mesh) = base.mesh.as_ref() else {
            continue;
        };
        if mesh.colliders.is_empty() {
            continue;
        }

        let to_world = base.local_to_world();
        for collider in &mesh.colliders {
            let Some((t, normal)) = ray_collider(origin, dir, &to_world, collider) else {
                continue;
            };
            if t > max_dist {
                continue;
            }
            if best.is_none_or(|b| t < b.dist) {
                best = Some(RayHit { dist: t, point: origin + dir * t, normal, object: i });
            }
        }
    }

    best
}

/// Ray/sphere test, used for picking grabbable props (whose meshes may carry no colliders --
/// bunny, teapot and suzanne all have zero, verified against the asset set).
pub fn ray_sphere(origin: Vector3, dir: Vector3, centre: Vector3, radius: f32) -> Option<f32> {
    let oc = origin - centre;
    let b = oc.dot(dir);
    let c = oc.mag_sq() - radius * radius;
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let sqrt_disc = disc.sqrt();
    // Near root first; if the origin is inside the sphere, fall back to the far root.
    let t = -b - sqrt_disc;
    if t > 1e-5 {
        return Some(t);
    }
    let t_far = -b + sqrt_disc;
    if t_far > 1e-5 {
        return Some(t_far);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collider::Collider;

    /// A unit quad in the XY plane at the origin, built the way Mesh's `c` lines build one.
    fn unit_quad() -> Collider {
        Collider::new(
            Vector3::new(-1.0, -1.0, 0.0),
            Vector3::new(1.0, -1.0, 0.0),
            Vector3::new(1.0, 1.0, 0.0),
        )
    }

    #[test]
    fn ray_hits_quad_head_on() {
        let c = unit_quad();
        let hit = ray_collider(
            Vector3::new(0.0, 0.0, 5.0),
            Vector3::new(0.0, 0.0, -1.0),
            &Matrix4::identity(),
            &c,
        );
        let (t, n) = hit.expect("ray down -Z should hit a quad in the XY plane");
        assert!((t - 5.0).abs() < 1e-4, "expected t=5, got {t}");
        // Normal must face back along the incoming ray, i.e. +Z.
        assert!(n.z > 0.9, "normal should face the ray, got {n:?}");
    }

    #[test]
    fn ray_misses_outside_rectangle() {
        let c = unit_quad();
        assert!(ray_collider(
            Vector3::new(9.0, 0.0, 5.0),
            Vector3::new(0.0, 0.0, -1.0),
            &Matrix4::identity(),
            &c,
        )
        .is_none());
    }

    #[test]
    fn ray_ignores_geometry_behind_it() {
        let c = unit_quad();
        assert!(ray_collider(
            Vector3::new(0.0, 0.0, 5.0),
            Vector3::new(0.0, 0.0, 1.0), // pointing away
            &Matrix4::identity(),
            &c,
        )
        .is_none());
    }

    #[test]
    fn ray_respects_object_transform() {
        let c = unit_quad();
        // Push the quad 10 further down -Z; the hit distance should move with it.
        let to_world = Matrix4::trans(Vector3::new(0.0, 0.0, -10.0));
        let (t, _) =
            ray_collider(Vector3::new(0.0, 0.0, 5.0), Vector3::new(0.0, 0.0, -1.0), &to_world, &c)
                .expect("translated quad should still be hit");
        assert!((t - 15.0).abs() < 1e-4, "expected t=15, got {t}");
    }

    #[test]
    fn sphere_hit_and_miss() {
        let t = ray_sphere(
            Vector3::new(0.0, 0.0, 5.0),
            Vector3::new(0.0, 0.0, -1.0),
            Vector3::zero(),
            1.0,
        )
        .expect("should hit unit sphere at origin");
        assert!((t - 4.0).abs() < 1e-4, "expected t=4, got {t}");

        assert!(ray_sphere(
            Vector3::new(0.0, 9.0, 5.0),
            Vector3::new(0.0, 0.0, -1.0),
            Vector3::zero(),
            1.0,
        )
        .is_none());
    }
}
