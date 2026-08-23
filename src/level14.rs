//! EXT: Scene `\` -- "Meadow". Not part of the C++ port.
//!
//! Rolling hills under cumulus -- the outdoor counterpart to the rooms, and the showcase for
//! the grass and cloud work. Everything here is built for **cheap**:
//!
//! * the terrain is one mesh (`Meshes/meadow.obj`) with smooth normals baked into its `vt`
//!   channel and a gradient-tilted collider shell, both generated offline by
//!   `tools/gen_meadow.py`;
//! * the grass is two texture taps of an offline-baked noise atlas plus three lighting tricks
//!   (`Shaders/grass.frag`) -- no per-pixel noise;
//! * the clouds are a panorama baked once and re-baked every few seconds
//!   (`src/ext/skybake.rs`), so the sky is one texture fetch per pixel even inside portals.
//!
//! A few grabbables sit on the spawn plateau so the perspective mechanic has something to
//! do against a horizon -- release an object toward a distant hill and it lands enormous.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ext::bounds::bounds_box;
use crate::ext::grab::Grabbable;
use crate::game_header::GH_PLAYER_HEIGHT;
use crate::object::{Object, ObjectT};
use crate::player::Player;
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec, Scene};
use crate::vector::Vector3;

pub struct Level14;

/// Terrain extent (tools/gen_meadow.py SIZE) and the spawn plateau height it reports.
const HALF: f32 = 110.0;
const PLATEAU_Y: f32 = 6.75;

fn grabbable(
    res: &Resources,
    mesh: &str,
    texture: &str,
    pos: Vector3,
    scale: f32,
) -> Rc<RefCell<dyn ObjectT>> {
    let m = res.acquire_mesh(mesh);
    // Physics radius from the mesh, so the prop rests on the grass instead of sinking into it.
    let mut g = Grabbable::new(m.bound_radius * scale * 0.8);
    {
        let o = g.obj_mut();
        o.mesh = Some(m);
        o.shader = Some(res.acquire_shader("texture"));
        o.texture = Some(res.acquire_texture(texture, 1, 1));
        o.scale = Vector3::splat(scale);
        o.pos = pos;
    }
    g.base.set_position(pos);
    Rc::new(RefCell::new(g))
}

impl Scene for Level14 {
    fn load(
        &self,
        _gl: &Rc<glow::Context>,
        res: &Resources,
        objs: &mut PObjectVec,
        _portals: &mut PPortalVec,
        player: &mut Player,
    ) {
        let mut terrain = Object::new();
        terrain.mesh = Some(res.acquire_mesh("meadow.obj"));
        terrain.shader = Some(res.acquire_shader("grass"));
        terrain.texture = Some(res.acquire_texture("grass_noise.bmp", 1, 1));
        objs.push(Rc::new(RefCell::new(terrain)));

        // Props on the plateau, at ground height plus a little so they settle.
        let y = PLATEAU_Y + 0.6;
        objs.push(grabbable(res, "cube.obj", "checker_gray.bmp", Vector3::new(2.5, y, -3.0), 0.45));
        objs.push(grabbable(
            res,
            "suzanne.obj",
            "gold.bmp",
            Vector3::new(-2.5, y + 0.3, -3.5),
            0.5,
        ));
        objs.push(grabbable(res, "teapot.obj", "white.bmp", Vector3::new(0.0, y, -5.0), 0.25));

        // Walls at the edge of the world, well below the hills' line of sight.
        objs.push(Rc::new(RefCell::new(bounds_box(
            res,
            Vector3::new(0.0, -5.0, 0.0),
            Vector3::new(HALF, 60.0, HALF),
        ))) as Rc<RefCell<dyn ObjectT>>);

        player.base.set_position(Vector3::new(0.0, PLATEAU_Y + GH_PLAYER_HEIGHT + 0.2, 0.0));
    }
}
