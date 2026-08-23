//! Port of Level2.h / Level2.cpp.

use std::cell::RefCell;
use std::rc::Rc;

use crate::game_header::GH_PLAYER_HEIGHT;
use crate::player::Player;
use crate::portal::{connect, connect_warps, Portal, Side};
use crate::props::{house, house_set_door1, house_set_door2, house_set_door3, house_set_door4};
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec, Scene};
use crate::vector::Vector3;

pub struct Level2 {
    num_rooms: i32,
}

impl Level2 {
    // Level2(int rooms) : num_rooms(rooms) {}   (Level2.h:6)
    pub fn new(rooms: i32) -> Level2 {
        Level2 { num_rooms: rooms }
    }
}

impl Scene for Level2 {
    fn load(
        &self,
        gl: &Rc<glow::Context>,
        res: &Resources,
        objs: &mut PObjectVec,
        portals: &mut PPortalVec,
        player: &mut Player,
    ) {
        let house1 = Rc::new(RefCell::new(house(gl, res, "three_room.bmp")));
        house1.borrow_mut().pos = Vector3::new(0.0, 0.0, -20.0);
        objs.push(house1.clone());

        // PORT: `std::shared_ptr<House> house2;` is default-constructed null and only reset when
        // num_rooms > 4 -> Option (was: Level2.cpp:9-14).
        let mut house2 = None;
        if self.num_rooms > 4 {
            let h = Rc::new(RefCell::new(house(gl, res, "three_room2.bmp")));
            h.borrow_mut().pos = Vector3::new(200.0, 0.0, -20.0);
            objs.push(h.clone());
            house2 = Some(h);
        }

        if self.num_rooms == 1 {
            let portal1 = Rc::new(RefCell::new(Portal::new(res)));
            house_set_door1(&house1.borrow(), &mut portal1.borrow_mut());
            portals.push(portal1.clone());

            let portal2 = Rc::new(RefCell::new(Portal::new(res)));
            house_set_door4(&house1.borrow(), &mut portal2.borrow_mut());
            portals.push(portal2.clone());

            connect(&portal1, &portal2);
        } else if self.num_rooms == 2 {
            let portal1 = Rc::new(RefCell::new(Portal::new(res)));
            house_set_door2(&house1.borrow(), &mut portal1.borrow_mut());
            portals.push(portal1.clone());

            let portal2 = Rc::new(RefCell::new(Portal::new(res)));
            house_set_door4(&house1.borrow(), &mut portal2.borrow_mut());
            portals.push(portal2.clone());

            connect(&portal1, &portal2);
        } else if self.num_rooms == 3 {
            let portal1 = Rc::new(RefCell::new(Portal::new(res)));
            house_set_door3(&house1.borrow(), &mut portal1.borrow_mut());
            portals.push(portal1.clone());

            let portal2 = Rc::new(RefCell::new(Portal::new(res)));
            house_set_door4(&house1.borrow(), &mut portal2.borrow_mut());
            portals.push(portal2.clone());

            connect(&portal1, &portal2);
        } else if self.num_rooms == 4 {
            // (empty in the original, Level2.cpp:46-47)
        } else if self.num_rooms == 5 {
            // PORT: `house2->SetDoorN(..)` dereferences a shared_ptr that is only non-null in
            // this branch; the Option is unwrapped for the same reason (Level2.cpp:53/57).
            let house2 = house2.as_ref().expect("house2 is null");

            let portal1 = Rc::new(RefCell::new(Portal::new(res)));
            house_set_door4(&house1.borrow(), &mut portal1.borrow_mut());
            portals.push(portal1.clone());

            let portal2 = Rc::new(RefCell::new(Portal::new(res)));
            house_set_door2(&house2.borrow(), &mut portal2.borrow_mut());
            portals.push(portal2.clone());

            let portal3 = Rc::new(RefCell::new(Portal::new(res)));
            house_set_door1(&house2.borrow(), &mut portal3.borrow_mut());
            portals.push(portal3.clone());

            // PORT: `Portal::Connect(Warp&, Warp&)` -> connect_warps(portal, side, portal, side)
            // (was: Portal::Connect(portal1->front, portal2->back), Level2.cpp:60-62).
            connect_warps(&portal1, Side::Front, &portal2, Side::Back);
            connect_warps(&portal2, Side::Front, &portal3, Side::Back);
            connect_warps(&portal3, Side::Front, &portal1, Side::Back);
        } else if self.num_rooms == 6 {
            let house2 = house2.as_ref().expect("house2 is null");

            let portal1 = Rc::new(RefCell::new(Portal::new(res)));
            house_set_door4(&house1.borrow(), &mut portal1.borrow_mut());
            portals.push(portal1.clone());

            let portal2 = Rc::new(RefCell::new(Portal::new(res)));
            house_set_door3(&house2.borrow(), &mut portal2.borrow_mut());
            portals.push(portal2.clone());

            let portal3 = Rc::new(RefCell::new(Portal::new(res)));
            house_set_door1(&house2.borrow(), &mut portal3.borrow_mut());
            portals.push(portal3.clone());

            connect_warps(&portal1, Side::Front, &portal2, Side::Back);
            connect_warps(&portal2, Side::Front, &portal3, Side::Back);
            connect_warps(&portal3, Side::Front, &portal1, Side::Back);
        }

        player
            .base
            .set_position(Vector3::new(3.0, GH_PLAYER_HEIGHT, 3.0));
    }
}
