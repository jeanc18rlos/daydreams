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
//! threshold. [`cut_box`] carves an axis-aligned box out of a triangle soup -- every triangle
//! is clipped against the box's six planes and the pieces outside are kept, so a wall that is
//! wider than the opening keeps colliding either side of it. Collision only: what is drawn is
//! untouched, which is why the thing set into the wall has to cover the hole visually itself.

use crate::vector::{Matrix4, Vector3};
use parry3d::math::Vector as PVec;
use parry3d::query::{PointQuery, Ray, RayCast};
use parry3d::shape::TriMesh;

/// Rounds of `push_sphere` the engine runs per hit sphere per step. Eight is more than any
/// corner needs -- three mutually perpendicular faces resolve in three -- and bounds the cost of
/// a sphere somehow wedged between surfaces that keep pushing it into each other.
pub const MAX_PUSHES: usize = 8;

/// Triangles whose doubled area squared is under this are dropped at build: a sliver a tenth
/// of a millimetre on a side, in metres. Far below anything a player could touch, far above
/// float noise.
const DEGENERATE_AREA_SQ: f32 = 1e-16;

pub struct TriMeshCollider {
    mesh: TriMesh,
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
        let mesh = TriMesh::new(vertices, tris).expect("trimesh collider: no triangles survived");
        TriMeshCollider { mesh }
    }

    /// As [`TriMeshCollider::new`], with the world-space boxes in `holes` carved out of the
    /// geometry first (see [`cut_box`]). The transform is applied before the cut, so the holes
    /// are given in the space the collider answers in.
    pub fn new_with_holes(
        positions: &[[f32; 3]],
        indices: &[u32],
        local_to_world: &Matrix4,
        holes: &[(Vector3, Vector3)],
    ) -> TriMeshCollider {
        let mut pos: Vec<[f32; 3]> = positions
            .iter()
            .map(|p| {
                let w = local_to_world.mul_point(Vector3::new(p[0], p[1], p[2]));
                [w.x, w.y, w.z]
            })
            .collect();
        let mut idx = indices.to_vec();
        for &(lo, hi) in holes {
            (pos, idx) = cut_box(&pos, &idx, lo, hi);
        }
        TriMeshCollider::new(&pos, &idx, &Matrix4::identity())
    }

    /// Test-only: how many triangles survived the build.
    #[cfg(test)]
    pub fn num_triangles(&self) -> usize {
        self.mesh.num_triangles()
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

/// The triangle soup `(positions, indices)` with the axis-aligned box `[lo, hi]` carved out:
/// triangles clear of the box are kept as they are, triangles inside it are dropped, and
/// triangles crossing its faces are clipped so that exactly the part outside survives, with
/// its winding. Convexity makes it simple: a triangle is a convex polygon, and a convex polygon
/// split by a plane is two convex polygons, so each of the six faces in turn peels off the
/// outside piece (fanned back into triangles) and hands the inside piece to the next face.
/// Whatever is still inside after the sixth face is inside the box.
pub fn cut_box(
    positions: &[[f32; 3]],
    indices: &[u32],
    lo: Vector3,
    hi: Vector3,
) -> (Vec<[f32; 3]>, Vec<u32>) {
    let v = |i: u32| {
        let p = positions[i as usize];
        Vector3::new(p[0], p[1], p[2])
    };
    let axis = |p: Vector3, k: usize| [p.x, p.y, p.z][k];
    let (lo_a, hi_a) = ([lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z]);
    let mut out_pos: Vec<[f32; 3]> = Vec::with_capacity(positions.len());
    let mut out_idx: Vec<u32> = Vec::with_capacity(indices.len());
    let emit = |poly: &[Vector3], out_pos: &mut Vec<[f32; 3]>, out_idx: &mut Vec<u32>| {
        let base = out_pos.len() as u32;
        out_pos.extend(poly.iter().map(|p| [p.x, p.y, p.z]));
        for i in 1..poly.len() as u32 - 1 {
            out_idx.extend_from_slice(&[base, base + i, base + i + 1]);
        }
    };
    for t in indices.chunks_exact(3) {
        let tri = [v(t[0]), v(t[1]), v(t[2])];
        let clear = (0..3).any(|k| {
            tri.iter().all(|p| axis(*p, k) <= lo_a[k]) || tri.iter().all(|p| axis(*p, k) >= hi_a[k])
        });
        if clear {
            emit(&tri, &mut out_pos, &mut out_idx);
            continue;
        }
        // Six planes: for each axis, "below lo" is outside, then "above hi" is outside.
        let mut inside: Vec<Vector3> = tri.to_vec();
        for k in 0..3 {
            for (bound, sign) in [(lo_a[k], -1.0f32), (hi_a[k], 1.0)] {
                // Signed distance: positive outside the box on this face.
                let d = |p: Vector3| sign * (axis(p, k) - bound);
                let mut outside_poly: Vec<Vector3> = Vec::new();
                let mut inside_poly: Vec<Vector3> = Vec::new();
                for i in 0..inside.len() {
                    let (a, b) = (inside[i], inside[(i + 1) % inside.len()]);
                    let (da, db) = (d(a), d(b));
                    if da > 0.0 {
                        outside_poly.push(a);
                    } else {
                        inside_poly.push(a);
                    }
                    if (da > 0.0) != (db > 0.0) {
                        let x = a + (b - a) * (da / (da - db));
                        outside_poly.push(x);
                        inside_poly.push(x);
                    }
                }
                if outside_poly.len() >= 3 {
                    emit(&outside_poly, &mut out_pos, &mut out_idx);
                }
                inside = inside_poly;
                if inside.len() < 3 {
                    break;
                }
            }
            if inside.len() < 3 {
                break;
            }
        }
        // `inside` is now the part within the box: dropped.
    }
    (out_pos, out_idx)
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

    /// Doubled signed area sum of a soup, as a check that a cut neither loses nor invents
    /// surface.
    fn area(pos: &[[f32; 3]], idx: &[u32]) -> f32 {
        idx.chunks_exact(3)
            .map(|t| {
                let v = |i: u32| {
                    Vector3::new(pos[i as usize][0], pos[i as usize][1], pos[i as usize][2])
                };
                (v(t[1]) - v(t[0])).cross(v(t[2]) - v(t[0])).mag() * 0.5
            })
            .sum()
    }

    /// A 10 x 4 wall in the plane z = 0 (two triangles, facing +z) with a 2 x 3 doorway cut
    /// out of its middle: the doorway lets a sphere through, the wall either side of it and
    /// above it still pushes, and the surface that is left is the wall less the opening.
    #[test]
    fn cut_box_opens_a_doorway_and_keeps_the_wall_around_it() {
        let p = [[-5.0, 0.0, 0.0], [5.0, 0.0, 0.0], [5.0, 4.0, 0.0], [-5.0, 4.0, 0.0]];
        let idx = [0u32, 1, 2, 0, 2, 3];
        let (pos, cut) =
            cut_box(&p, &idx, Vector3::new(-1.0, -0.5, -0.5), Vector3::new(1.0, 3.0, 0.5));
        // 40 - 2 * 3.5 (the box reaches below the wall's foot, so the hole is 2 x 3).
        assert!((area(&pos, &cut) - (40.0 - 6.0)).abs() < 1e-4, "area {}", area(&pos, &cut));
        // Winding kept: every piece still faces +z.
        for t in cut.chunks_exact(3) {
            let v =
                |i: u32| Vector3::new(pos[i as usize][0], pos[i as usize][1], pos[i as usize][2]);
            assert!((v(t[1]) - v(t[0])).cross(v(t[2]) - v(t[0])).z > 0.0);
        }
        let wall = TriMeshCollider::new(&pos, &cut, &Matrix4::identity());
        // Through the doorway: nothing to touch.
        assert!(wall.push_sphere(Vector3::new(0.0, 1.0, 0.0), 0.2).is_none());
        assert!(wall.push_sphere(Vector3::new(0.0, 2.5, 0.1), 0.2).is_none());
        // Beside it, and above it: the wall is still there.
        assert!(wall.push_sphere(Vector3::new(2.0, 1.0, 0.1), 0.2).is_some());
        assert!(wall.push_sphere(Vector3::new(-3.0, 3.5, -0.1), 0.2).is_some());
        assert!(wall.push_sphere(Vector3::new(0.0, 3.5, 0.1), 0.2).is_some());
        // The jamb: a sphere whose edge overlaps the wall beside the opening is caught.
        assert!(wall.push_sphere(Vector3::new(1.1, 1.0, 0.1), 0.2).is_some());
    }

    /// Triangles clear of the box pass through untouched, triangles inside it vanish, and a
    /// box touching a triangle only at its edge leaves it whole.
    #[test]
    fn cut_box_leaves_the_clear_and_drops_the_contained() {
        let p = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let idx = [0u32, 1, 2];
        let (pos, cut) =
            cut_box(&p, &idx, Vector3::new(5.0, 5.0, 5.0), Vector3::new(6.0, 6.0, 6.0));
        assert_eq!((pos.len(), cut.len()), (3, 3));
        let (_, cut) =
            cut_box(&p, &idx, Vector3::new(-1.0, -1.0, -1.0), Vector3::new(2.0, 2.0, 2.0));
        assert!(cut.is_empty());
        // Box beginning exactly on the triangle's far edge: the triangle is not inside it.
        let (_, cut) =
            cut_box(&p, &idx, Vector3::new(1.0, -1.0, -1.0), Vector3::new(2.0, 2.0, 2.0));
        assert_eq!(cut.len(), 3);
    }

    #[test]
    fn holes_are_cut_in_world_space() {
        // A floor quad moved to x = 100 with a hole under x = 100: a sphere over the hole falls.
        let p = [[-1.0, 0.0, -1.0], [1.0, 0.0, -1.0], [1.0, 0.0, 1.0], [-1.0, 0.0, 1.0]];
        let idx = [0u32, 2, 1, 0, 3, 2];
        let at = Matrix4::trans(Vector3::new(100.0, 0.0, 0.0));
        let hole = (Vector3::new(99.7, -0.5, -0.3), Vector3::new(100.3, 0.5, 0.3));
        let floor = TriMeshCollider::new_with_holes(&p, &idx, &at, &[hole]);
        assert!(floor.push_sphere(Vector3::new(100.0, 0.1, 0.0), 0.2).is_none());
        assert!(floor.push_sphere(Vector3::new(100.7, 0.1, 0.0), 0.2).is_some());
    }
}
