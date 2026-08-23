//! EXT: a freestanding door with a swinging leaf. Not part of the C++ port.
//!
//! The door is the intro level's whole idea: a white door alone in a stormy meadow that opens
//! onto somewhere else entirely. The "somewhere else" costs nothing special -- it is a portal
//! filling the frame, and the ported renderer already draws what a portal leads to. The door
//! just has to be a convincing frame for it.
//!
//! # Behaviour
//!
//! * The leaf opens as the player approaches (with hysteresis so it never chatters at the
//!   threshold) and swings **toward** whichever side they stand on. That is deliberate: the
//!   portal quad sits in the frame plane, so a leaf on the far side would be hidden behind the
//!   portal image, and the one thing a door must visibly do is open. It cannot push the player:
//!   it opens before they reach it, and the leaf carries no collider -- only the frame posts do.
//! * While open, the door publishes a warm **light pool** (`view::set_glow`) that the grass
//!   shader spills onto the ground in front of it -- the reference image's glowing patch.
//! * [`set_hold_open`] overrides the proximity test and holds every door in the scene open;
//!   the title screen uses it to keep its backdrop's door standing open from a vantage no
//!   player would ever trigger it from.
//!
//! The door is one object: `draw` draws the frame and then the leaf, whose transform is rebuilt
//! every fixed step from the hinge and the swing angle. Scenes push the door and set up the
//! portal at `portal_transform()` themselves.

use crate::camera::Camera;
use crate::ext::gltf_model::{Anchor, Fit, Frame, GltfModel, Load, PartSpec};
use crate::object::{Object, ObjectT, RenderCtx, UpdateCtx};
use crate::resources::Resources;
use crate::shader::Shader;
use crate::vector::Vector3;
use std::cell::Cell;
use std::rc::Rc;

/// Two frames that are ONE door, seen from its two worlds, share a link: each publishes how
/// open it wants to be, and each opens if its partner does. That is what makes the portal
/// symmetric -- whichever side you approach from, the leaf on the other side is already open
/// when you arrive, and the view through is never blocked by a closed leaf.
#[derive(Clone)]
pub struct DoorLink {
    mine: Rc<Cell<f32>>,
    partner: Rc<Cell<f32>>,
}

impl DoorLink {
    /// A linked pair: `(a, b)` where `a`'s partner is `b` and vice versa.
    pub fn pair() -> (DoorLink, DoorLink) {
        let x = Rc::new(Cell::new(0.0));
        let y = Rc::new(Cell::new(0.0));
        (DoorLink { mine: x.clone(), partner: y.clone() }, DoorLink { mine: y, partner: x })
    }
}

thread_local! {
    /// EXT: hold every door in the loaded scene open, whatever the player's distance.
    ///
    /// The title screen uses the intro level as its backdrop (engine.rs `render_menu_frame`),
    /// and that shot is framed from further back than any door's opening radius -- while the
    /// whole point of it is the world seen THROUGH the door. Ambient rather than a field for
    /// the same reason `ext::view`'s uniforms are: it applies to every door in the scene at
    /// once, and nothing that reaches a door individually is on the path that decides it.
    static HOLD_OPEN: Cell<bool> = const { Cell::new(false) };
}

/// Set the hold-open flag. Cleared on every scene load, so it cannot leak between scenes.
pub fn set_hold_open(on: bool) {
    HOLD_OPEN.with(|h| h.set(on));
}

fn hold_open() -> bool {
    HOLD_OPEN.with(|h| h.get())
}

/// Player distance at which the leaf starts opening / has closed again. `CLOSE_DIST` is public
/// because it is the radius a camera has to stand outside of for `set_hold_open` to be doing
/// any work -- `ext::meadow::title_view` is placed against it and tests that it still is.
const OPEN_DIST: f32 = 4.5;
pub const CLOSE_DIST: f32 = 5.5;
/// Fully open angle, and the per-step ease factor at 500 Hz (~0.45 s to open).
const OPEN_ANGLE: f32 = 1.9;
const EASE: f32 = 0.011;
/// How far the closed leaf sits proud of the frame's centre plane, so the two never land
/// coplanar. Small enough to still read as a shut door.
///
/// Zero, now that the loader fits the whole model from one origin: the real door already seats
/// its leaf 37 mm proud of the frame's reveal mid-plane, and that offset now survives the import
/// instead of being thrown away and guessed back. Kept as a named constant because the swing
/// axis genuinely is offset from the frame plane, and a future model might need a nudge.
const LEAF_SEAT: f32 = 0.0;

/// The source model, loaded at runtime with its own normals, tangents and material maps.
const MODEL: &str = "Meshes/Classic_Interior_Door.glb";

/// Texture side the door's maps are capped at. The door is 1.7 units tall (`HALF_H`) and stands
/// at conversational distance at most, so even 512 is more texel than it can show -- and
/// halving from 1024 quartered both the packing loop and the resident textures. The shipped GLB
/// is pre-shrunk to exactly this size by `tools/shrink_glb.py` (its 4096 sources were 79 MB),
/// so the loader resizes nothing; change this and re-run the tool together.
const MAP: u32 = 512;

/// Half-height of the opening, and so the door's whole size: the loader fits the leaf to
/// `2 * HALF_H` and scales the frame with it.
///
/// The player is [`GH_PLAYER_HEIGHT`] = 1.5 tall. At the 1.0 this started as, the door stood
/// two units against them -- a ratio of 1.33, where a real 2.03 m door against a 1.75 m person
/// is 1.16. That reads as a door built for someone taller than you, which on the title screen
/// is the first thing in frame. 0.85 puts the ratio at 1.13 and the opening at 1.7 units: still
/// a comfortable head of clearance, and the leaf finally looks like a door rather than a gate.
pub const HALF_H: f32 = 0.85;

/// The model's leaf half-width per unit of half-height, measured from the file. A property OF
/// THE MODEL, not a target it is stretched to -- stretching would distort real joinery -- which
/// is why the door's size is set by `HALF_H` alone and this follows. `Door::new` asserts the
/// loaded model still agrees, so a swapped model fails loudly at load rather than quietly
/// leaving the leaf too narrow for its frame.
const LEAF_ASPECT: f32 = 0.468;
pub const HALF_W: f32 = LEAF_ASPECT * HALF_H;

/// How deep the frame's collision posts stand either side of the opening. Scales with the door,
/// and `Meshes/intro_door_collide.obj` is written to match; `collider_posts_match_half_w` below
/// is what keeps the two in step.
#[allow(dead_code)] // Documents the asset; only `collider_posts_match_half_w` reads it.
pub const POST_DEPTH: f32 = 0.09 * HALF_H;

pub struct Door {
    /// The frame. Its transform is the door's transform.
    base: Object,
    leaf: Object,
    /// Frame and leaf geometry, straight from the GLB. Shared between every door in the scene.
    model: Rc<GltfModel>,
    shader: Rc<Shader>,
    angle: f32,
    swing_sign: f32,
    /// Whether this frame publishes the ground glow (only the stormy side's should).
    glows: bool,
    link: Option<DoorLink>,
}

impl Door {
    pub fn new(
        gl: &Rc<glow::Context>,
        res: &Resources,
        pos: Vector3,
        yaw: f32,
        glows: bool,
        link: Option<DoorLink>,
    ) -> Door {
        // The leaf is gathered from MatrixTransform_37, NOT from the "Door" node above it: that
        // node carries a 55-degree Y rotation modelling the door hanging ajar in the original
        // scene. Walking from the natural root would import the leaf pre-rotated and every
        // hinge calculation below would be nonsense. The handle is a sibling sub-tree and rides
        // with the leaf, so both land in one part and swing together.
        let model = GltfModel::acquire(
            gl,
            &Load {
                path: MODEL,
                parts: &[
                    PartSpec {
                        name: "leaf",
                        roots: &["MatrixTransform_37", "Door4_Handle"],
                        skip: &[],
                        // ...but keep that node's TRANSLATION, or the leaf loses its position
                        // in the assembly and the frame has to be re-aligned to it by hand.
                        frame: Frame::Translated("Door"),
                        // Origin ON THE HINGE: Object rotates about its own origin, so the
                        // leaf's local x=0 must be its hinge edge or it would orbit instead of
                        // swing.
                        anchor: Anchor::Hinge,
                    },
                    PartSpec {
                        name: "frame",
                        roots: &["Door4_Frame"],
                        skip: &[],
                        frame: Frame::Local,
                        anchor: Anchor::Around("leaf"),
                    },
                ],
                fit: Fit::Part { part: "leaf", height: HALF_H * 2.0 },
                max_map: MAP,
                translucent: &[],
            },
        );

        let leaf_b = model.bounds("leaf");
        debug_assert!(
            (leaf_b[1] * 0.5 - HALF_W).abs() < 2e-3,
            "model leaf fits {:.4} wide at HALF_H {HALF_H}; LEAF_ASPECT implies {:.4}",
            leaf_b[1],
            HALF_W * 2.0
        );

        let mut base = Object::new();
        // Collision proxy only -- never drawn. glTF carries no colliders and the engine reads
        // them off base().mesh (engine.rs:676-691), so the frame posts live in a tiny OBJ while
        // the visible geometry comes from the model.
        base.mesh = Some(res.acquire_mesh("intro_door_collide.obj"));
        base.pos = pos;
        base.euler.y = yaw;

        // The leaf has no colliders by design: it opens before the player reaches it.
        let leaf = Object::new();

        let mut d = Door {
            base,
            leaf,
            model,
            shader: res.acquire_shader("gltfpbr"),
            angle: 0.0,
            swing_sign: 1.0,
            glows,
            link,
        };
        d.place_leaf();
        d
    }

    /// World position of the hinge: the left post of the opening, at floor level.
    fn hinge_world(&self) -> Vector3 {
        self.base.local_to_world().mul_point(Vector3::new(-HALF_W, 0.0, 0.0))
    }

    fn place_leaf(&mut self) {
        // The leaf swings about the hinge, seated by whatever LEAF_SEAT asks for on top of the
        // offset the model itself carries (see Anchor::Around). Getting that offset from the
        // file is what stopped the leaf and the frame landing coplanar and z-fighting -- which
        // showed as stripes across the closed door.
        let out =
            self.base.local_to_world().mul_direction(Vector3::new(0.0, 0.0, 1.0)).normalized_safe();
        self.leaf.pos = self.hinge_world() + out * LEAF_SEAT;
        self.leaf.euler = Vector3::new(0.0, self.base.euler.y + self.angle * self.swing_sign, 0.0);
    }

    /// Where a scene should put this door's portal. See [`portal_placement`].
    pub fn portal_transform(&self) -> (Vector3, Vector3, Vector3) {
        portal_placement(self.base.pos, self.base.euler.y)
    }

    pub fn openness(&self) -> f32 {
        (self.angle / OPEN_ANGLE).clamp(0.0, 1.0)
    }
}

impl ObjectT for Door {
    fn base(&self) -> &Object {
        &self.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base
    }

    fn update(&mut self, ctx: &UpdateCtx) {
        let hinge = self.hinge_world();
        let mut d = ctx.player_pos - hinge;
        d.y = 0.0;
        let dist = d.mag();

        let opening = self.angle > 0.01;
        let near = if opening { dist < CLOSE_DIST } else { dist < OPEN_DIST };
        let held = hold_open();
        // Publish my own wish, then honour my partner's: one door, two frames.
        let partner_wants = if let Some(link) = &self.link {
            link.mine.set(if near || held { 1.0 } else { 0.0 });
            link.partner.get() > 0.5
        } else {
            false
        };
        let want_open = near || held || partner_wants;
        if !opening && want_open {
            // Swing toward the player when they are the one approaching: a positive angle
            // about the hinge carries the leaf's free edge to local -z (Matrix4::rot_y maps +x
            // to (cos a, 0, -sin a)), so a player on the -z side gets +1. A frame opened by its
            // partner swings out of its own frame's +z side by convention.
            let p_local = self.base.world_to_local().mul_point(ctx.player_pos);
            self.swing_sign = if near && p_local.z < 0.0 { 1.0 } else { -1.0 };
        }
        let target = if want_open { OPEN_ANGLE } else { 0.0 };
        self.angle += (target - self.angle) * EASE;
        self.place_leaf();

        if self.glows {
            // Light pool just in front of the opening, strength following the swing. At the
            // door's FOOT height, not y = 0: the pool's radius in the grass shaders is 3.5, and
            // the intro door stands on an 8-unit knoll, so a pool published at ground zero
            // never reached the grass it was meant to light.
            let (centre, _, _) = self.portal_transform();
            let front = self.base.local_to_world().mul_direction(Vector3::new(0.0, 0.0, 1.0));
            let sign = if self.swing_sign < 0.0 { 1.0 } else { -1.0 }; // toward the player
            crate::ext::view::set_glow(
                Vector3::new(centre.x, self.base.pos.y, centre.z) + front * (sign * 1.2),
                self.openness(),
            );
        }
    }

    fn draw(&self, ctx: &RenderCtx, cam: &Camera, _fbo: Option<glow::Framebuffer>) {
        // Frustum cull the whole door at once. The frame stands on its origin, HALF_H * 2 tall
        // and a little over HALF_W either side; the open leaf reaches a further 2 * HALF_W out
        // from the hinge. A sphere of this radius about the foot covers every pose.
        if !ctx.frustum.sphere(self.base.pos, HALF_H * 2.0 + HALF_W * 2.0) {
            return;
        }
        // Not Object::draw_impl: a glTF material needs two samplers and this Object's mesh is a
        // collider proxy with no faces. draw_part sets the same matrices and the same EXT
        // uniforms draw_impl does, so the door still grades with the weather like everything else.
        self.model.draw_part("frame", &self.base, &self.shader, cam, ctx);
        self.model.draw_part("leaf", &self.leaf, &self.shader, cam, ctx);
    }
}

/// The portal that fills a door standing at `pos` with yaw `yaw`: `(pos, euler, scale)` for
/// the portal's `Object` -- the centre of the opening, the same yaw, and the scale the ported
/// portal quad needs to fill a 2*HALF_W x 2*HALF_H opening. A free function of the placement
/// alone, with no door or GL behind it, so a test can reproduce a scene's portal pair exactly.
pub fn portal_placement(pos: Vector3, yaw: f32) -> (Vector3, Vector3, Vector3) {
    let mut frame = Object::new();
    frame.pos = pos;
    frame.euler.y = yaw;
    let centre = frame.local_to_world().mul_point(Vector3::new(0.0, HALF_H, 0.0));
    (centre, frame.euler, Vector3::new(HALF_W, HALF_H * 0.999, 1.0))
}

/// Yaw that makes a door's local +Z face world direction `dir` (horizontal).
pub fn yaw_facing(dir: Vector3) -> f32 {
    // Matrix4::rot_y maps local +Z to (sin a, 0, cos a) (Vector.h:195-200).
    dir.x.atan2(dir.z)
}

#[cfg(test)]
mod tests {
    /// The frame's collision posts are a hand-written OBJ (glTF carries no colliders), so
    /// nothing recomputes them when HALF_W changes. If they drift, the player either walks
    /// through the frame or bumps into thin air beside it.
    #[test]
    fn collider_posts_match_half_w() {
        let src =
            std::fs::read_to_string(crate::app::assets::path("Meshes/intro_door_collide.obj"))
                .expect("proxy mesh");
        let xs: Vec<f32> = src
            .lines()
            .filter(|l| l.starts_with("v "))
            .map(|l| l.split_whitespace().nth(1).unwrap().parse().unwrap())
            .collect();
        assert_eq!(xs.len(), 6, "expected two 3-corner rectangles");
        // Each post is given as three corners of a rectangle whose fourth is implied
        // (Collider.cpp:7-21): two corners on its outer edge, one on the edge of the opening.
        // Across both posts that is three x at HALF_W and three at HALF_W + 0.09, mirrored.
        let at = |t: f32| xs.iter().filter(|x| (x.abs() - t).abs() < 2e-3).count();
        assert_eq!(at(super::HALF_W), 3, "posts do not meet the opening at HALF_W: {xs:?}");
        assert_eq!(
            at(super::HALF_W + super::POST_DEPTH),
            3,
            "posts are not POST_DEPTH deep: {xs:?}"
        );
        assert!(xs.iter().any(|x| *x < 0.0) && xs.iter().any(|x| *x > 0.0), "one post only");

        // ...and they must be as tall as the opening, or the player can walk over a post that
        // stops short or bump into one that overshoots the frame.
        let ys: Vec<f32> = src
            .lines()
            .filter(|l| l.starts_with("v "))
            .map(|l| l.split_whitespace().nth(2).unwrap().parse().unwrap())
            .collect();
        let top = ys.iter().cloned().fold(f32::MIN, f32::max);
        assert!(
            (top - super::HALF_H * 2.0).abs() < 2e-3,
            "posts are {top} tall; the opening is {}",
            super::HALF_H * 2.0
        );
    }

    use super::*;
    use crate::game_header::GH_PI;

    #[test]
    fn yaw_facing_axes() {
        assert!((yaw_facing(Vector3::new(0.0, 0.0, 1.0))).abs() < 1e-6);
        assert!((yaw_facing(Vector3::new(1.0, 0.0, 0.0)) - GH_PI / 2.0).abs() < 1e-5);
    }

    #[test]
    fn hysteresis_is_real() {
        const { assert!(CLOSE_DIST > OPEN_DIST) }
    }
}
