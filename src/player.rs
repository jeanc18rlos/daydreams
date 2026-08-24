//! Port of Player.h / Player.cpp.

use crate::game_header::{
    GH_BOB_DAMP, GH_BOB_FREQ, GH_BOB_MIN, GH_BOB_OFFS, GH_DT, GH_MOUSE_SENSITIVITY, GH_PI,
    GH_PLAYER_HEIGHT, GH_PLAYER_RADIUS, GH_WALK_ACCEL, GH_WALK_SPEED,
};
use crate::object::{Object, ObjectT, UpdateCtx};
use crate::physical::Physical;
use crate::sphere::Sphere;
use crate::vector::{Matrix4, Vector3};
// EXT: the jump's take-once events are read through a shared borrow; see the fields.
use std::cell::Cell;

// PORT: `class Player : public Physical` -> composition, same as Physical/Object
// (was: class Player : public Physical, Player.h:5).
pub struct Player {
    pub base: Physical,

    cam_rx: f32,
    cam_ry: f32,

    bob_mag: f32,
    bob_phi: f32,

    // PORT: snake_case (was: bool onGround, Player.h:28).
    on_ground: bool,

    // EXT: footfalls taken, counted off the head-bob phase (two per bob cycle). Read by
    // `Engine::ext_update` to fire footstep sounds. Only ever grows; it is not reset with
    // the rest of the player because the consumer compares by inequality (see
    // `ExtState::fire_footstep_sfx`) and a reset could alias a real step.
    steps: u32,

    // EXT: everything between the jump button and the one line that adds the impulse: coyote
    // time, the input buffer, and the lockout that keeps one press from launching twice. See
    // src/ext/jump.rs.
    jump: crate::ext::jump::Jump,
    // EXT: this frame's jump events, for whoever wants to sound them. Taken once rather than
    // counted like `steps`, because `steps` has to be a counter -- two footfalls fit inside one
    // rendered frame at 500 Hz -- and these cannot: the shortest flight is 0.7 s. `Cell`, so
    // `Engine::ext_update` can take them through the shared borrow it already holds, the same
    // shape as `hint::take`.
    jumped: Cell<bool>,
    landed: Cell<Option<f32>>,
}

// Preserved verbatim from the C++ API surface; not every member is reachable from the
// scenes this port ships. Kept for fidelity rather than deleted.
#[allow(dead_code)]
impl Player {
    pub fn new() -> Player {
        // PORT: fields are initialized before the Reset() call that the C++ ctor relies on
        // (was: Player::Player() { Reset(); ... }, Player.cpp:7-11).
        let mut p = Player {
            base: Physical::new(),
            cam_rx: 0.0,
            cam_ry: 0.0,
            bob_mag: 0.0,
            bob_phi: 0.0,
            on_ground: true,
            // EXT:
            steps: 0,
            jump: crate::ext::jump::Jump::new(),
            jumped: Cell::new(false),
            landed: Cell::new(None),
        };
        p.reset();
        p.base.hit_spheres.push(Sphere::new_at(Vector3::new(0.0, 0.0, 0.0), GH_PLAYER_RADIUS));
        p.base.hit_spheres.push(Sphere::new_at(
            Vector3::new(0.0, GH_PLAYER_RADIUS - GH_PLAYER_HEIGHT, 0.0),
            GH_PLAYER_RADIUS,
        ));
        p
    }

    pub fn reset(&mut self) {
        self.base.reset();
        self.cam_rx = 0.0;
        self.cam_ry = 0.0;
        self.bob_mag = 0.0;
        self.bob_phi = 0.0;
        self.base.friction = 0.04;
        self.base.drag = 0.002;
        self.on_ground = true;
        // EXT: a buffered press or a half-spent coyote window must not survive the scene load
        // that reset the player, or the new level opens with a jump nobody asked for. `steps`
        // deliberately does not reset (see the field); this does, because its consumer reads a
        // state and not a difference.
        self.jump = crate::ext::jump::Jump::new();
        self.jumped.set(false);
        self.landed.set(None);
    }

    // PORT: named `update_player` so it does not collide with `Physical::update`, which is
    // reachable through the `base` field (was: void Player::Update(), Player.cpp:24).
    // GH_INPUT (the global) arrives as `ctx.input`.
    pub fn update_player(&mut self, ctx: &UpdateCtx) {
        // EXT: sprint multipliers for this step, all exactly 1.0 when not sprinting so every
        // ported expression below evaluates bit-identically to the original.
        let sprint = ctx.input.sprint;

        //Update bobbing motion
        let mut mag_t =
            (self.base.prev_pos - self.base.base.pos).mag() / (GH_DT * self.base.base.p_scale);
        if !self.on_ground {
            mag_t = 0.0;
        }
        self.bob_mag = self.bob_mag * (1.0 - GH_BOB_DAMP) + mag_t * GH_BOB_DAMP;
        if self.bob_mag < GH_BOB_MIN {
            self.bob_phi = 0.0;
        } else {
            // EXT: `sprint.bob` scales the cadence (Player.cpp:34 has no such factor).
            let prev_phi = self.bob_phi;
            self.bob_phi += GH_BOB_FREQ * sprint.bob * GH_DT;
            if self.bob_phi > 2.0 * GH_PI {
                self.bob_phi -= 2.0 * GH_PI;
            }
            // EXT: `cam_offset` puts the head lowest at phi = 0, pi and 2pi, so a footfall is
            // the phase crossing pi or wrapping past 2pi (which is the only way it decreases).
            if self.bob_phi < prev_phi || (prev_phi < GH_PI && self.bob_phi >= GH_PI) {
                self.steps += 1;
            }
        }

        //Jumping
        // EXT: the `#if 0` block CodeParade left further down this function (Player.cpp:61-66),
        // turned back on and grown the three things it needed to work: coyote time, an input
        // buffer, and a lockout that stops the floor the player has not left yet from firing a
        // second impulse. All of that is `ext::jump`; what is left here is the block's own shape.
        //
        // It has moved ABOVE the physics step, which is the one change to the original that is
        // not an addition, and the reason it could never have worked where it was. Applied after
        // `Physical::update` the impulse misses this step's position integration, so the feet are
        // still a few hundredths of a millimetre inside the carpet when the collision pass runs
        // -- and that pass projects the velocity onto the floor's push and cancels it outright.
        // Not always: a push under `Physical::on_collide`'s 1e-8 threshold leaves the velocity
        // alone, and a resting player crosses that threshold every few steps. So the jump worked
        // or silently did not, depending on where in that cycle the button landed. Applied here
        // it is the velocity the position integration uses, the player rises 8 mm on the spot,
        // and there is nothing left for the floor to push out of.
        //
        // PORT: VK_SPACE == 0x20 == b' ' in Input's ASCII key slots. Read as a level and not a
        // `key_press` edge -- `Input::end_frame` runs inside the engine's fixed-step loop
        // (Engine.cpp:112), so an edge is clear for every step of a frame but the first.
        let jump_held = ctx.input.key[b' ' as usize] || ctx.input.pad_jump;
        if self.jump.step(self.on_ground, jump_held) {
            // The stub's flat 2.0 becomes the impulse the target apex asks for, scaled by
            // `p_scale` exactly as the stub scaled its own.
            self.base.velocity.y += crate::ext::jump::impulse(self.base.base.p_scale);
            self.jumped.set(true);
        }

        //Physics
        self.base.update();

        //Looking
        // EXT: right-stick look is summed with the mouse delta. `pad_look_*` is already in
        // radians per fixed step, so it is divided back out by GH_MOUSE_SENSITIVITY here to
        // survive `Look`'s multiply (Player.cpp:74,82) without changing the ported formula.
        //
        // EXT: the mouse delta carries the mouse's sensitivity notch. Applied HERE and not
        // inside `Look`, because the stick's contribution arrives through the same two
        // arguments and already carries its own notch from `Gamepads::poll` -- scaling in
        // `Look` would apply the mouse's setting to the stick as well.
        let mouse = crate::ext::settings::mouse_scale();
        self.look(
            ctx.input.mouse_dx * mouse
                + ctx.input.pad_look_x / crate::game_header::GH_MOUSE_SENSITIVITY,
            ctx.input.mouse_dy * mouse
                + ctx.input.pad_look_y / crate::game_header::GH_MOUSE_SENSITIVITY,
        );

        //Movement
        let mut move_f = 0.0f32;
        let mut move_l = 0.0f32;
        if ctx.input.key[b'W' as usize] {
            move_f += 1.0;
        }
        if ctx.input.key[b'S' as usize] {
            move_f -= 1.0;
        }
        if ctx.input.key[b'A' as usize] {
            move_l += 1.0;
        }
        if ctx.input.key[b'D' as usize] {
            move_l -= 1.0;
        }
        // EXT: analog stick adds to the keyboard vector. `Move` clamps the combined magnitude
        // to 1 (Player.cpp:92-96), so partial stick deflection yields partial speed and
        // stick+key still tops out at walk speed.
        move_f += ctx.input.pad_move_f;
        move_l += ctx.input.pad_move_l;
        self.move_player(move_f, move_l, &sprint);

        //Reset ground state after update finishes
        self.on_ground = false;
    }

    pub fn look(&mut self, mouse_dx: f32, mouse_dy: f32) {
        //Adjust x-axis rotation
        self.cam_rx -= mouse_dy * GH_MOUSE_SENSITIVITY;
        // PORT: the two-branch limit as one clamp; identical for every value, NaN included
        // (was: if (cam_rx > GH_PI / 2) { cam_rx = GH_PI / 2; } else if (cam_rx < -GH_PI / 2)
        // { cam_rx = -GH_PI / 2; }, Player.cpp:87).
        self.cam_rx = self.cam_rx.clamp(-GH_PI / 2.0, GH_PI / 2.0);

        //Adjust y-axis rotation
        self.cam_ry -= mouse_dx * GH_MOUSE_SENSITIVITY;
        if self.cam_ry > GH_PI {
            self.cam_ry -= GH_PI * 2.0;
        } else if self.cam_ry < -GH_PI {
            self.cam_ry += GH_PI * 2.0;
        }
    }

    // PORT: named `move_player` because `move` is a Rust keyword
    // (was: void Player::Move(float moveF, float moveL), Player.cpp:90).
    // EXT: takes the sprint multipliers as a third argument rather than reading them from a
    // field, so the function stays as pure as the original -- its output depends only on its
    // inputs and the velocity it integrates.
    pub fn move_player(
        &mut self,
        mut move_f: f32,
        mut move_l: f32,
        sprint: &crate::ext::sprint::Factors,
    ) {
        //Make sure movement is not too fast
        let mag = (move_f * move_f + move_l * move_l).sqrt();
        if mag > 1.0 {
            move_f /= mag;
            move_l /= mag;
        }

        //Movement
        let cam_to_world = self.base.local_to_world() * Matrix4::rot_y(self.cam_ry);
        // EXT: `sprint.accel` / `sprint.speed` multiply the ported constants (Player.cpp:100,107).
        self.base.velocity += cam_to_world.mul_direction(Vector3::new(-move_l, 0.0, -move_f))
            * (GH_WALK_ACCEL * sprint.accel * GH_DT);

        //Don't allow non-falling speeds above the player's max speed
        let temp_y = self.base.velocity.y;
        self.base.velocity.y = 0.0;
        self.base.velocity.clip_mag(self.base.base.p_scale * GH_WALK_SPEED * sprint.speed);
        self.base.velocity.y = temp_y;
    }

    // PORT: dropped the unused `Object& other` first argument
    // (was: void Player::OnCollide(Object& other, const Vector3& push), Player.cpp:109).
    pub fn on_collide(&mut self, push: Vector3) {
        //Prevent player from rolling down hills if they're not too steep
        let mut new_push = push;
        if push.normalized().y > 0.7 {
            new_push.x = 0.0;
            new_push.z = 0.0;
            // EXT: touchdown, sampled here because this is the last place the fall speed still
            // exists -- the base call below cancels the velocity into the push. Gated on the time
            // the jump state says the player has been airborne, not on `on_ground`, which
            // `update_player` has already cleared by the time the collision pass runs: a player
            // stood still is "not on the ground" here every single step, and would land five
            // hundred times a second. The minimum air time is what keeps the seams in the scanned
            // floor from reading as landings too, and the upward test drops the floor a jump has
            // not left yet.
            if self.jump.air_time() >= crate::ext::jump::LANDING_MIN_AIR
                && self.base.velocity.y < 0.0
            {
                self.landed.set(Some(-self.base.velocity.y));
            }
            self.on_ground = true;
        }

        //Friction should only apply when player is on ground
        let cur_friction = self.base.friction;
        if !self.on_ground {
            self.base.friction = 0.0;
        }

        //Base call
        self.base.on_collide(new_push);
        self.base.friction = cur_friction;
    }

    pub fn world_to_cam(&self) -> Matrix4 {
        Matrix4::rot_x(-self.cam_rx)
            * Matrix4::rot_y(-self.cam_ry)
            * Matrix4::trans(-self.cam_offset())
            * self.base.world_to_local()
    }

    pub fn cam_to_world(&self) -> Matrix4 {
        self.base.local_to_world()
            * Matrix4::trans(self.cam_offset())
            * Matrix4::rot_y(self.cam_ry)
            * Matrix4::rot_x(self.cam_rx)
    }

    pub fn cam_offset(&self) -> Vector3 {
        //If bob is too small, don't even bother
        if self.bob_mag < GH_BOB_MIN {
            return Vector3::zero();
        }

        //Convert bob to translation
        let theta = (GH_PI / 2.0) * self.bob_phi.sin();
        let y = self.bob_mag * GH_BOB_OFFS * (1.0 - theta.cos());
        Vector3::new(0.0, y, 0.0)
    }

    // PORT: added -- reaching the Object base through two levels of composition
    // (player.base.base) is noisy, so these name it (the C++ just inherits it).
    pub fn obj(&self) -> &Object {
        &self.base.base
    }
    pub fn obj_mut(&mut self) -> &mut Object {
        &mut self.base.base
    }
}

impl ObjectT for Player {
    fn base(&self) -> &Object {
        &self.base.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base.base
    }

    fn update(&mut self, ctx: &UpdateCtx) {
        self.update_player(ctx)
    }

    // Player::OnCollide overrides Physical::OnCollide (Player.h:12).
    fn on_collide(&mut self, push: Vector3) {
        Player::on_collide(self, push)
    }

    fn as_physical(&self) -> Option<&Physical> {
        Some(&self.base)
    }
    fn as_physical_mut(&mut self) -> Option<&mut Physical> {
        Some(&mut self.base)
    }

    fn reset_obj(&mut self) {
        Player::reset(self)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// EXT: additions beyond the C++ port.
// ─────────────────────────────────────────────────────────────────────────────
impl Player {
    /// EXT: aim the camera directly (radians). Used by the `--shot` dev tooling so a
    /// screenshot can be framed without mouse input.
    pub fn set_look(&mut self, yaw: f32, pitch: f32) {
        self.cam_ry = yaw;
        self.cam_rx = pitch;
    }

    /// EXT: footfalls taken so far; see the `steps` field.
    pub fn steps(&self) -> u32 {
        self.steps
    }

    /// EXT: whether a jump fired since this was last asked -- true once, for the rendered frame
    /// the impulse landed in, and false again to the next reader. See the `jumped` field for why
    /// this is taken and `steps` is counted.
    pub fn just_jumped(&self) -> bool {
        self.jumped.replace(false)
    }

    /// EXT: the downward speed at the last touchdown, in units per second, taken the same way.
    /// `None` on every frame the player did not land. A step off a kerb reports a few tenths and
    /// a full jump about 3.1, which is enough to tell a scuff from a thump.
    pub fn just_landed(&self) -> Option<f32> {
        self.landed.replace(None)
    }

    /// EXT: make the next jump wait for the button to be released first. The engine calls this
    /// on every menu action that hands control back to the game: Space and Cross confirm a menu
    /// row and are also the jump, so CONTINUE would otherwise launch the player on the frame the
    /// menu closed under them.
    pub fn ignore_jump_until_release(&mut self) {
        self.jump.ignore_until_release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ext::sprint::{factors, SPRINT_SPEED};
    use crate::input::Input;

    /// Drive `move_player` forward until the cap bites and return the horizontal speed.
    fn top_speed(sprinting: bool) -> f32 {
        let mut p = Player::new();
        let f = factors(sprinting);
        for _ in 0..2000 {
            p.move_player(1.0, 0.0, &f);
        }
        let v = p.base.velocity;
        (v.x * v.x + v.z * v.z).sqrt()
    }

    #[test]
    fn walk_cap_is_the_ported_constant() {
        let p = Player::new();
        let cap = p.obj().p_scale * GH_WALK_SPEED;
        assert!((top_speed(false) - cap).abs() < 1e-5);
    }

    #[test]
    fn sprint_cap_is_1_8x() {
        let p = Player::new();
        let cap = p.obj().p_scale * GH_WALK_SPEED * SPRINT_SPEED;
        assert!((top_speed(true) - cap).abs() < 1e-5);
        assert!((SPRINT_SPEED - 1.8).abs() < 1e-6);
    }

    #[test]
    fn identity_factors_leave_one_step_bit_identical() {
        // The same input through the ported formula with and without the `* 1.0` factors.
        let mut a = Player::new();
        let mut b = Player::new();
        a.move_player(0.3, -0.7, &factors(false));
        let cam_to_world = b.base.local_to_world() * Matrix4::rot_y(0.0);
        b.base.velocity +=
            cam_to_world.mul_direction(Vector3::new(0.7, 0.0, -0.3)) * (GH_WALK_ACCEL * GH_DT);
        assert_eq!(a.base.velocity.x.to_bits(), b.base.velocity.x.to_bits());
        assert_eq!(a.base.velocity.z.to_bits(), b.base.velocity.z.to_bits());
    }

    /// Walk the player in a straight line on flat ground for `secs` at the fixed step, with
    /// `on_ground` held so the bob runs, and return the footfalls counted.
    fn steps_after(sprinting: bool, secs: f32) -> u32 {
        let mut p = Player::new();
        let mut input = Input::new();
        input.key[b'W' as usize] = true;
        input.sprint = factors(sprinting);
        let n = (secs / GH_DT) as usize;
        for _ in 0..n {
            // `update_player` clears on_ground each step; a real walk re-sets it from the
            // floor collision, which there is none of here.
            p.on_ground = true;
            let ctx = UpdateCtx {
                input: &input,
                cam_to_world: p.cam_to_world(),
                player_pos: p.obj().pos,
                scene: &[],
            };
            p.update_player(&ctx);
            // Undo the gravity the physics step added, so the walk stays level.
            p.base.velocity.y = 0.0;
        }
        p.steps()
    }

    #[test]
    fn footfalls_come_twice_per_bob_cycle_and_faster_when_sprinting() {
        // Two footfalls per 2pi of phase at GH_BOB_FREQ rad/s, after the bob has spun up.
        let secs = 4.0;
        let walk = steps_after(false, secs);
        let expect = (secs * GH_BOB_FREQ / GH_PI) as u32;
        assert!(walk.abs_diff(expect) <= 1, "walk {walk} vs {expect}");
        let run = steps_after(true, secs);
        let expect_run = (secs * GH_BOB_FREQ * crate::ext::sprint::SPRINT_BOB_FREQ / GH_PI) as u32;
        assert!(run.abs_diff(expect_run) <= 1, "run {run} vs {expect_run}");
        assert!(run > walk);
    }

    #[test]
    fn on_collide_grounds_the_player_only_for_pushes_steeper_than_0_7() {
        // A floor push: grounded, and the lateral part of the push is dropped so the player
        // does not slide down a gentle slope (Player.cpp:110-115).
        let mut p = Player::new();
        p.on_ground = false;
        p.base.velocity = Vector3::new(1.0, -3.0, 0.0);
        p.on_collide(Vector3::new(0.1, 0.5, 0.0));
        assert!(p.on_ground);
        assert!((p.obj().pos.x - 0.0).abs() < 1e-6 && (p.obj().pos.y - 0.5).abs() < 1e-6);
        assert!(p.base.velocity.y.abs() < 1e-6, "{:?}", p.base.velocity);
        // A wall push (normalised y of 0.196) leaves on_ground alone and moves the player by
        // the whole push.
        let mut p = Player::new();
        p.on_ground = false;
        p.on_collide(Vector3::new(0.5, 0.1, 0.0));
        assert!(!p.on_ground);
        assert!((p.obj().pos.x - 0.5).abs() < 1e-6 && (p.obj().pos.y - 0.1).abs() < 1e-6);
        // The threshold itself: y/|push| just over 0.7 grounds, just under does not.
        for (y, grounded) in [(0.71f32, true), (0.69, false)] {
            let mut p = Player::new();
            p.on_ground = false;
            let lateral = (1.0 - y * y).sqrt();
            p.on_collide(Vector3::new(lateral, y, 0.0) * 0.01);
            assert_eq!(p.on_ground, grounded, "y = {y}");
        }
        // Friction only bites on the ground: the same wall push in the air keeps the
        // tangential velocity, on the ground it is scaled by (1 - friction).
        let mut air = Player::new();
        air.on_ground = false;
        air.base.velocity = Vector3::new(0.0, 0.0, -2.0);
        air.on_collide(Vector3::new(0.1, 0.0, 0.0));
        assert!((air.base.velocity.z + 2.0).abs() < 1e-6, "{:?}", air.base.velocity);
        let mut ground = Player::new();
        ground.on_ground = true;
        ground.base.velocity = Vector3::new(0.0, 0.0, -2.0);
        ground.on_collide(Vector3::new(0.1, 0.0, 0.0));
        let expect = -2.0 * (1.0 - ground.base.friction);
        assert!((ground.base.velocity.z - expect).abs() < 1e-6, "{:?}", ground.base.velocity);
        assert!(ground.base.friction > 0.0, "the player has friction to lose");
    }

    /// One fixed step with the collision pass a flat floor at y = 0 would perform: the engine
    /// updates every object and then pushes each out of whatever it is inside
    /// (Engine.cpp:146-205), and that push is what grounds the player again.
    fn step_on_floor(p: &mut Player, input: &Input) {
        let ctx = UpdateCtx {
            input,
            cam_to_world: p.cam_to_world(),
            player_pos: p.obj().pos,
            scene: &[],
        };
        p.update_player(&ctx);
        if p.obj().pos.y < 0.0 {
            p.on_collide(Vector3::new(0.0, -p.obj().pos.y, 0.0));
        }
    }

    #[test]
    fn a_press_lifts_the_player_to_the_apex_and_lands_them_once() {
        let mut p = Player::new();
        let mut input = Input::new();
        let (mut apex, mut launches, mut airborne) = (0.0f32, 0u32, 0u32);
        let mut landings: Vec<f32> = Vec::new();
        // One second: long enough for the whole arc with a quarter to spare. The button is let
        // go after ten steps, because held it would hop again the moment this one lands.
        for i in 0..500 {
            input.key[b' ' as usize] = i < 10;
            step_on_floor(&mut p, &input);
            apex = apex.max(p.obj().pos.y);
            if p.just_jumped() {
                launches += 1;
            }
            if let Some(v) = p.just_landed() {
                landings.push(v);
            }
            if p.obj().pos.y > 1e-4 {
                airborne += 1;
            }
        }
        assert_eq!(launches, 1, "one press, one impulse");
        assert!((apex - crate::ext::jump::APEX).abs() < 0.01, "apex {apex}");
        let flight = airborne as f32 * GH_DT;
        assert!((0.6..0.8).contains(&flight), "airborne {flight} s");
        // And exactly one touchdown, at a speed the drag has taken the edge off.
        assert_eq!(landings.len(), 1, "{landings:?}");
        assert!((landings[0] - 3.1).abs() < 0.2, "landed at {}", landings[0]);
        assert!(p.obj().pos.y.abs() < 1e-3, "back on the floor: {}", p.obj().pos.y);
    }

    #[test]
    fn a_walk_with_nothing_pressed_is_the_ported_walk_bit_for_bit() {
        // The jump block's only write is `velocity.y +=`, behind `Jump::step`. With the button
        // never down there is nothing to write, so `update_player` must still be exactly the
        // physics step and the move the C++ does -- to the bit, not to a tolerance.
        let mut a = Player::new();
        let mut b = Player::new();
        let mut input = Input::new();
        input.key[b'W' as usize] = true;
        input.key[b'D' as usize] = true;
        for _ in 0..200 {
            let ctx = UpdateCtx {
                input: &input,
                cam_to_world: a.cam_to_world(),
                player_pos: a.obj().pos,
                scene: &[],
            };
            a.update_player(&ctx);
            // Player.cpp:36-59 without the bob (which moves the camera, not the player) and
            // without the look (a zero mouse delta leaves both angles at 0.0 exactly).
            b.base.update();
            b.move_player(1.0, -1.0, &factors(false));
        }
        for (x, y) in [(a.obj().pos, b.obj().pos), (a.base.velocity, b.base.velocity)] {
            assert_eq!(x.x.to_bits(), y.x.to_bits());
            assert_eq!(x.y.to_bits(), y.y.to_bits());
            assert_eq!(x.z.to_bits(), y.z.to_bits());
        }
        assert!(!a.just_jumped() && a.just_landed().is_none());
    }

    #[test]
    fn a_seam_in_the_floor_is_not_a_landing() {
        // The Backrooms floor is a scanned triangle mesh, and walking it leaves the player
        // unsupported for a step or two at a time, over and over. Those contacts must not read
        // as touchdowns or a walk down the hall sounds like a flight of stairs.
        let mut p = Player::new();
        let mut input = Input::new();
        input.key[b'W' as usize] = true;
        for i in 0..2000 {
            let seam = i % 40 < 2;
            let ctx = UpdateCtx {
                input: &input,
                cam_to_world: p.cam_to_world(),
                player_pos: p.obj().pos,
                scene: &[],
            };
            p.update_player(&ctx);
            if !seam && p.obj().pos.y < 0.0 {
                p.on_collide(Vector3::new(0.0, -p.obj().pos.y, 0.0));
            }
            assert!(p.just_landed().is_none(), "step {i}");
        }
    }

    #[test]
    fn a_player_stood_on_the_floor_never_reports_a_landing() {
        // The floor pushes a standing player out of itself every step, and `on_ground` is clear
        // by the time it does; without the airborne test that would read as 500 landings a
        // second.
        let mut p = Player::new();
        let input = Input::new();
        for _ in 0..1000 {
            step_on_floor(&mut p, &input);
            assert!(p.just_landed().is_none());
        }
    }

    /// Jump from a standstill or at a run and return the ground covered between launch and
    /// touchdown.
    fn jump_distance(sprinting: bool, forward: bool) -> f32 {
        let mut p = Player::new();
        let mut input = Input::new();
        input.key[b'W' as usize] = forward;
        input.sprint = factors(sprinting);
        // Up to speed first: at 75 u/s^2 the sprint cap is reached in a tenth of a second.
        for _ in 0..500 {
            step_on_floor(&mut p, &input);
        }
        let mut from = p.obj().pos;
        for i in 0..500 {
            input.key[b' ' as usize] = i < 10;
            step_on_floor(&mut p, &input);
            if p.just_jumped() {
                from = p.obj().pos;
            }
            if p.just_landed().is_some() {
                let d = p.obj().pos - from;
                return (d.x * d.x + d.z * d.z).sqrt();
            }
        }
        panic!("never landed");
    }

    #[test]
    fn a_running_jump_goes_further_than_a_walking_one_and_a_standing_one_goes_nowhere() {
        // `Move` clips only the horizontal component of the velocity (Player.cpp:101-107) and
        // runs in the air as much as on the ground, so the speed the player took off with is
        // the speed they keep -- the range follows the run for free.
        let standing = jump_distance(false, false);
        let walking = jump_distance(false, true);
        let running = jump_distance(true, true);
        assert!(standing < 0.01, "standing {standing}");
        assert!((1.8..2.3).contains(&walking), "walking {walking}");
        assert!(running > walking * 1.6, "running {running} vs walking {walking}");
    }

    #[test]
    fn the_jump_events_are_taken_once_and_a_reset_clears_them() {
        let mut p = Player::new();
        let mut input = Input::new();
        input.key[b' ' as usize] = true;
        step_on_floor(&mut p, &input);
        assert!(p.just_jumped());
        assert!(!p.just_jumped(), "taken once");
        input.key[b' ' as usize] = false;
        while p.just_landed().is_none() {
            step_on_floor(&mut p, &input);
        }
        assert!(p.just_landed().is_none(), "taken once");
        // A scene load resets the player; nothing may survive it.
        p.jumped.set(true);
        p.landed.set(Some(3.0));
        p.reset();
        assert!(!p.just_jumped() && p.just_landed().is_none());
    }

    #[test]
    fn a_menu_confirm_held_into_the_game_does_not_launch_the_player() {
        // Space and Cross confirm a menu row. `Engine::apply_menu_action` calls this on every
        // action that closes one, so the button has to come up before it is a jump again.
        let mut p = Player::new();
        let mut input = Input::new();
        input.key[b' ' as usize] = true;
        p.ignore_jump_until_release();
        for _ in 0..200 {
            step_on_floor(&mut p, &input);
            assert!(!p.just_jumped());
        }
        input.key[b' ' as usize] = false;
        step_on_floor(&mut p, &input);
        input.key[b' ' as usize] = true;
        step_on_floor(&mut p, &input);
        assert!(p.just_jumped(), "released and pressed again: a jump");
    }

    #[test]
    fn the_gamepad_button_jumps_too() {
        let mut p = Player::new();
        let mut input = Input::new();
        input.pad_jump = true;
        step_on_floor(&mut p, &input);
        assert!(p.just_jumped());
    }

    #[test]
    fn head_bob_does_not_run_in_the_air() {
        // `mag_t` is zeroed while `!on_ground` (Player.cpp:29-31), so the bob damps out over a
        // flight instead of striding on through it.
        let mut p = Player::new();
        let mut input = Input::new();
        input.key[b'W' as usize] = true;
        for _ in 0..500 {
            step_on_floor(&mut p, &input);
        }
        let walking = p.steps();
        assert!(walking > 0, "the walk was bobbing to begin with");
        for i in 0..400 {
            input.key[b' ' as usize] = i < 10;
            step_on_floor(&mut p, &input);
        }
        // Two steps' worth at most: the collision pass still grounds the player for the step or
        // two it takes the feet to clear the floor.
        assert!(p.steps() - walking <= 2, "{} footfalls in the air", p.steps() - walking);
    }

    #[test]
    fn standing_still_takes_no_steps() {
        let mut p = Player::new();
        let input = Input::new();
        for _ in 0..500 {
            p.on_ground = true;
            let ctx = UpdateCtx {
                input: &input,
                cam_to_world: p.cam_to_world(),
                player_pos: p.obj().pos,
                scene: &[],
            };
            p.update_player(&ctx);
            p.base.velocity.y = 0.0;
        }
        assert_eq!(p.steps(), 0);
    }
}
