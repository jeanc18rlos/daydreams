//! EXT: invisible scene bounds. Not part of the C++ port.
//!
//! `Meshes/bounds.obj` is a colliders-only unit box: it declares collider rectangles on its
//! four walls and ceiling but **no faces**, so there is nothing to rasterise even if a shader
//! were attached. On top of that, `Object::draw_impl` only draws when an object has BOTH a mesh
//! and a shader (Object.cpp:21), while the collision pass in `Engine::update` only requires the
//! mesh -- so an `Object` carrying `bounds.obj` with `shader: None` is never rendered yet still
//! collides. The grab-placement raycast in `ext/raycast.rs` tests every mesh's colliders too,
//! so held objects also stop at these walls; nothing extra is needed for them.
//!
//! The unit box spans x[-1,1], y[0,1], z[-1,1] and has **no floor collider**. Because y starts
//! at 0, `centre.y` is the FLOOR level of the box, and `half.y` is the wall height -- not a
//! half-extent as it is on x and z. `Object::scale` scales the box per-axis; the scale stays
//! axis-aligned, which keeps the collider rectangles orthogonal as `Collider::Collide` assumes.
//!
//! Also here, being about boxes: [`transformed_box`], a box through a placement, which is how
//! a level fences a placed model, and its inverse [`model_box`], which is how a world-space
//! opening is handed to the glTF loader to carve.

use crate::object::Object;
use crate::resources::Resources;
use crate::vector::{Matrix4, Vector3};

/// Build an invisible collision box keeping players and thrown props inside the playable area.
///
/// `centre.x`/`centre.z` are the box centre, `centre.y` is the floor level; `half.x`/`half.z`
/// are half-extents and `half.y` is the wall height (see module docs -- bounds.obj spans
/// y[0,1], not y[-1,1]).
pub fn bounds_box(res: &Resources, centre: Vector3, half: Vector3) -> Object {
    let mut obj = Object::new();
    obj.mesh = Some(res.acquire_mesh("bounds.obj"));
    // shader stays None: mesh-without-shader means "collides but never drawn" (Object.cpp:21).
    obj.pos = centre;
    obj.scale = half;
    obj
}

/// The axis-aligned box that holds the box `[lo, hi]` taken through `m`: the eight corners
/// transformed, then bounded. Exact for the translations and quarter turns the levels place
/// their models with -- a box turned a quarter is still a box -- and a safe over-estimate
/// for any other rotation.
pub fn transformed_box(m: &Matrix4, lo: Vector3, hi: Vector3) -> (Vector3, Vector3) {
    let mut out_lo = Vector3::splat(f32::MAX);
    let mut out_hi = Vector3::splat(f32::MIN);
    for corner in 0..8 {
        let p = m.mul_point(Vector3::new(
            if corner & 1 == 0 { lo.x } else { hi.x },
            if corner & 2 == 0 { lo.y } else { hi.y },
            if corner & 4 == 0 { lo.z } else { hi.z },
        ));
        out_lo = Vector3::new(out_lo.x.min(p.x), out_lo.y.min(p.y), out_lo.z.min(p.z));
        out_hi = Vector3::new(out_hi.x.max(p.x), out_hi.y.max(p.y), out_hi.z.max(p.z));
    }
    (out_lo, out_hi)
}

/// A world-space box in the model space of `placement`: what the glTF loader carves
/// (`Load::cut_boxes`) when a scene wants a world-space opening -- the elevator's
/// `wall_cut()` -- gone from a placed model. The answer is a box only because the placement
/// is a translation and a turn by a multiple of a quarter (and no tilt, no scale), which
/// every interior is placed with and which this asserts: under any other yaw the box would
/// come back as its over-wide bounds and carve wall that should have stayed.
pub fn model_box(placement: &Object, lo: Vector3, hi: Vector3) -> (Vector3, Vector3) {
    let quarter = placement.euler.y / std::f32::consts::FRAC_PI_2;
    assert!(
        (quarter - quarter.round()).abs() < 1e-4
            && placement.euler.x == 0.0
            && placement.euler.z == 0.0
            && (placement.scale * placement.p_scale - Vector3::splat(1.0)).mag() < 1e-6,
        "a box is carved from a model placed by translation and quarter turns only, not {:?}",
        placement.euler
    );
    transformed_box(&placement.world_to_local(), lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_header::GH_PI;

    #[test]
    fn a_box_follows_a_placement_and_comes_back() {
        let (lo, hi) = (Vector3::new(-1.0, 0.0, -5.0), Vector3::new(3.0, 2.0, 7.0));
        let mut obj = Object::new();
        obj.pos = Vector3::new(10.0, 1.0, 100.0);
        let (wlo, whi) = transformed_box(&obj.local_to_world(), lo, hi);
        assert!((wlo - Vector3::new(9.0, 1.0, 95.0)).mag() < 1e-5, "{wlo:?}");
        assert!((whi - Vector3::new(13.0, 3.0, 107.0)).mag() < 1e-5, "{whi:?}");
        let (mlo, mhi) = model_box(&obj, wlo, whi);
        assert!((mlo - lo).mag() < 1e-5 && (mhi - hi).mag() < 1e-5, "{mlo:?} {mhi:?}");
        // A quarter turn: local +x becomes world -z, local +z becomes world +x.
        let mut obj = Object::new();
        obj.euler.y = GH_PI / 2.0;
        let (wlo, whi) = transformed_box(&obj.local_to_world(), lo, hi);
        assert!((wlo - Vector3::new(-5.0, 0.0, -3.0)).mag() < 1e-4, "{wlo:?}");
        assert!((whi - Vector3::new(7.0, 2.0, 1.0)).mag() < 1e-4, "{whi:?}");
        let (mlo, mhi) = model_box(&obj, wlo, whi);
        assert!((mlo - lo).mag() < 1e-4 && (mhi - hi).mag() < 1e-4, "{mlo:?} {mhi:?}");
        // Three quarters and a half are quarter turns too.
        for turns in [2.0, 3.0, -1.0] {
            obj.euler.y = turns * GH_PI / 2.0;
            let (wlo, whi) = transformed_box(&obj.local_to_world(), lo, hi);
            let (mlo, mhi) = model_box(&obj, wlo, whi);
            assert!((mlo - lo).mag() < 1e-4 && (mhi - hi).mag() < 1e-4, "{turns}: {mlo:?}");
        }
    }

    #[test]
    #[should_panic(expected = "quarter turns only")]
    fn a_box_cannot_be_carved_from_a_model_turned_an_eighth() {
        let mut obj = Object::new();
        obj.euler.y = GH_PI / 4.0;
        model_box(&obj, Vector3::zero(), Vector3::splat(1.0));
    }
}
