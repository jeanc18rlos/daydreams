//! Port of Scene.h.
//!
//! PORT: the two container typedefs live here instead of in their original headers --
//! `PObjectVec` was `typedef std::vector<std::shared_ptr<Object>> PObjectVec` (Object.h:46) and
//! `PPortalVec` was `typedef std::vector<std::shared_ptr<Portal>> PPortalVec` (Portal.h:45).
//! Rust has no forward declarations, so gathering both aliases in the module that the levels
//! already import avoids a cyclic `use` between object.rs and portal.rs.

use std::cell::RefCell;
use std::rc::Rc;

use crate::object::ObjectT;
use crate::player::Player;
use crate::portal::Portal;
use crate::resources::Resources;

// PORT: std::shared_ptr<Object> -> Rc<RefCell<dyn ObjectT>>. The C++ vector holds base-class
// pointers and dispatches virtually through Object's vtable; `dyn ObjectT` is the equivalent,
// and the RefCell restores the interior mutability that a raw C++ pointer has for free
// (was: typedef std::vector<std::shared_ptr<Object>> PObjectVec, Object.h:46).
pub type PObjectVec = Vec<Rc<RefCell<dyn ObjectT>>>;
// PORT: same treatment for the portal vector. Portal is a concrete type here, not `dyn`,
// because Engine calls Portal-specific methods on it (was: Portal.h:45).
pub type PPortalVec = Vec<Rc<RefCell<Portal>>>;

pub trait Scene {
    // PORT: the pure virtual `Load` gains `gl` and `res` parameters. In C++ the level code
    // reaches the GL context and the resource caches through the file-scope AquireXxx()
    // functions and the GH_ENGINE global; those globals are not ported, so both are threaded
    // through explicitly (was: virtual void Load(PObjectVec&, PPortalVec&, Player&)=0, Scene.h:8).
    fn load(
        &self,
        gl: &Rc<glow::Context>,
        res: &Resources,
        objs: &mut PObjectVec,
        portals: &mut PPortalVec,
        player: &mut Player,
    );

    fn unload(&self) {}
}
