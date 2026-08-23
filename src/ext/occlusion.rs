//! EXT: one-frame-late portal occlusion queries. Not part of the C++ port.
//!
//! The ported renderer draws each portal's quad inside a `GL_SAMPLES_PASSED` query and reads
//! the count back **in the same pass** to decide whether to recurse into it (Engine.cpp:233-251).
//! That read is a full CPU-GPU round trip: the driver has to finish every command queued so
//! far before it can answer, so the CPU idles for the whole of the GPU's backlog, once per
//! pass that has a portal in view. It was most of the main thread's time.
//!
//! Here the queries are still issued every frame, but the answer used is **last frame's**,
//! which the GPU has long since produced. Every query is stamped with the frame it was issued
//! on, and the rule for the decision ([`SlotPolicy::decide`]) is:
//!
//! * a query was issued for this slot **on the previous frame exactly** and its result is
//!   available: use it -- hidden if it passed no samples, visible otherwise;
//! * otherwise -- never issued, issued two or more frames ago (the portal was out of the
//!   frustum since, so the slot was not asked), or the GPU has not caught up -- treat the
//!   portal as **visible** and draw it.
//!
//! The frame stamp is what makes the second rule safe. Without it a slot would keep the
//! result of the last query it ever ran -- from any number of frames ago, while its portal was
//! out of frame -- and hand that back the frame the portal came back into view: "0 samples"
//! from a view that no longer exists, and a portal missing for one frame. A result is only
//! ever believed when it is one frame old.
//!
//! So the scheme only ever errs toward drawing. What changes on screen: a portal that becomes
//! fully hidden is drawn for one extra frame (harmless: it is hidden), and a portal that is
//! *uncovered* after being fully hidden is drawn from the frame after it appears rather than
//! the frame it appears -- the uncovered sliver is one frame's motion wide, and for that frame
//! the quad shows what the pass drew behind it. A portal entering the view from outside the
//! frustum is unaffected: the CPU pre-test in `Engine::render` settles that in the current
//! frame, and its slot's last result is older than a frame, so it is visible by the rule.
//!
//! # Slots
//!
//! A slot is one portal seen from one pass, and a pass is named by the chain of portals it is
//! seen through: the main view is the empty chain; the pass inside portal 2 is `[2]`; the pass
//! inside portal 0 as seen through portal 2 is `[2, 0]`. The name is what makes last frame's
//! result mean the same thing as this frame's -- the same portal seen through two different
//! portals at the same recursion level is two different views of it, with two different
//! answers, and a slot keyed on the recursion level alone would hand one view the other's.
//! `GH_MAX_RECURSION` bounds the chain at three links and `GH_MAX_PORTALS` each link at 16, so
//! a name packs into a few bits of an integer.
//!
//! # Frames
//!
//! The engine ticks [`Occlusion::next_frame`] once at the top of `run_frame`, before any render
//! pass; every pass of that frame -- the main view and each nested portal view -- carries the
//! same stamp. A slot is asked at most once per frame, because a pass name occurs once per
//! frame, so "issued on the previous frame" and "asked on the previous frame" are the same
//! thing.

use std::collections::HashMap;
use std::rc::Rc;

use glow::HasContext;

use crate::game_header::{GH_MAX_PORTALS, GH_MAX_RECURSION};

/// A pass's name: the chain of portal indices it is seen through, packed little-end first.
pub type Path = u32;

/// The main view: seen through no portal at all.
pub const ROOT: Path = 0;

/// Bits per link; holds `GH_MAX_PORTALS` indices plus the "no link" zero.
const LINK_BITS: u32 = 5;

/// The name of the pass inside portal `i` of the pass named `path`.
pub fn push(path: Path, i: usize) -> Path {
    debug_assert!(i < GH_MAX_PORTALS && (1 << LINK_BITS) > GH_MAX_PORTALS);
    (path << LINK_BITS) | (i as Path + 1)
}

/// The decision rule, kept apart from the GL so it can be tested without a context.
pub struct SlotPolicy;

impl SlotPolicy {
    /// Whether a portal should be drawn on frame `now`, given that its slot's query was last
    /// issued on `issued` (`None` if never), whether that query's result is `available`, and
    /// the `samples` it counted if so. Hidden only on a one-frame-old, available, zero-sample
    /// result; visible in every other case.
    pub fn decide(now: u64, issued: Option<u64>, available: bool, samples: u32) -> bool {
        match issued {
            // Exactly one frame old: the only result that describes the view being drawn.
            Some(f) if f + 1 == now => !available || samples > 0,
            // Never asked, or asked about some older view: nothing to go on, so draw.
            _ => true,
        }
    }
}

/// Per-slot state: the GL query object, and the frame its query was last issued on.
struct Slot {
    query: glow::Query,
    issued: Option<u64>,
}

pub struct Occlusion {
    gl: Rc<glow::Context>,
    /// Keyed on `(path, portal index)`. Grown on demand; the set of slots a scene uses settles
    /// within a few frames, and the queries are kept until the engine is torn down.
    slots: HashMap<(Path, usize), Slot>,
    /// Number of portals the slots were issued against; a change makes every index mean
    /// something else, so every stored result is dropped.
    portal_count: usize,
    /// The frame in flight: what a query issued now is stamped with, and what a stored stamp
    /// is judged against. Ticked by `next_frame`.
    frame: u64,
}

impl Occlusion {
    pub fn new(gl: &Rc<glow::Context>) -> Occlusion {
        // Three links fit comfortably; this only matters if GH_MAX_RECURSION grows past what
        // a u32 path can name.
        debug_assert!((GH_MAX_RECURSION as u32 - 1) * LINK_BITS <= u32::BITS);
        Occlusion {
            gl: Rc::clone(gl),
            slots: HashMap::new(),
            portal_count: 0,
            // Frame 0 is never "the previous frame" of anything (no stamp precedes it), which
            // is the right answer for the very first frame: nothing has been asked yet.
            frame: 1,
        }
    }

    /// Begin a new frame: everything issued from now on is stamped with it, and only results
    /// issued on the frame just ended will be believed. Once per `Engine::run_frame`, ahead of
    /// every render pass.
    pub fn next_frame(&mut self) {
        self.frame += 1;
    }

    /// Forget every stored result. On a scene load the portal vector is rebuilt, so index
    /// `i` names a different portal than it did; the queries themselves are reusable.
    pub fn reset(&mut self) {
        for s in self.slots.values_mut() {
            s.issued = None;
        }
    }

    /// Whether the portal at index `i`, seen from the pass `path`, passed any samples the
    /// last time it was queried -- `true` unless a query issued on the previous frame says
    /// otherwise (`SlotPolicy::decide`). Never stalls: the result is only fetched once the
    /// driver reports it available, and a result that is not is treated as "visible".
    pub fn visible(&mut self, path: Path, i: usize, portal_count: usize) -> bool {
        if portal_count != self.portal_count {
            self.portal_count = portal_count;
            self.reset();
        }
        let now = self.frame;
        let Some(slot) = self.slots.get_mut(&(path, i)) else {
            return SlotPolicy::decide(now, None, false, 0);
        };
        // Only a one-frame-old stamp is worth the two driver calls; anything else the policy
        // answers "visible" without them.
        let (available, samples) = match slot.issued {
            Some(f) if f + 1 == now => unsafe {
                let gl = &self.gl;
                if gl.get_query_parameter_u32(slot.query, glow::QUERY_RESULT_AVAILABLE) == 0 {
                    (false, 0)
                } else {
                    (true, gl.get_query_parameter_u32(slot.query, glow::QUERY_RESULT))
                }
            },
            _ => (false, 0),
        };
        SlotPolicy::decide(now, slot.issued, available, samples)
    }

    /// Start a `SAMPLES_PASSED` query for the slot, stamped with the current frame; `end`
    /// closes it. The result is read by `visible` next frame. Beginning a query on an object
    /// whose previous result was never read simply discards that result, which is what a
    /// stale or unavailable one deserves.
    pub fn begin(&mut self, path: Path, i: usize) {
        let gl = &self.gl;
        let slot = self.slots.entry((path, i)).or_insert_with(|| Slot {
            query: unsafe { gl.create_query().expect("glGenQueries failed") },
            issued: None,
        });
        slot.issued = Some(self.frame);
        unsafe {
            gl.begin_query(glow::SAMPLES_PASSED, slot.query);
        }
    }

    pub fn end(&self) {
        unsafe {
            self.gl.end_query(glow::SAMPLES_PASSED);
        }
    }

    /// Delete the query objects. Called from `Engine::destroy_gl_objects`, while the context is
    /// current.
    pub fn destroy(&mut self) {
        unsafe {
            for (_, s) in self.slots.drain() {
                self.gl.delete_query(s.query);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_name_each_chain_uniquely() {
        let a = push(ROOT, 2);
        let b = push(a, 0);
        let c = push(push(ROOT, 0), 2);
        assert_ne!(a, ROOT);
        assert_ne!(a, b);
        assert_ne!(b, c, "[2, 0] and [0, 2] are different views");
        // Index 0 is distinguishable from "no link".
        assert_ne!(push(ROOT, 0), ROOT);
        assert_ne!(push(push(ROOT, 0), 0), push(ROOT, 0));
        // The deepest chain the engine can build still fits.
        let mut deep = ROOT;
        for _ in 0..GH_MAX_RECURSION - 1 {
            deep = push(deep, GH_MAX_PORTALS - 1);
        }
        assert!(deep > 0);
    }

    /// A slot that has never been asked has nothing to say against drawing.
    #[test]
    fn never_issued_is_visible() {
        assert!(SlotPolicy::decide(10, None, false, 0));
        assert!(SlotPolicy::decide(10, None, true, 0));
    }

    /// The one case that hides: last frame's query came back with no samples.
    #[test]
    fn zero_samples_from_last_frame_hides() {
        assert!(!SlotPolicy::decide(10, Some(9), true, 0));
        // And any sample at all from last frame draws.
        assert!(SlotPolicy::decide(10, Some(9), true, 1));
        assert!(SlotPolicy::decide(10, Some(9), true, 4096));
    }

    /// A result from two or more frames ago describes a view that no longer exists -- the
    /// portal was out of the frustum since -- and must not hide the portal on re-entry.
    #[test]
    fn stale_results_are_visible() {
        assert!(SlotPolicy::decide(10, Some(8), true, 0));
        assert!(SlotPolicy::decide(10, Some(1), true, 0));
        // A stamp from the current frame (or later) is not "last frame" either; the engine
        // never produces one, but the rule stays on the side of drawing if it did.
        assert!(SlotPolicy::decide(10, Some(10), true, 0));
        assert!(SlotPolicy::decide(10, Some(11), true, 0));
    }

    /// The GPU has not caught up: draw rather than wait.
    #[test]
    fn unavailable_result_is_visible() {
        assert!(SlotPolicy::decide(10, Some(9), false, 0));
    }
}
