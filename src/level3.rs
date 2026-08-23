//! Port of Level3.h / Level3.cpp.

use std::cell::RefCell;
use std::rc::Rc;

use crate::game_header::{GH_PI, GH_PLAYER_HEIGHT};
use crate::player::Player;
use crate::portal::{connect_warps, Portal, Side};
use crate::props::{ground, pillar, pillar_room, pillar_room_set_portal, statue};
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec, Scene};
use crate::vector::Vector3;

pub struct Level3;

impl Scene for Level3 {
    fn load(
        &self,
        gl: &Rc<glow::Context>,
        res: &Resources,
        objs: &mut PObjectVec,
        portals: &mut PPortalVec,
        player: &mut Player,
    ) {
        //Room 1
        let pillar1 = Rc::new(RefCell::new(pillar(gl, res)));
        objs.push(pillar1);

        let pillar_room1 = Rc::new(RefCell::new(pillar_room(gl, res)));
        objs.push(pillar_room1.clone());

        let ground1 = Rc::new(RefCell::new(ground(gl, res, false)));
        ground1.borrow_mut().scale *= 2.0;
        objs.push(ground1);

        let statue1 = Rc::new(RefCell::new(statue(gl, res, "teapot.obj")));
        {
            let mut s = statue1.borrow_mut();
            s.pos = Vector3::new(0.0, 0.5, 9.0);
            s.scale = Vector3::splat(0.5);
            s.euler.y = GH_PI / 2.0;
        }
        objs.push(statue1);

        //Room 2
        let pillar2 = Rc::new(RefCell::new(pillar(gl, res)));
        pillar2.borrow_mut().pos = Vector3::new(200.0, 0.0, 0.0);
        objs.push(pillar2);

        let pillar_room2 = Rc::new(RefCell::new(pillar_room(gl, res)));
        pillar_room2.borrow_mut().pos = Vector3::new(200.0, 0.0, 0.0);
        objs.push(pillar_room2.clone());

        let ground2 = Rc::new(RefCell::new(ground(gl, res, false)));
        {
            let mut g = ground2.borrow_mut();
            g.pos = Vector3::new(200.0, 0.0, 0.0);
            g.scale *= 2.0;
        }
        objs.push(ground2);

        let statue2 = Rc::new(RefCell::new(statue(gl, res, "bunny.obj")));
        {
            let mut s = statue2.borrow_mut();
            s.pos = Vector3::new(200.0, -0.4, 9.0);
            s.scale = Vector3::splat(14.0);
            s.euler.y = GH_PI;
        }
        objs.push(statue2);

        //Room 3
        let pillar3 = Rc::new(RefCell::new(pillar(gl, res)));
        pillar3.borrow_mut().pos = Vector3::new(400.0, 0.0, 0.0);
        objs.push(pillar3);

        let pillar_room3 = Rc::new(RefCell::new(pillar_room(gl, res)));
        pillar_room3.borrow_mut().pos = Vector3::new(400.0, 0.0, 0.0);
        objs.push(pillar_room3.clone());

        let ground3 = Rc::new(RefCell::new(ground(gl, res, false)));
        {
            let mut g = ground3.borrow_mut();
            g.pos = Vector3::new(400.0, 0.0, 0.0);
            g.scale *= 2.0;
        }
        objs.push(ground3);

        let statue3 = Rc::new(RefCell::new(statue(gl, res, "suzanne.obj")));
        {
            let mut s = statue3.borrow_mut();
            s.pos = Vector3::new(400.0, 0.9, 9.0);
            s.scale = Vector3::splat(1.2);
            s.euler.y = GH_PI;
        }
        objs.push(statue3);

        //Portals
        let portal1 = Rc::new(RefCell::new(Portal::new(gl, res)));
        pillar_room_set_portal(&pillar_room1.borrow(), &mut portal1.borrow_mut());
        portals.push(portal1.clone());

        let portal2 = Rc::new(RefCell::new(Portal::new(gl, res)));
        pillar_room_set_portal(&pillar_room2.borrow(), &mut portal2.borrow_mut());
        portals.push(portal2.clone());

        let portal3 = Rc::new(RefCell::new(Portal::new(gl, res)));
        pillar_room_set_portal(&pillar_room3.borrow(), &mut portal3.borrow_mut());
        portals.push(portal3.clone());

        connect_warps(&portal1, Side::Front, &portal2, Side::Back);
        connect_warps(&portal2, Side::Front, &portal3, Side::Back);
        connect_warps(&portal3, Side::Front, &portal1, Side::Back);

        player
            .base
            .set_position(Vector3::new(0.0, GH_PLAYER_HEIGHT, 3.0));
    }
}
