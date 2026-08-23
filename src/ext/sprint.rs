//! EXT: running. Not part of the C++ port -- CodeParade's player has one speed.
//!
//! Three things live here: the per-frame decision of whether the player is sprinting, the
//! multipliers the ported movement code applies when they are, and the field-of-view kick that
//! sells the speed. The decision runs once per rendered frame, the multipliers once per 500 Hz
//! step, the kick once per frame after physics.
//!
//! # Hold on the keyboard, toggle on the pad
//!
//! Shift is a hold: sprinting for exactly as long as it is down, which is what every PC game
//! does and what a key under the little finger is good at. L3 is a stick click, and holding a
//! stick clicked while also steering with it is miserable, so the pad gets the console idiom
//! instead: a press starts the run, and it ends when the stick returns to centre or on the
//! next press. Stopping cancels the toggle -- the player does not have to remember to click
//! again, and a run never resumes unasked when they next push the stick. A click while standing
//! still is ignored for the same reason: there is nothing to toggle yet, and arming a run to
//! fire on the next step is the surprise this rule exists to prevent.
//!
//! L3 *held* also counts, so a pad player who wants the hold idiom has it. And a Shift press
//! drops a pad toggle: a player switching from pad to keyboard mid-run is in hold mode from
//! that press on, never carried by a toggle they cannot see.
//!
//! # Forward only
//!
//! A run is something you do in the direction you are facing. Whatever the hold or toggle says,
//! the sprint applies only while the movement vector has a forward component of at least
//! [`FORWARD_MIN`] of its length -- diagonals count, strafing and backpedalling walk. The toggle
//! itself is not cancelled by a sidestep (the stick has not returned to centre), so a pad
//! player who strafes round a corner mid-run is running again the moment they push forward.
//! The field-of-view kick follows the run rather than the key, so a Shift held at a standstill
//! does nothing until the player moves forward.
//!
//! # The eased speed cap
//!
//! The acceleration and bob multipliers are stepwise -- on or off with the run -- but the speed
//! cap is eased. `Player::move_player` clips the horizontal velocity to the cap every 2 ms
//! step, so a cap that dropped from 1.8x to 1.0x in one step would decelerate the player at
//! some 1,160 u/s^2 while the field of view was still easing back over its 150 ms: a visible
//! jolt. Instead [`Sprint::resolve`] carries a per-frame [`Factors::speed`] that relaxes toward
//! its target with a [`SPEED_TAU`] time constant, so the run bleeds off over a few tenths of a
//! second, in step with the view. Once within a thousandth of 1.0 on the way down it snaps to
//! exactly 1.0, which is what keeps the walk bit-identical to the port: `x * 1.0` is exact.
//!
//! # Why `resolve` runs before the fixed-step loop
//!
//! `Input::end_frame` runs inside the 500 Hz loop and zeroes `key_press` (Input.cpp:11), so
//! the Shift edge is only visible before it. The resolved level is written into
//! `Input::sprint` and read by `Player::update_player` through `UpdateCtx` on every step of
//! the frame -- so the ported movement code sees one consistent answer per rendered frame and
//! never has to know about toggles or pads.
//!
//! # The field-of-view kick
//!
//! Widening the view a few degrees is the cheapest convincing speed cue there is; the whole
//! scene streams past faster at the edges. It eases in and out with a frame-rate-independent
//! exponential so a 30 fps and a 120 fps machine reach the same width in the same 150 ms.
//! While the pause menu is up the world is frozen and the kick is left wherever it was --
//! `run_frame` returns before reaching it -- so the resume frame picks up mid-ease rather than
//! snapping; the clamp on `dt` below is what keeps a long pause from counting as one long frame.

use crate::game_header::GH_FOV;
use crate::input::Input;

/// Win32 `VK_SHIFT`, the slot `input::key_index` maps both Shift keys into.
pub const KEY_SPRINT: usize = 16;

/// Top speed while sprinting, as a multiple of the ported `GH_WALK_SPEED` cap.
pub const SPRINT_SPEED: f32 = 1.8;
/// Acceleration while sprinting, as a multiple of `GH_WALK_ACCEL`. Less than the speed
/// multiple on purpose: the run builds over a few steps rather than arriving at once.
pub const SPRINT_ACCEL: f32 = 1.5;
/// Head-bob frequency while sprinting, as a multiple of `GH_BOB_FREQ`. The cadence of the
/// footfalls (and the footstep sounds they drive) is what reads as running.
pub const SPRINT_BOB_FREQ: f32 = 1.35;
/// How far the vertical field of view widens at full sprint, in degrees.
pub const SPRINT_FOV_KICK_DEG: f32 = 8.0;
/// Time constant of the FOV ease, in seconds: 63% of the way after one, 95% after three.
const FOV_TAU: f32 = 0.15;
/// Time constant of the speed-cap ease. Shorter than the view's: the body should settle a
/// little before the eye does, or the world keeps widening after the feet have slowed.
pub const SPEED_TAU: f32 = 0.1;
/// Below this distance from 1.0, on the way down, the eased speed snaps to exactly 1.0. A
/// thousandth of the walk speed is three millimetres a second -- imperceptible -- and the snap
/// is what makes "not sprinting" an exact multiply by one rather than an asymptote.
const SPEED_SNAP: f32 = 1e-3;
/// Smallest forward share of the movement vector that still counts as running forward. 0.3 is
/// about 17 degrees either side of a pure strafe: a diagonal (0.71) runs, a sidestep with a
/// little drift does not.
pub const FORWARD_MIN: f32 = 0.3;
/// Longest frame delta the ease will integrate. Anything longer is a stall or a pause, and
/// a single step that long would skip the ease entirely.
const MAX_DT: f32 = 0.1;
/// Movement-input magnitude below which the player counts as stopped, for the auto-cancel.
/// Anything past the stick deadzone rescales to well above this (gamepad.rs `deadzone`).
const STOP_EPS: f32 = 0.05;

/// The multipliers `Player` applies. [`Factors::WALK`] is all ones, and multiplying by 1.0 is
/// exact in IEEE arithmetic, so the walk feel stays bit-identical to the port when not
/// sprinting. `accel` and `bob` are stepwise; `speed` is eased by [`Sprint::resolve`] and is
/// exactly 1.0 whenever the run is over (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Factors {
    pub speed: f32,
    pub accel: f32,
    pub bob: f32,
}

impl Factors {
    /// Not sprinting: the ported walk, to the bit.
    pub const WALK: Factors = Factors { speed: 1.0, accel: 1.0, bob: 1.0 };
}

/// The stepwise multipliers for a sprint level: full sprint or the ported walk. What
/// `resolve` starts from before easing the speed.
pub fn factors(sprinting: bool) -> Factors {
    if sprinting {
        Factors { speed: SPRINT_SPEED, accel: SPRINT_ACCEL, bob: SPRINT_BOB_FREQ }
    } else {
        Factors::WALK
    }
}

/// The sprint state machine. One per `ExtState`; reset on every scene load.
pub struct Sprint {
    /// Pad toggle mode is on. Cleared when movement input stops or Shift is pressed.
    toggled: bool,
    /// The level `resolve` last settled on: the run is on and pointed forward. What the FOV
    /// ease follows.
    running: bool,
    /// Eased speed-cap multiplier, 1.0 ..= `SPRINT_SPEED`.
    speed: f32,
    /// `view::time()` at the last `resolve` call, for the speed ease's frame delta.
    speed_time: f32,
    /// Eased 0..1 sprint amount driving the FOV kick.
    fov_kick: f32,
    /// `view::time()` at the last `ease_fov` call, for the frame delta. Separate from
    /// `speed_time` because the two eases run at different points in the frame against the
    /// same clock reading, and the second would otherwise see a delta of zero.
    fov_time: f32,
}

impl Sprint {
    pub fn new(now: f32) -> Sprint {
        Sprint {
            toggled: false,
            running: false,
            speed: 1.0,
            speed_time: now,
            fov_kick: 0.0,
            fov_time: now,
        }
    }

    /// Scene load: no toggle survives, the speed cap is the walk's, and the view starts at the
    /// ported FOV (which `view::reset_fov` has already restored) so nothing leaks into the
    /// next scene.
    pub fn reset(&mut self, now: f32) {
        *self = Sprint::new(now);
    }

    /// This frame's multipliers. Call once per rendered frame, BEFORE the fixed-step loop
    /// (see the module docs), with the pad's L3 rising edge and the frame clock.
    pub fn resolve(&mut self, input: &Input, pad_edge: bool, now: f32) -> Factors {
        // The same vector `Player::update_player` builds, so "stopped" and "forward" mean the
        // same thing to the decision as they do to the feet.
        let mut move_f = input.pad_move_f;
        let mut move_l = input.pad_move_l;
        if input.key[b'W' as usize] {
            move_f += 1.0;
        }
        if input.key[b'S' as usize] {
            move_f -= 1.0;
        }
        if input.key[b'A' as usize] {
            move_l += 1.0;
        }
        if input.key[b'D' as usize] {
            move_l -= 1.0;
        }
        let mag = (move_f * move_f + move_l * move_l).sqrt();
        let moving = mag > STOP_EPS;
        let forward = moving && move_f >= FORWARD_MIN * mag;

        if !moving {
            self.toggled = false;
        } else if pad_edge {
            self.toggled = !self.toggled;
        }
        if input.key_press[KEY_SPRINT] {
            self.toggled = false;
        }

        self.running = (input.key[KEY_SPRINT] || input.pad_sprint || self.toggled) && forward;

        // Ease the speed cap toward this frame's target; see the module docs for why this one
        // multiplier is not stepwise. The same exact-exponential step as the FOV below.
        let dt = (now - self.speed_time).clamp(0.0, MAX_DT);
        self.speed_time = now;
        let target = if self.running { SPRINT_SPEED } else { 1.0 };
        self.speed += (target - self.speed) * (1.0 - (-dt / SPEED_TAU).exp());
        // Snap only on the way DOWN: a snap toward 1.0 while the target is 1.8 would, at a
        // frame rate high enough for one step to move less than the threshold, pin the run at
        // a walk for ever.
        if !self.running && (self.speed - 1.0).abs() < SPEED_SNAP {
            self.speed = 1.0;
        }

        Factors { speed: self.speed, ..factors(self.running) }
    }

    /// Advance the FOV ease to `now`, following the level `resolve` settled on this frame, and
    /// return the field of view to render with, in degrees. Call once per rendered frame,
    /// after the fixed-step loop.
    pub fn ease_fov(&mut self, now: f32) -> f32 {
        let dt = (now - self.fov_time).clamp(0.0, MAX_DT);
        self.fov_time = now;
        let target = if self.running { 1.0 } else { 0.0 };
        // x += (target - x) * (1 - e^(-dt/tau)) is the exact solution of dx/dt = (target - x)/tau
        // over one step, so the curve is the same whatever the frame rate.
        self.fov_kick += (target - self.fov_kick) * (1.0 - (-dt / FOV_TAU).exp());
        GH_FOV + SPRINT_FOV_KICK_DEG * self.fov_kick
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walking() -> Input {
        let mut input = Input::new();
        input.key[b'W' as usize] = true;
        input
    }

    /// `resolve` at time zero, so the eases are not in play: just the level.
    fn level(s: &mut Sprint, input: &Input, pad_edge: bool) -> bool {
        s.resolve(input, pad_edge, 0.0);
        s.running
    }

    #[test]
    fn factors_are_identity_when_not_sprinting() {
        assert_eq!(factors(false), Factors::WALK);
        assert_eq!(Factors::WALK, Factors { speed: 1.0, accel: 1.0, bob: 1.0 });
        let f = factors(true);
        assert!(f.speed > 1.0 && f.accel > 1.0 && f.bob > 1.0);
    }

    #[test]
    fn blank_input_never_sprints() {
        let mut s = Sprint::new(0.0);
        assert!(!level(&mut s, &Input::new(), false));
        // Not even with a toggle edge: there is no movement to toggle.
        assert!(!level(&mut s, &Input::new(), true));
        assert!(!level(&mut s, &Input::new(), false));
        // Nor with Shift down: a run needs somewhere to go.
        let mut input = Input::new();
        input.key[KEY_SPRINT] = true;
        assert!(!level(&mut s, &input, false));
    }

    #[test]
    fn shift_is_a_hold() {
        let mut s = Sprint::new(0.0);
        let mut input = walking();
        input.key[KEY_SPRINT] = true;
        input.key_press[KEY_SPRINT] = true;
        assert!(level(&mut s, &input, false));
        input.key_press[KEY_SPRINT] = false;
        assert!(level(&mut s, &input, false));
        input.key[KEY_SPRINT] = false;
        assert!(!level(&mut s, &input, false), "releasing Shift ends the run");
    }

    #[test]
    fn l3_toggles_while_moving() {
        let mut s = Sprint::new(0.0);
        let mut input = Input::new();
        input.pad_move_f = 0.8;
        assert!(!level(&mut s, &input, false));
        assert!(level(&mut s, &input, true), "press: on");
        assert!(level(&mut s, &input, false), "stays on with the button released");
        assert!(!level(&mut s, &input, true), "press again: off");
        assert!(!level(&mut s, &input, false));
    }

    #[test]
    fn l3_held_counts_as_hold() {
        let mut s = Sprint::new(0.0);
        let mut input = Input::new();
        input.pad_move_f = 1.0;
        input.pad_sprint = true;
        // Held, with its press edge already spent while the stick was centred (so nothing
        // was toggled): the level alone runs...
        assert!(level(&mut s, &input, false));
        input.pad_sprint = false;
        assert!(!level(&mut s, &input, false), "...and release stops, there being no toggle");
        // Held from a standing start: nothing to run with, whatever the button says.
        input.pad_move_f = 0.0;
        input.pad_sprint = true;
        assert!(!level(&mut s, &input, true));
    }

    #[test]
    fn stopping_cancels_the_toggle() {
        let mut s = Sprint::new(0.0);
        let mut input = Input::new();
        input.pad_move_f = 1.0;
        assert!(level(&mut s, &input, true));
        input.pad_move_f = 0.0;
        assert!(!level(&mut s, &input, false), "stick centred: run over");
        input.pad_move_f = 1.0;
        assert!(!level(&mut s, &input, false), "pushing again walks; the toggle does not come back");
    }

    #[test]
    fn keyboard_movement_keeps_a_pad_toggle_alive() {
        let mut s = Sprint::new(0.0);
        let mut input = walking();
        assert!(level(&mut s, &input, true));
        input.pad_move_f = 0.0;
        assert!(level(&mut s, &input, false), "W is still down, so the player is still moving");
    }

    #[test]
    fn shift_press_drops_a_pad_toggle() {
        let mut s = Sprint::new(0.0);
        let mut input = walking();
        assert!(level(&mut s, &input, true));
        input.key[KEY_SPRINT] = true;
        input.key_press[KEY_SPRINT] = true;
        assert!(level(&mut s, &input, false), "held Shift sprints");
        input.key[KEY_SPRINT] = false;
        input.key_press[KEY_SPRINT] = false;
        assert!(!level(&mut s, &input, false), "and its release ends the run: hold mode now");
    }

    #[test]
    fn only_a_forward_run_sprints() {
        let mut s = Sprint::new(0.0);
        let mut input = Input::new();
        input.key[KEY_SPRINT] = true;
        // Strafe, backpedal, and a back diagonal: all walk.
        for keys in [&[b'A'][..], &[b'D'], &[b'S'], &[b'S', b'A']] {
            input.key = [false; 256];
            input.key[KEY_SPRINT] = true;
            for k in keys {
                input.key[*k as usize] = true;
            }
            assert!(!level(&mut s, &input, false), "{keys:?} should walk");
        }
        // Forward and the forward diagonals run (0.71 of the vector is forward).
        for keys in [&[b'W'][..], &[b'W', b'A'], &[b'W', b'D']] {
            input.key = [false; 256];
            input.key[KEY_SPRINT] = true;
            for k in keys {
                input.key[*k as usize] = true;
            }
            assert!(level(&mut s, &input, false), "{keys:?} should run");
        }
        // The stick, either side of the threshold.
        input.key = [false; 256];
        input.key[KEY_SPRINT] = true;
        input.pad_move_l = 0.9;
        input.pad_move_f = 0.2; // 0.22 of the vector: a sidestep with drift
        assert!(!level(&mut s, &input, false));
        input.pad_move_f = 0.4; // 0.41: a forward-ish diagonal
        assert!(level(&mut s, &input, false));
    }

    #[test]
    fn a_sidestep_keeps_the_toggle_but_not_the_run() {
        let mut s = Sprint::new(0.0);
        let mut input = Input::new();
        input.pad_move_f = 1.0;
        assert!(level(&mut s, &input, true), "toggled on");
        input.pad_move_f = 0.0;
        input.pad_move_l = 1.0;
        assert!(!level(&mut s, &input, false), "strafing walks...");
        input.pad_move_f = 1.0;
        input.pad_move_l = 0.0;
        assert!(level(&mut s, &input, false), "...and forward again is running again");
    }

    /// Run `s` for `secs` at 60 fps, Shift held or not, and return the last speed factor.
    fn ease_speed(s: &mut Sprint, t: &mut f32, secs: f32, shift: bool) -> f32 {
        let mut input = walking();
        input.key[KEY_SPRINT] = shift;
        let mut f = Factors::WALK;
        for _ in 0..(secs * 60.0) as usize {
            *t += 1.0 / 60.0;
            f = s.resolve(&input, false, *t);
        }
        f.speed
    }

    #[test]
    fn speed_eases_up_and_back_down_and_snaps_to_one() {
        let mut s = Sprint::new(0.0);
        let mut t = 0.0;
        // One frame in: some of the way, nowhere near all of it.
        let first = ease_speed(&mut s, &mut t, 1.0 / 60.0, true);
        assert!(first > 1.05 && first < 1.3, "first frame: {first}");
        // A second of running: converged.
        let full = ease_speed(&mut s, &mut t, 1.0, true);
        assert!((full - SPRINT_SPEED).abs() < 1e-3, "{full}");
        // Release: the first frame is still most of a run, not a walk -- the jolt this exists
        // to remove -- and the cap then descends monotonically.
        let input = walking(); // `walking()` has Shift up already
        t += 1.0 / 60.0;
        let released = s.resolve(&input, false, t);
        let mut prev = released.speed;
        assert!(prev > 1.6 && prev < full, "released: {prev}");
        // Only the speed cap eases: the stepwise factors are off from the FIRST released
        // frame, while the cap is still most of a run.
        assert_eq!((released.accel, released.bob), (1.0, 1.0));
        let mut snapped_at = None;
        for i in 0..120 {
            t += 1.0 / 60.0;
            let cur = s.resolve(&input, false, t).speed;
            assert!(cur <= prev + 1e-6, "rose from {prev} to {cur}");
            prev = cur;
            if cur == 1.0 && snapped_at.is_none() {
                snapped_at = Some(i);
            }
        }
        // Exactly 1.0 -- the bits the walk-cap test in player.rs relies on -- and reached well
        // before the asymptote would have: ln(0.8 / 1e-3) time constants is ~0.67 s.
        assert_eq!(prev.to_bits(), 1.0f32.to_bits());
        let i = snapped_at.expect("never snapped");
        assert!((30..50).contains(&i), "snapped on frame {i}");
        // And still off on the last.
        let f = s.resolve(&input, false, t);
        assert_eq!((f.accel, f.bob), (1.0, 1.0));
    }

    #[test]
    fn speed_ease_is_frame_rate_independent() {
        let mut input = walking();
        input.key[KEY_SPRINT] = true;
        let mut slow = Sprint::new(0.0);
        let mut fast = Sprint::new(0.0);
        let a = slow.resolve(&input, false, 0.09).speed;
        let mut b = 0.0;
        for i in 1..=9 {
            b = fast.resolve(&input, false, i as f32 * 0.01).speed;
        }
        assert!((a - b).abs() < 1e-4, "{a} vs {b}");
    }

    #[test]
    fn fov_eases_in_and_back_out() {
        let mut s = Sprint::new(0.0);
        assert!((s.ease_fov(0.0) - GH_FOV).abs() < 1e-6);
        let mut input = walking();
        input.key[KEY_SPRINT] = true;
        s.resolve(&input, false, 0.0);
        // One time constant in, over three frames short enough not to hit the stall clamp:
        // about 63% of the kick.
        let mut t = 0.0;
        let mut f = 0.0;
        for _ in 0..3 {
            t += FOV_TAU / 3.0;
            f = s.ease_fov(t);
        }
        let frac = (f - GH_FOV) / SPRINT_FOV_KICK_DEG;
        assert!((frac - 0.632).abs() < 0.01, "got {frac}");
        // Keep sprinting for a second at 60 fps: converged.
        for _ in 0..60 {
            t += 1.0 / 60.0;
            f = s.ease_fov(t);
        }
        assert!((f - (GH_FOV + SPRINT_FOV_KICK_DEG)).abs() < 1e-2);
        // Stop: back to within a hundredth of a degree of the ported FOV after ten time
        // constants, monotonically.
        input.key[KEY_SPRINT] = false;
        s.resolve(&input, false, t);
        let mut prev = f;
        for _ in 0..90 {
            t += 1.0 / 60.0;
            let cur = s.ease_fov(t);
            assert!(cur <= prev + 1e-6);
            prev = cur;
        }
        assert!((prev - GH_FOV).abs() < 1e-2);
    }

    #[test]
    fn fov_ease_is_frame_rate_independent() {
        let mut input = walking();
        input.key[KEY_SPRINT] = true;
        let mut slow = Sprint::new(0.0);
        let mut fast = Sprint::new(0.0);
        slow.resolve(&input, false, 0.0);
        fast.resolve(&input, false, 0.0);
        let a = slow.ease_fov(0.09);
        let mut b = 0.0;
        for i in 1..=9 {
            b = fast.ease_fov(i as f32 * 0.01);
        }
        // 90 ms in one (11 fps) frame vs nine (100 fps) frames: the exact exponential gives
        // the same answer, up to float rounding over nine multiplies.
        assert!((a - b).abs() < 1e-4, "{a} vs {b}");
    }

    #[test]
    fn a_long_stall_still_eases() {
        let mut s = Sprint::new(0.0);
        let mut input = walking();
        input.key[KEY_SPRINT] = true;
        // A five-second pause then one frame of sprint integrates one clamped step, not five
        // seconds -- so both the kick and the cap are well short of full.
        let f = s.resolve(&input, false, 5.0);
        let expect = 1.0 - (-MAX_DT / SPEED_TAU).exp();
        assert!(((f.speed - 1.0) / (SPRINT_SPEED - 1.0) - expect).abs() < 1e-5);
        let f = s.ease_fov(5.0);
        let expect = 1.0 - (-MAX_DT / FOV_TAU).exp();
        assert!(((f - GH_FOV) / SPRINT_FOV_KICK_DEG - expect).abs() < 1e-5);
        // ...and a reset puts both straight back.
        s.reset(5.0);
        assert!((s.ease_fov(5.0) - GH_FOV).abs() < 1e-6);
        assert_eq!(s.resolve(&Input::new(), false, 5.0), Factors::WALK);
    }
}
