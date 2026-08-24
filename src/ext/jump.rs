//! EXT: jumping -- the mechanic CodeParade wrote and then switched off.
//!
//! `Player::Update` carries the whole thing inside an `#if 0` (Player.cpp:61-66): two lines
//! that add a flat 2 units of upward velocity while Space is down and `onGround` is set. Turned
//! back on as written it misfires twice over, both times on the engine's order of business, which
//! is update-then-collide. `on_ground` is set by the collision pass at the *end* of a step and
//! read by the *next* step's update, so on the step after a launch the feet are still in the
//! floor, `on_ground` is still true, and the impulse lands a second and a third time. And the
//! block sits after the physics step, so the impulse misses that step's position integration and
//! the collision pass finds a player still inside the carpet and cancels the velocity it was
//! given. This module is the small state machine that stands between the button and that one
//! line; the ordering fix is one move in `Player::update_player`.
//!
//! # The numbers, and where they come from
//!
//! **Apex [`APEX`] = 0.62 m.** High enough to step up onto the Backrooms' low furniture and over
//! the pool room's kerbs, and less than a third of the 2.43 m ceilings, so no jump anywhere in
//! the game can put the player's head through one. The impulse is *derived* from it (see
//! [`impulse`]) rather than tuned by hand, so moving the apex moves the jump and nothing else.
//!
//! **Coyote time [`COYOTE_TIME`] = 0.12 s.** A jump pressed within that long after walking off an
//! edge still fires. Players press jump when they see the edge arrive, not when their feet leave
//! it, and at a 2.9 u/s walk 0.12 s is 35 cm of floor -- enough to forgive the reaction, short
//! enough that nobody ever notices they were already falling.
//!
//! **Buffer [`BUFFER_TIME`] = 0.12 s.** A jump pressed within that long *before* landing fires on
//! touchdown instead of being dropped. The mirror image of the same reaction time, and what makes
//! a run of jumps chain instead of stuttering.
//!
//! **Launch lockout [`LAUNCH_LOCKOUT`] = 0.05 s.** The guard against the first misfire above: for
//! that long after a launch, a ground contact is the floor the player has not left yet rather
//! than a landing, and it is ignored. In the ordinary case nothing needs it -- with the impulse
//! applied before the physics step, the very step that jumps lifts the player 8 mm clear of a
//! floor they were four hundredths of a millimetre inside, and the collision pass finds nothing.
//! It earns its keep where the player cannot rise: a jump taken under something low enough to
//! bonk a head on. 25 steps is short enough that no real landing can hide inside it -- the
//! shortest flight is 0.71 s -- and long enough that a bonk cannot chain into a second impulse on
//! the way back down.
//!
//! # Why the button is a level and not an edge
//!
//! `Input::end_frame` is called *inside* the engine's fixed-step loop (Engine.cpp:112), so the
//! `key_press` edge slots are cleared after the first step of a rendered frame. A jump read off
//! an edge would therefore be visible to exactly one step per frame and invisible to the other
//! seven, which is not a race worth having. The level (`Input::key[' ']`, `Input::pad_jump`) is
//! true for every step of the frame instead, and this machine is what turns a level held for four
//! hundred steps into one impulse. Held across a landing it hops again, which is the behaviour
//! the buffer already implies and the one every game with a level-read jump has.

use crate::game_header::{GH_DT, GH_GRAVITY};

/// The height a jump reaches, in units, at `p_scale` 1.
pub const APEX: f32 = 0.62;

/// How long after leaving the ground a jump may still be pressed.
pub const COYOTE_TIME: f32 = 0.12;

/// How long before touchdown a jump may be pressed and still fire on it.
pub const BUFFER_TIME: f32 = 0.12;

/// How long a ground contact is ignored after a launch. See the module docs.
pub const LAUNCH_LOCKOUT: f32 = 0.05;

/// How long the player must have been off the ground for the next contact to count as a
/// *landing* (`Player::just_landed`).
///
/// Not a nicety. The Backrooms floor is a scanned triangle mesh, and walking over its seams and
/// the feet of its furniture leaves the player unsupported for a step or two at a time, over and
/// over. With nothing under it, `just_landed` answered every one of them: twelve touchdowns
/// between 0.03 and 0.5 u/s in a two-metre walk, measured. 0.08 s is the fall out of a 3 cm drop
/// -- below that it is a scuff, above it is a kerb or a jump.
pub const LANDING_MIN_AIR: f32 = 0.08;

/// Touchdown speed, in units per second, at or above which a landing is a thump rather than a
/// scuff (`landing_sfx`).
///
/// A jump from [`APEX`] lands at **3.05-3.08 u/s** in game (3.09 through the integrator), so an
/// ordinary hop has to be below this and a real drop above it. Four is a quarter clear of the hop
/// -- far enough that the walk's own micro-drops and a jump taken while running downhill still
/// read as soft -- and is reached by falling about **0.85 m**, which is a storey's step rather
/// than a kerb. Nothing in the shipped scenes is high enough to jump off and land hard; the pool
/// basin, a 1.19 m drop, is.
pub const HARD_LANDING: f32 = 4.0;

/// Which landing sound a touchdown at `speed` (downward, u/s) is.
///
/// Here rather than at the call site because the threshold is a fact about the jump: it is chosen
/// against the arc [`APEX`] produces, and moving the apex would move it.
pub fn landing_sfx(speed: f32) -> crate::ext::audio::Sfx {
    if speed >= HARD_LANDING {
        crate::ext::audio::Sfx::LandHard
    } else {
        crate::ext::audio::Sfx::LandSoft
    }
}

/// The player's own drag, set by `Player::reset` (Player.cpp:20). Named here because the apex
/// depends on it and the derivation below has to see it; the ported line remains the source of
/// truth, and `the_drag_solved_for_is_the_players_own` pins the two together so this copy cannot
/// drift away from it.
const PLAYER_DRAG: f32 = 0.002;

/// Where the two timers stop counting. Anything above both windows will do; it only keeps a long
/// fall from walking `since_ground` into the numbers where an `f32` stops resolving `GH_DT`.
const STALE: f32 = 1.0;

/// The height an upward impulse of `v0` actually reaches, at `p_scale` 1.
///
/// Not `v^2 / 2g`: `Physical::update` multiplies the velocity by `1 - drag` every step
/// (Physical.cpp:22), which is a continuous decay of `k = -ln(1 - drag) / GH_DT` per second --
/// one per second for the player's 0.002, a time constant the same order as the flight itself.
/// Solving `dv/dt = -g - k v` and integrating to the apex gives the closed form below.
fn rise(v0: f32) -> f32 {
    let k = -(1.0 - PLAYER_DRAG).ln() / GH_DT;
    let g = -GH_GRAVITY;
    v0 / k - (g / (k * k)) * (1.0 + k * v0 / g).ln()
}

/// The upward impulse a jump adds to the player's velocity, for a player at `p_scale`.
///
/// `v = sqrt(2 g h)` is the vacuum answer and the player is not in a vacuum: through their own
/// drag it reaches 0.50 m where 0.62 was asked for, a fifth of the height given away to a
/// constant that is right there in `Player::reset`. So the vacuum answer is the first guess and
/// nothing more. [`rise`] says what an impulse really reaches, it rises with `v0`, and thirty
/// bisections between the vacuum answer (always short) and twice it (always long) close the gap
/// far below what an `f32` can hold. Thirty logarithms at most twice a second is nothing, and the
/// alternative is a pasted constant nobody can re-derive when the apex moves.
///
/// Scaled by `p_scale` exactly as the ported stub scaled its 2.0. That is not a convention but
/// the arithmetic: gravity is scaled by `p_scale` too (Physical.cpp:21) while the drag is not, and
/// `rise` is linear in both together -- so a player of any size clears [`APEX`] times their own
/// scale, which is the same jump measured in their own body-lengths.
pub fn impulse(p_scale: f32) -> f32 {
    let mut lo = (2.0 * -GH_GRAVITY * APEX).sqrt();
    let mut hi = lo * 2.0;
    for _ in 0..30 {
        let mid = 0.5 * (lo + hi);
        if rise(mid) < APEX {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi) * p_scale
}

/// The state between the jump button and the impulse: coyote time, the buffer, and the lockout
/// that keeps one press from launching the player twice. Stepped once per fixed step by
/// `Player::update_player`, which is the only thing that owns one.
#[derive(Debug)]
pub struct Jump {
    /// Seconds since the player was last standing on something, capped at [`STALE`].
    since_ground: f32,
    /// Seconds since the button was last down, capped at [`STALE`].
    since_press: f32,
    /// Seconds of [`LAUNCH_LOCKOUT`] left.
    lockout: f32,
    /// The button must be seen released before it counts again. See [`Jump::ignore_until_release`].
    blocked: bool,
    /// Whether the player was off the ground going into this step; see [`Jump::airborne`].
    airborne: bool,
}

impl Jump {
    /// A player stood still on the ground with nothing pressed. `since_press` starts stale
    /// rather than zero: a fresh zero reads as "pressed this instant" and would launch the
    /// player on the first step of the level.
    pub fn new() -> Jump {
        Jump {
            since_ground: 0.0,
            since_press: STALE,
            lockout: 0.0,
            blocked: false,
            airborne: false,
        }
    }

    /// One fixed step. `on_ground` is the player's flag as the step begins -- which is to say the
    /// verdict of the *previous* step's collision pass -- and `pressed` is the jump button's
    /// level from either instrument. Returns true on the step the impulse should be applied.
    pub fn step(&mut self, on_ground: bool, pressed: bool) -> bool {
        self.lockout = (self.lockout - GH_DT).max(0.0);
        self.airborne = !on_ground || self.lockout > 0.0;
        self.since_ground = if self.airborne { stale(self.since_ground + GH_DT) } else { 0.0 };

        // A blocked button is not "not pressed": it has to go up before it can come down again.
        if !pressed {
            self.blocked = false;
        }
        self.since_press =
            if pressed && !self.blocked { 0.0 } else { stale(self.since_press + GH_DT) };

        if self.since_press <= BUFFER_TIME && self.since_ground <= COYOTE_TIME {
            // Both windows are spent by the jump they fired -- otherwise the coyote time left
            // over from the launch would answer a second press a few steps later as a double
            // jump, and the buffer would answer it as a second impulse on the way up.
            self.since_press = STALE;
            self.since_ground = STALE;
            self.lockout = LAUNCH_LOCKOUT;
            self.airborne = true;
            return true;
        }
        false
    }

    /// Seconds the player has been off the ground going into this step, the lockout included, and
    /// zero while the floor is holding them up. Read by `Player::on_collide` to tell a landing
    /// from the floor a standing player is pushed out of every step, and from the step or two of
    /// air a seam in the scan drops them through -- hence [`LANDING_MIN_AIR`] rather than a plain
    /// "were they airborne". Stops counting at a second, which is well past anything that asks.
    pub fn air_time(&self) -> f32 {
        self.since_ground
    }

    /// Swallow the button until it is released. The menus confirm on Space and on Cross, which
    /// are also the jump on both instruments; without this, CONTINUE would launch the player on
    /// the frame the menu closed under them.
    pub fn ignore_until_release(&mut self) {
        self.blocked = true;
    }
}

impl Default for Jump {
    fn default() -> Jump {
        Jump::new()
    }
}

fn stale(t: f32) -> f32 {
    t.min(STALE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physical::Physical;

    /// Fly an impulse with the engine's own integrator, from a floor at y = 0, and return the
    /// apex and the seconds spent in the air.
    fn flight(v0: f32) -> (f32, f32) {
        let mut p = Physical::new();
        p.drag = PLAYER_DRAG;
        p.velocity.y = v0;
        let mut apex = 0.0f32;
        let mut steps = 0u32;
        loop {
            p.update();
            steps += 1;
            apex = apex.max(p.base.pos.y);
            if p.base.pos.y < 0.0 {
                return (apex, steps as f32 * GH_DT);
            }
        }
    }

    /// Fall from `height` with an initial upward `v0`, through the same integrator, and return
    /// the downward speed at the moment the floor is reached.
    fn touchdown(v0: f32, height: f32) -> f32 {
        let mut p = Physical::new();
        p.drag = PLAYER_DRAG;
        p.velocity.y = v0;
        p.base.pos.y = height;
        loop {
            p.update();
            if p.base.pos.y <= 0.0 {
                return -p.velocity.y;
            }
        }
    }

    /// Which landing is which, measured against the arcs the threshold has to tell apart rather
    /// than asserted on the number itself.
    #[test]
    fn an_ordinary_jump_lands_soft_and_a_real_drop_lands_hard() {
        use crate::ext::audio::Sfx;
        let hop = touchdown(impulse(1.0), 0.0);
        assert!((3.0..3.2).contains(&hop), "a jump from the apex lands at {hop} u/s");
        assert_eq!(landing_sfx(hop), Sfx::LandSoft, "an ordinary hop is a scuff");
        // A step off the pool basin's edge -- 1.19 m, the one real drop in the shipped scenes.
        let basin = touchdown(0.0, 1.19);
        assert!(basin >= HARD_LANDING, "the basin lands at {basin} u/s");
        assert_eq!(landing_sfx(basin), Sfx::LandHard);
        // And the scanned floor's own micro-drops, which `LANDING_MIN_AIR` suppresses but which
        // must never be a thump if one ever gets through.
        assert_eq!(landing_sfx(0.5), Sfx::LandSoft);
    }

    #[test]
    fn the_drag_solved_for_is_the_players_own() {
        assert_eq!(crate::player::Player::new().base.drag.to_bits(), PLAYER_DRAG.to_bits());
    }

    #[test]
    fn the_derived_impulse_reaches_the_apex_through_the_real_integrator() {
        let (apex, airborne) = flight(impulse(1.0));
        assert!((apex - APEX).abs() < 0.01, "apex {apex} vs {APEX}");
        // And the flight is long enough to read as a jump rather than a stumble.
        assert!((0.6..0.8).contains(&airborne), "airborne {airborne} s");
        // The vacuum answer is the one this exists to correct: it falls a good 10 cm short.
        let (vacuum, _) = flight((2.0 * -GH_GRAVITY * APEX).sqrt());
        assert!(vacuum < APEX - 0.1, "vacuum apex {vacuum}");
    }

    #[test]
    fn the_apex_scales_with_the_player() {
        // Gravity scales with `p_scale` and the drag does not, and the apex still comes out
        // proportional -- a half-size player clears half as much world.
        for scale in [0.5f32, 1.0, 2.0, 7.0] {
            let mut p = Physical::new();
            p.drag = PLAYER_DRAG;
            p.base.p_scale = scale;
            p.velocity.y = impulse(scale);
            let mut apex = 0.0f32;
            while p.base.pos.y >= 0.0 || apex == 0.0 {
                p.update();
                apex = apex.max(p.base.pos.y);
            }
            assert!((apex - APEX * scale).abs() < 0.01 * scale, "scale {scale}: apex {apex}");
        }
    }

    /// Run `n` steps with the button and the ground held as given, counting the launches.
    fn launches(j: &mut Jump, n: usize, on_ground: bool, pressed: bool) -> usize {
        (0..n).filter(|_| j.step(on_ground, pressed)).count()
    }

    #[test]
    fn a_press_on_the_ground_fires_once_and_the_floor_under_it_does_not_fire_again() {
        let mut j = Jump::new();
        // Standing there doing nothing.
        assert_eq!(launches(&mut j, 100, true, false), 0);
        // The press, and then the floor the player has not left yet: exactly one impulse. One
        // step short of the whole window, because the step that spends the last of the lockout
        // is already out of it -- a player still standing on the floor 50 ms after a launch is
        // one who never left it, and a held button hops them again, which is what it should do.
        assert!(j.step(true, true));
        let lockout_steps = (LAUNCH_LOCKOUT / GH_DT) as usize - 1;
        assert_eq!(launches(&mut j, lockout_steps, true, true), 0, "the lockout swallows it");
        // And once the feet are really clear, the held button is not a double jump either.
        assert_eq!(launches(&mut j, 300, false, true), 0);
    }

    #[test]
    fn a_held_button_hops_again_on_landing() {
        let mut j = Jump::new();
        assert!(j.step(true, true));
        launches(&mut j, 350, false, true);
        // Touchdown: the button never went up, and the buffer is what fires it.
        assert!(j.step(true, true), "a held button hops again");
    }

    #[test]
    fn coyote_time_forgives_a_late_press_and_only_that_long() {
        // Walked off the edge, pressed just inside the window.
        let mut j = Jump::new();
        j.step(true, false);
        let inside = (COYOTE_TIME / GH_DT) as usize - 1;
        assert_eq!(launches(&mut j, inside, false, false), 0);
        assert!(j.step(false, true), "inside the window it fires");
        // Pressed just outside it: nothing, and no double jump later either.
        let mut j = Jump::new();
        j.step(true, false);
        let outside = (COYOTE_TIME / GH_DT) as usize + 2;
        assert_eq!(launches(&mut j, outside, false, false), 0);
        assert_eq!(launches(&mut j, 200, false, true), 0, "too late is too late");
    }

    #[test]
    fn a_press_just_before_landing_fires_on_touchdown() {
        let mut j = Jump::new();
        // Falling, well past coyote time.
        assert_eq!(launches(&mut j, 500, false, false), 0);
        // One step's press, then released, inside the buffer window.
        assert!(!j.step(false, true));
        let wait = (BUFFER_TIME / GH_DT) as usize - 2;
        assert_eq!(launches(&mut j, wait, false, false), 0);
        assert!(j.step(true, false), "the buffered press fires on touchdown");

        // The same press a hair too early is dropped rather than remembered.
        let mut j = Jump::new();
        launches(&mut j, 500, false, false);
        j.step(false, true);
        let wait = (BUFFER_TIME / GH_DT) as usize + 2;
        assert_eq!(launches(&mut j, wait, false, false), 0);
        assert!(!j.step(true, false));
    }

    #[test]
    fn a_blocked_button_waits_for_its_release() {
        let mut j = Jump::new();
        j.ignore_until_release();
        assert_eq!(launches(&mut j, 500, true, true), 0, "held from the menu: nothing");
        // Released, and the very next press works.
        assert!(!j.step(true, false));
        assert!(j.step(true, true));
    }

    #[test]
    fn air_time_is_zero_only_for_a_player_the_floor_is_holding_up() {
        let mut j = Jump::new();
        j.step(true, false);
        assert_eq!(j.air_time(), 0.0);
        j.step(false, false);
        assert!(j.air_time() > 0.0);
        j.step(true, false);
        assert_eq!(j.air_time(), 0.0, "back on the ground");
        // Through a launch it counts even while the collision pass still says otherwise.
        assert!(j.step(true, true));
        assert!(j.air_time() > LANDING_MIN_AIR);
        j.step(true, false);
        assert!(j.air_time() > 0.0, "the floor it has not left yet is not the ground");
    }

    #[test]
    fn a_step_or_two_of_air_is_not_a_landing() {
        // A seam in the scanned floor drops the player for a step or two at a time. Below
        // `LANDING_MIN_AIR` of air the next contact is a scuff, not a touchdown.
        let mut j = Jump::new();
        j.step(true, false);
        for _ in 0..3 {
            j.step(false, false);
            assert!(j.air_time() < LANDING_MIN_AIR);
        }
        // A real drop passes it well before it ends: 0.08 s is a 3 cm fall. Two steps of slack,
        // because forty accumulated `GH_DT`s land a hair under the constant they add up to.
        let mut j = Jump::new();
        for _ in 0..((LANDING_MIN_AIR / GH_DT) as usize + 2) {
            j.step(false, false);
        }
        assert!(j.air_time() >= LANDING_MIN_AIR);
    }
}
