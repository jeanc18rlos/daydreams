//! EXT: DualSense / gamepad support. Not part of the C++ port -- but arguably finishing it.
//!
//! CodeParade actually *registered* joystick and gamepad raw-input devices in
//! `Engine::SetupInputs` (Engine.cpp:454-465) and left three `//TODO:` stubs behind
//! (Input.cpp:43-45, Input.h:23-30). The handler was never written. This module fills that in.
//!
//! Built on `gilrs`, which reads the SDL game-controller database, so a DualSense connected over
//! USB or Bluetooth maps to the standard layout without any device-specific code.
//!
//! # Layout (DualSense names in brackets)
//!
//! | Control                  | Action                                        |
//! |--------------------------|-----------------------------------------------|
//! | Left stick               | Move (analog)                                  |
//! | Right stick              | Look (analog)                                  |
//! | `South` [Cross]          | Jump                                           |
//! | `West` [Square]          | Grab / release the held object                 |
//! | `RightTrigger2` [R2]     | Grab / release (alternate)                     |
//! | `RightTrigger` [R1]      | Hold: rotate the held object with the right stick |
//! | `LeftTrigger` [L1]       | (reserved as a modifier; currently unbound)    |
//! | `LeftThumb` [L3]         | Run: press to start; ends when the stick returns to centre or on the next press. Holding it runs too (src/ext/sprint.rs) |
//! | D-Pad left / right       | Previous / next scene                          |
//! | D-Pad up / down          | Move the menu selection                        |
//! | `East` [Circle]          | Menu: back                                     |
//! | `Start` [Options]        | Open the pause menu / close it again            |
//! | `Select` [Create]        | Toggle mute                                    |
//! | `Mode` [PS button]       | Toggle fullscreen                              |
//!
//! # Why Cross jumps and Square grabs
//!
//! Cross was the grab (and the elevator, and the key: they share one button). Jumping arrived and
//! took it, because Cross-to-jump is what a pad player's thumb already believes and because a
//! button cannot be both "jump" and "pick this up" -- every hop in front of a prop would have
//! grabbed it. Grab moved one seat left to Square, which was already an alternate for it, and R2
//! stayed. Cross keeps `menu_confirm` as well, since a menu has nothing to jump over; the frame a
//! menu closes calls `Player::ignore_jump_until_release` so the confirm cannot follow the player
//! back into the world as a launch.
//!
//! Jump is published as a *level* (`Input::pad_jump`) rather than raised as an edge here, like R1's
//! `pad_rotate_mod` and L3's `pad_sprint`: the fixed-step loop clears the edge slots partway
//! through a rendered frame, and `ext::jump` wants the button's state on every step.
//!
//! # Why Options opens the menu, and why PS no longer quits
//!
//! Both bindings moved. Options -> pause is the console convention, and without it a pad could
//! start a game and then never leave it: `menu_back` only reaches a menu that is *already*
//! open, and nothing else on the pad opened one. Quitting is reachable from that menu (PAUSE ->
//! MAIN MENU -> EXIT), so the pad no longer needs a quit button of its own -- which is just as
//! well, because the button it had was PS, the same button you press to wake a sleeping
//! DualSense. An unconfirmed instant exit on the wake button is a trap, so PS took over the
//! toggle Options gave up.
//!
//! # Why the bumpers are modifiers and nothing else
//!
//! L1/R1 once cycled scenes, as the D-pad did. Object rotation (src/ext/rotate.rs) wants R1 as
//! a *held* modifier, Superliminal-style, and a button cannot be both a held modifier and an
//! edge-triggered action without a "consumed" flag and an awkward ordering dependency between
//! this module and `Rotate::begin_frame`. Scene switching has since left the pad altogether --
//! SWITCH LEVEL in the pause menu is where a level is chosen -- so the bumpers are pure
//! modifiers: R1's level is published through `Input::pad_rotate_mod` every poll and nothing
//! here ever raises an event from it.
//!
//! # Analog vs the ported input model
//!
//! `Player::Update` reads digital keys (Player.cpp:47-58) and `Player::Move` normalises the
//! resulting vector only when its magnitude exceeds 1 (Player.cpp:92-96). That normalisation
//! rule means an analog stick can be *added* to the keyboard vector and simply works: a stick
//! at half deflection yields half speed, and a stick plus a key still clamps to full speed.
//! So the gamepad writes into two extra `Input` fields rather than faking key presses, and the
//! ported movement code needs only a two-line addition.

use crate::game_header::GH_DT;
use crate::input::Input;

/// Sticks report small non-zero values at rest; below this we treat them as centred.
const STICK_DEADZONE: f32 = 0.18;

/// Right-stick look speed at full deflection, in **radians per second**, at the middle
/// sensitivity notch.
///
/// Per second, not per step, and that is the whole of the calibration. `Input::pad_look_*` are
/// spent by `Player::update_player` on every 500 Hz fixed step ([`GH_DT`]) rather than once per
/// rendered frame, and `Input::end_frame` -- which decays the mouse's own delta on its way past
/// -- does not touch them, so a number that reads like a sane nudge for one frame is paid out
/// five hundred times a second. The 0.055 rad/step this replaced turned the camera at 27.5
/// rad/s: 1576 degrees a second, a full circle in under a quarter of a second, with the lowest
/// of the ten sensitivity notches still leaving 645 -- which is why the menu could not rescue
/// it. Stated per second there is nothing left to get wrong, and [`LOOK_RATE`] converts once.
///
/// 150 deg/s at the default notch. The settings ladder spans 0.41x to 3.05x
/// (`ext::settings::scale_of`), so the ten notches cover 61 to 458 deg/s: slower than a walk
/// at one end, faster than most console shooters at the other.
const LOOK_RATE_PER_SEC: f32 = 2.618;
/// [`LOOK_RATE_PER_SEC`] as radians per fixed step, which is what `Input::pad_look_*` carry.
const LOOK_RATE: f32 = LOOK_RATE_PER_SEC * GH_DT;

/// Exponent on the look stick's deflection (see [`look_stick`]).
///
/// A stick is a rate control with about 12 mm of throw, and a linear map spends most of that
/// throw in speeds nobody aims with: the first millimetre past the deadzone is already a tenth
/// of full speed. Squaring it makes half deflection a quarter of the rate, which puts the slow
/// end -- the end a player looks *with* -- across most of the travel, and leaves the top speed
/// where the notch says. It also takes the sting out of a drifting stick, whose deflection is
/// small by definition: the drift that made `--no-gamepad` necessary for reproducible runs now
/// arrives squared.
const LOOK_CURVE: f32 = 2.0;

/// Analog triggers read as an axis on some backends; past this they count as pressed.
const TRIGGER_THRESHOLD: f32 = 0.5;

/// Edge-triggered actions the gamepad can raise in a frame.
#[derive(Default, Clone, Copy, Debug)]
pub struct PadEvents {
    pub grab: bool,
    /// EXT: menu navigation edges -- D-pad up/down to move, left/right to change the value on
    /// a settings row, South to confirm, East to go back.
    pub menu_up: bool,
    pub menu_down: bool,
    pub menu_left: bool,
    pub menu_right: bool,
    pub menu_confirm: bool,
    pub menu_back: bool,
    /// EXT: open the pause menu, or close it again -- the pad's equivalent of Escape.
    pub pause: bool,
    pub toggle_fullscreen: bool,
    pub toggle_mute: bool,
    /// EXT: L3 went down this frame. Flips sprint's toggle mode (src/ext/sprint.rs); the
    /// button's *level* goes out separately as `Input::pad_sprint`, the same split as R1's
    /// `pad_rotate_mod`, because the stick click is both a tap-to-toggle and a hold.
    pub sprint: bool,
}

#[allow(dead_code)] // EXT: convenience API.
impl PadEvents {
    pub fn any(&self) -> bool {
        self.grab || self.pause || self.toggle_fullscreen || self.toggle_mute
    }
}

/// Digital buttons we track for edge detection.
#[derive(Default, Clone, Copy, PartialEq)]
struct ButtonState {
    grab: bool,
    menu_up: bool,
    menu_down: bool,
    menu_left: bool,
    menu_right: bool,
    menu_confirm: bool,
    menu_back: bool,
    pause: bool,
    fullscreen: bool,
    mute: bool,
    sprint: bool,
}

pub struct Gamepads {
    gilrs: Option<gilrs::Gilrs>,
    prev: ButtonState,
    connected: usize,
}

#[allow(dead_code)] // EXT: `connected` is used by the startup banner path.
impl Gamepads {
    /// Never fails hard: with no gamepad subsystem the engine simply runs keyboard-only.
    pub fn new() -> Gamepads {
        let gilrs = match gilrs::Gilrs::new() {
            Ok(g) => Some(g),
            Err(e) => {
                log::warn!("[gamepad] unavailable ({e}); keyboard and mouse only");
                None
            }
        };

        let mut pads = Gamepads { gilrs, prev: ButtonState::default(), connected: 0 };
        // A pad that is already connected when the process starts is NOT visible to
        // `gamepads()` yet: gilrs learns about it from a `Connected` event, and `gamepads()`
        // only yields pads it has already been told about. Fresh out of `Gilrs::new()` that
        // queue is full and undrained, so counting here without draining first reports zero
        // for a controller sitting right there -- which is exactly what a paired DualSense did.
        // `announce` drains and then counts. (`examples/pad_probe.rs` shows the raw sequence.)
        pads.announce();
        pads
    }

    /// Drain gilrs' queue, recount, and name any pad that has appeared since the last count.
    /// Called at startup and again whenever polling sees the topology change, so hot-plugging a
    /// controller mid-game says so too.
    fn announce(&mut self) {
        let before = self.connected;
        let Some(gilrs) = self.gilrs.as_mut() else { return };
        while gilrs.next_event().is_some() {}
        self.connected = gilrs.gamepads().count();
        if self.connected > before {
            for (_id, pad) in gilrs.gamepads() {
                log::info!("[gamepad] {} connected", pad.name());
            }
        } else if self.connected < before {
            log::info!("[gamepad] disconnected ({} left)", self.connected);
        }
    }

    pub fn connected(&self) -> usize {
        self.connected
    }

    /// Poll once per rendered frame. Drains gilrs' event queue (which keeps its internal state
    /// current), then samples the sticks and buttons and writes them into `input`.
    pub fn poll(&mut self, input: &mut Input) -> PadEvents {
        let mut events = PadEvents::default();

        let Some(gilrs) = self.gilrs.as_mut() else {
            return events;
        };

        // Draining the queue is what advances gilrs' cached gamepad state.
        let mut topology_changed = false;
        while let Some(gilrs::Event { event, .. }) = gilrs.next_event() {
            if matches!(event, gilrs::EventType::Connected | gilrs::EventType::Disconnected) {
                topology_changed = true;
            }
        }
        if topology_changed {
            self.announce();
        }
        // announce() re-borrowed self, so take the handle again for the sampling pass below.
        let Some(gilrs) = self.gilrs.as_mut() else {
            return events;
        };

        // Combine every connected pad, so it does not matter which one is "player one".
        let mut move_x = 0.0f32;
        let mut move_y = 0.0f32;
        let mut look_x = 0.0f32;
        let mut look_y = 0.0f32;
        let mut cur = ButtonState::default();
        // EXT: R1 level, for src/ext/rotate.rs.
        let mut rotate_mod = false;
        // EXT: Cross level, for src/ext/jump.rs. A local rather than a `ButtonState` field for
        // the same reason as R1's: nothing here edge-detects it.
        let mut jump = false;

        for (_id, pad) in gilrs.gamepads() {
            let lx = deadzone(pad.value(gilrs::Axis::LeftStickX));
            let ly = deadzone(pad.value(gilrs::Axis::LeftStickY));
            // The look stick gets its deadzone and curve as a vector, not per axis -- see
            // `look_stick`.
            let (rx, ry) = look_stick(
                pad.value(gilrs::Axis::RightStickX),
                pad.value(gilrs::Axis::RightStickY),
            );

            move_x += lx;
            move_y += ly;
            look_x += rx;
            look_y += ry;

            let pressed = |b: gilrs::Button| pad.is_pressed(b);
            let analog = |b: gilrs::Button| {
                pad.button_data(b).map_or(0.0, |d| d.value()) > TRIGGER_THRESHOLD
            };

            // EXT: Cross is the jump now; grab keeps Square and R2 -- see the module docs.
            cur.grab |= pressed(gilrs::Button::West)
                || pressed(gilrs::Button::RightTrigger2)
                || analog(gilrs::Button::RightTrigger2);
            jump |= pressed(gilrs::Button::South);
            rotate_mod |= pressed(gilrs::Button::RightTrigger);
            cur.menu_up |= pressed(gilrs::Button::DPadUp);
            cur.menu_down |= pressed(gilrs::Button::DPadDown);
            // The same two buttons as prev/next scene. They cannot collide: main.rs acts on
            // scene cycling only while no menu is open, and the menu only reads these while
            // one is.
            cur.menu_left |= pressed(gilrs::Button::DPadLeft);
            cur.menu_right |= pressed(gilrs::Button::DPadRight);
            cur.menu_confirm |= pressed(gilrs::Button::South);
            cur.menu_back |= pressed(gilrs::Button::East);
            cur.pause |= pressed(gilrs::Button::Start);
            cur.mute |= pressed(gilrs::Button::Select);
            cur.fullscreen |= pressed(gilrs::Button::Mode);
            cur.sprint |= pressed(gilrs::Button::LeftThumb);
        }

        // ── Analog axes into the ported input model ────────────────────────────────────────
        // Left stick: +Y is forward on gilrs, matching the sign `Player::Move` wants for moveF.
        // Left stick X is +right, but moveL is +left (Player.cpp:53-58), hence the negation.
        input.pad_move_f = clamp_unit(move_y);
        input.pad_move_l = clamp_unit(-move_x);

        // Right stick into look deltas. `Player::Look` negates both (Player.cpp:74,82), so the
        // signs here are chosen to give the conventional "stick right turns right, stick up
        // looks up" behaviour.
        //
        // EXT: scaled by the pad's own sensitivity notch, separately from the mouse's. They are
        // different instruments -- a stick is a rate control and a mouse is a displacement one --
        // and a player who uses both wants two numbers, not one compromise.
        let rate = LOOK_RATE * crate::ext::settings::pad_scale();
        input.pad_look_x = clamp_unit(look_x) * rate;
        input.pad_look_y = clamp_unit(-look_y) * rate;

        // Rotate modifier: a level, published raw. It is deliberately NOT edge-detected and
        // raises no event; `Rotate::begin_frame` samples it alongside the keyboard modifiers.
        input.pad_rotate_mod = rotate_mod;
        // EXT: sprint level, published raw for the same reason; the edge is raised below.
        input.pad_sprint = cur.sprint;
        // EXT: jump level, ditto. Written whether or not a menu is open, which is safe because
        // `Engine::run_frame` returns before the fixed-step loop on a menu frame, so nothing ever
        // reads it there.
        input.pad_jump = jump;

        // ── Rising-edge detection for the digital actions ──────────────────────────────────
        events.grab = cur.grab && !self.prev.grab;
        events.menu_up = cur.menu_up && !self.prev.menu_up;
        events.menu_down = cur.menu_down && !self.prev.menu_down;
        events.menu_left = cur.menu_left && !self.prev.menu_left;
        events.menu_right = cur.menu_right && !self.prev.menu_right;
        events.menu_confirm = cur.menu_confirm && !self.prev.menu_confirm;
        events.menu_back = cur.menu_back && !self.prev.menu_back;
        events.pause = cur.pause && !self.prev.pause;
        events.toggle_fullscreen = cur.fullscreen && !self.prev.fullscreen;
        events.toggle_mute = cur.mute && !self.prev.mute;
        events.sprint = cur.sprint && !self.prev.sprint;
        self.prev = cur;

        events
    }
}

/// The look stick's deflection: one **radial** deadzone and one response curve, applied to the
/// pair rather than to each axis.
///
/// Radial because look is a direction where movement is two independent amounts. Gate the axes
/// separately, as [`deadzone`] does for the left stick, and the dead region is a square: a
/// stick pushed exactly diagonally reads 0.17 on both axes -- a real deflection of 0.24 -- and
/// does nothing at all, while the same push a few degrees off the diagonal moves one axis and
/// not the other, so a slow diagonal pan arrives as a stair. One circle around the centre fixes
/// both, and it is also the shape the hardware's dead region actually has.
///
/// Returns the deflection in -1..1 per axis, of magnitude at most 1: rescaled so the curve
/// starts at zero at the deadzone's edge rather than jumping, raised to [`LOOK_CURVE`], and
/// pointed back along the direction the stick was pushed.
fn look_stick(x: f32, y: f32) -> (f32, f32) {
    let mag = x.hypot(y);
    if mag < STICK_DEADZONE {
        return (0.0, 0.0);
    }
    // A square-gated stick can report (1, 1), a magnitude of 1.41; the clamp keeps the corners
    // of the gate from reading as more than full deflection.
    let live = ((mag - STICK_DEADZONE) / (1.0 - STICK_DEADZONE)).min(1.0);
    let scale = live.powf(LOOK_CURVE) / mag;
    (x * scale, y * scale)
}

/// The movement stick's deadzone, per axis: `Player::Move` takes forward and left as two
/// independent amounts and clamps their combined magnitude itself (Player.cpp:92-96).
fn deadzone(v: f32) -> f32 {
    if v.abs() < STICK_DEADZONE {
        0.0
    } else {
        // Rescale so motion starts at zero right at the deadzone edge instead of jumping.
        let sign = v.signum();
        sign * ((v.abs() - STICK_DEADZONE) / (1.0 - STICK_DEADZONE))
    }
}

fn clamp_unit(v: f32) -> f32 {
    v.clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ext::settings;

    #[test]
    fn deadzone_suppresses_drift_and_rescales() {
        assert_eq!(deadzone(0.0), 0.0);
        assert_eq!(deadzone(0.1), 0.0, "inside the deadzone should read as centred");
        // Just past the edge should be near zero, not a jump to 0.19.
        assert!(deadzone(0.19).abs() < 0.02);
        // Full deflection must still reach full range.
        assert!((deadzone(1.0) - 1.0).abs() < 1e-6);
        assert!((deadzone(-1.0) + 1.0).abs() < 1e-6);
    }

    #[test]
    fn deadzone_is_symmetric() {
        for v in [0.3f32, 0.5, 0.75, 1.0] {
            assert!((deadzone(v) + deadzone(-v)).abs() < 1e-6);
        }
    }

    /// The regression this file exists to hold: `pad_look_*` are spent once per fixed step, so
    /// the turn rate is `LOOK_RATE / GH_DT`, and the constant has to be written knowing that.
    /// It was not, and full deflection turned the camera 1576 degrees a second.
    #[test]
    fn full_deflection_is_a_turn_speed_a_thumb_can_aim_with() {
        let deg_per_sec = |scale: f32| (LOOK_RATE * scale / GH_DT).to_degrees();
        let default = deg_per_sec(settings::scale_of(settings::DEFAULT_LEVEL));
        assert!((100.0..=200.0).contains(&default), "{default} deg/s at the default notch");
        // And the ladder's ends stay useful rather than unusable: the slowest notch still
        // turns you round, the fastest is fast without being the 645 deg/s the slowest used
        // to be.
        let slowest = deg_per_sec(settings::scale_of(1));
        let fastest = deg_per_sec(settings::scale_of(settings::LEVELS));
        assert!((40.0..=90.0).contains(&slowest), "{slowest} deg/s at notch 1");
        assert!(
            (300.0..=600.0).contains(&fastest),
            "{fastest} deg/s at notch {}",
            settings::LEVELS
        );
    }

    /// The curve is what makes the slow end aimable: half deflection is a quarter rate, not a
    /// half, and full deflection still reaches the notch's top speed.
    #[test]
    fn look_stick_is_curved_but_reaches_full_deflection() {
        let mag = |x: f32, y: f32| {
            let (a, b) = look_stick(x, y);
            a.hypot(b)
        };
        assert_eq!(mag(0.0, 0.0), 0.0);
        assert_eq!(mag(0.1, 0.05), 0.0, "inside the deadzone should read as centred");
        assert!(mag(0.19, 0.0) < 0.02, "just past the edge is near zero, not a jump");
        assert!((mag(1.0, 0.0) - 1.0).abs() < 1e-6, "full deflection must reach full rate");
        // Halfway along the live travel gives a quarter of the rate (LOOK_CURVE = 2).
        let half = STICK_DEADZONE + 0.5 * (1.0 - STICK_DEADZONE);
        assert!((mag(half, 0.0) - 0.25).abs() < 1e-5, "half travel gave {}", mag(half, 0.0));
        // Monotonic all the way out, so nothing ever slows down as the stick is pushed further.
        let mut prev = 0.0;
        for i in 0..=40 {
            let m = mag(i as f32 / 40.0, 0.0);
            assert!(m >= prev - 1e-6, "rate fell from {prev} to {m}");
            prev = m;
        }
    }

    /// Radial, not per-axis: a stick pushed exactly diagonally is a real deflection and must
    /// move the camera, and it must move it at the same rate as the same push along an axis.
    #[test]
    fn look_stick_deadzone_is_a_circle_not_a_square() {
        let d = STICK_DEADZONE;
        // Both axes inside the per-axis deadzone, but the vector is well outside it.
        let (x, y) = look_stick(d * 0.9, d * 0.9);
        assert!(x.hypot(y) > 0.0, "a diagonal push inside the square deadzone did nothing");
        // Same magnitude, any direction, same rate.
        let along = look_stick(0.7, 0.0);
        let diag = look_stick(0.7 / 2f32.sqrt(), 0.7 / 2f32.sqrt());
        let (m1, m2) = (along.0.hypot(along.1), diag.0.hypot(diag.1));
        assert!((m1 - m2).abs() < 1e-5, "{m1} along the axis, {m2} on the diagonal");
        // The corner of a square gate is not more than full deflection.
        let corner = look_stick(1.0, 1.0);
        assert!(corner.0.hypot(corner.1) <= 1.0 + 1e-6);
    }

    #[test]
    fn look_stick_is_symmetric() {
        for (x, y) in [(0.3f32, 0.0f32), (0.5, 0.5), (0.0, 0.75), (-0.4, 0.9)] {
            let (a, b) = look_stick(x, y);
            let (c, d) = look_stick(-x, -y);
            assert!((a + c).abs() < 1e-6 && (b + d).abs() < 1e-6, "({x}, {y}) is not symmetric");
        }
    }

    #[test]
    fn combined_axes_stay_in_range() {
        // Two pads pushed the same way must not exceed full deflection.
        assert_eq!(clamp_unit(2.0), 1.0);
        assert_eq!(clamp_unit(-2.0), -1.0);
    }
}
