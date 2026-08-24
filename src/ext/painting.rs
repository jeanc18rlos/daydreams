//! EXT: portraits whose eyes follow you, and whose faces change only while you are not
//! looking. Not part of the C++ port.
//!
//! A `Painting` is a canvas -- the ported `quad.obj` drawn with `Shaders/painting.*` -- inside
//! a gilt frame of four thin `cube.obj` bars drawn the way any ported prop is. The picture is
//! a public-domain painting the user cut into a sheet -- the sitter with blank eye sockets
//! and no mouth, and cutouts of the eyes and of three mouths -- which `tools/gen_portraits.py`
//! turns into a base texture, a parts atlas and the numbers in `ext/portrait_atlas.rs`: where
//! each part sits on the base and where the eye openings are ([`Portrait`]). The shader lays
//! the parts over the base. The painting hangs flush on a wall and has two behaviours built
//! on two things the engine already does:
//!
//! * **The eyes follow the camera of the pass.** Every draw receives the pass camera's eye
//!   (`RenderCtx.eye`); the painting turns that into a point in its own canvas metres and
//!   from it an offset per eye in the base's UV ([`iris_offsets`]), and the shader slides the
//!   eye part by that offset inside the eye's opening -- the iris and pupil by the whole of
//!   it, the sclera easing to nothing at the lid, so the lids stay put and nothing tears
//!   (`IRIS_CORE`, `GAZE_LIMIT`). A look further to the viewer's left than the warp carries
//!   crossfades to the sheet's left-looking eyes; the sheets have no right-looking pair, so
//!   to the right the warp saturates and stays. Because it is the PASS eye, a portrait seen
//!   through a portal looks at the portal camera -- at the person in the doorway, not at
//!   some spot on the meadow a thousand units away.
//! * **The face changes only while unobserved** -- Level10's statues, applied to an expression.
//!   [`Watch::seen_from`] is `ext::visibility`'s cone-and-line-of-sight test, extended to see
//!   through the scene's portals (below), and [`Expression`] advances only after [`GRACE`]
//!   seconds of nobody looking, one step round a cycle of three: the smile goes sad and the
//!   eyes turn to the left, frozen on where you were last seen from; then the mouth goes
//!   angry and the eyes come back to you; then the smile again. Look away and back, and it
//!   is different; you never catch it moving. The eyes have a memory too ([`Gaze`]): while
//!   nobody looks they stay aimed at where the viewer was last seen from, and when looked
//!   at again they slide from there to the viewer over [`REACQUIRE`] seconds -- so turning
//!   back finds them on the spot you left.
//!
//! # The key in the painting
//!
//! One portrait carries a key, painted in anamorphosis: the key proper lives on a virtual
//! picture plane through the canvas centre, perpendicular to the line from a sweet spot `V`
//! to that centre, and what is on the canvas is that key's central projection from `V` --
//! a smear along the canvas that only closes into a key seen from `V`, a grazing view along
//! the wall ([`to_plane`], [`to_canvas`]; the shader does the same sum per fragment). Stand
//! in the sweet spot and look at the canvas ([`KeySpec`], [`armed`]) and the paint catches
//! the light, the HUD says so, and a real key -- `ext::key::Key`, built at load and kept --
//! is spawned on the canvas at the painted key's spot and comes out of it over [`EMERGE`]
//! seconds toward the viewer, growing as the painted one fades ([`Emergence`]). Step out of the spot before
//! taking it and it sinks back and is unpainted again; take it (the key's `on_grab` sets a
//! flag both share) and the painting's key is gone for good.
//!
//! # Seeing through a door
//!
//! From the meadow the hall is a thousand units off along +x and the direct cone test says
//! "not looking" for every painting in it -- which would let them change while the player
//! watches them through the open door. So a painting also counts as observed when it is seen
//! through a portal: its image in the viewer's world (`Warp::delta`, the transform the portal
//! pass renders with) is in the view cone, the line from the eye to that image crosses the
//! portal's quad, the near half of that line is clear, and the far half -- from where the line
//! comes out of the far portal to the painting itself -- is clear of the building. A portal is
//! consulted only for paintings on its far side: a painting nearer this end of it than the
//! other is in this world and is seen directly or not at all.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use glow::HasContext;

use crate::camera::Camera;
use crate::collider::Collider;
use crate::ext::backrooms::WALL_FOG;
use crate::ext::cull::object_sphere;
use crate::ext::door::{yaw_facing, DoorLink};
use crate::ext::key::Key;
use crate::ext::portrait_atlas::Portrait;
use crate::ext::visibility::{has_line_of_sight, in_view_cone, WATCH_HALF_ANGLE};
use crate::ext::{hint, room};
use crate::game_header::GH_DT;
use crate::mesh::Mesh;
use crate::object::{Object, ObjectT, RenderCtx, UpdateCtx};
use crate::portal::{Portal, Warp};
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec};
use crate::texture::Texture;
use crate::vector::{Matrix4, Vector3};

/// Seconds a painting must go unobserved before its expression changes. Long enough that a
/// glance across it, or a head turn that sweeps it out of the cone and back, changes nothing.
pub const GRACE: f32 = 0.4;
/// Seconds a changed expression crossfades over, from the change: well inside the grace, so
/// that a glance that ends a stretch finds the new face settled, never half-way.
pub const FADE: f32 = 0.25;
/// Seconds over which the eyes slide from where the viewer was to where they are, once looked
/// at again. Slow enough to be seen, which is the point.
pub const REACQUIRE: f32 = 0.6;
/// Fixed steps between observation tests. A test walks every object in the scene for up to
/// two rays (cheap: the building's BVH answers a cast in well under a microsecond, measured),
/// but eight paintings asking at 500 Hz is still four thousand walks a second for an answer
/// that only has to be right within a fraction of [`GRACE`]. Every tenth step is 50 Hz, and
/// each painting asks on its own phase so the walks spread across steps.
const WATCH_EVERY: u32 = 10;

/// Half the side of a bar's square section. The bars are rolled 45 degrees about their
/// length, so what faces the room is a ridge between two bevels -- a moulding, not a slat --
/// and the ported `texture` shader's fixed light, which comes from above and from +z, lights
/// the upper bevel and shades the lower on whichever wall the frame hangs. A flat slat facing
/// -z was near black under that light.
const BAR_HALF: f32 = 0.02;
/// How far a rolled bar reaches from its centreline: the section's half-diagonal. Also how far
/// its ridge stands off the wall.
const BAR_REACH: f32 = BAR_HALF * std::f32::consts::SQRT_2;
/// Where the canvas sits: behind the bars' ridges, so an oblique view sees it recessed, and off
/// the wall, so nothing is coplanar with the scan. The bars' inner bevels are `BAR_REACH` wide
/// at their ridge and this much narrower at the canvas plane, and still cover its edge.
const CANVAS_DEPTH: f32 = 0.012;
/// The point the observed test asks about: a hand's breadth in front of the canvas rather
/// than on it. The line-of-sight ray stops just short of its target, and a scanned wall is not
/// a plane -- a bump a centimetre proud of the face would occlude a point on the face itself.
const PROBE_OUT: f32 = 0.15;

// ── The iris warp ────────────────────────────────────────────────────────────────────────────
/// Inside this fraction of an eye opening's radius the eye part moves by the whole gaze
/// offset; from there to the lid the movement eases out (a smoothstep): the shader's
/// `iris_core`. The ease's steepest slope is 1.5 / (1 - IRIS_CORE) per unit radius, and an
/// offset times that slope must stay under the radius or the warp folds over itself; the
/// limits below keep it at about half (`the_warp_never_folds`).
pub const IRIS_CORE: f32 = 0.35;
/// How far the iris travels per unit tangent of the viewing angle, as a fraction of the eye
/// opening's width: a viewer 45 degrees off the canvas's normal pulls it a quarter of the
/// eye's width toward them, before the clamp.
pub const GAZE_K: f32 = 0.25;
/// The furthest the iris goes, as a fraction of the eye's width across and of its height up
/// and down: twelve percent of the width (the eyes are real paintings, and a pupil jammed
/// into the corner reads as a squint), and fifteen of the height, which the lids would hide
/// anyway. Both are within what the warp carries without folding (`IRIS_CORE`).
pub const GAZE_LIMIT: (f32, f32) = (0.12, 0.15);
/// How many times the leftward limit the look must call for before the left-looking eyes
/// have fully taken over from the warped centre ones: the crossfade runs from one limit to
/// this many, so the two are never swapped for a look the warp could have carried.
pub const LEFT_FULL: f32 = 2.5;
/// The viewer's distance off the canvas is taken as at least this, so the tangent is bounded:
/// a viewer level with the canvas (or behind it) gets the iris pinned at the limit toward
/// their side rather than a divide by zero.
const GAZE_MIN_Z: f32 = 0.05;

/// The most pieces a portrait's seven variants may hold between them: the four eye variants
/// are one piece each (the iris warp needs a single rect to slide), and each of the three
/// mouths as many as fit. Every sheet that ships cuts one piece per variant -- seven in all,
/// the Hals moustache arriving inside the mouth's own crop -- and the headroom is what would
/// let a sitter whose mouth comes as separate cutouts hang without a shader change. The
/// shader's uniform arrays are sized to this (painting.frag `part_atlas`), so the two must
/// move together.
pub const MAX_PIECES: usize = 16;

/// The three faces a portrait cycles through while unobserved ([`Expression`]): the mouth
/// each one wears and whether the eyes have turned to the left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// The sitter as painted: the smile, the eyes on you.
    Smile,
    /// The mouth sad, the eyes turned to the viewer's left and frozen on where you were.
    SadAway,
    /// The mouth angry, the eyes back on you.
    Angry,
}

impl Stage {
    fn next(self) -> Stage {
        match self {
            Stage::Smile => Stage::SadAway,
            Stage::SadAway => Stage::Angry,
            Stage::Angry => Stage::Smile,
        }
    }

    /// The mouth, as an index into the atlas's three: smile, sad, angry.
    fn mouth(self) -> usize {
        match self {
            Stage::Smile => 0,
            Stage::SadAway => 1,
            Stage::Angry => 2,
        }
    }

    fn eyes_left(self) -> f32 {
        if self == Stage::SadAway {
            1.0
        } else {
            0.0
        }
    }
}

/// The expression state machine: a [`Stage`], advancing one step round the cycle per
/// unobserved stretch, with a short crossfade from the stage before.
#[derive(Clone, Copy, Debug)]
pub struct Expression {
    stage: Stage,
    /// The stage before the last change, for the crossfade.
    prev: Stage,
    /// Seconds since the last change, capped at [`FADE`].
    faded: f32,
    /// Seconds of the current unobserved stretch.
    unseen: f32,
    /// Whether this stretch has already had its one change.
    flipped: bool,
}

impl Default for Expression {
    fn default() -> Expression {
        Expression {
            stage: Stage::Smile,
            prev: Stage::Smile,
            faded: FADE,
            unseen: 0.0,
            flipped: false,
        }
    }
}

impl Expression {
    /// One fixed step: `observed` is this step's answer, `dt` its length in seconds.
    pub fn step(&mut self, observed: bool, dt: f32) {
        self.faded = (self.faded + dt).min(FADE);
        if observed {
            self.unseen = 0.0;
            self.flipped = false;
            return;
        }
        self.unseen += dt;
        if !self.flipped && self.unseen > GRACE {
            self.prev = self.stage;
            self.stage = self.stage.next();
            self.faded = 0.0;
            self.flipped = true;
        }
    }

    #[cfg(test)]
    pub fn stage(&self) -> Stage {
        self.stage
    }

    /// How far the crossfade from `prev` to `stage` has come, 0..1, eased.
    fn blend(&self) -> f32 {
        let t = (self.faded / FADE).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    /// The three mouths' weights -- smile, sad, angry -- summing to one: the shader's
    /// `mouth_w`.
    pub fn mouth_weights(&self) -> [f32; 3] {
        let t = self.blend();
        let mut w = [0.0; 3];
        w[self.prev.mouth()] += 1.0 - t;
        w[self.stage.mouth()] += t;
        w
    }

    /// How far the eyes have turned to the left-looking variant, 0..1.
    pub fn eyes_left(&self) -> f32 {
        let t = self.blend();
        self.prev.eyes_left() * (1.0 - t) + self.stage.eyes_left() * t
    }

    /// Whether the eyes are turned away and frozen on the last sighting rather than
    /// following the viewer: the [`Stage::SadAway`] stage, from its change on.
    pub fn eyes_frozen(&self) -> bool {
        self.stage == Stage::SadAway
    }
}

/// Where the eyes aim: the viewer, while watched; where the viewer was last seen from, while
/// not; and a slide between the two on being looked at again.
#[derive(Clone, Copy, Debug, Default)]
pub struct Gaze {
    /// Where the slide starts from: the viewer's eye as of the last settled observation, or
    /// as of the last sighting once it ended. `None` before the first look.
    anchor: Option<Vector3>,
    /// The viewer's eye as of this observed stretch's latest step, for the anchor to take
    /// when the stretch ends.
    last_seen: Option<Vector3>,
    /// Seconds of the current observed stretch, capped at [`REACQUIRE`].
    settled: f32,
}

impl Gaze {
    /// One fixed step: `seen_from` is the viewer's eye in the painting's world if the painting
    /// is observed this step. While watched, the anchor only follows the viewer once the
    /// slide is over, so a slide is always from the old anchor and never chases its own tail;
    /// when the look ends, the anchor is where the viewer was last seen from -- however short
    /// the look -- which is what the eyes then stay on.
    pub fn step(&mut self, seen_from: Option<Vector3>, dt: f32) {
        match seen_from {
            Some(from) => {
                self.settled = (self.settled + dt).min(REACQUIRE);
                if self.settled >= REACQUIRE || self.anchor.is_none() {
                    self.anchor = Some(from);
                }
                self.last_seen = Some(from);
            }
            None => {
                if let Some(last) = self.last_seen.take() {
                    self.anchor = Some(last);
                }
                self.settled = 0.0;
            }
        }
    }

    /// The point the eyes aim at for a render pass whose eye is `eye`.
    pub fn target(&self, eye: Vector3) -> Vector3 {
        let Some(anchor) = self.anchor else { return eye };
        let t = (self.settled / REACQUIRE).clamp(0.0, 1.0);
        let t = t * t * (3.0 - 2.0 * t);
        anchor + (eye - anchor) * t
    }

    /// Where the viewer was last seen from, with no slide toward `eye`: the eyes frozen
    /// there ([`Stage::SadAway`]). `eye` itself before the first look.
    pub fn remembered(&self, eye: Vector3) -> Vector3 {
        self.anchor.unwrap_or(eye)
    }
}

/// The iris offsets for a viewer at `v` in canvas metres (x along the wall, y up, z out of
/// the canvas toward the room), for a canvas `size` metres wide and high with its eye
/// openings at `eyes` (base UV: centre, radii; left then right, `Portrait::eyes`). Per eye:
/// the tangent of the angle from the opening's centre to the viewer, times `GAZE_K` of the
/// opening's width, clamped to the `GAZE_LIMIT` ellipse, in base UV with v down -- the
/// shader's `gaze` -- and how far toward the left-looking variant the look calls for, 0 up
/// to the clamp and 1 at `LEFT_FULL` times it. Both eyes aim at the same point, so the
/// nearer eye turns a little further.
pub fn iris_offsets(v: Vector3, size: (f32, f32), eyes: &[[f32; 4]; 2]) -> ([f32; 4], f32) {
    let (w, h) = size;
    let vz = v.z.max(GAZE_MIN_Z);
    let mut out = [0.0; 4];
    let mut left: f32 = 0.0;
    for (i, &[cx, cy, rx, ry]) in eyes.iter().enumerate() {
        let (ex, ey) = ((cx - 0.5) * w, (0.5 - cy) * h);
        let (width, height) = (2.0 * rx * w, 2.0 * ry * h);
        let k = GAZE_K * width;
        let (gx, gy) = ((v.x - ex) / vz * k, (v.y - ey) / vz * k);
        let (lx, ly) = (GAZE_LIMIT.0 * width, GAZE_LIMIT.1 * height);
        let n = ((gx / lx).powi(2) + (gy / ly).powi(2)).sqrt();
        let s = if n > 1.0 { 1.0 / n } else { 1.0 };
        out[2 * i] = gx * s / w;
        out[2 * i + 1] = -gy * s / h;
        let over = ((-gx / lx - 1.0) / (LEFT_FULL - 1.0)).clamp(0.0, 1.0);
        left = left.max(over * over * (3.0 - 2.0 * over));
    }
    (out, left)
}

/// What a painting looks through to decide whether it is being looked at: the scene's solid
/// objects and its portals, snapshotted at load. Cloned into every painting; the snapshots are
/// shared.
#[derive(Clone)]
pub struct Watch {
    blockers: Rc<[Rc<RefCell<dyn ObjectT>>]>,
    portals: Rc<[Rc<RefCell<Portal>>]>,
    /// The doors the portals belong to, when they can vanish (`DoorLink::vanish`): the engine
    /// drops their portals then, and the snapshot must stop looking through them.
    doors: Option<DoorLink>,
}

impl Watch {
    /// Snapshot the objects and portals built so far. The paintings are built after it, so
    /// they are not in the list; their canvas rectangles would not matter to the ray anyway,
    /// which stops short of a probe point that already stands off the canvas (`PROBE_OUT`).
    pub fn new(objs: &PObjectVec, portals: &PPortalVec) -> Watch {
        Watch {
            blockers: Rc::from(objs.as_slice()),
            portals: Rc::from(portals.as_slice()),
            doors: None,
        }
    }

    /// The same watch, looking through its portals only while the doors on `link` stand: once
    /// they have vanished (the Backrooms' one-way door, `level16.rs`) nothing is seen through
    /// a doorway that is no longer there.
    pub fn while_doors_stand(mut self, link: DoorLink) -> Watch {
        self.doors = Some(link);
        self
    }

    /// If `point` is observed from the player's eye transform, the eye's position in the
    /// point's own world: the player's, or the player's warped through the portal it is seen
    /// through. That is the position a render pass drawing the point would have as its eye.
    pub fn seen_from(&self, cam_to_world: &Matrix4, point: Vector3) -> Option<Vector3> {
        if in_view_cone(cam_to_world, point, WATCH_HALF_ANGLE)
            && has_line_of_sight(&self.blockers, cam_to_world, point, None)
        {
            return Some(cam_to_world.translation());
        }
        if self.doors.as_ref().is_some_and(DoorLink::vanished) {
            return None;
        }
        self.portals.iter().find_map(|p| {
            // `try_borrow`: this runs inside the update loop, where a portal is never borrowed,
            // but a miss is the right answer if one ever were.
            let p = p.try_borrow().ok()?;
            self.through(&p, cam_to_world, point)
        })
    }

    /// The portal half of `seen_from`; see the module docs.
    fn through(&self, portal: &Portal, cam_to_world: &Matrix4, point: Vector3) -> Option<Vector3> {
        let eye = cam_to_world.translation();
        let here = portal.base.pos;
        // The side the eye is on picks the warp, exactly as `Portal::draw` picks its camera.
        let normal = portal.base.forward();
        let warp = if (eye - here).dot(normal) > 0.0 { &portal.front } else { &portal.back };
        warp.to_portal?;
        if !beyond(here, warp, point) {
            return None;
        }
        let image = warp.delta.mul_point(point);
        if !in_view_cone(cam_to_world, image, WATCH_HALF_ANGLE) {
            return None;
        }
        // Through the opening, not past its edge: the line to the image must cross the quad.
        portal.intersects(eye, image, Vector3::zero())?;
        let da = normal.dot(eye - here);
        let db = normal.dot(image - here);
        let crossing = eye + (image - eye) * (da / (da - db));
        if !has_line_of_sight(&self.blockers, cam_to_world, crossing, None) {
            return None;
        }
        // The far half starts where the line comes out of the far portal, NOT at the warped
        // eye: a player a stride outside the meadow door is a stride behind the far door,
        // which is inside the building's end wall, and a ray from there hits the wall's back.
        let warped = warp.delta_inv * *cam_to_world;
        let far_side = Matrix4::trans(warp.delta_inv.mul_point(crossing));
        has_line_of_sight(&self.blockers, &far_side, point, None).then(|| warped.translation())
    }
}

/// Whether `point` is on the far side of the portal standing at `here` whose `warp` leads
/// through it: nearer the far end -- where this end lands through the warp -- than this end.
fn beyond(here: Vector3, warp: &Warp, point: Vector3) -> bool {
    let far_end = warp.delta_inv.mul_point(here);
    (point - far_end).mag_sq() < (point - here).mag_sq()
}

/// Seconds the key takes to come out of the canvas, and to sink back.
pub const EMERGE: f32 = 0.5;
/// How far off the canvas the key floats once it is out, measured along the wall's normal:
/// its half-length and a margin, so that nothing of it is left in the wall. It does not
/// travel along the normal, though, but toward the sweet spot (`KeyRig::out`): the view from
/// the spot is so grazing that a key ten centimetres straight out of the wall is seen against
/// the wall seventy centimetres further along, past the frame -- and that is where the grab's
/// placement ray, cast through the key, would hit and put it once taken: behind the frame's
/// upright, gone as far as the player can tell. Coming out along the line of sight the key
/// stays in front of the spot where it was painted, the ray through it hits the canvas (a
/// collider, `Painting::canvas`), and the taken key is held just off the picture.
pub const KEY_OUT: f32 = 0.07;
/// Where the key sits on the picture plane, in its (u, v) metres from the canvas centre:
/// across the sitter's bodice -- on the Mona Lisa, below the neckline's embroidery and
/// above the folded arms, where the dress is dark and gold reads; lower, the smear ran
/// over the gold sleeve -- and a little toward the far end of the canvas. The sweet spot's
/// view is grazing enough that the frame's near upright hides the canvas past x = 0.18 of
/// its 0.4 half-width, and the far end stretches more than the near one, so a 9 cm key
/// centred a centimetre toward the far end is what fits (`tools/gen_key.py`). THE SAME
/// NUMBERS AS `KEY_ON_PLANE` in `Shaders/painting.frag`.
pub const KEY_ON_PLANE: (f32, f32) = (-0.010, -0.16);
/// The 3D key is pitched this far about its length, its face turned up toward the light --
/// the `prop` shader's hemisphere is lit from above (Shaders/prop.frag): facing the sweet
/// spot squarely, edge-on to the lamps, it would come out of the canvas dull. A quarter of a
/// right angle costs a tenth of its apparent breadth from the sweet spot and doubles its
/// light.
pub const KEY_PITCH: f32 = 25.0 * std::f32::consts::PI / 180.0;
/// What the HUD says while the player stands in the sweet spot and the key is still there.
pub const TAKE_HINT: &str = "TAKE THE KEY";

/// A painting's key: where it is seen from, and how exactly the player has to stand there.
#[derive(Clone, Copy, Debug)]
pub struct KeySpec {
    /// The sweet spot, world space: where the eye must be.
    pub view: Vector3,
    /// How far from it the eye may be, in metres.
    pub radius: f32,
    /// How far off the canvas centre the look may be, in radians (a half-angle).
    pub cone: f32,
}

/// Where the key goes as it emerges, relative to its painted spot `rest`: along the line to
/// the sweet spot `view`, as far as it takes to stand [`KEY_OUT`] off the wall whose normal
/// is `facing` (see [`KEY_OUT`] for why not along the normal). A spot in the wall's own plane
/// -- no such painting; a guard -- sends it straight out instead.
pub fn emergence_path(rest: Vector3, view: Vector3, facing: Vector3) -> Vector3 {
    let facing = facing.normalized_safe();
    let to_view = (view - rest).normalized_safe();
    let along = to_view.dot(facing);
    if along < 0.05 {
        return facing * KEY_OUT;
    }
    to_view * (KEY_OUT / along)
}

/// Whether the player is in the sweet spot: eye within `radius` of the view point and the
/// canvas centre within `cone` of the look direction.
pub fn armed(cam_to_world: &Matrix4, spec: &KeySpec, centre: Vector3) -> bool {
    (cam_to_world.translation() - spec.view).mag() <= spec.radius
        && in_view_cone(cam_to_world, centre, spec.cone)
}

/// The picture plane's frame for a sweet spot `view` in canvas-plane metres (the canvas is
/// z = 0, z toward the room): `n` from the canvas centre to the eye, `u` horizontal across
/// it, `v` the nearest thing to up. Mirrors `anamorph` in `Shaders/painting.frag`.
pub fn plane_basis(view: Vector3) -> (Vector3, Vector3, Vector3) {
    let n = view.normalized();
    let u = Vector3::unit_y().cross(n).normalized();
    let v = n.cross(u);
    (u, v, n)
}

/// Where the ray from `view` through the canvas point `p` meets the picture plane, in the
/// plane's (u, v) metres: the sum `anamorph` in the shader does per fragment, here so the
/// tests can check it against `to_canvas`, which is the one the scene code needs.
#[cfg(test)]
pub fn to_plane(view: Vector3, p: (f32, f32)) -> (f32, f32) {
    let (u, v, n) = plane_basis(view);
    let p = Vector3::new(p.0, p.1, 0.0);
    let t = n.dot(view) / n.dot(view - p);
    let q = view + (p - view) * t;
    (q.dot(u), q.dot(v))
}

/// The inverse: where the picture-plane point `q` is painted on the canvas -- the ray from
/// `view` through it, carried on to z = 0.
pub fn to_canvas(view: Vector3, q: (f32, f32)) -> (f32, f32) {
    let (u, v, _) = plane_basis(view);
    let q = u * q.0 + v * q.1;
    let d = q - view;
    let t = -view.z / d.z;
    let p = view + d * t;
    (p.x, p.y)
}

/// What the key is doing, as a state machine over the sweet spot and the grab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyPhase {
    /// On the canvas, paint only.
    Painted,
    /// The real key coming out, the paint fading.
    Emerging,
    /// Out, floating in front of the canvas, waiting to be taken.
    Out,
    /// The player left the spot: going back in, the paint returning.
    Sinking,
    /// In the player's hand, or wherever it went from there: nothing left on the canvas.
    Taken,
}

/// What the painting must do about a step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyEvent {
    /// Put the key object in the scene.
    Spawn,
    /// Take it out again.
    Remove,
}

/// The emergence: a phase and how far out the key is, `0..=EMERGE` seconds.
#[derive(Clone, Copy, Debug)]
pub struct Emergence {
    phase: KeyPhase,
    t: f32,
}

impl Default for Emergence {
    fn default() -> Emergence {
        Emergence { phase: KeyPhase::Painted, t: 0.0 }
    }
}

impl Emergence {
    /// One fixed step: `armed` is whether the player is in the sweet spot, `taken` whether
    /// the key has been grabbed. A take wins over everything and is final; otherwise arming
    /// drives the key out and disarming lets it sink, reversible at any point, and the key
    /// object is in the scene from the first step out to the last step back.
    pub fn step(&mut self, armed: bool, taken: bool, dt: f32) -> Option<KeyEvent> {
        use KeyPhase::*;
        if taken {
            self.phase = Taken;
            self.t = EMERGE;
            return None;
        }
        match self.phase {
            Painted if armed => {
                self.phase = Emerging;
                self.t = 0.0;
                return Some(KeyEvent::Spawn);
            }
            Painted | Taken => {}
            Emerging | Out if !armed => self.phase = Sinking,
            Emerging => {
                self.t += dt;
                if self.t >= EMERGE {
                    self.t = EMERGE;
                    self.phase = Out;
                }
            }
            Out => {}
            Sinking if armed => self.phase = Emerging,
            Sinking => {
                self.t -= dt;
                if self.t <= 0.0 {
                    self.t = 0.0;
                    self.phase = Painted;
                    return Some(KeyEvent::Remove);
                }
            }
        }
        None
    }

    pub fn phase(&self) -> KeyPhase {
        self.phase
    }

    /// How far out the key is, 0 (painted) to 1 (out, or taken): the real key's scale and
    /// the painted one's fade.
    pub fn blend(&self) -> f32 {
        self.t / EMERGE
    }

    /// Whether the key object is in the scene and the painting's to move.
    pub fn floating(&self) -> bool {
        matches!(self.phase, KeyPhase::Emerging | KeyPhase::Out | KeyPhase::Sinking)
    }
}

/// A painting's key, built at load and kept: the spec, the key object and where it goes.
struct KeyRig {
    spec: KeySpec,
    /// The sweet spot in canvas-plane metres: the shader's `key_view`.
    view_local: Vector3,
    /// Where the painted key is, on the canvas, in the world; and where the real one is
    /// relative to that when fully emerged: toward the sweet spot, far enough to clear the
    /// wall by [`KEY_OUT`].
    rest: Vector3,
    out: Vector3,
    key: Rc<RefCell<Key>>,
    /// Set by the key's `on_grab`.
    taken: Rc<Cell<bool>>,
    emergence: Emergence,
    /// Armed and not yet taken, as of the last step: the paint catches the light.
    glint: bool,
}

pub struct Painting {
    /// The canvas's placement, and as its mesh the canvas as a rectangle collider only: what
    /// a ray (`ext/raycast.rs`) and the grab's fit stop at. The portrait itself is drawn from
    /// `quad` through the painting shader. Without the collider the grab's placement ray goes
    /// through the picture to the scanned wall behind it, and the key taken from the sweet
    /// spot is put there -- behind the canvas, gone as far as the player can see.
    canvas: Object,
    /// The ported `quad.obj`, drawn with `canvas`'s transform.
    quad: Rc<Mesh>,
    bars: [Object; 4],
    gl: Rc<glow::Context>,
    /// Which painting, and its two textures.
    portrait: &'static Portrait,
    base: Rc<Texture>,
    parts: Rc<Texture>,
    /// The portrait's piece rects, flattened for the shader's uniform arrays: the four eye
    /// variants' single pieces at 0..4, then every mouth piece in variant order, each an
    /// atlas rect and a placement rect; and the two eye ellipses. `mouth_bounds` fences the
    /// mouth pieces per variant -- variant `v` is indices `bounds[v]..bounds[v + 1]`, with
    /// `bounds[0]` = 4 -- so the shader can composite a variant's pieces over one another
    /// (in table order, last on top) before weighting the variant.
    part_atlas: [f32; 4 * MAX_PIECES],
    part_place: [f32; 4 * MAX_PIECES],
    mouth_bounds: [f32; 4],
    eyes: [f32; 8],
    /// World to canvas metres, without the canvas's scale: what the gaze wants.
    rigid_w2l: Matrix4,
    size: (f32, f32),
    watch: Watch,
    /// The point the observed test asks about.
    probe: Vector3,
    /// Steps taken, for the test's cadence; starts on a per-painting phase.
    steps: u32,
    /// The last test's answer, held between tests.
    seen_from: Option<Vector3>,
    expression: Expression,
    gaze: Gaze,
    centre: Vector3,
    key: Option<KeyRig>,
}

impl Painting {
    /// The canvas size for a portrait hung `width` metres wide: its height follows the base
    /// texture's aspect.
    pub fn size_for(portrait: &Portrait, width: f32) -> (f32, f32) {
        let (w, h) = portrait.base_size;
        (width, width * h as f32 / w as f32)
    }

    /// A `width` metre wide portrait of `portrait` centred at `centre`, hanging on a wall
    /// whose normal (toward the room) is `facing`; `phase` staggers its observation tests and
    /// `watch` is what it decides "being looked at" through. With a `key`, the sitter wears
    /// one (module docs).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        gl: &Rc<glow::Context>,
        res: &Resources,
        centre: Vector3,
        facing: Vector3,
        width: f32,
        portrait: &'static Portrait,
        phase: u32,
        watch: Watch,
        key: Option<KeySpec>,
    ) -> Painting {
        let yaw = yaw_facing(facing);
        // The painting's own frame: x along the wall, y up, z off the wall toward the room.
        let rigid = Matrix4::trans(centre) * Matrix4::rot_y(yaw);
        let rigid_w2l = Matrix4::rot_y(-yaw) * Matrix4::trans(-centre);
        let size = Painting::size_for(portrait, width);
        let (w, h) = size;

        let key = key.map(|spec| {
            // The sweet spot in the canvas plane's own metres (the quad is CANVAS_DEPTH off
            // the frame's origin), and the picture plane's frame through it.
            let view_local = rigid_w2l.mul_point(spec.view) - Vector3::new(0.0, 0.0, CANVAS_DEPTH);
            let (u, v, n) = plane_basis(view_local);
            // The key rests where it is painted: the picture-plane point's image on the
            // canvas, which is where the smear is centred.
            let (px, py) = to_canvas(view_local, KEY_ON_PLANE);
            let rest = rigid.mul_point(Vector3::new(px, py, CANVAS_DEPTH));
            let out = emergence_path(rest, spec.view, facing);
            // The key lies in the picture plane -- its length along u, its breadth along v,
            // facing the sweet spot -- so from there it is the painted key, stepped off the
            // canvas; less the pitch about u that turns its face to the light (`KEY_PITCH`).
            let (sin, cos) = KEY_PITCH.sin_cos();
            let mut rot = Matrix4::identity();
            rot.set_x_axis(rigid.mul_direction(u));
            rot.set_y_axis(rigid.mul_direction(v * cos - n * sin));
            rot.set_z_axis(rigid.mul_direction(n * cos + v * sin));
            let taken = Rc::new(Cell::new(false));
            let key = Key::new(res, taken.clone());
            key.borrow_mut().place(rest, rot);
            KeyRig {
                spec,
                view_local,
                rest,
                out,
                key,
                taken,
                emergence: Emergence::default(),
                glint: false,
            }
        });

        let quad = res.acquire_mesh("quad.obj");
        let mut canvas = Object::new();
        // The quad's rectangle, in the canvas's own space where the quad spans +-1 and the
        // scale below makes it the picture's size; the quad's radius so the cull still sees
        // something to draw (`object_sphere`).
        let mut collider = Mesh::colliders_only(
            gl,
            vec![Collider::rect(Vector3::zero(), Vector3::unit_x(), Vector3::unit_y())],
        );
        collider.bound_radius = quad.bound_radius;
        canvas.mesh = Some(Rc::new(collider));
        canvas.shader = Some(res.acquire_shader("painting"));
        canvas.pos = rigid.mul_point(Vector3::new(0.0, 0.0, CANVAS_DEPTH));
        canvas.euler.y = yaw;
        canvas.scale = Vector3::new(0.5 * w, 0.5 * h, 1.0);

        // Four bars, each a cube scaled to a bar and rolled a quarter-right-angle about its
        // length (`BAR_HALF`). The uprights run the full height plus their reach so the corners
        // close; the rails run to the uprights' centrelines, where their ends are inside the
        // uprights' sections.
        let bar = |at: Vector3, half: Vector3, roll_x: f32, roll_y: f32| {
            let mut o = Object::new();
            o.mesh = Some(res.acquire_mesh("cube.obj"));
            o.shader = Some(res.acquire_shader("texture"));
            o.texture = Some(res.acquire_texture("gold.bmp", 1, 1));
            o.pos = rigid.mul_point(at);
            o.euler.x = roll_x;
            o.euler.y = yaw + roll_y;
            o.scale = half;
            o
        };
        let roll = std::f32::consts::FRAC_PI_4;
        let z = BAR_REACH;
        let upright = Vector3::new(BAR_HALF, 0.5 * h + BAR_REACH, BAR_HALF);
        let rail = Vector3::new(0.5 * w, BAR_HALF, BAR_HALF);
        let bars = [
            bar(Vector3::new(-0.5 * w, 0.0, z), upright, 0.0, roll),
            bar(Vector3::new(0.5 * w, 0.0, z), upright, 0.0, roll),
            bar(Vector3::new(0.0, 0.5 * h, z), rail, roll, 0.0),
            bar(Vector3::new(0.0, -0.5 * h, z), rail, roll, 0.0),
        ];

        let mut part_atlas = [0.0; 4 * MAX_PIECES];
        let mut part_place = [0.0; 4 * MAX_PIECES];
        let mut fill = |i: usize, part: &crate::ext::portrait_atlas::Part| {
            part_atlas[4 * i..4 * i + 4].copy_from_slice(&part.atlas);
            part_place[4 * i..4 * i + 4].copy_from_slice(&part.place);
        };
        for (i, variant) in portrait.parts[..4].iter().enumerate() {
            assert_eq!(variant.len(), 1, "{}: an eye variant is one piece", portrait.name);
            fill(i, &variant[0]);
        }
        let mut mouth_bounds = [4.0; 4];
        let mut idx = 4;
        for (v, variant) in portrait.parts[4..].iter().enumerate() {
            for part in variant.iter() {
                assert!(idx < MAX_PIECES, "{}: too many pieces", portrait.name);
                fill(idx, part);
                idx += 1;
            }
            mouth_bounds[v + 1] = idx as f32;
        }
        let mut eyes = [0.0; 8];
        eyes[..4].copy_from_slice(&portrait.eyes[0]);
        eyes[4..].copy_from_slice(&portrait.eyes[1]);

        Painting {
            canvas,
            quad,
            bars,
            gl: Rc::clone(gl),
            portrait,
            base: res.acquire_texture(portrait.base, 1, 1),
            parts: res.acquire_texture(portrait.parts_texture, 1, 1),
            part_atlas,
            part_place,
            mouth_bounds,
            eyes,
            rigid_w2l,
            size,
            watch,
            probe: centre + facing.normalized_safe() * PROBE_OUT,
            steps: phase % WATCH_EVERY,
            seen_from: None,
            expression: Expression::default(),
            gaze: Gaze::default(),
            centre,
            key,
        }
    }

    /// The key's step: the sweet spot test, the state machine, and what it asks of the scene
    /// and of the key object -- which is a different cell from this painting's, so borrowing
    /// it here is sound (`try_borrow_mut` all the same, as nothing is worth a panic).
    fn step_key(&mut self, cam_to_world: &Matrix4) {
        let Some(rig) = &mut self.key else { return };
        let armed = armed(cam_to_world, &rig.spec, self.centre);
        let taken = rig.taken.get();
        match rig.emergence.step(armed, taken, GH_DT) {
            Some(KeyEvent::Spawn) => {
                room::request_spawn(rig.key.clone() as Rc<RefCell<dyn ObjectT>>);
            }
            Some(KeyEvent::Remove) => {
                room::request_remove(&(rig.key.clone() as Rc<RefCell<dyn ObjectT>>));
            }
            None => {}
        }
        if rig.emergence.floating() {
            if let Ok(mut key) = rig.key.try_borrow_mut() {
                let blend = rig.emergence.blend();
                key.float_at(rig.rest + rig.out * blend, blend);
            }
        }
        rig.glint = armed && rig.emergence.phase() != KeyPhase::Taken;
        if rig.glint {
            hint::set(TAKE_HINT);
        }
    }
}

impl ObjectT for Painting {
    fn base(&self) -> &Object {
        &self.canvas
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.canvas
    }

    fn update(&mut self, ctx: &UpdateCtx) {
        if self.steps % WATCH_EVERY == 0 {
            self.seen_from = self.watch.seen_from(&ctx.cam_to_world, self.probe);
        }
        self.steps = self.steps.wrapping_add(1);
        self.expression.step(self.seen_from.is_some(), GH_DT);
        self.gaze.step(self.seen_from, GH_DT);
        self.step_key(&ctx.cam_to_world);
    }

    fn draw(&self, ctx: &RenderCtx, cam: &Camera, _fbo: Option<glow::Framebuffer>) {
        // The frame is four ported props; draw_impl culls and lights each as it would any.
        for bar in &self.bars {
            bar.draw_impl(ctx, cam);
        }
        // The canvas: the same sphere cull, then the portrait shader's own uniforms.
        let Some((c, r)) = object_sphere(&self.canvas) else { return };
        if !ctx.frustum.sphere(c, r) {
            return;
        }
        let shader = self.canvas.shader.as_ref().expect("set in new");
        let mesh = &self.quad;
        let local_to_world = self.canvas.local_to_world();
        let mvp = cam.matrix() * local_to_world;
        let eye = ctx.eye;
        // This pass's eye, through the gaze memory -- or frozen on the last sighting while
        // the eyes are turned away -- into canvas metres, and from there the iris offsets.
        let target = if self.expression.eyes_frozen() {
            self.gaze.remembered(eye)
        } else {
            self.gaze.target(eye)
        };
        let v = self.rigid_w2l.mul_point(target);
        let (gaze, look_left) = iris_offsets(v, self.size, &self.portrait.eyes);
        let [smile, sad, angry] = self.expression.mouth_weights();
        shader.use_program();
        shader.set_mvp(Some(&mvp), None);
        shader.set_mat4("model", &local_to_world);
        shader.set_vec4("cam_pos", [eye.x, eye.y, eye.z, 1.0]);
        shader.set_vec4("fog_color", WALL_FOG);
        shader.set_vec4("size", [self.size.0, self.size.1, 0.0, 0.0]);
        // The base on unit 0 and the parts on unit 1; unit 0 is left active, as
        // Object::draw_impl binds its texture with no active_texture call of its own.
        unsafe {
            self.gl.active_texture(glow::TEXTURE1);
            self.parts.use_texture();
            self.gl.active_texture(glow::TEXTURE0);
            self.base.use_texture();
        }
        shader.set_i32("base", 0);
        shader.set_i32("parts", 1);
        shader.set_vec4_array("part_atlas", &self.part_atlas);
        shader.set_vec4_array("part_place", &self.part_place);
        shader.set_vec4("mouth_bounds", self.mouth_bounds);
        shader.set_vec4_array("eye", &self.eyes);
        shader.set_vec4("gaze", gaze);
        shader.set_f32("iris_core", IRIS_CORE);
        shader.set_f32("eye_left", look_left.max(self.expression.eyes_left()));
        shader.set_vec4("mouth_w", [smile, sad, angry, 0.0]);
        // The key: its sweet spot, how far out it is, and whether the paint glints. A
        // portrait without one says so in `key_view.w` and the shader draws nothing.
        let (view, state, glint) = match &self.key {
            Some(rig) => {
                let v = rig.view_local;
                ([v.x, v.y, v.z, 1.0], rig.emergence.blend(), if rig.glint { 1.0 } else { 0.0 })
            }
            None => ([0.0; 4], 1.0, 0.0),
        };
        shader.set_vec4("key_view", view);
        shader.set_f32("key_state", state);
        shader.set_f32("key_glint", glint);
        shader.set_f32("time", crate::ext::view::time());
        mesh.draw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive the state machine with a script of `(observed, seconds)` stretches at the fixed
    /// step, returning the stage after each stretch.
    fn run(script: &[(bool, f32)]) -> Vec<Stage> {
        let mut e = Expression::default();
        let mut out = Vec::new();
        for &(observed, secs) in script {
            let steps = (secs / GH_DT).round() as u32;
            for _ in 0..steps {
                e.step(observed, GH_DT);
            }
            out.push(e.stage());
        }
        out
    }

    #[test]
    fn starts_smiling_and_never_changes_while_watched() {
        use Stage::*;
        assert_eq!(run(&[(true, 10.0)]), [Smile]);
        let e = Expression::default();
        assert_eq!(e.mouth_weights(), [1.0, 0.0, 0.0]);
        assert_eq!(e.eyes_left(), 0.0);
        assert!(!e.eyes_frozen());
    }

    #[test]
    fn changes_once_after_the_grace_and_cycles_per_stretch() {
        use Stage::*;
        // A glance away shorter than the grace changes nothing; a longer one changes it once,
        // however long it goes on; each later stretch takes the next step round the cycle --
        // sad and looking away, angry and looking back, the smile again.
        let out = run(&[
            (false, GRACE * 0.5),
            (true, 0.1),
            (false, GRACE + 0.05),
            (false, 30.0),
            (true, 0.1),
            (false, GRACE + 0.05),
            (true, 0.1),
            (false, GRACE + 0.05),
            (true, 0.1),
            (false, GRACE + 0.05),
        ]);
        assert_eq!(
            out,
            [Smile, Smile, SadAway, SadAway, SadAway, Angry, Angry, Smile, Smile, SadAway]
        );
    }

    #[test]
    fn a_look_resets_the_grace() {
        // Two unobserved stretches each just under the grace, with a look between: no change.
        let out = run(&[(false, GRACE * 0.9), (true, GH_DT), (false, GRACE * 0.9)]);
        assert_eq!(out, [Stage::Smile; 3]);
    }

    /// A change crossfades from the face before over `FADE` -- the weights always sum to
    /// one -- and is settled before the grace could end the stretch it happened in.
    #[test]
    fn a_change_crossfades_and_settles_within_the_grace() {
        let mut e = Expression::default();
        for _ in 0..((GRACE + GH_DT * 1.5) / GH_DT) as u32 {
            e.step(false, GH_DT);
        }
        assert_eq!(e.stage(), Stage::SadAway);
        let w = e.mouth_weights();
        assert!(w[0] > 0.9 && (w.iter().sum::<f32>() - 1.0).abs() < 1e-5, "just changed: {w:?}");
        assert!(e.eyes_left() < 0.1 && e.eyes_frozen());
        for _ in 0..((FADE * 0.5) / GH_DT) as u32 {
            e.step(false, GH_DT);
        }
        let w = e.mouth_weights();
        assert!(
            w[0] > 0.3 && w[1] > 0.3 && (w.iter().sum::<f32>() - 1.0).abs() < 1e-5,
            "mid-fade: {w:?}"
        );
        assert!(e.eyes_left() > 0.3 && e.eyes_left() < 0.7);
        for _ in 0..((FADE * 0.6) / GH_DT) as u32 {
            e.step(false, GH_DT);
        }
        assert_eq!(e.mouth_weights(), [0.0, 1.0, 0.0]);
        assert_eq!(e.eyes_left(), 1.0);
        const { assert!(FADE < GRACE) };
        // The next change, to angry, brings the eyes back: frozen no more.
        for _ in 0..((GRACE * 2.0) / GH_DT) as u32 {
            e.step(true, GH_DT);
        }
        for _ in 0..((GRACE + FADE * 2.0) / GH_DT) as u32 {
            e.step(false, GH_DT);
        }
        assert_eq!(e.stage(), Stage::Angry);
        assert_eq!(e.mouth_weights(), [0.0, 0.0, 1.0]);
        assert!(e.eyes_left() == 0.0 && !e.eyes_frozen());
    }

    /// The Mona Lisa's eyes, on a canvas 0.8 m wide, as `iris_offsets` sees them: both a
    /// little above the centre, the left one at u 0.37 and the right at 0.47, openings two
    /// and a half to three and a half percent of the width in half-width.
    const EYES: [[f32; 4]; 2] = [[0.371, 0.174, 0.0257, 0.0080], [0.473, 0.173, 0.0342, 0.0095]];
    const SIZE: (f32, f32) = (0.8, 0.9365);

    #[test]
    fn the_irises_follow_the_viewer_and_converge() {
        // Square on and far off: no offset, and no call for the left variant.
        let (g, left) = iris_offsets(Vector3::new(0.0, 0.15, 5.0), SIZE, &EYES);
        assert!(g.iter().all(|x| x.abs() < 2e-3), "{g:?}");
        assert_eq!(left, 0.0);
        // A viewer a metre off the canvas, half a metre to the right (+x) and level with the
        // eyes (0.305 up): both irises go right (+u), the right eye -- nearer the viewer --
        // further, and nothing up or down to speak of; the left variant is not called for.
        let (g, left) = iris_offsets(Vector3::new(0.5, 0.305, 1.0), SIZE, &EYES);
        assert!(g[0] > 0.0 && g[2] > 0.0, "{g:?}");
        assert!(g[2] > g[0], "the nearer eye turns further: {g:?}");
        assert!(g[1].abs() < 1e-3 && g[3].abs() < 1e-3, "{g:?}");
        assert_eq!(left, 0.0);
        // A viewer above: the irises go up, which in top-origin UV is -v.
        let (g, _) = iris_offsets(Vector3::new(0.0, 1.0, 1.0), SIZE, &EYES);
        assert!(g[1] < 0.0 && g[3] < 0.0, "{g:?}");
        // The clamp: a viewer level with the canvas, far to the right, pins each iris at
        // GAZE_LIMIT of its eye's width and no further.
        let (g, _) = iris_offsets(Vector3::new(3.0, 0.305, 0.0), SIZE, &EYES);
        for (i, e) in EYES.iter().enumerate() {
            let width = 2.0 * e[2];
            assert!((g[2 * i] - GAZE_LIMIT.0 * width).abs() < 1e-5, "eye {i}: {g:?}");
        }
        // To the left, past the clamp, the left-looking eyes take over, fully at LEFT_FULL
        // times the limit; a look the warp carries does not call for them at all.
        let (g, left) = iris_offsets(Vector3::new(-3.0, 0.305, 0.0), SIZE, &EYES);
        assert!(g[0] < 0.0 && left == 1.0, "{g:?} {left}");
        let (_, left) = iris_offsets(Vector3::new(-0.2, 0.305, 2.0), SIZE, &EYES);
        assert_eq!(left, 0.0);
        let (_, mid) = iris_offsets(Vector3::new(-0.6, 0.305, 1.0), SIZE, &EYES);
        assert!(mid > 0.0 && mid < 1.0, "part way to the left variant: {mid}");
    }

    /// The warp is `uv - g * w(r)` with `w = 1 - smoothstep(IRIS_CORE, 1, r)`: it folds --
    /// two source points land on one -- if the offset times the ease's steepest slope
    /// reaches the radius along it. Neither limit does, with margin.
    #[test]
    fn the_warp_never_folds() {
        let slope = 1.5 / (1.0 - IRIS_CORE);
        // Across: the offset is GAZE_LIMIT.0 of the width, 2 * GAZE_LIMIT.0 of the radius.
        let across = 2.0 * GAZE_LIMIT.0 * slope;
        let up = 2.0 * GAZE_LIMIT.1 * slope;
        assert!(across < 0.7, "across: {across} of the radius per unit");
        assert!(up < 0.7, "up: {up}");
        // And the shader takes the same core.
        let frag = std::fs::read_to_string(crate::app::assets::path("Shaders/painting.frag"))
            .expect("Shaders/painting.frag");
        assert!(frag.contains("smoothstep(iris_core, 1.0, r)"));
    }

    /// The generated atlas (`tools/gen_portraits.py`): every piece's atlas rect lies inside
    /// its texture and its placement rect inside the base, each eye variant is a single
    /// piece with its eye's opening inside it, the pieces fit the shader's arrays, and on
    /// the base an eye piece never overlaps a mouth piece (an eye and a mouth really are
    /// summed; everything else crossfades, partitions or composites over).
    #[test]
    fn the_portrait_atlas_is_consistent() {
        use crate::ext::portrait_atlas::{PART_ORDER, PORTRAITS};
        use crate::texture::decode_bmp;
        let inside = |r: &[f32; 4]| {
            r[0] >= 0.0 && r[1] >= 0.0 && r[2] <= 1.0 && r[3] <= 1.0 && r[0] < r[2] && r[1] < r[3]
        };
        let overlap =
            |a: &[f32; 4], b: &[f32; 4]| a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3];
        // The three sitters the sheets cut, the Mona first: `level16` hangs them by name and
        // pins the key's frame to her. Named here so a portrait cannot quietly drop out of
        // the table and leave the rest of this test passing over what is left.
        let sitters: Vec<&str> = PORTRAITS.iter().map(|p| p.name).collect();
        assert_eq!(sitters, ["mona", "vermeer", "cavalier"]);
        for p in &PORTRAITS {
            for tex in [p.base, p.parts_texture] {
                let bytes =
                    std::fs::read(crate::app::assets::path(&format!("Textures/{tex}"))).expect(tex);
                let bmp = decode_bmp(&bytes, 1, 1).expect(tex);
                let expect = if tex == p.base { p.base_size } else { p.parts_size };
                assert_eq!((bmp.width as u32, bmp.height as u32), expect, "{tex}");
                assert_eq!(bmp.bpp, 32, "{tex}");
            }
            let pieces: usize = p.parts.iter().map(|v| v.len()).sum();
            assert!(pieces <= MAX_PIECES, "{}: {pieces} pieces", p.name);
            for (i, variant) in p.parts.iter().enumerate() {
                assert!(!variant.is_empty(), "{} {}: no pieces", p.name, PART_ORDER[i]);
                if i < 4 {
                    // The iris warp slides one rect: an eye variant is always one piece.
                    assert_eq!(variant.len(), 1, "{} {}", p.name, PART_ORDER[i]);
                }
                for part in variant.iter() {
                    assert!(
                        inside(&part.atlas),
                        "{} {}: atlas {:?}",
                        p.name,
                        PART_ORDER[i],
                        part.atlas
                    );
                    assert!(
                        inside(&part.place),
                        "{} {}: place {:?}",
                        p.name,
                        PART_ORDER[i],
                        part.place
                    );
                }
                for (j, other) in p.parts.iter().enumerate().skip(i + 1) {
                    let same_kind = (i < 4) == (j < 4);
                    if same_kind {
                        continue;
                    }
                    for a in variant.iter() {
                        for b in other.iter() {
                            assert!(
                                !overlap(&a.place, &b.place),
                                "{}: {} overlaps {}",
                                p.name,
                                PART_ORDER[i],
                                PART_ORDER[j]
                            );
                        }
                    }
                }
            }
            for (i, &[cx, cy, rx, ry]) in p.eyes.iter().enumerate() {
                for variant in [0, 2] {
                    let r = &p.parts[variant + i][0].place;
                    assert!(
                        cx - rx > r[0] && cx + rx < r[2] && cy - ry > r[1] && cy + ry < r[3],
                        "{}: eye {i} ({cx}, {cy}, {rx}, {ry}) outside part {}",
                        p.name,
                        PART_ORDER[variant + i]
                    );
                }
                assert!(rx > ry, "{}: an eye opening is wider than it is high", p.name);
            }
            // The left eye is left of the right one, and every mouth piece's centre below
            // both eye centres (only the centres are ordered, not the boxes: a mouth crop
            // reaches up the nose, and the Hals moustache reaches out past the cheeks).
            assert!(p.eyes[0][0] < p.eyes[1][0]);
            for variant in &p.parts[4..] {
                for part in variant.iter() {
                    let centre_y = 0.5 * (part.place[1] + part.place[3]);
                    assert!(centre_y > p.eyes[0][1] && centre_y > p.eyes[1][1], "{}", p.name);
                }
            }
        }
    }

    #[test]
    fn gaze_is_live_until_first_seen_then_remembers_and_slides() {
        let mut g = Gaze::default();
        let eye = Vector3::new(1.0, 1.5, 2.0);
        // Never seen: live.
        assert!((g.target(eye) - eye).mag() < 1e-6);
        // Seen from `a` long enough to settle: the anchor is `a`.
        let a = Vector3::new(0.0, 1.5, 0.0);
        for _ in 0..((REACQUIRE * 2.0) / GH_DT) as u32 {
            g.step(Some(a), GH_DT);
        }
        // Unobserved: aimed at `a`, whatever this pass's eye is.
        g.step(None, GH_DT);
        assert!((g.target(eye) - a).mag() < 1e-6);
        // Observed again from `eye`: halfway through the slide it is between the two, and at
        // the end it is on the viewer.
        for _ in 0..((REACQUIRE * 0.5) / GH_DT) as u32 {
            g.step(Some(eye), GH_DT);
        }
        let mid = g.target(eye);
        let along = (mid - a).dot((eye - a).normalized()) / (eye - a).mag();
        assert!(along > 0.3 && along < 0.7, "slide at {along}");
        for _ in 0..((REACQUIRE * 0.6) / GH_DT) as u32 {
            g.step(Some(eye), GH_DT);
        }
        assert!((g.target(eye) - eye).mag() < 1e-6);
    }

    /// The memory is the LAST sighting, however short: a glance from `b` after a long look
    /// from `a` leaves the eyes on `b` once it ends, and the next slide starts there.
    #[test]
    fn a_short_look_is_remembered_too() {
        let mut g = Gaze::default();
        let a = Vector3::new(0.0, 1.5, 0.0);
        let b = Vector3::new(3.0, 1.5, 1.0);
        for _ in 0..((REACQUIRE * 2.0) / GH_DT) as u32 {
            g.step(Some(a), GH_DT);
        }
        g.step(None, GH_DT);
        // A look from `b` shorter than the slide: mid-slide the eyes are between the two...
        for _ in 0..((REACQUIRE * 0.5) / GH_DT) as u32 {
            g.step(Some(b), GH_DT);
        }
        let mid = g.target(b);
        assert!((mid - a).mag() > 0.3 && (mid - b).mag() > 0.3, "mid-slide at {mid:?}");
        // ...and once it ends they stay on `b`, not `a`.
        g.step(None, GH_DT);
        assert!((g.target(a) - b).mag() < 1e-6, "remembered {:?}", g.target(a));
        // Looked at again from `a`, the slide starts at `b`.
        g.step(Some(a), GH_DT);
        assert!((g.target(a) - b).mag() < 0.05);
    }

    /// The watch tests need portals with warps and no GL: `Portal::detached` and `connect`
    /// give both. A door at the origin facing +z, linked to one at `FAR` facing +x, the way
    /// the Backrooms are wired (`level16::walking_through_lands_in_the_hall_facing_down_it`).
    fn two_doors() -> (Watch, Vector3, Rc<RefCell<Portal>>, Rc<RefCell<Portal>>) {
        use crate::ext::door::portal_placement;
        use crate::portal::connect;
        let portal = |pos: Vector3, facing: Vector3| {
            let (centre, euler, scale) = portal_placement(pos, yaw_facing(facing));
            let mut p = Portal::detached();
            p.base.pos = centre;
            p.base.euler = euler;
            p.base.scale = scale;
            Rc::new(RefCell::new(p))
        };
        let far = Vector3::new(1000.0, 0.0, 0.0);
        let here = portal(Vector3::zero(), Vector3::new(0.0, 0.0, 1.0));
        let there = portal(far, Vector3::new(1.0, 0.0, 0.0));
        connect(&here, &there);
        let watch = Watch::new(&Vec::new(), &vec![here.clone(), there.clone()]);
        (watch, far, here, there)
    }

    /// A camera at `eye` looking along `dir`, as `Player::cam_to_world` would build it.
    fn camera(eye: Vector3, dir: Vector3) -> Matrix4 {
        Matrix4::trans(eye) * Matrix4::rot_y(-dir.x.atan2(-dir.z))
    }

    #[test]
    fn seen_directly_answers_the_eye_itself() {
        let (watch, far, _, _) = two_doors();
        let eye = far + Vector3::new(-5.0, 1.5, 0.0);
        let cam = camera(eye, Vector3::new(0.0, 0.0, 1.0));
        let painting = far + Vector3::new(-5.0, 1.6, 2.0);
        let from = watch.seen_from(&cam, painting).expect("straight ahead");
        assert!((from - eye).mag() < 1e-5);
        // Facing the other way it is behind the player, and no portal helps.
        let cam = camera(eye, Vector3::new(0.0, 0.0, -1.0));
        assert!(watch.seen_from(&cam, painting).is_none());
    }

    #[test]
    fn seen_through_the_door_answers_the_warped_eye() {
        let (watch, far, _, _) = two_doors();
        // In the meadow, two strides in front of the door, looking at it.
        let eye = Vector3::new(0.0, 1.5, 2.0);
        let cam = camera(eye, Vector3::new(0.0, 0.0, -1.0));
        // A painting a few metres down the hall, near its centre line: straight through the
        // opening from here.
        let painting = far + Vector3::new(-4.0, 1.6, -0.5);
        let from = watch.seen_from(&cam, painting).expect("visible through the door");
        // The warped eye is where walking through would put the player: two strides out from
        // the far door on its +x side (it faces +x; the meadow side is its front).
        assert!((from.x - (far.x + 2.0)).abs() < 1e-3 && (from.y - 1.5).abs() < 1e-3, "{from:?}");
        // A painting well off the hall's axis is in the cone (31 degrees off) but the line to
        // it crosses the door's plane 1.2 m from the centre, past the 0.4 m half-width: not
        // through the opening.
        let beside = far + Vector3::new(-3.0, 1.6, -3.0);
        assert!(watch.seen_from(&cam, beside).is_none());
        // Turn to face along the meadow, away from the hall's direction (there are no walls
        // in this test to stop a thousand-unit line of sight): the door leaves the cone;
        // unobserved.
        let cam = camera(eye, Vector3::new(-1.0, 0.0, 0.0));
        assert!(watch.seen_from(&cam, painting).is_none());
        // Doors that can vanish: seen through them while they stand, not once they are gone,
        // while a direct look still counts.
        let (link, _) = DoorLink::pair();
        let watch = watch.while_doors_stand(link.clone());
        let cam = camera(eye, Vector3::new(0.0, 0.0, -1.0));
        assert!(watch.seen_from(&cam, painting).is_some());
        link.vanish();
        assert!(watch.seen_from(&cam, painting).is_none());
        let direct = camera(far + Vector3::new(-1.0, 1.5, -0.5), Vector3::new(-1.0, 0.0, 0.0));
        assert!(watch.seen_from(&direct, painting).is_some());
    }

    /// Something solid for the ray to hit, without GL: a square, as the scan's walls are.
    struct Slab {
        /// Meshless, so the ray goes straight to the triangles.
        base: Object,
        tm: Rc<crate::ext::trimesh::TriMeshCollider>,
    }
    impl ObjectT for Slab {
        fn base(&self) -> &Object {
            &self.base
        }
        fn base_mut(&mut self) -> &mut Object {
            &mut self.base
        }
        fn trimesh(&self) -> Option<Rc<crate::ext::trimesh::TriMeshCollider>> {
            Some(self.tm.clone())
        }
    }
    fn slab(centre: Vector3, half_y: f32, half_z: f32) -> Rc<RefCell<dyn ObjectT>> {
        let c = centre;
        let pos = [
            [c.x, c.y - half_y, c.z - half_z],
            [c.x, c.y + half_y, c.z - half_z],
            [c.x, c.y + half_y, c.z + half_z],
            [c.x, c.y - half_y, c.z + half_z],
        ];
        let idx = [0, 1, 2, 0, 2, 3];
        let tm = crate::ext::trimesh::TriMeshCollider::new(&pos, &idx, &Matrix4::identity());
        Rc::new(RefCell::new(Slab { base: Object::new(), tm: Rc::new(tm) }))
    }

    /// The hall's end wall stands a stride behind its door, where the meadow player's warped
    /// eye lands. The far half of the line of sight must start at the door, not there -- and a
    /// wall across the hall in front of the painting must still block it.
    #[test]
    fn the_far_half_starts_at_the_far_door() {
        let (_, far, here, there) = two_doors();
        let eye = Vector3::new(0.0, 1.5, 2.0);
        let cam = camera(eye, Vector3::new(0.0, 0.0, -1.0));
        let painting = far + Vector3::new(-4.0, 1.6, -0.5);
        // A wall the width of the hall, just behind the far door: between the warped eye and
        // the door, not between the door and the painting.
        let end_wall = slab(far + Vector3::new(0.5, 1.5, 0.0), 1.5, 3.0);
        let watch = Watch::new(&vec![end_wall], &vec![here.clone(), there.clone()]);
        assert!(watch.seen_from(&cam, painting).is_some(), "blocked by the wall behind the door");
        // The same wall across the hall in front of the painting.
        let across = slab(far + Vector3::new(-2.0, 1.5, 0.0), 1.5, 3.0);
        let watch = Watch::new(&vec![across], &vec![here, there]);
        assert!(watch.seen_from(&cam, painting).is_none(), "seen through a wall");
    }

    /// The sweet spot of the Backrooms' key in canvas-plane metres: 2.6 m along the wall,
    /// a tenth below the centre, half a metre out (`level16.rs`, `KEY_SPEC`).
    const VIEW: Vector3 = Vector3 { x: 2.6, y: -0.1, z: 0.488 };

    #[test]
    fn a_picture_plane_point_comes_back_to_itself_through_the_canvas() {
        for q in [(0.0, 0.0), (0.045, 0.0), (-0.045, 0.0), (0.0, 0.03), (-0.01, -0.27)] {
            let p = to_canvas(VIEW, q);
            let back = to_plane(VIEW, p);
            assert!(
                (back.0 - q.0).abs() < 1e-5 && (back.1 - q.1).abs() < 1e-5,
                "{q:?} -> {back:?}"
            );
        }
        // The plane passes through the canvas centre: its origin is painted there.
        assert!(to_canvas(VIEW, (0.0, 0.0)).0.abs() < 1e-6);
        // Seen square on, the plane IS the canvas and nothing is distorted.
        let square = Vector3::new(0.0, 0.0, 2.0);
        let p = to_canvas(square, (0.03, -0.02));
        assert!((p.0 - 0.03).abs() < 1e-6 && (p.1 + 0.02).abs() < 1e-6);
    }

    #[test]
    fn the_smear_is_elongated_along_the_view_direction() {
        // A circle on the plane, as painted: its extent along the canvas's x -- which is
        // where the line from the sweet spot to the centre runs, the view being along the
        // wall -- against its extent across it.
        let r = 0.019;
        let mut xs = (f32::MAX, f32::MIN);
        let mut ys = (f32::MAX, f32::MIN);
        for i in 0..64 {
            let a = i as f32 / 64.0 * std::f32::consts::TAU;
            let (x, y) = to_canvas(VIEW, (r * a.cos(), r * a.sin()));
            xs = (xs.0.min(x), xs.1.max(x));
            ys = (ys.0.min(y), ys.1.max(y));
        }
        let (wide, tall) = (xs.1 - xs.0, ys.1 - ys.0);
        assert!(wide > 4.0 * tall, "stretched {wide} by {tall}");
        assert!((tall - 2.0 * r).abs() < 0.003, "across the view it is its own size: {tall}");
        // And the far end stretches more than the near one (the end toward the viewer, +x).
        let near = to_canvas(VIEW, (r, 0.0)).0;
        let far = -to_canvas(VIEW, (-r, 0.0)).0;
        assert!(far > near * 1.05, "near {near} far {far}");
        // The key as hung fits on the canvas, and short of the near upright (painting.rs,
        // `KEY_ON_PLANE`).
        let (u0, v0) = KEY_ON_PLANE;
        let bow = to_canvas(VIEW, (u0 - 0.045, v0)).0;
        let tip = to_canvas(VIEW, (u0 + 0.045, v0)).0;
        assert!(bow > -0.37 && tip < 0.18, "bow at {bow}, tip at {tip}");
    }

    #[test]
    fn armed_needs_both_the_spot_and_the_look() {
        let centre = Vector3::new(997.0, 1.6, 2.05);
        let spec = KeySpec {
            view: Vector3::new(994.4, 1.5, 1.55),
            radius: 0.45,
            cone: 25.0_f32.to_radians(),
        };
        let at = |eye: Vector3, dir: Vector3| camera(eye, dir);
        let to_centre = (centre - spec.view).normalized();
        assert!(armed(&at(spec.view, to_centre), &spec, centre));
        // Within the radius, still looking: armed. A metre off: not.
        assert!(armed(&at(spec.view + Vector3::new(0.3, 0.0, 0.2), to_centre), &spec, centre));
        assert!(!armed(&at(spec.view + Vector3::new(-1.0, 0.0, 0.0), to_centre), &spec, centre));
        // In the spot, looking down the hall instead (90 degrees off): not.
        assert!(!armed(&at(spec.view, Vector3::new(0.0, 0.0, -1.0)), &spec, centre));
        // Twenty degrees off the centre is within the cone; thirty is not.
        let turned = |deg: f32| {
            let a = deg.to_radians();
            Vector3::new(
                to_centre.x * a.cos() - to_centre.z * a.sin(),
                0.0,
                to_centre.x * a.sin() + to_centre.z * a.cos(),
            )
        };
        assert!(armed(&at(spec.view, turned(20.0)), &spec, centre));
        assert!(!armed(&at(spec.view, turned(30.0)), &spec, centre));
    }

    /// Drive the emergence with `(armed, taken, seconds)` stretches at the fixed step,
    /// collecting the events and returning them with the final state.
    fn emerge(script: &[(bool, bool, f32)]) -> (Vec<KeyEvent>, Emergence) {
        let mut e = Emergence::default();
        let mut events = Vec::new();
        for &(armed, taken, secs) in script {
            for _ in 0..(secs / GH_DT).round() as u32 {
                events.extend(e.step(armed, taken, GH_DT));
            }
        }
        (events, e)
    }

    #[test]
    fn the_key_comes_out_on_arming_and_sinks_back_on_leaving() {
        // Nothing happens unarmed; arming spawns the key at once and it is out after EMERGE.
        let (ev, e) = emerge(&[(false, false, 1.0)]);
        assert!(ev.is_empty() && e.phase() == KeyPhase::Painted && e.blend() == 0.0);
        let (ev, e) = emerge(&[(true, false, EMERGE * 0.5)]);
        assert_eq!(ev, [KeyEvent::Spawn]);
        assert_eq!(e.phase(), KeyPhase::Emerging);
        assert!((e.blend() - 0.5).abs() < 0.02 && e.floating());
        let (ev, e) = emerge(&[(true, false, EMERGE + 0.1)]);
        assert_eq!(ev, [KeyEvent::Spawn]);
        assert!(e.phase() == KeyPhase::Out && e.blend() == 1.0);
        // Leave: it sinks for as long as it came out, then is removed and painted again.
        let (ev, e) = emerge(&[(true, false, EMERGE + 0.1), (false, false, EMERGE * 0.5)]);
        assert_eq!(ev, [KeyEvent::Spawn]);
        assert!(e.phase() == KeyPhase::Sinking && (e.blend() - 0.5).abs() < 0.02);
        let (ev, e) = emerge(&[(true, false, EMERGE + 0.1), (false, false, EMERGE + 0.1)]);
        assert_eq!(ev, [KeyEvent::Spawn, KeyEvent::Remove]);
        assert!(e.phase() == KeyPhase::Painted && e.blend() == 0.0 && !e.floating());
        // Come back mid-sink: it turns round without a second spawn, and a second leave
        // and return cycle spawns again only after it has been fully removed.
        let (ev, e) = emerge(&[
            (true, false, 0.2),
            (false, false, 0.1),
            (true, false, EMERGE),
            (false, false, 1.0),
            (true, false, 0.1),
        ]);
        assert_eq!(ev, [KeyEvent::Spawn, KeyEvent::Remove, KeyEvent::Spawn]);
        assert_eq!(e.phase(), KeyPhase::Emerging);
    }

    /// The emerged key, seen from the spot, is where it was painted; a ray from the spot
    /// through it lands on the canvas at the painted spot, inside the picture, not on the
    /// wall past the frame -- which is where the grab would put the taken key.
    #[test]
    fn the_key_emerges_toward_the_spot_and_stays_over_its_painted_spot() {
        use crate::ext::raycast::ray_collider;
        let centre = Vector3::new(997.0, 1.6, 2.05);
        let facing = Vector3::new(0.0, 0.0, -1.0);
        let view = Vector3::new(994.4, 1.5, 1.55);
        // The painted key's spot on the canvas, in the world, as `Painting::new` finds it.
        let rigid = Matrix4::trans(centre) * Matrix4::rot_y(yaw_facing(facing));
        let rigid_w2l = Matrix4::rot_y(-yaw_facing(facing)) * Matrix4::trans(-centre);
        let view_local = rigid_w2l.mul_point(view) - Vector3::new(0.0, 0.0, CANVAS_DEPTH);
        let (px, py) = to_canvas(view_local, KEY_ON_PLANE);
        let rest = rigid.mul_point(Vector3::new(px, py, CANVAS_DEPTH));
        let out = emergence_path(rest, view, facing);
        // Clear of the wall by KEY_OUT, and on the line to the eye.
        assert!((out.dot(facing) - KEY_OUT).abs() < 1e-5, "{out:?}");
        assert!(out.normalized().dot((view - rest).normalized()) > 0.9999);
        // The grab's ray, eye through the floating key, meets the canvas rectangle within
        // the picture (the canvas is the Mona Lisa's 0.8 x 0.94 quad, +-1 scaled); straight
        // out along the normal it would miss the picture by a third of a metre.
        let (w, h) = Painting::size_for(&crate::ext::portrait_atlas::PORTRAITS[0], 0.8);
        assert!((h - 0.9365).abs() < 0.002, "the Mona Lisa's canvas is {h} high");
        let mut canvas = Object::new();
        canvas.pos = rigid.mul_point(Vector3::new(0.0, 0.0, CANVAS_DEPTH));
        canvas.euler.y = yaw_facing(facing);
        canvas.scale = Vector3::new(0.5 * w, 0.5 * h, 1.0);
        let rect = Collider::rect(Vector3::zero(), Vector3::unit_x(), Vector3::unit_y());
        let hits = |key: Vector3| {
            ray_collider(view, (key - view).normalized(), &canvas.local_to_world(), &rect)
        };
        let (t, _) = hits(rest + out).expect("the ray through the emerged key hits the canvas");
        assert!((t - (rest - view).mag()).abs() < 0.02, "at the painted spot: {t}");
        assert!(hits(rest + facing * 0.10).is_none(), "straight out it misses the picture");
        // Nothing in the picture plane's own frame: a spot in the wall's plane goes straight out.
        let flat = emergence_path(rest, rest + Vector3::new(-1.0, 0.0, 0.0), facing);
        assert!((flat - facing * KEY_OUT).mag() < 1e-6);
    }

    /// The key's silhouette lives in three places -- the shader's defines, the generator's
    /// constants and `KEY_ON_PLANE` here -- held together by nothing but this: the numbers
    /// must agree, and the shipped mesh must be the generator's length.
    #[test]
    fn the_shader_the_generator_and_the_mesh_agree_on_the_key() {
        let frag = std::fs::read_to_string(crate::app::assets::path("Shaders/painting.frag"))
            .expect("Shaders/painting.frag");
        let py = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tools/gen_key.py"),
        )
        .expect("tools/gen_key.py");
        // `#define NAME value` / `NAME = value  # ...`: the numbers after the name, in order.
        fn numbers(line: &str) -> Vec<f32> {
            line.replace("vec3", "")
                .replace("vec2", "")
                .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
                .filter_map(|t| t.parse::<f32>().ok())
                .collect()
        }
        let define = |name: &str| -> Vec<f32> {
            let line = frag
                .lines()
                .find(|l| l.starts_with(&format!("#define {name} ")))
                .unwrap_or_else(|| panic!("no #define {name}"));
            numbers(&line[name.len() + 8..])
        };
        let py_const = |name: &str| -> Vec<f32> {
            let line = py
                .lines()
                .find(|l| l.starts_with(&format!("{name} =")))
                .unwrap_or_else(|| panic!("no {name} in gen_key.py"));
            numbers(line.split('#').next().unwrap()[name.len() + 2..].trim())
        };
        for (shader, generator) in [
            ("KEY_LENGTH", "LENGTH"),
            ("KEY_BOW_R", "BOW_R"),
            ("KEY_BOW_HOLE", "BOW_HOLE"),
            ("KEY_SHAFT_HW", "SHAFT_HW"),
        ] {
            assert_eq!(define(shader), py_const(generator), "{shader} vs {generator}");
        }
        let teeth: Vec<f32> = [define("KEY_TOOTH1"), define("KEY_TOOTH2")].concat();
        assert_eq!(teeth.len(), 6);
        assert_eq!(teeth, py_const("TEETH"), "the teeth");
        // The spot on the picture plane, shader vs scene.
        assert_eq!(define("KEY_ON_PLANE"), vec![KEY_ON_PLANE.0, KEY_ON_PLANE.1]);
        // The shipped mesh spans the generator's length along x.
        let obj = std::fs::read_to_string(crate::app::assets::path("Meshes/key.obj"))
            .expect("Meshes/key.obj");
        let xs: Vec<f32> = obj
            .lines()
            .filter(|l| l.starts_with("v "))
            .filter_map(|l| l.split_whitespace().nth(1)?.parse().ok())
            .collect();
        assert!(!xs.is_empty(), "Meshes/key.obj has no vertices (an LFS pointer?)");
        let span = xs.iter().cloned().fold(f32::MIN, f32::max)
            - xs.iter().cloned().fold(f32::MAX, f32::min);
        assert!((span - py_const("LENGTH")[0]).abs() < 1e-4, "mesh spans {span}");
    }

    #[test]
    fn taking_the_key_is_final() {
        // Taken while out: no removal, fully blended, and nothing the spot does matters.
        let (ev, e) = emerge(&[
            (true, false, EMERGE + 0.1),
            (true, true, 0.1),
            (false, true, 2.0),
            (true, true, 0.1),
        ]);
        assert_eq!(ev, [KeyEvent::Spawn]);
        assert!(e.phase() == KeyPhase::Taken && e.blend() == 1.0 && !e.floating());
        // Taken mid-emergence: the same, with the painted key gone at once.
        let (ev, e) = emerge(&[(true, false, 0.1), (true, true, GH_DT)]);
        assert_eq!(ev, [KeyEvent::Spawn]);
        assert!(e.phase() == KeyPhase::Taken && e.blend() == 1.0);
        // Taken while sinking, before the removal: still no removal.
        let (ev, e) = emerge(&[(true, false, EMERGE), (false, false, 0.1), (false, true, 1.0)]);
        assert_eq!(ev, [KeyEvent::Spawn]);
        assert_eq!(e.phase(), KeyPhase::Taken);
    }

    #[test]
    fn a_portal_is_only_consulted_for_its_far_side() {
        let (_, far, here, there) = two_doors();
        // A painting in the hall is beyond the meadow door, whichever side of it the eye is
        // on, and is not beyond the hall door from either side.
        let painting = far + Vector3::new(-6.0, 1.6, 2.0);
        let (here, there) = (here.borrow(), there.borrow());
        assert!(beyond(here.base.pos, &here.front, painting));
        assert!(beyond(here.base.pos, &here.back, painting));
        assert!(!beyond(there.base.pos, &there.front, painting));
        assert!(!beyond(there.base.pos, &there.back, painting));
    }
}
