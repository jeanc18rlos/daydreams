//! EXT: triangle-mesh collision. Not part of the C++ port.
//!
//! The engine's only collision primitive is the axis-aligned rectangle a mesh declares on its
//! `c` lines (`Collider::Collide`, Collider.cpp:23-45), tested against each of a `Physical`'s
//! hit spheres. That suits hand-built rooms and is hopeless for a scanned interior: the
//! backrooms model is 70,000 triangles of walls, skirting, furniture and lamps, none of it
//! axis-aligned in any useful sense and none of it annotated. So this module answers the same
//! question the rectangle path answers -- "push this sphere out of that geometry" -- against a
//! triangle soup, using parry3d's BVH to find the handful of triangles near the sphere.
//!
//! # Static, in world space
//!
//! The mesh is built ONCE, with the owning object's transform already applied. The engine's
//! collision pass (engine.rs) therefore hands this module world-space spheres and gets
//! world-space pushes back, with no per-step matrix work over 70k vertices. The price is that
//! the object must never move after `new`: a `Backrooms` that translated its base would leave
//! its walls behind. Scenery is the only thing this is for.
//!
//! # One push at a time
//!
//! `push_sphere` reports a single push -- for the DEEPEST penetration -- and the engine applies
//! it and asks again, up to [`MAX_PUSHES`] times. This mirrors how rectangle colliders are
//! applied: sequentially, each followed by a matrix rebuild (Engine.cpp:176-190), so a sphere in
//! a corner is resolved against one wall, then the other. Deepest-first rather than first-found
//! because adjacent triangles of one flat surface all report the same push: resolving the deepest
//! clears its coplanar neighbours in the same step, where a shallow edge contact resolved first
//! can leave the sphere still inside the face and costs another round.
//!
//! # Openings
//!
//! Scenery sometimes needs a hole where the scan has a wall: the Backrooms' elevator is set
//! into the end wall of a corridor, and the wall's triangles would stop the player on the
//! threshold. That is not done here but in the glTF loader, which carves boxes out of a model
//! as it is parsed (`Load::cut_boxes`, `ext/carve.rs`), so the triangles a collider is built
//! from are the ones that are drawn -- a wall is gone from both or from neither.

use std::sync::Arc;

use crate::vector::{Matrix4, Vector3};
use parry3d::math::Vector as PVec;
use parry3d::query::{PointQuery, Ray, RayCast};
use parry3d::shape::{TriMesh, TriMeshFlags};

/// Rounds of `push_sphere` the engine runs per hit sphere per step. Eight is more than any
/// corner needs -- three mutually perpendicular faces resolve in three -- and bounds the cost of
/// a sphere somehow wedged between surfaces that keep pushing it into each other.
pub const MAX_PUSHES: usize = 8;

/// Triangles whose doubled area squared is under this are dropped at build: a sliver a tenth
/// of a millimetre on a side, in metres. Far below anything a player could touch, far above
/// float noise.
const DEGENERATE_AREA_SQ: f32 = 1e-16;

pub struct TriMeshCollider {
    /// Shared, because the rigid-body world (`ext/physics.rs`) collides its props with the
    /// same mesh: one BVH build per scene load serves both, where a second copy would cost
    /// as much again (16 ms for the Backrooms' 70k triangles).
    mesh: Arc<TriMesh>,
}

fn to_p(v: Vector3) -> PVec {
    PVec::new(v.x, v.y, v.z)
}

fn from_p(v: PVec) -> Vector3 {
    Vector3::new(v.x, v.y, v.z)
}

impl TriMeshCollider {
    /// Build from local-space triangles and the object's `local_to_world`, which is baked in.
    ///
    /// Zero-area triangles (which scanned models do contain) are dropped here: projecting a
    /// point onto one yields NaN, and a NaN push would poison the player's position for good.
    /// parry's own `DELETE_DEGENERATE_TRIANGLES` only catches repeated indices, not three
    /// distinct collinear points, so the area test is ours.
    pub fn new(
        positions: &[[f32; 3]],
        indices: &[u32],
        local_to_world: &Matrix4,
    ) -> TriMeshCollider {
        let vertices: Vec<PVec> = positions
            .iter()
            .map(|p| to_p(local_to_world.mul_point(Vector3::new(p[0], p[1], p[2]))))
            .collect();
        let tris: Vec<[u32; 3]> = indices
            .chunks_exact(3)
            .map(|t| [t[0], t[1], t[2]])
            .filter(|t| {
                let (a, b, c) =
                    (vertices[t[0] as usize], vertices[t[1] as usize], vertices[t[2] as usize]);
                (b - a).cross(c - a).length_squared() > DEGENERATE_AREA_SQ
            })
            .collect();
        // FIX_INTERNAL_EDGES is for the rigid bodies (`ext/physics.rs`): it gives every
        // triangle the pseudo-normals of its edges, so a body rolling across a flat floor of
        // many triangles is not bumped at each shared edge by that triangle's own normal. It
        // merges duplicate vertices on the way (the same geometry, fewer vertices) and costs
        // about half as much again as the BVH alone. Nothing here reads the pseudo-normals:
        // `push_sphere` projects onto triangles one at a time, and a ray cast never did.
        let mesh = TriMesh::with_flags(vertices, tris, TriMeshFlags::FIX_INTERNAL_EDGES)
            .expect("trimesh collider: no triangles survived");
        TriMeshCollider { mesh: Arc::new(mesh) }
    }

    /// How many triangles survived the build.
    pub fn num_triangles(&self) -> usize {
        self.mesh.num_triangles()
    }

    /// The mesh itself, shared: what the rigid-body world's fixed collider is built on.
    pub fn shape(&self) -> Arc<TriMesh> {
        Arc::clone(&self.mesh)
    }

    /// The push that moves a sphere out of its deepest penetration, or `None` if it touches
    /// nothing. See the module docs for why one push, and why the deepest.
    pub fn push_sphere(&self, centre: Vector3, radius: f32) -> Option<Vector3> {
        let c = to_p(centre);
        let aabb = parry3d::bounding_volume::Aabb::from_half_extents(c, PVec::splat(radius));
        let mut best: Option<(f32, Vector3)> = None;
        for i in self.mesh.bvh().intersect_aabb(&aabb) {
            let tri = self.mesh.triangle(i);
            let proj = tri.project_local_point(c, false);
            let away = from_p(c - proj.point);
            let dist = away.mag();
            if dist >= radius {
                continue;
            }
            let depth = radius - dist;
            if best.is_some_and(|(d, _)| d >= depth) {
                continue;
            }
            // Direction to push: away from the closest point, or along the face normal if the
            // centre sits exactly on the triangle and there is no "away" to speak of.
            let dir =
                if dist > 1e-6 { away / dist } else { from_p(tri.normal().unwrap_or(PVec::Y)) };
            best = Some((depth, dir * depth));
        }
        best.map(|(_, push)| push)
    }

    /// Nearest hit along a ray: `(distance, normal)`, the normal flipped to face the ray the
    /// way `ext::raycast::ray_collider` does, so callers can offset along it without sign
    /// juggling. `dir` must be normalised.
    pub fn cast_ray(&self, origin: Vector3, dir: Vector3, max_dist: f32) -> Option<(f32, Vector3)> {
        let ray = Ray::new(to_p(origin), to_p(dir));
        let hit = self.mesh.cast_local_ray_and_get_normal(&ray, max_dist, true)?;
        if hit.time_of_impact <= 1e-5 {
            return None;
        }
        let mut n = from_p(hit.normal).normalized_safe();
        if n.dot(dir) > 0.0 {
            n = -n;
        }
        Some((hit.time_of_impact, n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A closed unit cube `[-1, 1]^3` as 12 triangles, wound outward.
    fn cube() -> TriMeshCollider {
        let p = |x: f32, y: f32, z: f32| [x, y, z];
        let pos = [
            p(-1.0, -1.0, -1.0),
            p(1.0, -1.0, -1.0),
            p(1.0, 1.0, -1.0),
            p(-1.0, 1.0, -1.0),
            p(-1.0, -1.0, 1.0),
            p(1.0, -1.0, 1.0),
            p(1.0, 1.0, 1.0),
            p(-1.0, 1.0, 1.0),
        ];
        #[rustfmt::skip]
        let idx: [u32; 36] = [
            0, 2, 1, 0, 3, 2, // -z
            4, 5, 6, 4, 6, 7, // +z
            0, 1, 5, 0, 5, 4, // -y
            3, 7, 6, 3, 6, 2, // +y
            0, 4, 7, 0, 7, 3, // -x
            1, 2, 6, 1, 6, 5, // +x
        ];
        TriMeshCollider::new(&pos, &idx, &Matrix4::identity())
    }

    /// Surfaces have no inside: a sphere is kept on whichever side of a face its centre is on,
    /// which is what lets single-sided scanned walls stop the player. Inside the box, 0.1 from
    /// the +x face with radius 0.2, the sphere is therefore pushed BACK toward the box's middle
    /// until it just touches that face.
    #[test]
    fn sphere_inside_box_is_pushed_off_the_nearest_face() {
        let c = cube();
        let push = c.push_sphere(Vector3::new(0.9, 0.0, 0.0), 0.2).expect("touching +x face");
        assert!(
            (push.x + 0.1).abs() < 1e-5 && push.y.abs() < 1e-6 && push.z.abs() < 1e-6,
            "{push:?}"
        );
        // At x = 0.8 it rests on the face and is left alone from then on.
        let rest = c.push_sphere(Vector3::new(0.9, 0.0, 0.0) + push, 0.2);
        assert!(rest.is_none_or(|p| p.mag() < 1e-5), "{rest:?}");
        // Well inside, it touches nothing.
        assert!(c.push_sphere(Vector3::zero(), 0.2).is_none());
    }

    #[test]
    fn deepest_penetration_wins() {
        let c = cube();
        // 0.05 into the +x face and 0.15 into the +y face: the y push comes first, alone.
        let push = c.push_sphere(Vector3::new(0.85, 0.95, 0.0), 0.2).expect("corner contact");
        assert!((push.y + 0.15).abs() < 1e-5 && push.x.abs() < 1e-6, "{push:?}");
        // A second round then clears the x face, the way the engine loops.
        let push2 = c.push_sphere(Vector3::new(0.85, 0.95, 0.0) + push, 0.2).expect("x face next");
        assert!((push2.x + 0.05).abs() < 1e-5 && push2.y.abs() < 1e-6, "{push2:?}");
    }

    #[test]
    fn floor_contact_pushes_up() {
        // The cube's top face, seen from above, is a floor: a sphere resting 0.1 into it.
        let c = cube();
        let push =
            c.push_sphere(Vector3::new(0.0, 1.1, 0.0), 0.2).expect("standing on the top face");
        assert!(push.y > 0.0 && (push.y - 0.1).abs() < 1e-5, "{push:?}");
        assert!(push.normalized().y > 0.7, "Player::on_collide would not count this as ground");
    }

    #[test]
    fn world_transform_is_baked_in() {
        let p = [[-1.0, 0.0, -1.0], [1.0, 0.0, -1.0], [1.0, 0.0, 1.0], [-1.0, 0.0, 1.0]];
        let idx = [0u32, 2, 1, 0, 3, 2];
        let floor = TriMeshCollider::new(&p, &idx, &Matrix4::trans(Vector3::new(1000.0, 5.0, 0.0)));
        let push =
            floor.push_sphere(Vector3::new(1000.2, 5.1, 0.1), 0.2).expect("on the moved floor");
        assert!((push.y - 0.1).abs() < 1e-5, "{push:?}");
        assert!(floor.push_sphere(Vector3::new(0.0, 5.1, 0.0), 0.2).is_none());
    }

    #[test]
    fn ray_hits_the_expected_face_with_a_facing_normal() {
        let c = cube();
        let (t, n) = c
            .cast_ray(Vector3::new(5.0, 0.0, 0.0), Vector3::new(-1.0, 0.0, 0.0), 100.0)
            .expect("ray toward the cube");
        assert!((t - 4.0).abs() < 1e-5, "t = {t}");
        assert!(n.x > 0.99, "normal should face the ray, got {n:?}");
        // From inside, the first face along +y is the top at distance 1, and the normal is
        // flipped to face back down the ray.
        let (t, n) =
            c.cast_ray(Vector3::zero(), Vector3::new(0.0, 1.0, 0.0), 100.0).expect("inside");
        assert!((t - 1.0).abs() < 1e-5 && n.y < -0.99, "t = {t} n = {n:?}");
        // Range-limited.
        assert!(c
            .cast_ray(Vector3::new(5.0, 0.0, 0.0), Vector3::new(-1.0, 0.0, 0.0), 3.0)
            .is_none());
        // Pointing away.
        assert!(c
            .cast_ray(Vector3::new(5.0, 0.0, 0.0), Vector3::new(1.0, 0.0, 0.0), 100.0)
            .is_none());
    }

    #[test]
    fn degenerate_triangles_are_dropped() {
        let p = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        // One collinear (zero-area) triangle and one real one.
        let idx = [0u32, 1, 2, 0, 1, 3];
        let m = TriMeshCollider::new(&p, &idx, &Matrix4::identity());
        assert_eq!(m.num_triangles(), 1);
    }
}
