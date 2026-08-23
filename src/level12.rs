//! EXT: Scene `[` -- "The Painted Cube". Not part of the C++ port.
//!
//! A plain room with a decal of a cube stuck flat on the far wall. It is a sticker, and it looks
//! like one -- an ordinary picture, undistorted, hanging on a wall.
//!
//! Stand on the marked spot and the picture stops reading as a picture. It reads as a cube
//! hanging in the middle of the room. Aim at it -- the crosshair goes gold when you have it --
//! and press **E**. What comes away in your hand is a real cube.
//!
//! # What actually creates the illusion
//!
//! Not distortion. The decal is a plain flat image; the sight-line is only about 7 degrees off
//! perpendicular to the wall, so the keystone is under 1% and invisible.
//!
//! It is **size**. The decal subtends exactly the angle a real cube would subtend at the place it
//! depicts. Your eye has no way to tell a small picture far away from a large object nearby, so at
//! the station point the two readings are equally valid -- and the cube reading wins, because the
//! image carries the shading and checkerboard perspective of a solid object.
//!
//! Step off the spot and the angle no longer matches anything, so it collapses back into a
//! sticker on a wall.
//!
//! # Baking
//!
//! `tools/bake_cube.py` walks every texel of the decal, takes its world position, casts the ray
//! from the station point through it, and intersects the cube. The shading term is
//! `dot(n, LIGHT) * 0.5 + 0.5`, lifted from `Shaders/texture.frag`, so the paint is lit exactly
//! like the solid cube it depicts.
//!
//! The baker asserts the projection clears the floor -- at eye height the lower half of the image
//! would sink below the floor line and be clipped, which is why the cube hangs slightly high.
//!
//! # Taking it
//!
//! The real cube sits where the paint implies but has **no mesh**, so it is invisible while
//! remaining pickable: `Object::Draw` needs both a mesh and a shader (Object.cpp:21), and the
//! `Physical` hit sphere `ext::grab` picks against is unaffected by either.
//!
//! Its `p_scale` gates pickability -- `HIDDEN` away from the spot, so the cube cannot be grabbed
//! out of thin air from a wrong angle. Grabbing is the only thing that changes the room: the
//! decal retires, the cube is given its mesh, and you walk off holding it.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ext::grab::Grabbable;
use crate::ext::room::RoomLogic;
use crate::game_header::{GH_PI, GH_PLAYER_HEIGHT};
use crate::object::{Object, ObjectT};
use crate::player::Player;
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec, Scene};
use crate::vector::Vector3;

pub struct Level12;

// ── These MUST stay in step with the header of tools/bake_cube.py ────────────────────────────
/// The marked spot: where the decal's angular size matches a real cube's.
const STATION: Vector3 = Vector3 { x: 0.0, y: GH_PLAYER_HEIGHT, z: 6.0 };
/// Where the cube appears to hang. Slightly above eye height so its painted image clears the
/// floor line -- centred at eye height, the lower half would be occluded by the floor.
const CUBE_POS: Vector3 = Vector3 { x: 0.0, y: 2.3, z: 0.0 };
const CUBE_HALF: f32 = 0.7;
const CUBE_RY: f32 = GH_PI / 4.0;
const CUBE_RX: f32 = 0.615_479_71; // atan(1/sqrt(2))
/// Decal plane, a hair proud of the z = -5 wall so it cannot z-fight with it.
const WALL_Z: f32 = -4.98;
/// Decal centre height: where the eye->cube sight-line meets the wall.
const STICKER_Y: f32 = 2.964;
const STICKER_HALF: f32 = 2.5;

/// Room floor centre and half-extents. Interior: x[-9, 9], y[0, 6], z[-5, 15].
const ROOM_CENTRE: Vector3 = Vector3 { x: 0.0, y: 0.0, z: 5.0 };
const ROOM_HALF: Vector3 = Vector3 { x: 9.0, y: 6.0, z: 10.0 };

/// Enter/exit radii for "standing on the spot". Hysteresis, so it cannot chatter on the boundary.
const SOLVE_ENTER: f32 = 1.5;
const SOLVE_EXIT: f32 = 2.2;
/// `p_scale` that makes the cube unpickable. Never zero -- `world_to_local` divides by it
/// (Object.cpp:42), and the pick radius scales with it.
const HIDDEN: f32 = 0.002;
/// Pick radius at `p_scale == 1`. Comfortably over the cube's body diagonal
/// (0.7 * sqrt(3) = 1.21) so anywhere on the painted cube counts as aiming at it.
const CUBE_PICK_RADIUS: f32 = 1.5;

impl Scene for Level12 {
    fn load(
        &self,
        gl: &Rc<glow::Context>,
        res: &Resources,
        objs: &mut PObjectVec,
        _portals: &mut PPortalVec,
        player: &mut Player,
    ) {
        // ── An ordinary room ──────────────────────────────────────────────────────────────
        // Non-uniform scale is safe: every wall is axis-aligned, so the collider rectangles stay
        // rectangular with orthogonal half-axes, which is what Collider::Collide assumes
        // (Collider.cpp:23-45).
        let mut room = Object::new();
        room.mesh = Some(res.acquire_mesh("room.obj"));
        room.shader = Some(res.acquire_shader("texture"));
        room.texture = Some(res.acquire_texture("checker_green.bmp", 1, 1));
        room.pos = ROOM_CENTRE;
        room.scale = ROOM_HALF;
        objs.push(Rc::new(RefCell::new(room)));

        // ── The decal ─────────────────────────────────────────────────────────────────────
        let sticker = {
            let mut p = Object::new();
            p.mesh = Some(res.acquire_mesh("double_quad.obj"));
            // `cutout`, not `texture`: the black surround must be discarded, and the image is
            // already lit so it must not be shaded a second time.
            p.shader = Some(res.acquire_shader("cutout"));
            p.texture = Some(res.acquire_texture("cube_sticker.bmp", 1, 1));
            p.pos = Vector3::new(0.0, STICKER_Y, WALL_Z);
            p.scale = Vector3::new(STICKER_HALF, STICKER_HALF, 1.0);
            // euler.y = pi turns the quad to face +Z, into the room.
            p.euler.y = GH_PI;
            Rc::new(RefCell::new(p))
        };
        objs.push(sticker.clone());

        // ── Floor markers for the station point ──────────────────────────────────────────
        for (dx, dz) in [(-0.6f32, 0.0f32), (0.6, 0.0), (0.0, -0.6), (0.0, 0.6)] {
            let mut m = crate::props::pillar(gl, res);
            m.pos = Vector3::new(STATION.x + dx, 0.0, STATION.z + dz);
            m.scale = Vector3::new(0.022, 0.004, 0.022);
            objs.push(Rc::new(RefCell::new(m)));
        }

        // ── The cube the decal is a picture of ───────────────────────────────────────────
        let cube_mesh = res.acquire_mesh("cube.obj");
        let cube = {
            let mut c = Grabbable::new(CUBE_PICK_RADIUS);
            {
                let o = c.obj_mut();
                // No mesh -> invisible, but everything else about the object stays live.
                o.mesh = None;
                o.shader = Some(res.acquire_shader("texture"));
                o.texture = Some(res.acquire_texture("checker_gray.bmp", 1, 1));
                o.pos = CUBE_POS;
                o.scale = Vector3::splat(CUBE_HALF);
                o.euler = Vector3::new(CUBE_RX, CUBE_RY, 0.0);
                o.p_scale = HIDDEN;
            }
            c.base.set_position(CUBE_POS);
            // Hangs where the paint says it is. `grab::release` restores gravity on let-go.
            c.base.gravity.set_zero();
            Rc::new(RefCell::new(c))
        };
        objs.push(cube.clone());

        // ── Room logic ───────────────────────────────────────────────────────────────────
        let mut taken = false;
        let mut announced = false;
        let logic = RoomLogic::new(move |ctx| {
            if taken {
                return;
            }

            // Carried off? `grab` repositions a held object every frame, so displacement from its
            // home is a reliable "it is mine now" signal needing no extra plumbing.
            if let Ok(c) = cube.try_borrow() {
                if (c.base().pos - CUBE_POS).mag_sq() > 0.02 {
                    taken = true;
                }
            }
            if taken {
                if let Ok(mut s) = sticker.try_borrow_mut() {
                    s.p_scale = HIDDEN; // the paint has become an object
                }
                if let Ok(mut c) = cube.try_borrow_mut() {
                    c.base_mut().mesh = Some(cube_mesh.clone());
                }
                log::debug!("[cube] taken -- the sticker was a cube all along");
                return;
            }

            // Standing on the spot? This gates pickability only. Nothing about the room changes
            // until the player actually reaches out.
            let mut d = ctx.player_pos - STATION;
            d.y = 0.0;
            let dist = d.mag();
            let (solved, was) = if let Ok(c) = cube.try_borrow() {
                let already = c.base().p_scale > 0.5;
                (
                    if already { dist < SOLVE_EXIT } else { dist < SOLVE_ENTER },
                    already,
                )
            } else {
                (false, false)
            };
            if let Ok(mut c) = cube.try_borrow_mut() {
                c.base_mut().p_scale = if solved { 1.0 } else { HIDDEN };
            }
            if solved && !was && !announced {
                announced = true;
                log::debug!("[cube] on the spot -- aim at the cube (crosshair turns gold) and press E");
            }
        });
        objs.push(Rc::new(RefCell::new(logic)) as Rc<RefCell<dyn ObjectT>>);

        // Spawn across the room and off to one side, so the decal reads as a wall sticker first.
        player.base.set_position(Vector3::new(5.0, GH_PLAYER_HEIGHT, 12.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ext::grab::GRAB_REACH;

    /// Everything must be inside the walls -- spawning the player outside them was a real bug in
    /// an earlier version of this scene.
    #[test]
    fn scene_fits_inside_the_room() {
        let lo = Vector3::new(ROOM_CENTRE.x - ROOM_HALF.x, 0.0, ROOM_CENTRE.z - ROOM_HALF.z);
        let hi = Vector3::new(
            ROOM_CENTRE.x + ROOM_HALF.x,
            ROOM_HALF.y,
            ROOM_CENTRE.z + ROOM_HALF.z,
        );
        let spawn = Vector3::new(5.0, GH_PLAYER_HEIGHT, 12.0);
        for (name, p) in [("spawn", spawn), ("station", STATION), ("cube", CUBE_POS)] {
            assert!(
                p.x > lo.x && p.x < hi.x && p.z > lo.z && p.z < hi.z && p.y >= 0.0 && p.y < hi.y,
                "{name} at ({}, {}, {}) is outside the room",
                p.x, p.y, p.z
            );
        }
    }

    /// The illusion is entirely a size match: the decal must subtend the same angle as the cube.
    /// If these drift apart, the sticker stops reading as an object.
    #[test]
    fn decal_subtends_the_same_angle_as_the_cube() {
        let cube_dist = (CUBE_POS - STATION).mag();
        let cube_angle = (CUBE_HALF * 3.0f32.sqrt()) / cube_dist;

        let decal_centre = Vector3::new(0.0, STICKER_Y, WALL_Z);
        let decal_dist = (decal_centre - STATION).mag();
        let decal_angle = STICKER_HALF / decal_dist;

        // The decal frame is deliberately a little larger than the cube, so the image has margin.
        assert!(
            decal_angle > cube_angle,
            "cube ({cube_angle}) overruns its decal frame ({decal_angle})"
        );
        assert!(
            decal_angle < cube_angle * 1.6,
            "decal frame ({decal_angle}) dwarfs the cube ({cube_angle}); wasted texture"
        );
    }

    /// The painted cube must sit clear of the floor line, or its lower half is occluded and the
    /// illusion breaks from the one place it is supposed to work.
    #[test]
    fn painted_cube_clears_the_floor() {
        let cube_dist = (CUBE_POS - STATION).mag();
        let decal_dist = (Vector3::new(0.0, STICKER_Y, WALL_Z) - STATION).mag();
        let proj_radius = (CUBE_HALF * 3.0f32.sqrt()) / cube_dist * decal_dist;
        assert!(
            STICKER_Y - proj_radius > 0.05,
            "painted cube reaches y = {}, below the floor",
            STICKER_Y - proj_radius
        );
    }

    /// The point of the scene: the cube must be takeable from the station point.
    #[test]
    fn cube_is_reachable_from_the_station() {
        let d = (CUBE_POS - STATION).mag() - CUBE_PICK_RADIUS;
        assert!(d < GRAB_REACH, "cube is {d} away, reach is {GRAB_REACH}");
    }

    /// Hysteresis must actually be hysteresis.
    #[test]
    fn solve_radii_are_hysteretic() {
        assert!(SOLVE_EXIT > SOLVE_ENTER);
    }
}
