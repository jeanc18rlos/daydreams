//! EXT: Scene "Liminal Neighborhood". Not part of the C++ port.
//!
//! Elbolillo's "Abandoned_House" (`Meshes/abandoned_house.glb`, CC-BY-4.0; licence beside it,
//! credit in `THIRD_PARTY.md`): one derelict brick house, modelled room by room down to the
//! cobwebs, standing on a lot in half a kilometre of suburb -- two rows of identical shells
//! either side of one straight road, on lawns gone to seed, under power lines. Loaded through
//! `ext/interior.rs`, which is the first thing here that is not what it says: this is an
//! exterior. What it takes from that module is the shape of a level that is one glTF file --
//! a solid prop, a fence, a dark cap under it and a rule for whoever falls off the world --
//! and the interior grade (`view::MOOD_INTERIOR`) is what a night exterior wants anyway: no
//! weather, the hemisphere in `Shaders/gltfpbr.frag`, and a sky that is not quite black --
//! `Shaders/sky.frag` grades that mood from vec3(0.030, 0.024, 0.016) at the horizon to a third
//! of it overhead, "near-black with the faintest warm cast". Near-black is the whole palette
//! here: nothing is lit but the sky, and the sky is out.
//!
//! An **isolated** level: no elevator, no door, nothing to carry in or out. It is reached from
//! SWITCH LEVEL in the pause menu and left the same way, and everything in it is the file, the
//! four objects `interior::load` builds around it, a moon (`ext/moon.rs`) -- the one thing in
//! the sky, keeping station on the eye so that it never parallaxes and never reaches the far
//! plane -- and the two portals that make the street endless.
//!
//! Numbering note: level19-24 stay reserved for the shelved campaign sheets (GDD 7.6),
//! level25-29 for the shelved design pack; pivot arenas start at 30 (`level30.rs`).
//!
//! # What the file holds, measured (model metres, Y up)
//!
//! Bounds x[-276.5, 249.0], y[-4.34, 10.62], z[-164.4, 40.1], but the far ends of that box are
//! the power lines and a radio mast, not ground: the ground is a rectangle about x[-270, 230],
//! z[-162, -38] -- 500 x 124 m -- and outside it there is nothing to stand on. The one road
//! runs east-west across it at z[-105, -93], its tarmac at **y = -1.29**, which is the lowest
//! surface in the file a player can stand on and therefore this level's floor; the lawns
//! either side of it sit 0.81 m above that at -0.48, and the houses' own floors 0.29 m above
//! those again at -0.19. The one exception is the empty swimming pool in the abandoned house's
//! back garden (x[-32, -22], z[-146, -142]), whose deep end is 2.68 down: further under the
//! floor than the respawn rule allows (`backrooms::FALL_DEPTH`), so sliding into it wakes you
//! up where you came in, which is the closest thing this level has to a hazard.
//!
//! The abandoned house is the lot at x[-58, -22], z[-140, -117]: a west wing whose facade
//! stands at z = -118.05, an east wing behind a porch at -117.92, and between them a recessed
//! centre 10.75 m wide whose brick wall is a single plane at **z = -121.78** from x = -46.25
//! to -35.5, pierced by windows at head height and eaved at 3.41. The lawn in front of that
//! wall is flat at -0.48 for the four metres out to where the ground starts falling toward the
//! road. That recess is where the player starts.
//!
//! Everything green -- `Planta5`, `Arbol1` -- and every cobweb (`Telaraa`) is alpha-tested
//! cards and does not collide (`GltfModel::solid_triangles`); so is the window glass
//! (`Vidrio.001`, `BLEND` at alpha 0.06, which the loader's alpha policy tests at 0.5 and so
//! discards), which is why the windows are holes and the parked van's are empty frames. The
//! file needs no `metallic_override`: its four metals carry a real `metallicFactor` and a
//! metallic-roughness map, and everything else is written 0.
//!
//! # Placement
//!
//! World y = 0 is the tarmac, not the spot the player stands on -- the one deviation from
//! `level17.rs` and `level18.rs`, and the reason for it is `interior::load`'s two uses of
//! `floor_y`: the dark ground cap goes just under it and the respawn rule fires half a metre
//! below it. Put the spawn's own lawn at zero and the road, 0.81 m lower, is a fall; put the
//! road at zero and the lawn is simply a step up, the cap hides under the tarmac, and the only
//! thing left below the line is the pool. So the spawn stands at [`LAWN_Y`], and the model is
//! turned a half turn (model +z, out of the house, becomes world -z, the engine's default
//! heading, which keeps `--yaw 0` meaning "straight ahead" here as everywhere else) and shifted
//! so the spot the player stands on is the origin in x and z.
//!
//! The spawn is two metres out from the recess's brick wall, in the middle of its ten: the
//! house is at your back and the first frame is a dead suburban street.
//!
//! # The street loops
//!
//! Two portals stand across the street near its two ends, at [`LOOP_WEST_X`] and
//! [`LOOP_EAST_X`], connected. Both face +x and neither is turned, tilted or resized against
//! the other, so `Physical::try_portal` applies a pure translation of [`LOOP_LENGTH`] along the
//! street and nothing else: walk east off the end of it and you arrive at the other end still
//! walking east, at the same speed, the same height, the same distance from the kerb and the
//! same size. Keep walking and the street never ends. Walk the other way and the same thing
//! happens in reverse. That is the engine's oldest trick (`level1.rs` has had it since the
//! port) pointed at the one subject it was made for.
//!
//! **Where the cuts go.** The loop has to close where its two ends match, or the join shows.
//! The neighbourhood's lots repeat on a 50 m pitch, so a plane and its partner a whole number
//! of lots away are nearly the same plane -- but only nearly, because the abandoned house's lot
//! is unique and its neighbours are not identical either. The ground along the road runs
//! x[-264, 233], and sweeping the candidates a metre at a time -- comparing ground height and
//! skyline every half metre down the whole 130 m of walkable depth -- the longest clean pair
//! that fits is **-230 and +220**: 3 cm of worst mismatch, nothing of the model within half a
//! metre of either plane, and only 34 m of street left over past the west cut and 13 m past the
//! east one. Nine lots of it, 450 m, with the abandoned house and the spawn near the middle.
//! `the_cuts_span_the_ground_and_the_skyline_at_their_planes` re-measures both planes.
//!
//! **How big the portals are.** A portal only hides what its quad covers, so a cut that spanned
//! the road alone would let the unlooped street show over it and either side, and a player who
//! stepped onto the lawn would simply walk around the end of the world. These span the whole
//! cross-section: 132 m across, from under the tarmac to 20 m up, which clears the ground
//! (z[-81.8, 42.2]) and the skyline with metres to spare. The radio mast 160 m off the street is
//! outside them and stays outside them: it is past the far plane, there is no ground under it,
//! and anyone who went looking would fall out of the world and wake up at the spawn first.
//!
//! **What makes the join invisible.** Three things, and none of them is the geometry -- that
//! part was the easy half. A portal pass normally shades cheaply: `Shaders/gltfpbr.frag` drops
//! normal mapping and the whole specular lobe when `view::detail` says the pass is a portal's,
//! which is the right trade for a doorway-sized quad paid for four times over and the wrong one
//! for a seam that fills the screen, where it means the brick and the kerbs change the instant
//! you step across. `view::set_seamless_portals` turns it off for this scene. A crossing is
//! also normally audible in a level that declares its ground (`audio::portal_audible`), on the
//! reasoning that such a portal is a thing you can see and mean to walk through; this one is
//! not, so `audio::set_portal_audible(false)` overrides it and the cuts are silent.
//!
//! And the ground cap, which was the loud one. `interior::Floor` splits what used to be a
//! single `floor_y`, because the two jobs it did pull apart here: the fall line has to sit just
//! under the road, which put the near-black cap 5 cm under the tarmac -- finer than the depth
//! buffer resolves at 60 m once the engine collapses the near plane to a centimetre, which it
//! does whenever the player is up against a portal. The cap punched through the road in bands,
//! and only near a cut, so the street appeared to break up as you walked into one and heal as
//! you stepped through: the seam, drawn in by the one object in the scene that is never meant
//! to be seen. [`CAP_Y`] hangs it two metres down instead, which is clear at any precision this
//! level can reach and has the side benefit of putting it under the empty pool, which used to
//! read as a black plate rather than a hole.
//!
//! # The fog
//!
//! The street runs past the engine's far plane (`GH_FAR`, 100 units) in every direction, and the
//! engine's own haze is not built to hide that: `1 - exp(-dist * 0.008)` capped at 90% of the
//! way to the weather's tone is half applied at 100 units, so the cut showed as a hard edge and
//! geometry crossing it appeared into a scene that still had most of its colour. `view::Fog`
//! lets a scene say otherwise, and this one says **black, dense, and squared**:
//!
//! * *Black* -- but the engine's black, not zero. Silent Hill's fog was white because Silent
//!   Hill had a daylit sky; over a night suburb the same trick is the dark closing in. The
//!   temptation is to fade to vec3(0), and it is wrong: `Shaders/sky.frag` fades this mood to
//!   vec3(0.030, 0.024, 0.016) at the horizon, so a world converging on zero dissolves into
//!   something DARKER than what is behind it. Measured at the horizon that was a step from
//!   8/255 to 3/255 across the whole frame, with distant rooflines as black cut-outs against a
//!   lighter sky -- the far-plane pop inverted rather than removed. The fog takes the sky's own
//!   horizon tone instead, which is `backrooms::CAP_COLOR`, pinned to it byte for byte for
//!   exactly this reason, and a test pins the fog to that in turn.
//! * *Squared* -- `1 - exp(-t*t)`, the falloff `Shaders/gltfunlit.frag` has always used. A plain
//!   exponential thick enough to close at 100 m is already a third of the way in at 20; the
//!   square stays out of the way close up and then shuts hard, which is what hides a draw
//!   distance rather than merely dimming everything in front of it.
//! * *All the way* -- `cap` 1 rather than the engine's 0.9. The 0.9 leaves every surface a tenth
//!   of its own colour at any distance, and that tenth is exactly what would still be there to
//!   pop at the far plane, however dense the fog got.
//!
//! At [`FOG_DISTANCE`] = 45 that is 99% at the far plane and 98% ten metres short of it -- so
//! nothing is left to pop -- against 5% at 10 m and 18% at 20, which leaves the house, the van
//! and the near street their colour. `the_fog_is_shut_before_the_far_plane_and_open_near_the_player`
//! computes both ends with the shader's own formula.
//!
//! Two things follow the fog. The **ground cap** takes its colour rather than its own
//! (`backrooms::GroundCap::under`), so that a scene which fades its distance somewhere else
//! keeps the cap == sky invariant its comment is about; with the colour above the two are the
//! same value and the cap is exactly what it always was. It is read once, at construction, not
//! at draw time -- `ext/moon.rs` clears the fog for the length of its own draw, and reading it
//! later would hand the cap whatever the object before it left behind. The **moon** is that
//! exception: it hangs 80 units out, which is precisely where this fog closes, and it is not in
//! the air to be fogged. It opts out of the scene's fog and keeps the engine's thin haze, which
//! is what makes it a pale disc rather than a white hole.
//!
//! One limit, recorded rather than fixed: `Shaders/gltfunlit.frag` has no such override, so a
//! scene fog does not reach an unlit glTF material. Nothing here has one -- `abandoned_house.glb`
//! is PBR throughout -- but the first unlit asset added to this level would fade on that
//! shader's own curve instead, with no diagnostic.
//!
//! **What the distance buys.** A full-screen portal is a whole extra pass, and a chain of them
//! is a whole extra pass each: with the cuts a block apart the frame went from 2.4 ms to 9,
//! because from anywhere on the street you were looking down a stack of four of them. At 450 m
//! neither cut is inside the far plane (`GH_FAR` = 100) from anywhere near the middle, so the
//! engine's frustum pre-test drops both and the level costs what it did before the portals
//! existed. Walk up to one and it starts drawing -- one pass, not four, because the *other*
//! cut is 450 m beyond it and clipped away. The loop pays for itself only where you can see it.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ext::gltf_model::{Anchor, Fit, Frame, Load, PartSpec};
use crate::ext::interior::{self, Openings};
use crate::ext::moon::Moon;
use crate::ext::room::{self, Respawn, RoomLogic};
use crate::ext::view::Fog;
use crate::game_header::{GH_PI, GH_PLAYER_HEIGHT};
use crate::object::{Object, ObjectT, UpdateCtx};
use crate::player::Player;
use crate::portal::{connect, Portal};
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec, Scene};
use crate::vector::Vector3;

pub struct Level31;

const MODEL: &str = "Meshes/abandoned_house.glb";
/// The whole file, from the scene root (its centimetres-to-metres scale applied).
const PART: &str = "all";
/// The file's largest maps are 1024 square: nothing is resampled.
const MAP: u32 = 1024;

const PARTS: [PartSpec<'static>; 1] =
    [PartSpec { name: PART, roots: &[], skip: &[], frame: Frame::Scene, anchor: Anchor::Hinge }];

/// EXT: "All Hallows" (level32.rs) borrows this whole stage -- the spec, the placement,
/// the fog and the spawn are pub(crate) for it, and for nothing else.
pub(crate) fn load_spec() -> Load<'static> {
    Load {
        path: MODEL,
        parts: &PARTS,
        fit: Fit::Identity,
        max_map: MAP,
        translucent: &[],
        metallic_override: &[],
        cut_boxes: &[],
    }
}

/// The tarmac, in the model: the lowest ground a player can stand on, and so what world y = 0
/// is put at (module docs).
const ROAD_MODEL_Y: f32 = -1.29;
/// The lawn in front of the recess, in the model.
const LAWN_MODEL_Y: f32 = -0.48;
/// How high that lawn -- and so the spot the player stands on -- is over this level's floor.
pub const LAWN_Y: f32 = LAWN_MODEL_Y - ROAD_MODEL_Y;

/// The recessed centre of the abandoned house's facade: one brick plane from x = -46.25 to
/// -35.5 (module docs). This is its outer face.
const WALL_Z: f32 = -121.78;
/// Where the player stands, in the model: the middle of that recess, two metres out from the
/// brick on flat lawn, looking model +z -- out of the house, down the garden, at the street.
const SPAWN_MODEL: Vector3 = Vector3 { x: -42.5, y: LAWN_MODEL_Y, z: WALL_Z + 2.0 };
/// The way the player faces once the model is turned -- model +z, out of the house -- which
/// is -z, the engine's default heading.
const FACING_WORLD: Vector3 = Vector3 { x: 0.0, y: 0.0, z: -1.0 };
/// The turn that takes model +z to world -z (`Matrix4::rot_y`).
const MODEL_YAW: f32 = GH_PI;

/// Where [`SPAWN_MODEL`] lands: over the origin, on the lawn (module docs).
const SPAWN_SPOT: Vector3 = Vector3 { x: 0.0, y: LAWN_Y, z: 0.0 };

/// This level's ground: the tarmac (module docs). What `backrooms::fell_out` measures a fall
/// against -- the road is on it, and the empty pool 1.4 m under it is a fall.
pub(crate) const FLOOR_Y: f32 = 0.0;
/// What the dark ground cap hangs under, which is NOT [`FLOOR_Y`] (`interior::Floor`).
///
/// Five centimetres under the tarmac -- where a shared floor height would put it -- is under
/// the depth buffer's resolution at 60 m once the engine collapses the near plane to a
/// centimetre, which it does whenever the player is up against a portal. The near-black cap
/// then punches through the road in bands, and only near a cut, so the road breaks up as you
/// walk to one and heals as you step through: the seam, drawn in, by the one object in the
/// scene that is never meant to be seen. Two metres down is clear of it at any precision the
/// level can reach, and still under every surface in the file -- the pool's floor, the lowest
/// of them, is at -1.39.
pub(crate) const CAP_Y: f32 = -2.0;

/// The distance at which the fog's exponent reaches 1, which with a squareness of 1 puts the
/// fade at 63%. Chosen from the far plane backwards (module docs, "The fog").
const FOG_DISTANCE: f32 = 45.0;

/// The dark this level fades into: black, dense, and squared, so that the street is gone before
/// the far plane can cut it (module docs, "The fog").
pub(crate) const FOG: Fog = Fog {
    // The dark -- but the engine's dark, not zero. `Shaders/sky.frag`'s interior branch is not
    // black: it is a gradient from vec3(0.030, 0.024, 0.016) at the horizon to a third of that
    // overhead, "near-black with the faintest warm cast". Converge the fog on 0 instead and the
    // world dissolves into something DARKER than the sky behind it -- measured at the horizon,
    // a step from 8/255 to 3/255 running the full width of the frame, with distant rooflines
    // reading as black cut-outs against a lighter sky. That is the far-plane pop inverted, not
    // removed. This is the horizon's own tone, which is also `backrooms::CAP_COLOR`, pinned to
    // it byte for byte for exactly this reason; the world now fades into the sky and stops.
    color: [0.030, 0.024, 0.016],
    density: 1.0 / FOG_DISTANCE,
    // The Silent Hill curve: `1 - exp(-t*t)` stays out of the way close up and then closes
    // hard, which is what hides a draw distance instead of merely dimming everything in front
    // of it.
    squareness: 1.0,
    // All the way to black. The engine's 0.9 deliberately leaves the far end some of its own
    // colour, which is exactly the 10% that would still be there to pop at the far plane.
    cap: 1.0,
};

// ── The loop ──────────────────────────────────────────────────────────────────────────────
//
// Two portals across the street, near its two ends and connected: walk east off the end of it
// and you arrive at the other end still walking east, forever. See "The street loops" above.
/// World x of the west cut, and how far east of it the other one is: as near the two ends of
/// the street as a clean cut can be put (module docs, "The street loops").
pub(crate) const LOOP_WEST_X: f32 = -230.0;
pub(crate) const LOOP_LENGTH: f32 = 450.0;
pub(crate) const LOOP_EAST_X: f32 = LOOP_WEST_X + LOOP_LENGTH;
/// The cuts run across the street, so their plane is x = const and their normal is +x: a
/// quarter turn takes a portal's local +z (its `Object::forward`) there.
const LOOP_YAW: f32 = GH_PI / 2.0;
/// Middle of the strip either portal has to cover, and its half-extents: the walkable ground
/// is z[-81.8, 42.2] and nothing on it reaches 20 m, so this spans the world at that plane
/// with metres to spare on every side. A portal has to cover the whole cross-section or the
/// unlooped world shows past its edge.
const LOOP_CENTRE_Z: f32 = -20.0;
const LOOP_HALF_Z: f32 = 66.0;
const LOOP_CENTRE_Y: f32 = 7.0;
const LOOP_HALF_Y: f32 = 13.0;

/// Where the player starts, and where the pool puts them back: stood on the lawn at
/// [`SPAWN_SPOT`], looking -z at the street.
pub(crate) fn spawn() -> Respawn {
    Respawn::facing(SPAWN_SPOT + Vector3::new(0.0, GH_PLAYER_HEIGHT, 0.0), FACING_WORLD)
}

/// Put a player found outside the block back inside it, by exactly one [`LOOP_LENGTH`].
///
/// A net, not a mechanism: `Physical::try_portal` lands a crossing 2.4 cm inside the far cut
/// (its bump is `2 * GH_NEAR_MIN`), and 2.4 cm is not much margin to bet a level on. Outside
/// is not a harmless place to be, either -- it is the 34 m of street left over past the west
/// cut and the 13 m past the east one, and from there the cut fills the view and shows the
/// *other* tail, so the street reads as thirteen metres of tarmac and then nothing. Anyone who
/// gets there is put back where the portal would have put them, keeping their heading, which
/// is the same transform the portal applies and therefore invisible if it ever fires.
pub(crate) fn wrap_into_the_block(ctx: &UpdateCtx) {
    let x = ctx.player_pos.x;
    let shift = if x < LOOP_WEST_X {
        LOOP_LENGTH
    } else if x > LOOP_EAST_X {
        -LOOP_LENGTH
    } else {
        return;
    };
    // The heading the camera has now: `Respawn::facing` wants a direction, and the point of
    // the net is that nothing about the player changes but their x.
    let forward = ctx.cam_to_world.mul_direction(Vector3::new(0.0, 0.0, -1.0));
    let flat = Vector3::new(forward.x, 0.0, forward.z);
    let pos = ctx.player_pos + Vector3::new(shift, 0.0, 0.0);
    room::request_respawn(Respawn::facing(pos, flat));
}

/// One of the two cuts across the street, at world x `x`: a plane spanning the whole
/// cross-section of the walkable world, facing +x.
fn place_cut(p: &mut Portal, x: f32) {
    p.base.pos = Vector3::new(x, LOOP_CENTRE_Y, LOOP_CENTRE_Z);
    p.base.euler.y = LOOP_YAW;
    // `Portal::intersects` and `dist_to` read the scale as half-extents of the quad's own x
    // and y axes; the quarter turn puts local x along world z.
    p.base.scale = Vector3::new(LOOP_HALF_Z, LOOP_HALF_Y, 1.0);
}

pub(crate) fn cut(res: &Resources, x: f32) -> Portal {
    let mut p = Portal::new(res);
    place_cut(&mut p, x);
    p
}

/// Where the model stands: turned by [`MODEL_YAW`] and shifted so [`SPAWN_MODEL`] is
/// [`SPAWN_SPOT`], which puts the tarmac at world y = 0.
pub(crate) fn placement() -> Object {
    let mut obj = Object::new();
    obj.euler.y = MODEL_YAW;
    obj.pos = SPAWN_SPOT - obj.local_to_world().mul_direction(SPAWN_MODEL);
    obj
}

impl Scene for Level31 {
    fn load(
        &self,
        gl: &Rc<glow::Context>,
        res: &Resources,
        objs: &mut PObjectVec,
        portals: &mut PPortalVec,
        player: &mut Player,
    ) {
        // EXT: what the footsteps land on (src/ext/audio.rs). One surface for the whole level:
        // the lawns are what it is mostly walked on, and the game has no tarmac set.
        crate::ext::audio::set_surface(crate::ext::audio::Surface::Grass);
        // ...but the cuts across the street are silent, whatever the ground says. A portal
        // here is a seam and not a doorway: there is nothing at it to hear, and the whoosh
        // that suits a level you step through a door in is the one sound that would give this
        // one away (`ext/audio.rs`, `set_portal_audible`).
        crate::ext::audio::set_portal_audible(false);
        // And the far side of a cut is shaded like the near side. A portal pass normally drops
        // normal mapping and the specular lobe -- worth it for a doorway-sized quad paid for
        // four times over, wrong for a seam that fills the screen, where it means the brick and
        // the kerbs change the instant you step across (`ext/view.rs`, `set_seamless_portals`).
        crate::ext::view::set_seamless_portals(true);
        // And the dark that hides where the street stops being drawn (module docs, "The fog").
        crate::ext::view::set_fog(FOG);
        // Nothing is set into this model and nothing stands outside it, so there is nothing to
        // carve out of it and nothing for the fence to take in but the file itself.
        let openings = Openings { cut: &[], also_inside: &[] };
        let spec = load_spec();
        let floor = interior::Floor { walk: FLOOR_Y, cap: CAP_Y };
        interior::load(gl, res, objs, player, &spec, &placement(), openings, floor, spawn());
        // The one thing in the sky (`ext/moon.rs`). It hangs off whichever eye is drawing, so
        // it needs nothing of the placement -- and so it survives the portals below.
        objs.push(Rc::new(RefCell::new(Moon::new(gl, res))) as Rc<RefCell<dyn ObjectT>>);

        // ── The street loops (module docs) ────────────────────────────────────────────────
        let west = Rc::new(RefCell::new(cut(res, LOOP_WEST_X)));
        let east = Rc::new(RefCell::new(cut(res, LOOP_EAST_X)));
        portals.push(west.clone());
        portals.push(east.clone());
        // Same orientation and a pure translation apart, so crossing either is a step of
        // exactly `LOOP_LENGTH` along the street with nothing turned, tilted or resized:
        // `Physical::try_portal` applies `west_l2w * east_w2l`, which for two parallel
        // portals is that translation and nothing else.
        connect(&west, &east);
        // ...and the net under them (`wrap_into_the_block`).
        objs.push(
            Rc::new(RefCell::new(RoomLogic::new(wrap_into_the_block))) as Rc<RefCell<dyn ObjectT>>
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ext::backrooms::FALL_DEPTH;
    use crate::ext::door::yaw_facing;
    use crate::ext::interior::probe::{ceiling_over, floor_under, nearest_hit, world_triangles};
    use crate::ext::interior::{facing, FENCE_MARGIN};
    use crate::physical::Physical;
    use std::cell::RefCell;

    /// Out of the house, in the model: what [`MODEL_YAW`] has to turn into [`FACING_WORLD`].
    const FACING_MODEL: Vector3 = Vector3 { x: 0.0, y: 0.0, z: 1.0 };

    fn tris() -> Vec<[Vector3; 3]> {
        world_triangles(&load_spec(), PART, &placement())
    }

    #[test]
    fn placement_puts_the_spawn_where_the_constants_say() {
        let m = placement().local_to_world();
        let spot = m.mul_point(SPAWN_MODEL);
        assert!((spot - SPAWN_SPOT).mag() < 1e-4, "spawn spot at {spot:?}");
        assert!((m.mul_direction(FACING_MODEL) - FACING_WORLD).mag() < 1e-6);
        let s = spawn();
        assert!((s.pos - (SPAWN_SPOT + Vector3::new(0.0, GH_PLAYER_HEIGHT, 0.0))).mag() < 1e-5);
        // Out of the house is the engine's default heading, so `--yaw 0` means "straight
        // ahead" here as everywhere else. (`Respawn`'s yaw is the camera's convention, in
        // which -z is 0; an `Object::euler.y` facing the same way is `door::yaw_facing`'s pi,
        // which is what MODEL_YAW turns the file by.)
        assert!(s.yaw.abs() < 1e-6, "the spawn looks {} off the default heading", s.yaw);
        assert!((yaw_facing(FACING_WORLD) - MODEL_YAW).abs() < 1e-6);
        assert!((facing(MODEL_YAW) - FACING_WORLD).mag() < 1e-6);
    }

    /// The tarmac is world zero and the lawn stands [`LAWN_Y`] over it -- the whole point of
    /// the placement, measured back out of the file at the road in front of the house and at
    /// the spawn.
    #[test]
    fn the_road_is_the_floor_and_the_lawn_stands_over_it() {
        let t = tris();
        let m = placement().local_to_world();
        let road = m.mul_point(Vector3::new(SPAWN_MODEL.x, 5.0, -100.0));
        let under = floor_under(&t, road).expect("tarmac");
        assert!((under - FLOOR_Y).abs() < 0.01, "the road is at {under}, not the floor");
        let lawn = floor_under(&t, spawn().pos).expect("lawn");
        assert!((lawn - LAWN_Y).abs() < 0.01, "the lawn is at {lawn}");
        // Which is what putting zero at the road rather than at the lawn buys: `fell_out`
        // measures downward only, so the lawn standing over the floor is a step up, and the
        // road -- the level's lowest ground -- is not under it at all.
        const { assert!(LAWN_Y > 0.0, "the lawn is not over the road") };
        assert!(under > FLOOR_Y - FALL_DEPTH, "walking the road would be a fall");
    }

    /// The spawn stands on flat lawn under open sky, with the brick of the recess two metres
    /// behind it and the whole garden and street clear in front.
    #[test]
    fn the_spawn_is_on_open_lawn_with_the_house_behind() {
        let t = tris();
        let eye = spawn().pos;
        let lawn = floor_under(&t, eye).expect("a lawn");
        assert!((eye.y - lawn - GH_PLAYER_HEIGHT).abs() < 0.01);
        assert_eq!(ceiling_over(&t, eye), None, "something is over the spawn");
        // Half a metre up rather than at the eye: at head height this stretch of wall is
        // windows, and a ray through one reaches the room's far wall 3.7 m further back.
        let low = eye - Vector3::new(0.0, GH_PLAYER_HEIGHT - 0.5, 0.0);
        let back = nearest_hit(&t, low, -FACING_WORLD).expect("the recess's wall");
        assert!((back - 2.0).abs() < 0.05, "the brick is {back} m behind, not two");
        let ahead = nearest_hit(&t, eye, FACING_WORLD).unwrap_or(f32::MAX);
        assert!(ahead > 15.0, "only {ahead} m of garden ahead of the spawn");
        // Flat lawn all round it: a metre either way is the same height, so nobody starts on
        // a kerb or half inside a bush.
        for step in [Vector3::new(1.0, 0.0, 0.0), Vector3::new(0.0, 0.0, 1.0)] {
            for s in [-1.0, 1.0] {
                let p = eye + step * s;
                let h = floor_under(&t, p).expect("lawn beside the spawn");
                assert!((h - lawn).abs() < 0.02, "the lawn is {h} a metre off at {p:?}");
            }
        }
    }

    /// The net under the portals: anyone outside the block is put back inside it by exactly
    /// one loop, keeping their heading, and anyone inside is left alone. It should never fire
    /// -- a crossing lands 2.4 cm inside -- which is precisely why it is worth having.
    #[test]
    fn the_net_wraps_only_what_is_outside_the_block() {
        let cam = crate::vector::Matrix4::identity();
        let ask = |x: f32| {
            room::take_respawn();
            let ctx = UpdateCtx {
                input: &crate::input::Input::new(),
                cam_to_world: cam,
                player_pos: Vector3::new(x, 2.31, -20.8),
                player_p_scale: 1.0,
                scene: &[],
            };
            wrap_into_the_block(&ctx);
            room::take_respawn()
        };
        // Inside, and on either cut exactly: left alone.
        for x in [LOOP_WEST_X, 0.0, LOOP_EAST_X, LOOP_WEST_X + 0.01, LOOP_EAST_X - 0.01] {
            assert!(ask(x).is_none(), "the net fired at x={x}, inside the block");
        }
        // A hand's width past either cut: put back one loop, and nothing else touched.
        for (out, want) in [
            (LOOP_WEST_X - 0.1, LOOP_EAST_X - 0.1),
            (LOOP_EAST_X + 0.1, LOOP_WEST_X + 0.1),
            (LOOP_WEST_X - 30.0, LOOP_EAST_X - 30.0),
        ] {
            let r = ask(out).unwrap_or_else(|| panic!("the net did not fire at x={out}"));
            assert!((r.pos.x - want).abs() < 1e-3, "x={out} went to {}, wanted {want}", r.pos.x);
            assert!((r.pos.y - 2.31).abs() < 1e-6 && (r.pos.z + 20.8).abs() < 1e-6);
            // The identity camera looks -z, which is the default heading: yaw 0.
            assert!(r.yaw.abs() < 1e-6, "the net turned the player to {}", r.yaw);
        }
    }

    /// The fog's one job: nothing is left to see by the time geometry reaches the far plane,
    /// so it never pops into view there. Computed with the shader's own formula
    /// (`Shaders/gltfpbr.frag`), so the constant and the curve cannot drift apart.
    #[test]
    fn the_fog_is_shut_before_the_far_plane_and_open_near_the_player() {
        // mix(1.0, t, squareness), then 1 - exp(-t), then the cap: line for line the shader.
        let fog_at = |d: f32| {
            let t = d * FOG.density;
            let t = t * (1.0 + FOG.squareness * (t - 1.0));
            (1.0 - (-t).exp()) * FOG.cap
        };
        let far = fog_at(crate::game_header::GH_FAR);
        assert!(far > 0.99, "only {far} of the way to the fog at the far plane -- it will pop");
        // A metre before the far plane it is already shut, so the last metre cannot flicker.
        assert!(fog_at(crate::game_header::GH_FAR - 10.0) > 0.98);
        // ...and it stays out of the way near the player: the house is 2 m from the spawn and
        // the van 8, and a fog that greyed those would be a curtain, not a distance.
        assert!(fog_at(10.0) < 0.10, "{} at ten metres", fog_at(10.0));
        assert!(fog_at(20.0) < 0.25, "{} at twenty metres", fog_at(20.0));
        assert_eq!(fog_at(0.0), 0.0, "the air at arm's length is not fog");
        // All the way to the colour: anything less leaves a rim at the far plane.
        assert_eq!(FOG.cap, 1.0);
        // ...and the colour is the one the sky behind it fades to, or the rim comes back the
        // other way up. `backrooms::CAP_COLOR` is pinned to `Shaders/sky.frag`'s interior
        // horizon; this is pinned to that, so the three cannot drift apart.
        let cap = crate::ext::backrooms::CAP_COLOR;
        assert_eq!(FOG.color, [cap[0], cap[1], cap[2]], "the fog and the sky must agree");
    }

    /// The cap hangs clear of the road, and still under every surface in the file.
    ///
    /// This is why [`CAP_Y`] exists (`interior::Floor`). At the shared floor height the cap
    /// sits 5 cm under the tarmac, which is finer than the depth buffer resolves at 60 m once
    /// the near plane collapses to a centimetre against a portal -- and the road is exactly
    /// what a player walking into a cut is looking down. Measured rather than argued: at 5 cm
    /// the road banded, at 3 m it did not, and a metre of clearance is the line drawn between
    /// them.
    #[test]
    fn the_ground_cap_hangs_clear_of_the_road_and_under_the_world() {
        let cap = CAP_Y - crate::ext::backrooms::CAP_DROP;
        assert!(FLOOR_Y - cap > 1.0, "the cap is only {} m under the road", FLOOR_Y - cap);
        // ...and under everything anyone can be standing on to see it. Not under the whole
        // model -- the house's basement is 3 m down, below the cap -- but nobody is ever there
        // to look up at it: the fall line is a metre higher, so a player who gets under the
        // floor is back at the spawn before the frame is drawn.
        assert!(cap < FLOOR_Y - FALL_DEPTH, "the cap is at {cap}, above the fall line");
        // And under the pool's floor, which the old shared height was not: a cap 5 cm under
        // the road cuts across a pool 1.4 m deep, and the empty pool reads as a black plate
        // instead of a hole.
        let t = tris();
        let m = placement().local_to_world();
        let pool = floor_under(&t, m.mul_point(Vector3::new(-23.0, 5.0, -144.0))).expect("a pool");
        assert!(cap < pool, "the cap is at {cap}, over the pool's floor at {pool}");
    }

    /// The pool in the back garden is deeper under the floor than the respawn rule allows,
    /// which is the whole of its behaviour: fall in and you wake up back at the spawn.
    #[test]
    fn the_empty_pool_is_a_fall() {
        let t = tris();
        let m = placement().local_to_world();
        let over = m.mul_point(Vector3::new(-23.0, 5.0, -144.0));
        let bottom = floor_under(&t, over).expect("the pool's floor");
        assert!(bottom < FLOOR_Y - FALL_DEPTH, "the pool bottom is at {bottom}");
    }

    #[test]
    fn spawn_is_inside_the_fence() {
        let spec = load_spec();
        let b = crate::ext::gltf_model::GltfModel::probe_bounds(&spec, PART);
        let (lo, hi) = interior::world_bounds(b, &placement());
        let p = spawn().pos;
        for (v, l, h) in [(p.x, lo.x, hi.x), (p.y, lo.y, hi.y), (p.z, lo.z, hi.z)] {
            assert!(v > l - FENCE_MARGIN && v < h + FENCE_MARGIN, "{p:?} outside {lo:?}..{hi:?}");
        }
    }

    /// The two cuts, connected, without a GL context: `Portal::detached` is there for this.
    fn loop_pair() -> (Rc<RefCell<Portal>>, Rc<RefCell<Portal>>) {
        let west = Rc::new(RefCell::new(Portal::detached()));
        let east = Rc::new(RefCell::new(Portal::detached()));
        place_cut(&mut west.borrow_mut(), LOOP_WEST_X);
        place_cut(&mut east.borrow_mut(), LOOP_EAST_X);
        connect(&west, &east);
        (west, east)
    }

    /// A `Physical` whose last step took it from `from` to `to`.
    fn stepped(from: Vector3, to: Vector3) -> Physical {
        let mut p = Physical::new();
        p.set_position(from);
        p.base.pos = to;
        p.velocity = (to - from) * (1.0 / crate::game_header::GH_DT);
        p
    }

    /// Walking east out of the block puts you at its west end, still walking east, the same
    /// height and the same distance from the kerb -- a step of exactly one [`LOOP_LENGTH`]
    /// west and nothing else. That is the whole level's one trick.
    #[test]
    fn walking_east_out_of_the_block_arrives_at_its_west_end() {
        let (_west, east) = loop_pair();
        let east = east.borrow();
        // Crossing the east cut, on the street, walking east.
        let (before, after) = (
            Vector3::new(LOOP_EAST_X - 0.1, 2.31, -20.8),
            Vector3::new(LOOP_EAST_X + 0.1, 2.31, -20.8),
        );
        let mut p = stepped(before, after);
        let speed = p.velocity.mag();
        assert!(p.try_portal(&east), "the east cut did not take a step across it");
        // One block west, to the millimetre-ish, and nothing else touched.
        let want = after - Vector3::new(LOOP_LENGTH, 0.0, 0.0);
        assert!((p.base.pos - want).mag() < 0.01, "landed at {:?}, wanted {want:?}", p.base.pos);
        assert!(p.base.pos.x > LOOP_WEST_X, "landed west of the block, not inside it");
        // Still walking east at the same speed: the warp is a translation, so nothing turns...
        assert!((p.velocity - Vector3::new(speed, 0.0, 0.0)).mag() < 1e-3, "{:?}", p.velocity);
        // ...nothing tilts the heading...
        assert!(p.base.euler.y.abs() < 1e-5, "the heading turned to {}", p.base.euler.y);
        // ...and nothing resizes the traveller, which a portal pair of different sizes would.
        assert!((p.base.p_scale - 1.0).abs() < 1e-6, "p_scale became {}", p.base.p_scale);
        assert!((p.prev_pos - p.base.pos).mag() < 1e-6, "prev_pos must follow, or it re-crosses");
    }

    /// And the other way: west out of the block arrives at its east end.
    #[test]
    fn walking_west_out_of_the_block_arrives_at_its_east_end() {
        let (west, _east) = loop_pair();
        let west = west.borrow();
        let after = Vector3::new(LOOP_WEST_X - 0.1, 2.31, -20.8);
        let mut p = stepped(Vector3::new(LOOP_WEST_X + 0.1, 2.31, -20.8), after);
        assert!(p.try_portal(&west));
        let want = after + Vector3::new(LOOP_LENGTH, 0.0, 0.0);
        assert!((p.base.pos - want).mag() < 0.01, "landed at {:?}, wanted {want:?}", p.base.pos);
        assert!(p.base.pos.x < LOOP_EAST_X, "landed east of the block, not inside it");
    }

    /// The cuts have to cover the whole cross-section of the world where the player is, or the
    /// unlooped street shows past their edges. Measured against the ground and the skyline the
    /// file actually has along those two planes -- not against the model's bounding box, which
    /// a radio mast 160 m off the street and well past the far plane stretches to twice the
    /// size of anything anyone can walk on.
    #[test]
    fn the_cuts_span_the_ground_and_the_skyline_at_their_planes() {
        let t = tris();
        let down = Vector3::new(0.0, -1.0, 0.0);
        for x in [LOOP_WEST_X, LOOP_EAST_X] {
            let mut p = Portal::detached();
            place_cut(&mut p, x);
            let m = p.base.local_to_world();
            // The quad's own axes, as `Portal::intersects` reads them: half-extents.
            let (ax, ay) = (m.x_axis(), m.y_axis());
            assert!(ax.x.abs() + ax.y.abs() < 1e-4, "the cut is not a plane of constant x");
            assert!(ay.x.abs() + ay.z.abs() < 1e-4, "the cut is not upright");
            let (zlo, zhi) = (p.base.pos.z - ax.mag(), p.base.pos.z + ax.mag());
            let (ylo, yhi) = (p.base.pos.y - ay.mag(), p.base.pos.y + ay.mag());
            // How far the ground reaches along this plane, how low it goes, and how high
            // anything on it gets.
            let (mut gz0, mut gz1) = (f32::MAX, f32::MIN);
            let (mut lowest, mut tallest) = (f32::MAX, f32::MIN);
            let mut z = zlo - 20.0;
            while z <= zhi + 20.0 {
                for dx in [-1.0f32, 0.0, 1.0] {
                    if let Some(g) = floor_under(&t, Vector3::new(x + dx, 3.0, z)) {
                        gz0 = gz0.min(z);
                        gz1 = gz1.max(z);
                        lowest = lowest.min(g);
                    }
                    let up = Vector3::new(x + dx, 40.0, z);
                    if let Some(h) = nearest_hit(&t, up, down).map(|d| 40.0 - d) {
                        tallest = tallest.max(h);
                    }
                }
                z += 0.5;
            }
            assert!(
                zlo < gz0 && zhi > gz1,
                "the cut spans z[{zlo}, {zhi}], the ground [{gz0}, {gz1}]"
            );
            assert!(ylo < lowest - 1.0, "the cut stops at y={ylo}, the ground at {lowest}");
            assert!(yhi > tallest + 1.0, "the cut stops at y={yhi}, the skyline at {tallest}");
        }
    }

    /// The block holds the whole abandoned house, so the thing the level is named for is what
    /// repeats -- and the spawn is inside it, not on the far side of a cut.
    #[test]
    fn the_block_holds_the_house_and_the_spawn() {
        let b = crate::ext::gltf_model::GltfModel::probe_bounds(&load_spec(), PART);
        let (lo, hi) = interior::world_bounds(b, &placement());
        assert!(LOOP_WEST_X > lo.x && LOOP_EAST_X < hi.x, "a cut is outside the model");
        let spawn = spawn().pos;
        assert!(spawn.x > LOOP_WEST_X && spawn.x < LOOP_EAST_X, "the spawn is outside the block");
        // The house itself: its recess wall is what the spawn stands two metres off, and both
        // ends of it are inside the block (`level31` module docs: model x[-58, -22]).
        let m = placement().local_to_world();
        for model_x in [-58.0f32, -22.0] {
            let world_x = m.mul_point(Vector3::new(model_x, LAWN_MODEL_Y, WALL_Z)).x;
            assert!(
                world_x > LOOP_WEST_X && world_x < LOOP_EAST_X,
                "the house reaches x={world_x}, outside the block"
            );
        }
    }

    /// Isolated: registered so SWITCH LEVEL can reach it, and named by nothing else -- no
    /// elevator floor, which is what keeps it a place of its own rather than a stop on a hub.
    #[test]
    fn registered_and_not_a_floor_of_the_elevator() {
        let name = "Liminal Neighborhood";
        crate::ext::scenes::SCENES.iter().find(|e| e.name == name).expect("listed");
        assert!(crate::ext::elevator::FLOORS.iter().all(|f| f.scene_name != name));
    }
}
