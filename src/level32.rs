//! EXT: Scene "Open House". Not part of the C++ port.
//!
//! Liminal Neighborhood's suburb, with every EMPTY house made real. The stage GLB models
//! one true house -- the two-storey brick one behind the spawn, furnished room by room,
//! basement to boxes -- and lines the rest of both street rows with hollow facade shells,
//! seventeen identical fronts baked into one backdrop mesh with nothing behind the
//! windows. This scene keeps the original house exactly as authored and REPLACES the
//! shells: each shell lot is carved out of the stage (one world-space cut box per lot,
//! drawn and collided alike -- the lawn continues underneath, measured), and a PROCEDURAL
//! house from Elbolillo's village pack is built on it (`ext/prochouse.rs`): the same small
//! shell every time, furnished differently by a per-lot seed. `--house-seed N` pins the
//! street; the log prints the night's number.
//!
//! Somebody's exploration kit is scattered across the street (`ext/tool.rs`): eight
//! instruments, ONE hidden per house, the flashlight always in the house straight across
//! the road from the spawn. Take them (E), pocket them (F), switch them on (E again):
//! every instrument reads the same one signal -- the mannequin that walks the pavement,
//! reported to `tool::report_presence` each step. The nearer it walks, the higher the
//! bars, the lower the mercury.
//!
//! The mannequin walker stays from this scene's proving-ground days: one Mixamo mannequin
//! ping-ponging a line of the verge, playing its in-place walk clip, finding the sloping
//! ground by raycast every step.
//!
//! Numbering note: level19-29 stay reserved (see level30.rs).

use std::cell::RefCell;
use std::rc::Rc;

use crate::camera::Camera;
use crate::ext::disguise::{self, Disguise, Station, WearOffers, Wearable};
use crate::ext::interior::{self, Openings};
use crate::ext::moon::Moon;
use crate::ext::npc::{self, Npc};
use crate::ext::pack::PackProps;
use crate::ext::room::RoomLogic;
use crate::ext::skinned::{Instance, SkinnedModel};
use crate::ext::warphouse::{self, Mouth};
use crate::ext::{hunt, prochouse, raycast, room, tool};
use crate::game_header::{GH_DT, GH_PI};
use crate::level31;
use crate::object::{Object, ObjectT, RenderCtx, UpdateCtx};
use crate::player::Player;
use crate::portal::connect;
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec, Scene};
use crate::vector::Vector3;

pub struct Level32;

// ---------------------------------------------------------------------------------------
// The lots: where the facade shells were.
//
// The backdrop mesh (`Fondo_Casa_f_0`) lines the street with hollow fronts in two rows,
// eight north (the ninth north slot holds the real house, untouched) and nine south, each
// about 24 m wide on a 48 m pitch. Their footprints were measured by clustering the mesh's
// wall band in Blender; under level31's placement (world = rot_y(PI) * model + t) each
// blob becomes the world cut box carved out of the stage. The box floor sits 2 cm over the
// lawn -- which, measured, DOES continue under every shell (unlike under the real house)
// -- and the ceiling clears the shells' rooflines.
// ---------------------------------------------------------------------------------------

/// EXT-round: where a seeker waits out the hide phase -- a short loop on the porch and the
/// front path, backs to the door (`ext/hunt.rs`). Model space, like every other spot here.
const POST_A: [(f32, f32); 3] = [(7.80, 9.60), (5.60, 10.40), (7.80, 11.20)];
const POST_B: [(f32, f32); 3] = [(7.80, 10.80), (10.20, 10.40), (7.80, 9.90)];

/// EXT-round: the sweep. Every waypoint sits inside a `prochouse` room rect or one of its
/// CLEAR doorway zones, so the route cannot ask a seeker to walk into a sofa. The two
/// seekers walk the same ring started three rooms apart, so they split the house instead of
/// queueing behind each other.
const SWEEP: [(f32, f32); 6] = [
    (7.80, 4.20),   // the corridor, in from the front door
    (12.10, 1.20),  // living room
    (12.10, -2.40), // lounge
    (13.20, 4.60),  // big bedroom
    (9.80, 4.60),   // small bedroom
    (5.30, 2.30),   // the kitchen's free floor
];

/// One street lot: the model-space x span of the shell that stood there, and its row.
struct Lot {
    model_x: [f32; 2],
    north: bool,
}

/// Shell rows in stage MODEL space (gltf): north row shells span gz[-141.2,-122.4], south
/// row gz[-74.3,-55.5]; heights up to 6.0.
const LOTS: [Lot; 17] = [
    Lot { model_x: [-236.0, -210.0], north: true },
    Lot { model_x: [-186.0, -162.0], north: true },
    Lot { model_x: [-138.0, -114.0], north: true },
    Lot { model_x: [-90.0, -64.0], north: true },
    Lot { model_x: [-2.0, 24.0], north: true },
    Lot { model_x: [46.0, 72.0], north: true },
    Lot { model_x: [96.0, 120.0], north: true },
    Lot { model_x: [144.0, 168.0], north: true },
    Lot { model_x: [-236.0, -210.0], north: false },
    Lot { model_x: [-186.0, -162.0], north: false },
    Lot { model_x: [-138.0, -114.0], north: false },
    Lot { model_x: [-90.0, -64.0], north: false },
    Lot { model_x: [-46.0, -20.0], north: false },
    Lot { model_x: [-2.0, 24.0], north: false },
    Lot { model_x: [46.0, 72.0], north: false },
    Lot { model_x: [96.0, 120.0], north: false },
    Lot { model_x: [144.0, 168.0], north: false },
];

/// The house straight across the road from the spawn: where the flashlight always is.
const LOT_FACING_SPAWN: usize = 12;

// ---------------------------------------------------------------------------------------
// The fittings: what is non-Euclidean about this street, and where.
//
// Every one of these is a portal pair (`ext/warphouse.rs`), and the portal budget is hard:
// `GH_MAX_PORTALS` is 16 and the render path indexes fixed arrays by portal index, so a
// seventeenth is a panic every frame in release. The street loop spends 2; these spend 6.
// ---------------------------------------------------------------------------------------

/// The lot whose lounge holds the scale mouth: the house across the road from the spawn,
/// so the first house anyone walks into is the one that is bigger inside.
const SCALE_LOT: usize = LOT_FACING_SPAWN;
/// The lot whose big bedroom has a pocket in the back of it.
const POCKET_LOT: usize = LOT_FACING_SPAWN;
/// The far end of the ghost house's second pocket: a north-row house well down the street,
/// so the pocket is a way THROUGH the block and not just a cupboard.
const POCKET_LOT_B: usize = 4;

/// The house that is nowhere -- reachable only through a pocket, standing three kilometres
/// off the street where nothing will ever walk into it. The design doc calls this shape the
/// orphan bubble, and its rule for a hidden room is that it always has two watchable exits;
/// this one has exactly two, one at each end of the block.
///
/// It is moved out along Z, not X, and that is not arbitrary: level31's street loop wraps
/// anyone whose **x** leaves `[LOOP_WEST_X, LOOP_EAST_X]` back into the block
/// (`wrap_into_the_block`), so a ghost house three kilometres east was dragged home the
/// instant a player arrived in it -- 450 metres at a time, seven times in one step. Z is
/// not wrapped and the fall-out rule only fires inside the stage's own footprint, so a
/// house due north of everything is simply somewhere the street's rules do not reach.
const GHOST_T: Vector3 = Vector3 { x: 0.0, y: 0.746, z: 3000.0 };

/// The ghost house's model-to-world map: no turn, so its front door faces +z as authored.
fn ghost_to_world(p: Vector3) -> Vector3 {
    Vector3::new(GHOST_T.x + p.x, GHOST_T.y + p.y, GHOST_T.z + p.z)
}

/// Model-space fittings inside a house (the pack's own metres; `Lot::to_world` places them).
/// The lounge's south wall is model z = -4.40 (`prochouse`'s LOUNGE rect); the mouth stands
/// its own stand-off proud of it, or the player's head sphere could never reach the plane.
const LOUNGE_SOUTH_WALL: f32 = -4.40;
const SCALE_MOUTH_MODEL: Vector3 =
    Vector3 { x: 12.1, y: prochouse::FLOOR_Y, z: LOUNGE_SOUTH_WALL + warphouse::STAND_OFF };
/// Where the shrunk player steps out, in the living room, walking into it.
const SCALE_OUT_MODEL: Vector3 = Vector3 { x: 12.1, y: prochouse::FLOOR_Y, z: 0.3 };
/// The pocket in the back of the big bedroom, and its twin in the ghost house's lounge.
const POCKET_MODEL: Vector3 = Vector3 { x: 13.2, y: prochouse::FLOOR_Y, z: 6.0 };
const GHOST_LOUNGE_MODEL: Vector3 = Vector3 { x: 12.1, y: prochouse::FLOOR_Y, z: -3.5 };
const GHOST_BED_MODEL: Vector3 = Vector3 { x: 13.2, y: prochouse::FLOOR_Y, z: 6.0 };

/// The three walls of the near house a player may flatten onto, as (decal centre, station
/// eye), in house model space. Three pegged walls per arena is the design doc's v0 budget.
const STATIONS_MODEL: [(Vector3, Vector3, f32); 3] = [
    // Lounge, east wall: stand back across the room and the smear resolves.
    (Vector3 { x: 15.20, y: 1.30, z: -2.40 }, Vector3 { x: 12.80, y: 1.59, z: -2.40 }, GH_PI / 2.0),
    // Living room, on the wall it shares with the big bedroom -- EAST of that bedroom's
    // doorway, which is the opening at model x[11.30, 12.50] (`prochouse`'s CLEAR[4]). A
    // decal hung in the middle of that wall hangs in the doorway itself, and the eye station
    // that went with it stood inside the partition.
    (Vector3 { x: 13.90, y: 1.30, z: 2.20 }, Vector3 { x: 13.90, y: 1.59, z: 0.55 }, 0.0),
    // Big bedroom, west wall.
    (Vector3 { x: 11.30, y: 1.30, z: 4.60 }, Vector3 { x: 13.70, y: 1.59, z: 4.60 }, -GH_PI / 2.0),
];

/// Which catalog categories a player may wear. Big enough to stand behind, common enough
/// that one more of them in a room is not obviously a lie.
fn is_wearable(part: &str) -> bool {
    part.starts_with("armchair")
        || part.starts_with("bed")
        || part.starts_with("chest")
        || part.starts_with("wardrobe")
        || part.starts_with("bookcase")
        || part.starts_with("fitment")
        || part.starts_with("table")
}

/// The prompt a wearable offers. One string per category keeps them inside the HUD's
/// 40-character budget without formatting at runtime.
fn wear_hint(part: &str) -> (&'static str, &'static str) {
    if part.starts_with("armchair") {
        ("ARMCHAIR", "E  BECOME THE ARMCHAIR")
    } else if part.starts_with("bed") {
        ("BED", "E  BECOME THE BED")
    } else if part.starts_with("chest") {
        ("CHEST", "E  BECOME THE CHEST")
    } else if part.starts_with("wardrobe") {
        ("WARDROBE", "E  BECOME THE WARDROBE")
    } else if part.starts_with("bookcase") || part.starts_with("fitment") {
        ("BOOKCASE", "E  BECOME THE BOOKCASE")
    } else {
        ("TABLE", "E  BECOME THE TABLE")
    }
}

/// The sleepwalkers' rounds, in world coordinates: closed routes they walk for ever. Kept
/// near the spawn and the near houses, because a patrol nobody meets is a patrol nobody
/// believes in. `y` is a guess -- every step re-finds the ground by raycast.
const ROUTES: [&[(f32, f32)]; 5] = [
    // The north verge, up and down past the spawn.
    &[(-26.0, -11.6), (26.0, -11.6), (26.0, -6.0), (-26.0, -6.0)],
    // The road itself, a longer beat.
    &[(-46.0, -20.0), (44.0, -20.0), (44.0, -24.0), (-46.0, -24.0)],
    // The south lawn, in front of the house across from the spawn.
    &[(-20.0, -40.0), (2.0, -40.0), (2.0, -31.0), (-20.0, -31.0)],
    // Up the near house's front path and back down it.
    &[(-9.5, -44.0), (-9.5, -30.0), (-16.0, -34.0)],
    // The far north verge, so the street is never quite empty.
    &[(-70.0, -11.6), (-30.0, -11.6), (-30.0, -4.0), (-70.0, -4.0)],
];

/// Houses beyond this stop drawing entirely. Level31's fog curve (1 - exp(-(d/45)^2)) is
/// 63% at 45 m and only crosses 99% near 97 m -- a gate at 60 popped houses in and out in
/// plain sight. 97 puts the pop under one part in a hundred of sky.
const HOUSE_DRAW_RANGE: f32 = 97.0;

/// Stage model gz spans of the two shell rows, padded 0.4 m.
const ROW_GZ: [[f32; 2]; 2] = [[-141.6, -122.0], [-74.7, -55.1]]; // [north, south]
/// Cut band, model y: over the lawn (-0.48), over the rooflines (6.0).
const CUT_MODEL_Y: [f32; 2] = [-0.46, 7.5];

/// Stage model -> world, as level31's placement() does it: a half turn about y, then this
/// translation (asserted against placement() in the tests).
const STAGE_T: Vector3 = Vector3 { x: -42.5, y: 1.29, z: -119.78 };

fn stage_to_world(m: Vector3) -> Vector3 {
    Vector3::new(-m.x + STAGE_T.x, m.y + STAGE_T.y, -m.z + STAGE_T.z)
}

impl Lot {
    /// The world cut box that removes this lot's shell.
    fn cut(&self) -> (Vector3, Vector3) {
        let gz = ROW_GZ[usize::from(!self.north)];
        let a = stage_to_world(Vector3::new(self.model_x[0], CUT_MODEL_Y[0], gz[0]));
        let b = stage_to_world(Vector3::new(self.model_x[1], CUT_MODEL_Y[1], gz[1]));
        (
            Vector3::new(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z)),
            Vector3::new(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z)),
        )
    }

    /// World x centre of the lot.
    fn centre_x(&self) -> f32 {
        -(self.model_x[0] + self.model_x[1]) / 2.0 + STAGE_T.x
    }

    /// The procedural house's rigid placement on this lot: model -> world is a half turn
    /// for the north row (the pack's door faces its own +z; a north house faces the street
    /// at world -z) and none for the south, plus this translation. The translation stands
    /// the pack's interior floor (model 0.094) at 0.84 -- 3 cm over the lawn, a door sill
    /// -- and puts the front door a porch from where the shell's front stood.
    fn house_yaw(&self) -> f32 {
        if self.north {
            GH_PI
        } else {
            0.0
        }
    }

    fn house_t(&self) -> Vector3 {
        // The door sits at model (7.94, z 7.58), 1.7 m west of the house's centre; the
        // baked lawns keep each lot's front path, which runs to the LOT centre -- so the
        // door, not the house, is what lines up with the centre, and every path still
        // leads to a front door.
        if self.north {
            // Door lands at world (centre, z = t.z - 7.58 = 2.0), a stride onto the path.
            Vector3::new(self.centre_x() + 7.94, 0.746, 9.58)
        } else {
            // Door lands at world (centre, z = t.z + 7.58 = -47.0), facing the road.
            Vector3::new(self.centre_x() - 7.94, 0.746, -54.58)
        }
    }

    fn to_world(&self, p: Vector3) -> Vector3 {
        let t = self.house_t();
        if self.north {
            Vector3::new(t.x - p.x, t.y + p.y, t.z - p.z)
        } else {
            Vector3::new(t.x + p.x, t.y + p.y, t.z + p.z)
        }
    }
}

// ---------------------------------------------------------------------------------------
// The walker (unchanged from the scene's Mannequin Test days, plus one line: it reports
// itself as the presence the tools read).
// ---------------------------------------------------------------------------------------

const MODEL: &str = "Meshes/mannequin.glb";
/// The one mesh in the file (`tools/build_mannequin.py` names it).
const PART: &str = "Mannequin";

/// Clip names are the slugs of the files downloaded from Mixamo.
const CLIP_WALK: &str = "walking";

/// Mixamo's walk covers this much ground a second, so the clip is paced to the speed
/// rather than the other way round.
const WALK_CYCLE_SPEED: f32 = 1.15;
const SPEED: f32 = 1.2;

/// The two ends of the stroll, on the verge between the houses and the road.
///
/// The `y` here is only a starting guess: this strip is not the flat lawn the spawn stands
/// on, it slopes toward the kerb (measured, 0.32 down to 0.24 across these thirty metres),
/// so the walker finds the floor by raycast every step and this value only has to be close
/// enough to start inside the world.
const GROUND_GUESS: f32 = 0.3;
const FROM: Vector3 = Vector3 { x: -14.0, y: GROUND_GUESS, z: -11.6 };
const TO: Vector3 = Vector3 { x: 16.0, y: GROUND_GUESS, z: -11.6 };
/// How far above itself the walker looks for the floor, and how far down.
const GROUND_PROBE_UP: f32 = 2.0;
const GROUND_PROBE_DOWN: f32 = 6.0;

/// How fast it turns at the end of a leg, radians a second.
const TURN_RATE: f32 = 3.0;
/// Close enough to the end of a leg to turn around.
const ARRIVE: f32 = 0.15;

/// A mannequin that walks a line, turns, and walks back.
struct Walker {
    base: Object,
    inst: Instance,
    /// The model's feet sit at its own origin, so this stays 0 -- kept because it is what
    /// any other character asset would need.
    lift: f32,
    target: Vector3,
    want_yaw: f32,
}

impl Walker {
    fn new(gl: &Rc<glow::Context>, res: &Resources) -> Walker {
        let model = SkinnedModel::acquire(gl, MODEL);
        let mut inst = Instance::new(gl, res, &model, Some(PART));
        inst.set_clip(CLIP_WALK, SPEED / WALK_CYCLE_SPEED);
        let b = inst.bounds();
        let mut base = Object::new();
        base.pos = FROM;
        let want_yaw = (TO - FROM).x.atan2((TO - FROM).z);
        base.euler.y = want_yaw;
        Walker { base, inst, lift: -b[2], target: TO, want_yaw }
    }
}

impl ObjectT for Walker {
    fn base(&self) -> &Object {
        &self.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base
    }

    fn update(&mut self, ctx: &UpdateCtx) {
        let d = self.target - self.base.pos;
        let flat = Vector3::new(d.x, 0.0, d.z);
        let dist = flat.mag();
        if dist <= ARRIVE {
            self.target = if self.target.x > 0.0 { FROM } else { TO };
            let n = self.target - self.base.pos;
            self.want_yaw = n.x.atan2(n.z);
        } else {
            self.base.pos += flat * (SPEED * GH_DT / dist);
        }
        // Stand on whatever is actually underfoot. The verge slopes, and a character
        // floating a hand's width over it is the first thing anyone notices; the scenery's
        // triangle mesh is the only honest answer to "how high is the ground here".
        let eye = self.base.pos + Vector3::new(0.0, GROUND_PROBE_UP, 0.0);
        let down = Vector3::new(0.0, -1.0, 0.0);
        if let Some(hit) =
            raycast::raycast(ctx.scene, eye, down, GROUND_PROBE_UP + GROUND_PROBE_DOWN, None)
        {
            self.base.pos.y = hit.point.y;
        }
        // It IS the presence every instrument in the house reads.
        tool::report_presence(self.base.pos);
        // Ease round rather than snapping: a mannequin that pivots on the spot reads as a
        // bug even when the walk itself is right.
        let mut diff = self.want_yaw - self.base.euler.y;
        while diff > GH_PI {
            diff -= 2.0 * GH_PI;
        }
        while diff < -GH_PI {
            diff += 2.0 * GH_PI;
        }
        self.base.euler.y += diff.signum() * (TURN_RATE * GH_DT).min(diff.abs());
        self.inst.update();
    }

    fn draw(&self, ctx: &RenderCtx, cam: &Camera, _fbo: Option<glow::Framebuffer>) {
        let mut obj = Object::new();
        obj.pos = self.base.pos + Vector3::new(0.0, self.lift, 0.0);
        // No turn to correct: the mannequin's geometry faces glTF +z (measured from its own
        // toes), and `euler.y = atan2(dx, dz)` already points a model's +z along the way it
        // is walking.
        obj.euler.y = self.base.euler.y;
        self.inst.draw(&obj, ctx, cam);
    }

    /// Scenery does not push it and the portals do not warp it: it drives itself along a
    /// line and nothing else may touch it.
    fn engine_collision(&self) -> bool {
        false
    }
    fn static_collision(&self) -> bool {
        false
    }
}

impl Scene for Level32 {
    fn load(
        &self,
        gl: &Rc<glow::Context>,
        res: &Resources,
        objs: &mut PObjectVec,
        portals: &mut PPortalVec,
        player: &mut Player,
    ) {
        // The stage, exactly as Liminal Neighborhood dresses it (level31.rs explains every
        // one of these choices) -- except for the cut where its baked house was.
        crate::ext::audio::set_surface(crate::ext::audio::Surface::Grass);
        crate::ext::audio::set_portal_audible(false);
        crate::ext::view::set_seamless_portals(true);
        crate::ext::view::set_fog(level31::FOG);
        tool::reset();
        disguise::reset();
        npc::reset();
        let cuts: Vec<(Vector3, Vector3)> = LOTS.iter().map(Lot::cut).collect();
        let openings = Openings { cut: &cuts, also_inside: &[] };
        let spec = level31::load_spec();
        let floor = interior::Floor { walk: level31::FLOOR_Y, cap: level31::CAP_Y };
        interior::load(
            gl,
            res,
            objs,
            player,
            &spec,
            &level31::placement(),
            openings,
            floor,
            level31::spawn(),
        );
        // ---- the arena, and where the round starts ---------------------------------------
        // The one house with content is lot 12, straight across the road; the neighborhood's
        // own spawn stands the player 48 m away from it, in fog thick enough to hide its front
        // door. The round is played in that house, so it starts on its porch, facing in.
        let arena = &LOTS[LOT_FACING_SPAWN];
        let porch = arena.to_world(Vector3::new(7.80, prochouse::FLOOR_Y, 9.60));
        let door_in = arena.to_world(Vector3::new(7.80, prochouse::FLOOR_Y, 6.00)) - porch;
        let spawn = room::Respawn::facing(
            porch + Vector3::new(0.0, crate::game_header::GH_PLAYER_HEIGHT, 0.0),
            Vector3::new(door_in.x, 0.0, door_in.z),
        );
        player.base.set_position(spawn.pos);
        player.set_look(spawn.yaw, 0.0);

        // And a moon, which level31 has and this scene had simply never pushed: without one
        // the sky is a black rectangle and the street reads as a bug rather than a night.
        objs.push(Rc::new(RefCell::new(Moon::new(gl, res))) as Rc<RefCell<dyn ObjectT>>);

        let west = Rc::new(RefCell::new(level31::cut(res, level31::LOOP_WEST_X)));
        let east = Rc::new(RefCell::new(level31::cut(res, level31::LOOP_EAST_X)));
        portals.push(west.clone());
        portals.push(east.clone());
        connect(&west, &east);
        objs.push(Rc::new(RefCell::new(RoomLogic::new(level31::wrap_into_the_block)))
            as Rc<RefCell<dyn ObjectT>>);

        // The street of the night: every shell lot carved (the CUTS const fed to
        // interior::load above) gets its own furnishing of the pack's house, and eight of
        // the seventeen get one instrument of the exploration kit each -- dealt by the
        // master seed, except the flashlight, which is always straight across the road.
        let master = prochouse::scene_seed();
        log::info!("[house] street seed {master} (pin it with --house-seed)");
        let hspec = prochouse::load_spec();
        let mut deal = prochouse::Rng::new(master ^ 0xD00F_5EED);
        let mut lots_order: Vec<usize> = (0..LOTS.len()).collect();
        for i in (1..lots_order.len()).rev() {
            lots_order.swap(i, deal.below_pub(i + 1));
        }
        let mut dealt: Vec<Vec<crate::ext::tool::ToolKind>> = vec![Vec::new(); LOTS.len()];
        for (kind, lot) in crate::ext::tool::ToolKind::ALL.iter().zip(&lots_order) {
            dealt[*lot].push(*kind);
        }
        // The flashlight goes to the house facing the spawn, whoever drew it -- by
        // SWAPPING the two lots' hands, so the deal stays one instrument per house
        // (retain-and-push left one house with two and the flashlight's old house empty).
        let fl = crate::ext::tool::ToolKind::Flashlight;
        if !dealt[LOT_FACING_SPAWN].contains(&fl) {
            let from =
                dealt.iter().position(|l| l.contains(&fl)).expect("the flashlight is always dealt");
            dealt.swap(from, LOT_FACING_SPAWN);
        }
        let mut pieces = 0;
        let mut tools = 0;
        let mut wearables: Vec<Wearable> = Vec::new();
        for (i, lot) in LOTS.iter().enumerate() {
            let seed = master ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
            let mut props = PackProps::new(gl, res, &hspec);
            props.set_draw_range(HOUSE_DRAW_RANGE);
            let yaw = lot.house_yaw();
            for part in ["house_shell", "kitchen_fit", "fridge_fit", "dining_fit"] {
                let anchor = props.anchor(part);
                let pos = lot.to_world(anchor);
                props.add(part, pos, yaw, true);
            }
            let plan = prochouse::generate(seed, &|p| props.size(p), &dealt[i]);
            if plan.door_open {
                let pos = lot.to_world(Vector3::new(
                    prochouse::OPEN_DOOR_POS[0],
                    prochouse::FLOOR_Y,
                    prochouse::OPEN_DOOR_POS[1],
                ));
                props.add("house_door", pos, yaw + GH_PI / 2.0, true);
            }
            for pl in &plan.places {
                props.add(pl.part, lot.to_world(pl.pos), pl.yaw + yaw, pl.collide);
                // Whatever the seed stood in this house is what a player may become in it:
                // the disguise catalog is the furnishing, so a worn chair is always a chair
                // that belongs in the room it is standing in.
                if pl.collide && is_wearable(pl.part) {
                    let (label, hint) = wear_hint(pl.part);
                    wearables.push(Wearable {
                        part: pl.part,
                        label,
                        hint,
                        // Offered at the height a person would look at it, not at its feet.
                        pos: lot.to_world(pl.pos) + Vector3::new(0.0, 0.6, 0.0),
                    });
                }
            }
            props.seal();
            pieces += plan.places.len();
            objs.push(Rc::new(RefCell::new(props)) as Rc<RefCell<dyn ObjectT>>);
            for (kind, pos, tyaw) in &plan.tools {
                let t = tool::Tool::new(gl, res, *kind);
                let w = lot.to_world(*pos) + Vector3::new(0.0, 0.12, 0.0);
                log::info!("[house] left the {kind:?} at ({:.1}, {:.1}, {:.1})", w.x, w.y, w.z);
                t.borrow_mut().place(w, *tyaw);
                objs.push(t as Rc<RefCell<dyn ObjectT>>);
                tools += 1;
            }
        }
        log::info!(
            "[house] street furnished: {} lots, {pieces} pieces, {tools} instruments",
            LOTS.len()
        );

        // ---- the house that is nowhere ------------------------------------------------
        // Furnished like any other, standing three kilometres off the street, reachable
        // only through the two pockets. Its own seed, so it is not a copy of a neighbour.
        {
            let mut ghost = PackProps::new(gl, res, &hspec);
            ghost.set_draw_range(HOUSE_DRAW_RANGE);
            let yaw = 0.0;
            for part in ["house_shell", "kitchen_fit", "fridge_fit", "dining_fit"] {
                let pos = ghost_to_world(ghost.anchor(part));
                ghost.add(part, pos, yaw, true);
            }
            let plan = prochouse::generate(master ^ 0x6047_5748, &|p| ghost.size(p), &[]);
            for pl in &plan.places {
                ghost.add(pl.part, ghost_to_world(pl.pos), pl.yaw + yaw, pl.collide);
                if pl.collide && is_wearable(pl.part) {
                    let (label, hint) = wear_hint(pl.part);
                    wearables.push(Wearable {
                        part: pl.part,
                        label,
                        hint,
                        pos: ghost_to_world(pl.pos) + Vector3::new(0.0, 0.6, 0.0),
                    });
                }
            }
            // Grass under it too: the ghost house has windows, and a house standing on
            // nothing at all reads as a bug rather than as a secret.
            ghost.add("lot_ground", ghost_to_world(Vector3::new(9.6, -0.06, 1.4)), 0.0, true);
            ghost.seal();
            objs.push(Rc::new(RefCell::new(ghost)) as Rc<RefCell<dyn ObjectT>>);

            // The ghost stands outside the stage's fence AND outside the footprint its
            // fall-out rule watches, so it carries its own. Without this, stepping off its
            // lot -- through the front door the pack ships open -- drops you for ever: no
            // floor beyond the lot, no fence to stop you, and the stage's respawn declines
            // to fire because you are nowhere near the stage.
            let floor = GHOST_T.y + prochouse::FLOOR_Y;
            let home = room::Respawn::facing(
                ghost_to_world(Vector3::new(9.6, prochouse::FLOOR_Y, 1.4))
                    + Vector3::new(0.0, crate::game_header::GH_PLAYER_HEIGHT, 0.0),
                Vector3::new(0.0, 0.0, 1.0),
            );
            let lo = ghost_to_world(Vector3::new(-12.0, 0.0, -14.0));
            let hi = ghost_to_world(Vector3::new(31.0, 0.0, 15.0));
            objs.push(Rc::new(RefCell::new(RoomLogic::new(move |ctx| {
                if crate::ext::backrooms::fell_out(
                    ctx.player_pos,
                    ctx.player_p_scale,
                    floor,
                    (lo, hi),
                ) {
                    room::request_respawn(home);
                }
            }))) as Rc<RefCell<dyn ObjectT>>);
        }

        // ---- the fittings ---------------------------------------------------------------
        // The scale mouth: an opening in the near house's lounge that halves whoever walks
        // into it, so the ordinary living room they step out into is twice the room it was.
        let scale = &LOTS[SCALE_LOT];
        warphouse::scale_mouth(
            res,
            portals,
            Mouth::new(scale.to_world(SCALE_MOUTH_MODEL), scale.house_yaw(), warphouse::MOUTH_BIG),
            scale.to_world(SCALE_OUT_MODEL),
            scale.house_yaw() + GH_PI,
        );

        // Two pockets, one at each end of the block, both opening into the ghost house.
        let pocket_a = &LOTS[POCKET_LOT];
        warphouse::pocket(
            res,
            portals,
            pocket_a.to_world(POCKET_MODEL),
            pocket_a.house_yaw(),
            ghost_to_world(GHOST_LOUNGE_MODEL),
            0.0,
        );
        let pocket_b = &LOTS[POCKET_LOT_B];
        warphouse::pocket(
            res,
            portals,
            ghost_to_world(GHOST_BED_MODEL),
            0.0,
            pocket_b.to_world(GHOST_LOUNGE_MODEL),
            pocket_b.house_yaw(),
        );
        debug_assert!(
            portals.len() - warphouse::LOOP_PORTALS <= warphouse::PORTAL_BUDGET,
            "the street is over the portal budget: {} fittings of {}",
            portals.len() - warphouse::LOOP_PORTALS,
            warphouse::PORTAL_BUDGET
        );
        log::info!("[house] {} portals standing", portals.len());

        // ---- the hide -------------------------------------------------------------------
        let stations: Vec<Station> = STATIONS_MODEL
            .iter()
            .map(|(centre, view, yaw)| Station {
                centre: scale.to_world(*centre),
                view: scale.to_world(*view),
                yaw: yaw + scale.house_yaw(),
            })
            .collect();
        log::info!(
            "[house] {} things to become, {} walls to flatten onto",
            wearables.len(),
            stations.len()
        );
        if let Some(w) = wearables.iter().min_by(|a, b| {
            let d = |p: Vector3| (p - Vector3::new(-9.5, 0.84, -50.0)).mag();
            d(a.pos).total_cmp(&d(b.pos))
        }) {
            log::debug!(
                "[house] nearest wearable to the near house: {} at ({:.2}, {:.2}, {:.2})",
                w.label,
                w.pos.x,
                w.pos.y,
                w.pos.z
            );
        }
        objs.push(Rc::new(RefCell::new(WearOffers::new(wearables))) as Rc<RefCell<dyn ObjectT>>);
        objs.push(
            Rc::new(RefCell::new(Disguise::new(gl, res, stations))) as Rc<RefCell<dyn ObjectT>>
        );

        // ---- the sleepwalkers -------------------------------------------------------------
        // Three keep the street alive; they never enter the house and never hunt.
        for (i, route) in ROUTES.iter().take(3).enumerate() {
            let waypoints: Vec<Vector3> =
                route.iter().map(|(x, z)| Vector3::new(*x, level31::LAWN_Y, *z)).collect();
            // The phase staggers their sight tests against each other, so the street costs
            // one raycast every few steps rather than one per sleepwalker per step.
            let npc = Npc::new(gl, res, waypoints, (i as u32) * 7);
            objs.push(Rc::new(RefCell::new(npc)) as Rc<RefCell<dyn ObjectT>>);
        }

        // ---- the torch on the porch ---------------------------------------------------------
        // The house's own flashlight is inside it somewhere, dealt with the rest of the kit;
        // this is a second one, at the player's feet at the start of every round. The first
        // five seconds should be "pick up the torch and get inside", not "grope in the dark
        // for the thing that lets you see".
        {
            let t = tool::Tool::new(gl, res, tool::ToolKind::Flashlight);
            t.borrow_mut()
                .place(arena.to_world(Vector3::new(7.80, prochouse::FLOOR_Y + 0.06, 8.55)), 0.0);
            objs.push(t as Rc<RefCell<dyn ObjectT>>);
        }

        // ---- the two that hunt (`ext/hunt.rs`) ---------------------------------------------
        // Held by handle, because the round re-routes them between phases: a porch loop while
        // the player hides, the house sweep once the hunt is on.
        let to_house = |pts: &[(f32, f32)], from: usize| -> Vec<Vector3> {
            (0..pts.len())
                .map(|k| {
                    let (x, z) = pts[(from + k) % pts.len()];
                    arena.to_world(Vector3::new(x, prochouse::FLOOR_Y, z))
                })
                .collect()
        };
        let posts = vec![to_house(&POST_A, 0), to_house(&POST_B, 0)];
        let sweeps = vec![to_house(&SWEEP, 0), to_house(&SWEEP, 3)];
        let mut seekers = Vec::new();
        for (i, post) in posts.iter().enumerate() {
            let npc = Rc::new(RefCell::new(Npc::new(gl, res, post.clone(), 3 + (i as u32) * 11)));
            seekers.push(Rc::clone(&npc));
            objs.push(npc as Rc<RefCell<dyn ObjectT>>);
        }
        log::info!("[house] 3 sleepwalkers on the street, 2 seekers on the porch");

        // ---- the round --------------------------------------------------------------------
        // It is also the one reader of the attention channel (it needs to know whether a
        // seeker has the player in sight to write its own line), so nothing fights over it.
        let lo = arena.to_world(Vector3::new(3.0, -1.0, -6.5));
        let hi = arena.to_world(Vector3::new(16.5, 4.0, 11.5));
        objs.push(Rc::new(RefCell::new(hunt::round_logic(hunt::Setup {
            seekers,
            posts,
            sweeps,
            start: spawn,
            house_lo: Vector3::new(lo.x.min(hi.x), lo.y.min(hi.y), lo.z.min(hi.z)),
            house_hi: Vector3::new(lo.x.max(hi.x), lo.y.max(hi.y), lo.z.max(hi.z)),
        }))) as Rc<RefCell<dyn ObjectT>>);

        objs.push(Rc::new(RefCell::new(Walker::new(gl, res))) as Rc<RefCell<dyn ObjectT>>);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ext::scenes;

    #[test]
    fn registered_and_not_a_floor_of_the_elevator() {
        scenes::index_of("Open House").expect("Open House is registered");
        assert!(
            !crate::ext::elevator::FLOORS.iter().any(|f| f.scene_name == "Open House"),
            "reached from SWITCH LEVEL, not the elevator"
        );
    }

    /// `stage_to_world` must BE level31's placement: same half turn, same translation.
    #[test]
    fn the_stage_map_is_level31s_placement() {
        let placement = level31::placement();
        for m in
            [Vector3::zero(), Vector3::new(-42.5, -0.48, -119.78), Vector3::new(59.0, 2.0, -131.0)]
        {
            let via_obj = placement.local_to_world().mul_point(m);
            let via_map = stage_to_world(m);
            assert!((via_obj - via_map).mag() < 1e-3, "{via_obj:?} vs {via_map:?}");
        }
    }

    /// Every lot's carve: floor over the lawn (which continues underneath, measured),
    /// box inside the street loop, and NOWHERE near the real house's lot or the spawn.
    #[test]
    fn the_carves_take_the_shells_and_leave_everything_else() {
        // The real house spans model x[-57.6,-21.3] on the north row.
        for (i, lot) in LOTS.iter().enumerate() {
            let (lo, hi) = lot.cut();
            assert!(lo.y > level31::LAWN_Y, "lot {i}: the cut would take the lawn with it");
            assert!(lo.y < 0.90, "lot {i}: a cut floor this high leaves wall stubs");
            assert!(
                lo.x > level31::LOOP_WEST_X + 10.0 && hi.x < level31::LOOP_EAST_X - 10.0,
                "lot {i} leaves the loop"
            );
            if lot.north {
                assert!(
                    lot.model_x[1] <= -57.6 - 1.0 || lot.model_x[0] >= -21.3 + 1.0,
                    "lot {i} overlaps the real house"
                );
            }
            // The spawn and its lawn stay: feet at (0, 0.81, 0).
            assert!(
                !(lo.x < 0.0 && hi.x > 0.0 && lo.z < 0.0 && hi.z > 0.0),
                "lot {i} carves the spawn"
            );
        }
    }

    /// Every procedural house lands inside its own carved lot, floor a sill over the
    /// lawn, door facing the road.
    #[test]
    fn every_house_stands_inside_its_lot_facing_the_road() {
        // House model footprint, measured in Blender: x[3.68,15.59], z[-4.74,7.58].
        for (i, lot) in LOTS.iter().enumerate() {
            let (lo, hi) = lot.cut();
            for (mx, mz) in [(3.68, -4.74), (15.59, -4.74), (3.68, 7.58), (15.59, 7.58)] {
                let w = lot.to_world(Vector3::new(mx, 0.0, mz));
                assert!(
                    w.x > lo.x - 0.5 && w.x < hi.x + 0.5,
                    "lot {i}: house corner {w:?} leaves the lot x"
                );
                assert!(
                    w.z > lo.z - 1.5 && w.z < hi.z + 1.5,
                    "lot {i}: house corner {w:?} leaves the lot z"
                );
            }
            let door = lot.to_world(Vector3::new(7.94, 0.0, 7.58));
            let centre = lot.to_world(Vector3::new(9.63, 0.0, 1.42));
            // The road runs between the rows (world z about -8..-15): the door side of
            // the house must be the road side.
            let road_z = -11.6_f32;
            assert!(
                (door.z - road_z).abs() < (centre.z - road_z).abs(),
                "lot {i}: the front door faces away from the road"
            );
            let floor = lot.house_t().y + prochouse::FLOOR_Y;
            assert!(floor > level31::LAWN_Y && floor - level31::LAWN_Y < 0.08);
        }
    }

    /// The walker's stroll stays on the verge, clear of both carved rows.
    #[test]
    fn the_stroll_stays_on_the_verge_and_off_the_lots() {
        for p in [FROM, TO] {
            assert!(p.x > level31::LOOP_WEST_X + 60.0 && p.x < level31::LOOP_EAST_X - 60.0);
            assert!(p.y > level31::FLOOR_Y && p.y < level31::LAWN_Y);
            for (i, lot) in LOTS.iter().enumerate() {
                let (lo, hi) = lot.cut();
                assert!(p.z < lo.z || p.z > hi.z, "lot {i}: the stroll crosses a carved lot");
            }
        }
        assert!((TO - FROM).mag() > 5.0, "far enough apart to see a walk cycle");
    }

    /// The ghost house has to be somewhere the street's own rules cannot reach -- and the
    /// rule that matters is the loop wrap, which looks at X ONLY. A house moved out along
    /// x was hauled back onto the street 450 m at a time the moment a player arrived in it;
    /// this test is that bug.
    #[test]
    fn the_ghost_house_stands_where_the_street_rules_cannot_reach() {
        // Inside the wrap's x band, so `wrap_into_the_block` never fires for it...
        let corners = [
            ghost_to_world(Vector3::new(3.68, 0.0, -4.74)),
            ghost_to_world(Vector3::new(15.59, 0.0, 7.58)),
        ];
        for c in corners {
            assert!(
                c.x > level31::LOOP_WEST_X && c.x < level31::LOOP_EAST_X,
                "the ghost house would be wrapped back onto the street"
            );
        }
        // ...and far enough out in z that nothing on the street can walk, see or fall into it.
        for c in corners {
            assert!(c.z > 1000.0, "the ghost house is close enough to stumble into");
        }
        for lot in LOTS.iter() {
            let near = lot.to_world(Vector3::new(9.6, 0.0, 1.4));
            assert!(
                (near - ghost_to_world(Vector3::new(9.6, 0.0, 1.4))).mag() > 1000.0,
                "the ghost house is standing on the street"
            );
        }
    }

    /// Walking into the scale mouth must actually change the player's size, and the far end
    /// must be somewhere they can stand: the lounge mouth and the living-room exit are in
    /// different rooms of the same house.
    #[test]
    fn the_scale_mouth_halves_you_and_puts_you_somewhere_else() {
        assert!((warphouse::SCALE_RATIO - 0.5).abs() < 1e-6);
        let lot = &LOTS[SCALE_LOT];
        let mouth = lot.to_world(SCALE_MOUTH_MODEL);
        let out = lot.to_world(SCALE_OUT_MODEL);
        assert!((mouth - out).mag() > 3.0, "the two ends are the same spot");
        // Both inside the house's own footprint, model x[3.68,15.59], z[-4.74,7.58].
        for m in [SCALE_MOUTH_MODEL, SCALE_OUT_MODEL] {
            assert!(m.x > 3.68 && m.x < 15.59, "mouth outside the house");
            assert!(m.z > -4.74 && m.z < 7.58, "mouth outside the house");
        }
        // The mouth stands proud of its wall by exactly the stand-off.
        assert!((SCALE_MOUTH_MODEL.z - LOUNGE_SOUTH_WALL - warphouse::STAND_OFF).abs() < 1e-6);
    }

    /// Both pockets open into the ghost house and nowhere else, and they are its only two
    /// ways in or out -- the design doc's rule for a hidden room.
    #[test]
    fn the_ghost_house_has_exactly_two_ways_in() {
        let here = LOTS[POCKET_LOT].to_world(POCKET_MODEL);
        let there = ghost_to_world(GHOST_LOUNGE_MODEL);
        let back = ghost_to_world(GHOST_BED_MODEL);
        let out = LOTS[POCKET_LOT_B].to_world(GHOST_LOUNGE_MODEL);
        assert!((here - there).mag() > 1000.0, "the pocket does not leave the street");
        assert!((back - out).mag() > 1000.0, "the second pocket does not leave the ghost");
        // The two ghost-side mouths are in different rooms, so one is watchable from the
        // other's room and neither is a trap.
        assert!((there - back).mag() > 3.0, "both exits open into the same corner");
        assert_ne!(POCKET_LOT, POCKET_LOT_B, "both pockets land on the same lot");
    }

    /// The flashlight's house is the one the spawn looks straight at.
    #[test]
    fn the_flashlight_house_faces_the_spawn() {
        let lot = &LOTS[LOT_FACING_SPAWN];
        assert!(!lot.north, "the facing lot is across the road, on the south row");
        assert!(
            lot.model_x[0] < -42.5 && -42.5 < lot.model_x[1],
            "the facing lot does not straddle the spawn's model x (-42.5)"
        );
    }

    #[test]
    fn the_model_carries_the_clip_the_walk_asks_for() {
        let path = crate::app::assets::path(MODEL);
        let Ok(bytes) = std::fs::read(path) else { return };
        let gltf = gltf::Gltf::from_slice(&bytes).unwrap();
        let names: Vec<_> =
            gltf.document.animations().filter_map(|a| a.name().map(String::from)).collect();
        assert!(names.iter().any(|n| n == CLIP_WALK), "no {CLIP_WALK:?} in {}", names.len());
        assert_eq!(gltf.document.skins().count(), 1, "one skeleton");
    }

    /// Both pack GLBs carry every part the specs name -- when the files are present (the
    /// build tools write them; a source checkout without assets skips this).
    #[test]
    fn the_packs_carry_every_named_node() {
        for (file, roots) in [
            (
                tool::MODEL,
                crate::ext::tool::ToolKind::ALL.iter().map(|k| k.part()).collect::<Vec<_>>(),
            ),
            (
                prochouse::MODEL,
                vec!["House_Demo_040", "House_Demo_039", "Lot_Ground", "Fridge_Shelf", "bed_02"],
            ),
        ] {
            let path = crate::app::assets::path(file);
            let Ok(bytes) = std::fs::read(path) else { continue };
            let gltf = gltf::Gltf::from_slice(&bytes).unwrap();
            let names: Vec<String> =
                gltf.document.nodes().filter_map(|n| n.name().map(String::from)).collect();
            for want in roots {
                assert!(names.iter().any(|n| n == want), "{file} lacks node {want:?}");
            }
        }
    }
}
