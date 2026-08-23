//! EXT: per-frame room logic. Not part of the C++ port.
//!
//! The ported `Scene` trait has exactly one method, `Load` (Scene.h:7-9). Scenes build objects
//! and then have no further say -- there is no per-frame hook anywhere in the engine.
//!
//! Rather than add one to the ported `Scene` trait, rooms that need behaviour push a `RoomLogic`
//! object into the scene. It is an ordinary `ObjectT` with no mesh and no shader, so
//! `Object::Draw` skips it entirely (Object.cpp:21 only draws when both are present) while
//! `Engine::Update` still calls its `update` every step, exactly like any other object.
//!
//! The closure captures `Rc<RefCell<..>>` handles to whatever the room wants to animate,
//! grabbed at load time. Borrowing them during `update` is safe: they are *different* `RefCell`s
//! from the one holding the `RoomLogic` itself, so there is no aliasing -- the same reasoning
//! that makes the ported collision pass sound (Engine.cpp:162).
//!
//! # Moving the player
//!
//! The one thing a closure cannot capture is the player: `Scene::load` receives `&mut Player`
//! (Scene.h:9), never the `Rc` the engine keeps it in. A room that needs to put the player
//! somewhere -- the Backrooms, when they have fallen under its floor -- asks for it with
//! [`request_respawn`], and the engine carries it out at the end of the same step through
//! [`apply_respawn`], after the portal pass. The same ambient-channel shape as
//! `ext::terrain::wrap_player`, and placed next to it, for the same reason: a position set
//! mid-step would hand `try_portal` a segment from wherever the player was to wherever they are
//! now, and a long enough one sweeps a doorway.
//!
//! Two more requests ride the same channel shape, for the same reason -- a room's closure can
//! reach neither the portal vector nor the scene list:
//!
//! * [`request_remove_portals`] takes portals out of the scene for good (the Backrooms' door,
//!   once crossed). Applied by `Engine::update` after the portal pass, like the respawn: the
//!   pass that just ran may have warped the player through one of them.
//! * [`request_scene_load`] asks for another scene (the elevator's ride). Applied by
//!   `Engine::run_frame` once the fixed-step loop is over -- never mid-step, because the load
//!   replaces the object vector the step is iterating.
//! * [`request_spawn`] and [`request_remove`] add an object to the scene and take one out of
//!   it (a key that has been used, a prop a puzzle conjures). Applied by `Engine::update` after
//!   the portal pass, like the respawn, for the same reason the scene load waits: the object
//!   vector is iterated by index through the whole step. Removal shifts the indices of
//!   everything after the removed object, and the grab holds one of those indices across
//!   frames, so `apply_removes` reports what went and the engine hands that to
//!   `grab::GrabState::on_removed`.
//! * [`request_unlock_window`] is a one-shot flag between two objects that cannot see each
//!   other: the key raises it, the window takes it with [`take_unlock_window`].

use crate::camera::Camera;
use crate::object::{Object, ObjectT, RenderCtx, UpdateCtx};
use crate::player::Player;
use crate::scene::{PObjectVec, PPortalVec};
use crate::vector::Vector3;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// An invisible object that runs a closure every fixed step.
pub struct RoomLogic {
    base: Object,
    logic: Box<dyn FnMut(&UpdateCtx)>,
}

impl RoomLogic {
    pub fn new(logic: impl FnMut(&UpdateCtx) + 'static) -> RoomLogic {
        RoomLogic {
            // No mesh and no shader, so it is never drawn and never collides.
            base: Object::new(),
            logic: Box::new(logic),
        }
    }
}

impl ObjectT for RoomLogic {
    fn base(&self) -> &Object {
        &self.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base
    }
    fn update(&mut self, ctx: &UpdateCtx) {
        (self.logic)(ctx);
    }
    /// Explicitly draws nothing. `Object::draw_impl` would already no-op without a mesh, but
    /// being explicit keeps the intent obvious.
    fn draw(&self, _ctx: &RenderCtx, _cam: &Camera, _fbo: Option<glow::Framebuffer>) {}
}

/// Where to put the player, and which way to face them.
#[derive(Clone, Copy, Debug)]
pub struct Respawn {
    /// The player's position -- eye height, as `Player` keeps it.
    pub pos: Vector3,
    /// Camera yaw, in the engine's own convention: forward is `(-sin yaw, 0, -cos yaw)`.
    pub yaw: f32,
}

impl Respawn {
    /// Stand at `pos` looking along the horizontal direction `dir`. The yaw is the one
    /// `Physical::try_portal` derives from a forward vector (Physical.cpp:57-59), so facing
    /// "down the hall" here means the same thing as arriving through the door facing it.
    pub fn facing(pos: Vector3, dir: Vector3) -> Respawn {
        Respawn { pos, yaw: -dir.x.atan2(-dir.z) }
    }
}

thread_local! {
    /// EXT: the respawn a room asked for this step, if any. See the module docs.
    static RESPAWN: Cell<Option<Respawn>> = const { Cell::new(None) };
}

/// Ask the engine to move the player at the end of this step. A later request in the same
/// step wins; there is no sensible way to honour two.
pub fn request_respawn(r: Respawn) {
    RESPAWN.with(|c| c.set(Some(r)));
}

/// Carry out a pending request. Called by `Engine::update` after the portal pass, exactly
/// once per step. `set_position` moves `prev_pos` with `pos`, so the next step's portal test
/// sees no segment; the velocity is zeroed so nothing of the fall survives; and the body's own
/// yaw -- which a portal warp sets, and which the camera's adds to -- is cleared so the
/// heading asked for is the heading got.
pub fn apply_respawn(player: &mut Player) {
    let Some(r) = RESPAWN.with(|c| c.take()) else { return };
    player.base.set_position(r.pos);
    player.base.velocity = Vector3::zero();
    player.base.base.euler = Vector3::zero();
    player.set_look(r.yaw, 0.0);
}

thread_local! {
    /// EXT: portal ids a room has asked to have removed. Ids, not indices: `Portal::id` is
    /// what `Warp::to_portal` names a portal by, and it survives the vector being reordered.
    static REMOVE_PORTALS: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
    /// EXT: the registry index a room has asked the engine to load next.
    static SCENE_LOAD: Cell<Option<usize>> = const { Cell::new(None) };
}

/// Ask the engine to drop the portals with these ids at the end of this step. Requests
/// accumulate until applied; an id the scene does not have is ignored.
pub fn request_remove_portals(ids: &[u32]) {
    REMOVE_PORTALS.with(|r| r.borrow_mut().extend_from_slice(ids));
}

/// Carry out pending removals on the scene's portal vector: every portal whose id was asked
/// for is dropped, the rest keep their order. Called by `Engine::update` after the portal
/// pass, once per step. Returns whether anything was removed, so the engine can forget the
/// occlusion results that named the old vector's indices.
pub fn apply_remove_portals(portals: &mut PPortalVec) -> bool {
    let ids = REMOVE_PORTALS.with(|r| std::mem::take(&mut *r.borrow_mut()));
    if ids.is_empty() {
        return false;
    }
    let before = portals.len();
    portals.retain(|p| !ids.contains(&p.borrow().id));
    portals.len() != before
}

/// Ask the engine to load registry scene `ix` once the current frame's steps are done. A
/// later request in the same frame wins.
pub fn request_scene_load(ix: usize) {
    SCENE_LOAD.with(|c| c.set(Some(ix)));
}

/// The pending scene request, consumed. Called by `Engine::run_frame` after the fixed-step
/// loop.
pub fn take_scene_load() -> Option<usize> {
    SCENE_LOAD.with(|c| c.take())
}

thread_local! {
    /// EXT: objects a room has asked to have added to the scene, in request order.
    static SPAWNS: RefCell<Vec<Rc<RefCell<dyn ObjectT>>>> = const { RefCell::new(Vec::new()) };
    /// EXT: objects a room has asked to have taken out of the scene, by identity.
    static REMOVES: RefCell<Vec<Rc<RefCell<dyn ObjectT>>>> = const { RefCell::new(Vec::new()) };
    /// EXT: whether the window's key has been used since the window last looked.
    static UNLOCK_WINDOW: Cell<bool> = const { Cell::new(false) };
}

/// Ask the engine to add `obj` to the scene at the end of this step. Requests accumulate
/// until applied and keep their order.
#[allow(dead_code)] // EXT: scene code calls it (the window, its key, spawned props).
pub fn request_spawn(obj: Rc<RefCell<dyn ObjectT>>) {
    SPAWNS.with(|q| q.borrow_mut().push(obj));
}

/// Carry out pending spawns: the objects are appended to the scene's object vector in the
/// order they were asked for. Called by `Engine::update` after the portal pass, once per
/// step. Appended, not inserted before the player: `load_scene` pushes the player last, but
/// nothing reads the vector's tail as the player -- every pass walks the whole vector and
/// dispatches on `as_physical`, and the grab and the room closures keep `Rc`s, not positions.
pub fn apply_spawns(objs: &mut PObjectVec) {
    SPAWNS.with(|q| objs.append(&mut q.borrow_mut()));
}

/// Ask the engine to take `obj` out of the scene at the end of this step. Matched by
/// `Rc::ptr_eq`, so the handle must be a clone of the one the scene was given; an object
/// that is not in the scene is ignored. Asking twice removes it once.
#[allow(dead_code)] // EXT: scene code calls it (the window, its key, spawned props).
pub fn request_remove(obj: &Rc<RefCell<dyn ObjectT>>) {
    REMOVES.with(|q| q.borrow_mut().push(Rc::clone(obj)));
}

/// Carry out pending removals on the scene's object vector; the rest keep their order.
/// Called by `Engine::update` after the portal pass, once per step. Returns the indices the
/// removed objects HAD, ascending, so the engine can fix whatever still names objects by
/// index (`grab::GrabState::on_removed`); empty when nothing went.
pub fn apply_removes(objs: &mut PObjectVec) -> Vec<usize> {
    let asked = REMOVES.with(|q| std::mem::take(&mut *q.borrow_mut()));
    if asked.is_empty() {
        return Vec::new();
    }
    let gone: Vec<usize> = objs
        .iter()
        .enumerate()
        .filter(|(_, o)| asked.iter().any(|a| Rc::ptr_eq(a, o)))
        .map(|(i, _)| i)
        .collect();
    // Back to front, so each removal leaves the indices still to go untouched.
    for &i in gone.iter().rev() {
        objs.remove(i);
    }
    gone
}

/// Tell the window its key has been used. Stays raised until the window takes it, so the
/// two need not run in any particular order within a step.
#[allow(dead_code)] // EXT: scene code calls it (the window, its key, spawned props).
pub fn request_unlock_window() {
    UNLOCK_WINDOW.with(|c| c.set(true));
}

/// Whether the key has been used since the last call; consumed.
#[allow(dead_code)] // EXT: scene code calls it (the window, its key, spawned props).
pub fn take_unlock_window() -> bool {
    UNLOCK_WINDOW.with(Cell::take)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_header::GH_PI;
    use crate::portal::Portal;
    use std::rc::Rc;

    #[test]
    fn facing_matches_the_portal_convention() {
        // Heading -x, the way a player arrives in the Backrooms hall.
        let r = Respawn::facing(Vector3::zero(), Vector3::new(-1.0, 0.0, 0.0));
        assert!((r.yaw - GH_PI / 2.0).abs() < 1e-6, "yaw {}", r.yaw);
        let forward = Vector3::new(-r.yaw.sin(), 0.0, -r.yaw.cos());
        assert!((forward - Vector3::new(-1.0, 0.0, 0.0)).mag() < 1e-6);
        // And the spawn's own facing: -z is yaw 0.
        assert!(Respawn::facing(Vector3::zero(), Vector3::new(0.0, 0.0, -1.0)).yaw.abs() < 1e-6);
    }

    #[test]
    fn a_request_is_applied_once_and_leaves_no_segment() {
        let mut p = Player::new();
        p.base.set_position(Vector3::new(5.0, 1.5, 5.0));
        p.base.velocity = Vector3::new(0.0, -9.0, 0.0);
        p.base.base.euler.y = 1.0;
        let r = Respawn::facing(Vector3::new(0.0, 1.5, 0.0), Vector3::new(-1.0, 0.0, 0.0));
        request_respawn(r);
        apply_respawn(&mut p);
        assert!((p.obj().pos - r.pos).mag() == 0.0);
        assert!((p.base.prev_pos - r.pos).mag() == 0.0, "prev_pos must move with pos");
        assert!(p.base.velocity.mag() == 0.0);
        assert!(p.obj().euler.mag() == 0.0);
        // Consumed: a second apply changes nothing.
        let elsewhere = Vector3::new(1.0, 1.0, 1.0);
        p.base.set_position(elsewhere);
        apply_respawn(&mut p);
        assert!((p.obj().pos - elsewhere).mag() == 0.0);
    }

    #[test]
    fn removing_portals_keeps_the_others_in_order() {
        let portals: PPortalVec =
            (0..4).map(|_| Rc::new(RefCell::new(Portal::detached()))).collect();
        let ids: Vec<u32> = portals.iter().map(|p| p.borrow().id).collect();
        let mut v = portals.clone();
        assert!(!apply_remove_portals(&mut v), "nothing asked for, nothing removed");
        request_remove_portals(&[ids[1]]);
        request_remove_portals(&[ids[3], 0xFFFF_FFF0]);
        assert!(apply_remove_portals(&mut v));
        let left: Vec<u32> = v.iter().map(|p| p.borrow().id).collect();
        assert_eq!(left, [ids[0], ids[2]]);
        // Consumed: nothing more goes.
        assert!(!apply_remove_portals(&mut v));
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn a_scene_load_request_is_taken_once_and_the_last_wins() {
        assert_eq!(take_scene_load(), None);
        request_scene_load(3);
        request_scene_load(7);
        assert_eq!(take_scene_load(), Some(7));
        assert_eq!(take_scene_load(), None);
    }

    /// A scene of bare objects, each told apart by its x position.
    fn scene(n: usize) -> PObjectVec {
        (0..n)
            .map(|i| {
                let mut o = Object::new();
                o.pos.x = i as f32;
                Rc::new(RefCell::new(o)) as Rc<RefCell<dyn ObjectT>>
            })
            .collect()
    }

    fn xs(v: &PObjectVec) -> Vec<f32> {
        v.iter().map(|o| o.borrow().base().pos.x).collect()
    }

    #[test]
    fn spawns_are_appended_in_request_order_and_once() {
        let mut v = scene(2);
        apply_spawns(&mut v);
        assert_eq!(v.len(), 2, "nothing asked for, nothing added");
        let new = scene(3);
        request_spawn(Rc::clone(&new[2]));
        request_spawn(Rc::clone(&new[0]));
        apply_spawns(&mut v);
        assert_eq!(xs(&v), [0.0, 1.0, 2.0, 0.0]);
        assert!(Rc::ptr_eq(&v[2], &new[2]) && Rc::ptr_eq(&v[3], &new[0]));
        apply_spawns(&mut v);
        assert_eq!(v.len(), 4, "consumed");
    }

    #[test]
    fn removes_match_by_identity_keep_order_and_report_old_indices() {
        let mut v = scene(5);
        assert!(apply_removes(&mut v).is_empty());
        let stranger = scene(1);
        request_remove(&v[3]);
        request_remove(&v[1]);
        request_remove(&v[1]); // twice: once is enough
        request_remove(&stranger[0]); // not in the scene: ignored
        assert_eq!(apply_removes(&mut v), [1, 3]);
        assert_eq!(xs(&v), [0.0, 2.0, 4.0]);
        assert!(apply_removes(&mut v).is_empty(), "consumed");
        assert_eq!(v.len(), 3);
    }

    #[test]
    fn the_unlock_flag_is_one_shot_and_sticks_until_taken() {
        assert!(!take_unlock_window());
        request_unlock_window();
        request_unlock_window();
        assert!(take_unlock_window());
        assert!(!take_unlock_window());
    }
}
