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
//! * A linked pair can **vanish** ([`DoorLink::vanish`]): both frames stop drawing, stop
//!   colliding and stop glowing, for good. The Backrooms does this the moment the player has
//!   crossed into it -- the way in is one way -- and removes the portals through the engine
//!   at the same time (`ext::room::request_remove_portals`), since a door that is gone must
//!   not leave a hole in the air that still leads somewhere.
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
    /// Shared by both halves: once set, neither door exists any more. See the module docs.
    vanished: Rc<Cell<bool>>,
}

impl DoorLink {
    /// A linked pair: `(a, b)` where `a`'s partner is `b` and vice versa.
    pub fn pair() -> (DoorLink, DoorLink) {
        let x = Rc::new(Cell::new(0.0));
        let y = Rc::new(Cell::new(0.0));
        let vanished = Rc::new(Cell::new(false));
        (
            DoorLink { mine: x.clone(), partner: y.clone(), vanished: vanished.clone() },
            DoorLink { mine: y, partner: x, vanished },
        )
    }

    /// Make both doors of the pair disappear, permanently. A clone of either half will do:
    /// the flag is shared.
    pub fn vanish(&self) {
        self.vanished.set(true);
    }

    pub fn vanished(&self) -> bool {
        self.vanished.get()
    }
}

/// Whether a leaf should be open, given everything that can ask it to be: the player standing
/// on its threshold, the title's hold-open, a partner that is open. A vanished door never
/// gets this far (`Door::update` returns before asking): nothing brings back a door that is
/// gone.
fn wants_open(near: bool, held: bool, partner_wants: bool) -> bool {
    near || held || partner_wants
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
/// Zero, because the loader fits the whole model from one origin and whatever seat the file
/// gives its leaf survives the import instead of being thrown away and guessed back. On the
/// white PSX door that seat is nothing much -- the leaf sits centred in the casing's depth,
/// where the door this replaced hung 37 mm proud of its reveal -- so zero is right here for a
/// different reason than it was there. Kept as a named constant because the swing axis
/// genuinely can be offset from the frame plane, and the next model may need the nudge.
const LEAF_SEAT: f32 = 0.0;

/// The source model, loaded at runtime with its own normals, tangents and material maps.
///
/// Icevanilla's "Low-Poly PSX Style Essential Doors Pack" (CC-BY-4.0; licence beside it, credit
/// in `THIRD_PARTY.md` and on the Credits screen): seven door-and-frame pairs laid out along +x
/// on one shared colour map. This door is the white one, `Bathroom Door_002.001`, with the knob
/// that hangs under it and the frame beside it.
///
/// The shipped file is the pack with a **half-turn baked onto those two nodes**
/// (`tools/turn_white_door.py`). Every door in the pack is modelled with its knob at the low-x
/// end, so its hinge is the high-x edge -- and `Anchor::Hinge` puts a part's origin on its
/// low-x edge, which would have swung this leaf about its own doorknob. A half-turn about Y is
/// a rotation rather than a mirror, so winding survives and the loader carries the normals and
/// tangents through with it; and it is free to look at, because this door is symmetric front to
/// back. All it changes is which side the knob is on.
const MODEL: &str = "Meshes/psx_essential_doors_pack.glb";

/// Texture side the door's maps are capped at. The pack's one colour map is 1024x256 and this
/// is its long side, so the loader resamples nothing.
///
/// The 512 that stood here was right for a model with a map to itself: a door seen at
/// conversational distance cannot show more. This map is an atlas shared by all seven doors in
/// the pack, so ours has about a seventh of its width -- 146 texels at 1024, and 73 at 512,
/// which is where the panel edges start to crawl.
const MAP: u32 = 1024;

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
///
/// The white PSX leaf measures 0.756 x 1.806 in the file, so 0.756 / 1.806. The door it replaced
/// was 0.468, a hand's width broader for its height; `Meshes/intro_door_collide.obj` is
/// re-derived from the `HALF_W` this gives, and `collider_posts_match_half_w` holds the two
/// together.
const LEAF_ASPECT: f32 = 0.4186;
pub const HALF_W: f32 = LEAF_ASPECT * HALF_H;

/// How deep the frame's collision posts stand either side of the opening: the casing's own jamb
/// width, so the player meets the frame exactly where they can see it. Scales with the door, and
/// `Meshes/intro_door_collide.obj` is written to match; `collider_posts_match_half_w` below is
/// what keeps the two in step.
///
/// The white PSX frame is 0.897 wide against its 0.756 leaf, so each jamb is 0.0705 in the file
/// and 0.0664 once fitted -- 0.0781 of `HALF_H`. The 0.09 that stood here was the old door's
/// jamb; kept on this frame it would have left a centimetre of collision standing proud of the
/// casing on each side, which is a sliver of nothing to walk into.
#[allow(dead_code)] // Documents the asset; only `collider_posts_match_half_w` reads it.
pub const POST_DEPTH: f32 = 0.0781 * HALF_H;

/// The file and how its two parts are gathered. A function rather than a literal inside
/// `Door::new` so the tests can measure the very spec the door loads, without a GL context:
/// the `debug_assert` below needs one, so nothing in `cargo test` would otherwise notice a
/// model swapped for one of different proportions.
fn load_spec() -> Load<'static> {
    Load {
        path: MODEL,
        parts: &[
            PartSpec {
                name: "leaf",
                roots: &["Bathroom Door_002.001"],
                skip: &[],
                frame: Frame::Scene,
                // Origin ON THE HINGE: Object rotates about its own origin, so the
                // leaf's local x=0 must be its hinge edge or it would orbit instead of
                // swing. The baked half-turn (see MODEL) is what puts the hinge there.
                anchor: Anchor::Hinge,
            },
            PartSpec {
                name: "frame",
                roots: &["Bathroom Doorframe_001.001"],
                skip: &[],
                frame: Frame::Scene,
                anchor: Anchor::Around("leaf"),
            },
        ],
        fit: Fit::Part { part: "leaf", height: HALF_H * 2.0 },
        max_map: MAP,
        translucent: &[],
        metallic_override: &[],
        cut_boxes: &[],
    }
}

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
        // Both parts are walked from the scene root (`Frame::Scene`), which is what makes the
        // offset between them real: `Anchor::Around("leaf")` reads the frame's true seat around it
        // rebate straight out of the file instead of it being guessed back as a constant. The
        // knob hangs UNDER the leaf's node, so it is gathered with it and swings with it.
        //
        // The pack's own layout puts the seven doors side by side along x; naming two nodes
        // gathers only those two, and the other six are never walked.
        let model = GltfModel::acquire(gl, &load_spec());

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
        if self.link.as_ref().is_some_and(DoorLink::vanished) {
            // Gone: the posts stop colliding (the engine reads colliders off `base.mesh`), the
            // leaf sits shut so a door that came back by some bug would at least be closed,
            // and the glow it published is withdrawn. `draw` draws nothing.
            self.base.mesh = None;
            self.angle = 0.0;
            self.place_leaf();
            if self.glows {
                crate::ext::view::set_glow(Vector3::zero(), 0.0);
            }
            return;
        }
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
        let want_open = wants_open(near, held, partner_wants);
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
        if self.link.as_ref().is_some_and(DoorLink::vanished) {
            return;
        }
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
    /// The model measured without a GL context, which is the only way `cargo test` can see it:
    /// the guard inside `Door::new` is a `debug_assert` behind `GltfModel::acquire`, so a model
    /// swapped for one of different proportions passes the whole suite and fails only when
    /// somebody launches the intro in a debug build.
    ///
    /// What it holds: the leaf is fitted to the opening and hung by its low-x edge, so its
    /// bounds run 0..2*HALF_W from the hinge; the frame is anchored around it, so it straddles
    /// the origin and is wider than the leaf by a jamb either side. Get the hinge edge wrong --
    /// the pack this door comes from models every one of its doors knob-first -- and the leaf's
    /// bounds still look right while it swings about its own doorknob, which is why the knob's
    /// side is checked too.
    #[test]
    fn the_model_is_the_door_the_constants_describe() {
        use crate::ext::gltf_model::GltfModel;
        let spec = super::load_spec();
        let leaf = GltfModel::probe_bounds(&spec, "leaf");
        let frame = GltfModel::probe_bounds(&spec, "frame");
        // Fitted to the opening's height, to the millimetre.
        assert!((leaf[3] - leaf[2] - 2.0 * super::HALF_H).abs() < 1e-3, "leaf {leaf:?}");
        // Hung by its low-x edge: that is what `Anchor::Hinge` means and what `place_leaf`
        // assumes when it puts the leaf's origin on the hinge post.
        assert!(leaf[0].abs() < 1e-4, "the leaf's hinge edge is at x = {}, not 0", leaf[0]);
        assert!((leaf[1] - 2.0 * super::HALF_W).abs() < 2e-3, "leaf is {} wide", leaf[1]);
        // The frame straddles the opening and overhangs it by a jamb on each side.
        assert!(frame[0] < -super::HALF_W && frame[1] > super::HALF_W, "frame {frame:?}");
        let jamb = 0.5 * ((frame[1] - frame[0]) - (leaf[1] - leaf[0]));
        assert!(
            (jamb - super::POST_DEPTH).abs() < 5e-3,
            "the casing's jamb is {jamb}, but the collision posts stand {} deep",
            super::POST_DEPTH
        );
        assert!(frame[3] > 2.0 * super::HALF_H * 0.98, "the frame is shorter than the opening");

        // And the KNOB is at the far end from the hinge, which is the whole of the reason this
        // file ships turned (see `MODEL`). Bounds cannot say so -- a leaf hung backwards has
        // exactly the same box -- so the knob is picked out of the geometry: it is the only
        // part of the leaf that stands proud of the panel's thickness, so the vertices furthest
        // from the panel's mid-plane in z ARE the knob, and their x says which edge it is on.
        let (pos, idx) = GltfModel::probe_triangles(&spec, "leaf");
        let mid_z = 0.5 * (leaf[4] + leaf[5]);
        let mut proud: Vec<[f32; 3]> = idx.iter().map(|&i| pos[i as usize]).collect::<Vec<_>>();
        proud.sort_by(|a, b| (b[2] - mid_z).abs().total_cmp(&(a[2] - mid_z).abs()));
        proud.truncate(proud.len() / 10);
        let knob_x = proud.iter().map(|p| p[0]).sum::<f32>() / proud.len() as f32;
        assert!(
            knob_x > super::HALF_W,
            "the knob sits at x = {knob_x}, the hinge half of a leaf spanning 0..{} -- the \
             door is hung backwards and would swing about its own handle",
            2.0 * super::HALF_W
        );
    }

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

    /// Vanishing is shared by the pair and seen through any clone of either half.
    #[test]
    fn vanish_reaches_both_halves_of_the_link() {
        let (here, there) = DoorLink::pair();
        let kept = there.clone();
        assert!(!here.vanished() && !there.vanished());
        kept.vanish();
        assert!(here.vanished() && there.vanished());
    }

    /// Proximity, the title's hold-open and an open partner each open a door on their own.
    #[test]
    fn any_one_reason_opens_a_door() {
        assert!(wants_open(true, false, false));
        assert!(wants_open(false, true, false));
        assert!(wants_open(false, false, true));
        assert!(!wants_open(false, false, false));
    }
}
