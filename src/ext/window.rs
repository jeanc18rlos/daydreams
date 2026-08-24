//! EXT: a grabbable, resizable, locked window into the Overgrown room. Not part of the C++
//! port.
//!
//! A framed opening hangs on the Backrooms' hall wall, and through it is somewhere else: the
//! Overgrown room (`level18.rs`), loaded a second time two kilometres east of the hall
//! ([`FAR2`]) so the ported portal renderer can draw it the way it draws anything -- it is only
//! ever a portal looking at geometry that is really there. The frame is a prop like any other
//! grabbable: the grab (`ext/grab.rs`) picks it up, lays it flat on whatever the crosshair
//! hits (`ObjectT::place_flat`) and resizes it by perspective like the rest. What makes it a
//! window is that its opening is a `Portal`, and a portal whose quad moves and grows has to be
//! re-placed and re-connected every fixed step: `Portal::connect` bakes both ends' transforms
//! into the warp matrices, so a window that has been carried or rescaled since the last
//! connect would warp to a stale place. `connect` is two small matrix products; 500 of them a
//! second is nothing.
//!
//! # The opening stands off the wall
//!
//! The player's head is a sphere of `GH_PLAYER_RADIUS` (0.2) round the eye, and the collision
//! pass keeps it out of the wall -- so an opening flush with the wall could never be walked
//! through: the eye would stop a fifth of a metre short of its plane. The frame is therefore
//! a box, [`DEPTH`] deep at unit scale, standing proud of the surface, and the opening is its
//! outer face. The depth scales with the window, as a thing's depth does, and the size gate
//! ([`PASS_HEIGHT`]) is what guarantees a passable window is deep enough: at the smallest
//! passable scale the box is [`PASS_HEIGHT`]`/`[`OPENING`]`.1 *` [`DEPTH`] = 0.285 m deep,
//! past the head's radius with room to spare.
//!
//! # Lock, size and pose
//!
//! The window starts locked: the opening is greenish glass ([`LOCKED_TINT`] over the far side,
//! `Portal::tint`), `Portal::passable` is off, and a collider-only rectangle over the opening
//! (`Mesh::colliders_only`, `Collider::rect`) keeps the player from leaning into it. The key
//! (a sibling's) raises `room::request_unlock_window`; the window takes the flag on its next
//! step and the glass clears. Unlocked, it is still a 30 cm square until the player grabs it
//! and steps back: the opening is walked through only when it is [`PASS_HEIGHT`] tall and the
//! frame stands on a wall. A frame the grab has laid on a floor or a ceiling is a picture
//! frame lying there: its portal is parked out of sight rather than tilted, because portals
//! must stay vertical (`Physical::try_portal` only re-aims the camera's yaw, and
//! `Portal::draw` asserts it), and the HUD says to stand it up. [`Opening`] names the four
//! states and the pure rules -- which are passable, what they say, what tint they wear -- are
//! functions of it, tested below.
//!
//! # What the engine is told
//!
//! `ObjectT::engine_collision` is false. The frame's hit sphere (the pick target) is centred
//! on the wall and would be pushed out of it by the collision pass every step, which is what
//! the pass does to a sphere inside a wall; and the portal pass must never warp the window
//! through its own opening, whose plane its centre is a hand's breadth from. So nothing of
//! the engine moves it, and the body is never integrated either (`Physical::update` is not
//! called from [`Window::update`]): a placed window stays exactly where the player put it,
//! on a wall, a floor or a ceiling. It still blocks through its collider and is still picked,
//! carried and resized, since the grab reads the hit sphere and writes `pos` itself.
//!
//! # One way
//!
//! Walking through lands the player in the copy, and a `RoomLogic` the level adds loads the
//! real Overgrown level on the spot (`room::request_scene_load`) with the player at the same
//! relative place and heading, handed over through [`set_arrival`] / [`take_arrival`]. The
//! copy and the level are the same model at the same placement, so nothing is seen to change;
//! the copy exists only to be looked into and stepped into. There is no window on the far
//! side -- the elevator brings the player back.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::camera::Camera;
use crate::collider::Collider;
use crate::ext::audio::{self, Sfx};
use crate::ext::door::yaw_facing;
use crate::ext::room::take_unlock_window;
use crate::level18::CEILING;
use crate::mesh::Mesh;
use crate::object::{Object, ObjectT, RenderCtx, UpdateCtx};
use crate::physical::Physical;
use crate::portal::{connect, Portal};
use crate::resources::Resources;
use crate::sphere::Sphere;
use crate::vector::Vector3;

/// Where the far copy of the Overgrown room stands: `level18.rs`'s placement shifted by this,
/// so a point in the copy is the same point in the level plus `FAR2`. A kilometre past the
/// Backrooms (`meadow::FAR`, x = 1000, whose scan ends 60 m further east) and twice as far as
/// the meadow's tiles or the sea could reach; well past the mood split, so the copy grades as
/// the interior it is.
pub const FAR2: Vector3 = Vector3 { x: 2000.0, y: 0.0, z: 0.0 };
/// A player east of this is in the copy, and the level loads the real room.
pub const CROSSING_X: f32 = 1500.0;
/// The partner opening, in the Overgrown level's own coordinates, on the west wall's inner
/// face (x = -9.11 in that level) with its back to the wall and its far side facing +x into
/// the room; its height is the window's own (`place_portals`), so that whatever height the
/// window is hung at, an eye that goes in at some height above the frame's centre comes out
/// the same height above the partner's, and a player never emerges in the floor or the
/// ceiling. A quarter of a metre off the wall, not a couple of centimetres: the player
/// arrives a hair past the plane, and a head sphere that starts a step inside the wall is
/// shoved out of it, a jolt the transition would show. Nothing ever sees the gap -- the
/// nested pass that draws the copy is clipped to the opening's plane, and the wall behind is
/// behind that plane. The spot is on the corridor that runs south along that wall: a maze
/// wall stands 1.5 m to its +z and the room opens 5.8 m ahead, so a door-sized opening
/// (`level16.rs` measures it) is clear of everything but the wall it hangs on.
pub const PARTNER: Vector3 = Vector3 { x: -8.86, y: 0.0, z: -9.3 };
/// The direction the partner's far side faces: into the room.
pub const PARTNER_FACING: Vector3 = Vector3 { x: 1.0, y: 0.0, z: 0.0 };

/// The opening, width by height, at unit scale.
pub const OPENING: (f32, f32) = (0.3, 0.3);
/// The opening is walked through only when at least this tall: a metre ninety, a door.
pub const PASS_HEIGHT: f32 = 1.9;
/// How deep the frame's box is at unit scale, as a fraction of the opening (module docs).
pub const DEPTH: f32 = 0.045;
/// Half the thickness of a frame bar at unit scale.
pub const BAR_HALF: f32 = 0.015;
/// The locked opening's glass: a green cast over the far side, a little under half opaque.
pub const LOCKED_TINT: [f32; 4] = [0.55, 0.70, 0.55, 0.45];
/// How far below its frame a parked portal goes (module docs): past the far plane, under the
/// ground cap.
const PARK: f32 = 200.0;

/// The largest scale a window hung with its centre `y` above the floor can have: the partner
/// stands at the same height in the far room (`place_portals`), and past this the opening
/// rises through that room's ceiling ([`CEILING`]) or sinks through its floor and the nested
/// pass shows the outside of the model as a black band across the opening. At the hang
/// height of 1.35 m that is 7.2, a 2.16 m door; the grab's own `MAX_P_SCALE` (25, a 7.5 m
/// opening) is never reached. Never under the grab's floor, so a frame dragged along the
/// skirting is small, not inverted.
pub fn max_scale(y: f32) -> f32 {
    (y.min(CEILING - y) / (0.5 * OPENING.1)).max(crate::ext::grab::MIN_P_SCALE)
}

/// What the opening is, this step: the lock first, then the pose, then the size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opening {
    /// The key has not been used: green glass.
    Locked,
    /// Unlocked but lying on a floor or a ceiling: stand it up. (The portal is parked on the
    /// pose alone, locked or not -- `place_portals`.)
    Flat,
    /// Unlocked and upright, but under [`PASS_HEIGHT`].
    Small,
    /// A door.
    Open,
}

/// The rule: `flat` is the frame lying on a floor or a ceiling rather than standing on a
/// wall, `p_scale` the window's physical scale.
pub fn opening(locked: bool, flat: bool, p_scale: f32) -> Opening {
    if locked {
        Opening::Locked
    } else if flat {
        Opening::Flat
    } else if OPENING.1 * p_scale < PASS_HEIGHT {
        Opening::Small
    } else {
        Opening::Open
    }
}

impl Opening {
    /// Whether `Physical::try_portal` may warp through it.
    pub fn passable(self) -> bool {
        self == Opening::Open
    }

    /// Whether the collider rectangle covers it: every state that is not walked through is
    /// leant against, glass or not.
    pub fn shut(self) -> bool {
        !self.passable()
    }

    /// The colour over the far side.
    pub fn tint(self) -> [f32; 4] {
        if self == Opening::Locked {
            LOCKED_TINT
        } else {
            [0.0; 4]
        }
    }

    /// The HUD line while the crosshair is on the frame (`ObjectT::pick_hint`).
    pub fn hint(self) -> Option<&'static str> {
        match self {
            Opening::Locked => Some("LOCKED - IT NEEDS A KEY"),
            Opening::Flat => Some("STAND IT UP ON A WALL"),
            Opening::Small => Some("TOO SMALL - GRAB IT AND STEP BACK"),
            Opening::Open => None,
        }
    }
}

/// Whether a frame with these Euler angles is lying flat. `grab::flat_euler` pitches a
/// flat-placed object a quarter turn about X on a floor or a ceiling and leaves X at zero on
/// a wall; nothing else writes the pitch.
pub fn is_flat(euler: Vector3) -> bool {
    euler.x.abs() > 1e-3
}

/// The portal's transform for a frame whose `Object` is `base`: its centre, yaw and scale.
/// The centre is the box's outer face, [`DEPTH`] out along the frame's forward (into the
/// room) at the frame's scale; the yaw is the frame's, and only the yaw (module docs); the
/// scale is the half-size of the opening, with the frame's `p_scale` carried on the portal's
/// own so the partner can match it exactly.
pub fn portal_transform(base: &Object) -> (Vector3, f32, Vector3, f32) {
    let forward = Vector3::new(-base.euler.y.sin(), 0.0, -base.euler.y.cos());
    let pos = base.pos + forward * (DEPTH * base.p_scale);
    (pos, base.euler.y, Vector3::new(0.5 * OPENING.0, 0.5 * OPENING.1, 1.0), base.p_scale)
}

/// Place `here` at the frame and match `there` to it -- the scale, and the height
/// ([`PARTNER`]) -- then connect the two. Run every step (module docs): both ends must agree
/// for the warp to be rigid, and the warp is baked at connect time. A frame lying flat has
/// its `here` parked [`PARK`] below it, on the POSE and not on the state: the lock wins the
/// state (`opening`), and a locked frame dropped on the carpet would otherwise keep a
/// vertical tinted quad standing up through it.
pub fn place_portals(
    here: &Rc<RefCell<Portal>>,
    there: &Rc<RefCell<Portal>>,
    base: &Object,
    opening: Opening,
) {
    let (pos, yaw, scale, p_scale) = portal_transform(base);
    {
        let mut h = here.borrow_mut();
        h.base.pos = if is_flat(base.euler) { pos - Vector3::new(0.0, PARK, 0.0) } else { pos };
        h.base.euler = Vector3::new(0.0, yaw, 0.0);
        h.base.scale = scale;
        h.base.p_scale = p_scale;
        h.passable = opening.passable();
        h.tint = opening.tint();
    }
    {
        let mut t = there.borrow_mut();
        t.base.pos = FAR2 + Vector3::new(PARTNER.x, pos.y, PARTNER.z);
        t.base.scale = scale;
        t.base.p_scale = p_scale;
        t.passable = opening.passable();
        t.tint = opening.tint();
    }
    connect(here, there);
}

/// The frame's bounding radius at unit scale: the half-diagonal of its box, measured from the
/// hit sphere's centre on the wall. What the grab picks by and fits other props against.
fn frame_radius() -> f32 {
    let half_w = 0.5 * OPENING.0 + 2.0 * BAR_HALF;
    let half_h = 0.5 * OPENING.1 + 2.0 * BAR_HALF;
    (half_w * half_w + half_h * half_h + DEPTH * DEPTH).sqrt()
}

/// The four bars' centres and half-extents in the frame's own space (x along the wall, y up,
/// -z out of the wall), at unit scale, and whether the bar is a rail: uprights outside the
/// opening running the full height plus the rails' thickness, rails between them. The box's
/// far face is the wall (z = 0) and its near face the opening (z = -DEPTH).
///
/// A rail is drawn rolled a quarter turn about its depth, so that every bar has its length
/// along the cube's own y. `cube.obj` maps the whole texture onto each face with the axes
/// assigned per face, and with the length on y the face toward the room and the faces into
/// the opening all run the texture's v across the bar -- which is the edge the painted
/// texture darkens (`tools/gen_window.py`), so the bars get an edge along their length and
/// nothing stretched along it.
fn bar_layout() -> [(Vector3, Vector3, bool); 4] {
    let (w, h) = (0.5 * OPENING.0, 0.5 * OPENING.1);
    let z = -0.5 * DEPTH;
    let upright = Vector3::new(BAR_HALF, h + 2.0 * BAR_HALF, 0.5 * DEPTH);
    let rail = Vector3::new(w, BAR_HALF, 0.5 * DEPTH);
    [
        (Vector3::new(-(w + BAR_HALF), 0.0, z), upright, false),
        (Vector3::new(w + BAR_HALF, 0.0, z), upright, false),
        (Vector3::new(0.0, h + BAR_HALF, z), rail, true),
        (Vector3::new(0.0, -(h + BAR_HALF), z), rail, true),
    ]
}

/// The dev preset for the window as it is built (`--window-scale`, `--unlock-window`): its
/// physical scale and whether the key has already been used. Set once for the process by the
/// command line, read by every Backrooms load; a RESTART gives the same window again.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preset {
    pub p_scale: f32,
    pub unlocked: bool,
}

impl Preset {
    /// As the game builds it: unit scale, locked.
    pub const DEFAULT: Preset = Preset { p_scale: 1.0, unlocked: false };
}

impl Default for Preset {
    fn default() -> Preset {
        Preset::DEFAULT
    }
}

/// Where a player who has just walked through the window is, in the Overgrown level's own
/// coordinates, and which way they look: what the level loads them at in place of its
/// elevator arrival. The camera's yaw and pitch both, so the crossing is seamless.
#[derive(Clone, Copy, Debug)]
pub struct Arrival {
    pub pos: Vector3,
    pub yaw: f32,
    pub pitch: f32,
}

thread_local! {
    static PRESET: Cell<Preset> = const { Cell::new(Preset::DEFAULT) };
    /// The arrival left for the Overgrown level by the crossing, if any.
    static ARRIVAL: Cell<Option<Arrival>> = const { Cell::new(None) };
    /// Whether the key has been used on the window, for the life of the process: every
    /// Backrooms load rebuilds the window, and a player who unlocked it, rode the elevator
    /// away and came back would otherwise find it locked again with the key spent.
    static UNLOCKED: Cell<bool> = const { Cell::new(false) };
}

/// Whether the key has been used on the window since the game started (module docs).
pub fn unlocked() -> bool {
    UNLOCKED.with(Cell::get)
}

/// Record that the key has been used: the window does, on taking the unlock.
fn set_unlocked() {
    UNLOCKED.with(|u| u.set(true));
}

/// Set the preset for every window built from now on (the command line's dev flags).
pub fn set_preset(p: Preset) {
    PRESET.with(|c| c.set(p));
}

/// The preset a window is built with.
pub fn preset() -> Preset {
    PRESET.with(Cell::get)
}

/// Leave an arrival for the Overgrown level about to load: what the crossing does as it asks
/// for the load, with the player's position already brought back by `FAR2`.
pub fn set_arrival(pos: Vector3, yaw: f32, pitch: f32) {
    ARRIVAL.with(|a| a.set(Some(Arrival { pos, yaw, pitch })));
}

/// The arrival the crossing set up for the level now loading, consumed.
pub fn take_arrival() -> Option<Arrival> {
    ARRIVAL.with(Cell::take)
}

/// The heading and pitch of the camera whose transform is `cam_to_world`, as `Player::set_look`
/// takes them: yaw in the engine's convention (forward is `(-sin yaw, 0, -cos yaw)`), pitch
/// positive looking up. Derived from the forward vector rather than read off the player,
/// which a room's logic cannot borrow (`ext/room.rs`).
pub fn look_of(cam_to_world: &crate::vector::Matrix4) -> (f32, f32) {
    let f = cam_to_world.mul_direction(Vector3::new(0.0, 0.0, -1.0)).normalized_safe();
    let flat = (f.x * f.x + f.z * f.z).sqrt();
    (-f.x.atan2(-f.z), f.y.atan2(flat))
}

/// The window: a grabbable frame whose opening is a portal.
pub struct Window {
    /// The body the grab handles: one hit sphere, the collider rectangle as its mesh.
    body: Physical,
    here: Rc<RefCell<Portal>>,
    there: Rc<RefCell<Portal>>,
    /// The frame's bars: the mesh, shader and texture of each, placed from the body at
    /// draw time (`draw`) -- the body is where the grab put it this frame, and the bars drawn
    /// from a placement taken in the fixed step would trail the crosshair by a frame.
    bars: [Object; 4],
    /// The collider rectangle over the opening, and the same mesh without it: swapped onto the
    /// body as the opening shuts and opens. Both carry the frame's bounding radius, so the
    /// grab's pick sphere is the same whichever is on.
    pane: Rc<Mesh>,
    clear: Rc<Mesh>,
    locked: bool,
    opening: Opening,
}

impl Window {
    /// A window whose frame's back is at `pos`, hanging on a surface whose normal toward the
    /// room is `facing`, with its opening `here` and its partner `there` -- both already in
    /// the scene's portal vector. `there` is turned to face the copy's room; where it stands
    /// follows the window (`place_portals`).
    pub fn new(
        gl: &Rc<glow::Context>,
        res: &Resources,
        pos: Vector3,
        facing: Vector3,
        here: Rc<RefCell<Portal>>,
        there: Rc<RefCell<Portal>>,
        preset: Preset,
    ) -> Window {
        let radius = frame_radius();
        let mut body = Physical::new();
        body.base.pos = pos;
        // The grab's flat placement: local +z into the wall, so the face (-z) looks out.
        body.base.euler.y = yaw_facing(-facing);
        body.base.p_scale = preset.p_scale;
        // Never integrated (module docs); the grab puts gravity back on release and it goes
        // unused either way.
        body.gravity = Vector3::zero();
        // Exactly one hit sphere: what `grab::as_grabbable` recognises a prop by.
        body.hit_spheres.push(Sphere::new_at(Vector3::zero(), radius));

        // The rectangle over the opening, on the box's outer face, in the frame's own space;
        // the body's transform scales it with the window.
        let rect = Collider::rect(
            Vector3::new(0.0, 0.0, -DEPTH),
            Vector3::new(0.5 * OPENING.0, 0.0, 0.0),
            Vector3::new(0.0, 0.5 * OPENING.1, 0.0),
        );
        let with_radius = |colliders| {
            let mut m = Mesh::colliders_only(gl, colliders);
            m.bound_radius = radius;
            Rc::new(m)
        };
        let pane = with_radius(vec![rect]);
        let clear = with_radius(Vec::new());

        let bars = std::array::from_fn(|_| {
            let mut o = Object::new();
            o.mesh = Some(res.acquire_mesh("cube.obj"));
            // Flat, unlit: the bake around it is, and the ported `texture` light would leave
            // a bar facing down the hall near black (see `ext/painting.rs`).
            o.shader = Some(res.acquire_shader("cutout"));
            o.texture = Some(res.acquire_texture("window_frame.bmp", 1, 1));
            o
        });

        there.borrow_mut().base.euler = Vector3::new(0.0, yaw_facing(PARTNER_FACING), 0.0);

        let mut w = Window {
            body,
            here,
            there,
            bars,
            pane,
            clear,
            locked: !(preset.unlocked || unlocked()),
            opening: Opening::Locked,
        };
        w.settle();
        w
    }

    /// Bring everything that follows the body up to date: the scale within what the far
    /// room allows, the opening's state, the body's collider and the two portals.
    fn settle(&mut self) {
        self.clamp_scale();
        let base = &self.body.base;
        self.opening = opening(self.locked, is_flat(base.euler), base.p_scale);
        place_portals(&self.here, &self.there, base, self.opening);
        let mesh = if self.opening.shut() { &self.pane } else { &self.clear };
        self.body.base.mesh = Some(Rc::clone(mesh));
    }

    /// Keep the scale under [`max_scale`] for the frame's height. The grab writes `p_scale`
    /// once per rendered frame and its own eased value keeps growing past the clamp; this
    /// runs after every such write (`on_rescale`) and every step (`settle`), so the drawn
    /// frame and the portal never disagree.
    fn clamp_scale(&mut self) {
        let base = &mut self.body.base;
        base.p_scale = base.p_scale.min(max_scale(base.pos.y));
    }

    /// A bar placed on the body as it stands now: `template` is the bar's mesh, shader and
    /// texture, `at`/`half`/`rail` its slot in [`bar_layout`].
    fn placed_bar(
        template: &Object,
        base: &Object,
        at: Vector3,
        half: Vector3,
        rail: bool,
    ) -> Object {
        let mut bar = Object::new();
        bar.mesh = template.mesh.clone();
        bar.shader = template.shader.clone();
        bar.texture = template.texture.clone();
        bar.pos = base.local_to_world().mul_point(at);
        bar.euler = base.euler;
        bar.scale = half * base.p_scale;
        if rail {
            // The roll is the innermost rotation (`Object::local_to_world`): about the bar's
            // own depth, whatever the frame's pose.
            bar.euler.z = std::f32::consts::FRAC_PI_2;
            bar.scale = Vector3::new(bar.scale.y, bar.scale.x, bar.scale.z);
        }
        bar
    }
}

impl ObjectT for Window {
    fn base(&self) -> &Object {
        &self.body.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.body.base
    }

    fn update(&mut self, _ctx: &UpdateCtx) {
        if take_unlock_window() {
            self.locked = false;
            set_unlocked();
        }
        let was = self.opening;
        self.settle();
        // EXT: the one moment the frame stops being furniture and becomes a way through --
        // whether it got there by the key or by being carried backwards until it was tall
        // enough. Here rather than in `settle`, which the constructor also calls: a window
        // built already unlocked and oversized (`--unlock-window --window-scale`) would
        // otherwise swell at the player the instant the level loaded.
        if !was.passable() && self.opening.passable() {
            audio::request(Sfx::WindowGrow);
        }
    }

    fn draw(&self, ctx: &RenderCtx, cam: &Camera, _fbo: Option<glow::Framebuffer>) {
        // The body draws nothing (no shader); the opening is drawn by the portal pass. The
        // bars are placed from the body as it is NOW -- where the grab put it after the fixed
        // steps -- so a carried frame does not trail the crosshair by a frame. The portal
        // cannot follow: its warp is baked by `connect` in the step, and the render path
        // holds the portals immutably; the opening is a frame behind the bars while carried.
        let base = &self.body.base;
        for (bar, (at, half, rail)) in self.bars.iter().zip(bar_layout()) {
            Window::placed_bar(bar, base, at, half, rail).draw_impl(ctx, cam);
        }
    }

    fn engine_collision(&self) -> bool {
        false
    }
    /// The pane is carried about and comes and goes with the lock: not in the rigid-body
    /// world's load-time snapshot (src/ext/physics.rs).
    fn static_collision(&self) -> bool {
        false
    }
    fn place_flat(&self) -> bool {
        true
    }
    fn pick_hint(&self) -> Option<&'static str> {
        self.opening.hint()
    }
    /// The held key's target (`ext/key.rs`): the key finds the window by this while it is
    /// locked, and once used the unlock arrives through `take_unlock_window` on the next
    /// step; an unlocked window takes no key.
    fn accepts_key(&self) -> bool {
        self.locked
    }

    /// The grab has just written `p_scale`: back under the clamp before the frame is drawn.
    fn on_rescale(&mut self, _p_scale: f32) {
        self.clamp_scale();
    }

    fn as_physical(&self) -> Option<&Physical> {
        Some(&self.body)
    }
    fn as_physical_mut(&mut self) -> Option<&mut Physical> {
        Some(&mut self.body)
    }
}

/// A finished object drawn where it was put and never updated: the far copy's elevator cabin.
///
/// The copy is built with the same doorway cut out of its wall as the level's, so that the
/// two loads share one model (`GltfModel::acquire` caches by the whole `Load`, cut boxes
/// included, and the crossing then costs no parse) -- and a cut needs a cabin in it. A second
/// `Elevator` cannot be in the scene as itself: its update writes the elevator's ambient
/// channels (the offer, the fade, the press) and would overwrite the hall's own. So the cabin
/// is built where the level would build it, for the cut the level would make, moved to the
/// copy, and drawn from here with its update and its collider left behind -- the collider was
/// built at the unmoved spot, and nobody stands in the copy long enough to need one.
pub struct Still(Box<dyn ObjectT>);

impl Still {
    /// `obj` shifted by `by`, drawn there.
    pub fn new(mut obj: Box<dyn ObjectT>, by: Vector3) -> Still {
        obj.base_mut().pos += by;
        Still(obj)
    }
}

impl ObjectT for Still {
    fn base(&self) -> &Object {
        self.0.base()
    }
    fn base_mut(&mut self) -> &mut Object {
        self.0.base_mut()
    }
    fn draw(&self, ctx: &RenderCtx, cam: &Camera, fbo: Option<glow::Framebuffer>) {
        self.0.draw(ctx, cam, fbo);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_header::GH_PI;
    use crate::vector::Matrix4;

    fn near(a: Vector3, b: Vector3) -> bool {
        (a - b).mag() < 1e-4
    }

    #[test]
    fn the_lock_then_the_pose_then_the_size_decide_the_opening() {
        let big = PASS_HEIGHT / OPENING.1;
        assert_eq!(opening(true, false, big), Opening::Locked, "locked beats everything");
        assert_eq!(opening(true, true, 1.0), Opening::Locked);
        assert_eq!(opening(false, true, big), Opening::Flat, "flat beats size");
        assert_eq!(opening(false, false, 1.0), Opening::Small);
        assert_eq!(opening(false, false, big * 0.999), Opening::Small);
        assert_eq!(opening(false, false, big), Opening::Open, "exactly tall enough is enough");
        assert_eq!(opening(false, false, 7.0), Opening::Open);
    }

    #[test]
    fn only_an_open_window_is_passable_and_the_rest_say_why() {
        for o in [Opening::Locked, Opening::Flat, Opening::Small] {
            assert!(!o.passable() && o.shut(), "{o:?}");
            assert!(o.hint().is_some(), "{o:?}");
        }
        assert!(Opening::Open.passable() && !Opening::Open.shut());
        assert_eq!(Opening::Open.hint(), None);
        assert_eq!(Opening::Locked.hint(), Some("LOCKED - IT NEEDS A KEY"));
        assert_eq!(Opening::Small.hint(), Some("TOO SMALL - GRAB IT AND STEP BACK"));
        assert_eq!(Opening::Flat.hint(), Some("STAND IT UP ON A WALL"));
        // The glass is the lock's and nothing else's.
        assert_eq!(Opening::Locked.tint(), LOCKED_TINT);
        for o in [Opening::Flat, Opening::Small, Opening::Open] {
            assert_eq!(o.tint(), [0.0; 4], "{o:?}");
        }
    }

    #[test]
    fn a_passable_window_is_deeper_than_the_players_head() {
        let depth = PASS_HEIGHT / OPENING.1 * DEPTH;
        assert!(depth > crate::game_header::GH_PLAYER_RADIUS + 0.05, "box {depth} m deep");
    }

    #[test]
    fn flat_is_the_pitch_the_grab_writes() {
        assert!(!is_flat(Vector3::new(0.0, 1.3, 0.0)));
        assert!(is_flat(crate::ext::grab::flat_euler(
            Vector3::new(0.0, 1.0, 0.0),
            Vector3::new(0.3, -0.9, 0.1)
        )));
        assert!(!is_flat(crate::ext::grab::flat_euler(
            Vector3::new(0.0, 0.0, -1.0),
            Vector3::new(0.3, -0.2, 0.9)
        )));
    }

    /// A frame on the hall's north wall (facing -z, yaw 0) at scale s: the portal is
    /// `DEPTH * s` out along -z, the same yaw, vertical, and the opening's half-size.
    #[test]
    fn the_portal_sits_on_the_boxs_outer_face_at_the_frames_scale() {
        let mut base = Object::new();
        base.pos = Vector3::new(987.0, 1.5, 2.06);
        base.euler.y = yaw_facing(Vector3::new(0.0, 0.0, 1.0));
        base.p_scale = 7.0;
        let (pos, yaw, scale, p_scale) = portal_transform(&base);
        assert!(near(pos, Vector3::new(987.0, 1.5, 2.06 - 7.0 * DEPTH)), "{pos:?}");
        assert_eq!(yaw, 0.0);
        assert!(near(scale, Vector3::new(0.15, 0.15, 1.0)));
        assert_eq!(p_scale, 7.0);
        // The frame's forward, the way `Object::forward` reports it, is out of the wall.
        assert!(near(base.forward(), Vector3::new(0.0, 0.0, -1.0)));
        // On a wall facing +x the portal is out along +x.
        base.euler.y = yaw_facing(Vector3::new(-1.0, 0.0, 0.0));
        base.p_scale = 1.0;
        let (pos, _, _, _) = portal_transform(&base);
        assert!(near(pos, base.pos + Vector3::new(DEPTH, 0.0, 0.0)), "{pos:?}");
    }

    /// Two detached portals through `place_portals`: the partner matches the scale, the warp
    /// is rigid, and a point just inside the hall's opening lands just inside the partner's
    /// far side at the same offset.
    #[test]
    fn the_partner_matches_and_the_warp_is_rigid() {
        let here = Rc::new(RefCell::new(Portal::detached()));
        let there = Rc::new(RefCell::new(Portal::detached()));
        there.borrow_mut().base.euler = Vector3::new(0.0, yaw_facing(PARTNER_FACING), 0.0);
        let mut base = Object::new();
        base.pos = Vector3::new(987.0, 1.35, 2.06);
        base.euler.y = yaw_facing(Vector3::new(0.0, 0.0, 1.0));
        base.p_scale = 7.0;
        place_portals(&here, &there, &base, Opening::Open);
        let (h, t) = (here.borrow(), there.borrow());
        assert!(near(h.base.scale, t.base.scale) && h.base.p_scale == t.base.p_scale);
        // The partner stands at its spot, at the window's height.
        assert!(
            near(t.base.pos, FAR2 + Vector3::new(PARTNER.x, 1.35, PARTNER.z)),
            "{:?}",
            t.base.pos
        );
        assert!(h.passable && t.passable);
        assert_eq!(h.tint, [0.0; 4]);
        assert!(h.base.euler.x == 0.0 && h.base.euler.z == 0.0);
        assert!(t.base.euler.x == 0.0 && t.base.euler.z == 0.0);
        // The player comes at the opening from the hall side, which is the portal's front
        // (its forward points into the room), so the front warp is the one taken.
        let hall_side = h.base.pos + Vector3::new(0.3, -0.2, -0.5);
        assert!((hall_side - h.base.pos).dot(h.base.forward()) > 0.0);
        let warp = &h.front;
        assert_eq!(warp.to_portal, Some(t.id));
        assert!((warp.delta_inv.x_axis().mag() - 1.0).abs() < 1e-4, "rigid");
        let landed = warp.delta_inv.mul_point(h.base.pos + Vector3::new(0.3, -0.2, 0.01));
        // Just past the partner, 0.3 to the partner's right and 0.2 down: the partner faces
        // +x with its back to -x, so its local x is world -z... and a step through along +z
        // in the hall is a step along +x there.
        assert!(near(landed, t.base.pos + Vector3::new(0.01, -0.2, -0.3)), "{landed:?}");
        let heading = warp.delta_inv.mul_direction(Vector3::new(0.0, 0.0, 1.0));
        assert!(near(heading, PARTNER_FACING), "{heading:?}");
    }

    #[test]
    fn a_flat_frame_parks_its_portal_and_a_locked_one_tints_both() {
        let here = Rc::new(RefCell::new(Portal::detached()));
        let there = Rc::new(RefCell::new(Portal::detached()));
        let mut base = Object::new();
        base.pos = Vector3::new(10.0, 0.01, 10.0);
        base.euler = crate::ext::grab::flat_euler(Vector3::new(0.0, 1.0, 0.0), Vector3::unit_x());
        place_portals(&here, &there, &base, Opening::Flat);
        {
            let h = here.borrow();
            assert!(h.base.pos.y < -100.0, "parked: {:?}", h.base.pos);
            assert!(!h.passable && h.base.euler.x == 0.0);
        }
        // Locked AND flat -- the first thing a player can do to the window is drop it on the
        // carpet: the state is the lock's (tint, hint) but the portal is parked all the same.
        assert_eq!(opening(true, true, 1.0), Opening::Locked);
        place_portals(&here, &there, &base, Opening::Locked);
        {
            let h = here.borrow();
            assert!(h.base.pos.y < -100.0, "a locked flat frame parks too: {:?}", h.base.pos);
            assert!(!h.passable && h.base.euler.x == 0.0);
            assert_eq!(h.tint, LOCKED_TINT);
        }
        // On the ceiling as well.
        base.euler = crate::ext::grab::flat_euler(Vector3::new(0.0, -1.0, 0.0), Vector3::unit_x());
        place_portals(&here, &there, &base, Opening::Locked);
        assert!(here.borrow().base.pos.y < -100.0, "ceiling: {:?}", here.borrow().base.pos);
        base.euler = Vector3::zero();
        place_portals(&here, &there, &base, Opening::Locked);
        let (h, t) = (here.borrow(), there.borrow());
        assert!(near(h.base.pos, base.pos + Vector3::new(0.0, 0.0, -DEPTH)), "back in place");
        assert!(!h.passable && !t.passable);
        assert_eq!(h.tint, LOCKED_TINT);
        assert_eq!(t.tint, LOCKED_TINT);
    }

    #[test]
    fn the_frame_radius_covers_the_bars() {
        let r = frame_radius();
        for (at, half, _) in bar_layout() {
            let corner =
                Vector3::new(at.x.abs() + half.x, at.y.abs() + half.y, at.z.abs() + half.z);
            assert!(corner.mag() <= r + 1e-6, "{corner:?} outside {r}");
        }
        // And the opening itself is clear of every bar.
        for (at, half, _) in bar_layout() {
            let inside = at.x.abs() - half.x < 0.5 * OPENING.0 - 1e-6
                && at.y.abs() - half.y < 0.5 * OPENING.1 - 1e-6;
            assert!(!inside, "bar at {at:?} intrudes on the opening");
        }
    }

    #[test]
    fn look_of_reads_the_camera_the_way_set_look_writes_it() {
        for (yaw, pitch) in [(0.0, 0.0), (GH_PI / 2.0, 0.0), (-0.7, 0.3), (2.5, -0.4)] {
            let m = Matrix4::trans(Vector3::new(1.0, 2.0, 3.0))
                * Matrix4::rot_y(yaw)
                * Matrix4::rot_x(pitch);
            let (y, p) = look_of(&m);
            assert!((y - yaw).abs() < 1e-5 && (p - pitch).abs() < 1e-5, "{yaw} {pitch}: {y} {p}");
        }
    }

    /// The opening fits under the far room's ceiling and over its floor at the frame's
    /// height, whatever the grab asks for.
    #[test]
    fn the_scale_is_capped_by_the_far_rooms_floor_and_ceiling() {
        // Hung at 1.35 m: the ceiling is the nearer, 1.08 m up, so a 2.16 m opening at most.
        assert!((max_scale(1.35) - 1.08 / 0.15).abs() < 1e-4);
        assert!(max_scale(1.35) * OPENING.1 >= PASS_HEIGHT, "still a door at the hang height");
        // Mid-height is the most it can ever be; near the floor it is the floor's.
        assert!(max_scale(0.5 * CEILING) > max_scale(1.35));
        assert!((max_scale(0.3) - 2.0).abs() < 1e-4);
        assert!(max_scale(0.3) * OPENING.1 < PASS_HEIGHT, "too low to be a door");
        // Never under the grab's floor, even dragged along the skirting or above the ceiling.
        assert_eq!(max_scale(0.0), crate::ext::grab::MIN_P_SCALE);
        assert_eq!(max_scale(CEILING + 1.0), crate::ext::grab::MIN_P_SCALE);
    }

    #[test]
    fn the_unlock_outlives_the_window() {
        assert!(!unlocked());
        set_unlocked();
        assert!(unlocked(), "not consumed: every later Backrooms load builds it unlocked");
        assert!(unlocked());
        UNLOCKED.with(|u| u.set(false));
    }

    #[test]
    fn the_arrival_is_taken_once_and_the_preset_sticks() {
        assert!(take_arrival().is_none());
        set_arrival(Vector3::new(-8.0, 1.5, -8.5), 1.0, -0.1);
        let a = take_arrival().expect("set");
        assert!(near(a.pos, Vector3::new(-8.0, 1.5, -8.5)) && a.yaw == 1.0 && a.pitch == -0.1);
        assert!(take_arrival().is_none(), "consumed");
        assert_eq!(preset(), Preset::default());
        set_preset(Preset { p_scale: 7.0, unlocked: true });
        assert_eq!(preset(), Preset { p_scale: 7.0, unlocked: true });
        assert_eq!(preset().p_scale, 7.0, "not consumed");
        set_preset(Preset::default());
    }
}
