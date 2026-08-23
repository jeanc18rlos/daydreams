//! EXT: a field of real grass blades. Not part of the C++ port.
//!
//! `Meshes/grass_patch.obj` is a 34x34 unit patch of ~26,000 blades (312k triangles), built by
//! `tools/gen_grass_houdini.py`. Drawing a whole level's worth of blades is out of the question,
//! so the patch **follows the player**: every step its position snaps to a grid, keeping a
//! neighbourhood of blades around wherever they stand. Beyond the patch the ground's grass
//! *texture* takes over, and the blade shader fades into the same distance haze the ground uses,
//! so the handover is not visible.
//!
//! # Why it snaps rather than tracks smoothly
//!
//! If the patch simply moved with the player, every blade would slide along the ground with
//! them -- the single most obvious tell in any faked grass. Snapping to a grid means the patch
//! only ever jumps by exactly one cell, and because the blades are dense and irregular, a jump
//! of one cell is indistinguishable from the field having been there all along. The cell is
//! much smaller than the patch, so the covered area always extends well past the player.
//!
//! # Height
//!
//! Blades sit on the plane the scene passes in (`ground_y`). That is right for every flat-ground
//! level. The Meadow's rolling terrain would need per-blade height sampling, which needs a
//! second texture binding -- noted in the README as the remaining piece.

use crate::object::{Object, ObjectT, UpdateCtx};
use crate::resources::Resources;
use crate::vector::Vector3;

/// Patch side, from tools/gen_grass_houdini.py.
#[allow(dead_code)] // read by the coverage test
pub const PATCH: f32 = 26.0;
/// Snap cell. Small enough that the jump is never noticed, large enough that it is rare.
const SNAP: f32 = 2.0;

pub struct GrassField {
    base: Object,
    ground_y: f32,
}

impl GrassField {
    pub fn new(res: &Resources, ground_y: f32) -> GrassField {
        let mut base = Object::new();
        base.mesh = Some(res.acquire_mesh("grass_patch.obj"));
        base.shader = Some(res.acquire_shader("grassblade"));
        // Same noise atlas the ground material uses, so patchiness agrees across the seam.
        base.texture = Some(res.acquire_texture("grass_noise.bmp", 1, 1));
        base.pos = Vector3::new(0.0, ground_y, 0.0);
        GrassField { base, ground_y }
    }
}

impl ObjectT for GrassField {
    fn base(&self) -> &Object {
        &self.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base
    }

    /// Skip the blade field entirely in portal passes. Half a million triangles re-drawn into
    /// four levels of 2048x2048 framebuffer is the single most expensive thing in the game, and
    /// the ground's grass texture already covers that view.
    fn draw(
        &self,
        ctx: &crate::object::RenderCtx,
        cam: &crate::camera::Camera,
        _fbo: Option<glow::Framebuffer>,
    ) {
        if crate::ext::view::detail() < 0.5 {
            return;
        }
        self.base.draw_impl(ctx, cam);
    }

    fn update(&mut self, ctx: &UpdateCtx) {
        // Past the mood split the player is in the far world behind the door (the sea, the
        // backrooms), where a patch of meadow following them would stand up through whatever
        // floor they are on. Leave it where it was.
        if ctx.player_pos.x > crate::ext::view::MOOD_SPLIT_X {
            return;
        }
        let snap = |v: f32| (v / SNAP).round() * SNAP;
        self.base.pos = Vector3::new(snap(ctx.player_pos.x), self.ground_y, snap(ctx.player_pos.z));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The patch must extend well past the player even at the worst snap offset, or the blade
    /// field would visibly end inside the player's view.
    #[test]
    fn patch_covers_the_player_at_worst_offset() {
        let worst = SNAP / 2.0;
        let reach = PATCH / 2.0 - worst;
        assert!(reach > 11.0, "only {reach} units of blades around the player");
    }

    /// Snapping must be to exact multiples, or the jump is not a whole cell and blades slide.
    #[test]
    fn snap_is_exact() {
        let snap = |v: f32| (v / SNAP).round() * SNAP;
        for v in [-7.3f32, -0.4, 0.0, 1.2, 9.9] {
            let s = snap(v);
            assert!((s / SNAP - (s / SNAP).round()).abs() < 1e-5, "{v} -> {s} not on the grid");
        }
    }
}
