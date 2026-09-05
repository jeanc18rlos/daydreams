//! EXT: Scene `;` -- "Intro". Not part of the C++ port. NEW GAME started here until the
//! Backrooms (`level16`) took its meadow and its door; it is reached from the level list now,
//! and it is what the **title screen** stands in (`ext::scenes::TITLE`) -- so this scene is
//! also the game's cover, and is lit like one.
//!
//! A stormy meadow, empty except for a white door. Walk up and it swings open onto a sunset
//! sea -- a different world, with its own weather -- and you can step through. The meadow,
//! the door and the title screen's view of them are `ext/meadow.rs`, shared with the
//! Backrooms; this file is the sea.
//!
//! # The contrast is the point
//!
//! The near side stays **overcast**. It is dusk rather than midday storm ([`view::MOOD_DUSK`]) --
//! violet-blue overhead with the cloud bases catching the last of the light near the horizon --
//! but it is still cloud, and the ground under it is still lit coldly. Every warm thing in the
//! frame comes out of the doorway: the sea through it, the light it throws on the grass, the
//! dust in that light. So the door reads as a hole cut into a better evening rather than as a
//! window onto more of the same, which is the idea the title screen is built on.
//!
//! Returning is symmetrical: a second door on the far side leads back.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ext::audio;
use crate::ext::bounds::bounds_box;
use crate::ext::door::yaw_facing;
use crate::ext::doorlight::{Motes, Shafts};
use crate::ext::meadow::{door_with_portal, load_meadow, DOOR_POS, FAR};
use crate::ext::view;
use crate::object::{Object, ObjectT};
use crate::player::Player;
use crate::portal::connect;
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec, Scene};
use crate::vector::Vector3;

pub struct Level15;

/// EXT: what the footsteps land on (`ext/audio.rs`). Two worlds in one scene, so the surface is
/// resolved where the player's feet are rather than fixed for the load: the meadow this side of
/// `view::MOOD_SPLIT_X`, and beyond it the sea, which is waded through rather than walked on --
/// its ground collider sits just below the surface (below).
pub const SURFACE: audio::Surface = audio::Surface::SplitX {
    at: crate::ext::view::MOOD_SPLIT_X,
    west: &audio::Surface::Grass,
    east: &audio::Surface::Water,
};

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
        // Dusk overcast rather than the Backrooms' midday storm -- the same cloud, a different
        // hour. After `load_meadow`, which turns the split on (and every load resets this).
        view::set_near_mood(view::MOOD_DUSK);
        audio::set_surface(SURFACE);

        // ── What the door throws into the meadow ──────────────────────────────────────────
        // The light itself, and the dust in it (`ext/doorlight.rs`). Only this scene has them:
        // the Backrooms' door opens on a lit office, which spills a pool onto the grass but no
        // beam, and a sun on the other side of it would be a fluorescent tube. Pushed here
        // rather than inside `load_meadow` for exactly that reason.
        //
        // Before the sea below, and so before the portal, which is the order they have to be
        // drawn in: these are additive and write no depth (see the module docs).
        let door_yaw = yaw_facing(Vector3::new(0.0, 0.0, 1.0));
        objs.push(
            Rc::new(RefCell::new(Shafts::new(res, DOOR_POS, door_yaw))) as Rc<RefCell<dyn ObjectT>>
        );
        objs.push(Rc::new(RefCell::new(Motes::new(gl, res, DOOR_POS, door_yaw)))
            as Rc<RefCell<dyn ObjectT>>);

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
