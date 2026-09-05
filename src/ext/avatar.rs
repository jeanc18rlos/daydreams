//! EXT: the player's own body. Not part of the C++ port.
//!
//! First person needs no avatar and the port never had one -- `Player` carries two collision
//! spheres and no mesh at all. Hide 'N Dream needs one: the game is about your silhouette
//! being seen, and the third-person camera (`ext/thirdperson.rs`) is pointed at a hole in
//! the world without this.
//!
//! It is the same Mixamo mannequin the sleepwalkers wear (`ext/npc.rs`), driven by one
//! rule -- the clip follows the speed, the speed is never fitted to the clip -- so a walking
//! player's feet keep up with the ground the way the NPCs' do.
//!
//! # It is a puppet, not a body
//!
//! The avatar has no hit sphere, no collision and no physics: `Player` remains the only
//! thing the world pushes, and this only ever mirrors where that ended up. Two consequences
//! worth stating, because both are load-bearing for the round:
//!
//! * it cannot be grabbed (grabbability is "exactly one hit sphere", `ext/grab.rs`), so the
//!   seeker can never pick up their own body;
//! * it is not in the `blockers` list any observation test raycasts against, so a player can
//!   never occlude themselves from a watcher -- the Chameleon Rule stays honest.
//!
//! In first person it draws nothing, which is also what happens when the boom is pulled in
//! so far that the camera is inside the mannequin's head.

use crate::camera::Camera;
use crate::ext::skinned::{Instance, SkinnedModel};
use crate::ext::thirdperson;
use crate::game_header::{GH_DT, GH_PLAYER_HEIGHT};
use crate::object::{Object, ObjectT, RenderCtx, UpdateCtx};
use crate::resources::Resources;
use crate::vector::Vector3;
use std::rc::Rc;

// The mannequin and its part name are the sleepwalkers' own (`ext/npc.rs`): one parse and
// one upload serve every body in the scene, the player's included.
use crate::ext::npc::{MODEL, PART};

const CLIP_IDLE: &str = "standing_idle";
const CLIP_WALK: &str = "walking";
const CLIP_RUN: &str = "running";

/// Ground covered per second by the walk and run clips at playback rate 1.0. The clip is
/// paced to the player's speed by dividing, exactly as `ext/npc.rs` paces the sleepwalkers.
const WALK_CYCLE_SPEED: f32 = 1.15;
const RUN_CYCLE_SPEED: f32 = 3.2;

/// Below this the player is standing still (m/s at `p_scale == 1`).
const STILL: f32 = 0.35;
/// Above this the walk becomes a run. The walk cap is 2.9 m/s and sprint reaches 5.22.
const RUNNING: f32 = 3.4;

/// Which clip is playing, so a clip is only re-set when it actually changes -- `set_clip`
/// re-poses the whole mesh.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Gait {
    Idle,
    Walk,
    Run,
}

pub struct Avatar {
    base: Object,
    inst: Instance,
    /// Distance from the model's origin down to its soles, so it stands ON the floor.
    lift: f32,
    /// Smoothed horizontal speed, in metres per second of the player's own scale.
    speed: f32,
    gait: Gait,
    /// Last step's eye position, to difference into a speed.
    prev: Option<Vector3>,
}

impl Avatar {
    pub fn new(gl: &Rc<glow::Context>, res: &Resources) -> Avatar {
        let model = SkinnedModel::acquire(gl, MODEL);
        let mut inst = Instance::new(gl, res, &model, Some(PART));
        inst.set_clip(CLIP_IDLE, 1.0);
        let b = inst.bounds();
        Avatar { base: Object::new(), inst, lift: -b[2], speed: 0.0, gait: Gait::Idle, prev: None }
    }

    fn set_gait(&mut self, gait: Gait, speed: f32) {
        // The run and walk clips are re-paced every step even when the gait has not
        // changed -- that is a rate, not a re-pose, and it is what keeps the feet planted
        // as the player accelerates.
        match gait {
            Gait::Idle => {
                if self.gait != gait {
                    self.inst.set_clip(CLIP_IDLE, 1.0);
                }
            }
            Gait::Walk => self.inst.set_clip(CLIP_WALK, (speed / WALK_CYCLE_SPEED).max(0.35)),
            Gait::Run => self.inst.set_clip(CLIP_RUN, (speed / RUN_CYCLE_SPEED).max(0.5)),
        }
        self.gait = gait;
    }
}

impl ObjectT for Avatar {
    fn base(&self) -> &Object {
        &self.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base
    }

    fn update(&mut self, ctx: &UpdateCtx) {
        let p_scale = ctx.player_p_scale.max(1e-4);
        // The player's position is their EYE; the avatar stands on their soles.
        let feet = ctx.player_pos - Vector3::new(0.0, GH_PLAYER_HEIGHT * p_scale, 0.0);

        // Speed, measured in the player's own scale so a shrunk player still reads as
        // walking rather than sprinting.
        let step = match self.prev {
            Some(prev) => {
                let mut d = ctx.player_pos - prev;
                d.y = 0.0;
                // A portal crossing moves the eye a long way in one step; that is a
                // teleport, not a stride, so it must not spike the gait.
                let raw = d.mag() / (GH_DT * p_scale);
                if raw > 40.0 {
                    self.speed
                } else {
                    raw
                }
            }
            None => 0.0,
        };
        self.prev = Some(ctx.player_pos);
        self.speed += (step - self.speed) * 0.08;

        self.base.pos = feet + Vector3::new(0.0, self.lift * p_scale, 0.0);
        self.base.scale = Vector3::splat(p_scale);
        // Face where the player is looking, flattened: the body turns with the head but
        // never leans with it.
        let fwd = ctx.cam_to_world.mul_direction(Vector3::new(0.0, 0.0, -1.0));
        if fwd.x.abs() + fwd.z.abs() > 1e-4 {
            self.base.euler.y = fwd.x.atan2(fwd.z);
        }

        // Skinning is CPU work; an unseen body does not pay for it. The pose it resumes on
        // is whatever it held when the camera last looked, which nobody can tell from a
        // fresh one.
        if !thirdperson::enabled() {
            return;
        }

        let gait = if self.speed < STILL {
            Gait::Idle
        } else if self.speed < RUNNING {
            Gait::Walk
        } else {
            Gait::Run
        };
        self.set_gait(gait, self.speed);
        self.inst.update();
    }

    fn draw(&self, ctx: &RenderCtx, cam: &Camera, _fbo: Option<glow::Framebuffer>) {
        // Nothing to draw from inside your own head. `MIN_FRACTION` is where the boom stops
        // shortening, so this also covers the camera being shoved against a wall.
        if !thirdperson::enabled() {
            return;
        }
        self.inst.draw(&self.base, ctx, cam);
    }

    /// A puppet: nothing collides with it, nothing warps it, and with no hit sphere the
    /// grab cannot pick it up. See the module docs.
    fn engine_collision(&self) -> bool {
        false
    }
}
