//! EXT: carving an axis-aligned box out of a triangle soup. Not part of the C++ port.
//!
//! Scenery sometimes needs a hole where a model has a wall: the elevator (`ext/elevator.rs`)
//! is set into the end wall of a corridor, and the wall's triangles would both stop the
//! player on the threshold and be drawn across the open doorway, 3 cm behind the leaves,
//! with the cabin hidden behind them. The glTF loader carves such boxes out of a model as it
//! is parsed (`Load::cut_boxes`, `ext/gltf_model.rs`), so the same triangles are gone from
//! what is drawn and from what collides.
//!
//! The clip is generic over the vertex. A collision soup is positions only; a drawn mesh
//! carries a UV, a normal and a tangent per vertex, and a vertex made on a clipped edge has
//! to carry those interpolated along the edge or the texture would tear at the cut. The
//! caller says where a vertex is ([`clip_outside_box`]'s `at`) and how to make one a
//! fraction of the way along an edge (`lerp`), and gets back convex polygons with the
//! triangle's own winding.
//!
//! Convexity makes the clip simple: a triangle is a convex polygon, and a convex polygon
//! split by a plane is two convex polygons, so each of the box's six faces in turn peels off
//! the piece outside it (handed back whole, to be fanned into triangles) and passes the piece
//! inside to the next face. Whatever is still inside after the sixth face is inside the box,
//! and dropped.

use crate::vector::Vector3;

/// Whether the triangle lies wholly outside the box `[lo, hi]` on some axis -- touching a
/// face counts as outside -- and so is kept untouched by a cut. Separate from the clip so a
/// caller can keep the triangle's own vertices (and their indices) rather than copies.
pub fn clear_of_box(tri: &[Vector3; 3], lo: Vector3, hi: Vector3) -> bool {
    let axis = |p: Vector3, k: usize| [p.x, p.y, p.z][k];
    let (lo, hi) = ([lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z]);
    (0..3).any(|k| {
        tri.iter().all(|p| axis(*p, k) <= lo[k]) || tri.iter().all(|p| axis(*p, k) >= hi[k])
    })
}

/// The part of `tri` outside the box `[lo, hi]`, as convex polygons handed to `emit` one at a
/// time, each wound as `tri` is. `at` reads a vertex's position; `lerp(a, b, t)` makes the
/// vertex the fraction `t` of the way from `a` to `b`, every attribute blended. A triangle
/// wholly inside the box emits nothing; one that [`clear_of_box`] says is clear is emitted
/// whole, though a caller that has asked first need not ask again.
pub fn clip_outside_box<V: Copy>(
    tri: [V; 3],
    lo: Vector3,
    hi: Vector3,
    at: impl Fn(&V) -> Vector3,
    lerp: impl Fn(&V, &V, f32) -> V,
    mut emit: impl FnMut(&[V]),
) {
    if clear_of_box(&[at(&tri[0]), at(&tri[1]), at(&tri[2])], lo, hi) {
        emit(&tri);
        return;
    }
    let axis = |p: Vector3, k: usize| [p.x, p.y, p.z][k];
    let (lo, hi) = ([lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z]);
    // Six planes: for each axis, "below lo" is outside, then "above hi" is outside.
    let mut inside: Vec<V> = tri.to_vec();
    for k in 0..3 {
        for (bound, sign) in [(lo[k], -1.0f32), (hi[k], 1.0)] {
            // Signed distance: positive outside the box on this face.
            let d = |v: &V| sign * (axis(at(v), k) - bound);
            let mut outside_poly: Vec<V> = Vec::new();
            let mut inside_poly: Vec<V> = Vec::new();
            for i in 0..inside.len() {
                let (a, b) = (&inside[i], &inside[(i + 1) % inside.len()]);
                let (da, db) = (d(a), d(b));
                // A vertex on the plane belongs to both pieces as it is; only an edge that
                // truly crosses makes a new one, so no piece gets a vertex twice.
                if da >= 0.0 {
                    outside_poly.push(*a);
                }
                if da <= 0.0 {
                    inside_poly.push(*a);
                }
                if da * db < 0.0 {
                    let x = lerp(a, b, da / (da - db));
                    outside_poly.push(x);
                    inside_poly.push(x);
                }
            }
            if outside_poly.len() >= 3 {
                emit(&outside_poly);
            }
            inside = inside_poly;
            if inside.len() < 3 {
                return;
            }
        }
    }
    // `inside` is now the part within the box: dropped.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ext::trimesh::TriMeshCollider;
    use crate::vector::Matrix4;

    /// A positions-only cut, the shape the collider tests want: the soup with the box carved
    /// out, polygons fanned into triangles.
    fn cut_box(
        positions: &[[f32; 3]],
        indices: &[u32],
        lo: Vector3,
        hi: Vector3,
    ) -> (Vec<[f32; 3]>, Vec<u32>) {
        let v = |i: u32| Vector3::from_slice(&positions[i as usize]);
        let mut out_pos: Vec<[f32; 3]> = Vec::new();
        let mut out_idx: Vec<u32> = Vec::new();
        for t in indices.chunks_exact(3) {
            clip_outside_box(
                [v(t[0]), v(t[1]), v(t[2])],
                lo,
                hi,
                |p| *p,
                |a, b, t| *a + (*b - *a) * t,
                |poly| {
                    let base = out_pos.len() as u32;
                    out_pos.extend(poly.iter().map(|p| [p.x, p.y, p.z]));
                    for i in 1..poly.len() as u32 - 1 {
                        out_idx.extend_from_slice(&[base, base + i, base + i + 1]);
                    }
                },
            );
        }
        (out_pos, out_idx)
    }

    /// Area sum of a soup, as a check that a cut neither loses nor invents surface.
    fn area(pos: &[[f32; 3]], idx: &[u32]) -> f32 {
        idx.chunks_exact(3)
            .map(|t| {
                let v = |i: u32| Vector3::from_slice(&pos[i as usize]);
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
            let v = |i: u32| Vector3::from_slice(&pos[i as usize]);
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
        let tri = [Vector3::zero(), Vector3::new(1.0, 0.0, 0.0), Vector3::new(0.0, 1.0, 0.0)];
        let far = (Vector3::new(5.0, 5.0, 5.0), Vector3::new(6.0, 6.0, 6.0));
        assert!(clear_of_box(&tri, far.0, far.1));
        let (pos, cut) = cut_box(&p, &idx, far.0, far.1);
        assert_eq!((pos.len(), cut.len()), (3, 3));
        let around = (Vector3::new(-1.0, -1.0, -1.0), Vector3::new(2.0, 2.0, 2.0));
        assert!(!clear_of_box(&tri, around.0, around.1));
        let (_, cut) = cut_box(&p, &idx, around.0, around.1);
        assert!(cut.is_empty());
        // Box beginning exactly on the triangle's far edge: the triangle is not inside it.
        let edge = (Vector3::new(1.0, -1.0, -1.0), Vector3::new(2.0, 2.0, 2.0));
        assert!(clear_of_box(&tri, edge.0, edge.1));
        let (_, cut) = cut_box(&p, &idx, edge.0, edge.1);
        assert_eq!(cut.len(), 3);
    }
}
