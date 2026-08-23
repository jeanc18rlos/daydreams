//! EXT: portraits whose eyes follow you, and whose faces change only while you are not
//! looking. Not part of the C++ port.
//!
//! A `Painting` is a canvas -- the ported `quad.obj` drawn with `Shaders/painting.*`, which
//! paints the sitter procedurally -- inside a gilt frame of four thin `cube.obj` bars drawn the
//! way any ported prop is. It hangs flush on a wall, collides with nothing, and has two
//! behaviours built on two things the engine already does:
//!
//! * **The eyes follow the camera of the pass.** Every draw receives the pass camera's eye
//!   (`RenderCtx.eye`); the painting turns that into a point in its own canvas metres and the
//!   shader displaces each iris toward it. Because it is the PASS eye, a portrait seen through
//!   a portal looks at the portal camera -- at the person in the doorway, not at some spot on
//!   the meadow a thousand units away.
//! * **The face changes only while unobserved** -- Level10's statues, applied to an expression.
//!   [`Watch::seen_from`] is `ext::visibility`'s cone-and-line-of-sight test, extended to see
//!   through the scene's portals (below), and [`Expression`] advances only after [`GRACE`]
//!   seconds of nobody looking: the brows lower, the mouth flattens and the gaze stops
//!   following and stares straight out. Look away and back, and it is different; you never
//!   catch it moving. The next unobserved stretch puts it back, so it alternates. The eyes
//!   have a memory too ([`Gaze`]): while nobody looks they stay aimed at where the viewer was
//!   last seen from, and when looked at again they slide from there to the viewer over
//!   [`REACQUIRE`] seconds -- so turning back finds them on the spot you left.
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

use std::cell::RefCell;
use std::rc::Rc;

use crate::camera::Camera;
use crate::ext::backrooms::WALL_FOG;
use crate::ext::cull::object_sphere;
use crate::ext::door::{yaw_facing, DoorLink};
use crate::ext::visibility::{has_line_of_sight, in_view_cone, WATCH_HALF_ANGLE};
use crate::game_header::GH_DT;
use crate::object::{Object, ObjectT, RenderCtx, UpdateCtx};
use crate::portal::{Portal, Warp};
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec};
use crate::vector::{Matrix4, Vector3};

/// Seconds a painting must go unobserved before its expression changes. Long enough that a
/// glance across it, or a head turn that sweeps it out of the cone and back, changes nothing.
pub const GRACE: f32 = 0.4;
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

/// The expression state machine: neutral or changed, advancing only while unobserved.
#[derive(Clone, Copy, Debug, Default)]
pub struct Expression {
    changed: bool,
    /// Seconds of the current unobserved stretch.
    unseen: f32,
    /// Whether this stretch has already had its one change.
    flipped: bool,
}

impl Expression {
    /// One fixed step: `observed` is this step's answer, `dt` its length in seconds.
    pub fn step(&mut self, observed: bool, dt: f32) {
        if observed {
            self.unseen = 0.0;
            self.flipped = false;
            return;
        }
        self.unseen += dt;
        if !self.flipped && self.unseen > GRACE {
            self.changed = !self.changed;
            self.flipped = true;
        }
    }

    pub fn changed(&self) -> bool {
        self.changed
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
    /// Snapshot the objects and portals built so far. Paintings themselves carry no colliders,
    /// so whether they are in the list makes no difference to the ray.
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

pub struct Painting {
    canvas: Object,
    bars: [Object; 4],
    /// World to canvas metres, without the canvas's scale: what the shader's gaze wants.
    rigid_w2l: Matrix4,
    size: (f32, f32),
    seed: u32,
    watch: Watch,
    /// The point the observed test asks about.
    probe: Vector3,
    /// Steps taken, for the test's cadence; starts on a per-painting phase.
    steps: u32,
    /// The last test's answer, held between tests.
    seen_from: Option<Vector3>,
    expression: Expression,
    gaze: Gaze,
}

impl Painting {
    /// A `size.0` x `size.1` metre portrait centred at `centre`, hanging on a wall whose
    /// normal (toward the room) is `facing`; `seed` picks the sitter and `watch` is what it
    /// decides "being looked at" through.
    pub fn new(
        res: &Resources,
        centre: Vector3,
        facing: Vector3,
        size: (f32, f32),
        seed: u32,
        watch: Watch,
    ) -> Painting {
        let yaw = yaw_facing(facing);
        // The painting's own frame: x along the wall, y up, z off the wall toward the room.
        let rigid = Matrix4::trans(centre) * Matrix4::rot_y(yaw);
        let rigid_w2l = Matrix4::rot_y(-yaw) * Matrix4::trans(-centre);
        let (w, h) = size;

        let mut canvas = Object::new();
        canvas.mesh = Some(res.acquire_mesh("quad.obj"));
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

        Painting {
            canvas,
            bars,
            rigid_w2l,
            size,
            seed,
            watch,
            probe: centre + facing.normalized_safe() * PROBE_OUT,
            steps: seed % WATCH_EVERY,
            seen_from: None,
            expression: Expression::default(),
            gaze: Gaze::default(),
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
        let mesh = self.canvas.mesh.as_ref().expect("set in new");
        let local_to_world = self.canvas.local_to_world();
        let mvp = cam.matrix() * local_to_world;
        let eye = ctx.eye;
        // This pass's eye, through the gaze memory, into canvas metres.
        let v = self.rigid_w2l.mul_point(self.gaze.target(eye));
        shader.use_program();
        shader.set_mvp(Some(&mvp), None);
        shader.set_mat4("model", &local_to_world);
        shader.set_vec4("cam_pos", [eye.x, eye.y, eye.z, 1.0]);
        shader.set_vec4("fog_color", WALL_FOG);
        shader.set_vec4("viewer_local", [v.x, v.y, v.z, 1.0]);
        shader.set_vec4("size", [self.size.0, self.size.1, 0.0, 0.0]);
        shader.set_f32("seed", self.seed as f32);
        shader.set_f32("expression", if self.expression.changed() { 1.0 } else { 0.0 });
        mesh.draw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive the state machine with a script of `(observed, seconds)` stretches at the fixed
    /// step, returning the expression after each stretch.
    fn run(script: &[(bool, f32)]) -> Vec<bool> {
        let mut e = Expression::default();
        let mut out = Vec::new();
        for &(observed, secs) in script {
            let steps = (secs / GH_DT).round() as u32;
            for _ in 0..steps {
                e.step(observed, GH_DT);
            }
            out.push(e.changed());
        }
        out
    }

    #[test]
    fn starts_neutral_and_never_changes_while_watched() {
        assert_eq!(run(&[(true, 10.0)]), [false]);
    }

    #[test]
    fn changes_once_after_the_grace_and_alternates_per_stretch() {
        // A glance away shorter than the grace changes nothing; a longer one changes it once,
        // however long it goes on; the next stretch puts it back.
        let out = run(&[
            (false, GRACE * 0.5),
            (true, 0.1),
            (false, GRACE + 0.05),
            (false, 30.0),
            (true, 0.1),
            (false, GRACE + 0.05),
            (true, 0.1),
            (false, GRACE + 0.05),
        ]);
        assert_eq!(out, [false, false, true, true, true, false, false, true]);
    }

    #[test]
    fn a_look_resets_the_grace() {
        // Two unobserved stretches each just under the grace, with a look between: no change.
        let out = run(&[(false, GRACE * 0.9), (true, GH_DT), (false, GRACE * 0.9)]);
        assert_eq!(out, [false, false, false]);
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
