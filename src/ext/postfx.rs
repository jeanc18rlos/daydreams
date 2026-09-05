//! EXT: the title screen's post-process chain -- bloom, veil, vignette. Not part of the C++
//! port, which has no offscreen pass of any kind outside the portals.
//!
//! # What it does
//!
//! The world is rendered into a texture instead of straight to the window. Then:
//!
//! 1. **Bright pass** -- everything above a knee, at quarter resolution. On the title screen
//!    that is the sunset through the doorway and nothing else, which is the point: it is the
//!    only light source in a grey meadow.
//! 2. **Blur** -- one separable Gaussian, horizontal then vertical, over the quarter-res
//!    image. At quarter resolution a nine-tap kernel with linear filtering reaches about forty
//!    full-resolution pixels, which is a wide enough bloom for a light source this small.
//! 3. **Resolve** -- scene plus bloom, then a veil that lifts the shadows toward the bloom's
//!    own colour (light scattering in the air rather than a flat fog), a vignette, and a
//!    dither.
//!
//! # Why only the title
//!
//! `Engine::render_menu_frame` runs this for title frames; the game does not. A menu backdrop
//! is a still photograph that can afford four full-screen passes, and it is the one frame in
//! the game whose job is to look like a poster. Gameplay keeps its direct-to-window path and
//! its frame budget untouched -- there is no risk of this costing anyone a frame while they
//! are playing, and no second code path to keep in step, because it is bracketed around the
//! same `Engine::render` call.
//!
//! # Sizes
//!
//! The scene target is the drawable's size; the two ping-pong targets are a quarter of it in
//! each axis. All three are rebuilt by [`PostFx::ensure`] when the drawable changes, which is
//! the same rule `Engine::ensure_portal_fbos` follows.

use std::cell::RefCell;
use std::rc::Rc;

use glow::HasContext;

use crate::mesh::Mesh;
use crate::resources::Resources;
use crate::shader::Shader;

/// Bloom resolution divisor. 4 is what makes a nine-tap blur reach far enough to read as
/// glow rather than as a halo, and it costs a sixteenth of the fill.
const DOWNSCALE: i32 = 4;

/// One colour target, with a depth buffer only where something is rendered into it.
struct Target {
    fbo: glow::Framebuffer,
    tex: glow::Texture,
    depth: Option<glow::Renderbuffer>,
    w: i32,
    h: i32,
}

impl Target {
    fn new(gl: &glow::Context, w: i32, h: i32, with_depth: bool) -> Option<Target> {
        unsafe {
            let tex = gl.create_texture().ok()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            // LINEAR, not the NEAREST `FrameBuffer` uses: every pass here samples between
            // texels on purpose -- the downsample averages four, and the blur leans on the
            // hardware to make each of its nine taps a bilinear pair.
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            // Clamped, so the blur's taps off the edge repeat the border instead of wrapping
            // the far side of the frame into it.
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                w,
                h,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            let fbo = gl.create_framebuffer().ok()?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(tex),
                0,
            );
            let depth = if with_depth {
                let rb = gl.create_renderbuffer().ok()?;
                gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rb));
                gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT24, w, h);
                gl.framebuffer_renderbuffer(
                    glow::FRAMEBUFFER,
                    glow::DEPTH_ATTACHMENT,
                    glow::RENDERBUFFER,
                    Some(rb),
                );
                Some(rb)
            } else {
                None
            };
            let ok = gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            if !ok {
                log::warn!("[postfx] {w}x{h} target incomplete; the title runs unfiltered");
                return None;
            }
            Some(Target { fbo, tex, depth, w, h })
        }
    }

    fn delete(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_framebuffer(self.fbo);
            gl.delete_texture(self.tex);
            if let Some(rb) = self.depth {
                gl.delete_renderbuffer(rb);
            }
        }
    }
}

pub struct PostFx {
    gl: Rc<glow::Context>,
    quad: Rc<Mesh>,
    bright: Rc<Shader>,
    blur: Rc<Shader>,
    resolve: Rc<Shader>,
    /// Scene, then the two quarter-res ping-pong targets. `None` until the first `ensure`, and
    /// again if a target ever fails to build -- in which case every entry point below is a
    /// no-op and the caller renders straight to the window, unfiltered but working.
    targets: RefCell<Option<(Target, Target, Target)>>,
}

impl PostFx {
    pub fn new(gl: &Rc<glow::Context>, res: &Resources) -> PostFx {
        PostFx {
            gl: Rc::clone(gl),
            quad: res.acquire_mesh("quad.obj"),
            bright: res.acquire_shader("post_bright"),
            blur: res.acquire_shader("post_blur"),
            resolve: res.acquire_shader("post_resolve"),
            targets: RefCell::new(None),
        }
    }

    /// (Re)build the targets at the drawable's size. A no-op on every frame the size has not
    /// changed; the old set is deleted before the new one is built so the two never coexist.
    fn ensure(&self, w: i32, h: i32) {
        let (w, h) = (w.max(1), h.max(1));
        let mut slot = self.targets.borrow_mut();
        if slot.as_ref().is_some_and(|(s, _, _)| s.w == w && s.h == h) {
            return;
        }
        if let Some((a, b, c)) = slot.take() {
            a.delete(&self.gl);
            b.delete(&self.gl);
            c.delete(&self.gl);
        }
        let (sw, sh) = ((w / DOWNSCALE).max(1), (h / DOWNSCALE).max(1));
        *slot = (|| {
            Some((
                Target::new(&self.gl, w, h, true)?,
                Target::new(&self.gl, sw, sh, false)?,
                Target::new(&self.gl, sw, sh, false)?,
            ))
        })();
    }

    /// Bind the scene target and return it, for `Engine::render` to carry as its current
    /// framebuffer so that portal passes restore to it rather than to the window. `None` means
    /// the chain is unavailable and the caller should render to the window as usual.
    pub fn begin(&self, w: i32, h: i32) -> Option<glow::Framebuffer> {
        self.ensure(w, h);
        let slot = self.targets.borrow();
        let (scene, _, _) = slot.as_ref()?;
        unsafe {
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(scene.fbo));
            self.gl.viewport(0, 0, scene.w, scene.h);
        }
        Some(scene.fbo)
    }

    /// Run the chain and composite to the window. Pairs with [`begin`]; does nothing if that
    /// returned `None`.
    ///
    /// [`begin`]: PostFx::begin
    pub fn resolve(&self) {
        let slot = self.targets.borrow();
        let Some((scene, ping, pong)) = slot.as_ref() else { return };
        let gl = &self.gl;
        unsafe {
            // Full-screen passes: no depth, no culling, no blending. Each writes every pixel
            // of its target, so none of them needs a clear either.
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::BLEND);

            // 1. Bright pass, scene -> ping, at a quarter of the size.
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(ping.fbo));
            gl.viewport(0, 0, ping.w, ping.h);
            self.bright.use_program();
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(scene.tex));
            self.bright.set_i32("tex", 0);
            self.quad.draw();

            // 2. Blur, separable: ping -> pong across, pong -> ping down.
            self.blur.use_program();
            self.blur.set_i32("tex", 0);
            let (tw, th) = (1.0 / ping.w as f32, 1.0 / ping.h as f32);
            for (src, dst, step) in [(ping, pong, [tw, 0.0]), (pong, ping, [0.0, th])] {
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(dst.fbo));
                gl.viewport(0, 0, dst.w, dst.h);
                gl.bind_texture(glow::TEXTURE_2D, Some(src.tex));
                self.blur.set_vec2("step_uv", step);
                self.quad.draw();
            }

            // 3. Resolve to the window.
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.viewport(0, 0, scene.w, scene.h);
            self.resolve.use_program();
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(ping.tex));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(scene.tex));
            self.resolve.set_i32("tex", 0);
            self.resolve.set_i32("tex2", 1);
            self.quad.draw();

            // Back to the state the rest of the frame -- the 2D menu layer, then the next
            // frame's world -- expects to find (Engine.cpp:421-425).
            gl.enable(glow::CULL_FACE);
            gl.enable(glow::DEPTH_TEST);
        }
    }
}

impl Drop for PostFx {
    fn drop(&mut self) {
        if let Some((a, b, c)) = self.targets.borrow_mut().take() {
            a.delete(&self.gl);
            b.delete(&self.gl);
            c.delete(&self.gl);
        }
    }
}
