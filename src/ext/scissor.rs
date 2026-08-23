//! EXT: scissoring a portal's nested pass to the quad's screen footprint. Not part of the C++
//! port.
//!
//! The ported `Portal::Draw` renders the whole frustum into the portal's framebuffer
//! (Portal.cpp:42) and then draws the quad with `Shaders/portal.frag`, which samples that
//! framebuffer by a screen-space projective lookup -- so the region of the framebuffer that is
//! ever read is exactly the quad's footprint on screen, a few percent of it for a door across
//! the meadow. Everything else is drawn and thrown away: the sky, the terrain, the blade
//! field, at framebuffer resolution, once per portal per level.
//!
//! The fix is a scissor box. The quad's four corners are projected with the pass camera, their
//! NDC bounding box is clamped to the screen and padded by a couple of pixels, and the nested
//! `Engine::render` runs with `GL_SCISSOR_TEST` on. Its depth clear is scissored too (the GL
//! spec applies the scissor to `glClear`), which is what makes the stale pixels outside the
//! box harmless: nothing samples them. A quad with a corner at or behind the eye (`w <= 0`) has
//! no finite projection, and gets the full viewport.
//!
//! The pixel math is GL-free so it can be tested; the GL state handling is a small guard that
//! restores whatever scissor state it found, which is how nested passes compose: each level
//! sets its own box for its own framebuffer and hands the outer level's box back on return.

use glow::HasContext;

use crate::vector::{Matrix4, Vector4};

/// Pixels added on every side of the projected box. `portal.frag` samples with NEAREST, so a
/// screen pixel on the quad's edge can land up to a texel outside the exact box; one texel
/// would do, two is a margin that costs nothing.
pub const PAD: i32 = 2;

/// Clip-space positions of a portal quad's four corners: `double_quad.obj` spans (+-1, +-1, 0)
/// in the portal's local frame.
pub fn quad_clip(local_to_world: &Matrix4, view_proj: &Matrix4) -> [Vector4; 4] {
    let m = *view_proj * *local_to_world;
    [
        m * Vector4::new(-1.0, -1.0, 0.0, 1.0),
        m * Vector4::new(1.0, -1.0, 0.0, 1.0),
        m * Vector4::new(-1.0, 1.0, 0.0, 1.0),
        m * Vector4::new(1.0, 1.0, 0.0, 1.0),
    ]
}

/// The scissor box `(x, y, w, h)`, in the pixels of a `width` x `height` framebuffer, that
/// covers the screen footprint of a quad whose corners project to `clip`. Window coordinates,
/// origin bottom-left, as `glScissor` takes them. The full framebuffer when any corner has
/// `w <= 0`: such a quad crosses the eye plane, its projection is unbounded, and the only safe
/// answer is everything.
pub fn footprint(clip: &[Vector4; 4], width: i32, height: i32) -> [i32; 4] {
    if clip.iter().any(|c| c.w <= 0.0) {
        return [0, 0, width, height];
    }
    let (mut lo_x, mut lo_y, mut hi_x, mut hi_y) = (1.0f32, 1.0f32, -1.0f32, -1.0f32);
    for c in clip {
        let x = (c.x / c.w).clamp(-1.0, 1.0);
        let y = (c.y / c.w).clamp(-1.0, 1.0);
        lo_x = lo_x.min(x);
        lo_y = lo_y.min(y);
        hi_x = hi_x.max(x);
        hi_y = hi_y.max(y);
    }
    // NDC -1..1 -> 0..n, the viewport transform; floor/ceil so the box never loses a partial
    // pixel on either edge, then the pad, then back inside the framebuffer.
    let px = |ndc: f32, n: i32| (ndc * 0.5 + 0.5) * n as f32;
    let x0 = (px(lo_x, width).floor() as i32 - PAD).max(0);
    let y0 = (px(lo_y, height).floor() as i32 - PAD).max(0);
    let x1 = (px(hi_x, width).ceil() as i32 + PAD).min(width);
    let y1 = (px(hi_y, height).ceil() as i32 + PAD).min(height);
    [x0, y0, x1 - x0, y1 - y0]
}

/// The GL state a scissored pass replaces, handed back by `end`. Saving it rather than
/// assuming "disabled, full window" is what lets a nested portal pass scissor its own
/// framebuffer and leave the enclosing pass's box exactly as it was.
pub struct ScissorGuard {
    was_enabled: bool,
    old_box: [i32; 4],
}

impl ScissorGuard {
    /// Enable the scissor test on `rect` (`[x, y, w, h]`), remembering what it replaces.
    pub fn begin(gl: &glow::Context, rect: [i32; 4]) -> ScissorGuard {
        let mut old_box = [0i32; 4];
        let was_enabled = unsafe {
            gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut old_box);
            let was = gl.is_enabled(glow::SCISSOR_TEST);
            gl.scissor(rect[0], rect[1], rect[2], rect[3]);
            gl.enable(glow::SCISSOR_TEST);
            was
        };
        ScissorGuard { was_enabled, old_box }
    }

    /// Put the previous scissor box and enable state back.
    pub fn end(self, gl: &glow::Context) {
        unsafe {
            let b = self.old_box;
            gl.scissor(b[0], b[1], b[2], b[3]);
            if !self.was_enabled {
                gl.disable(glow::SCISSOR_TEST);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::Camera;
    use crate::object::Object;
    use crate::vector::Vector3;

    /// A camera at the origin looking down -z, 16:9, as `Engine::run_frame` builds it.
    fn cam() -> Camera {
        let mut c = Camera::new();
        c.set_size(1600, 900, 0.1, 100.0);
        c
    }

    #[test]
    fn quad_dead_ahead_gives_a_centred_box() {
        let mut q = Object::new();
        q.pos = Vector3::new(0.0, 0.0, -10.0);
        let clip = quad_clip(&q.local_to_world(), &cam().matrix());
        let [x, y, w, h] = footprint(&clip, 2048, 2048);
        // Centred: the box is symmetric about the middle of the framebuffer.
        assert_eq!(x + w, 2048 - x, "x {x} w {w}");
        assert_eq!(y + h, 2048 - y, "y {y} h {h}");
        // And small: a 2-unit quad at 10 units under a 60 degree FOV is 17% of the height,
        // and the 16:9 projection squeezes x to 9/16 of that. Padded by 2 px each side.
        let e = 1.0 / (30f32.to_radians()).tan();
        let exp_h = (2048.0 * e / 10.0) as i32;
        let exp_w = (2048.0 * e * (900.0 / 1600.0) / 10.0) as i32;
        assert!((h - exp_h).abs() <= 2 * PAD + 2, "h {h} expected ~{exp_h}");
        assert!((w - exp_w).abs() <= 2 * PAD + 2, "w {w} expected ~{exp_w}");
    }

    #[test]
    fn quad_behind_the_eye_gives_the_full_viewport() {
        let mut q = Object::new();
        q.pos = Vector3::new(0.0, 0.0, 10.0);
        let clip = quad_clip(&q.local_to_world(), &cam().matrix());
        assert_eq!(footprint(&clip, 2048, 2048), [0, 0, 2048, 2048]);
    }

    #[test]
    fn quad_straddling_the_eye_plane_gives_the_full_viewport() {
        // One corner in front, one behind: unbounded on screen.
        let mut q = Object::new();
        q.euler.y = std::f32::consts::FRAC_PI_2;
        q.pos = Vector3::new(0.5, 0.0, 0.0);
        let clip = quad_clip(&q.local_to_world(), &cam().matrix());
        assert!(clip.iter().any(|c| c.w <= 0.0));
        assert_eq!(footprint(&clip, 1024, 512), [0, 0, 1024, 512]);
    }

    #[test]
    fn box_is_clamped_to_the_framebuffer_and_follows_its_size() {
        // A quad so close it overflows the screen on every side.
        let mut q = Object::new();
        q.pos = Vector3::new(0.0, 0.0, -0.5);
        let clip = quad_clip(&q.local_to_world(), &cam().matrix());
        assert_eq!(footprint(&clip, 1024, 512), [0, 0, 1024, 512]);
        // A quad in the upper-right quadrant lands in the upper-right of any framebuffer.
        q.pos = Vector3::new(4.0, 3.0, -10.0);
        let clip = quad_clip(&q.local_to_world(), &cam().matrix());
        let [x, y, w, h] = footprint(&clip, 1000, 500);
        assert!(x > 500 && y > 250, "x {x} y {y}");
        assert!(x + w <= 1000 && y + h <= 500);
        assert!(w > 0 && h > 0);
    }
}
