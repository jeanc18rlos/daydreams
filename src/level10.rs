//! EXT: Scene `-` -- "Unobserved". Not part of the C++ port.
//!
//! Observation-dependent geometry. Statues only ever move while you are not looking at them.
//! Face one and it is inert stone. Turn away, turn back, and it is closer than it was.
//!
//! # How "being looked at" is decided
//!
//! Two tests per statue, per fixed step (`ext/visibility.rs`):
//!   * a **view-cone** test against the player's eye transform, deliberately wider than the
//!     render frustum so peripheral vision counts as watching;
//!   * a **line-of-sight raycast**, so a statue genuinely hidden behind a pillar may advance
//!     even while it is nominally in front of you.
//!
//! The second test is what makes the room feel deliberate rather than broken: cover is real, and
//! a statue can gain ground by letting the architecture eclipse it.
//!
//! # The portal twist
//!
//! There are **two** chambers, 200 units apart and portal-linked in CodeParade's Level3 idiom
//! (Level3.cpp:24,38,52), each with its own statues. The doorway does not show you a wall -- it
//! shows you the other chamber, live.
//!
//! So the room you are not standing in is still being watched, as long as you are facing the
//! doorway. Guarding one chamber means turning your back on the other, and the portal is the
//! only thing that lets you watch both at once -- which is exactly when the statues behind
//! *you* start moving.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ext::bounds::bounds_box;
use crate::ext::room::RoomLogic;
use crate::ext::visibility::is_observed;
use crate::game_header::{GH_PI, GH_PLAYER_HEIGHT};
use crate::object::{Object, ObjectT};
use crate::player::Player;
use crate::portal::{connect, Portal};
use crate::props::{ground, pillar, pillar_room, pillar_room_set_portal, statue};
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec, Scene};
use crate::vector::Vector3;

pub struct Level10;

/// World units a statue creeps per fixed step while unobserved.
/// At 500 Hz that is roughly 0.6 units/second -- noticeable, but never caught in the act.
const CREEP_SPEED: f32 = 0.0012;
/// Statues stop this far out, so they crowd rather than intersect the player.
const STOP_DIST: f32 = 1.5;
/// Chamber separation, matching the Level3 idiom.
const CHAMBER_SPACING: f32 = 200.0;

impl Scene for Level10 {
    fn load(
        &self,
        gl: &Rc<glow::Context>,
        res: &Resources,
        objs: &mut PObjectVec,
        portals: &mut PPortalVec,
        player: &mut Player,
    ) {
        // Everything solid, for the line-of-sight test to raycast against.
        let mut blockers: Vec<Rc<RefCell<dyn ObjectT>>> = Vec::new();
        // Every statue, paired with the chamber origin it belongs to.
        let mut statues: Vec<(Rc<RefCell<Object>>, Vector3)> = Vec::new();
        let mut chamber_portals: Vec<Rc<RefCell<Portal>>> = Vec::new();

        for c in 0..2 {
            let origin = Vector3::new(c as f32 * CHAMBER_SPACING, 0.0, 0.0);

            let mut room = pillar_room(gl, res);
            room.pos = origin;
            room.scale = Vector3::splat(1.3);
            let room = Rc::new(RefCell::new(room));
            blockers.push(room.clone() as Rc<RefCell<dyn ObjectT>>);
            objs.push(room.clone());

            let mut g = ground(gl, res, false);
            g.pos = origin;
            g.scale *= 2.0;
            objs.push(Rc::new(RefCell::new(g)));

            // Pillars double as cover.
            for (dx, dz) in [(-3.5f32, -3.5f32), (3.5, -3.5), (-3.5, 3.5), (3.5, 3.5)] {
                let mut p = pillar(gl, res);
                p.pos = origin + Vector3::new(dx, 0.0, dz);
                p.scale = Vector3::splat(0.12);
                let p = Rc::new(RefCell::new(p));
                blockers.push(p.clone() as Rc<RefCell<dyn ObjectT>>);
                objs.push(p);
            }

            // Two statues per chamber, on opposite sides, so neither chamber is ever "clear".
            let placements: [(&str, Vector3, f32); 2] = if c == 0 {
                [
                    ("suzanne.obj", Vector3::new(0.0, 1.0, -8.0), 0.9),
                    ("teapot.obj", Vector3::new(7.5, 0.5, 2.0), 0.5),
                ]
            } else {
                [
                    ("bunny.obj", Vector3::new(0.0, -0.4, -8.0), 12.0),
                    ("suzanne.obj", Vector3::new(-7.5, 1.0, 2.0), 0.9),
                ]
            };
            for (mesh, offset, scale) in placements {
                let mut s = statue(gl, res, mesh);
                s.pos = origin + offset;
                s.scale = Vector3::splat(scale);
                let s = Rc::new(RefCell::new(s));
                statues.push((s.clone(), origin));
                objs.push(s);
            }

            let portal = Rc::new(RefCell::new(Portal::new(gl, res)));
            pillar_room_set_portal(&room.borrow(), &mut portal.borrow_mut());
            portals.push(portal.clone());
            chamber_portals.push(portal);

            // EXT: invisible bounds keeping players and thrown props inside this chamber.
            objs.push(Rc::new(RefCell::new(bounds_box(
                res,
                origin,
                Vector3::new(20.0, 8.0, 20.0),
            ))) as Rc<RefCell<dyn ObjectT>>);
        }

        // Link the chambers. `connect` wires front<->back both ways (Portal.cpp:106-109), so the
        // doorway is two-way and each chamber's portal renders the other chamber live.
        connect(&chamber_portals[0], &chamber_portals[1]);

        // ── The logic object: invisible, ticks every fixed step. ───────────────────────────
        let logic = RoomLogic::new(move |ctx| {
            for (s, origin) in &statues {
                let Ok(mut st) = s.try_borrow_mut() else { continue };
                let pos = st.pos;

                // `skip` is None: statues are not in `blockers`, so none can occlude itself.
                if is_observed(&blockers, &ctx.cam_to_world, pos, None) {
                    continue; // watched -- hold perfectly still
                }

                // Creep toward the player's position *within its own chamber*. The player is
                // only ever in one chamber; the other chamber's statues advance on the matching
                // spot in theirs, so returning through the portal is never safe either.
                let mut target = ctx.player_pos;
                let player_chamber = (ctx.player_pos.x / CHAMBER_SPACING).round() * CHAMBER_SPACING;
                target.x = target.x - player_chamber + origin.x;

                let mut delta = target - pos;
                delta.y = 0.0;
                let dist = delta.mag();
                if dist <= STOP_DIST {
                    continue;
                }
                st.pos += delta / dist * CREEP_SPEED;

                // Always face the player, so the approach reads as intent rather than drift.
                st.euler.y = (-delta.x).atan2(-delta.z) + GH_PI;
            }
        });
        objs.push(Rc::new(RefCell::new(logic)) as Rc<RefCell<dyn ObjectT>>);

        player.base.set_position(Vector3::new(0.0, GH_PLAYER_HEIGHT, 3.0));
    }
}
