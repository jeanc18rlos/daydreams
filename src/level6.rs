//! Port of Level6.h / Level6.cpp.

use std::cell::RefCell;
use std::rc::Rc;

use crate::game_header::GH_PLAYER_HEIGHT;
use crate::player::Player;
use crate::props::{floorplan, floorplan_add_portals};
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec, Scene};
use crate::vector::Vector3;

pub struct Level6;

impl Scene for Level6 {
    fn load(
        &self,
        gl: &Rc<glow::Context>,
        res: &Resources,
        objs: &mut PObjectVec,
        portals: &mut PPortalVec,
        player: &mut Player,
    ) {
        let floorplan1 = Rc::new(RefCell::new(floorplan(gl, res)));
        objs.push(floorplan1.clone());
        // PORT: `floorplan->AddPortals(portals)` -> a free function that also takes `gl`/`res`,
        // because it constructs six Portals and the C++ reaches the caches through globals
        // (was: Level6.cpp:7).
        floorplan_add_portals(&floorplan1.borrow(), gl, res, portals);

        player
            .base
            .set_position(Vector3::new(2.0, GH_PLAYER_HEIGHT, 2.0));
    }
}
