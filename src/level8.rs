//! EXT: Scene 9 -- "Penrose Ascent". Not part of the C++ port.
//!
//! An infinite staircase. Four sloped tunnels, each descending two units end to end, wired into
//! a **closed four-cycle**: the bottom of each tunnel opens onto the top of the next, and the
//! bottom of the fourth opens onto the top of the first.
//!
//! Walk forwards and you descend forever. Turn around and you climb forever. Neither direction
//! ever arrives anywhere, and at no point is there a visible seam or a moment of teleportation --
//! the portal renders the next flight, so you simply keep walking.
//!
//! This is the Penrose staircase built honestly: nothing here is a forced-perspective trick or a
//! camera cheat. The geometry really is four ordinary descending corridors; it is the *topology*
//! that is impossible.
//!
//! # Why the flights sit 200 units apart
//!
//! They are never seen together, so there is no reason to make them physically adjacent -- and
//! separating them avoids the four tunnels colliding in world space. This is exactly the idiom
//! CodeParade used for the three pillar rooms in Level3 (Level3.cpp:24,38,52), which live at
//! x = 0, 200 and 400 while appearing to be one connected space.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ext::bounds::bounds_box;
use crate::game_header::{GH_PI, GH_PLAYER_HEIGHT};
use crate::player::Player;
use crate::portal::{connect, Portal};
use crate::props::{ground, tunnel, tunnel_set_door1, tunnel_set_door2, TunnelType};
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec, Scene};
use crate::vector::Vector3;

pub struct Level8;

/// Number of flights in the cycle. Four gives the classic square Penrose footprint.
const FLIGHTS: usize = 4;
/// How far apart the flights sit in world space. Never visible simultaneously.
const FLIGHT_SPACING: f32 = 200.0;

impl Scene for Level8 {
    fn load(
        &self,
        gl: &Rc<glow::Context>,
        res: &Resources,
        objs: &mut PObjectVec,
        portals: &mut PPortalVec,
        player: &mut Player,
    ) {
        // Each flight reproduces Level4's known-good sloped-tunnel configuration
        // (Level4.cpp:6-20): tunnel rotated by PI, ground left unrotated, ground stretched 2x
        // vertically so the slope actually meets the tunnel floor.
        let mut tops: Vec<Rc<RefCell<Portal>>> = Vec::with_capacity(FLIGHTS);
        let mut bottoms: Vec<Rc<RefCell<Portal>>> = Vec::with_capacity(FLIGHTS);

        for i in 0..FLIGHTS {
            let x = i as f32 * FLIGHT_SPACING;

            let t = Rc::new(RefCell::new(tunnel(gl, res, TunnelType::Slope)));
            {
                let mut t = t.borrow_mut();
                t.base.pos = Vector3::new(x, 0.0, 0.0);
                t.base.scale = Vector3::new(1.0, 1.0, 5.0);
                t.base.euler.y = GH_PI;
            }

            let g = Rc::new(RefCell::new(ground(gl, res, true)));
            {
                let mut g = g.borrow_mut();
                g.pos = Vector3::new(x, 0.0, 0.0);
                g.scale *= Vector3::new(1.0, 2.0, 1.0);
            }
            objs.push(g);

            // door1 is the high end of the slope, door2 the low end (Tunnel.h:33-45).
            let top = Rc::new(RefCell::new(Portal::new(res)));
            tunnel_set_door1(&t.borrow(), &mut top.borrow_mut());
            portals.push(top.clone());
            tops.push(top);

            let bottom = Rc::new(RefCell::new(Portal::new(res)));
            tunnel_set_door2(&t.borrow(), &mut bottom.borrow_mut());
            portals.push(bottom.clone());
            bottoms.push(bottom);

            objs.push(t);

            // EXT: invisible bounds keeping players and thrown props inside this flight.
            // Floor level is -2.0, not 0.0: ground_slope.obj descends to y=-1 in unit space and
            // the ground is scaled 2x vertically, so the low plateau sits at y=-2. bounds.obj
            // spans y[0,1] from its centre, so a centre at y=0 would leave the walls starting
            // above the player's hit spheres (eye y=-0.5, feet y=-1.8) on the lower half of the
            // flight, letting them walk off the edge. Wall height 10 keeps the ceiling at y=8.
            objs.push(Rc::new(RefCell::new(bounds_box(
                res,
                Vector3::new(x, -2.0, 0.0),
                Vector3::new(10.0, 10.0, 10.0),
            ))));
        }

        // The cycle: bottom of flight i opens onto the top of flight i+1, wrapping at the end.
        // That wrap is the whole trick -- it is what makes the descent never terminate.
        for i in 0..FLIGHTS {
            connect(&bottoms[i], &tops[(i + 1) % FLIGHTS]);
        }

        // Start at the top of the first flight, matching Level4's spawn height for a sloped
        // tunnel (Level4.cpp:48).
        player.base.set_position(Vector3::new(0.0, GH_PLAYER_HEIGHT - 2.0, 8.0));
    }
}
