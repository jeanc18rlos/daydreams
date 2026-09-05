//! EXT: the sleepwalkers -- mannequin NPCs that patrol the Open House street and notice
//! what does not belong. Not part of the C++ port.
//!
//! One [`Npc`] is a `skinned::Instance` of `Meshes/mannequin.glb` walking a closed route of
//! world-space waypoints, finding the ground by raycast each step exactly as level32's
//! Walker does, and choosing its clip from what it is doing: `walking` on the move,
//! `standing_idle` at a pause, `running` when it goes to look at something.
//!
//! # What they can see
//!
//! Their eyes are the engine's own observation kernel (`ext/visibility.rs`): a 120-degree
//! cone plus a line-of-sight raycast, the same test the statues and the Duel's Dreamer are
//! judged by. Two things the kernel does NOT do, and this module must:
//!
//! * **Range.** `is_observed` has no far limit -- a point sixty metres down a straight
//!   street with clear line of sight is "observed" forever. A sleepwalker therefore gates on
//!   [`SIGHT_RANGE`] first, and the gate is cheap enough to run before the raycast.
//! * **Portals.** The kernel is portal-blind: it cannot see through the street seam or a
//!   pillar pocket, and it happily "sees" through a portal that is really a doorway
//!   elsewhere. Sleepwalkers only ever look at things in their own stretch of street, which
//!   is why that is sound here; anything portal-aware wants `painting::Watch::seen_from`.
//!
//! # The register
//!
//! `docs/hide-n-dream.md` is emphatic that this game's presences are not monsters: the
//! seeker's only verb is the tag, a lost life is "waking up a bit", and the campaign's rule
//! was that the presence never pursues and never harms. A sleepwalker keeps that: noticing
//! is the whole of its aggression. It stops, it turns, it walks over to look, it says so on
//! the hint line -- and then it loses interest and goes back to its round. What noticing
//! COSTS is the hide (`ext/disguise.rs`): a seen player is a found player, and the disguise
//! falls off.
//!
//! # Cost
//!
//! Skinning is CPU work at 31 Hz, staggered across instances by `ext/skinned.rs`; the sight
//! test is one cone test and at most one raycast, itself throttled to [`LOOK_EVERY`] steps
//! and staggered by the same trick, so a street of them costs a raycast every few steps
//! rather than one per NPC per step.

use std::rc::Rc;

use crate::camera::Camera;
use crate::ext::skinned::{Instance, SkinnedModel};
use crate::ext::visibility::is_observed;
use crate::ext::{disguise, raycast};
use crate::game_header::{GH_DT, GH_PI};
use crate::object::{Object, ObjectT, RenderCtx, UpdateCtx};
use crate::resources::Resources;
use crate::vector::{Matrix4, Vector3};

pub const MODEL: &str = "Meshes/mannequin.glb";
/// The one mesh in the file (`tools/build_mannequin.py` names it).
pub const PART: &str = "Mannequin";

/// Clip names, as `tools/build_mannequin.py` slugged them from the Mixamo downloads.
const CLIP_WALK: &str = "walking";
const CLIP_IDLE: &str = "standing_idle";
const CLIP_RUN: &str = "running";
/// Mixamo's walk and run cover this much ground a second; the clip is paced to the speed
/// rather than the speed to the clip.
const WALK_CYCLE_SPEED: f32 = 1.15;
const RUN_CYCLE_SPEED: f32 = 3.2;

/// Patrol pace and the quicker walk toward something noticed, in metres a second.
const WALK_SPEED: f32 = 1.15;
const LOOK_SPEED: f32 = 2.4;
/// Close enough to a waypoint to take the next one.
const ARRIVE: f32 = 0.5;
/// How close a sleepwalker will come to something it noticed before it just stands and
/// looks. Nothing in this game closes the distance: the design doc's presences never
/// pursue, and a mannequin that walks into your face is a jump scare, which is the one
/// register the whole thing is built to avoid. Two and a half metres is the tag reach --
/// close enough to be unbearable, far enough to still be a room.
const KEEP_BACK: f32 = 2.6;
/// Dead band on [`KEEP_BACK`] for the arrival clip. `look_at` only refreshes on the look
/// slot, so without a band a player loitering at the stop distance flips the clip several
/// times a second -- and `Instance::set_clip` re-poses the whole mesh on every change.
const KEEP_BACK_HYST: f32 = 0.4;
/// How fast a sleepwalker turns, radians a second.
const TURN_RATE: f32 = 2.6;

/// How far a sleepwalker can notice anything at all. The kernel has no range of its own
/// (module docs); without this a patrol at one end of the 450 m street watches the other.
pub const SIGHT_RANGE: f32 = 22.0;
/// Within this, a sleepwalker notices a disguised player anyway -- the prop is the wrong
/// prop, this close. The law of the design doc is that every hide has a find; this is the
/// prop disguise's cheapest one.
pub const RUMBLE_RANGE: f32 = 1.9;
/// Steps between sight tests: 25 of the 500 Hz steps, so 20 Hz, staggered per NPC.
const LOOK_EVERY: u32 = 25;

/// Steps a sleepwalker stands still at a waypoint pause.
const PAUSE_STEPS: u32 = 500;
/// Steps it keeps walking toward something it noticed before losing interest.
const INTEREST_STEPS: u32 = 1500;
/// How far above its own feet a sleepwalker's eyes sit.
const EYE_HEIGHT: f32 = 1.55;
/// Ground probe, as level32's Walker does it.
const GROUND_PROBE_UP: f32 = 2.0;
const GROUND_PROBE_DOWN: f32 = 6.0;
/// How far ahead a sleepwalker feels for a wall, and at what height. A sleepwalker owns its
/// own motion (`engine_collision` is false, so the engine never pushes it), which means the
/// houses' triangle colliders do nothing for it and it will walk through a wall unless it
/// looks. The probe is at chest height -- below the shell's window sills, so a wall it can
/// see over is still a wall it cannot walk through.
/// EXT: how close a hunting sleepwalker must be to tag the player. The same 2.5 m the key
/// uses to reach a lock (`ext/key.rs`) and the disguise uses to offer a costume.
pub const TAG_REACH: f32 = 2.5;
/// EXT: look slots spent blocked before a patrol gives up on the leg it cannot reach. At
/// LOOK_EVERY = 10 and 500 Hz that is about a third of a second of pushing at a wall.
const STUCK_SLOTS: u32 = 16;

const FEEL_AHEAD: f32 = 0.9;
const FEEL_HEIGHT: f32 = 1.0;

/// Whether a sleepwalker closing on something it noticed should still be moving, given how
/// far it has left and whether it was moving already. A free function so the rule can be
/// tested without a GL context -- an `Npc` needs a skinned `Instance`, the tests do not.
fn curious_moving(left: f32, was_moving: bool, keep_back: f32) -> bool {
    left > if was_moving { keep_back } else { keep_back + KEEP_BACK_HYST }
}

/// What a sleepwalker is doing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mood {
    /// Walking the round.
    Patrol,
    /// Standing at a waypoint, looking at nothing.
    Pause,
    /// Walking over to look at something it noticed.
    Curious,
}

pub struct Npc {
    base: Object,
    inst: Instance,
    /// The model's feet sit at its own origin, so this is 0 for the mannequin -- kept
    /// because it is what any other character asset would need.
    lift: f32,
    route: Vec<Vector3>,
    leg: usize,
    mood: Mood,
    /// Steps left in the current pause or spell of interest.
    timer: u32,
    want_yaw: f32,
    /// Where it is walking to when [`Mood::Curious`].
    look_at: Vector3,
    /// Step counter and the per-NPC offset that staggers the sight tests.
    tick: u32,
    phase: u32,
    /// Whether it noticed the player on its last sight test: the hint line and the
    /// disguise's find both read this.
    sees: bool,
    /// Whether the clip currently playing is a walking one -- so arriving at what it came
    /// to look at can settle into a stand without re-setting the clip every step.
    mood_clip_is_moving: bool,
    /// Whether something solid is a stride ahead, refreshed on the look slot. A sleepwalker
    /// drives its own motion, so this is the only thing between it and the wallpaper.
    blocked: bool,
    /// EXT: consecutive look slots spent blocked while patrolling. `advance` refuses to move
    /// while blocked and `Mood::Patrol` only takes its next leg on ARRIVAL, so a sleepwalker
    /// that meets a doorframe waits for an arrival that can never happen and stands there for
    /// the rest of the night. Harmless on the open verge it was written for; fatal the moment
    /// a route goes through a house, which is what a seeker's route is.
    stuck: u32,
    /// EXT: whether this one is hunting the player rather than sleepwalking past them --
    /// `ext/hunt.rs` turns it on for the seek phase. A hunter closes to arm's length and
    /// tags; a sleepwalker stops well short and just looks.
    hunting: bool,
}

impl Npc {
    /// A sleepwalker walking `route` for ever, starting at its first waypoint. `phase`
    /// staggers its sight tests against its neighbours' -- pass the index in the cast.
    pub fn new(gl: &Rc<glow::Context>, res: &Resources, route: Vec<Vector3>, phase: u32) -> Npc {
        let model = SkinnedModel::acquire(gl, MODEL);
        let mut inst = Instance::new(gl, res, &model, Some(PART));
        inst.set_clip(CLIP_WALK, WALK_SPEED / WALK_CYCLE_SPEED);
        let b = inst.bounds();
        let mut base = Object::new();
        base.pos = *route.first().unwrap_or(&Vector3::zero());
        let want_yaw = match route.get(1) {
            Some(next) => (*next - base.pos).x.atan2((*next - base.pos).z),
            None => 0.0,
        };
        base.euler.y = want_yaw;
        Npc {
            base,
            inst,
            lift: -b[2],
            route,
            leg: 1,
            mood: Mood::Patrol,
            timer: 0,
            want_yaw,
            look_at: Vector3::zero(),
            tick: 0,
            phase,
            sees: false,
            mood_clip_is_moving: true,
            blocked: false,
            stuck: 0,
            hunting: false,
        }
    }

    /// EXT: how close this one comes before it stops. A sleepwalker keeps its distance and
    /// stares; a hunter has to be able to reach you, or it could never tag.
    fn keep_back(&self) -> f32 {
        if self.hunting {
            TAG_REACH * 0.6
        } else {
            KEEP_BACK
        }
    }

    /// EXT: hunt or sleepwalk (`ext/hunt.rs`).
    pub fn set_hunting(&mut self, on: bool) {
        self.hunting = on;
    }

    /// EXT: send it somewhere else -- the seek phase swaps a porch loop for a room-by-room
    /// sweep of the house. Restarts the round at the first leg and drops any interest it had.
    pub fn set_route(&mut self, route: Vec<Vector3>) {
        if route.is_empty() {
            return;
        }
        self.route = route;
        self.leg = 1;
        self.stuck = 0;
        self.timer = 0;
        self.set_mood(Mood::Patrol);
    }

    /// Whether this sleepwalker can see the given point right now, by the engine's own
    /// observation kernel plus the range gate the kernel lacks.
    pub fn can_see(&self, scene: &[Rc<std::cell::RefCell<dyn ObjectT>>], point: Vector3) -> bool {
        let d = point - self.eye();
        if d.mag() > SIGHT_RANGE {
            return false;
        }
        is_observed(scene, &self.eye_matrix(), point, None)
    }

    fn eye(&self) -> Vector3 {
        self.base.pos + Vector3::new(0.0, EYE_HEIGHT, 0.0)
    }

    /// An eye transform in the CAMERA's convention: the observation kernel reads a
    /// viewer's facing as its matrix's -Z, while the mannequin's own geometry faces +Z at
    /// `euler.y`. The half turn is that difference, and getting it wrong points the
    /// sleepwalker's attention out of the back of its head.
    fn eye_matrix(&self) -> Matrix4 {
        Matrix4::trans(self.eye()) * Matrix4::rot_y(self.base.euler.y + GH_PI)
    }

    /// Whether there is a wall within a stride of `from`, along `dir`.
    fn wall_ahead(
        &self,
        scene: &[Rc<std::cell::RefCell<dyn ObjectT>>],
        from: Vector3,
        dir: Vector3,
    ) -> bool {
        let chest = from + Vector3::new(0.0, FEEL_HEIGHT, 0.0);
        raycast::raycast(scene, chest, dir.normalized_safe(), FEEL_AHEAD, None).is_some()
    }

    /// Walk toward `target` at `speed`; returns the horizontal distance left.
    fn advance(&mut self, target: Vector3, speed: f32) -> f32 {
        let d = target - self.base.pos;
        let flat = Vector3::new(d.x, 0.0, d.z);
        let dist = flat.mag();
        if dist > 1e-4 {
            self.want_yaw = flat.x.atan2(flat.z);
            if dist > ARRIVE && !self.blocked {
                self.base.pos += flat * (speed * GH_DT / dist);
            }
        }
        dist
    }

    fn set_mood(&mut self, mood: Mood) {
        if self.mood == mood {
            return;
        }
        self.mood = mood;
        self.mood_clip_is_moving = mood != Mood::Pause;
        match mood {
            Mood::Patrol => self.inst.set_clip(CLIP_WALK, WALK_SPEED / WALK_CYCLE_SPEED),
            Mood::Pause => self.inst.set_clip(CLIP_IDLE, 1.0),
            Mood::Curious => self.inst.set_clip(CLIP_RUN, LOOK_SPEED / RUN_CYCLE_SPEED),
        }
    }
}

impl ObjectT for Npc {
    fn base(&self) -> &Object {
        &self.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base
    }

    fn update(&mut self, ctx: &UpdateCtx) {
        self.tick = self.tick.wrapping_add(1);

        // --- what it can see, on its own slot of the shared look budget -----------------
        if (self.tick + self.phase) % LOOK_EVERY == 0 {
            // Feel for a wall on the same slot as the sight test, so the cast costs one
            // more raycast every LOOK_EVERY steps rather than one per step.
            let heading = Vector3::new(self.want_yaw.sin(), 0.0, self.want_yaw.cos());
            self.blocked = self.wall_ahead(ctx.scene, self.base.pos, heading);
            // A disguised player who is holding still is furniture, and furniture is not
            // worth looking at -- unless the sleepwalker is close enough to know this room
            // has no such chair (the prop disguise's find).
            let hidden = disguise::hides_from(self.eye(), ctx.player_pos, &self.eye_matrix());
            self.sees = !hidden && self.can_see(ctx.scene, ctx.player_pos);
            if self.sees {
                self.look_at = ctx.player_pos;
                self.timer = INTEREST_STEPS;
                self.set_mood(Mood::Curious);
                // Being seen is what a hide costs: the prop falls off.
                disguise::blow();
                // EXT: and if it is hunting and already within arm's reach, that is the tag.
                // Horizontal, for the same reason the disguise's own find is
                // (`disguise::hides_from`): a floor below is not a hiding place.
                if self.hunting {
                    let d = ctx.player_pos - self.base.pos;
                    if Vector3::new(d.x, 0.0, d.z).mag() <= TAG_REACH {
                        crate::ext::hunt::report_tag();
                    }
                }
            }
            // EXT: count the look slots spent walking into something, so a wedged patrol can
            // give up on the leg it cannot reach (see the `stuck` field).
            if self.blocked && self.mood == Mood::Patrol {
                self.stuck = self.stuck.saturating_add(1);
            } else {
                self.stuck = 0;
            }
        }
        // Say what this one notices; the loudest report of the step reaches the HUD.
        if self.sees {
            report(Attention::Seen);
        } else if (self.base.pos - ctx.player_pos).mag() < RUMBLE_RANGE * 2.5 {
            report(Attention::Near);
        }

        // --- what it does about it -----------------------------------------------------
        match self.mood {
            Mood::Patrol => {
                let target = self.route[self.leg % self.route.len()];
                // EXT: `|| self.stuck > STUCK_SLOTS` is the doorframe fix -- take the next leg
                // when the current one has proved unreachable, not only when it is reached.
                if self.advance(target, WALK_SPEED) <= ARRIVE || self.stuck > STUCK_SLOTS {
                    self.leg = (self.leg + 1) % self.route.len();
                    self.stuck = 0;
                    self.timer = PAUSE_STEPS;
                    self.set_mood(Mood::Pause);
                }
            }
            Mood::Pause => {
                self.timer = self.timer.saturating_sub(1);
                if self.timer == 0 {
                    self.set_mood(Mood::Patrol);
                }
            }
            Mood::Curious => {
                // Turn to it always; close on it only until KEEP_BACK, then stand and look.
                let d = self.look_at - self.base.pos;
                let flat = Vector3::new(d.x, 0.0, d.z);
                let left = flat.mag();
                if left > 1e-4 {
                    self.want_yaw = flat.x.atan2(flat.z);
                }
                // ONE predicate drives both the step and the clip, so they cannot
                // disagree. They did: the arrival swapped to the idle clip and nothing ever
                // put the run clip back, so a sleepwalker whose quarry backed off went on
                // translating at full speed with its feet planted -- gliding.
                let moving = curious_moving(left, self.mood_clip_is_moving, self.keep_back())
                    && !self.blocked;
                if moving {
                    self.base.pos += flat * (LOOK_SPEED * GH_DT / left);
                }
                if moving != self.mood_clip_is_moving {
                    if moving {
                        self.inst.set_clip(CLIP_RUN, LOOK_SPEED / RUN_CYCLE_SPEED);
                    } else {
                        self.inst.set_clip(CLIP_IDLE, 1.0);
                    }
                    self.mood_clip_is_moving = moving;
                }
                self.timer = self.timer.saturating_sub(1);
                if self.timer == 0 {
                    self.timer = PAUSE_STEPS;
                    self.set_mood(Mood::Pause);
                }
            }
        }

        // --- the ground, and the turn ---------------------------------------------------
        let eye = self.base.pos + Vector3::new(0.0, GROUND_PROBE_UP, 0.0);
        let down = Vector3::new(0.0, -1.0, 0.0);
        if let Some(hit) =
            raycast::raycast(ctx.scene, eye, down, GROUND_PROBE_UP + GROUND_PROBE_DOWN, None)
        {
            self.base.pos.y = hit.point.y;
        }
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
        // toes), which is where `euler.y = atan2(dx, dz)` already points it.
        obj.euler.y = self.base.euler.y;
        self.inst.draw(&obj, ctx, cam);
    }

    /// Sleepwalkers drive themselves: nothing pushes them, no portal warps them, and with
    /// no hit sphere at all the grab cannot pick one up (exactly one sphere is what marks a
    /// thing grabbable -- `ext/grab.rs`).
    fn engine_collision(&self) -> bool {
        false
    }
    fn static_collision(&self) -> bool {
        false
    }
}

/// The line the street says when a sleepwalker is looking at you, and the one it says when
/// one is merely close. Both inside the HUD's 40-character budget.
pub const SEEN_HINT: &str = "SOMEONE IS LOOKING AT YOU";
pub const NEAR_HINT: &str = "SOMEONE IS CLOSE";

/// How loud the cast's attention is this step. One slot, LOUDEST wins (not last, not
/// first): every sleepwalker writes what it noticed and the scene's rule reads the worst of
/// it, so the one hint line cannot be fought over. The reader takes it, which is what makes
/// the level fall silent the moment nobody is watching.
///
/// This is the same ambient-channel shape as `ext/room.rs` and `ext/tool.rs`, and it exists
/// because a `RoomLogic` closure cannot ask a `dyn ObjectT` whether it happens to be an
/// [`Npc`] -- there is no downcast in this trait, by design.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Attention {
    /// Somebody is within a couple of strides.
    Near = 1,
    /// Somebody has their eyes on you.
    Seen = 2,
}

thread_local! {
    static WATCH: std::cell::Cell<Option<Attention>> = const { std::cell::Cell::new(None) };
}

/// A sleepwalker's step says what it noticed; the loudest report of the step survives.
pub fn report(a: Attention) {
    WATCH.with(|w| {
        if w.get().is_none_or(|had| a > had) {
            w.set(Some(a));
        }
    });
}

/// The scene's rule takes it once a step and writes the hint line.
pub fn take_attention() -> Option<Attention> {
    WATCH.with(std::cell::Cell::take)
}

/// Scene-load hygiene: a street's attention must not haunt the next one.
pub fn reset() {
    WATCH.with(|w| w.set(None));
}

impl Attention {
    pub fn line(self) -> &'static str {
        match self {
            Attention::Near => NEAR_HINT,
            Attention::Seen => SEEN_HINT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A route walker with no GL: `Npc` needs an `Instance`, so the geometry the tests care
    /// about is exercised through the free functions and a hand-built base.
    fn npc_at(pos: Vector3, yaw: f32) -> Object {
        let mut o = Object::new();
        o.pos = pos;
        o.euler.y = yaw;
        o
    }

    /// The eye matrix must point where the mannequin's body points. The kernel reads a
    /// viewer's facing as the matrix's -Z; the model faces +z; the half turn is the bridge,
    /// and without it a sleepwalker watches its own back.
    #[test]
    fn the_eye_looks_the_way_the_body_walks() {
        for yaw in [0.0, GH_PI / 2.0, GH_PI, -GH_PI / 2.0, 2.3] {
            let body = npc_at(Vector3::new(3.0, 0.0, -4.0), yaw);
            let eye = Matrix4::trans(body.pos + Vector3::new(0.0, EYE_HEIGHT, 0.0))
                * Matrix4::rot_y(yaw + GH_PI);
            let looks = eye.mul_direction(Vector3::new(0.0, 0.0, -1.0)).normalized_safe();
            // Where the body walks: the model faces +z at euler.y (level32's Walker).
            let walks = Vector3::new(yaw.sin(), 0.0, yaw.cos());
            assert!(
                (looks - walks).mag() < 1e-5,
                "yaw {yaw}: eye looks {looks:?} but the body walks {walks:?}"
            );
        }
    }

    /// The range gate is the whole reason this module wraps the kernel: `is_observed` has
    /// no far limit, so without it a sleepwalker watches the length of the street.
    #[test]
    fn sight_is_gated_by_range_before_the_raycast() {
        let eye = Vector3::new(0.0, EYE_HEIGHT, 0.0);
        let far = eye + Vector3::new(0.0, 0.0, SIGHT_RANGE + 1.0);
        let near = eye + Vector3::new(0.0, 0.0, SIGHT_RANGE - 1.0);
        assert!((far - eye).mag() > SIGHT_RANGE);
        assert!((near - eye).mag() < SIGHT_RANGE);
        // And the gate is tighter than the street loop, or a sleepwalker would notice the
        // player through the seam, which the portal-blind kernel cannot reason about.
        assert!(SIGHT_RANGE < crate::level31::LOOP_LENGTH * 0.5);
    }

    #[test]
    fn the_hint_lines_fit_the_hud() {
        for line in [SEEN_HINT, NEAR_HINT] {
            assert!(line.len() <= 40, "{line:?} is too long for the hint line");
            assert!(line.is_ascii(), "{line:?} has glyphs the font atlas lacks");
        }
    }

    /// Noticing is the whole of a sleepwalker's aggression: the moods are a closed set,
    /// none of them is an attack, and none of them ends with its face in yours
    /// (docs/hide-n-dream.md's non-violent register).
    #[test]
    fn a_sleepwalker_has_no_verb_but_looking() {
        let moods = [Mood::Patrol, Mood::Pause, Mood::Curious];
        assert_eq!(moods.len(), 3);
        assert!(LOOK_SPEED < 3.0, "walking over to look must not read as a chase");
        // It stops a room's width away, not at the lens: nearer than this and the mannequin
        // is inside the camera, which is a jump scare rather than a presence.
        assert!(KEEP_BACK > 2.0, "a sleepwalker would end up in the player's face");
        assert!(KEEP_BACK < SIGHT_RANGE, "it would never approach at all");
        // And it never comes closer than the range at which a disguise is seen through,
        // or standing still as a chair could never survive being looked at.
        assert!(KEEP_BACK > RUMBLE_RANGE, "its approach alone would blow every hide");
    }

    /// The step and the clip must come from ONE predicate, or a sleepwalker that has
    /// arrived and whose quarry then backs off keeps translating with its feet planted.
    /// The dead band is what stops a player loitering at the stop distance from flipping
    /// the clip -- and re-posing the whole mesh -- several times a second.
    #[test]
    fn the_arrival_clip_and_the_arrival_step_agree() {
        // Far away, from a standing start: move.
        assert!(curious_moving(KEEP_BACK + KEEP_BACK_HYST + 1.0, false, KEEP_BACK));
        // The regression: arrived (not moving), quarry retreats -- must move again.
        assert!(curious_moving(7.6, false, KEEP_BACK), "a retreating quarry must be followed");
        // Inside the stop distance: stand, whichever state it was in.
        assert!(!curious_moving(KEEP_BACK - 0.1, true, KEEP_BACK));
        assert!(!curious_moving(KEEP_BACK - 0.1, false, KEEP_BACK));
        // In the band: keep doing whatever it was doing -- that is the whole point.
        let in_band = KEEP_BACK + KEEP_BACK_HYST * 0.5;
        assert!(curious_moving(in_band, true, KEEP_BACK), "it should not stop mid-band");
        assert!(!curious_moving(in_band, false, KEEP_BACK), "nor start mid-band");
    }

    /// The attention channel keeps the LOUDEST report of a step, so one sleepwalker's
    /// glance is not overwritten by a neighbour's indifference, and taking it empties it.
    #[test]
    fn the_loudest_watcher_of_the_step_wins() {
        reset();
        assert_eq!(take_attention(), None);
        report(Attention::Near);
        report(Attention::Seen);
        report(Attention::Near);
        assert_eq!(take_attention(), Some(Attention::Seen));
        assert_eq!(take_attention(), None, "taking the line empties it");
        report(Attention::Near);
        reset();
        assert_eq!(take_attention(), None, "a scene load clears the street");
    }
}
