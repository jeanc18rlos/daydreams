//! Port of Player.h / Player.cpp.

use crate::game_header::{
    GH_BOB_DAMP, GH_BOB_FREQ, GH_BOB_MIN, GH_BOB_OFFS, GH_DT, GH_MOUSE_SENSITIVITY, GH_PI,
    GH_PLAYER_HEIGHT, GH_PLAYER_RADIUS, GH_WALK_ACCEL, GH_WALK_SPEED,
};
use crate::object::{Object, ObjectT, UpdateCtx};
use crate::physical::Physical;
use crate::sphere::Sphere;
use crate::vector::{Matrix4, Vector3};

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
        };
        p.reset();
        p.base
            .hit_spheres
            .push(Sphere::new_at(Vector3::new(0.0, 0.0, 0.0), GH_PLAYER_RADIUS));
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
    }

    // PORT: named `update_player` so it does not collide with `Physical::update`, which is
    // reachable through the `base` field (was: void Player::Update(), Player.cpp:24).
    // GH_INPUT (the global) arrives as `ctx.input`.
    pub fn update_player(&mut self, ctx: &UpdateCtx) {
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
            self.bob_phi += GH_BOB_FREQ * GH_DT;
            if self.bob_phi > 2.0 * GH_PI {
                self.bob_phi -= 2.0 * GH_PI;
            }
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
        self.move_player(move_f, move_l);

        // PORT: the `#if 0` jumping block (Player.cpp:61-66) is preserved verbatim as a
        // never-called private fn, since Rust has no `#if 0`.
        #[allow(dead_code)]
        fn _jump_disabled(p: &mut Player, ctx: &UpdateCtx) {
            //Jumping
            // PORT: VK_SPACE == 0x20 == b' ' in Input's ASCII key slots.
            if p.on_ground && ctx.input.key[b' ' as usize] {
                p.base.velocity.y += 2.0 * p.base.base.p_scale;
            }
        }

        //Reset ground state after update finishes
        self.on_ground = false;
    }

    pub fn look(&mut self, mouse_dx: f32, mouse_dy: f32) {
        //Adjust x-axis rotation
        self.cam_rx -= mouse_dy * GH_MOUSE_SENSITIVITY;
        if self.cam_rx > GH_PI / 2.0 {
            self.cam_rx = GH_PI / 2.0;
        } else if self.cam_rx < -GH_PI / 2.0 {
            self.cam_rx = -GH_PI / 2.0;
        }

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
    pub fn move_player(&mut self, mut move_f: f32, mut move_l: f32) {
        //Make sure movement is not too fast
        let mag = (move_f * move_f + move_l * move_l).sqrt();
        if mag > 1.0 {
            move_f /= mag;
            move_l /= mag;
        }

        //Movement
        let cam_to_world = self.base.local_to_world() * Matrix4::rot_y(self.cam_ry);
        self.base.velocity += cam_to_world.mul_direction(Vector3::new(-move_l, 0.0, -move_f))
            * (GH_WALK_ACCEL * GH_DT);

        //Don't allow non-falling speeds above the player's max speed
        let temp_y = self.base.velocity.y;
        self.base.velocity.y = 0.0;
        self.base
            .velocity
            .clip_mag(self.base.base.p_scale * GH_WALK_SPEED);
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
}
