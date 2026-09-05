//! EXT: a static glTF interior as a level's whole world. Not part of the C++ port.
//!
//! `ext/backrooms.rs` built this by hand for one unlit scan behind a door: the model, a
//! triangle-mesh collider, an invisible fence a metre outside it, a dark ground cap under it,
//! and a `RoomLogic` that puts a player who has fallen out of the building back at the
//! arrival point. The Pool Rooms and the Overgrown room (`level17.rs`, `level18.rs`) are the
//! same shape with no meadow in front of them -- lit PBR rooms the player is simply stood in
//! -- so the shape is a function here and the levels are a `Load`, a placement and a few
//! measured constants each. `Backrooms` stays as it is: it is placed by its door, drawn
//! through the meadow's split and tested to the centimetre, and nothing here would make it
//! smaller.
//!
//! # What the function does
//!
//! * Grades the whole scene as an interior (`view::set_scene_mood`): the hemisphere light in
//!   `Shaders/gltfpbr.frag`, a dark sky, no weather.
//! * Loads the model as a solid [`GltfProp`] at the placement given -- every part at one
//!   `Object`, the collider from the **solid** triangles only (`GltfModel::solid_triangles`),
//!   so the player wades through the pool's water and walks through the foliage cards rather
//!   than bumping into their transparent corners -- with the world-space `openings` the
//!   level asks for carved out of it as it is parsed (`Load::cut_boxes`, `bounds::model_box`):
//!   the elevator's doorway, gone from the drawn wall and from the collision alike.
//! * Fences the model's world-space extent [`FENCE_MARGIN`] out -- together with whatever
//!   `also_inside` the level stands outside the model, which is the elevator's cabin, sunk
//!   into the wall and reaching past its outer face -- caps the ground under it in the
//!   interior sky's dark (`backrooms::GroundCap`), and respawns anyone who ends up under
//!   the floor (`backrooms::fell_out`, the same rule as the Backrooms': the walls of these
//!   rooms are single quads, and a sphere that starts a step inside one is pushed out
//!   whichever side its centre is on).
//! * Stands the player at the arrival point, facing the way it says.
//!
//! # The arrival point and the elevator
//!
//! Each level places its model so that its arrival spot -- a point of open floor with
//! headroom, chosen by probing the file (a throwaway occupancy raster, not shipped; the
//! tests re-measure the numbers from the file) -- is the world origin with the floor at
//! y = 0, and the player faces -z, the engine's default heading, which keeps `--yaw 0`
//! meaning "straight ahead" as on every other level. The arrival is [`SPAWN_AHEAD`] in
//! front of a stretch of wall the level reserves for its elevator (`ELEVATOR_SPOT` /
//! `ELEVATOR_YAW` in each level): the cabin's floor point on the wall's inner face, and the
//! yaw its doorway faces. The level builds the elevator first (`ext/elevator.rs`, "How a
//! level adds one") and hands its `wall_cut()` and `world_bounds()` in here.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ext::backrooms::{fell_out, GroundCap};
use crate::ext::bounds::{bounds_box, model_box, transformed_box};
use crate::ext::gltf_model::Load;
use crate::ext::gltf_prop::GltfProp;
use crate::ext::room::{request_respawn, Respawn, RoomLogic};
use crate::ext::view;
use crate::game_header::GH_PLAYER_HEIGHT;
use crate::object::{Object, ObjectT};
use crate::player::Player;
use crate::resources::Resources;
use crate::scene::PObjectVec;
use crate::vector::Vector3;

/// Clearance the invisible fence keeps around the model, as the Backrooms keeps it: enough
/// to stand in an outer doorway without the fence showing through it, not enough to walk
/// away from the building.
pub const FENCE_MARGIN: f32 = 1.0;

/// How far in front of the elevator's doorway the player arrives: a stride out of the cabin.
pub const SPAWN_AHEAD: f32 = 1.5;

/// The horizontal direction an `Object` with yaw `yaw` faces: its local +z in the world
/// (`Matrix4::rot_y` takes local +z to `(sin a, 0, cos a)`), the inverse of
/// `door::yaw_facing`.
pub fn facing(yaw: f32) -> Vector3 {
    Vector3::new(yaw.sin(), 0.0, yaw.cos())
}

/// Where a player arrives from an elevator whose cabin floor point is `spot` and whose
/// doorway faces `yaw`: [`SPAWN_AHEAD`] out of the doorway, at eye height, looking the way
/// the doorway does. The levels place their models so this is the origin facing -z (module
/// docs); the function rather than the constant, so the level's spawn and its reserved
/// elevator cannot disagree.
pub fn arrival(spot: Vector3, yaw: f32) -> Respawn {
    let dir = facing(yaw);
    Respawn::facing(spot + dir * SPAWN_AHEAD + Vector3::new(0.0, GH_PLAYER_HEIGHT, 0.0), dir)
}

/// World-space axis-aligned bounds of a part's fitted bounds `b` (`[minx, maxx, miny, maxy,
/// minz, maxz]`) placed by `obj` (`bounds::transformed_box`).
pub fn world_bounds(b: [f32; 6], obj: &Object) -> (Vector3, Vector3) {
    transformed_box(
        &obj.local_to_world(),
        Vector3::new(b[0], b[2], b[4]),
        Vector3::new(b[1], b[3], b[5]),
    )
}

/// The invisible fence round the world-space box `(lo, hi)`, [`FENCE_MARGIN`] out, and the
/// rule that puts whoever ends up under the floor at `floor_y` back at `arrival`. Shared by
/// the Backrooms (`level16.rs`), whose model and door take their own path, and by [`load`].
pub fn fence_and_respawn(
    res: &Resources,
    objs: &mut PObjectVec,
    (lo, hi): (Vector3, Vector3),
    floor_y: f32,
    arrival: Respawn,
) {
    // bounds_box takes the floor level and the wall height on y (not a half-extent), so the
    // floor is dropped a margin below the model and the walls rise a margin above it.
    objs.push(Rc::new(RefCell::new(bounds_box(
        res,
        Vector3::new(0.5 * (lo.x + hi.x), lo.y - FENCE_MARGIN, 0.5 * (lo.z + hi.z)),
        Vector3::new(
            0.5 * (hi.x - lo.x) + FENCE_MARGIN,
            hi.y - lo.y + 2.0 * FENCE_MARGIN,
            0.5 * (hi.z - lo.z) + FENCE_MARGIN,
        ),
    ))) as Rc<RefCell<dyn ObjectT>>);

    // Under the floor there is nothing, on purpose (see `ext/backrooms.rs`): whoever ends
    // up there is put back at the arrival point. The rule is checked every step against the
    // fenced footprint and the floor level.
    let footprint = (
        Vector3::new(lo.x - FENCE_MARGIN, lo.y, lo.z - FENCE_MARGIN),
        Vector3::new(hi.x + FENCE_MARGIN, hi.y, hi.z + FENCE_MARGIN),
    );
    objs.push(Rc::new(RefCell::new(RoomLogic::new(move |ctx| {
        if fell_out(ctx.player_pos, ctx.player_p_scale, floor_y, footprint) {
            request_respawn(arrival);
        }
    }))) as Rc<RefCell<dyn ObjectT>>);
}

/// The smallest box holding both.
pub fn union((alo, ahi): (Vector3, Vector3), (blo, bhi): (Vector3, Vector3)) -> (Vector3, Vector3) {
    (
        Vector3::new(alo.x.min(blo.x), alo.y.min(blo.y), alo.z.min(blo.z)),
        Vector3::new(ahi.x.max(bhi.x), ahi.y.max(bhi.y), ahi.z.max(bhi.z)),
    )
}

/// The two heights [`load`] needs, which are only the same height by luck.
///
/// `floor_y` used to be one number doing both jobs, and for a room with a flat floor it can be:
/// the dark cap goes `backrooms::CAP_DROP` under it and `backrooms::fell_out` calls anyone half
/// a metre below it fallen out. The two pull apart the moment a level's ground is not flat.
///
/// The Liminal Neighborhood's is not: its road lies 0.81 m below its lawns, so the fall line has
/// to sit just under the road (any higher and walking the road is a fall; any lower and the
/// empty pool in the back garden stops being one). That puts the cap 5 cm under the tarmac --
/// and 5 cm is under the depth buffer's resolution at 60 m once the engine collapses the near
/// plane to a centimetre, which it does whenever the player is up against a portal
/// (`Engine::render`, `nearest_portal_dist`). The near-black cap then punches through the road
/// in bands, and it does it *only* near a cut, so the road appears to break up as you approach
/// one and heal as you step through. Which is the one thing a seam cannot do.
#[derive(Clone, Copy, Debug)]
pub struct Floor {
    /// Where the level's ground is: what a fall is measured from.
    pub walk: f32,
    /// What the dark cap is hung under. Anything comfortably below every surface the player
    /// can see; it only has to be under the world, not near it.
    pub cap: f32,
}

impl Floor {
    /// Both jobs at one height: a room with a flat floor and no portal to stand against.
    pub fn at(y: f32) -> Floor {
        Floor { walk: y, cap: y }
    }
}

/// What a level has built outside its model before the model is loaded: the elevator.
pub struct Openings<'a> {
    /// World-space boxes carved out of the model, drawn and collided alike: the cabin's
    /// `wall_cut()`.
    pub cut: &'a [(Vector3, Vector3)],
    /// World-space boxes the fence must also enclose: the cabin's `world_bounds()`.
    pub also_inside: &'a [(Vector3, Vector3)],
}

/// Build the interior (see the module docs): `spec` placed by `placement` with
/// `openings.cut` carved out of it, its floor at world `floor_y` where the player arrives,
/// the player stood at `arrival`, the fence round the model and `openings.also_inside`.
/// Returns the world-space bounds the fence and the respawn rule were built from.
pub fn load(
    gl: &Rc<glow::Context>,
    res: &Resources,
    objs: &mut PObjectVec,
    player: &mut Player,
    spec: &Load,
    placement: &Object,
    openings: Openings,
    floor: Floor,
    arrival: Respawn,
) -> (Vector3, Vector3) {
    // An interior from the first step: every eye grades as one (no meadow split to cross).
    view::set_scene_mood(view::MOOD_INTERIOR);
    let bounds = build(gl, res, objs, spec, placement, openings, floor, arrival, Vector3::zero());
    player.base.set_position(arrival.pos);
    player.set_look(arrival.yaw, 0.0);
    bounds
}

/// The interior's objects alone -- the cap, the model, the fence and the respawn rule --
/// with the whole of it stood `shift` from where `placement` and `openings` say, and no word
/// about the player or the scene's mood. What [`load`] builds, and what a level that wants a
/// second copy of another level's room somewhere else in its world (the Backrooms' window,
/// `ext/window.rs`) builds beside its own.
///
/// The cuts are taken in the model's space against the UNSHIFTED placement, and only then is
/// everything moved: `GltfModel::acquire` caches by the whole `Load`, cut boxes included, so
/// a copy cut from the same openings at the same placement is the same model as the level's
/// own -- one parse, shared -- where the same boxes taken a couple of thousand units out and
/// brought back would differ in their last bits and load the file again.
pub fn build(
    gl: &Rc<glow::Context>,
    res: &Resources,
    objs: &mut PObjectVec,
    spec: &Load,
    placement: &Object,
    openings: Openings,
    floor: Floor,
    respawn: Respawn,
    shift: Vector3,
) -> (Vector3, Vector3) {
    // The cuts, in the model's space; the loader carves them from what it draws and what
    // the prop's collider is then built from.
    let cut: Vec<(Vector3, Vector3)> =
        openings.cut.iter().map(|&(lo, hi)| model_box(placement, lo, hi)).collect();
    let spec = Load { cut_boxes: &cut, ..*spec };
    let mut placed = Object::new();
    placed.pos = placement.pos + shift;
    placed.euler = placement.euler;
    placed.scale = placement.scale;
    let prop = GltfProp::new(gl, res, &spec, placed.pos, placed.euler.y, &[], true);
    let mut bounds = (Vector3::splat(f32::MAX), Vector3::splat(f32::MIN));
    for part in spec.parts {
        bounds = union(bounds, world_bounds(prop.model().bounds(part.name), &placed));
    }
    for &(lo, hi) in openings.also_inside {
        bounds = union(bounds, (lo + shift, hi + shift));
    }
    let (lo, hi) = bounds;

    // The cap first, then the model: a translucent surface blends over what was drawn
    // before it, and the cap is the dark the water would otherwise show the sky through
    // where a wall lets the outside in.
    objs.push(Rc::new(RefCell::new(GroundCap::under(res, (lo, hi), floor.cap)))
        as Rc<RefCell<dyn ObjectT>>);
    objs.push(Rc::new(RefCell::new(prop)) as Rc<RefCell<dyn ObjectT>>);

    fence_and_respawn(res, objs, (lo, hi), floor.walk, respawn);
    (lo, hi)
}

/// Measuring a placed model without a GL context, for the levels' tests: the file's
/// triangles through the placement, and rays cast against them.
#[cfg(test)]
pub mod probe {
    use super::*;
    use crate::ext::gltf_model::GltfModel;

    /// A part's solid triangles (what the collider takes), in world space under `placement`.
    pub fn world_triangles(spec: &Load, part: &str, placement: &Object) -> Vec<[Vector3; 3]> {
        placed(GltfModel::probe_solid_triangles(spec, part), placement)
    }

    /// Every triangle of the part, water and foliage included, in world space.
    pub fn world_triangles_all(spec: &Load, part: &str, placement: &Object) -> Vec<[Vector3; 3]> {
        placed(GltfModel::probe_triangles(spec, part), placement)
    }

    fn placed((pos, idx): (Vec<[f32; 3]>, Vec<u32>), placement: &Object) -> Vec<[Vector3; 3]> {
        let m = placement.local_to_world();
        let v = |i: u32| {
            let p = pos[i as usize];
            m.mul_point(Vector3::new(p[0], p[1], p[2]))
        };
        idx.chunks_exact(3).map(|t| [v(t[0]), v(t[1]), v(t[2])]).collect()
    }

    /// Distance along the unit ray `(origin, dir)` to the nearest triangle it crosses, either
    /// face (a single-sided wall seen from behind still counts as a wall), or `None`.
    pub fn nearest_hit(tris: &[[Vector3; 3]], origin: Vector3, dir: Vector3) -> Option<f32> {
        let mut best: Option<f32> = None;
        for [a, b, c] in tris {
            // Moller-Trumbore.
            let (e1, e2) = (*b - *a, *c - *a);
            let p = dir.cross(e2);
            let det = e1.dot(p);
            if det.abs() < 1e-9 {
                continue;
            }
            let inv = 1.0 / det;
            let s = origin - *a;
            let u = s.dot(p) * inv;
            if !(0.0..=1.0).contains(&u) {
                continue;
            }
            let q = s.cross(e1);
            let v = dir.dot(q) * inv;
            if v < 0.0 || u + v > 1.0 {
                continue;
            }
            let t = e2.dot(q) * inv;
            if t > 1e-4 && best.is_none_or(|b| t < b) {
                best = Some(t);
            }
        }
        best
    }

    /// The floor under `p`: the first surface a ray straight down from it meets, as a world
    /// height.
    pub fn floor_under(tris: &[[Vector3; 3]], p: Vector3) -> Option<f32> {
        nearest_hit(tris, p, Vector3::new(0.0, -1.0, 0.0)).map(|d| p.y - d)
    }

    /// The ceiling over `p`, as a world height.
    pub fn ceiling_over(tris: &[[Vector3; 3]], p: Vector3) -> Option<f32> {
        nearest_hit(tris, p, Vector3::new(0.0, 1.0, 0.0)).map(|d| p.y + d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_header::GH_PI;

    #[test]
    fn facing_inverts_yaw_facing() {
        use crate::ext::door::yaw_facing;
        for dir in [
            Vector3::new(0.0, 0.0, -1.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(-0.6, 0.0, 0.8),
        ] {
            assert!((facing(yaw_facing(dir)) - dir).mag() < 1e-6, "{dir:?}");
        }
        assert!((facing(GH_PI) - Vector3::new(0.0, 0.0, -1.0)).mag() < 1e-6);
    }

    /// A doorway on the wall at z = SPAWN_AHEAD facing -z puts the arrival at eye height
    /// over the origin, looking -z: the engine's default heading.
    #[test]
    fn arrival_is_a_stride_out_of_the_doorway_facing_its_way() {
        let a = arrival(Vector3::new(0.0, 0.0, SPAWN_AHEAD), GH_PI);
        assert!((a.pos - Vector3::new(0.0, GH_PLAYER_HEIGHT, 0.0)).mag() < 1e-6, "{:?}", a.pos);
        assert!(a.yaw.abs() < 1e-6, "yaw 0 is -z: {}", a.yaw);
        // Turned a quarter: the stride goes along the doorway's facing.
        let b = arrival(Vector3::new(2.0, 0.0, 3.0), GH_PI / 2.0);
        assert!((b.pos - Vector3::new(2.0 + SPAWN_AHEAD, GH_PLAYER_HEIGHT, 3.0)).mag() < 1e-5);
        let forward = Vector3::new(-b.yaw.sin(), 0.0, -b.yaw.cos());
        assert!((forward - Vector3::new(1.0, 0.0, 0.0)).mag() < 1e-5);
    }

    /// The loader's `[minx, maxx, miny, maxy, minz, maxz]` read as a box, through a
    /// placement (the placement itself is `bounds::transformed_box`'s business).
    #[test]
    fn world_bounds_read_the_loaders_six_numbers() {
        let b = [-1.0, 3.0, 0.0, 2.0, -5.0, 7.0];
        let mut obj = Object::new();
        obj.pos = Vector3::new(10.0, 1.0, 100.0);
        let (lo, hi) = world_bounds(b, &obj);
        assert!((lo - Vector3::new(9.0, 1.0, 95.0)).mag() < 1e-5, "{lo:?}");
        assert!((hi - Vector3::new(13.0, 3.0, 107.0)).mag() < 1e-5, "{hi:?}");
        let u = union((lo, hi), (Vector3::new(20.0, -1.0, 0.0), Vector3::new(21.0, 0.0, 1.0)));
        assert!((u.0 - Vector3::new(9.0, -1.0, 0.0)).mag() < 1e-5, "{:?}", u.0);
        assert!((u.1 - Vector3::new(21.0, 3.0, 107.0)).mag() < 1e-5, "{:?}", u.1);
    }

    #[test]
    fn rays_find_the_nearest_face_from_either_side() {
        // A floor quad at y = 0.5 and a wall at z = -2.
        let tris = [
            [
                Vector3::new(-1.0, 0.5, -1.0),
                Vector3::new(1.0, 0.5, -1.0),
                Vector3::new(1.0, 0.5, 1.0),
            ],
            [
                Vector3::new(-1.0, 0.5, -1.0),
                Vector3::new(1.0, 0.5, 1.0),
                Vector3::new(-1.0, 0.5, 1.0),
            ],
            [
                Vector3::new(-1.0, 0.0, -2.0),
                Vector3::new(1.0, 0.0, -2.0),
                Vector3::new(1.0, 3.0, -2.0),
            ],
            [
                Vector3::new(-1.0, 0.0, -2.0),
                Vector3::new(1.0, 3.0, -2.0),
                Vector3::new(-1.0, 3.0, -2.0),
            ],
        ];
        let eye = Vector3::new(0.0, 1.5, 0.0);
        assert!((probe::floor_under(&tris, eye).unwrap() - 0.5).abs() < 1e-6);
        assert!(probe::ceiling_over(&tris, eye).is_none());
        // From below, the same floor is a ceiling.
        let under = Vector3::new(0.0, -1.0, 0.0);
        assert!((probe::ceiling_over(&tris, under).unwrap() - 0.5).abs() < 1e-6);
        let d = probe::nearest_hit(&tris, eye, Vector3::new(0.0, 0.0, -1.0)).unwrap();
        assert!((d - 2.0).abs() < 1e-6);
        assert!(probe::nearest_hit(&tris, eye, Vector3::new(0.0, 0.0, 1.0)).is_none());
        // Past the quad's edge: nothing.
        assert!(probe::floor_under(&tris, Vector3::new(5.0, 1.5, 0.0)).is_none());
    }
}
