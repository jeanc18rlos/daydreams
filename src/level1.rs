//! Port of Level1.h / Level1.cpp.

use std::cell::RefCell;
use std::rc::Rc;

use crate::game_header::GH_PLAYER_HEIGHT;
use crate::player::Player;
use crate::portal::{connect, Portal};
use crate::props::{ground, tunnel, tunnel_set_door1, tunnel_set_door2, TunnelType};
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec, Scene};
use crate::vector::Vector3;

// PORT: the C++ class has no state and no members (Level1.h:4-7); a unit struct is the same
// thing.
pub struct Level1;

impl Scene for Level1 {
    fn load(
        &self,
        gl: &Rc<glow::Context>,
        res: &Resources,
        objs: &mut PObjectVec,
        portals: &mut PPortalVec,
        player: &mut Player,
    ) {
        // PORT: `std::shared_ptr<Tunnel> tunnel1(new Tunnel(Tunnel::NORMAL))` -> an
        // Rc<RefCell<Tunnel>>; the Rc is kept locally after being pushed into `objs`, because
        // SetDoor1/SetDoor2 are called on it later (Level1.cpp:6-9).
        let tunnel1 = Rc::new(RefCell::new(tunnel(gl, res, TunnelType::Normal)));
        {
            let mut t = tunnel1.borrow_mut();
            t.base.pos = Vector3::new(-2.4, 0.0, -1.8);
            t.base.scale = Vector3::new(1.0, 1.0, 4.8);
        }
        objs.push(tunnel1.clone());

        let tunnel2 = Rc::new(RefCell::new(tunnel(gl, res, TunnelType::Normal)));
        {
            let mut t = tunnel2.borrow_mut();
            t.base.pos = Vector3::new(2.4, 0.0, 0.0);
            t.base.scale = Vector3::new(1.0, 1.0, 0.6);
        }
        objs.push(tunnel2.clone());

        let ground1 = Rc::new(RefCell::new(ground(gl, res, false)));
        ground1.borrow_mut().scale *= 1.2;
        objs.push(ground1);

        let portal1 = Rc::new(RefCell::new(Portal::new(gl, res)));
        tunnel_set_door1(&tunnel1.borrow(), &mut portal1.borrow_mut());
        portals.push(portal1.clone());

        let portal2 = Rc::new(RefCell::new(Portal::new(gl, res)));
        tunnel_set_door1(&tunnel2.borrow(), &mut portal2.borrow_mut());
        portals.push(portal2.clone());

        let portal3 = Rc::new(RefCell::new(Portal::new(gl, res)));
        tunnel_set_door2(&tunnel1.borrow(), &mut portal3.borrow_mut());
        portals.push(portal3.clone());

        let portal4 = Rc::new(RefCell::new(Portal::new(gl, res)));
        tunnel_set_door2(&tunnel2.borrow(), &mut portal4.borrow_mut());
        portals.push(portal4.clone());

        connect(&portal1, &portal2);
        connect(&portal3, &portal4);

        // PORT: `player.SetPosition(..)` is Physical::SetPosition, reached through the `base`
        // field (was: Level1.cpp:39).
        player
            .base
            .set_position(Vector3::new(0.0, GH_PLAYER_HEIGHT, 5.0));
    }
}
