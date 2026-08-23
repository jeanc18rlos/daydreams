//! EXT: Scene 8 -- "Perspective Gallery". Not part of the C++ port.
//!
//! The forced-perspective playground. Everything here exists to make one idea legible:
//! **where you release an object decides how big it really is.**
//!
//! Layout is deliberately depth-heavy -- a near wall you can put something down against, a long
//! run of ground, and pillars receding into the distance. The same object dropped at the near
//! wall and at the far pillar differs in real size by more than an order of magnitude, while
//! never changing size on screen until the moment you let go.
//!
//! Controls: look at an object and press **E** (or Cross / Square / R2 on a DualSense) to pick
//! it up; press again to release.

use crate::ext::bounds::bounds_box;
use crate::ext::grab::Grabbable;
use crate::game_header::{GH_PI, GH_PLAYER_HEIGHT};
use crate::object::ObjectT;
use crate::props;
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec, Scene};
use crate::vector::Vector3;
use std::cell::RefCell;
use std::rc::Rc;

pub struct Level7;

/// Build one grabbable prop.
///
/// `radius` is the object's bounding radius at `p_scale == 1`; it drives both picking and the
/// offset that keeps the object clear of whatever surface it is placed against.
fn grabbable(
    gl: &Rc<glow::Context>,
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
    let _ = gl;
    Rc::new(RefCell::new(g))
}

impl Scene for Level7 {
    fn load(
        &self,
        gl: &Rc<glow::Context>,
        res: &Resources,
        objs: &mut PObjectVec,
        portals: &mut PPortalVec,
        player: &mut crate::player::Player,
    ) {
        // ── Ground: wide, so there is somewhere to place something enormous. ────────────────
        // EXT: stretched along z so the far pillars (z=-70) and backstop (z=-90) stand on it:
        // ground.obj is a 2x2 quad at base scale (10,1,10), so z half becomes 62.5 centred at
        // -32.5 -> floor covers x in [-30,30], z in [-95,30].
        let mut ground = props::ground(gl, res, false);
        ground.scale *= Vector3::new(3.0, 3.0, 6.25);
        ground.pos.z = -32.5;
        objs.push(Rc::new(RefCell::new(ground)) as Rc<RefCell<dyn ObjectT>>);

        // ── A near enclosure, giving close walls to place small copies against. ─────────────
        let mut room = props::pillar_room(gl, res);
        room.pos = Vector3::new(0.0, 0.0, 0.0);
        room.scale = Vector3::splat(1.6);
        objs.push(Rc::new(RefCell::new(room)) as Rc<RefCell<dyn ObjectT>>);

        // ── Pillars receding into the distance: the depth cues that sell the illusion. ──────
        // Spaced ever wider so each is a visibly further release point than the last.
        for (i, z) in [-14.0f32, -26.0, -44.0, -70.0].iter().enumerate() {
            for side in [-1.0f32, 1.0] {
                let mut p = props::pillar(gl, res);
                // Widen the corridor with distance so the pillars stay visible past each other.
                p.pos = Vector3::new(side * (4.0 + i as f32 * 2.0), 0.0, *z);
                p.scale = Vector3::splat(0.1 + i as f32 * 0.02);
                objs.push(Rc::new(RefCell::new(p)) as Rc<RefCell<dyn ObjectT>>);
            }
        }

        // ── A far backstop, so a ray down the corridor always terminates somewhere. ─────────
        let mut backstop = props::pillar_room(gl, res);
        backstop.pos = Vector3::new(0.0, 0.0, -90.0);
        backstop.scale = Vector3::splat(6.0);
        backstop.euler.y = GH_PI;
        objs.push(Rc::new(RefCell::new(backstop)) as Rc<RefCell<dyn ObjectT>>);

        // ── The grabbables. Three different shapes so it is obvious which one you are holding.
        objs.push(grabbable(
            gl,
            res,
            "suzanne.obj",
            "gold.bmp",
            Vector3::new(-1.6, 1.0, 4.0),
            0.45,
            0.5,
        ) as Rc<RefCell<dyn ObjectT>>);

        objs.push(grabbable(
            gl,
            res,
            "teapot.obj",
            "gold.bmp",
            Vector3::new(0.0, 0.9, 4.6),
            0.30,
            0.5,
        ) as Rc<RefCell<dyn ObjectT>>);

        objs.push(grabbable(
            gl,
            res,
            "bunny.obj",
            "white.bmp",
            Vector3::new(1.6, 0.8, 4.0),
            9.0,
            0.5,
        ) as Rc<RefCell<dyn ObjectT>>);

        let _ = portals; // no portals in this scene -- see Level9 for grab + portal compounding

        player.base.set_position(Vector3::new(0.0, GH_PLAYER_HEIGHT, 8.0));

        // EXT: invisible bounds keeping players and thrown props inside the playable area.
        objs.push(Rc::new(RefCell::new(bounds_box(
            res,
            Vector3::new(0.0, 0.0, -32.5),
            Vector3::new(30.0, 6.0, 62.5),
        ))) as Rc<RefCell<dyn ObjectT>>);
    }
}
