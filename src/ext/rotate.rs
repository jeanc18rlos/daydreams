//! EXT: rotating the held object, Superliminal-style. Not part of the C++ port.
//!
//! While an object is held (src/ext/grab.rs) and the rotate modifier is down, the look input
//! stops driving the camera and drives the object instead: stick / mouse left-right yaws it,
//! up-down pitches it, and the camera freezes for the duration. Two axes only -- there is no
//! roll, matching the game this imitates.
//!
//! # Modifiers
//!
//! * Keyboard / mouse: right mouse button (`Input::mouse_button[2]`) or the R key
//!   (`Input::key[b'R']`, mapped in `input::key_index`).
//! * DualSense: R1 (`gilrs::Button::RightTrigger`), published as `Input::pad_rotate_mod` by
//!   src/ext/gamepad.rs every poll. R1 used to double as "next scene"; that binding moved to
//!   the D-pad so a held modifier never fires an edge action (see gamepad.rs module docs).
//!
//! # Why this runs before the fixed-step loop
//!
//! The ported engine consumes look input inside its 500 Hz loop: `Player::Update` reads
//! `mouse_dx/dy` plus `pad_look_x/y` (player.rs:88-93, Player.cpp:60-62) on every step, and
//! `Input::EndFrame` -- also inside the loop (Engine.cpp:112) -- folds the raw `mouse_ddx/ddy`
//! into the smoothed `mouse_dx/dy` accumulator (Input.cpp:13-16) before zeroing the raw deltas.
//! So to keep the camera still for a rendered frame, `begin_frame` must run before that loop
//! and zero **all four** mouse fields:
//!
//! * `mouse_ddx/ddy` -- this frame's raw motion, which would otherwise be mixed into `dx/dy`
//!   by the first `EndFrame` of the loop;
//! * `mouse_dx/dy` -- the smoothed value, which still carries `GH_MOUSE_SMOOTH` (= 0.5) of the
//!   previous frame's motion and would keep the camera drifting for a few steps after the
//!   modifier went down.
//!
//! `pad_look_x/y` are rewritten by `Gamepads::poll` each rendered frame and read directly by
//! the player, so zeroing them once here holds for the whole loop.
//!
//! When not rotating, `Input` is left untouched -- the camera path must see exactly the values
//! the platform layer and gamepad wrote.
//!
//! # Signs
//!
//! The engine applies the deltas as `euler.y += yaw; euler.x += pitch` (engine.rs), and
//! `Object::LocalToWorld` is `Trans * RotY(euler.y) * RotX(euler.x) * ...` (Object.cpp:9-13).
//! `MakeRotY(a)` maps +Z to (sin a, 0, cos a) (Vector.h:195-200), i.e. a positive yaw swings
//! the +Z face toward +X. With the camera looking down its local -Z, an object in front of it
//! presents its +Z face and +X is screen-right -- and because the rotation is about the world
//! up axis, "near face moves to the viewer's right" holds from any horizontal camera azimuth.
//! Mouse-right is `+ddx` and stick-right is `+pad_look_x` (gamepad.rs chooses that sign), so
//! **yaw = +ddx**: no negation.
//!
//! `MakeRotX(a)` maps +Z to (0, -sin a, cos a) (Vector.h:188-193): a positive pitch tips the
//! near face *down*. Mouse-up is `-ddy` (raw-input convention, Input.cpp:26) and stick-up is
//! `-pad_look_y` (gamepad.rs negates the stick's +up), so **pitch = +ddy** makes mouse/stick-up
//! tip the near face up. Note pitch is about the object's *yawed* X axis (RotX sits inside
//! RotY), which is the natural "tumble toward me" for a held object.
//!
//! # Units
//!
//! `mouse_ddx/ddy` are raw pixels this frame; `pad_look_x/y` are already radians per fixed
//! step (gamepad.rs `LOOK_RATE`), scaled by `PAD_ROT_SENS` and applied once per rendered frame.

use crate::input::Input;

/// Mouse rotation sensitivity in radians per pixel of raw motion.
/// A 500 px drag turns the object a little under 180 degrees.
pub const MOUSE_ROT_SENS: f32 = 0.006;
/// Pad rotation rate as a fraction of the camera look rate (`pad_look_*` are already scaled).
pub const PAD_ROT_SENS: f32 = 0.8;

#[derive(Default)]
pub struct Rotate {
    /// True for frames in which look input was diverted to the held object.
    pub active: bool,
}

impl Rotate {
    /// Whether any rotate modifier is currently held.
    pub fn modifier_down(input: &Input) -> bool {
        input.mouse_button[2] || input.key[b'R' as usize] || input.pad_rotate_mod
    }

    /// Call once at the top of every rendered frame, before the fixed-step loop.
    ///
    /// Returns `Some((yaw, pitch))` in radians to add to the held object's `euler.y` / `euler.x`
    /// when `holding` and a modifier is down -- in which case this frame's look input has been
    /// zeroed so the camera stays put. Returns `None` and leaves `input` untouched otherwise.
    pub fn begin_frame(&mut self, input: &mut Input, holding: bool) -> Option<(f32, f32)> {
        if !(holding && Self::modifier_down(input)) {
            self.active = false;
            return None;
        }

        let (yaw, pitch) = Self::deltas(input);

        // Take the look input away from the camera for this frame. Both the raw and the
        // smoothed mouse fields must go -- see the module docs.
        input.mouse_ddx = 0.0;
        input.mouse_ddy = 0.0;
        input.mouse_dx = 0.0;
        input.mouse_dy = 0.0;
        input.pad_look_x = 0.0;
        input.pad_look_y = 0.0;

        self.active = true;
        Some((yaw, pitch))
    }

    /// Pure mapping from this frame's look input to (yaw, pitch) object deltas.
    fn deltas(input: &Input) -> (f32, f32) {
        let yaw = input.mouse_ddx * MOUSE_ROT_SENS + input.pad_look_x * PAD_ROT_SENS;
        let pitch = input.mouse_ddy * MOUSE_ROT_SENS + input.pad_look_y * PAD_ROT_SENS;
        (yaw, pitch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input_with_look(ddx: f32, ddy: f32, px: f32, py: f32) -> Input {
        let mut i = Input::new();
        i.mouse_ddx = ddx;
        i.mouse_ddy = ddy;
        // Pretend a previous frame left something in the smoothed accumulator.
        i.mouse_dx = 3.0;
        i.mouse_dy = -2.0;
        i.pad_look_x = px;
        i.pad_look_y = py;
        i
    }

    fn assert_look_untouched(i: &Input, ddx: f32, ddy: f32, px: f32, py: f32) {
        assert_eq!(i.mouse_ddx, ddx);
        assert_eq!(i.mouse_ddy, ddy);
        assert_eq!(i.mouse_dx, 3.0);
        assert_eq!(i.mouse_dy, -2.0);
        assert_eq!(i.pad_look_x, px);
        assert_eq!(i.pad_look_y, py);
    }

    #[test]
    fn not_holding_leaves_input_alone() {
        let mut i = input_with_look(10.0, 5.0, 0.1, 0.2);
        i.mouse_button[2] = true;
        let mut r = Rotate::default();
        assert_eq!(r.begin_frame(&mut i, false), None);
        assert!(!r.active);
        assert_look_untouched(&i, 10.0, 5.0, 0.1, 0.2);
    }

    #[test]
    fn no_modifier_leaves_input_alone() {
        let mut i = input_with_look(10.0, 5.0, 0.1, 0.2);
        let mut r = Rotate::default();
        assert_eq!(r.begin_frame(&mut i, true), None);
        assert!(!r.active);
        assert_look_untouched(&i, 10.0, 5.0, 0.1, 0.2);
    }

    #[test]
    fn every_modifier_source_activates() {
        for setup in [
            (|i: &mut Input| i.mouse_button[2] = true) as fn(&mut Input),
            |i| i.key[b'R' as usize] = true,
            |i| i.pad_rotate_mod = true,
        ] {
            let mut i = input_with_look(0.0, 0.0, 0.0, 0.0);
            setup(&mut i);
            let mut r = Rotate::default();
            assert!(r.begin_frame(&mut i, true).is_some());
            assert!(r.active);
        }
    }

    #[test]
    fn rotating_zeroes_raw_smoothed_and_pad_look() {
        let mut i = input_with_look(10.0, 5.0, 0.1, 0.2);
        i.key[b'R' as usize] = true;
        let mut r = Rotate::default();
        assert!(r.begin_frame(&mut i, true).is_some());
        assert_eq!(i.mouse_ddx, 0.0);
        assert_eq!(i.mouse_ddy, 0.0);
        assert_eq!(i.mouse_dx, 0.0, "smoothed accumulator must be cleared too");
        assert_eq!(i.mouse_dy, 0.0);
        assert_eq!(i.pad_look_x, 0.0);
        assert_eq!(i.pad_look_y, 0.0);
        // Running EndFrame afterwards (as the fixed-step loop does) must keep the camera still.
        i.end_frame();
        assert_eq!(i.mouse_dx, 0.0);
        assert_eq!(i.mouse_dy, 0.0);
    }

    #[test]
    fn mouse_right_yaws_positive_and_mouse_up_pitches_negative() {
        // +yaw swings the +Z (near) face toward +X = screen right; -pitch tips it up.
        let mut i = input_with_look(100.0, -50.0, 0.0, 0.0);
        i.mouse_button[2] = true;
        let (yaw, pitch) = Rotate::default().begin_frame(&mut i, true).unwrap();
        assert!((yaw - 100.0 * MOUSE_ROT_SENS).abs() < 1e-6);
        assert!((pitch + 50.0 * MOUSE_ROT_SENS).abs() < 1e-6);
    }

    #[test]
    fn stick_right_and_up_match_mouse_signs() {
        // gamepad.rs: stick right -> +pad_look_x, stick up -> -pad_look_y.
        let mut i = input_with_look(0.0, 0.0, 0.05, -0.05);
        i.pad_rotate_mod = true;
        let (yaw, pitch) = Rotate::default().begin_frame(&mut i, true).unwrap();
        assert!(yaw > 0.0);
        assert!(pitch < 0.0);
        assert!((yaw - 0.05 * PAD_ROT_SENS).abs() < 1e-6);
    }

    #[test]
    fn mouse_and_pad_sum() {
        let mut i = input_with_look(10.0, 0.0, 0.05, 0.0);
        i.pad_rotate_mod = true;
        let (yaw, _) = Rotate::default().begin_frame(&mut i, true).unwrap();
        assert!((yaw - (10.0 * MOUSE_ROT_SENS + 0.05 * PAD_ROT_SENS)).abs() < 1e-6);
    }

    #[test]
    fn active_clears_when_modifier_released() {
        let mut i = input_with_look(1.0, 1.0, 0.0, 0.0);
        i.mouse_button[2] = true;
        let mut r = Rotate::default();
        r.begin_frame(&mut i, true);
        assert!(r.active);
        i.mouse_button[2] = false;
        assert_eq!(r.begin_frame(&mut i, true), None);
        assert!(!r.active);
    }
}
