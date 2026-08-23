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

use crate::object::Object;
use crate::resources::Resources;
use crate::vector::Vector3;

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
