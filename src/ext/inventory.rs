//! EXT: the small set of slots the player carries between levels. Not part of the C++ port.
//!
//! The Superliminal carry (`ext/grab.rs`) holds exactly one thing, in front of the eye, and
//! puts it down where you aim. That is unchanged. This is the other half: six slots you can
//! **stow** into and take back out, which survive walking through a door and riding the
//! elevator.
//!
//! # What a slot owns
//!
//! A [`Slot`] owns the object itself -- the same `Rc<RefCell<dyn ObjectT>>` the scene held --
//! not a description of it. Stowing takes the object out of the world with
//! [`room::request_remove`] and keeps that `Rc` alive in the slot; taking it back out puts it
//! in with [`room::request_spawn`]. Both are queues the engine already applies between fixed
//! steps, so nothing here has to reach the object vector while a step is iterating it. The
//! object keeps its identity, its scale and its state, which is the whole point: the apple you
//! pocket in the Backrooms is the apple that rolls across the Pool Rooms' tiles.
//!
//! The slot also remembers the carry's pin -- `ratio` (`k = p_scale / distance`), `radius` and
//! the `p_scale` the object was at -- so a retrieve can hand it back to the grab at exactly the
//! apparent size it was stowed at, rather than at whatever size the crosshair happens to imply.
//!
//! # One frame of flight
//!
//! `request_spawn` lands at the end of the NEXT fixed step, so the index the grab needs does
//! not exist at the moment the player presses the key. A retrieve therefore places the object,
//! tells it ([`ObjectT::on_unstow`], then `on_grab`), asks for the spawn, and records a
//! [`Pending`] naming the slot it came from; the next frame finds the object in the vector by
//! identity and pins the carry. The slot keeps the item for that one frame, so a retrieve that
//! never lands -- nothing observed can cause it, the guard is for a change that could -- leaves
//! the item in the inventory rather than nowhere at all. While a retrieve is in flight the keys
//! do nothing: there is no hand to stow from and the item is already spoken for.
//!
//! # Scene-bound state
//!
//! Between the two hooks an object is drawn by nobody and stepped by nobody, and a scene load
//! replaces the world around it. Three things in this codebase are bound to the scene the
//! object left and are given up in `on_stow` and rebuilt in `on_unstow`:
//!
//! * A [`RigidProp`](crate::ext::rigid::RigidProp)'s rapier body. The world outlives a scene
//!   load (only its static colliders are rebuilt), so the body would keep falling with nothing
//!   drawing it and would still appear in the `[prop]` report. It leaves the world on stow and
//!   a fresh one is built at the pose and scale it comes back at.
//! * The [`Key`](crate::ext::key::Key)'s standing "a lock is in reach" offer, which would let a
//!   pocketed key claim the frame's E press.
//! * `Physical::prev_pos`. The portal pass reads the segment from `prev_pos` to `pos`, and a
//!   segment from the level the object was stowed in to the one it is taken out in sweeps every
//!   doorway between them. Every placement here goes through `set_position`, which moves both.
//!
//! The [`Window`](crate::ext::window::Window) is the one thing that refuses outright
//! (`ObjectT::can_stow`): it is level furniture the size of a door, its opening is a pair of
//! portals in the scene's portal vector, and those are gone on the next load -- pocketing the
//! way out and unpacking a dead frame somewhere else is not a mechanic. The key's link to the
//! painting that painted it (a shared `Rc<Cell<bool>>`) needs nothing: it is already set, and
//! the painting reading it is only ever in the scene the key came from.
//!
//! # Bindings
//!
//! `F` stows what is in hand, or takes the selected slot's item into the hand when the hand is
//! empty; `G` drops the selected item straight into the world; the mouse wheel changes the
//! selection. They were chosen against a crowded keyboard: the number row and the punctuation
//! keys are the scene registry's, `E` is grab/use, `M` mute, `R` rotate, `Shift` sprint,
//! `Space` jump. The pad is deliberately left out -- its D-pad left/right already cycles
//! scenes (`ext/gamepad.rs`), and a binding that means two things depending on how long you
//! have been playing is worse than no binding.

use crate::ext::grab::{self, GrabState};
use crate::ext::raycast::raycast;
use crate::ext::{hint, room};
use crate::object::ObjectT;
use crate::vector::{Matrix4, Vector3};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// How many slots the player has.
///
/// Six because the row has to be readable at a glance at the bottom of the screen without
/// crowding the hint line, and because there are three props, a key and room to spare in the
/// levels that exist -- enough that "which slot" is a real choice, few enough that scrolling
/// to one is never a chore.
pub const CAPACITY: usize = 6;

/// How long a refusal stays on the hint line, in seconds. A key press is over in a frame, and
/// a line that showed for one frame would never be read.
const NOTICE_SECS: f32 = 1.6;
/// How far in front of the eye `G` puts an item down, in world units, when nothing is nearer.
const DROP_DIST: f32 = 0.9;
/// Frames a retrieve may wait for its spawn before the item is left in its slot.
const PENDING_FRAMES: u32 = 60;

/// Refusals, on the hint line.
pub const FULL_HINT: &str = "POCKETS FULL";
pub const REFUSED_HINT: &str = "IT WILL NOT FIT";
pub const EMPTY_HINT: &str = "THAT SLOT IS EMPTY";

/// What just happened to the inventory, for whoever turns it into a sound.
///
/// A single slot on a thread-local, like the elevator's ride-start edge
/// (`ext::elevator::take_ride_started`): the inventory sets it and the engine takes it once per
/// rendered frame, so a stale one can never sound a frame late.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Event {
    Stowed,
    Retrieved,
    Dropped,
    Refused,
}

thread_local! {
    static EVENT: Cell<Option<Event>> = const { Cell::new(None) };
}

/// The event since the last call, consumed. Called by `Engine::ext_update` once per rendered
/// frame.
pub fn take_event() -> Option<Event> {
    EVENT.with(Cell::take)
}

fn fire(e: Event) {
    EVENT.with(|c| c.set(Some(e)));
}

/// The sound an event makes. Here rather than at the call site because which sound goes with
/// which event is a fact about the inventory, and a fifth event would have to answer it.
pub fn sfx(e: Event) -> crate::ext::audio::Sfx {
    use crate::ext::audio::Sfx;
    match e {
        Event::Stowed => Sfx::Stow,
        Event::Retrieved => Sfx::Retrieve,
        Event::Dropped => Sfx::Drop,
        Event::Refused => Sfx::Refuse,
    }
}

/// One frame's inventory input, latched by `Engine::run_frame` before the fixed-step loop
/// clears the key edges (`Input::end_frame`), exactly as the grab's E is.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Edges {
    /// `F`: stow what is in hand, or take the selected slot's item into it.
    pub stow: bool,
    /// `G`: put the selected slot's item down in front of the player.
    pub drop: bool,
    /// Mouse wheel since the last frame, in notches.
    pub wheel: f32,
}

/// One filled slot: the object, the word the HUD prints, and the carry's pin.
pub struct Slot {
    pub obj: Rc<RefCell<dyn ObjectT>>,
    pub label: &'static str,
    /// `k = p_scale / distance` at the moment of stowing, and the radius and `p_scale` that
    /// went with it (`ext/grab.rs`): the three numbers a retrieve needs to hand the object
    /// back at the size it went in at.
    ratio: f32,
    radius: f32,
    p_scale: f32,
}

/// A retrieve waiting for `room::apply_spawns` to put its object back in the object vector.
struct Pending {
    /// The slot the object is still sitting in until the spawn lands.
    from: usize,
    age: u32,
}

pub struct Inventory {
    slots: [Option<Slot>; CAPACITY],
    selected: usize,
    pending: Option<Pending>,
    /// Wheel notches not yet spent on a selection change, so a trackpad's many small deltas
    /// add up to exactly one step rather than being rounded away one at a time.
    wheel: f32,
    /// A refusal and the time it was made, held on the hint line for [`NOTICE_SECS`].
    notice: Option<(&'static str, f32)>,
}

impl Default for Inventory {
    fn default() -> Inventory {
        Inventory {
            slots: std::array::from_fn(|_| None),
            selected: 0,
            pending: None,
            wheel: 0.0,
            notice: None,
        }
    }
}

impl Inventory {
    /// Which slot the keys act on.
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// What the HUD prints, in slot order.
    pub fn labels(&self) -> [Option<&'static str>; CAPACITY] {
        std::array::from_fn(|i| self.slots[i].as_ref().map(|s| s.label))
    }

    /// The first empty slot, or `None` when all six are full.
    fn free_slot(&self) -> Option<usize> {
        self.slots.iter().position(Option::is_none)
    }

    /// Accumulate `wheel` notches and move the selection by whole ones.
    ///
    /// Scrolling away from you goes to the previous slot, the way a hotbar reads. The
    /// remainder is kept: a trackpad delivers a notch as a dozen fractions, and truncating
    /// each on its own would never reach one.
    pub fn scroll(&mut self, wheel: f32) {
        if !wheel.is_finite() {
            return;
        }
        self.wheel += wheel;
        let steps = self.wheel.trunc();
        if steps == 0.0 {
            return;
        }
        self.wheel -= steps;
        let n = CAPACITY as i32;
        let steps = steps.clamp(-(n as f32), n as f32) as i32;
        self.selected = (self.selected as i32 - steps).rem_euclid(n) as usize;
    }

    /// Empty every slot and forget everything about them.
    ///
    /// NOT called from a scene load. An elevator ride and a window crossing are loads too, and
    /// carrying things across them is the whole point (`ExtState::on_scene_loaded`). This is for
    /// the menu actions that mean a different run rather than somewhere else in this one --
    /// `MenuAction::starts_fresh`, applied by `Engine::apply_menu_action`. Without it NEW GAME
    /// began carrying the last game's loot, and RESTART LEVEL put a pocketed apple in the hall
    /// beside the one the level rebuilt.
    ///
    /// A retrieve in flight has already asked for its object's spawn; that request is withdrawn
    /// here, because applying it would drop the object into whatever scene loads next. Dropping
    /// the slot then drops the object itself, and a `RigidProp` gave its rapier body up on the
    /// stow, so nothing is left in the physics world either.
    pub fn clear(&mut self) {
        for slot in self.slots.iter().flatten() {
            room::withdraw_spawn(&slot.obj);
        }
        *self = Inventory::default();
    }

    /// Say why nothing happened, for [`NOTICE_SECS`].
    fn refuse(&mut self, text: &'static str) {
        self.notice = Some((text, crate::ext::view::time()));
        fire(Event::Refused);
        log::debug!("[inv] refused: {text}");
    }

    /// Keep a live refusal on the hint line. `insist` rather than `set`: it is about what the
    /// player just tried to do, which outranks whatever the crosshair happens to be on
    /// (`ext/hint.rs`).
    fn show_notice(&mut self) {
        let Some((text, at)) = self.notice else { return };
        if crate::ext::view::time() - at > NOTICE_SECS {
            self.notice = None;
            return;
        }
        hint::insist(text);
    }

    /// Put the object in hand into the first free slot.
    fn stow(&mut self, objects: &[Rc<RefCell<dyn ObjectT>>], grab: &mut GrabState) {
        let Some((_, obj)) = grab::held_object(objects, grab) else { return };
        let Ok(mut o) = obj.try_borrow_mut() else { return };
        if !o.can_stow() {
            drop(o);
            self.refuse(REFUSED_HINT);
            return;
        }
        let Some(slot) = self.free_slot() else {
            drop(o);
            self.refuse(FULL_HINT);
            return;
        };
        let label = o.stow_label();
        let p_scale = o.base().p_scale;
        o.on_stow();
        drop(o);

        // The pin as the carry left it, not as the object's own numbers imply: `ratio` is what
        // was measured at the pickup and is what makes the retrieve the same size.
        let (ratio, radius) = (grab.ratio, grab.radius);
        grab::release_without_dropping(grab);
        room::request_remove(&obj);
        self.slots[slot] = Some(Slot { obj, label, ratio, radius, p_scale });
        // Select what was just put away, so a second F is an undo.
        self.selected = slot;
        self.notice = None;
        fire(Event::Stowed);
        log::debug!("[inv] {label} stowed in slot {}", slot + 1);
    }

    /// Put the selected slot's object back into the world in front of the eye, and hand it to
    /// the spawn queue. Returns the placement, or `None` when the slot is empty.
    ///
    /// `into_hand` decides both where it goes and what it is told: a retrieve puts it at the
    /// distance its own pin implies and calls `on_grab`, so the body of a rigid prop is
    /// kinematic from its very first step rather than falling for the frame of flight; a drop
    /// puts it just clear of whatever the crosshair is on, gives gravity back, and lets go.
    fn unstow(
        &mut self,
        objects: &[Rc<RefCell<dyn ObjectT>>],
        cam_to_world: &Matrix4,
        into_hand: bool,
    ) -> Option<usize> {
        let ix = self.selected;
        let slot = self.slots[ix].as_ref()?;
        let origin = cam_to_world.translation();
        let dir = cam_to_world.mul_direction(Vector3::new(0.0, 0.0, -1.0)).normalized_safe();
        let world_r = slot.radius * slot.p_scale;

        let dist = if into_hand {
            // Where the stowed pin says this size belongs, kept clear of the eye.
            (slot.p_scale / slot.ratio.max(1e-6))
                .clamp(grab::player_floor(slot.ratio, slot.radius), grab::MAX_PLACE_DIST)
        } else {
            // An arm's length, or short of whatever is nearer -- a wall a hand's breadth away
            // must not receive the apple inside it.
            let reach = DROP_DIST.max(world_r + grab::PLAYER_CLEARANCE);
            raycast(objects, origin, dir, reach + world_r, None)
                .map_or(reach, |h| (h.dist - world_r).max(grab::FIT_D_MIN))
        };
        let pos = origin + dir * dist;

        let obj = Rc::clone(&slot.obj);
        let label = slot.label;
        {
            let Ok(mut o) = obj.try_borrow_mut() else { return None };
            // `set_position` moves `prev_pos` with `pos`, so the portal pass is never handed a
            // segment from the level this object was stowed in (module docs).
            if let Some(phys) = o.as_physical_mut() {
                phys.set_position(pos);
                phys.velocity = Vector3::zero();
                phys.gravity = if into_hand {
                    // The carry suspends gravity; `grab::release` puts it back at the drop.
                    Vector3::zero()
                } else {
                    Vector3::new(0.0, crate::game_header::GH_GRAVITY, 0.0)
                };
            } else {
                o.base_mut().pos = pos;
            }
            o.base_mut().p_scale = slot.p_scale;
            o.on_unstow();
            if into_hand {
                o.on_grab();
            }
        }
        room::request_spawn(Rc::clone(&obj));
        log::debug!(
            "[inv] {label} out of slot {} at {dist:.2} units{}",
            ix + 1,
            if into_hand { ", into the hand" } else { "" }
        );
        Some(ix)
    }

    /// `F` with an empty hand: take the selected item into it.
    fn retrieve(&mut self, objects: &[Rc<RefCell<dyn ObjectT>>], cam_to_world: &Matrix4) {
        if self.slots[self.selected].is_none() {
            self.refuse(EMPTY_HINT);
            return;
        }
        let Some(from) = self.unstow(objects, cam_to_world, true) else { return };
        self.pending = Some(Pending { from, age: 0 });
        self.notice = None;
        fire(Event::Retrieved);
    }

    /// `G`: put the selected item down in front of the player, without going through the hand.
    fn drop_selected(&mut self, objects: &[Rc<RefCell<dyn ObjectT>>], cam_to_world: &Matrix4) {
        if self.slots[self.selected].is_none() {
            self.refuse(EMPTY_HINT);
            return;
        }
        let Some(ix) = self.unstow(objects, cam_to_world, false) else { return };
        // Nothing more has to happen to it, so the slot empties now rather than on the frame
        // the spawn lands.
        self.slots[ix] = None;
        self.notice = None;
        fire(Event::Dropped);
    }

    /// Finish a retrieve whose object has arrived in the object vector.
    fn resolve_pending(&mut self, objects: &[Rc<RefCell<dyn ObjectT>>], grab: &mut GrabState) {
        let Some(p) = self.pending.as_mut() else { return };
        let Some(slot) = self.slots[p.from].as_ref() else {
            self.pending = None;
            return;
        };
        let found = objects.iter().position(|o| Rc::ptr_eq(o, &slot.obj));
        let Some(idx) = found else {
            p.age += 1;
            if p.age > PENDING_FRAMES {
                log::warn!("[inv] {} never reached the scene; it stays in its slot", slot.label);
                self.pending = None;
            }
            return;
        };
        // The object was placed and told when the retrieve was asked for; this only pins the
        // carry to the size it was stowed at.
        grab::hold_retrieved(objects, idx, slot.ratio, slot.radius, slot.p_scale, grab);
        let from = p.from;
        self.slots[from] = None;
        self.pending = None;
    }
}

/// One frame of inventory logic.
///
/// Called from `Engine::ext_update` BEFORE `grab::update`, so a retrieve's first carried frame
/// is the frame it lands and a stow has already emptied the hand before the carry would have
/// moved the object again.
pub fn update(
    objects: &[Rc<RefCell<dyn ObjectT>>],
    cam_to_world: &Matrix4,
    edges: Edges,
    grab: &mut GrabState,
    inv: &mut Inventory,
) {
    inv.scroll(edges.wheel);
    inv.resolve_pending(objects, grab);
    // A retrieve in flight owns the hand and the slot it came from; both keys wait one frame.
    if inv.pending.is_none() {
        if edges.stow {
            if grab.held.is_some() {
                inv.stow(objects, grab);
            } else {
                inv.retrieve(objects, cam_to_world);
            }
        }
        if edges.drop {
            inv.drop_selected(objects, cam_to_world);
        }
    }
    inv.show_notice();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::Object;
    use crate::physical::Physical;

    /// A stowable thing that counts what it has been told.
    struct Item {
        base: Physical,
        label: &'static str,
        stows: u32,
        unstows: u32,
        grabs: u32,
        allowed: bool,
    }

    impl Item {
        fn new(label: &'static str, allowed: bool) -> Rc<RefCell<Item>> {
            let mut base = Physical::new();
            base.hit_spheres.push(crate::sphere::Sphere::new_at(Vector3::zero(), 0.2));
            Rc::new(RefCell::new(Item { base, label, stows: 0, unstows: 0, grabs: 0, allowed }))
        }
    }

    impl ObjectT for Item {
        fn base(&self) -> &Object {
            &self.base.base
        }
        fn base_mut(&mut self) -> &mut Object {
            &mut self.base.base
        }
        fn as_physical(&self) -> Option<&Physical> {
            Some(&self.base)
        }
        fn as_physical_mut(&mut self) -> Option<&mut Physical> {
            Some(&mut self.base)
        }
        fn on_grab(&mut self) {
            self.grabs += 1;
        }
        fn on_stow(&mut self) {
            self.stows += 1;
        }
        fn on_unstow(&mut self) {
            self.unstows += 1;
        }
        fn can_stow(&self) -> bool {
            self.allowed
        }
        fn stow_label(&self) -> &'static str {
            self.label
        }
    }

    /// A camera at the origin looking along -z, as `Player::cam_to_world` builds it at yaw 0.
    fn cam() -> Matrix4 {
        Matrix4::trans(Vector3::new(0.0, 1.5, 0.0))
    }

    /// A scene of one item, and a grab already carrying it.
    fn carrying(item: &Rc<RefCell<Item>>) -> (Vec<Rc<RefCell<dyn ObjectT>>>, GrabState) {
        let scene: Vec<Rc<RefCell<dyn ObjectT>>> =
            vec![Rc::clone(item) as Rc<RefCell<dyn ObjectT>>];
        let grab = GrabState { held: Some(0), ratio: 2.0, radius: 0.2, ..GrabState::default() };
        item.borrow_mut().base.base.p_scale = 1.0;
        (scene, grab)
    }

    /// Drain the channels a test is not looking at, so one test cannot see another's leavings.
    fn drain() {
        take_event();
        hint::take();
        let mut empty: Vec<Rc<RefCell<dyn ObjectT>>> = Vec::new();
        room::apply_removes(&mut empty);
        room::apply_spawns(&mut empty);
    }

    #[test]
    fn stowing_empties_the_hand_and_keeps_the_object() {
        drain();
        let apple = Item::new("APPLE", true);
        let (scene, mut grab) = carrying(&apple);
        let mut inv = Inventory::default();
        update(&scene, &cam(), Edges { stow: true, ..Edges::default() }, &mut grab, &mut inv);

        assert!(grab.held.is_none(), "the hand is empty");
        assert!(!grab.just_released, "a stow is not a drop");
        assert_eq!(inv.labels()[0], Some("APPLE"));
        assert_eq!(inv.selected(), 0);
        assert_eq!(apple.borrow().stows, 1);
        assert_eq!(take_event(), Some(Event::Stowed));
        // Its removal was asked for by identity, and the slot still owns it afterwards.
        let mut world = scene.clone();
        assert_eq!(room::apply_removes(&mut world), [0]);
        assert!(world.is_empty());
        drop(scene);
        assert_eq!(Rc::strong_count(&apple), 2, "the slot and this test hold it");
        drain();
    }

    #[test]
    fn a_full_inventory_refuses_the_seventh_gracefully() {
        drain();
        let mut inv = Inventory::default();
        for i in 0..CAPACITY {
            let item = Item::new("ITEM", true);
            let (scene, mut grab) = carrying(&item);
            update(&scene, &cam(), Edges { stow: true, ..Edges::default() }, &mut grab, &mut inv);
            assert!(inv.labels()[i].is_some());
        }
        let seventh = Item::new("SEVENTH", true);
        let (scene, mut grab) = carrying(&seventh);
        update(&scene, &cam(), Edges { stow: true, ..Edges::default() }, &mut grab, &mut inv);
        assert_eq!(grab.held, Some(0), "still in hand");
        assert_eq!(seventh.borrow().stows, 0);
        assert_eq!(take_event(), Some(Event::Refused));
        assert_eq!(hint::take().as_deref(), Some(FULL_HINT));
        drain();
    }

    #[test]
    fn something_that_refuses_to_be_stowed_stays_in_the_hand() {
        drain();
        let window = Item::new("WINDOW", false);
        let (scene, mut grab) = carrying(&window);
        let mut inv = Inventory::default();
        update(&scene, &cam(), Edges { stow: true, ..Edges::default() }, &mut grab, &mut inv);
        assert_eq!(grab.held, Some(0));
        assert!(inv.labels().iter().all(Option::is_none));
        assert_eq!(window.borrow().stows, 0);
        assert_eq!(take_event(), Some(Event::Refused));
        assert_eq!(hint::take().as_deref(), Some(REFUSED_HINT));
        drain();
    }

    #[test]
    fn a_retrieve_lands_the_frame_after_the_spawn_and_keeps_the_apparent_size() {
        drain();
        let apple = Item::new("APPLE", true);
        let (scene, mut grab) = carrying(&apple);
        apple.borrow_mut().base.base.p_scale = 1.5;
        let mut inv = Inventory::default();
        let take = Edges { stow: true, ..Edges::default() };
        update(&scene, &cam(), take, &mut grab, &mut inv);

        // Out of the world; the hand is empty.
        let mut world: Vec<Rc<RefCell<dyn ObjectT>>> = scene.clone();
        room::apply_removes(&mut world);
        take_event();

        // F again: the spawn is asked for, the object told, the slot still holds it.
        update(&world, &cam(), take, &mut grab, &mut inv);
        assert_eq!(take_event(), Some(Event::Retrieved));
        assert!(grab.held.is_none(), "not in hand until the spawn lands");
        assert_eq!(inv.labels()[0], Some("APPLE"));
        {
            let a = apple.borrow();
            assert_eq!(a.unstows, 1);
            assert_eq!(a.grabs, 1);
            assert_eq!(a.base.base.p_scale, 1.5, "the size it was stowed at");
            assert!(
                (a.base.prev_pos - a.base.base.pos).mag() == 0.0,
                "no segment for the portal pass"
            );
        }

        // The engine applies the spawn between steps; the next frame pins the carry.
        room::apply_spawns(&mut world);
        assert_eq!(world.len(), 1);
        update(&world, &cam(), Edges::default(), &mut grab, &mut inv);
        assert_eq!(grab.held, Some(0));
        assert_eq!(grab.ratio, 2.0, "the pin it was stowed with");
        assert_eq!(grab.eased_scale, 1.5);
        assert!(inv.labels()[0].is_none(), "the slot is empty once it is in hand");
        assert_eq!(apple.borrow().grabs, 1, "told once, not twice");
        drain();
    }

    #[test]
    fn dropping_puts_it_in_the_world_with_gravity_back_on() {
        drain();
        let die = Item::new("DIE", true);
        let (scene, mut grab) = carrying(&die);
        let mut inv = Inventory::default();
        update(&scene, &cam(), Edges { stow: true, ..Edges::default() }, &mut grab, &mut inv);
        let mut world: Vec<Rc<RefCell<dyn ObjectT>>> = scene.clone();
        room::apply_removes(&mut world);
        take_event();

        update(&world, &cam(), Edges { drop: true, ..Edges::default() }, &mut grab, &mut inv);
        assert_eq!(take_event(), Some(Event::Dropped));
        assert!(grab.held.is_none(), "a drop never touches the hand");
        assert!(inv.labels()[0].is_none());
        {
            let d = die.borrow();
            assert_eq!(d.unstows, 1);
            assert_eq!(d.grabs, 0, "not grabbed");
            assert!(d.base.gravity.y < 0.0, "it falls");
            // In front of the eye, down the crosshair (-z at yaw 0).
            assert!(d.base.base.pos.z < -0.2 && d.base.base.pos.y == 1.5);
        }
        room::apply_spawns(&mut world);
        assert_eq!(world.len(), 1);
        drain();
    }

    /// F and G on the same rendered frame used to annihilate the item: the stow's remove and
    /// the drop's spawn named the same `Rc`, `apply_spawns` appended a second handle and
    /// `apply_removes` then took both copies out. `room::request_spawn` cancels the pending
    /// remove now, so the object ends in the world or in a slot -- never in neither.
    #[test]
    fn a_stow_and_a_drop_in_one_frame_cannot_destroy_the_item() {
        drain();
        let apple = Item::new("APPLE", true);
        let (scene, mut grab) = carrying(&apple);
        let mut inv = Inventory::default();
        let both = Edges { stow: true, drop: true, ..Edges::default() };
        update(&scene, &cam(), both, &mut grab, &mut inv);

        // The engine applies the queues in this order, between fixed steps.
        let mut world: Vec<Rc<RefCell<dyn ObjectT>>> = scene.clone();
        room::apply_spawns(&mut world);
        room::apply_removes(&mut world);

        let in_world = world.iter().filter(|o| Rc::ptr_eq(o, &scene[0])).count();
        let in_slot = inv.labels().iter().filter(|l| l.is_some()).count();
        assert_eq!(in_world + in_slot, 1, "the item is somewhere, exactly once");
        assert_eq!(in_world, 1, "G put it down, so it is the world that has it");
        assert!(grab.held.is_none(), "and not in the hand");
        assert!(apple.borrow().base.gravity.y < 0.0, "with gravity back on");
        drain();
    }

    /// The same hazard from the other side: two F presses with no fixed step between them --
    /// the stow, then the retrieve -- used to leave the prop outside the scene with a live
    /// rapier body, which is the exact leak `on_stow` exists to prevent.
    #[test]
    fn two_stows_before_the_queues_are_applied_cannot_strand_the_item() {
        drain();
        let apple = Item::new("APPLE", true);
        let (scene, mut grab) = carrying(&apple);
        let mut inv = Inventory::default();
        let take = Edges { stow: true, ..Edges::default() };
        // Frame N stows; frame N+1 retrieves. A rendered frame shorter than one 2 ms step runs
        // no step at all, so the engine has applied neither queue and the object is still in
        // the vector both times.
        update(&scene, &cam(), take, &mut grab, &mut inv);
        update(&scene, &cam(), take, &mut grab, &mut inv);

        let mut world: Vec<Rc<RefCell<dyn ObjectT>>> = scene.clone();
        room::apply_spawns(&mut world);
        room::apply_removes(&mut world);
        assert_eq!(world.len(), 1, "it never left the scene, and was never doubled");

        // And the retrieve still lands: the next frame finds it and pins the carry.
        update(&world, &cam(), Edges::default(), &mut grab, &mut inv);
        assert_eq!(grab.held, Some(0));
        assert!(inv.labels().iter().all(Option::is_none), "the slot let go once it was in hand");
        let a = apple.borrow();
        assert_eq!((a.stows, a.unstows, a.grabs), (1, 1, 1), "told once each, not left half-told");
        drop(a);
        drain();
    }

    #[test]
    fn an_empty_slot_says_so() {
        drain();
        let mut inv = Inventory::default();
        let mut grab = GrabState::default();
        update(&[], &cam(), Edges { stow: true, ..Edges::default() }, &mut grab, &mut inv);
        assert_eq!(take_event(), Some(Event::Refused));
        assert_eq!(hint::take().as_deref(), Some(EMPTY_HINT));
        drain();
    }

    /// What NEW GAME and MAIN MENU do: the slots empty, the selection goes back to the first,
    /// and a retrieve that had already asked for its spawn does not land in the next scene.
    #[test]
    fn clearing_empties_the_slots_and_withdraws_a_retrieve_in_flight() {
        drain();
        let apple = Item::new("APPLE", true);
        let (scene, mut grab) = carrying(&apple);
        let mut inv = Inventory::default();
        let take = Edges { stow: true, ..Edges::default() };
        update(&scene, &cam(), take, &mut grab, &mut inv);
        let mut world: Vec<Rc<RefCell<dyn ObjectT>>> = scene.clone();
        room::apply_removes(&mut world);
        assert!(world.is_empty());
        // F again asks for the spawn; the slot still holds the apple until it lands.
        update(&world, &cam(), take, &mut grab, &mut inv);
        assert_eq!(inv.labels()[0], Some("APPLE"));

        inv.clear();
        assert!(inv.labels().iter().all(Option::is_none));
        assert_eq!(inv.selected(), 0);
        room::apply_spawns(&mut world);
        assert!(world.is_empty(), "the queued spawn was withdrawn, not left for the next scene");
        drop(scene);
        assert_eq!(Rc::strong_count(&apple), 1, "only this test still holds it");
        drain();
    }

    /// Every event has a sound of its own, and no two share one -- the four files exist so that
    /// a refusal does not sound like a put-down.
    #[test]
    fn every_event_has_its_own_sound() {
        use crate::ext::audio::Sfx;
        let all = [Event::Stowed, Event::Retrieved, Event::Dropped, Event::Refused];
        assert_eq!(sfx(Event::Stowed), Sfx::Stow);
        assert_eq!(sfx(Event::Retrieved), Sfx::Retrieve);
        assert_eq!(sfx(Event::Dropped), Sfx::Drop);
        assert_eq!(sfx(Event::Refused), Sfx::Refuse);
        let mut sounds: Vec<Sfx> = all.iter().map(|&e| sfx(e)).collect();
        sounds.dedup();
        assert_eq!(sounds.len(), all.len());
    }

    #[test]
    fn the_wheel_wraps_and_spends_whole_notches_only() {
        let mut inv = Inventory::default();
        inv.scroll(-1.0);
        assert_eq!(inv.selected(), 1, "toward you is the next slot");
        inv.scroll(1.0);
        assert_eq!(inv.selected(), 0);
        inv.scroll(1.0);
        assert_eq!(inv.selected(), CAPACITY - 1, "wraps at both ends");
        // A trackpad's fractions add up to exactly one step and no more.
        inv.selected = 0;
        for _ in 0..3 {
            inv.scroll(-0.3);
            assert_eq!(inv.selected(), 0);
        }
        inv.scroll(-0.3);
        assert_eq!(inv.selected(), 1);
        inv.scroll(f32::NAN);
        assert_eq!(inv.selected(), 1, "a NaN delta is ignored");
    }
}
