//! EXT: Hide 'N Dream -- the offline Duel, v0 of the multiplayer pivot.
//!
//! One seeker (the player) hunts one **Dreamer**: a prop that is secretly alive. The Dreamer
//! creeps toward an exit ONLY while unobserved -- the Chameleon Rule (`ext/visibility.rs`),
//! which is Level10's statue machinery wearing a disguise. Grabbing the Dreamer catches it;
//! if it reaches the exit, or the clock runs out, the dream keeps its secret.
//!
//! Zero netcode: this module exists to prove the round is fun before a socket is written
//! (docs/hide-n-dream.md §7). The multiplayer round machine will replace [`duel_logic`];
//! the [`Dreamer`]'s chameleon update is already written against `is_observed_by_any`, so
//! handing it more eyes later is a one-line change at the call site.
//!
//! # Channel
//!
//! The round result rides the same ambient-channel shape as `ext/room.rs`: the Dreamer
//! reports how the round ended, the [`duel_logic`] object takes it. First report wins --
//! being caught in the same step you touch the exit resolves in the seeker's favor only if
//! the grab hook ran first, which is the order the engine already fixes (object updates run
//! before the grab pass consumes E).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::camera::Camera;
use crate::ext::grab::Grabbable;
use crate::ext::hint;
use crate::ext::room::{self, RoomLogic};
use crate::ext::visibility::is_observed_by_any;
use crate::object::{Object, ObjectT, RenderCtx, UpdateCtx};
use crate::physical::Physical;
use crate::vector::Vector3;

/// World units the Dreamer creeps per fixed step while unobserved: Level10's statue speed,
/// verbatim (~0.6 u/s at 500 Hz) -- noticeable, never caught in the act.
const CREEP_SPEED: f32 = 0.0012;
/// Reaching this close to the exit (horizontally) is an escape.
const EXIT_RADIUS: f32 = 1.0;
/// Fixed steps in a Duel seek phase: 180 s at 500 Hz.
const SEEK_STEPS: u32 = 180 * 500;
/// Fixed steps the result line stays up before the next round loads.
const RESOLVE_STEPS: u32 = 5 * 500;

/// How a Duel round ended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DuelEnd {
    /// The seeker grabbed the Dreamer.
    Caught,
    /// The Dreamer reached the exit.
    Escaped,
    /// The clock ran out with the Dreamer still hidden.
    TimeUp,
}

thread_local! {
    /// The end a round reported this scene, if any. Cleared by [`duel_logic`]'s constructor,
    /// so a scene reload always starts clean.
    static END: Cell<Option<DuelEnd>> = const { Cell::new(None) };
}

/// Report how the round ended. The first report wins; later ones are dropped.
pub fn report_end(end: DuelEnd) {
    END.with(|c| {
        if c.get().is_none() {
            c.set(Some(end));
        }
    });
}

fn take_end() -> Option<DuelEnd> {
    END.with(|c| c.take())
}

/// A prop that is secretly alive: a [`Grabbable`] in every respect the seeker can test --
/// it picks up, throws, falls, collides -- plus a will of its own while nobody watches.
pub struct Dreamer {
    inner: Grabbable,
    /// Where it is trying to go.
    exit: Vector3,
    /// What the line-of-sight test raycasts against (the level shell; never the Dreamer).
    blockers: Vec<Rc<RefCell<dyn ObjectT>>>,
    /// Set once the round is decided; the disguise goes inert.
    done: bool,
}

impl Dreamer {
    pub fn new(radius: f32, exit: Vector3, blockers: Vec<Rc<RefCell<dyn ObjectT>>>) -> Dreamer {
        Dreamer { inner: Grabbable::new(radius), exit, blockers, done: false }
    }

    /// The visual `Object`, for the scene to dress exactly like its innocent twins.
    pub fn obj_mut(&mut self) -> &mut Object {
        self.inner.obj_mut()
    }

    /// Place it (position and `prev_pos` together, like every spawn must).
    pub fn set_position(&mut self, pos: Vector3) {
        self.inner.base.set_position(pos);
    }
}

impl ObjectT for Dreamer {
    fn base(&self) -> &Object {
        self.inner.base()
    }
    fn base_mut(&mut self) -> &mut Object {
        self.inner.base_mut()
    }
    fn on_collide(&mut self, push: Vector3) {
        self.inner.on_collide(push);
    }
    fn as_physical(&self) -> Option<&Physical> {
        self.inner.as_physical()
    }
    fn as_physical_mut(&mut self) -> Option<&mut Physical> {
        self.inner.as_physical_mut()
    }
    fn draw(&self, ctx: &RenderCtx, cam: &Camera, fbo: Option<glow::Framebuffer>) {
        self.inner.draw(ctx, cam, fbo);
    }

    fn update(&mut self, ctx: &UpdateCtx) {
        // Ordinary prop physics first: it falls, settles, and can be thrown like anything.
        self.inner.update(ctx);
        if self.done {
            return;
        }

        let pos = self.inner.base.base.pos;
        let mut to_exit = self.exit - pos;
        to_exit.y = 0.0;
        let dist = to_exit.mag();
        if dist <= EXIT_RADIUS {
            self.done = true;
            report_end(DuelEnd::Escaped);
            return;
        }

        // The Chameleon Rule: hold perfectly still under any watching eye. One eye today;
        // netplay will pass every seeker's here.
        let eyes = [ctx.cam_to_world];
        if is_observed_by_any(&self.blockers, &eyes, pos, None) {
            return;
        }

        // Creep. Position is written directly, statue-style: the segment this leaves for the
        // next step's portal pass is real, so a Dreamer that slips through a doorway portal
        // is relocated exactly like any loose traveler -- in the Floorplan, that is a hide.
        // Deliberately no facing change: a teapot that turned to look at the exit would be
        // a tell, and the disguise stays a disguise.
        self.inner.base.base.pos += to_exit / dist * CREEP_SPEED;
    }

    /// The seeker's hand closing around it IS the tag.
    fn on_grab(&mut self) {
        if !self.done {
            self.done = true;
            report_end(DuelEnd::Caught);
        }
    }
}

/// The round clock and result card, as a `RoomLogic`.
///
/// `scene_index` is what to reload for the next round; `disguise` names the Dreamer's prop
/// kind for the reveal line (uppercase, it goes to the hint HUD).
pub fn duel_logic(scene_index: usize, disguise: &'static str) -> RoomLogic {
    // A fresh round: whatever a previous scene reported is stale.
    let _ = take_end();

    let mut steps_left: u32 = SEEK_STEPS;
    let mut over: Option<(DuelEnd, u32)> = None;

    RoomLogic::new(move |_ctx| {
        if let Some((end, ref mut wait)) = over {
            hint::set(match end {
                DuelEnd::Caught => format!("CAUGHT! THE {disguise} WAS DREAMING"),
                DuelEnd::Escaped => "THE DREAMER REACHED THE EXIT".to_string(),
                DuelEnd::TimeUp => "TIME UP - THE DREAM KEEPS ITS SECRET".to_string(),
            });
            *wait = wait.saturating_sub(1);
            if *wait == 0 {
                room::request_scene_load(scene_index);
            }
            return;
        }

        if let Some(end) = take_end() {
            over = Some((end, RESOLVE_STEPS));
            return;
        }

        steps_left = steps_left.saturating_sub(1);
        if steps_left == 0 {
            over = Some((DuelEnd::TimeUp, RESOLVE_STEPS));
            return;
        }
        let secs = steps_left / 500;
        hint::set(format!("SEEK {}:{:02} - SOMETHING HERE DREAMS", secs / 60, secs % 60));
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_report_wins() {
        let _ = take_end();
        report_end(DuelEnd::Caught);
        report_end(DuelEnd::Escaped);
        assert_eq!(take_end(), Some(DuelEnd::Caught));
        assert_eq!(take_end(), None, "taking consumes the report");
    }

    #[test]
    fn duel_logic_clears_stale_reports() {
        report_end(DuelEnd::Escaped);
        let _logic = duel_logic(0, "TEAPOT");
        assert_eq!(take_end(), None, "a new round must not inherit the last round's end");
    }

    /// The reveal line must fit the hint HUD's 40-char legibility cap for every disguise.
    #[test]
    fn reveal_lines_fit_the_hint_budget() {
        for kind in ["TEAPOT", "MONKEY", "RABBIT"] {
            assert!(format!("CAUGHT! THE {kind} WAS DREAMING").len() <= 40);
        }
        assert!("TIME UP - THE DREAM KEEPS ITS SECRET".len() <= 40);
        assert!("SEEK 2:59 - SOMETHING HERE DREAMS".len() <= 40);
    }
}
