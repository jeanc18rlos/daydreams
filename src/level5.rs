//! Port of Level5.h / Level5.cpp.

use std::cell::RefCell;
use std::rc::Rc;

use crate::game_header::{GH_PI, GH_PLAYER_HEIGHT};
use crate::player::Player;
use crate::portal::{connect, Portal};
use crate::props::{ground, tunnel, tunnel_set_door1, tunnel_set_door2, TunnelType};
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec, Scene};
use crate::vector::Vector3;

pub struct Level5;

impl Scene for Level5 {
    fn load(
        &self,
        gl: &Rc<glow::Context>,
        res: &Resources,
        objs: &mut PObjectVec,
        portals: &mut PPortalVec,
        player: &mut Player,
    ) {
        let tunnel1 = Rc::new(RefCell::new(tunnel(gl, res, TunnelType::Scale)));
        {
            let mut t = tunnel1.borrow_mut();
            t.base.pos = Vector3::new(-1.2, 0.0, 0.0);
            t.base.scale = Vector3::new(1.0, 1.0, 2.4);
        }
        objs.push(tunnel1.clone());

        let ground1 = Rc::new(RefCell::new(ground(gl, res, false)));
        ground1.borrow_mut().scale *= 1.2;
        objs.push(ground1);

        let tunnel2 = Rc::new(RefCell::new(tunnel(gl, res, TunnelType::Normal)));
        {
            let mut t = tunnel2.borrow_mut();
            t.base.pos = Vector3::new(201.2, 0.0, 0.0);
            t.base.scale = Vector3::new(1.0, 1.0, 2.4);
        }
        objs.push(tunnel2.clone());

        let ground2 = Rc::new(RefCell::new(ground(gl, res, false)));
        {
            let mut g = ground2.borrow_mut();
            g.pos = Vector3::new(200.0, 0.0, 0.0);
            g.scale *= 1.2;
        }
        objs.push(ground2);

        let portal1 = Rc::new(RefCell::new(Portal::new(res)));
        tunnel_set_door1(&tunnel1.borrow(), &mut portal1.borrow_mut());
        portals.push(portal1.clone());

        let portal2 = Rc::new(RefCell::new(Portal::new(res)));
        tunnel_set_door1(&tunnel2.borrow(), &mut portal2.borrow_mut());
        portals.push(portal2.clone());

        let portal3 = Rc::new(RefCell::new(Portal::new(res)));
        tunnel_set_door2(&tunnel1.borrow(), &mut portal3.borrow_mut());
        portals.push(portal3.clone());

        let portal4 = Rc::new(RefCell::new(Portal::new(res)));
        tunnel_set_door2(&tunnel2.borrow(), &mut portal4.borrow_mut());
        portals.push(portal4.clone());

        connect(&portal1, &portal2);
        connect(&portal3, &portal4);

        let tunnel3 = Rc::new(RefCell::new(tunnel(gl, res, TunnelType::Normal)));
        {
            let mut t = tunnel3.borrow_mut();
            t.base.pos = Vector3::new(-1.0, 0.0, -4.2);
            t.base.scale = Vector3::new(0.25, 0.25, 0.6);
            t.base.euler.y = GH_PI / 2.0;
        }
        objs.push(tunnel3);

        player
            .base
            .set_position(Vector3::new(0.0, GH_PLAYER_HEIGHT, 5.0));
    }
}
