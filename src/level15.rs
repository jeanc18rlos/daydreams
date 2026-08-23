//! EXT: Scene `;` -- "Intro". Not part of the C++ port. NEW GAME started here until the
//! Backrooms (`level16`) took its meadow and its door; it is reached from the level list now.
//!
//! A stormy meadow, empty except for a white door. Walk up and it swings open onto a sunset
//! sea -- a different world, with its own weather -- and you can step through. The meadow,
//! the door and the title screen's view of them are `ext/meadow.rs`, shared with the
//! Backrooms; this file is the sea.
//!
//! Returning is symmetrical: a second door on the far side leads back.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ext::bounds::bounds_box;
use crate::ext::meadow::{door_with_portal, load_meadow, FAR};
use crate::object::{Object, ObjectT};
use crate::player::Player;
use crate::portal::connect;
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec, Scene};
use crate::vector::Vector3;

pub struct Level15;

impl Scene for Level15 {
    fn load(
        &self,
        gl: &Rc<glow::Context>,
        res: &Resources,
        objs: &mut PObjectVec,
        portals: &mut PPortalVec,
        player: &mut Player,
    ) {
        let meadow = load_meadow(gl, res, objs, portals, player);

        // ── The sea world: nothing of the meadow here, only water to every horizon ─────────
        // The sea quad carries the ground collider, set just below the surface so the player
        // wades rather than walks on water. No grass, no land: through the door from this
        // side you see the meadow, and around the door you see only sea.
        let mut sea = Object::new();
        sea.mesh = Some(res.acquire_mesh("ground.obj"));
        sea.shader = Some(res.acquire_shader("sea"));
        sea.texture = Some(res.acquire_texture("grass_noise.bmp", 1, 1));
        sea.pos = FAR + Vector3::new(0.0, -0.28, 0.0);
        sea.scale = Vector3::new(400.0, 1.0, 400.0);
        objs.push(Rc::new(RefCell::new(sea)));

        objs.push(Rc::new(RefCell::new(bounds_box(
            res,
            FAR + Vector3::new(0.0, -3.0, 0.0),
            Vector3::new(120.0, 40.0, 120.0),
        ))) as Rc<RefCell<dyn ObjectT>>);

        // The same door, from the sea side. Its frame sits on the water; it leads back.
        let there = door_with_portal(
            gl,
            res,
            FAR + Vector3::new(0.0, -0.28, -6.0),
            Vector3::new(0.0, 0.0, 1.0),
            false,
            meadow.link_there,
            objs,
            portals,
        );

        // Front of the meadow door <-> back of the sea door: walk in here, walk out there.
        connect(&meadow.here, &there);
    }
}
