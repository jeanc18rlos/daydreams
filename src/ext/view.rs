//! EXT: runtime field of view, for the dolly-zoom effect. Not part of the C++ port.
//!
//! `GH_FOV` is a compile-time constant read directly by `Camera::SetSize` (Camera.cpp:19), so the
//! original engine has no way to change it. The dolly zoom -- Hitchcock's vertigo shot, where
//! the FOV widens as you move forward so the corridor appears to stretch away from you -- needs
//! it to be animatable.
//!
//! A thread-local `Cell` rather than a field threaded through the camera: `Camera` is `Copy` and
//! gets duplicated per portal recursion level (`Portal::Draw`, Portal.cpp:35), and a per-instance
//! FOV would have to be propagated through every one of those copies to stay consistent. A single
//! ambient value is both simpler and more correct here -- every camera in a frame, including all
//! the recursive portal cameras, must agree on the projection or the portal views would not line
//! up with the geometry around them.
//!
//! The engine is single-threaded (the ported design uses `Rc`/`RefCell` throughout), so a
//! thread-local is exactly as shared as it needs to be.

use crate::game_header::GH_FOV;
use std::cell::Cell;

thread_local! {
    static FOV: Cell<f32> = const { Cell::new(GH_FOV) };
    /// EXT: seconds since the engine started, published once per rendered frame so shaders
    /// can animate (cloud drift, grass wind) without any per-object plumbing.
    static TIME: Cell<f32> = const { Cell::new(0.0) };
}

/// EXT: scenes that want two worlds with different weather put the second one beyond this x.
/// Every shader that cares (sky, grass, sea) receives `mood` = 0 (storm) or 1 (sunset) based
/// on where the CAMERA of the current render pass is -- so a portal pass looking into the far
/// region grades itself as sunset while the main pass stays stormy. Same baked clouds, two
/// colour grades, no extra cost. Scenes that never cross the line never see mood 1.
pub const MOOD_SPLIT_X: f32 = 250.0;
// Whether the current scene uses the split at all (set by the intro level on load, cleared by
// every load), and the door's warm light pool as (x, y, z, intensity).
thread_local! {
    static MOOD_ENABLED: Cell<bool> = const { Cell::new(false) };
    static GLOW: Cell<[f32; 4]> = const { Cell::new([0.0; 4]) };
    /// 1.0 while drawing the main view, 0.0 inside a portal's framebuffer.
    static DETAIL: Cell<f32> = const { Cell::new(1.0) };
    /// EXT: side of the world's fundamental square, or 0 in an ordinary unbounded scene.
    ///
    /// The intro meadow is a flat torus (`ext/terrain.rs`): the player's position is wrapped
    /// every step, which is only invisible if every world-space pattern repeats over exactly
    /// that distance. Materials receive this as the `wrap` uniform and round their world-space
    /// frequencies to whole cycles per period; without it the grass patchiness would slide
    /// sideways each time the player crossed the seam.
    static WRAP: Cell<f32> = const { Cell::new(0.0) };
}

/// Set the world period, or 0 for scenes that do not wrap. Cleared on every scene load.
pub fn set_wrap(period: f32) {
    WRAP.with(|w| w.set(period));
}

pub fn wrap() -> f32 {
    WRAP.with(|w| w.get())
}

pub fn set_mood_enabled(on: bool) {
    MOOD_ENABLED.with(|m| m.set(on));
}

/// Mood for a render pass whose camera eye is at `eye`:
///   -1 = plain daylight (scenes without the split),
///    0 = storm (intro meadow), 1 = sunset (the world behind the door).
pub fn mood_for(eye: crate::vector::Vector3) -> f32 {
    if !MOOD_ENABLED.with(|m| m.get()) {
        -1.0
    } else if eye.x > MOOD_SPLIT_X {
        1.0
    } else {
        0.0
    }
}

pub fn set_glow(pos: crate::vector::Vector3, intensity: f32) {
    GLOW.with(|g| g.set([pos.x, pos.y, pos.z, intensity]));
}

pub fn glow() -> [f32; 4] {
    GLOW.with(|g| g.get())
}

/// EXT: detail level for the render pass in flight. The ported renderer re-draws the whole
/// scene into every portal's 2048x2048 framebuffer, up to four levels deep (Engine.cpp:207-270),
/// so anything expensive costs several times what the main view suggests. Materials use this to
/// take a cheap path in portal passes, where the result is a small quad on screen anyway.
pub fn detail() -> f32 {
    DETAIL.with(|d| d.get())
}

pub fn set_detail(v: f32) {
    DETAIL.with(|d| d.set(v));
}

/// Seconds since start, as of the current frame.
pub fn time() -> f32 {
    TIME.with(|t| t.get())
}

pub fn set_time(t: f32) {
    TIME.with(|c| c.set(t));
}

/// Current vertical field of view in degrees. Defaults to `GH_FOV`.
pub fn fov() -> f32 {
    FOV.with(|f| f.get())
}

/// Override the field of view. Clamped to a sane range -- past roughly 150 degrees the
/// projection degenerates and the near plane math in `Camera::SetSize` stops being meaningful.
#[allow(dead_code)] // EXT: available for a dolly-zoom effect; no scene currently drives it.
pub fn set_fov(deg: f32) {
    FOV.with(|f| f.set(deg.clamp(20.0, 150.0)));
}

/// Restore the ported default. Called on every scene load so an effect cannot leak between
/// scenes.
pub fn reset_fov() {
    FOV.with(|f| f.set(GH_FOV));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_the_ported_constant() {
        reset_fov();
        assert!((fov() - GH_FOV).abs() < 1e-6);
    }

    #[test]
    fn set_and_reset_round_trip() {
        set_fov(95.0);
        assert!((fov() - 95.0).abs() < 1e-6);
        reset_fov();
        assert!((fov() - GH_FOV).abs() < 1e-6);
    }

    #[test]
    fn clamps_degenerate_values() {
        set_fov(1.0);
        assert!(fov() >= 20.0);
        set_fov(400.0);
        assert!(fov() <= 150.0);
        reset_fov();
    }
}
