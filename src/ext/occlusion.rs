//! EXT: one-frame-late portal occlusion queries. Not part of the C++ port.
//!
//! The ported renderer draws each portal's quad inside a `GL_SAMPLES_PASSED` query and reads
//! the count back **in the same pass** to decide whether to recurse into it (Engine.cpp:233-251).
//! That read is a full CPU-GPU round trip: the driver has to finish every command queued so
//! far before it can answer, so the CPU idles for the whole of the GPU's backlog, once per
//! pass that has a portal in view. It was most of the main thread's time.
//!
//! Here the queries are still issued every frame, but the answer used is **last frame's**,
//! which the GPU has long since produced. The rule for the decision is:
//!
//! * a query was issued for this slot last frame and its result is available: use it;
//! * otherwise -- never issued, the slot was out of view last frame, or the GPU has not
//!   caught up -- treat the portal as **visible** and draw it.
//!
//! So the scheme only ever errs toward drawing. What changes on screen: a portal that becomes
//! fully hidden is drawn for one extra frame (harmless: it is hidden), and a portal that is
//! *uncovered* after being fully hidden is drawn from the frame after it appears rather than
//! the frame it appears -- the uncovered sliver is one frame's motion wide, and for that frame
//! the quad shows what the pass drew behind it. A portal entering the view from outside the
//! frustum is unaffected: the CPU pre-test in `Engine::render` settles that in the current
//! frame, and a slot that was out of view last frame falls under the second rule.
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

/// Per-slot state: the GL query object, and whether a query issued last frame is waiting to
/// be read.
struct Slot {
    query: glow::Query,
    pending: bool,
}

pub struct Occlusion {
    gl: Rc<glow::Context>,
    /// Keyed on `(path, portal index)`. Grown on demand; the set of slots a scene uses settles
    /// within a few frames, and the queries are kept until the engine is torn down.
    slots: HashMap<(Path, usize), Slot>,
    /// Number of portals the slots were issued against; a change makes every index mean
    /// something else, so every pending result is dropped.
    portal_count: usize,
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
        }
    }

    /// Forget every pending result. On a scene load the portal vector is rebuilt, so index
    /// `i` names a different portal than it did; the queries themselves are reusable.
    pub fn reset(&mut self) {
        for s in self.slots.values_mut() {
            s.pending = false;
        }
    }

    /// Whether the portal at index `i`, seen from the pass `path`, passed any samples the
    /// last time it was queried -- `true` unless a query issued last frame says otherwise.
    /// Consumes the pending result, so a frame that skips the query (portal out of the
    /// frustum) cannot leave a stale answer for the frame that next asks.
    pub fn visible(&mut self, path: Path, i: usize, portal_count: usize) -> bool {
        if portal_count != self.portal_count {
            self.portal_count = portal_count;
            self.reset();
        }
        let Some(slot) = self.slots.get_mut(&(path, i)) else { return true };
        if !slot.pending {
            return true;
        }
        slot.pending = false;
        unsafe {
            let gl = &self.gl;
            if gl.get_query_parameter_u32(slot.query, glow::QUERY_RESULT_AVAILABLE) == 0 {
                return true;
            }
            gl.get_query_parameter_u32(slot.query, glow::QUERY_RESULT) > 0
        }
    }

    /// Start a `SAMPLES_PASSED` query for the slot; `end` closes it. The result is read by
    /// `visible` next frame. Beginning a query on an object whose previous result was never
    /// read simply discards that result, which is what an unavailable one deserves.
    pub fn begin(&mut self, path: Path, i: usize) {
        let gl = &self.gl;
        let slot = self.slots.entry((path, i)).or_insert_with(|| Slot {
            query: unsafe { gl.create_query().expect("glGenQueries failed") },
            pending: false,
        });
        slot.pending = true;
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
}
