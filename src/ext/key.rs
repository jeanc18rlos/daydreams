//! EXT: the key that comes out of a painting and opens the window. Not part of the C++ port.
//!
//! A `Key` is a `Grabbable`-shaped prop (`ext/grab.rs`): a `Physical` with one hit sphere,
//! which is what makes the grab see it, carried by the engine's own physics -- gravity and the
//! rectangle and triangle colliders -- once it is loose. Its mesh is `Meshes/key.obj`, a flat
//! silhouette from `tools/gen_key.py` in `gold.bmp` through the rigid-body props' `prop`
//! shader (`ext/rigid.rs`): in the hall that is the cabin's hemisphere light and the walls'
//! fog, and the ported `texture` light -- one fixed lamp, from above and +z -- had left the
//! key near black from most directions.
//!
//! It has two lives:
//!
//! * **Floating.** The painting that painted it (`ext/painting.rs`) spawns it a hand's
//!   breadth in front of the canvas and animates it out: scale and position are the
//!   painting's to set each step ([`Key::float_at`]), the key runs no physics and
//!   `engine_collision` is false, so the collision pass never pushes it and the portal pass
//!   never warps it. The grab can still take it, at any point of the animation.
//! * **In hand and loose.** `on_grab` ends the floating life for good: the shared `taken`
//!   flag tells the painting its key is gone, physics is on, and the grab carries it. While
//!   held the key looks down the crosshair, once per rendered frame, for something that
//!   `ObjectT::accepts_key` -- the locked window -- within [`USE_REACH`]; with one there it
//!   offers [`USE_HINT`] and says so on a channel the engine reads ([`take_wants_use`]).
//!
//! # The press
//!
//! The use goes through the same latch as every other E: the keyboard's key, the gamepad's
//! button and `--e-at` all set `Engine::pad_grab`, and `Engine::ext_update` hands the frame's
//! press to whoever claims it first -- the elevator when the player stands in its cabin, then
//! the held key when it wants the press ([`press`]), and only otherwise the grab, as a
//! pickup or a release. So a press with a lock in reach uses the key and is NOT the grab's
//! release, and a pad player is not left dropping the key at the window's foot. The key sees
//! the press on its next fixed step ([`take_press`]), raises `room::request_unlock_window`,
//! marks itself used and asks for its own removal (`room::request_remove`), which lands at
//! the end of that step: the grab is told (`GrabState::on_removed`) and is simply holding
//! nothing, with no release and no press left over that could pick up the window instead.
//!
//! # The window's side of the contract
//!
//! `ObjectT::accepts_key` (object.rs) defaults to false. The window answers true while it is
//! locked, and it is found by its bounding sphere (`grab::bound_radius` times `p_scale`, as
//! the grab picks) along the crosshair, nearest first, within `USE_REACH` metres of the eye;
//! it need not be grabbable and need not collide. When the key is used the window finds
//! `room::take_unlock_window()` true on its next step. Its `pick_hint` ("LOCKED") is outranked
//! by the key's prompt while the key is in hand (`hint::insist`), so it need not know about
//! the key at all.

use crate::camera::Camera;
use crate::ext::audio::{self, Sfx};
use crate::ext::grab::{bound_radius, GrabState};
use crate::ext::raycast::{ray_sphere, raycast_ignoring};
use crate::ext::{hint, room};
use crate::object::{Object, ObjectT, RenderCtx, UpdateCtx};
use crate::physical::Physical;
use crate::resources::Resources;
use crate::scene::PObjectVec;
use crate::sphere::Sphere;
use crate::vector::{Matrix4, Vector3};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

/// How far from the eye a lock may be for the held key to reach it, in metres.
pub const USE_REACH: f32 = 2.5;

thread_local! {
    /// Set by the held key's step while a lock is in reach; taken by the engine once per
    /// rendered frame, before it decides whose the frame's E press is.
    static WANTS_USE: Cell<bool> = const { Cell::new(false) };
    /// The frame's E, handed to the key by the engine; taken by the key's next step.
    static PRESS: Cell<bool> = const { Cell::new(false) };
}

/// Whether the held key wants the frame's E press -- a lock is under the crosshair within
/// reach. Consumed: the key's steps set it afresh every frame it holds, so the engine reads
/// it once per frame and never sees a stale offer from a key since dropped or removed.
pub fn take_wants_use() -> bool {
    WANTS_USE.with(Cell::take)
}

/// Hand the frame's E press to the key (module docs, "The press").
pub fn press() {
    PRESS.with(|p| p.set(true));
}

/// The key's side: say the press is wanted, and take one the engine has handed over.
fn offer_use() {
    WANTS_USE.with(|w| w.set(true));
}

fn take_press() -> bool {
    PRESS.with(Cell::take)
}
/// The prompt while the held key is on a lock.
pub const USE_HINT: &str = "E  USE THE KEY";
/// The prompt while the crosshair is on the key itself (through `pick_hint`).
pub const TAKE_HINT: &str = "E  TAKE THE KEY";
/// The smallest scale the emergence may draw the key at: a zero scale has no inverse, and
/// `draw_impl` needs `world_to_local` for the normal matrix.
const SCALE_FLOOR: f32 = 0.02;

pub struct Key {
    base: Physical,
    /// The cell the scene holds this key in, for `room::request_remove`. Weak, or the key
    /// would own itself.
    me: Weak<RefCell<Key>>,
    /// Shared with the painting that painted it: set at the first grab, after which the
    /// painting's key is gone for good.
    taken: Rc<Cell<bool>>,
    /// Animated by the painting, out of its canvas: no physics, no collision.
    floating: bool,
    held: bool,
    used: bool,
    /// The lock scan's answer and the frame clock it was taken at: the scan walks the scene,
    /// and once per rendered frame is as often as its answer can change the HUD or the press.
    lock_near: bool,
    scanned_at: f32,
}

impl Key {
    /// A key, not yet in any scene, at the origin: the painting places it (`place`) or the
    /// dev hold puts it in hand (`dev_hold`).
    pub fn new(res: &Resources, taken: Rc<Cell<bool>>) -> Rc<RefCell<Key>> {
        let mesh = res.acquire_mesh("key.obj");
        let mut base = Physical::new();
        // A little drag so a dropped key settles instead of skittering forever.
        base.drag = 0.002;
        base.friction = 0.08;
        // Exactly ONE hit sphere: the grab's marker for a grabbable (`grab::as_grabbable`),
        // and what the collision pass pushes. The mesh's own radius, so it rests on its rim.
        base.hit_spheres.push(Sphere::new_at(Vector3::zero(), mesh.bound_radius));
        base.base.mesh = Some(mesh);
        base.base.shader = Some(res.acquire_shader("prop"));
        base.base.texture = Some(res.acquire_texture("gold.bmp", 1, 1));
        Rc::new_cyclic(|me| {
            RefCell::new(Key {
                base,
                me: me.clone(),
                taken,
                floating: true,
                held: false,
                used: false,
                lock_near: false,
                scanned_at: f32::NAN,
            })
        })
    }

    /// Where and which way up the key lies while it is the painting's: `rot` is a pure
    /// rotation (the picture plane's frame, `Object::rot`), set once; the position and scale
    /// follow each step through `float_at`.
    pub fn place(&mut self, pos: Vector3, rot: Matrix4) {
        self.base.base.rot = Some(rot);
        self.float_at(pos, 0.0);
    }

    /// One step of the emergence: the key at `pos`, scaled to `blend` of its full size.
    /// `set_position` moves `prev_pos` too, so the portal pass -- which skips it anyway --
    /// could never see a segment.
    pub fn float_at(&mut self, pos: Vector3, blend: f32) {
        self.base.set_position(pos);
        self.base.base.scale = Vector3::splat(blend.max(SCALE_FLOOR));
        self.base.velocity = Vector3::zero();
    }
}

/// The nearest object that `accepts_key` under the crosshair within [`USE_REACH`], as its
/// index in `scene`: picked by bounding sphere, as the grab picks (`grab::pick`), so a lock
/// is found the same way whether or not it can be picked up. The key's own cell is mutably
/// borrowed while this runs from its step, and `try_borrow` skips it -- it is not a lock.
pub fn lock_under_crosshair(
    scene: &[Rc<RefCell<dyn ObjectT>>],
    cam_to_world: &Matrix4,
) -> Option<usize> {
    let origin = cam_to_world.translation();
    let dir = cam_to_world.mul_direction(Vector3::new(0.0, 0.0, -1.0)).normalized_safe();
    let mut best: Option<(usize, f32)> = None;
    for (i, cell) in scene.iter().enumerate() {
        let Ok(obj) = cell.try_borrow() else { continue };
        if !obj.accepts_key() {
            continue;
        }
        let base = obj.base();
        let Some(t) = ray_sphere(origin, dir, base.pos, bound_radius(base) * base.p_scale) else {
            continue;
        };
        if t <= USE_REACH && best.is_none_or(|(_, bt)| t < bt) {
            best = Some((i, t));
        }
    }
    // EXT: and the lock has to be in sight, or the key would open a window through the wall
    // it hangs on. The key's own cell is borrowed by the caller, so `raycast`'s `try_borrow`
    // already skips it; the lock itself is exempt because its own pane is what the ray ends on.
    let (i, t) = best?;
    if raycast_ignoring(scene, origin, dir, t - crate::ext::grab::LOS_SLACK, &[i]).is_some() {
        return None;
    }
    Some(i)
}

/// A stand-in lock for `--hold-key` in a scene that has none: an invisible, collisionless
/// point that accepts the key, so the use chain can be driven before the window exists. Not
/// planted once anything in the scene accepts a key.
struct StubLock {
    base: Object,
}

impl ObjectT for StubLock {
    fn base(&self) -> &Object {
        &self.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base
    }
    fn accepts_key(&self) -> bool {
        true
    }
}

/// `--hold-key`: a key already in the player's hand at scene start. Built loose, pushed onto
/// the scene, and handed to the grab as if it had just been picked up at `HOLD_DIST` with
/// its full size -- the next `grab::update` carries it from there. Gravity is off as the
/// carry keeps it, and `on_grab` has run, so the use test is live from the first step. In a
/// scene with nothing that accepts a key, a [`StubLock`] is planted a metre down the
/// crosshair to take it.
pub fn dev_hold(
    res: &Resources,
    objs: &mut PObjectVec,
    grab: &mut GrabState,
    cam_to_world: &Matrix4,
) {
    const HOLD_DIST: f32 = 0.6;
    const STUB_DIST: f32 = 1.0;
    let origin = cam_to_world.translation();
    let dir = cam_to_world.mul_direction(Vector3::new(0.0, 0.0, -1.0)).normalized_safe();
    if !objs.iter().any(|o| o.borrow().accepts_key()) {
        let mut base = Object::new();
        base.pos = origin + dir * STUB_DIST;
        objs.push(Rc::new(RefCell::new(StubLock { base })) as Rc<RefCell<dyn ObjectT>>);
        log::info!("[key] --hold-key: nothing here accepts a key; a stand-in lock is planted {STUB_DIST} m ahead");
    }
    let key = Key::new(res, Rc::new(Cell::new(false)));
    let hold = origin + dir * HOLD_DIST;
    let radius = {
        let mut k = key.borrow_mut();
        k.base.set_position(hold);
        k.base.gravity = Vector3::zero();
        k.on_grab();
        bound_radius(k.base())
    };
    objs.push(key as Rc<RefCell<dyn ObjectT>>);
    grab.held = Some(objs.len() - 1);
    grab.ratio = 1.0 / HOLD_DIST;
    grab.radius = radius;
    grab.eased_scale = 1.0;
    // The hand's last known place and time, as a real pickup's first carry leaves them:
    // without these the first `grab::update` would measure the hand's velocity as the key's
    // position over the seconds since launch -- thousands of units a second handed to
    // `on_release` by an `--e-at 1`. (The key ignores the throw; a prop would not.)
    grab.hand_pos = hold;
    grab.hand_time = crate::ext::view::time();
    log::info!("[key] --hold-key: the key is in hand");
}

impl ObjectT for Key {
    fn base(&self) -> &Object {
        &self.base.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base.base
    }

    fn update(&mut self, ctx: &UpdateCtx) {
        if self.floating {
            // The painting's to move (`float_at`).
            return;
        }
        self.base.update();
        if !self.held || self.used {
            return;
        }
        // A press handed over is this frame's, whether or not the lock is still in reach: it
        // is taken every step so that one cannot wait for a later aim.
        let pressed = take_press();
        // The frame clock moves once per rendered frame (`Engine::run_frame`): one scan per
        // frame, shared by the steps in it.
        let now = crate::ext::view::time();
        if now != self.scanned_at {
            self.scanned_at = now;
            self.lock_near = lock_under_crosshair(ctx.scene, &ctx.cam_to_world).is_some();
        }
        if !self.lock_near {
            return;
        }
        offer_use();
        hint::insist(USE_HINT);
        if pressed {
            self.used = true;
            // EXT: the lock turning over -- fired here, on the press, rather than off the
            // window's unlock a step later, because this is the frame the player acted on.
            audio::request(Sfx::KeyUse);
            room::request_unlock_window();
            log::info!("[key] used on the window: unlock requested");
            // Removal by identity; the cell this key lives in is the one the scene holds.
            if let Some(me) = self.me.upgrade() {
                room::request_remove(&(me as Rc<RefCell<dyn ObjectT>>));
            }
        }
    }

    fn on_grab(&mut self) {
        self.held = true;
        self.floating = false;
        self.taken.set(true);
        // EXT: the small ring of it coming off the picture plane. Once: `on_grab` is what
        // ends the floating life, and a key already in hand cannot be taken again.
        audio::request(Sfx::KeyTake);
        let b = &mut self.base.base;
        // Whatever point of the emergence it was taken at, in hand it is whole.
        b.scale = Vector3::ones();
        // The picture plane's frame (`place`) was a rotation-matrix override, which the
        // rotate modifier cannot add to and which would hold the key edge-on to the hall in
        // world space however the player turned: in hand it becomes the Euler path's, as a
        // rigid prop's does (`ext/rigid.rs`), and turns with the rest.
        if let Some(rot) = b.rot.take() {
            b.euler = rot.to_euler();
        }
        // A fresh hold starts with no press pending and no scan to trust.
        take_press();
        self.scanned_at = f32::NAN;
    }

    fn on_release(&mut self, _velocity: Vector3) {
        self.held = false;
    }

    fn engine_collision(&self) -> bool {
        !self.floating
    }

    fn pick_hint(&self) -> Option<&'static str> {
        Some(TAKE_HINT)
    }

    fn on_collide(&mut self, push: Vector3) {
        self.base.on_collide(push);
    }
    fn as_physical(&self) -> Option<&Physical> {
        Some(&self.base)
    }
    fn as_physical_mut(&mut self) -> Option<&mut Physical> {
        Some(&mut self.base)
    }
    fn draw(&self, ctx: &RenderCtx, cam: &Camera, _fbo: Option<glow::Framebuffer>) {
        // The prop shader's own uniforms, as `RigidProp::draw` sets them (Shaders/prop.frag).
        let b = &self.base.base;
        if let Some(shader) = &b.shader {
            shader.use_program();
            shader.set_vec4("fog_color", crate::ext::backrooms::WALL_FOG);
            shader.set_mat4("model", &b.local_to_world());
        }
        b.draw_impl(ctx, cam);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Something that takes a key, or does not, at a point: meshless, so its bounding
    /// sphere is `bound_radius`'s half-unit default.
    struct Lock {
        base: Object,
        locked: bool,
    }
    impl ObjectT for Lock {
        fn base(&self) -> &Object {
            &self.base
        }
        fn base_mut(&mut self) -> &mut Object {
            &mut self.base
        }
        fn accepts_key(&self) -> bool {
            self.locked
        }
    }
    fn lock(pos: Vector3, locked: bool) -> Rc<RefCell<dyn ObjectT>> {
        let mut base = Object::new();
        base.pos = pos;
        Rc::new(RefCell::new(Lock { base, locked }))
    }

    /// A camera at `eye` looking along -z, as `Player::cam_to_world` builds it at yaw 0.
    fn camera(eye: Vector3) -> Matrix4 {
        Matrix4::trans(eye)
    }

    #[test]
    fn the_nearest_lock_under_the_crosshair_within_reach_is_found() {
        let eye = Vector3::new(0.0, 1.5, 0.0);
        let cam = camera(eye);
        let scene: Vec<Rc<RefCell<dyn ObjectT>>> = vec![
            lock(eye + Vector3::new(0.0, 0.0, -2.0), true), // on the crosshair, in reach
            lock(eye + Vector3::new(0.0, 0.0, -1.0), false), // nearer, but not a lock
            lock(eye + Vector3::new(0.0, 0.0, -1.5), true), // nearer still, and a lock
            lock(eye + Vector3::new(3.0, 0.0, -1.0), true), // off to the side
            lock(eye + Vector3::new(0.0, 0.0, -USE_REACH - 1.0), true), // past the reach
        ];
        assert_eq!(lock_under_crosshair(&scene, &cam), Some(2));
        // Turned away, nothing.
        let away = Matrix4::trans(eye) * Matrix4::rot_y(std::f32::consts::PI);
        assert_eq!(lock_under_crosshair(&scene, &away), None);
        // A cell already borrowed mutably -- the key's own, mid-update -- is skipped, not a
        // panic.
        let held = scene[2].borrow_mut();
        assert_eq!(lock_under_crosshair(&scene, &cam), Some(0));
        drop(held);
    }

    /// The press channel: the key's offer is consumed by the engine's read, so a stale offer
    /// cannot claim a later frame's press; a press handed over is taken once by the key.
    #[test]
    fn the_offer_and_the_press_are_each_taken_once() {
        assert!(!take_wants_use());
        offer_use();
        offer_use(); // several steps a frame
        assert!(take_wants_use());
        assert!(!take_wants_use(), "consumed by the frame's read");
        assert!(!take_press());
        press();
        assert!(take_press());
        assert!(!take_press(), "consumed by the key's step");
    }

    #[test]
    fn reach_is_measured_to_the_spheres_near_side() {
        let eye = Vector3::new(0.0, 1.5, 0.0);
        let cam = camera(eye);
        // The sphere (radius 0.5) is hit at USE_REACH exactly when its centre is half a unit
        // further; a hair past that is out of reach.
        let just = lock(eye + Vector3::new(0.0, 0.0, -(USE_REACH + 0.5 - 0.01)), true);
        let past = lock(eye + Vector3::new(0.0, 0.0, -(USE_REACH + 0.5 + 0.01)), true);
        assert_eq!(lock_under_crosshair(&[just], &cam), Some(0));
        assert_eq!(lock_under_crosshair(&[past], &cam), None);
    }
}
