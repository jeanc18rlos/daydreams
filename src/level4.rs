//! Port of Level4.h / Level4.cpp.

use std::cell::RefCell;
use std::rc::Rc;

use crate::game_header::{GH_PI, GH_PLAYER_HEIGHT};
use crate::player::Player;
use crate::portal::{connect, Portal};
use crate::props::{ground, tunnel, tunnel_set_door1, tunnel_set_door2, TunnelType};
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec, Scene};
use crate::vector::Vector3;

pub struct Level4;

impl Scene for Level4 {
    fn load(
        &self,
        gl: &Rc<glow::Context>,
        res: &Resources,
        objs: &mut PObjectVec,
        portals: &mut PPortalVec,
        player: &mut Player,
    ) {
        let tunnel1 = Rc::new(RefCell::new(tunnel(gl, res, TunnelType::Slope)));
        {
            let mut t = tunnel1.borrow_mut();
            t.base.pos = Vector3::new(0.0, 0.0, 0.0);
            t.base.scale = Vector3::new(1.0, 1.0, 5.0);
            t.base.euler.y = GH_PI;
        }
        objs.push(tunnel1.clone());

        let ground1 = Rc::new(RefCell::new(ground(gl, res, true)));
        ground1.borrow_mut().scale *= Vector3::new(1.0, 2.0, 1.0);
        objs.push(ground1);

        let tunnel2 = Rc::new(RefCell::new(tunnel(gl, res, TunnelType::Slope)));
        {
            let mut t = tunnel2.borrow_mut();
            t.base.pos = Vector3::new(200.0, 0.0, 0.0);
            t.base.scale = Vector3::new(1.0, 1.0, 5.0);
        }
        objs.push(tunnel2.clone());

        let ground2 = Rc::new(RefCell::new(ground(gl, res, true)));
        {
            let mut g = ground2.borrow_mut();
            g.pos = Vector3::new(200.0, 0.0, 0.0);
            g.scale *= Vector3::new(1.0, 2.0, 1.0);
            g.euler.y = GH_PI;
        }
        objs.push(ground2);

        let portal1 = Rc::new(RefCell::new(Portal::new(res)));
        tunnel_set_door1(&tunnel1.borrow(), &mut portal1.borrow_mut());
        portals.push(portal1.clone());

        let portal2 = Rc::new(RefCell::new(Portal::new(res)));
        tunnel_set_door2(&tunnel1.borrow(), &mut portal2.borrow_mut());
        portals.push(portal2.clone());

        let portal3 = Rc::new(RefCell::new(Portal::new(res)));
        tunnel_set_door1(&tunnel2.borrow(), &mut portal3.borrow_mut());
        portal3.borrow_mut().base.euler.y -= GH_PI;
        portals.push(portal3.clone());

        let portal4 = Rc::new(RefCell::new(Portal::new(res)));
        tunnel_set_door2(&tunnel2.borrow(), &mut portal4.borrow_mut());
        portal4.borrow_mut().base.euler.y -= GH_PI;
        portals.push(portal4.clone());

        connect(&portal1, &portal4);
        connect(&portal2, &portal3);

        player.base.set_position(Vector3::new(0.0, GH_PLAYER_HEIGHT - 2.0, 8.0));
    }
}
