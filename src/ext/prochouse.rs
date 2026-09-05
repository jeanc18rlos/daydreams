//! EXT: the procedural house -- a seeded furnishing of Elbolillo's village demo house. Not
//! part of the C++ port.
//!
//! The village pack ships one complete PSX house: an L of rooms behind a south porch --
//! kitchen wing to the west with its counters, fridge and laid dining table authored in
//! place, a lounge and a living room in the east wing, two bedrooms behind them, a paved
//! patio in the northwest notch. What it does not ship is furniture IN those rooms: the
//! showroom's ninety-odd pieces stand in rows outside. This module is the estate agent: from
//! one seed it draws up a [`Plan`] -- which bed in which bedroom against which wall, whose
//! sofa faces the lounge window, where the paintings hang, what got left on the tables, and
//! which of the exploration tools were forgotten in which room -- so that every visit to the
//! scene furnishes the same shell differently.
//!
//! # Geometry
//!
//! Everything here works in the HOUSE'S OWN model space (glTF: y up, the front door facing
//! +z), in the pack's authored metres; the scene places the finished plan into the world
//! with one rigid transform. The room rectangles and doorway-clearance zones below were
//! measured off the shell's wall quads in Blender (wall band 0.4..2.0), not guessed; the
//! walls are double-faced quads about 0.17 m apart and the rectangles hug their inner faces.
//!
//! # The algorithm, honestly
//!
//! Recipes per room, rejection sampling per piece: a recipe asks for "a bed against a wall,
//! a chest, maybe a wardrobe"; each pick takes a random catalog entry of the category that
//! FITS (a two-metre bed does not enter the small bedroom), a wall side or a free spot, and
//! up to a dozen placement attempts against everything placed so far plus the doorway
//! zones. What cannot be placed after that is dropped -- an under-furnished room is a
//! result, an armchair wedged into a doorway is a bug. Clutter stacks on furniture tops
//! (the chest knows its own height); paintings hang on the walls a recipe names; ceiling
//! pieces hang from the slab. All of it is deterministic in the seed.

use crate::ext::gltf_model::{Fit, Load};
use crate::ext::tool::ToolKind;
use crate::ext::village_catalog::{Cat, CATALOG, PARTS};
use crate::vector::Vector3;
use std::cell::Cell;
use std::f32::consts::PI;

thread_local! {
    /// `--house-seed N`: pin the night's seed. Set once at startup from the command line.
    static CLI_SEED: Cell<Option<u64>> = const { Cell::new(None) };
}

pub fn set_cli_seed(seed: Option<u64>) {
    CLI_SEED.with(|c| c.set(seed));
}

/// The seed a fresh scene load furnishes with: the pinned one, or the clock's -- a new
/// house every visit, and the log prints the number so a good one can be pinned.
pub fn scene_seed() -> u64 {
    CLI_SEED.with(Cell::get).unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64 ^ d.as_secs())
            .unwrap_or(1)
    })
}

pub const MODEL: &str = "Meshes/village_objects.glb";

pub fn load_spec() -> Load<'static> {
    Load {
        path: MODEL,
        parts: &PARTS,
        fit: Fit::Identity,
        max_map: 512,
        translucent: &[],
        metallic_override: &[],
        cut_boxes: &[],
    }
}

/// The interior floor of the shell, model y: measured by raycast (0.094) and used for every
/// foot the plan places. The patio slab is a step lower.
pub const FLOOR_Y: f32 = 0.094;
pub const PATIO_Y: f32 = -0.067;
/// Underside of the ceiling slab the walls carry (wall quads end at 2.64).
pub const CEIL_Y: f32 = 2.64;

/// An axis-aligned floor rectangle, model space: `[x0, x1, z0, z1]`.
pub type Rect = [f32; 4];

/// Which wall of a room a piece stands against or hangs on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Side {
    North, // z0: away from the front door
    South, // z1: toward it
    West,  // x0
    East,  // x1
}

/// A room of the shell and what may go where in it.
struct Room {
    rect: Rect,
    floor: f32,
    /// Walls a recipe may hang art on -- the interior faces, so a painting never covers a
    /// window from inside.
    art: &'static [Side],
}

const LOUNGE: Room =
    Room { rect: [8.95, 15.25, -4.40, -0.45], floor: FLOOR_Y, art: &[Side::South] };
const LIVING: Room =
    Room { rect: [8.95, 15.25, 0.05, 2.25], floor: FLOOR_Y, art: &[Side::North, Side::South] };
const BED_SMALL: Room =
    Room { rect: [9.05, 10.70, 2.80, 6.45], floor: FLOOR_Y, art: &[Side::North] };
const BED_BIG: Room =
    Room { rect: [11.25, 15.25, 2.80, 6.45], floor: FLOOR_Y, art: &[Side::North, Side::West] };
const PATIO: Room = Room { rect: [3.85, 8.45, -4.55, -2.00], floor: PATIO_Y, art: &[] };
/// The kitchen wing's free floor, east of the authored counters and north of the laid
/// table: enough for junk and a chair, nothing bigger.
const KITCHEN_SPARE: Room =
    Room { rect: [4.10, 6.60, 1.60, 3.10], floor: FLOOR_Y, art: &[Side::East] };

/// Doorway clearances: no plan may stand furniture in these. The corridor from the front
/// door north past the kitchen is one long zone -- it is the only way in.
const CLEAR: [Rect; 7] = [
    [6.90, 8.75, 2.30, 7.60],  // porch + corridor, front door to the living room
    [8.50, 9.30, -0.20, 2.50], // spine opening, corridor/kitchen into the living room
    [9.10, 10.40, -0.85, 0.35], // lounge doorway
    [8.80, 10.20, 2.20, 3.20], // small bedroom doorway
    [11.30, 12.50, 2.20, 3.20], // big bedroom doorway
    [14.70, 15.60, 0.45, 1.75], // back door, east wall of the living room
    [6.90, 8.10, -2.20, -1.30], // patio doorway out of the kitchen
];

/// Floor the SCENE's non-Euclidean fittings stand on, and the spots a player must be able
/// to stand to use them (`level32`'s scale mouth, pocket mouths and station eye points).
/// The pack's shell is identical in every house, so these are the same model-space spots in
/// all of them, and reserving them everywhere costs no more than a sofa choosing a
/// different wall.
///
/// They are kept here, beside [`CLEAR`], rather than passed in, because this is the only
/// module that knows where furniture may not stand -- and a mouth with a sofa in it is not
/// a mouth. The lounge recipe places its sofa against the z0 wall FIRST, into an empty
/// room, so without this every house put a three-seater exactly where the scale mouth is.
pub const FITTINGS: [Rect; 6] = [
    [11.30, 12.90, -4.60, -3.20], // the scale mouth, lounge z0 wall
    [11.30, 12.90, -0.40, 1.00],  // where a shrunk player steps out, in the living room
    // The bedroom pocket. Kept off the big bedroom's north wall: its bed stands there and
    // a 2.57 m one reaches z = 5.39, so a reservation starting any earlier than this leaves
    // the room with nowhere to put a bed at all.
    [12.30, 14.10, 5.70, 6.60],
    [11.20, 13.00, -4.30, -2.70], // the ghost house's lounge pocket
    [12.00, 13.60, -3.00, -1.80], // station 1's eye point, lounge
    [11.30, 12.90, -0.80, 0.40],  // station 2's eye point, living room
];

/// Whether a footprint clashes with a doorway clearance or a fitting.
fn reserved(fp: Rect, margin: f32) -> bool {
    CLEAR.iter().chain(FITTINGS.iter()).any(|c| overlaps(fp, *c, margin))
}

/// Where the front door's leaf stands when the plan leaves it open: swung inward against
/// the porch's west side, hinge staying at the frame's west jamb.
pub const OPEN_DOOR_POS: [f32; 2] = [7.53, 6.99];

/// One piece of the plan: a catalog part stood at `pos` (its foot centre), turned `yaw`.
#[derive(Clone, Debug)]
pub struct Place {
    pub part: &'static str,
    pub pos: Vector3,
    pub yaw: f32,
    pub collide: bool,
}

/// A furnished house: the fixed shell pieces are the scene's to add; this is everything the
/// seed decided, still in house model space.
#[derive(Clone, Debug, Default)]
pub struct Plan {
    pub places: Vec<Place>,
    /// Front door open (standing at [`OPEN_DOOR_POS`]) or gone altogether.
    pub door_open: bool,
    /// The tools someone left behind: kind, foot position, yaw.
    pub tools: Vec<(ToolKind, Vector3, f32)>,
}

/// xorshift64* -- the engine carries no RNG crate and a house does not need one.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.max(1))
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn f32(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }
    /// Public: the street deal in level32 shuffles lot indices with it.
    pub fn below_pub(&mut self, n: usize) -> usize {
        self.below(n)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn chance(&mut self, p: f32) -> bool {
        self.f32() < p
    }
}

/// Model-space size of a part, `(dx, dy, dz)`: the scene passes `PackProps::size`; tests
/// pass what they please.
pub type Dims<'a> = dyn Fn(&str) -> Vector3 + 'a;

fn overlaps(a: Rect, b: Rect, margin: f32) -> bool {
    a[0] - margin < b[1] && b[0] - margin < a[1] && a[2] - margin < b[3] && b[2] - margin < a[3]
}

fn inside(outer: Rect, inner: Rect) -> bool {
    inner[0] >= outer[0] && inner[1] <= outer[1] && inner[2] >= outer[2] && inner[3] <= outer[3]
}

/// The footprint of `size` stood at `pos` under a quarter-turn `yaw`.
fn footprint(pos: Vector3, size: Vector3, yaw: f32) -> Rect {
    let quarter = ((yaw / (PI / 2.0)).round() as i32).rem_euclid(2) == 1;
    let (dx, dz) = if quarter { (size.z, size.x) } else { (size.x, size.z) };
    [pos.x - dx / 2.0, pos.x + dx / 2.0, pos.z - dz / 2.0, pos.z + dz / 2.0]
}

/// Furniture front faces -z in the pack; this yaw turns it to face into the room off `side`.
fn face_room(side: Side) -> f32 {
    match side {
        Side::North => PI,
        Side::South => 0.0,
        Side::West => -PI / 2.0,
        Side::East => PI / 2.0,
    }
}

struct Planner<'a> {
    rng: Rng,
    dims: &'a Dims<'a>,
    occupied: Vec<Rect>,
    plan: Plan,
    /// Tops a tool or a gizmo may rest on: footprint, surface height.
    tops: Vec<(Rect, f32)>,
    /// Tops already carrying a tool: one forgotten instrument per surface reads as a
    /// story, two reads as a shelf display.
    used_tops: Vec<usize>,
}

impl<'a> Planner<'a> {
    /// Pieces of `cat` no wider than the room can take, shuffled order courtesy of `skip`.
    fn pick(&mut self, cat: Cat, fits: impl Fn(Vector3) -> bool) -> Option<&'static str> {
        let all: Vec<&'static str> =
            CATALOG.iter().filter(|e| e.cat == cat && e.part != "stove").map(|e| e.part).collect();
        if all.is_empty() {
            return None;
        }
        let start = self.rng.below(all.len());
        for i in 0..all.len() {
            let part = all[(start + i) % all.len()];
            if fits((self.dims)(part)) {
                return Some(part);
            }
        }
        None
    }

    /// Stand one `cat` piece in `room`, against `side` if given, else free. Up to `tries`
    /// throws; the winner joins the plan and the occupancy map.
    fn stand(
        &mut self,
        room: &Room,
        cat: Cat,
        side: Option<Side>,
        collide: bool,
        tries: usize,
    ) -> Option<usize> {
        let [x0, x1, z0, z1] = room.rect;
        let room_w = x1 - x0;
        let room_d = z1 - z0;
        let part = self.pick(cat, |s| {
            s.x.min(s.z) < room_w.min(room_d) - 0.2 && s.x.max(s.z) < room_w.max(room_d) - 0.2
        })?;
        let size = (self.dims)(part);
        for _ in 0..tries {
            let (yaw, pos) = match side {
                Some(s) => {
                    let yaw = face_room(s);
                    let fp = footprint(Vector3::zero(), size, yaw);
                    let (hw, hd) = (fp[1], fp[3]);
                    let pos = match s {
                        Side::North => Vector3::new(
                            self.rng.range(x0 + hw, x1 - hw),
                            room.floor,
                            z0 + hd + 0.02,
                        ),
                        Side::South => Vector3::new(
                            self.rng.range(x0 + hw, x1 - hw),
                            room.floor,
                            z1 - hd - 0.02,
                        ),
                        Side::West => Vector3::new(
                            x0 + hw + 0.02,
                            room.floor,
                            self.rng.range(z0 + hd, z1 - hd),
                        ),
                        Side::East => Vector3::new(
                            x1 - hw - 0.02,
                            room.floor,
                            self.rng.range(z0 + hd, z1 - hd),
                        ),
                    };
                    (yaw, pos)
                }
                None => {
                    let yaw = face_room(match self.rng.below(4) {
                        0 => Side::North,
                        1 => Side::South,
                        2 => Side::West,
                        _ => Side::East,
                    });
                    let fp = footprint(Vector3::zero(), size, yaw);
                    let (hw, hd) = (fp[1], fp[3]);
                    if x1 - hw <= x0 + hw || z1 - hd <= z0 + hd {
                        continue;
                    }
                    let pos = Vector3::new(
                        self.rng.range(x0 + hw, x1 - hw),
                        room.floor,
                        self.rng.range(z0 + hd, z1 - hd),
                    );
                    (yaw, pos)
                }
            };
            let fp = footprint(pos, size, yaw);
            if !inside(room.rect, fp) {
                continue;
            }
            // A free-standing solid piece keeps a player's width of clearance around it:
            // the doorway zones only guard the doorways' mouths, and a table wedged just
            // past one could still seal the room off.
            let margin = if side.is_none() && collide { 0.55 } else { 0.08 };
            if reserved(fp, margin - 0.08) {
                continue;
            }
            if self.occupied.iter().any(|o| overlaps(fp, *o, margin)) {
                continue;
            }
            self.occupied.push(fp);
            self.tops.push((fp, pos.y + size.y));
            self.plan.places.push(Place { part, pos, yaw, collide });
            // The handle is into `tops`: what `rest_on` stacks onto. `places` also grows
            // through hang/overhead/rest_on, so its indices are nobody's handles.
            return Some(self.tops.len() - 1);
        }
        // A dropped piece is a result; the debug line is for whoever tunes the recipes.
        log::debug!("[plan] no spot for {cat:?} ({part})");
        None
    }

    /// Rest a small `cat` piece on the top of the plan's piece `on`. A piece whose top
    /// already carries authored dressing (the showroom's laid tables cluster to well over a
    /// metre of bounds) takes nothing more -- stacking on the bounds top would float a milk
    /// carton over the plates.
    fn rest_on(&mut self, on: usize, cat: Cat) {
        let (fp, top_y) = self.tops[on];
        if top_y - FLOOR_Y > 1.15 {
            return;
        }
        // The clutter's turn is drawn FIRST so the fit can use its yawed extents against
        // BOTH of the top's -- a deep pot on a shallow sideboard overhangs otherwise.
        let yaw = face_room(match self.rng.below(4) {
            0 => Side::North,
            1 => Side::South,
            2 => Side::West,
            _ => Side::East,
        });
        let quarter = ((yaw / (PI / 2.0)).round() as i32).rem_euclid(2) == 1;
        let (tw, td) = (fp[1] - fp[0], fp[3] - fp[2]);
        let Some(part) = self.pick(cat, |s| {
            let (dx, dz) = if quarter { (s.z, s.x) } else { (s.x, s.z) };
            dx < tw && dz < td
        }) else {
            return;
        };
        let size = (self.dims)(part);
        let (dx, dz) = if quarter { (size.z, size.x) } else { (size.x, size.z) };
        let cx = (fp[0] + fp[1]) / 2.0;
        let cz = (fp[2] + fp[3]) / 2.0;
        let jx = (tw / 2.0 - dx / 2.0 - 0.02).max(0.0);
        let jz = (td / 2.0 - dz / 2.0 - 0.02).max(0.0);
        let pos = Vector3::new(cx + self.rng.range(-jx, jx), top_y, cz + self.rng.range(-jz, jz));
        self.plan.places.push(Place { part, pos, yaw, collide: false });
    }

    /// Hang one painting on an art wall of `room`, 1.15 m up. Doorways live in walls:
    /// each throw is tested against the clearance zones, or the small bedroom's one
    /// painting always floated in its own doorway.
    fn hang(&mut self, room: &Room) {
        let Some(&side) = room.art.get(self.rng.below(room.art.len().max(1))) else {
            return;
        };
        let Some(part) = self.pick(Cat::Painting, |_| true) else { return };
        let size = (self.dims)(part);
        let [x0, x1, z0, z1] = room.rect;
        let yaw = face_room(side);
        let depth = size.z / 2.0 + 0.02;
        let half = size.x / 2.0 + 0.05;
        for _ in 0..10 {
            let pos = match side {
                Side::North | Side::South => {
                    if x1 - half <= x0 + half {
                        return;
                    }
                    let x = self.rng.range(x0 + half, x1 - half);
                    let z = if side == Side::North { z0 + depth } else { z1 - depth };
                    Vector3::new(x, 1.15, z)
                }
                Side::West | Side::East => {
                    if z1 - half <= z0 + half {
                        return;
                    }
                    let z = self.rng.range(z0 + half, z1 - half);
                    let x = if side == Side::West { x0 + depth } else { x1 - depth };
                    Vector3::new(x, 1.15, z)
                }
            };
            let fp = footprint(pos, Vector3::new(size.x, size.y, size.x), yaw);
            if reserved(fp, 0.0) {
                continue;
            }
            self.plan.places.push(Place { part, pos, yaw, collide: false });
            return;
        }
    }

    /// Hang something from the ceiling over the middle of `room`.
    fn overhead(&mut self, room: &Room, cat: Cat) {
        let Some(part) = self.pick(cat, |_| true) else { return };
        let size = (self.dims)(part);
        let [x0, x1, z0, z1] = room.rect;
        let pos = Vector3::new((x0 + x1) / 2.0, CEIL_Y - size.y - 0.02, (z0 + z1) / 2.0);
        self.plan.places.push(Place { part, pos, yaw: face_room(Side::South), collide: false });
    }

    /// A tool someone left: on a surface the plan already made -- each surface at most
    /// once, jittered off its centre -- or failing that on a clear patch of floor, never
    /// inside a piece of furniture or a doorway.
    fn leave_tool(&mut self, kind: ToolKind, room: &Room) {
        // Only surfaces in the dealt room, at heights a thing gets put down on: the deal
        // said which room, and a patio chair is not where anyone leaves a thermal camera.
        let candidates: Vec<usize> = (0..self.tops.len())
            .filter(|ix| !self.used_tops.contains(ix))
            .filter(|ix| {
                let (fp, top_y) = self.tops[*ix];
                inside(room.rect, fp) && (0.25..1.2).contains(&(top_y - room.floor))
            })
            .collect();
        if self.rng.chance(0.7) && !candidates.is_empty() {
            {
                let ix = candidates[self.rng.below(candidates.len())];
                self.used_tops.push(ix);
                let (fp, top_y) = self.tops[ix];
                let jx = ((fp[1] - fp[0]) / 2.0 - 0.15).max(0.0);
                let jz = ((fp[3] - fp[2]) / 2.0 - 0.15).max(0.0);
                let pos = Vector3::new(
                    (fp[0] + fp[1]) / 2.0 + self.rng.range(-jx, jx),
                    top_y + 0.02,
                    (fp[2] + fp[3]) / 2.0 + self.rng.range(-jz, jz),
                );
                self.plan.tools.push((kind, pos, self.rng.range(0.0, PI * 2.0)));
                return;
            }
        }
        let [x0, x1, z0, z1] = room.rect;
        for _ in 0..12 {
            let pos = Vector3::new(
                self.rng.range(x0 + 0.3, x1 - 0.3),
                room.floor + 0.1,
                self.rng.range(z0 + 0.3, z1 - 0.3),
            );
            let fp = [pos.x - 0.15, pos.x + 0.15, pos.z - 0.15, pos.z + 0.15];
            if self.occupied.iter().any(|o| overlaps(fp, *o, 0.05)) || reserved(fp, 0.0) {
                continue;
            }
            self.plan.tools.push((kind, pos, self.rng.range(0.0, PI * 2.0)));
            return;
        }
        // Twelve misses in a furnished room: leave it in the doorway after all -- a tool
        // that does not spawn is worse than one underfoot.
        let pos = Vector3::new((x0 + x1) / 2.0, room.floor + 0.1, (z0 + z1) / 2.0);
        self.plan.tools.push((kind, pos, 0.0));
    }
}

/// Draw up one house from one seed. `dims` answers a part's model-space size --
/// `PackProps::size` in the scene, anything plausible in a test. `kinds` is the share of
/// the exploration kit forgotten in THIS house: the street scatters one instrument per
/// house rather than a whole kit bag in each.
pub fn generate(seed: u64, dims: &Dims, kinds: &[ToolKind]) -> Plan {
    let mut p = Planner {
        rng: Rng::new(seed),
        dims,
        occupied: Vec::new(),
        plan: Plan::default(),
        tops: Vec::new(),
        used_tops: Vec::new(),
    };

    // The lounge: where the sofa lives.
    if let Some(sofa) = p.stand(&LOUNGE, Cat::Sofa, Some(Side::North), true, 14) {
        let _ = sofa;
    }
    p.stand(&LOUNGE, Cat::Armchair, Some(Side::West), true, 10);
    if p.rng.chance(0.7) {
        p.stand(&LOUNGE, Cat::Armchair, Some(Side::East), true, 10);
    }
    if let Some(t) = p.stand(&LOUNGE, Cat::Table, None, true, 14) {
        if p.rng.chance(0.8) {
            p.rest_on(t, Cat::Food);
        }
        if p.rng.chance(0.6) {
            p.rest_on(t, Cat::Gizmo);
        }
    }
    if let Some(c) = p.stand(&LOUNGE, Cat::Sideboard, Some(Side::South), true, 10) {
        p.rest_on(c, Cat::Gizmo);
        if p.rng.chance(0.7) {
            p.rest_on(c, Cat::TableLamp);
        }
    }
    if p.rng.chance(0.5) {
        p.stand(&LOUNGE, Cat::FloorFan, None, true, 8);
    }
    p.hang(&LOUNGE);
    p.hang(&LOUNGE);
    let lounge_fan = p.rng.chance(0.5);
    p.overhead(&LOUNGE, if lounge_fan { Cat::CeilingFan } else { Cat::CeilingLight });

    // The living room, between the lounge and the bedrooms.
    if let Some(c) = p.stand(&LIVING, Cat::Chest, Some(Side::North), true, 10) {
        p.rest_on(c, Cat::Gizmo);
        if p.rng.chance(0.5) {
            p.rest_on(c, Cat::TableLamp);
        }
    }
    p.stand(&LIVING, Cat::Armchair, Some(Side::South), true, 10);
    if p.rng.chance(0.6) {
        p.stand(&LIVING, Cat::Bookcase, Some(Side::North), true, 10);
    }
    if p.rng.chance(0.6) {
        p.stand(&LIVING, Cat::Plant, None, false, 8);
    }
    p.hang(&LIVING);
    p.overhead(&LIVING, Cat::CeilingLight);

    // The bedrooms: a bed each, whatever else fits. The small room's north wall is its
    // own doorway wall -- a bed there always clipped the clearance zone and was dropped,
    // so its bed backs onto the south wall instead.
    for (room, bed_side, extras) in
        [(&BED_SMALL, Side::South, false), (&BED_BIG, Side::North, true)]
    {
        p.stand(room, Cat::Bed, Some(bed_side), true, 16);
        if let Some(c) = p.stand(room, Cat::Chest, Some(Side::East), true, 10) {
            p.rest_on(c, Cat::Gizmo);
            if p.rng.chance(0.5) {
                p.rest_on(c, Cat::TableLamp);
            }
        }
        if extras {
            if p.rng.chance(0.8) {
                p.stand(room, Cat::Wardrobe, Some(Side::West), true, 10);
            }
            if p.rng.chance(0.5) {
                p.stand(room, Cat::Bookcase, Some(Side::West), true, 8);
            }
            let fan = p.rng.chance(0.4);
            p.overhead(room, if fan { Cat::CeilingFan } else { Cat::CeilingLight });
        } else {
            p.overhead(room, Cat::CeilingLight);
        }
        p.hang(room);
        if p.rng.chance(0.4) {
            p.stand(room, Cat::Junk, None, true, 6);
        }
    }

    // The patio: garden chairs and what the rain gets.
    p.stand(&PATIO, Cat::Chair, None, true, 10);
    if p.rng.chance(0.7) {
        p.stand(&PATIO, Cat::Chair, None, true, 10);
    }
    if p.rng.chance(0.6) {
        p.stand(&PATIO, Cat::Plant, None, false, 8);
    }
    if p.rng.chance(0.5) {
        p.stand(&PATIO, Cat::Junk, None, true, 8);
    }

    // The kitchen's spare floor: junk gravitates to kitchens.
    p.stand(&KITCHEN_SPARE, Cat::Junk, None, true, 8);
    if p.rng.chance(0.4) {
        p.stand(&KITCHEN_SPARE, Cat::Chair, None, true, 8);
    }

    // The front door: mostly left open, sometimes gone altogether.
    p.plan.door_open = p.rng.chance(0.65);

    // Whatever share of the kit this house was dealt, left where the furniture is.
    for kind in kinds {
        let room = match p.rng.below(4) {
            0 => &LOUNGE,
            1 => &LIVING,
            2 => &BED_BIG,
            _ => &BED_SMALL,
        };
        p.leave_tool(*kind, room);
    }

    p.plan
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plausible sizes by name, no GL: what the catalog's pieces roughly are.
    fn dims(part: &str) -> Vector3 {
        let s = |x: f32, y: f32, z: f32| Vector3::new(x, y, z);
        if part.starts_with("bed") {
            s(1.5, 1.2, 2.5)
        } else if part.starts_with("armchair") {
            s(1.0, 1.0, 0.9)
        } else if part.starts_with("wardrobe") || part.starts_with("bookcase") {
            s(1.5, 1.9, 0.5)
        } else if part.starts_with("chest") {
            s(1.1, 1.0, 0.7)
        } else if part.starts_with("table") {
            s(1.6, 0.9, 1.3)
        } else if part.starts_with("painting") {
            s(0.95, 0.95, 0.06)
        } else {
            s(0.4, 0.3, 0.4)
        }
    }

    #[test]
    fn the_same_seed_furnishes_the_same_house() {
        let a = generate(77, &dims, &[ToolKind::Flashlight, ToolKind::Emf]);
        let b = generate(77, &dims, &[ToolKind::Flashlight, ToolKind::Emf]);
        assert_eq!(a.places.len(), b.places.len());
        for (x, y) in a.places.iter().zip(&b.places) {
            assert_eq!(x.part, y.part);
            assert!((x.pos - y.pos).mag() < 1e-6);
        }
        assert_eq!(a.tools.len(), b.tools.len());
    }

    #[test]
    fn different_seeds_furnish_differently() {
        let a = generate(1, &dims, &[]);
        let b = generate(2, &dims, &[]);
        let same = a.places.len() == b.places.len()
            && a.places
                .iter()
                .zip(&b.places)
                .all(|(x, y)| x.part == y.part && (x.pos - y.pos).mag() < 1e-6);
        assert!(!same, "two seeds drew the identical plan");
    }

    /// Nothing may stand where the scene's portals do. The lounge recipe places its sofa
    /// against the z0 wall first, into an empty room -- so before the fittings were
    /// reserved, EVERY house put a three-seater exactly where the scale mouth stands, and
    /// walking into the mouth meant walking into a sofa.
    #[test]
    fn nothing_stands_in_a_fitting() {
        for seed in [1, 7, 42, 1234, 99999, 5, 8] {
            let plan = generate(seed, &dims, &[ToolKind::Flashlight]);
            for pl in &plan.places {
                if !pl.collide {
                    continue;
                }
                let fp = footprint(pl.pos, dims(pl.part), pl.yaw);
                for f in FITTINGS {
                    assert!(
                        !overlaps(fp, f, -0.01),
                        "seed {seed}: {} stands in a fitting at {:?}",
                        pl.part,
                        pl.pos
                    );
                }
            }
            // And the tools someone left behind are not in them either.
            for (kind, pos, _) in &plan.tools {
                let fp = [pos.x - 0.12, pos.x + 0.12, pos.z - 0.12, pos.z + 0.12];
                for f in FITTINGS {
                    assert!(
                        !overlaps(fp, f, -0.01),
                        "seed {seed}: the {kind:?} was left in a portal mouth"
                    );
                }
            }
        }
    }

    #[test]
    fn nothing_stands_in_a_doorway() {
        for seed in [1, 7, 42, 1234, 99999] {
            let plan = generate(seed, &dims, &[ToolKind::SpiritBox]);
            for pl in &plan.places {
                if !pl.collide {
                    continue;
                }
                let fp = footprint(pl.pos, dims(pl.part), pl.yaw);
                for c in CLEAR {
                    assert!(
                        !overlaps(fp, c, -0.01),
                        "seed {seed}: {} stands in a doorway at {:?}",
                        pl.part,
                        pl.pos
                    );
                }
            }
        }
    }

    #[test]
    fn colliding_furniture_never_interpenetrates() {
        for seed in [3, 11, 555] {
            let plan = generate(seed, &dims, &[]);
            let solid: Vec<_> = plan.places.iter().filter(|p| p.collide).collect();
            for (i, a) in solid.iter().enumerate() {
                for b in &solid[i + 1..] {
                    let fa = footprint(a.pos, dims(a.part), a.yaw);
                    let fb = footprint(b.pos, dims(b.part), b.yaw);
                    assert!(
                        !overlaps(fa, fb, -0.02),
                        "seed {seed}: {} and {} share floor",
                        a.part,
                        b.part
                    );
                }
            }
        }
    }

    #[test]
    fn every_house_has_beds_and_leaves_what_it_was_dealt() {
        for seed in [5, 8, 13, 21, 34] {
            let plan = generate(seed, &dims, &[ToolKind::Flashlight]);
            let beds = plan.places.iter().filter(|p| p.part.starts_with("bed")).count();
            assert!(beds >= 1, "seed {seed}: a house with no bed at all");
            assert_eq!(plan.tools.len(), 1, "seed {seed}: the dealt tool was not left");
            assert_eq!(plan.tools[0].0, ToolKind::Flashlight);
            let none = generate(seed, &dims, &[]);
            assert!(none.tools.is_empty(), "seed {seed}: tools nobody dealt");
        }
    }

    #[test]
    fn the_seed_is_never_zero_locked() {
        // xorshift dies at state 0; the constructor must dodge it.
        let mut r = Rng::new(0);
        assert_ne!(r.next(), 0);
    }
}
