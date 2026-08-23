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
//! instead: a press *toggles* the run, and letting the stick return to centre ends it. Stopping
//! cancels the toggle -- the player does not have to remember to click again, and a run never
//! resumes unasked when they next push the stick. A click while standing still is ignored for
//! the same reason: there is nothing to toggle yet, and arming a run to fire on the next step
//! is the surprise this rule exists to prevent.
//!
//! L3 *held* also counts, so a pad player who wants the hold idiom has it. And a Shift press
//! drops a pad toggle: a player switching from pad to keyboard mid-run is in hold mode from
//! that press on, never carried by a toggle they cannot see.
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
/// Longest frame delta the ease will integrate. Anything longer is a stall or a pause, and
/// a single step that long would skip the ease entirely.
const MAX_DT: f32 = 0.1;
/// Movement-input magnitude below which the player counts as stopped, for the auto-cancel.
/// Anything past the stick deadzone rescales to well above this (gamepad.rs `deadzone`).
const STOP_EPS: f32 = 0.05;

/// The multipliers `Player` applies. `factors(false)` is all ones, and multiplying by 1.0 is
/// exact in IEEE arithmetic, so the walk feel stays bit-identical to the port when not
/// sprinting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Factors {
    pub speed: f32,
    pub accel: f32,
    pub bob: f32,
}

pub fn factors(sprinting: bool) -> Factors {
    if sprinting {
        Factors { speed: SPRINT_SPEED, accel: SPRINT_ACCEL, bob: SPRINT_BOB_FREQ }
    } else {
        Factors { speed: 1.0, accel: 1.0, bob: 1.0 }
    }
}

/// The sprint state machine. One per `ExtState`; reset on every scene load.
pub struct Sprint {
    /// Pad toggle mode is on. Cleared when movement input stops or Shift is pressed.
    toggled: bool,
    /// Eased 0..1 sprint amount driving the FOV kick.
    fov_kick: f32,
    /// `view::time()` at the last `ease_fov` call, for the frame delta.
    last_time: f32,
}

impl Sprint {
    pub fn new(now: f32) -> Sprint {
        Sprint { toggled: false, fov_kick: 0.0, last_time: now }
    }

    /// Scene load: no toggle survives, and the view starts at the ported FOV (which
    /// `view::reset_fov` has already restored) so the kick cannot leak into the next scene.
    pub fn reset(&mut self, now: f32) {
        *self = Sprint::new(now);
    }

    /// This frame's sprint level. Call once per rendered frame, BEFORE the fixed-step loop
    /// (see the module docs), with the pad's L3 rising edge.
    pub fn resolve(&mut self, input: &Input, pad_edge: bool) -> bool {
        // The same vector `Player::update_player` builds, so "stopped" means the same thing
        // to the toggle as it does to the feet.
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
        let moving = (move_f * move_f + move_l * move_l).sqrt() > STOP_EPS;

        if !moving {
            self.toggled = false;
        } else if pad_edge {
            self.toggled = !self.toggled;
        }
        if input.key_press[KEY_SPRINT] {
            self.toggled = false;
        }

        input.key[KEY_SPRINT] || input.pad_sprint || self.toggled
    }

    /// Advance the FOV ease to `now` and return the field of view to render with, in degrees.
    /// Call once per rendered frame, after the fixed-step loop.
    pub fn ease_fov(&mut self, now: f32, sprinting: bool) -> f32 {
        let dt = (now - self.last_time).clamp(0.0, MAX_DT);
        self.last_time = now;
        let target = if sprinting { 1.0 } else { 0.0 };
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

    #[test]
    fn factors_are_identity_when_not_sprinting() {
        assert_eq!(factors(false), Factors { speed: 1.0, accel: 1.0, bob: 1.0 });
        let f = factors(true);
        assert!(f.speed > 1.0 && f.accel > 1.0 && f.bob > 1.0);
    }

    #[test]
    fn blank_input_never_sprints() {
        let mut s = Sprint::new(0.0);
        assert!(!s.resolve(&Input::new(), false));
        // Not even with a toggle edge: there is no movement to toggle.
        assert!(!s.resolve(&Input::new(), true));
        assert!(!s.resolve(&Input::new(), false));
    }

    #[test]
    fn shift_is_a_hold() {
        let mut s = Sprint::new(0.0);
        let mut input = walking();
        input.key[KEY_SPRINT] = true;
        input.key_press[KEY_SPRINT] = true;
        assert!(s.resolve(&input, false));
        input.key_press[KEY_SPRINT] = false;
        assert!(s.resolve(&input, false));
        input.key[KEY_SPRINT] = false;
        assert!(!s.resolve(&input, false), "releasing Shift ends the run");
    }

    #[test]
    fn l3_toggles_while_moving() {
        let mut s = Sprint::new(0.0);
        let mut input = Input::new();
        input.pad_move_f = 0.8;
        assert!(!s.resolve(&input, false));
        assert!(s.resolve(&input, true), "press: on");
        assert!(s.resolve(&input, false), "stays on with the button released");
        assert!(!s.resolve(&input, true), "press again: off");
        assert!(!s.resolve(&input, false));
    }

    #[test]
    fn l3_held_counts_as_hold() {
        let mut s = Sprint::new(0.0);
        let mut input = Input::new();
        input.pad_sprint = true;
        assert!(s.resolve(&input, true));
        input.pad_sprint = false;
        assert!(!s.resolve(&input, false), "idle: the press toggled nothing, so release stops");
    }

    #[test]
    fn stopping_cancels_the_toggle() {
        let mut s = Sprint::new(0.0);
        let mut input = Input::new();
        input.pad_move_f = 1.0;
        assert!(s.resolve(&input, true));
        input.pad_move_f = 0.0;
        assert!(!s.resolve(&input, false), "stick centred: run over");
        input.pad_move_f = 1.0;
        assert!(!s.resolve(&input, false), "pushing again walks; the toggle does not come back");
    }

    #[test]
    fn keyboard_movement_keeps_a_pad_toggle_alive() {
        let mut s = Sprint::new(0.0);
        let mut input = walking();
        assert!(s.resolve(&input, true));
        input.pad_move_f = 0.0;
        assert!(s.resolve(&input, false), "W is still down, so the player is still moving");
    }

    #[test]
    fn shift_press_drops_a_pad_toggle() {
        let mut s = Sprint::new(0.0);
        let mut input = walking();
        assert!(s.resolve(&input, true));
        input.key[KEY_SPRINT] = true;
        input.key_press[KEY_SPRINT] = true;
        assert!(s.resolve(&input, false), "held Shift sprints");
        input.key[KEY_SPRINT] = false;
        input.key_press[KEY_SPRINT] = false;
        assert!(!s.resolve(&input, false), "and its release ends the run: hold mode now");
    }

    #[test]
    fn fov_eases_in_and_back_out() {
        let mut s = Sprint::new(0.0);
        assert!((s.ease_fov(0.0, false) - GH_FOV).abs() < 1e-6);
        // One time constant in, over three frames short enough not to hit the stall clamp:
        // about 63% of the kick.
        let mut t = 0.0;
        let mut f = 0.0;
        for _ in 0..3 {
            t += FOV_TAU / 3.0;
            f = s.ease_fov(t, true);
        }
        let frac = (f - GH_FOV) / SPRINT_FOV_KICK_DEG;
        assert!((frac - 0.632).abs() < 0.01, "got {frac}");
        // Keep sprinting for a second at 60 fps: converged.
        for _ in 0..60 {
            t += 1.0 / 60.0;
            f = s.ease_fov(t, true);
        }
        assert!((f - (GH_FOV + SPRINT_FOV_KICK_DEG)).abs() < 1e-2);
        // Stop: back to within a hundredth of a degree of the ported FOV after ten time
        // constants, monotonically.
        let mut prev = f;
        for _ in 0..90 {
            t += 1.0 / 60.0;
            let cur = s.ease_fov(t, false);
            assert!(cur <= prev + 1e-6);
            prev = cur;
        }
        assert!((prev - GH_FOV).abs() < 1e-2);
    }

    #[test]
    fn fov_ease_is_frame_rate_independent() {
        let mut slow = Sprint::new(0.0);
        let mut fast = Sprint::new(0.0);
        let a = slow.ease_fov(0.09, true);
        let mut b = 0.0;
        for i in 1..=9 {
            b = fast.ease_fov(i as f32 * 0.01, true);
        }
        // 90 ms in one (11 fps) frame vs nine (100 fps) frames: the exact exponential gives
        // the same answer, up to float rounding over nine multiplies.
        assert!((a - b).abs() < 1e-4, "{a} vs {b}");
    }

    #[test]
    fn a_long_stall_still_eases() {
        let mut s = Sprint::new(0.0);
        // A five-second pause then one frame of sprint integrates one clamped step, not five
        // seconds -- so the kick is well short of full.
        let f = s.ease_fov(5.0, true);
        let expect = 1.0 - (-MAX_DT / FOV_TAU).exp();
        assert!(((f - GH_FOV) / SPRINT_FOV_KICK_DEG - expect).abs() < 1e-5);
        // ...and a reset puts it straight back.
        s.reset(5.0);
        assert!((s.ease_fov(5.0, false) - GH_FOV).abs() < 1e-6);
    }
}
