//! EXT: the elevator -- the hub between the game's interiors. Not part of the C++ port.
//!
//! EFX's "Elevator with Animation LOWPOLY" (`Meshes/elevator_with_animation_lowpoly.glb`,
//! CC-BY-4.0, see `THIRD_PARTY.md`): a cabin with a wallpapered wall slab around its doorway,
//! a call button and a floor display outside, and two telescoping door leaves that the file's
//! one clip, `Doors open`, slides open and shut. A level stands one of these in a wall and it
//! becomes the way to every other level that has one: step in, press E, the doors close, the
//! screen goes dark, and the doors open again somewhere else.
//!
//! # Floors
//!
//! [`FLOORS`] is the list of levels an elevator stops at, in riding order, by registry name.
//! A floor whose scene is not registered is skipped (with one warning in the log), so the
//! list can name a level before it exists. From any floor the ride goes to the next one in the
//! list that resolves, wrapping round; with only one floor registered the hint says so and E
//! does nothing.
//!
//! # The ride
//!
//! `Elevator::update` drives a small state machine ([`Ride`], GL-free and tested with a fake
//! clock): `Idle` (doors open) -> E inside the cabin -> `Closing` (the leaves slide shut over
//! [`CLOSE_SECS`]) -> `Fading` (a black overlay comes up over [`FADE_SECS`]) -> the scene load
//! is requested through `ext::room::request_scene_load`, which the engine applies after the
//! fixed-step loop, never mid-step -> the destination level takes the [`Arrival`] with
//! [`take_arrival`], builds its elevator with it (doors shut, screen black) and stands the
//! player inside with [`Elevator::board`] -> `Arriving` (the black fades over [`FADE_SECS`],
//! then the doors open over [`OPEN_SECS`]) -> `Idle`.
//!
//! The doors are drawn as their own parts, slid by the clip's `node_delta` at a clip time
//! proportional to the ride's openness -- `openness * T_OPEN`, where `T_OPEN` is the moment of
//! widest opening, found once at load by sampling the clip -- so closing is the opening curve
//! played backwards. Collision is two triangle meshes: the cabin (floor, sill, walls,
//! ceiling and the wall slab) always, and the door leaves at rest, on a helper object
//! ([`ElevatorDoors`]) that only offers them while the doors are less than half open -- shut
//! doors block the doorway, open ones are inside the wall and block nothing.
//!
//! # Set into a wall
//!
//! The cabin is meant to stand BEHIND the wall of whatever room it serves, its slab a few
//! centimetres proud of that wall and its doorway cut through it. The host carves the box
//! [`Elevator::wall_cut`] out of its model as the glTF loader parses it (`Load::cut_boxes`,
//! `ext/carve.rs`): the wall's triangles behind the doorway are gone from what is drawn and
//! from what collides, so through the open leaves the player sees the cabin and nothing of
//! the host, and walks in. The slab, a margin wider than the cut on every side, covers the
//! cut's edges. A level therefore builds its elevator BEFORE the model it is set into, so the
//! box is known when the model is loaded (`Backrooms::new`, `interior::load`).
//!
//! # Channels
//!
//! An object in the scene vector cannot reach the engine's input, its HUD or its audio, and
//! nothing survives a scene load but the engine. So, as with `ext::view` and `ext::room`, the
//! handful of values that cross those lines are ambient: the fade level the overlay draws
//! ([`fade`]), the E press the engine hands over ([`wants_interact`], [`press`]), the
//! ride-start edge for the sound ([`take_ride_started`]), the arrival the next level picks up
//! ([`take_arrival`]) and the registry index of the scene being played, which is how an
//! elevator knows which floor it is on ([`on_scene_loaded`]). The hint line the HUD shows goes
//! through the shared channel every prompt uses (`ext::hint`), set each step the offer stands.
//!
//! The channels are written by the one elevator's `update`, last writer wins, which is to say
//! **one elevator per scene**: a second would erase the first's hint and fade every step it
//! ran after it, and take the E press the first was offered. Every level has one; a level
//! that wanted two would give the channels a merge rule first.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::camera::Camera;
use crate::ext::bounds::transformed_box;
use crate::ext::gltf_model::{Anchor, Fit, Frame, GltfModel, Load, PartSpec};
use crate::ext::hint;
use crate::ext::room::{request_scene_load, Respawn};
use crate::ext::scenes;
use crate::ext::trimesh::TriMeshCollider;
use crate::game_header::{GH_DT, GH_PLAYER_HEIGHT};
use crate::object::{Object, ObjectT, RenderCtx, UpdateCtx};
use crate::player::Player;
use crate::resources::Resources;
use crate::shader::Shader;
use crate::vector::{Matrix4, Vector3};

/// A level the elevator stops at.
pub struct Floor {
    /// The scene's name in the registry (`scenes::SCENES`), which is how it is found.
    pub scene_name: &'static str,
    /// What the HUD calls it: "E  RIDE TO <label>".
    pub label: &'static str,
}

/// Every floor, in riding order. Floors whose scene is not registered are skipped at runtime.
pub const FLOORS: &[Floor] = &[
    Floor { scene_name: "Backrooms", label: "BACKROOMS" },
    Floor { scene_name: "Pool Rooms", label: "POOL ROOMS" },
    Floor { scene_name: "Overgrown", label: "OVERGROWN" },
];

/// What a level that has just been loaded by a ride learns from [`take_arrival`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Arrival {
    /// Index into [`FLOORS`] of the floor the ride started on.
    pub from_floor: usize,
}

/// Seconds for the doors to close at the start of a ride.
pub const CLOSE_SECS: f32 = 1.5;
/// Seconds for the screen to go black once the doors are shut, and to come back on arrival.
pub const FADE_SECS: f32 = 0.5;
/// Seconds for the doors to open on arrival.
pub const OPEN_SECS: f32 = 1.5;

/// Openness below which the shut door leaves collide. Half: the leaves are clear of the
/// opening well before they are fully retracted, and a player cannot be standing in the gap
/// at half travel anyway -- the ride only starts with them inside the cabin.
const DOORS_BLOCK_BELOW: f32 = 0.5;

const MODEL: &str = "Meshes/elevator_with_animation_lowpoly.glb";
/// The file's maps are at most 1024 square; nothing is resampled.
const MAP: u32 = 1024;
const CLIP: &str = "Doors open";
/// Part names: everything that does not move, and the two leaves.
const CABIN: &str = "cabin";
const DOOR1: &str = "door1";
const DOOR2: &str = "door2";
/// The nodes the clip moves, which are also the roots the leaf parts are gathered from.
const NODE_DOOR1: &str = "Door1";
const NODE_DOOR2: &str = "Door2";
/// Interval the clip is sampled at to find its widest opening: 10 ms over a 10 s clip.
const SAMPLE_STEP: f32 = 0.01;

// ── The model, measured. In its own space: metres, Y up, the doorway facing +z. The test
// `cabin_is_where_the_constants_say` reads them back from the GLB, so a re-export cannot
// move the floor out from under the constants.
/// Top of the cabin floor.
const FLOOR_Y: f32 = -0.022;
/// The floor's footprint, and head room above it: where a player counts as inside.
const CABIN_LO: Vector3 = Vector3 { x: -2.87, y: FLOOR_Y, z: -1.37 };
const CABIN_HI: Vector3 = Vector3 { x: 1.14, y: FLOOR_Y + 3.0, z: 0.95 };
/// The doorway's clear opening: the faces of its two jambs. The shut leaves overlap the west
/// jamb by 14 cm, so they span more than this.
pub const OPENING_X: (f32, f32) = (-0.797, 0.979);
/// The inner face of the leaves: a player past this z is in the doorway, not the cabin.
const DOOR_PLANE_Z: f32 = 0.98;
/// The outer face of the wall slab the doorway is set in, and the slab's x extent -- what
/// a level has to find room for in its wall.
const WALL_FACE_Z: f32 = 1.18;
pub const SLAB_X: (f32, f32) = (-3.02, 1.21);
/// The point `Elevator::new` puts at `pos`: the centre of the opening, at floor level, on
/// the slab's outer face. A level reasons about where the doorway meets its wall; the cabin
/// behind it follows.
pub const THRESHOLD: Vector3 =
    Vector3 { x: 0.5 * (OPENING_X.0 + OPENING_X.1), y: FLOOR_Y, z: WALL_FACE_Z };
/// How far the slab's face should stand proud of the host wall it is set into
/// (`ELEVATOR_SPOT` in each level is the wall's face plus this, along the facing): enough
/// that the two are never coplanar, less than the call button stands out from the slab.
pub const PROUD: f32 = 0.03;
/// The wall cut (`wall_cut`): how far under the cabin's floor and past its back it reaches,
/// so the host's floor cannot z-fight the cabin's and a host wall a hand behind the cabin's
/// back is gone too.
const CUT_MARGIN: f32 = 0.1;

const PARTS: [PartSpec<'static>; 3] = [
    PartSpec {
        name: CABIN,
        roots: &[],
        skip: &[NODE_DOOR1, NODE_DOOR2],
        frame: Frame::Scene,
        // Irrelevant under Fit::Identity.
        anchor: Anchor::Hinge,
    },
    PartSpec {
        name: DOOR1,
        roots: &[NODE_DOOR1],
        skip: &[],
        frame: Frame::Scene,
        anchor: Anchor::Hinge,
    },
    PartSpec {
        name: DOOR2,
        roots: &[NODE_DOOR2],
        skip: &[],
        frame: Frame::Scene,
        anchor: Anchor::Hinge,
    },
];

fn load_spec() -> Load<'static> {
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

// ─────────────────────────────────────────────────────────────────────────────
// The ambient channels (see the module docs).
// ─────────────────────────────────────────────────────────────────────────────

thread_local! {
    static FADE: Cell<f32> = const { Cell::new(0.0) };
    /// Whether the player stands in an idle cabin, as of the last `update`: the elevator is
    /// offering the E press, and the hint line says what it would do.
    static OFFERING: Cell<bool> = const { Cell::new(false) };
    static PRESSED: Cell<bool> = const { Cell::new(false) };
    static RIDE_STARTED: Cell<bool> = const { Cell::new(false) };
    static ARRIVAL: Cell<Option<Arrival>> = const { Cell::new(None) };
    /// Whether the scene now loading was asked for by a ride, so it starts black.
    static ARRIVING: Cell<bool> = const { Cell::new(false) };
    /// Registry index of the scene being played, from `on_scene_loaded`.
    static CURRENT_SCENE: Cell<Option<usize>> = const { Cell::new(None) };
    /// Floors found unregistered, so each is warned about once.
    static WARNED: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

/// Black overlay level for this frame, 0 (none) to 1 (solid). Drawn by the engine's overlay
/// block.
pub fn fade() -> f32 {
    FADE.with(Cell::get)
}

/// Whether an E press this frame belongs to the elevator rather than the grab: true while
/// the player stands in an idle cabin, whether or not there is anywhere to go -- a press in a
/// cabin with no other floor does nothing, and should not pick anything up either.
pub fn wants_interact() -> bool {
    OFFERING.with(Cell::get)
}

/// Hand the elevator this frame's E press. Consumed by its next `update`.
pub fn press() {
    PRESSED.with(|p| p.set(true));
}

/// Whether a ride began since the last call: the cue for `Sfx::Elevator`.
pub fn take_ride_started() -> bool {
    RIDE_STARTED.with(Cell::take)
}

/// Leave an arrival for the next scene to load: what a ride does as it asks for the load, and
/// what `--arrive` does before one (`Engine::start_direct`).
pub fn deliver(arrival: Arrival) {
    ARRIVAL.with(|a| a.set(Some(arrival)));
}

/// The arrival a ride has set up for the level now loading, consumed. A level that builds an
/// elevator passes it to [`Elevator::new`]; `Some` means the screen is black and the player
/// should be stood in the cabin ([`Elevator::board`]).
pub fn take_arrival() -> Option<Arrival> {
    let a = ARRIVAL.with(Cell::take);
    if a.is_some() {
        ARRIVING.with(|c| c.set(true));
    }
    a
}

/// Called by `ExtState::on_scene_loaded` once a scene is built: records which scene is being
/// played (how an elevator finds its own floor), drops the hint and any press left over from
/// the scene just left, and settles the fade -- black if this load was a ride that the level
/// honoured with [`take_arrival`], clear otherwise, so a scene reached by any other route
/// (a level key, RESTART, a ride into a level without an elevator) can never start under a
/// black overlay nobody will lift.
pub fn on_scene_loaded(scene: usize) {
    CURRENT_SCENE.with(|c| c.set(Some(scene)));
    OFFERING.with(|o| o.set(false));
    PRESSED.with(|p| p.set(false));
    if ARRIVAL.with(Cell::take).is_some() {
        log::warn!("[elevator] scene {scene} did not take its arrival: no elevator there");
    }
    FADE.with(|f| f.set(if ARRIVING.with(Cell::take) { 1.0 } else { 0.0 }));
}

/// Index into [`FLOORS`] of the scene being played, if it is a floor.
fn current_floor() -> Option<usize> {
    let scene = CURRENT_SCENE.with(Cell::get)?;
    FLOORS.iter().position(|f| scenes::index_of(f.scene_name) == Some(scene))
}

/// Whether floor `i` has a scene to go to, warning once about each that has not.
fn floor_registered(i: usize) -> bool {
    if scenes::index_of(FLOORS[i].scene_name).is_some() {
        return true;
    }
    WARNED.with(|w| {
        let mut w = w.borrow_mut();
        if !w.contains(&i) {
            w.push(i);
            log::warn!("[elevator] floor {:?} has no registered scene; skipped", FLOORS[i].label);
        }
    });
    false
}

/// The floor a ride from `here` goes to: the next in [`FLOORS`] order that `registered`
/// accepts, wrapping round, never `here` itself. `None` when there is no such floor, or when
/// `here` is not a floor at all -- an elevator in a scene the list does not name has nowhere
/// to come back from, so it goes nowhere.
fn next_floor(here: Option<usize>, registered: impl Fn(usize) -> bool) -> Option<usize> {
    let here = here?;
    (1..FLOORS.len()).map(|k| (here + k) % FLOORS.len()).find(|&i| registered(i))
}

// ─────────────────────────────────────────────────────────────────────────────
// The ride.
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    /// Doors open, waiting.
    Idle,
    /// Doors closing; seconds since the press.
    Closing(f32),
    /// Doors shut, screen darkening; seconds since they shut.
    Fading(f32),
    /// Screen black, load requested. Nothing more happens to this elevator.
    Gone,
    /// Just arrived: screen clearing, then doors opening; seconds since the load.
    Arriving(f32),
}

/// What a step of the ride asks the outside world to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// The doors have started closing toward this floor.
    Started { to: usize },
    /// The screen is black: load this floor's scene now.
    Load { to: usize },
}

/// The ride's state machine. Pure: it knows time only as the `dt` it is stepped by.
pub struct Ride {
    phase: Phase,
    openness: f32,
    fade: f32,
    /// Where the ride in progress is going.
    to: Option<usize>,
}

impl Ride {
    /// At rest on a floor: doors open, nothing pending.
    pub fn at_rest() -> Ride {
        Ride { phase: Phase::Idle, openness: 1.0, fade: 0.0, to: None }
    }

    /// Just delivered: doors shut behind a black screen, about to open.
    pub fn arriving() -> Ride {
        Ride { phase: Phase::Arriving(0.0), openness: 0.0, fade: 1.0, to: None }
    }

    /// Advance by `dt` seconds. `inside` is whether the player is in the cabin, `pressed`
    /// whether E went down since the last step, and `next` the floor a ride would go to.
    /// A press counts only while idle with the player inside and a floor to go to.
    pub fn step(
        &mut self,
        dt: f32,
        inside: bool,
        pressed: bool,
        next: Option<usize>,
    ) -> Option<Event> {
        match self.phase {
            Phase::Idle => {
                let to = next.filter(|_| inside && pressed)?;
                self.phase = Phase::Closing(0.0);
                self.to = Some(to);
                Some(Event::Started { to })
            }
            Phase::Closing(t) => {
                let t = t + dt;
                self.openness = (1.0 - t / CLOSE_SECS).max(0.0);
                self.phase = if t >= CLOSE_SECS { Phase::Fading(0.0) } else { Phase::Closing(t) };
                None
            }
            Phase::Fading(t) => {
                let t = t + dt;
                self.fade = (t / FADE_SECS).min(1.0);
                if t < FADE_SECS {
                    self.phase = Phase::Fading(t);
                    return None;
                }
                self.phase = Phase::Gone;
                Some(Event::Load { to: self.to.expect("a ride in progress has a destination") })
            }
            Phase::Gone => None,
            Phase::Arriving(t) => {
                let t = t + dt;
                self.fade = (1.0 - t / FADE_SECS).max(0.0);
                self.openness = ((t - FADE_SECS) / OPEN_SECS).clamp(0.0, 1.0);
                self.phase =
                    if t >= FADE_SECS + OPEN_SECS { Phase::Idle } else { Phase::Arriving(t) };
                None
            }
        }
    }

    /// 0 shut .. 1 open.
    pub fn openness(&self) -> f32 {
        self.openness
    }

    /// 0 clear .. 1 black.
    pub fn fade(&self) -> f32 {
        self.fade
    }

    /// Whether a press now would start a ride.
    pub fn is_idle(&self) -> bool {
        self.phase == Phase::Idle
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// The object.
// ─────────────────────────────────────────────────────────────────────────────

/// The `Object` the model is drawn at for a threshold at `pos` facing yaw `yaw`: the model's
/// [`THRESHOLD`] lands on `pos`, and its +z (the way the doorway faces) on `(sin yaw, 0, cos
/// yaw)` -- the convention `door::yaw_facing` produces and `Door::new` takes.
fn placement(pos: Vector3, yaw: f32) -> Object {
    let mut base = Object::new();
    base.euler.y = yaw;
    base.pos = pos - Matrix4::rot_y(yaw).mul_direction(THRESHOLD);
    base
}

/// Whether a point in the model's space is in the cabin: over the floor, under the ceiling,
/// and behind the leaves' plane -- on the threshold is not inside, so the doors can never
/// close on the player.
fn inside_cabin(local: Vector3) -> bool {
    local.x >= CABIN_LO.x
        && local.x <= CABIN_HI.x
        && local.y >= CABIN_LO.y
        && local.y <= CABIN_HI.y
        && local.z >= CABIN_LO.z
        && local.z <= DOOR_PLANE_Z
}

/// The clip time at which the doors stand widest: the clip opens and closes again within its
/// ten seconds, and openness maps onto its first half only.
fn time_of_widest_opening(model: &GltfModel) -> f32 {
    let anim = model.animation(CLIP).unwrap_or_else(|| panic!("{MODEL} has no {CLIP:?} clip"));
    let steps = (anim.duration() / SAMPLE_STEP) as usize;
    let (t_open, widest) = (0..=steps)
        .map(|i| i as f32 * SAMPLE_STEP)
        .map(|t| {
            let d = model
                .node_delta(anim, NODE_DOOR2, t)
                .unwrap_or_else(|| panic!("{CLIP:?} in {MODEL} no longer moves {NODE_DOOR2:?}"));
            (t, d.mag())
        })
        .fold((0.0, 0.0), |best, (t, w)| if w > best.1 { (t, w) } else { best });
    // A clip that moves the leaf nowhere would leave the doors drawn shut for ever while the
    // ride believed them open: as loud a failure as the measurements in the tests.
    assert!(widest > 0.1, "{CLIP:?} in {MODEL} slides {NODE_DOOR2:?} only {widest} m");
    t_open
}

pub struct Elevator {
    /// The model's placement. Static after `new`: the colliders are baked in world space.
    base: Object,
    model: Rc<GltfModel>,
    shader: Rc<Shader>,
    t_open: f32,
    ride: Ride,
    /// The ride's openness, shared with the doors' collider object.
    openness: Rc<Cell<f32>>,
    cabin: Rc<TriMeshCollider>,
    doors: Rc<TriMeshCollider>,
}

impl Elevator {
    /// Stand an elevator with the centre of its threshold (floor level, on the outer face of
    /// its wall slab) at `pos`, its doorway facing `(sin yaw, 0, cos yaw)` -- the yaw
    /// `door::yaw_facing` gives for a facing direction. With an `arrival` the doors start
    /// shut behind a black screen and open on their own; the level should then [`board`]
    /// the player. Without one they stand open.
    ///
    /// [`board`]: Elevator::board
    pub fn new(
        gl: &Rc<glow::Context>,
        res: &Resources,
        pos: Vector3,
        yaw: f32,
        arrival: Option<Arrival>,
    ) -> Elevator {
        let model = GltfModel::acquire(gl, &load_spec());
        let base = placement(pos, yaw);
        let local_to_world = base.local_to_world();

        // The cabin: every part but the leaves. Its floor stops at the leaves' plane, but the
        // model carries its own sill from there to the slab's face, so the step in is level.
        let (pos, idx) = model.triangles(CABIN);
        let cabin = Rc::new(TriMeshCollider::new(pos, idx, &local_to_world));

        // The leaves at rest -- shut -- as one mesh.
        let mut pos: Vec<[f32; 3]> = Vec::new();
        let mut idx: Vec<u32> = Vec::new();
        for part in [DOOR1, DOOR2] {
            let (p, i) = model.triangles(part);
            let off = pos.len() as u32;
            pos.extend_from_slice(p);
            idx.extend(i.iter().map(|k| k + off));
        }
        let doors = Rc::new(TriMeshCollider::new(&pos, &idx, &local_to_world));

        let t_open = time_of_widest_opening(&model);
        log::info!("[elevator] {CLIP:?} widest at {t_open:.2} s");

        let ride = if arrival.is_some() { Ride::arriving() } else { Ride::at_rest() };
        let openness = Rc::new(Cell::new(ride.openness()));
        Elevator {
            base,
            model,
            shader: res.acquire_shader("gltfpbr"),
            t_open,
            ride,
            openness,
            cabin,
            doors,
        }
    }

    /// The collider for the leaves, to push into the scene beside the elevator itself. It
    /// blocks the doorway only while the doors are shut (see [`ElevatorDoors`]).
    pub fn doors(&self) -> ElevatorDoors {
        ElevatorDoors {
            base: Object::new(),
            openness: self.openness.clone(),
            collider: self.doors.clone(),
        }
    }

    /// Stand the player in the middle of the cabin, facing the doors: where a ride delivers
    /// them. Velocity is cleared and `prev_pos` moves with the position, as a respawn does.
    pub fn board(&self, player: &mut Player) {
        let local_to_world = self.base.local_to_world();
        let stand = local_to_world.mul_point(Vector3::new(
            THRESHOLD.x,
            FLOOR_Y + GH_PLAYER_HEIGHT,
            0.5 * (CABIN_LO.z + CABIN_HI.z),
        ));
        let facing = local_to_world.mul_direction(Vector3::new(0.0, 0.0, 1.0)).normalized_safe();
        let r = Respawn::facing(stand, facing);
        player.base.set_position(r.pos);
        player.base.velocity = Vector3::zero();
        player.set_look(r.yaw, 0.0);
    }

    /// World-space bounds of the whole model, `(min, max)`, for a level's fence.
    pub fn world_bounds(&self) -> (Vector3, Vector3) {
        let mut lo = Vector3::splat(f32::MAX);
        let mut hi = Vector3::splat(f32::MIN);
        for part in PARTS.iter().map(|p| p.name) {
            let b = self.model.bounds(part);
            let (wlo, whi) = transformed_box(
                &self.base.local_to_world(),
                Vector3::new(b[0], b[2], b[4]),
                Vector3::new(b[1], b[3], b[5]),
            );
            lo = Vector3::new(lo.x.min(wlo.x), lo.y.min(wlo.y), lo.z.min(wlo.z));
            hi = Vector3::new(hi.x.max(whi.x), hi.y.max(whi.y), hi.z.max(whi.z));
        }
        (lo, hi)
    }

    /// The world-space box a level carves out of the model this elevator is set into
    /// (`Backrooms::new`, `interior::load`; see the module docs): everything behind the
    /// slab's face that the elevator occupies -- the slab's width, from a margin under the
    /// cabin's floor to the slab's top, from a margin behind the cabin's back to the face.
    /// The host's wall behind the doorway goes, and so does whatever of the host stood where
    /// the cabin now is (a thick wall's inside, a floor under the cabin's); the slab, which
    /// stands [`PROUD`] of the host's face, covers the cut's edges.
    pub fn wall_cut(&self) -> (Vector3, Vector3) {
        let b = self.model.bounds(CABIN);
        transformed_box(
            &self.base.local_to_world(),
            Vector3::new(SLAB_X.0, FLOOR_Y - CUT_MARGIN, b[4] - CUT_MARGIN),
            Vector3::new(SLAB_X.1, b[3], WALL_FACE_Z),
        )
    }

    /// Whether the player (eye position) is in the cabin.
    pub fn contains(&self, player_pos: Vector3) -> bool {
        inside_cabin(self.base.world_to_local().mul_point(player_pos))
    }

    /// The `Object` a leaf is drawn at: the elevator's, slid by the clip at the current
    /// openness. The delta is in the model's space; the placement's rotation carries it out.
    fn leaf_placement(&self, node: &str) -> Object {
        let mut obj = Object::new();
        obj.pos = self.base.pos;
        obj.euler = self.base.euler;
        let t = self.ride.openness() * self.t_open;
        if let Some(d) = self.model.animation(CLIP).and_then(|a| self.model.node_delta(a, node, t))
        {
            obj.pos += self.base.local_to_world().mul_direction(d);
        }
        obj
    }
}

impl ObjectT for Elevator {
    fn base(&self) -> &Object {
        &self.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base
    }

    fn update(&mut self, ctx: &UpdateCtx) {
        let inside = self.contains(ctx.player_pos);
        let pressed = PRESSED.with(Cell::take);
        let here = current_floor();
        let next = next_floor(here, floor_registered);
        match self.ride.step(GH_DT, inside, pressed, next) {
            Some(Event::Started { .. }) => RIDE_STARTED.with(|r| r.set(true)),
            Some(Event::Load { to }) => {
                // `next_floor` only names a floor from a floor, and only a registered one.
                let from_floor = here.expect("a ride starts on a floor");
                let scene = scenes::index_of(FLOORS[to].scene_name).expect("a registered floor");
                deliver(Arrival { from_floor });
                request_scene_load(scene);
            }
            None => {}
        }
        self.openness.set(self.ride.openness());
        FADE.with(|f| f.set(self.ride.fade()));
        // The offer, restated every step it stands: the hint channel is emptied each frame
        // (ext::hint), and the press is only taken while the cabin is idle with the player in.
        let offering = inside && self.ride.is_idle();
        OFFERING.with(|o| o.set(offering));
        if offering {
            hint::set(match next {
                Some(f) => format!("E  RIDE TO {}", FLOORS[f].label),
                None => "NO OTHER FLOORS".to_string(),
            });
        }
    }

    fn draw(&self, ctx: &RenderCtx, cam: &Camera, _fbo: Option<glow::Framebuffer>) {
        // draw_part culls each part by its own sphere, the leaves at their slid placement.
        self.model.draw_part(CABIN, &self.base, &self.shader, cam, ctx);
        self.model.draw_part(DOOR1, &self.leaf_placement(NODE_DOOR1), &self.shader, cam, ctx);
        self.model.draw_part(DOOR2, &self.leaf_placement(NODE_DOOR2), &self.shader, cam, ctx);
    }

    fn trimesh(&self) -> Option<Rc<TriMeshCollider>> {
        Some(self.cabin.clone())
    }
}

/// The door leaves' collision: shut doors block the doorway, open ones are inside the wall
/// and block nothing. Its own object because `ObjectT::trimesh` is one mesh per object and
/// the cabin's must always be there; it draws nothing -- the elevator draws the leaves.
pub struct ElevatorDoors {
    base: Object,
    openness: Rc<Cell<f32>>,
    collider: Rc<TriMeshCollider>,
}

impl ObjectT for ElevatorDoors {
    fn base(&self) -> &Object {
        &self.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base
    }
    fn draw(&self, _ctx: &RenderCtx, _cam: &Camera, _fbo: Option<glow::Framebuffer>) {}
    fn trimesh(&self) -> Option<Rc<TriMeshCollider>> {
        (self.openness.get() < DOORS_BLOCK_BELOW).then(|| self.collider.clone())
    }
    /// Not in the rigid-body world's load-time snapshot: a ride arrives with the doors shut,
    /// and the leaves would stay in that world after they opened (src/ext/physics.rs).
    fn static_collision(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_header::GH_PI;

    /// Run the ride for `secs` at the fixed step, collecting every event.
    fn run(ride: &mut Ride, secs: f32, inside: bool, next: Option<usize>) -> Vec<Event> {
        let steps = (secs / GH_DT).round() as usize;
        (0..steps).filter_map(|_| ride.step(GH_DT, inside, false, next)).collect()
    }

    #[test]
    fn a_ride_closes_fades_loads_then_opens_on_the_far_side() {
        let mut r = Ride::at_rest();
        assert!(r.is_idle() && r.openness() == 1.0 && r.fade() == 0.0);
        // Press, inside, with somewhere to go.
        assert_eq!(r.step(GH_DT, true, true, Some(1)), Some(Event::Started { to: 1 }));
        assert!(!r.is_idle());
        // Half-way through closing the doors are half open and the screen is clear.
        assert!(run(&mut r, CLOSE_SECS * 0.5, true, Some(1)).is_empty());
        assert!((r.openness() - 0.5).abs() < 0.01, "{}", r.openness());
        assert_eq!(r.fade(), 0.0);
        // Shut at CLOSE_SECS; then the fade climbs.
        assert!(run(&mut r, CLOSE_SECS * 0.5 + GH_DT, true, Some(1)).is_empty());
        assert_eq!(r.openness(), 0.0);
        let events = run(&mut r, FADE_SECS * 0.5, true, Some(1));
        assert!(events.is_empty() && (r.fade() - 0.5).abs() < 0.01, "{} {events:?}", r.fade());
        // The load is asked for exactly once, when the screen is black, and then nothing.
        let events = run(&mut r, FADE_SECS, true, Some(1));
        assert_eq!(events, [Event::Load { to: 1 }]);
        assert_eq!(r.fade(), 1.0);
        assert!(run(&mut r, 5.0, true, Some(1)).is_empty());
        assert!(r.step(GH_DT, true, true, Some(1)).is_none(), "gone: a press does nothing");

        // The far side: black and shut, then clear, then open.
        let mut a = Ride::arriving();
        assert!(a.fade() == 1.0 && a.openness() == 0.0 && !a.is_idle());
        assert!(run(&mut a, FADE_SECS * 0.5, true, Some(0)).is_empty());
        assert!((a.fade() - 0.5).abs() < 0.01 && a.openness() == 0.0);
        assert!(run(&mut a, FADE_SECS * 0.5 + OPEN_SECS * 0.5, true, Some(0)).is_empty());
        assert!(a.fade() == 0.0 && (a.openness() - 0.5).abs() < 0.01, "{}", a.openness());
        assert!(a.step(GH_DT, true, true, Some(0)).is_none(), "a press while opening is ignored");
        assert!(run(&mut a, OPEN_SECS * 0.5 + GH_DT, true, Some(0)).is_empty());
        assert!(a.is_idle() && a.openness() == 1.0);
    }

    #[test]
    fn e_is_ignored_outside_the_cabin_while_moving_and_with_nowhere_to_go() {
        let mut r = Ride::at_rest();
        assert!(r.step(GH_DT, false, true, Some(1)).is_none(), "outside");
        assert!(r.step(GH_DT, true, true, None).is_none(), "no other floor");
        assert!(r.step(GH_DT, true, false, Some(1)).is_none(), "not pressed");
        assert!(r.is_idle());
        assert_eq!(r.step(GH_DT, true, true, Some(2)), Some(Event::Started { to: 2 }));
        // Moving: presses do nothing, inside or out.
        assert!(r.step(GH_DT, true, true, Some(1)).is_none());
        run(&mut r, CLOSE_SECS, false, Some(1));
        assert!(r.step(GH_DT, false, true, Some(1)).is_none());
        // And the destination is the one pressed for, not the one offered later.
        let events = run(&mut r, FADE_SECS + GH_DT, false, Some(1));
        assert_eq!(events, [Event::Load { to: 2 }]);
    }

    #[test]
    fn next_floor_walks_the_list_skipping_the_unregistered_and_wrapping() {
        let all = |_: usize| true;
        assert_eq!(next_floor(Some(0), all), Some(1));
        assert_eq!(next_floor(Some(1), all), Some(2));
        assert_eq!(next_floor(Some(2), all), Some(0), "wraps");
        // Only the Backrooms registered (this branch): nowhere to go.
        let only_first = |i: usize| i == 0;
        assert_eq!(next_floor(Some(0), only_first), None);
        // Backrooms and Overgrown: they alternate.
        let no_pool = |i: usize| i != 1;
        assert_eq!(next_floor(Some(0), no_pool), Some(2));
        assert_eq!(next_floor(Some(2), no_pool), Some(0));
        // Not on a floor: nowhere to go.
        assert_eq!(next_floor(None, all), None);
    }

    /// Every floor names a registered scene -- a typo here would silently skip a level with
    /// one warning in the log -- the first is where NEW GAME starts, and no scene is listed
    /// twice.
    #[test]
    fn every_floor_is_a_registered_scene() {
        assert!(FLOORS.iter().all(|f| !f.label.is_empty() && !f.scene_name.is_empty()));
        let mut indices: Vec<usize> = FLOORS
            .iter()
            .map(|f| scenes::index_of(f.scene_name).unwrap_or_else(|| panic!("{:?}", f.scene_name)))
            .collect();
        assert_eq!(indices[0], scenes::INTRO);
        assert!((0..FLOORS.len()).all(floor_registered));
        indices.sort_unstable();
        indices.dedup();
        assert_eq!(indices.len(), FLOORS.len());
    }

    #[test]
    fn the_threshold_lands_on_pos_facing_the_yaw() {
        let pos = Vector3::new(10.0, 2.0, -3.0);
        for (yaw, facing) in [
            (0.0, Vector3::new(0.0, 0.0, 1.0)),
            (GH_PI / 2.0, Vector3::new(1.0, 0.0, 0.0)),
            (GH_PI, Vector3::new(0.0, 0.0, -1.0)),
        ] {
            let obj = placement(pos, yaw);
            let m = obj.local_to_world();
            assert!((m.mul_point(THRESHOLD) - pos).mag() < 1e-4, "yaw {yaw}");
            let f = m.mul_direction(Vector3::new(0.0, 0.0, 1.0));
            assert!((f - facing).mag() < 1e-5, "yaw {yaw}: doorway faces {f:?}");
            // And the cabin floor is at pos.y.
            let floor = m.mul_point(Vector3::new(0.0, FLOOR_Y, 0.0));
            assert!((floor.y - pos.y).abs() < 1e-5);
        }
        // `door::yaw_facing` is the convention.
        let yaw = crate::ext::door::yaw_facing(Vector3::new(1.0, 0.0, 0.0));
        assert!((yaw - GH_PI / 2.0).abs() < 1e-5);
    }

    #[test]
    fn inside_means_over_the_floor_behind_the_leaves() {
        let eye = FLOOR_Y + GH_PLAYER_HEIGHT;
        assert!(inside_cabin(Vector3::new(THRESHOLD.x, eye, -0.2)));
        assert!(inside_cabin(Vector3::new(CABIN_LO.x + 0.1, eye, CABIN_LO.z + 0.1)));
        let threshold = Vector3::new(THRESHOLD.x, eye, DOOR_PLANE_Z + 0.05);
        assert!(!inside_cabin(threshold), "on the threshold");
        assert!(!inside_cabin(Vector3::new(THRESHOLD.x, eye, WALL_FACE_Z + 1.0)), "outside");
        assert!(!inside_cabin(Vector3::new(CABIN_HI.x + 0.5, eye, 0.0)), "in the wall");
        assert!(!inside_cabin(Vector3::new(THRESHOLD.x, FLOOR_Y - 1.0, 0.0)), "under the floor");
    }

    /// The shut leaves, as `ElevatorDoors` offers them, stop a player sphere anywhere across
    /// the doorway at any height; the cabin without them does not.
    #[test]
    fn shut_leaves_block_the_doorway() {
        let mut pos: Vec<[f32; 3]> = Vec::new();
        let mut idx: Vec<u32> = Vec::new();
        for part in [DOOR1, DOOR2] {
            let (p, i) = GltfModel::probe_triangles(&load_spec(), part);
            let off = pos.len() as u32;
            pos.extend_from_slice(&p);
            idx.extend(i.iter().map(|k| k + off));
        }
        let leaves = TriMeshCollider::new(&pos, &idx, &Matrix4::identity());
        let (cabin_pos, cabin_idx) = GltfModel::probe_triangles(&load_spec(), CABIN);
        let cabin = TriMeshCollider::new(&cabin_pos, &cabin_idx, &Matrix4::identity());
        let r = crate::game_header::GH_PLAYER_RADIUS;
        for x in [OPENING_X.0 + r, THRESHOLD.x, OPENING_X.1 - r] {
            // A hair off the floor: resting on it is a contact too.
            for y in [FLOOR_Y + r + 0.01, FLOOR_Y + GH_PLAYER_HEIGHT] {
                // A sphere in the doorway, its edge just past the leaves' inner face.
                let c = Vector3::new(x, y, DOOR_PLANE_Z - r + 0.05);
                assert!(leaves.push_sphere(c, r).is_some(), "not stopped at {c:?}");
                assert!(cabin.push_sphere(c, r).is_none(), "the open doorway stops at {c:?}");
            }
        }
    }

    /// The constants the elevator is placed and entered by, read back from the GLB itself.
    #[test]
    fn cabin_is_where_the_constants_say() {
        let v = |pos: &[[f32; 3]], i: u32| Vector3::from_slice(&pos[i as usize]);
        // The floor: the largest upward face of the cabin part below head height.
        let (pos, idx) = GltfModel::probe_triangles(&load_spec(), CABIN);
        let mut floor = (0.0f32, Vector3::zero(), Vector3::zero());
        for t in idx.chunks_exact(3) {
            let (a, b, c) = (v(&pos, t[0]), v(&pos, t[1]), v(&pos, t[2]));
            let n = (b - a).cross(c - a);
            let area = n.mag() * 0.5;
            if n.normalized_safe().y > 0.99 && a.y < 1.0 && area > floor.0 {
                let lo = Vector3::new(a.x.min(b.x).min(c.x), a.y, a.z.min(b.z).min(c.z));
                let hi = Vector3::new(a.x.max(b.x).max(c.x), a.y, a.z.max(b.z).max(c.z));
                floor = (area, lo, hi);
            }
        }
        assert!((floor.1.y - FLOOR_Y).abs() < 0.005, "floor at y = {}", floor.1.y);
        assert!((floor.1.x - CABIN_LO.x).abs() < 0.01 && (floor.2.x - CABIN_HI.x).abs() < 0.01);
        assert!((floor.1.z - CABIN_LO.z).abs() < 0.01 && (floor.2.z - CABIN_HI.z).abs() < 0.01);
        let bounds = |part: &str, roots: &[&str]| {
            let parts = [PartSpec {
                name: part,
                roots,
                skip: &[],
                frame: Frame::Scene,
                anchor: Anchor::Hinge,
            }];
            let spec = Load { parts: &parts, ..load_spec() };
            let (pos, _) = GltfModel::probe_triangles(&spec, part);
            let mut lo = Vector3::splat(f32::MAX);
            let mut hi = Vector3::splat(f32::MIN);
            for p in &pos {
                lo = Vector3::new(lo.x.min(p[0]), lo.y.min(p[1]), lo.z.min(p[2]));
                hi = Vector3::new(hi.x.max(p[0]), hi.y.max(p[1]), hi.z.max(p[2]));
            }
            (lo, hi)
        };
        // The wall slab's outer face and extent. The call button and the floor display
        // outside stand a couple of centimetres proud of it, which is why the slab is measured
        // on its own.
        let wall = bounds("wall", &["Wall"]);
        assert!((wall.1.z - WALL_FACE_Z).abs() < 0.01, "slab face at z = {}", wall.1.z);
        assert!((wall.0.x - SLAB_X.0).abs() < 0.01 && (wall.1.x - SLAB_X.1).abs() < 0.01);
        let far_z = pos.iter().map(|p| p[2]).fold(f32::MIN, f32::max);
        assert!(far_z - WALL_FACE_Z < 0.03, "something stands {far_z} out from the slab");
        // The jambs: the door-height faces that look into the opening, either side of it.
        let (mut west, mut east) = (f32::MIN, f32::MAX);
        for t in idx.chunks_exact(3) {
            let (a, b, c) = (v(&pos, t[0]), v(&pos, t[1]), v(&pos, t[2]));
            let n = (b - a).cross(c - a).normalized_safe();
            let (lo_y, hi_y) = (a.y.min(b.y).min(c.y), a.y.max(b.y).max(c.y));
            let tall = hi_y - lo_y > 2.5 && a.z > DOOR_PLANE_Z - 0.05 && a.x.abs() < 1.2;
            if tall && n.x > 0.99 {
                west = west.max(a.x);
            } else if tall && n.x < -0.99 {
                east = east.min(a.x);
            }
        }
        assert!((west - OPENING_X.0).abs() < 0.01, "west jamb at x = {west}");
        assert!((east - OPENING_X.1).abs() < 0.01, "east jamb at x = {east}");
        // The model's own sill: an upward face spanning the opening from the leaves' plane
        // to the slab's face, at floor level.
        let sill = idx.chunks_exact(3).any(|t| {
            let (a, b, c) = (v(&pos, t[0]), v(&pos, t[1]), v(&pos, t[2]));
            let n = (b - a).cross(c - a).normalized_safe();
            let (lo_z, hi_z) = (a.z.min(b.z).min(c.z), a.z.max(b.z).max(c.z));
            n.y > 0.99
                && (a.y - FLOOR_Y).abs() < 0.01
                && lo_z < DOOR_PLANE_Z
                && hi_z > WALL_FACE_Z - 0.03
                && a.x.min(b.x).min(c.x) < OPENING_X.0 + 0.01
                && a.x.max(b.x).max(c.x) > OPENING_X.1 - 0.01
        });
        assert!(sill, "no sill across the threshold");
        // The leaves: between them they span the doorway, and DOOR_PLANE_Z is their inner
        // face.
        let (d1, d2) = (bounds(DOOR1, &[NODE_DOOR1]), bounds(DOOR2, &[NODE_DOOR2]));
        // The leaves' span at rest, outer edge to outer edge: over the west jamb by 14 cm.
        assert!((d1.0.x + 0.934).abs() < 0.01 && (d2.1.x - 0.979).abs() < 0.01);
        assert!(d1.0.x < OPENING_X.0 && d2.1.x >= OPENING_X.1, "leaves cover the jambs");
        // Top of the leaves: head room and more.
        let top = d1.1.y.max(d2.1.y);
        assert!((top - 2.767).abs() < 0.01 && top - FLOOR_Y > GH_PLAYER_HEIGHT + 0.5);
        let inner = d1.0.z.min(d2.0.z);
        assert!((inner - DOOR_PLANE_Z).abs() < 0.01, "leaves from z = {inner}");
        assert!(d1.1.z.max(d2.1.z) < WALL_FACE_Z, "leaves inside the slab's face");
        // The doorway is a doorway: wide enough to walk through.
        const { assert!(OPENING_X.1 - OPENING_X.0 > 1.5) }
    }
}
