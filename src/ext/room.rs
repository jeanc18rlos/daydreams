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

use crate::camera::Camera;
use crate::object::{Object, ObjectT, RenderCtx, UpdateCtx};

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
