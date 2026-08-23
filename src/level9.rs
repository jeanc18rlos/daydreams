//! EXT: Scene 0 -- "Compound". Not part of the C++ port.
//!
//! The one thing here that neither NonEuclidean nor Superliminal does on its own.
//!
//! Superliminal resizes objects by perspective. CodeParade's Level5 resizes them by walking them
//! through a scaling portal. Both effects are the *same field* -- `p_scale` -- so they multiply:
//!
//!   1. pick up the teapot up close, so it is small;
//!   2. carry it through the shrinking tunnel, which multiplies its `p_scale` again
//!      (`Physical::try_portal`, Physical.cpp:62);
//!   3. release it down the long side, where perspective multiplies it a third time.
//!
//! No code was written to make this compose. It falls out of the fact that the port kept
//! `p_scale` as a genuine physical scale rather than a rendering trick -- it already feeds
//! gravity (Physical.cpp:21), walk speed (Player.cpp:105) and collision (Physical.cpp:31), so a
//! compounded object is heavier, slower and larger in every system at once.
//!
//! The layout is Level5's (Level5.cpp), which is the known-good scaling-tunnel geometry, with
//! grabbable props added on both sides of the size boundary.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ext::bounds::bounds_box;
use crate::ext::grab::Grabbable;
use crate::game_header::GH_PLAYER_HEIGHT;
use crate::object::ObjectT;
use crate::player::Player;
use crate::portal::{connect, Portal};
use crate::props::{ground, tunnel, tunnel_set_door1, tunnel_set_door2, TunnelType};
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec, Scene};
use crate::vector::Vector3;

pub struct Level9;

fn grabbable(
    res: &Resources,
    mesh: &str,
    texture: &str,
    pos: Vector3,
    scale: f32,
    radius: f32,
) -> Rc<RefCell<Grabbable>> {
    let mut g = Grabbable::new(radius);
    {
        let o = g.obj_mut();
        o.mesh = Some(res.acquire_mesh(mesh));
        o.shader = Some(res.acquire_shader("texture"));
        o.texture = Some(res.acquire_texture(texture, 1, 1));
        o.scale = Vector3::splat(scale);
        o.pos = pos;
    }
    g.base.set_position(pos);
    Rc::new(RefCell::new(g))
}

impl Scene for Level9 {
    fn load(
        &self,
        gl: &Rc<glow::Context>,
        res: &Resources,
        objs: &mut PObjectVec,
        portals: &mut PPortalVec,
        player: &mut Player,
    ) {
        // ── Level5's scaling pair, verbatim in layout (Level5.cpp:6-25). ────────────────────
        let tunnel1 = Rc::new(RefCell::new(tunnel(gl, res, TunnelType::Scale)));
        {
            let mut t = tunnel1.borrow_mut();
            t.base.pos = Vector3::new(-1.2, 0.0, 0.0);
            t.base.scale = Vector3::new(1.0, 1.0, 2.4);
        }

        let ground1 = Rc::new(RefCell::new(ground(gl, res, false)));
        ground1.borrow_mut().scale *= 1.2;
        objs.push(ground1);

        let tunnel2 = Rc::new(RefCell::new(tunnel(gl, res, TunnelType::Normal)));
        {
            let mut t = tunnel2.borrow_mut();
            t.base.pos = Vector3::new(201.2, 0.0, 0.0);
            t.base.scale = Vector3::new(1.0, 1.0, 2.4);
        }

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

        objs.push(tunnel1);
        objs.push(tunnel2);

        // ── Props on the large side, to carry through and shrink. ──────────────────────────
        objs.push(
            grabbable(res, "teapot.obj", "gold.bmp", Vector3::new(1.0, 0.9, 4.2), 0.30, 0.45)
                as Rc<RefCell<dyn ObjectT>>,
        );
        objs.push(
            grabbable(res, "suzanne.obj", "gold.bmp", Vector3::new(-1.0, 1.0, 4.6), 0.40, 0.5)
                as Rc<RefCell<dyn ObjectT>>,
        );

        // ── And one already on the small side, to carry back and grow. ─────────────────────
        objs.push(
            grabbable(res, "bunny.obj", "white.bmp", Vector3::new(200.0, 0.6, 3.4), 6.0, 0.35)
                as Rc<RefCell<dyn ObjectT>>,
        );

        player.base.set_position(Vector3::new(0.0, GH_PLAYER_HEIGHT, 5.0));

        // EXT: invisible bounds keeping players and thrown props inside the playable area,
        // one box per side of the scaling pair.
        for cx in [0.0f32, 200.0] {
            objs.push(Rc::new(RefCell::new(bounds_box(
                res,
                Vector3::new(cx, 0.0, 0.0),
                Vector3::new(12.0, 8.0, 12.0),
            ))) as Rc<RefCell<dyn ObjectT>>);
        }
    }
}
