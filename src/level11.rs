//! EXT: Scene `=` -- "Anamorphic Chamber". Not part of the C++ port.
//!
//! Twelve fragments hang scattered through the room at wildly different depths and sizes. From
//! almost everywhere they are debris. From **one** spot they resolve into a perfect ring, and
//! holding that spot materialises what the ring frames.
//!
//! This is anamorphosis, the trick behind Superliminal's painted-object puzzles and behind
//! Holbein's skull: a form that exists only from a single station point.
//!
//! # Constructing it
//!
//! Work backwards from the answer. Pick the solution eye point, then for each fragment choose
//! where it should land *in the eye's view* -- a point on a circle, expressed as a direction in
//! tangent space -- and how far away it should sit. The world position is just
//!
//! ```text
//!     world = eye + normalize(u, v, -1) * depth
//! ```
//!
//! Any depth puts the fragment on the same line of sight, so every choice of depth produces the
//! same picture from the eye and a different picture from anywhere else. Scaling each fragment
//! proportionally to its depth makes them all *look* the same size too, which is what removes
//! the last depth cue and makes the illusion snap.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ext::bounds::bounds_box;
use crate::ext::room::RoomLogic;
use crate::game_header::{GH_PI, GH_PLAYER_HEIGHT};
use crate::object::ObjectT;
use crate::player::Player;
use crate::props::{ground, pillar, statue};
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec, Scene};
use crate::vector::Vector3;

pub struct Level11;

/// Number of fragments in the ring.
const FRAGMENTS: usize = 12;
/// Ring radius in tangent space. 0.35 is about 19 degrees, comfortably inside the 60 degree FOV.
const RING_RADIUS: f32 = 0.35;
/// How close the player must stand to the station point for the illusion to be "solved".
const SOLVE_RADIUS: f32 = 1.3;
/// How fast the reward statue fades in and out, per fixed step.
const REVEAL_SPEED: f32 = 0.010;
/// Never let scale reach zero -- `Object::world_to_local` divides by it (Object.cpp:42).
const MIN_SCALE: f32 = 0.001;

impl Scene for Level11 {
    fn load(
        &self,
        gl: &Rc<glow::Context>,
        res: &Resources,
        objs: &mut PObjectVec,
        _portals: &mut PPortalVec,
        player: &mut Player,
    ) {
        let mut g = ground(gl, res, false);
        g.scale *= 3.0;
        objs.push(Rc::new(RefCell::new(g)));

        // The station point: where the player must stand for the ring to resolve.
        let eye = Vector3::new(0.0, GH_PLAYER_HEIGHT, 10.0);

        for k in 0..FRAGMENTS {
            let theta = 2.0 * GH_PI * (k as f32) / (FRAGMENTS as f32);
            // Where this fragment should appear in the solved image.
            let u = RING_RADIUS * theta.cos();
            let v = RING_RADIUS * theta.sin();

            // Deterministic depth scatter -- irrational-ish stride so no two land alike and the
            // arrangement never looks periodic from off-axis.
            let depth = 5.0 + 9.0 * (0.5 + 0.5 * (k as f32 * 2.399963).sin());

            // The eye looks down -Z, so tangent-space (u, v) becomes direction (u, v, -1).
            let dir = Vector3::new(u, v, -1.0).normalized();
            let pos = eye + dir * depth;

            let mut frag = pillar(gl, res);
            frag.pos = pos;
            // Scale with depth so every fragment subtends the same angle from the station point.
            // This is the step that kills the last depth cue.
            frag.scale = Vector3::splat(0.02 * depth);
            // Tumble them so they read as debris rather than as a deliberate arc.
            frag.euler = Vector3::new(theta * 0.7, theta * 1.3, theta * 0.4);
            objs.push(Rc::new(RefCell::new(frag)));
        }

        // What the ring frames, once solved. Starts invisibly small.
        let reward = {
            let mut s = statue(gl, res, "suzanne.obj");
            // Centre of the ring, at a depth in the middle of the fragment spread.
            s.pos = eye + Vector3::new(0.0, 0.0, -1.0) * 9.5;
            s.scale = Vector3::splat(MIN_SCALE);
            s.euler.y = GH_PI;
            Rc::new(RefCell::new(s))
        };
        objs.push(reward.clone());

        // Markers on the floor, so the station point is findable rather than pixel-hunted.
        for (dx, dz) in [(-0.6f32, 0.0f32), (0.6, 0.0), (0.0, -0.6), (0.0, 0.6)] {
            let mut m = pillar(gl, res);
            m.pos = Vector3::new(eye.x + dx, 0.0, eye.z + dz);
            m.scale = Vector3::new(0.02, 0.004, 0.02);
            objs.push(Rc::new(RefCell::new(m)));
        }

        // ── Reveal logic: grow the statue while the player holds the station point. ────────
        let mut progress = 0.0f32;
        let logic = RoomLogic::new(move |ctx| {
            let mut d = ctx.player_pos - eye;
            d.y = 0.0; // height should not matter; standing on the spot is enough
            let solved = d.mag() < SOLVE_RADIUS;

            progress = if solved {
                (progress + REVEAL_SPEED).min(1.0)
            } else {
                (progress - REVEAL_SPEED).max(0.0)
            };

            if let Ok(mut r) = reward.try_borrow_mut() {
                // Ease so the arrival feels like a materialisation, not a linear grow.
                let eased = progress * progress * (3.0 - 2.0 * progress);
                r.scale = Vector3::splat(MIN_SCALE + eased * 1.6);
            }
        });
        objs.push(Rc::new(RefCell::new(logic)) as Rc<RefCell<dyn ObjectT>>);

        // Spawn off to the side and well off-axis, so the first view is definitively scattered.
        player.base.set_position(Vector3::new(7.0, GH_PLAYER_HEIGHT, 14.0));

        // EXT: invisible bounds keeping players and thrown props inside the playable area.
        objs.push(Rc::new(RefCell::new(bounds_box(
            res,
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(30.0, 8.0, 30.0),
        ))) as Rc<RefCell<dyn ObjectT>>);
    }
}
