//! EXT: Scene `'` -- "Backrooms". Not part of the C++ port. NEW GAME starts here.
//!
//! The intro's meadow and its white door, exactly as in `level15` (both are `ext/meadow.rs`)
//! -- but the door no longer opens onto a sunset sea. It opens onto the Backrooms: a scanned,
//! light-baked office interior (`ext/backrooms.rs`) with yellowed walls, buzzing lamps and a
//! maze of rooms behind the hall you arrive in. Same weather trick, same linked door; only the
//! far world differs.
//!
//! # What is different about this far world
//!
//! * **It is solid.** The sea was a quad with one ground collider. The backrooms is 70,000
//!   triangles, and the player has to walk on its carpet and be stopped by its walls, skirting
//!   and armchairs -- so the whole model is a triangle-mesh collider (`ext/trimesh.rs`),
//!   consulted by the engine beside the rectangle colliders it was born with.
//! * **It is its own weather.** Every other surface takes the `mood` grade of the pass camera;
//!   the backrooms' lighting is painted into its maps and its shader ignores the grade. What
//!   does take it -- the return door's paint, and the sky through any gap in the scan's
//!   single-sided walls -- is told the far world is an *interior* (`view::MOOD_INTERIOR`):
//!   the door stays the white it is, and the sky is the near-black of an unlit building going
//!   on past its walls. A dark ground cap (`backrooms::GroundCap`) does the same for the
//!   ground below the horizon. The intro's sunset grade would have turned the door pink.
//! * **It is placed by its door.** The level says where the return door stands (`FAR`) and
//!   `Backrooms::new` puts the model's own door spot on that point, carpet at world y = 0.
//! * **It can be fallen out of.** Single-sided walls push a sphere caught inside them to
//!   whichever side its centre is on, and the wrong side has no floor. A player under the
//!   carpet is put back at the arrival point (`ARRIVAL`), facing down the hall, by a
//!   `RoomLogic` -- see `ext::backrooms::fell_out` for the rule.
//! * **There is no way back.** The door on the carpet is the same door, and a player who has
//!   just stepped through it is looking at it -- but the first step they take past the split
//!   (`meadow::in_far_world`), both doors vanish (`DoorLink::vanish`) and both portals are
//!   taken out of the scene (`room::request_remove_portals`). What is left at the hall's end
//!   is its bare wall. The meadow stays loaded, a kilometre away and culled, because the
//!   title screen is still looking at it. The title's own camera never crosses, so the
//!   backdrop keeps its door.
//! * **The way on is the elevator.** At the dead end of the entrance corridor south of the
//!   hall stands `ext::elevator::Elevator`, set into the end wall: the next floor is whatever
//!   `elevator::FLOORS` names after this one. A ride INTO this level arrives in its cabin.
//! * **The other way on is the window.** Between the second and third portraits on the
//!   hall's north wall hangs a small locked window (`ext::window::Window`) onto the Overgrown
//!   room, and the key to it is painted into the last portrait (`ext::painting`, `ext::key`).
//!   Unlocked, stood on a wall and grown to a door by the grab, it is walked through: the
//!   far copy of that room is loaded with this level (`window::FAR2`) and a `RoomLogic`
//!   loads the real one on the far side. One way, like the door.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ext::backrooms::{Backrooms, GroundCap, DOOR_FACING, DOOR_SPOT, FLOOR_Y};
use crate::ext::door::yaw_facing;
use crate::ext::elevator::{self, Elevator, PROUD, SLAB_X, THRESHOLD};
use crate::ext::interior::{fence_and_respawn, union};
use crate::ext::meadow::{door_with_portal, in_far_world, load_meadow, FAR};
use crate::ext::room::{request_remove_portals, Respawn, RoomLogic};
use crate::ext::view;
use crate::game_header::GH_PLAYER_HEIGHT;
use crate::object::ObjectT;
use crate::player::Player;
use crate::portal::connect;
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec, Scene};
use crate::vector::Vector3;

pub struct Level16;

/// Where a player who has fallen out of the building is put back: a stride inside the arrival
/// door, at standing height on the carpet, which is where walking through it lands them. They
/// face down the hall, away from the door, as they would have on arrival.
const ARRIVAL: Vector3 = Vector3 { x: FAR.x - 1.0, y: FAR.y + GH_PLAYER_HEIGHT, z: FAR.z };

/// The entrance corridor south of the hall (the scan's `Moquette_1`) dead-ends at this z, in
/// model coordinates: a single-sided wall seen from inside, between these x, with nothing of
/// the scan behind it. The elevator's cabin goes there.
const CORRIDOR_END_Z: f32 = -1.89;
const CORRIDOR_X: (f32, f32) = (-2.68, 2.38);
/// Where the elevator's threshold meets the end wall, in model coordinates. The slab is
/// 4.23 m wide and its doorway is off its centre, a metre east of it; centring the SLAB in
/// the 5.06 m corridor -- so a hand's width of the scan's own wall shows either side of it
/// and it reads as set into that wall -- puts the doorway east of the corridor's centre line
/// (-0.15), at x = 0.85. That also keeps the opening clear of the armchair the scan parks
/// against the end wall's west half: its east edge is at x = -1.01, and a doorway centred on
/// the corridor would have had its west jamb at -1.04, in the chair. The west jamb is at
/// -0.04 (`the_elevator_sits_in_the_corridors_end_wall` measures both).
const ELEVATOR_SPOT: Vector3 = Vector3 {
    x: 0.5 * (CORRIDOR_X.0 + CORRIDOR_X.1) + THRESHOLD.x - 0.5 * (SLAB_X.0 + SLAB_X.1),
    y: FLOOR_Y,
    // The slab stands proud of the wall, and the scan is carved away behind it
    // (`Backrooms::new`, from `Elevator::wall_cut`).
    z: CORRIDOR_END_Z + PROUD,
};
/// The doorway faces up the corridor, toward the hall (+z in the model's space).
const ELEVATOR_FACING: Vector3 = Vector3 { x: 0.0, y: 0.0, z: 1.0 };

/// The window's back (`ext/window.rs`), on the hall's north wall between the second and
/// third portraits: the wall's face (2.07, where the portraits hang too) less the grab's own
/// standoff, which is where a re-hung window sits. Its centre is a little under eye height:
/// grown to a door (`window::PASS_HEIGHT` and more) it must still fit under the far room's
/// 2.43 m ceiling, since the partner opening hangs at the same height (`window::PARTNER`).
const WINDOW_SPOT: Vector3 = Vector3 { x: 987.0, y: 1.35, z: 2.07 - crate::ext::grab::FLAT_OFFSET };
/// The wall's normal, into the hall.
const WINDOW_FACING: Vector3 = Vector3 { x: 0.0, y: 0.0, z: -1.0 };

/// Which sitter hangs on which of the hall's eight frames: an index into
/// `portrait_atlas::PORTRAITS` (0 the Mona Lisa, 1 the Girl with a Pearl Earring, 2 the
/// Laughing Cavalier) per seed. Seeds 0..5 run west to east along the north wall and 5..8
/// along the south. Written out rather than taken modulo the number of sitters, which put a
/// different face on every frame each time a sheet was added -- the key's among them. No two
/// neighbours on a wall repeat, and nothing faces its own likeness across the hall
/// (`the_hanging_covers_the_hall_and_pins_the_key_to_the_mona`).
const HANGING: [usize; 8] = [0, 1, 2, 1, 0, 2, 0, 2];
/// Where the frames hang along each wall, west to east, in world x.
const NORTH_X: [f32; 5] = [981.0, 985.0, 989.0, 993.0, 997.0];
const SOUTH_X: [f32; 3] = [983.0, 991.0, 999.0];
/// EXT: what the footsteps land on (`ext/audio.rs`). Two worlds in one scene, so the surface is
/// resolved where the player's feet are rather than fixed for the load: the scene opens in the
/// meadow, four hundred metres west of the carpet, and NEW GAME's first steps are taken there.
pub const SURFACE: crate::ext::audio::Surface = crate::ext::audio::Surface::SplitX {
    at: view::MOOD_SPLIT_X,
    west: &crate::ext::audio::Surface::Grass,
    east: &crate::ext::audio::Surface::Carpet,
};

/// The seed whose sitter wears the key: the last frame on the north wall, the one by the
/// bare end wall. [`HANGING`] must keep a Mona Lisa there.
const KEY_SEED: u32 = 4;

impl Scene for Level16 {
    fn load(
        &self,
        gl: &Rc<glow::Context>,
        res: &Resources,
        objs: &mut PObjectVec,
        portals: &mut PPortalVec,
        player: &mut Player,
    ) {
        let meadow = load_meadow(gl, res, objs, portals, player);
        // Past the split is a building, not a sunset: see the module docs. After `load_meadow`,
        // which turns the split on (and every load resets this to sunset).
        view::set_far_mood(view::MOOD_INTERIOR);
        crate::ext::audio::set_surface(SURFACE);

        // ── The elevator, set into the end wall of the entrance corridor. Built before the
        // backrooms because the scan's collision is cut away behind its doorway.
        let arrival = elevator::take_arrival();
        let lift = Elevator::new(
            gl,
            res,
            FAR - DOOR_SPOT + ELEVATOR_SPOT,
            yaw_facing(ELEVATOR_FACING),
            arrival,
        );
        if arrival.is_some() {
            // Delivered by a ride: in the cabin, facing its doors, which are about to open.
            lift.board(player);
        }

        // ── The backrooms: the model, placed so its door spot is FAR with the carpet at y = 0.
        let rooms = Backrooms::new(gl, res, FAR, &[lift.wall_cut()]);
        // The building's extent and the cabin's, which stands outside it behind the wall.
        let bounds = union(rooms.world_bounds(), lift.world_bounds());
        // Darkness under and around the building, for wherever its walls let the outside show.
        objs.push(Rc::new(RefCell::new(GroundCap::new(res, &rooms))) as Rc<RefCell<dyn ObjectT>>);
        objs.push(Rc::new(RefCell::new(rooms)) as Rc<RefCell<dyn ObjectT>>);
        objs.push(Rc::new(RefCell::new(lift.doors())) as Rc<RefCell<dyn ObjectT>>);
        objs.push(Rc::new(RefCell::new(lift)) as Rc<RefCell<dyn ObjectT>>);

        // An invisible fence round the lot, so nothing leaves through a doorway in an outer
        // wall, and the rule that puts a player who has fallen under the carpet (see the
        // module docs) back at the arrival point -- the carpet level is FAR.y by construction
        // (`carpet_lands_at_the_doors_foot`).
        fence_and_respawn(res, objs, bounds, FAR.y, Respawn::facing(ARRIVAL, -DOOR_FACING));

        // The same door, from the carpet. It faces the hall's end wall, so stepping out of it
        // means looking down the hall.
        let link = meadow.link_there.clone();
        let there =
            door_with_portal(gl, res, FAR, DOOR_FACING, false, meadow.link_there, objs, portals);

        // Walk in here, walk out there...
        connect(&meadow.here, &there);

        // ...and that is all. The first step past the split -- the step after the warp, or
        // the first step of a level entered by elevator -- takes both doors and both portals
        // away for good (see the module docs). The title backdrop's camera is parked on the
        // meadow and never gets here.
        let portal_ids = [meadow.here.borrow().id, there.borrow().id];
        let doors = link.clone();
        objs.push(Rc::new(RefCell::new(RoomLogic::new(move |ctx| {
            if !link.vanished() && in_far_world(ctx.player_pos) {
                link.vanish();
                request_remove_portals(&portal_ids);
            }
        }))) as Rc<RefCell<dyn ObjectT>>);

        // ── Portraits along the hall, watching (`ext/painting.rs`). Five on the north wall,
        // three on the south, each a hair off its wall face so nothing is coplanar with the
        // scan, the three sitters hung where [`HANGING`] says (`ext/portrait_atlas.rs`). Every
        // frame is the same width and takes its height from its own sitter's aspect, so the
        // row is a row of pictures and not of boxes. The watch is taken now, after both doors
        // exist, so a painting knows it is being looked at through the meadow door as well as
        // from the carpet -- and only while the doors stand: once they have gone, so have
        // their portals.
        {
            use crate::ext::painting::{KeySpec, Painting, Watch};
            use crate::ext::portrait_atlas::PORTRAITS;
            /// The hall's wall faces in world z: `backrooms::DOOR_SPOT` puts model z = 3.48 and
            /// 7.07 here (the scan's walls are planes; a ray probe along the hall finds them
            /// at -1.5214 and 2.0721 at every x).
            const HALL_SOUTH_Z: f32 = -1.52;
            const HALL_NORTH_Z: f32 = 2.07;
            /// Clearance a painting's back keeps from the wall, so the two never z-fight.
            const WALL_GAP: f32 = 0.02;
            /// Centre height, and width: the height follows each portrait's own aspect. Eye
            /// level for a standing player is 1.5, so the sitters' eyes -- above the canvas
            /// centre -- are just above theirs.
            const HEIGHT: f32 = 1.6;
            const WIDTH: f32 = 0.8;
            /// The key (`painting.rs`, "The key in the painting") is in the last portrait on
            /// the north wall, the one by the bare end wall -- a Mona Lisa, on whose dark
            /// bodice the gold reads -- and is seen from 2.6 m west of it along the wall and
            /// half a metre out -- a grazing look along the wall, at eye height: the spot,
            /// how near to it the eye must be, and how near to the canvas centre the look.
            const KEY_SPEC: KeySpec = KeySpec {
                view: Vector3 { x: 994.4, y: GH_PLAYER_HEIGHT, z: 1.55 },
                radius: 0.45,
                cone: 25.0 * std::f32::consts::PI / 180.0,
            };

            let watch = Watch::new(objs, portals).while_doors_stand(doors);
            let hang = |x: f32, wall_z: f32, facing_z: f32, seed: u32| {
                let centre = Vector3::new(x, HEIGHT, wall_z + facing_z * WALL_GAP);
                let facing = Vector3::new(0.0, 0.0, facing_z);
                let key = (seed == KEY_SEED).then_some(KEY_SPEC);
                let portrait = &PORTRAITS[HANGING[seed as usize]];
                // EXT: the anamorphic key's KEY_ON_PLANE spot was tuned for the Mona's dark
                // bodice; an edit to the hanging would silently re-seat the key on whichever
                // sitter lands on this seed. Pin it so such an edit fails here instead of
                // shipping a key on the wrong painting.
                if key.is_some() {
                    assert_eq!(portrait.name, "mona", "KEY_SEED must land on the Mona");
                }
                Painting::new(gl, res, centre, facing, WIDTH, portrait, seed, watch.clone(), key)
            };
            let north = NORTH_X.into_iter().map(|x| (x, HALL_NORTH_Z, -1.0));
            let south = SOUTH_X.into_iter().map(|x| (x, HALL_SOUTH_Z, 1.0));
            for (seed, (x, wall_z, facing_z)) in north.chain(south).enumerate() {
                objs.push(Rc::new(RefCell::new(hang(x, wall_z, facing_z, seed as u32)))
                    as Rc<RefCell<dyn ObjectT>>);
            }
        }

        // ── Three things on the carpet a few steps in from the door, each a rigid body
        // (`ext/rigid.rs`, `ext/physics.rs`): an apple, a die and a chess king. Grab them,
        // throw them, knock them over. Placed a hair above their resting height so the first
        // steps settle them onto the carpet rather than start them inside it; the king
        // stands on its origin and is placed ON the carpet.
        {
            use crate::ext::physics::{Material, Shape};
            use crate::ext::rigid::RigidProp;
            let props = [
                RigidProp::new(
                    res,
                    "apple",
                    "apple.obj",
                    "apple.bmp",
                    Shape::Ball { radius: 0.045 },
                    Material { friction: 0.7, restitution: 0.25, density: 800.0 },
                    Vector3::new(997.4, 0.06, 0.6),
                ),
                RigidProp::new(
                    res,
                    "dice",
                    "dice.obj",
                    "dice.bmp",
                    // The inner box plus the border is the 6 cm die (`Shape::RoundCuboid`).
                    Shape::RoundCuboid { half: Vector3::splat(0.026), radius: 0.004 },
                    Material { friction: 0.5, restitution: 0.35, density: 1200.0 },
                    Vector3::new(996.7, 0.04, -0.5),
                ),
                RigidProp::new(
                    res,
                    "king",
                    "chess_king.obj",
                    "chess_wood.bmp",
                    Shape::Cylinder { radius: 0.03, height: 0.14 },
                    Material { friction: 0.6, restitution: 0.1, density: 700.0 },
                    Vector3::new(998.3, 0.0, -0.9),
                ),
            ];
            for prop in props {
                objs.push(Rc::new(RefCell::new(prop)) as Rc<RefCell<dyn ObjectT>>);
            }
        }

        // ── The window (`ext/window.rs`): a small locked frame on the north wall between the
        // second and third portraits, and through it the Overgrown room. The room is loaded a
        // second time, two kilometres east (`window::FAR2`), from `level18`'s own spec and
        // placement -- with the level's elevator cabin standing still in its wall, so the copy
        // is the level down to the cut the loader carves and the two loads share one parse
        // (`interior::build`, `window::Still`). Once the key has opened the window and the
        // grab has made a door of it, walking through lands in the copy, and the logic at the
        // end loads the real level at the same spot. One way: the elevator brings you back.
        {
            use crate::ext::interior::{self, Openings};
            use crate::ext::room::request_scene_load;
            use crate::ext::scenes;
            use crate::ext::window::{
                self, Still, Window, CROSSING_X, FAR2, PARTNER, PARTNER_FACING,
            };
            use crate::level18;
            use crate::portal::Portal;

            /// The level to load once the player is in the copy.
            const OVERGROWN: usize = match scenes::index_of("Overgrown") {
                Some(i) => i,
                None => panic!("the scene registry has no Overgrown scene for the window"),
            };

            // The copy. The cabin is built where the level builds its own and moved after,
            // so its cut is the level's to the bit (see `interior::build`).
            let far_lift =
                Elevator::new(gl, res, level18::ELEVATOR_SPOT, level18::ELEVATOR_YAW, None);
            let openings =
                Openings { cut: &[far_lift.wall_cut()], also_inside: &[far_lift.world_bounds()] };
            // Whoever ends up under the copy's floor is put back a stride in from the partner
            // opening; the crossing below then takes them on to the level, as it would have.
            let respawn = Respawn::facing(
                FAR2 + PARTNER + PARTNER_FACING * 0.6 + Vector3::new(0.0, GH_PLAYER_HEIGHT, 0.0),
                PARTNER_FACING,
            );
            interior::build(
                gl,
                res,
                objs,
                &level18::load_spec(),
                &level18::placement(),
                openings,
                0.0,
                respawn,
                FAR2,
            );
            objs.push(Rc::new(RefCell::new(Still::new(Box::new(far_lift), FAR2)))
                as Rc<RefCell<dyn ObjectT>>);

            // The opening and its partner, in the scene's portal vector like any other pair;
            // the window places and connects them every step.
            let here = Rc::new(RefCell::new(Portal::new(res)));
            let there = Rc::new(RefCell::new(Portal::new(res)));
            portals.push(here.clone());
            portals.push(there.clone());
            let window =
                Window::new(gl, res, WINDOW_SPOT, WINDOW_FACING, here, there, window::preset());
            objs.push(Rc::new(RefCell::new(window)) as Rc<RefCell<dyn ObjectT>>);

            // The crossing: a player in the copy is loaded into the level at the same spot,
            // looking the same way. No fade -- the two are the same room.
            objs.push(Rc::new(RefCell::new(RoomLogic::new(|ctx| {
                if ctx.player_pos.x > CROSSING_X {
                    let (yaw, pitch) = window::look_of(&ctx.cam_to_world);
                    window::set_arrival(ctx.player_pos - FAR2, yaw, pitch);
                    request_scene_load(OVERGROWN);
                }
            }))) as Rc<RefCell<dyn ObjectT>>);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ext::door::portal_placement;
    use crate::ext::elevator::OPENING_X;
    use crate::ext::gltf_model::{Anchor, Fit, Frame, GltfModel, Load, PartSpec};
    use crate::ext::meadow::DOOR_POS;
    use crate::game_header::{GH_PI, GH_PLAYER_RADIUS};
    use crate::object::Object;

    /// The entrance corridor's dead end, measured from the scan (no GL): the end wall's z and
    /// x extent, its ceiling, and the east-most furniture standing against it.
    struct CorridorEnd {
        wall_z: f32,
        wall_x: (f32, f32),
        ceiling_y: f32,
        /// Largest x of anything within a metre of the end wall that is not wall, floor or
        /// ceiling: the armchair.
        furniture_east: f32,
    }

    fn measure_corridor_end() -> CorridorEnd {
        const PARTS: [PartSpec<'static>; 1] = [PartSpec {
            name: "all",
            roots: &["Sketchfab_model"],
            skip: &[],
            frame: Frame::Local,
            anchor: Anchor::Hinge,
        }];
        let spec = Load {
            path: "Meshes/backrooms_vr.glb",
            parts: &PARTS,
            fit: Fit::Identity,
            max_map: 1024,
            translucent: &[],
            metallic_override: &[],
            cut_boxes: &[],
        };
        let (pos, idx) = GltfModel::probe_triangles(&spec, "all");
        let v = |i: u32| Vector3::from_slice(&pos[i as usize]);
        // The corridor, generously: the tests below only need what is near ELEVATOR_SPOT.
        let near = |p: Vector3| (p.x - ELEVATOR_SPOT.x).abs() < 4.0 && p.z > -2.5 && p.z < -0.5;
        let (mut wall_z, mut wall_x, mut ceiling_y, mut furniture_east) =
            (f32::MAX, (f32::MAX, f32::MIN), f32::MAX, f32::MIN);
        for t in idx.chunks_exact(3) {
            let (a, b, c) = (v(t[0]), v(t[1]), v(t[2]));
            if !(near(a) && near(b) && near(c)) {
                continue;
            }
            let n = (b - a).cross(c - a);
            let area = n.mag() * 0.5;
            let n = n.normalized_safe();
            let (lo_x, hi_x) = (a.x.min(b.x).min(c.x), a.x.max(b.x).max(c.x));
            let (lo_y, hi_y) = (a.y.min(b.y).min(c.y), a.y.max(b.y).max(c.y));
            if area > 0.3 && n.z > 0.99 && lo_y < FLOOR_Y + 1.0 && hi_y > FLOOR_Y + 1.0 {
                // A wall face spanning mid-height, facing into the corridor.
                wall_z = wall_z.min(a.z);
                wall_x = (wall_x.0.min(lo_x), wall_x.1.max(hi_x));
            } else if area > 0.3 && n.y < -0.99 && lo_y > FLOOR_Y + 2.0 {
                ceiling_y = ceiling_y.min(a.y);
            } else if area < 0.3
                && lo_y > FLOOR_Y + 0.05
                && hi_y < FLOOR_Y + 2.0
                && lo_x > -2.58
                && hi_x < 2.28
            {
                // Small faces off the floor, below the wall's middle and clear of both side
                // walls (whose frames and trim are small faces too): furniture.
                furniture_east = furniture_east.max(hi_x);
            }
        }
        CorridorEnd { wall_z, wall_x, ceiling_y, furniture_east }
    }

    /// The elevator's slab must fit in the corridor's end wall, its doorway must be clear
    /// of the armchair, and its threshold must stand a hair proud of the wall -- all measured
    /// from the scan, so the placement constants cannot drift out from under the model.
    #[test]
    fn the_elevator_sits_in_the_corridors_end_wall() {
        let end = measure_corridor_end();
        assert!((end.wall_z - CORRIDOR_END_Z).abs() < 0.01, "end wall at z = {}", end.wall_z);
        assert!((end.wall_x.0 - CORRIDOR_X.0).abs() < 0.02, "west wall at x = {}", end.wall_x.0);
        assert!((end.wall_x.1 - CORRIDOR_X.1).abs() < 0.02, "east wall at x = {}", end.wall_x.1);
        assert!((end.ceiling_y - 2.75).abs() < 0.01, "ceiling at y = {}", end.ceiling_y);
        assert!((end.furniture_east + 1.01).abs() < 0.05, "chair to x = {}", end.furniture_east);

        // The slab, as `Elevator::new` places it about the threshold (its extents are the
        // elevator's own measured constants), fits in the end wall...
        let threshold = ELEVATOR_SPOT;
        let slab_w = threshold.x + SLAB_X.0 - THRESHOLD.x;
        let slab_e = threshold.x + SLAB_X.1 - THRESHOLD.x;
        assert!(slab_w > end.wall_x.0 && slab_e < end.wall_x.1, "slab {slab_w}..{slab_e}");
        // ...and roughly centred in it.
        let off = 0.5 * (slab_w + slab_e) - 0.5 * (end.wall_x.0 + end.wall_x.1);
        assert!(off.abs() < 0.05, "slab {off} m off the corridor's centre");
        // The opening clears the chair by more than a player.
        let jamb_w = threshold.x + OPENING_X.0 - THRESHOLD.x;
        assert!(jamb_w - end.furniture_east > 2.0 * GH_PLAYER_RADIUS, "jamb at {jamb_w}");
        // Proud of the wall, by less than a skirting's depth.
        assert!(threshold.z > end.wall_z && threshold.z - end.wall_z < 0.05);
        assert_eq!(threshold.y, FLOOR_Y, "the cabin floor meets the carpet");
        assert!((ELEVATOR_FACING.mag() - 1.0).abs() < 1e-6 && ELEVATOR_FACING.z > 0.0);
    }

    /// The hanging: a sitter the atlas has on each of the eight frames, all three hung, the
    /// key's frame a Mona Lisa (so `load`'s assert cannot fire), no two neighbours along a
    /// wall alike, and nothing facing its own likeness across the hall -- the hall is 3.6 m
    /// wide, so a south frame faces the north ones within a couple of metres of its x.
    #[test]
    fn the_hanging_covers_the_hall_and_pins_the_key_to_the_mona() {
        use crate::ext::portrait_atlas::PORTRAITS;
        assert_eq!(HANGING.len(), NORTH_X.len() + SOUTH_X.len());
        assert!(HANGING.iter().all(|&i| i < PORTRAITS.len()));
        for name in ["mona", "vermeer", "cavalier"] {
            assert!(HANGING.iter().any(|&i| PORTRAITS[i].name == name), "{name} is not hung");
        }
        assert_eq!(PORTRAITS[HANGING[KEY_SEED as usize]].name, "mona");
        for wall in [&HANGING[..NORTH_X.len()], &HANGING[NORTH_X.len()..]] {
            assert!(wall.windows(2).all(|w| w[0] != w[1]), "two of a kind side by side");
        }
        for (s, &sx) in SOUTH_X.iter().enumerate() {
            for (n, &nx) in NORTH_X.iter().enumerate() {
                if (sx - nx).abs() < 2.5 {
                    assert_ne!(
                        HANGING[NORTH_X.len() + s],
                        HANGING[n],
                        "seed {n} is faced by its own"
                    );
                }
            }
        }
    }

    /// The carpet must be at world y = 0 where the door stands: the door's foot is at FAR.y
    /// and the model is shifted so its floor level lands there.
    #[test]
    fn carpet_lands_at_the_doors_foot() {
        let model_pos = FAR - DOOR_SPOT;
        assert!((model_pos.y + crate::ext::backrooms::FLOOR_Y - FAR.y).abs() < 1e-6);
    }

    /// The two doors have different yaws (the meadow's faces +z, the carpet's faces +x), and
    /// `connect` must carry a player across that turn. This reproduces the transform
    /// `connect_warps` builds for walking in through `here` (portal.rs: `there_l2w * here_w2l`)
    /// and the re-aim `Physical::try_portal` applies to it, on the transforms the two doors'
    /// portals actually get -- `door_with_portal` and this both call `portal_placement`.
    #[test]
    fn walking_through_lands_in_the_hall_facing_down_it() {
        let portal = |pos: Vector3, facing: Vector3| {
            let (centre, euler, scale) = portal_placement(pos, yaw_facing(facing));
            let mut p = Object::new();
            p.pos = centre;
            p.euler = euler;
            p.scale = scale;
            p
        };
        let here = portal(DOOR_POS, Vector3::new(0.0, 0.0, 1.0));
        let there = portal(FAR, DOOR_FACING);
        let delta_inv = there.local_to_world() * here.world_to_local();

        // The relative yaw is a quarter turn.
        assert!((there.euler.y - here.euler.y - GH_PI / 2.0).abs() < 1e-6);

        // A player a hair past the meadow door's plane, having walked in along -z...
        let p = DOOR_POS + Vector3::new(0.0, GH_PLAYER_HEIGHT, -0.01);
        let q = delta_inv.mul_point(p);
        // ...lands the same hair past the carpet door on its -x side, at the same height.
        let expect = FAR + Vector3::new(-0.01, GH_PLAYER_HEIGHT, 0.0);
        assert!((q - expect).mag() < 1e-3, "landed at {q:?}, expected {expect:?}");
        // The respawn point is a stride further along the same line: where an arrival would
        // be a moment later, not somewhere else in the building.
        let along = ARRIVAL - expect;
        assert!(along.x < -0.5 && along.y.abs() < 1e-6 && along.z.abs() < 1e-6, "{along:?}");

        // Physical::try_portal re-aims the camera yaw from the warped forward vector.
        let yaw_in = 0.0f32; // looking -z at the meadow door, as the spawn does
        let forward = Vector3::new(-yaw_in.sin(), 0.0, -yaw_in.cos());
        let new_dir = delta_inv.mul_direction(forward);
        let yaw_out = -new_dir.x.atan2(-new_dir.z);
        let forward_out = Vector3::new(-yaw_out.sin(), 0.0, -yaw_out.cos());
        // Facing -x: down the hall, away from the end wall the door faces.
        assert!(forward_out.x < -0.999, "came out facing {forward_out:?}");
    }

    /// The window's wall: flat behind the whole of a door-sized frame, measured from the
    /// scan. Rays from across the opening's face at its largest useful size, cast into the
    /// wall, all meet it a hand's width behind the frame's back -- no skirting, picture rail
    /// or doorway in the way -- and the paintings either side are clear of the frame.
    /// The scan as a whole, and where `Backrooms::new` stands it.
    fn backrooms() -> (Load<'static>, Object) {
        const PARTS: [PartSpec<'static>; 1] = [PartSpec {
            name: "all",
            roots: &["Sketchfab_model"],
            skip: &[],
            frame: Frame::Local,
            anchor: Anchor::Hinge,
        }];
        let spec = Load {
            path: "Meshes/backrooms_vr.glb",
            parts: &PARTS,
            fit: Fit::Identity,
            max_map: 1024,
            translucent: &[],
            metallic_override: &[],
            cut_boxes: &[],
        };
        let mut placement = Object::new();
        placement.pos = FAR - DOOR_SPOT;
        (spec, placement)
    }

    #[test]
    fn the_window_hangs_on_a_flat_stretch_of_the_north_wall() {
        use crate::ext::window::{BAR_HALF, OPENING, PASS_HEIGHT};
        let (spec, placement) = backrooms();
        let tris = crate::ext::interior::probe::world_triangles(&spec, "all", &placement);
        // A door and a bit: the largest the window gets before its top meets the hall's
        // ceiling, with the frame's bars round it.
        let scale = (PASS_HEIGHT + 0.2) / OPENING.1;
        let half_w = (0.5 * OPENING.0 + 2.0 * BAR_HALF) * scale;
        let half_h = (0.5 * OPENING.1 + 2.0 * BAR_HALF) * scale;
        let into_wall = -WINDOW_FACING;
        for i in 0..=6 {
            for j in 0..=6 {
                let x = WINDOW_SPOT.x - half_w + half_w * i as f32 / 3.0;
                // The bars' full reach, short of the ceiling and the carpet.
                let y = (WINDOW_SPOT.y - half_h + half_h * j as f32 / 3.0).clamp(0.1, 2.45);
                // From a stride in front of the frame's back, into the wall.
                let from = Vector3::new(x, y, WINDOW_SPOT.z - 0.5);
                let d = crate::ext::interior::probe::nearest_hit(&tris, from, into_wall)
                    .expect("a wall behind the window");
                let gap = d - 0.5;
                assert!(
                    gap > 0.0 && gap < 0.03,
                    "at ({x:.2}, {y:.2}) the wall is {gap:.3} m behind the frame's back"
                );
            }
        }
        // The opening itself (the bars may run into the ceiling; they are hidden there)
        // clears the hall's ceiling and its carpet.
        let half_open = 0.5 * OPENING.1 * scale;
        assert!(WINDOW_SPOT.y + half_open < 2.55, "the opening clears the hall's ceiling");
        assert!(WINDOW_SPOT.y - half_open > 0.0, "and its carpet");
        // The portraits at 985 and 989 are 0.8 m wide; the frame is between them.
        assert!(WINDOW_SPOT.x - half_w > 985.0 + 0.4 + 0.1);
        assert!(WINDOW_SPOT.x + half_w < 989.0 - 0.4 - 0.1);
    }

    /// The partner opening's stretch of the Overgrown room's west wall, measured from that
    /// file through `level18`'s own placement: a wall a quarter of a metre behind the opening
    /// across its whole door-sized face, floor at 0 and the ceiling over it, and nothing in
    /// front of it for a stride and more, so the player steps out of the window into the
    /// room and not into a maze wall.
    #[test]
    fn the_partner_opening_is_clear_on_the_far_rooms_west_wall() {
        use crate::ext::interior::{
            self,
            probe::{floor_under, nearest_hit, world_triangles},
        };
        use crate::ext::window::{OPENING, PARTNER, PARTNER_FACING, PASS_HEIGHT};
        use crate::level18;
        let tris = world_triangles(&level18::load_spec(), level18::PART, &level18::placement());
        let centre = Vector3::new(PARTNER.x, WINDOW_SPOT.y, PARTNER.z);
        let scale = (PASS_HEIGHT + 0.2) / OPENING.1;
        let (half_w, half_h) = (0.5 * OPENING.0 * scale, 0.5 * OPENING.1 * scale);
        let across = PARTNER_FACING.cross(Vector3::new(0.0, 1.0, 0.0));
        for i in 0..=6 {
            for j in 0..=6 {
                let p = centre
                    + across * (half_w * (i as f32 / 3.0 - 1.0))
                    + Vector3::new(0.0, half_h * (j as f32 / 3.0 - 1.0), 0.0);
                let behind = nearest_hit(&tris, p, -PARTNER_FACING).expect("the west wall");
                assert!((behind - 0.25).abs() < 0.02, "wall {behind} m behind {p:?}");
                let ahead = nearest_hit(&tris, p, PARTNER_FACING).unwrap_or(f32::MAX);
                assert!(ahead > 2.0, "only {ahead} m clear in front of {p:?}");
            }
        }
        let floor = floor_under(&tris, centre).expect("a floor");
        assert!(floor.abs() < 0.01, "floor at {floor}");
        assert!(centre.y + half_h < 2.43, "the opening clears the far room's ceiling");
        // The copy is well clear of everything else in this scene -- the Backrooms' scan
        // ends a kilometre short of it, with the crossing line between the two -- and well
        // past the mood split, as its interior grade needs.
        use crate::ext::window::{CROSSING_X, FAR2};
        let (hall_spec, hall_placement) = backrooms();
        let hall = GltfModel::probe_bounds(&hall_spec, "all");
        let (_, hall_hi) = interior::world_bounds(hall, &hall_placement);
        let copy = GltfModel::probe_bounds(&level18::load_spec(), level18::PART);
        let (copy_lo, _) = interior::world_bounds(copy, &level18::placement());
        let copy_lo = copy_lo + FAR2;
        assert!(hall_hi.x + 100.0 < CROSSING_X, "the scan ends at x = {}", hall_hi.x);
        assert!(CROSSING_X + 100.0 < copy_lo.x, "the copy begins at x = {}", copy_lo.x);
        assert!(copy_lo.x > crate::ext::view::MOOD_SPLIT_X + 1000.0);
    }
}
