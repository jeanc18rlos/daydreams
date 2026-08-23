//! EXT: white 2D contour around the held object. Not part of the C++ port.
//!
//! # Why not an inverted hull
//!
//! The first version drew the mesh a second time, scaled out along its normals with front-face
//! culling -- the classic "inverted hull". On this engine's flat-shaded, low-poly meshes it
//! read as exactly what it is: a second 3D object, with visible facets and a thickness that
//! changed with viewing angle. Superliminal's outline is a *vector* line: constant pixel
//! width, no facets, hugging the silhouette.
//!
//! # Screen-space silhouette
//!
//! Two passes, both 2D in spirit:
//!
//! 1. **Mask.** Render only the held object, flat white, into an offscreen texture the size of
//!    the window (no depth test -- the whole silhouette, even the parts a wall hides, so a
//!    half-buried object still reads as a complete shape).
//! 2. **Edge.** Draw a fullscreen quad that samples the mask in a ring of `TAPS` points at a
//!    radius of `WIDTH_PX` pixels. A pixel that is *outside* the mask but sees the mask within
//!    the ring is on the contour: paint it white. Everything else is discarded.
//!
//! The result is a line of exactly `WIDTH_PX` pixels, independent of geometry, distance or
//! mesh density. The offscreen target is recreated whenever the drawable size changes, so
//! resizing and fullscreen keep the width correct.
//!
//! Drawn in the main pass only (`Engine::run_frame` overlays) -- inside `Engine::render` it
//! would be painted into every portal's framebuffer as well.

use crate::camera::Camera;
use crate::mesh::Mesh;
use crate::object::ObjectT;
use crate::resources::Resources;
use crate::shader::Shader;
use glow::HasContext;
use std::cell::Cell;
use std::rc::Rc;

/// Contour thickness in pixels (physical).
const WIDTH_PX: f32 = 3.0;

pub struct Outline {
    gl: Rc<glow::Context>,
    mask_shader: Rc<Shader>,
    edge_shader: Rc<Shader>,
    quad: Rc<Mesh>,
    fbo: Cell<Option<glow::Framebuffer>>,
    tex: Cell<Option<glow::Texture>>,
    size: Cell<(i32, i32)>,
}

impl Outline {
    pub fn new(gl: &Rc<glow::Context>, res: &Resources) -> Outline {
        Outline {
            gl: Rc::clone(gl),
            mask_shader: res.acquire_shader("outline_mask"),
            edge_shader: res.acquire_shader("outline_edge"),
            quad: res.acquire_mesh("quad.obj"),
            fbo: Cell::new(None),
            tex: Cell::new(None),
            size: Cell::new((0, 0)),
        }
    }

    /// (Re)create the offscreen mask target at the current drawable size.
    fn ensure_target(&self, width: i32, height: i32) {
        if self.size.get() == (width, height) && self.fbo.get().is_some() {
            return;
        }
        self.release();
        unsafe {
            let gl = &self.gl;
            let tex = gl.create_texture().expect("outline mask texture");
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::R8 as i32,
                width,
                height,
                0,
                glow::RED,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            let fbo = gl.create_framebuffer().expect("outline mask fbo");
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(tex),
                0,
            );
            if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
                log::warn!("[outline] mask framebuffer incomplete");
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            self.fbo.set(Some(fbo));
            self.tex.set(Some(tex));
            self.size.set((width, height));
        }
    }

    fn release(&self) {
        unsafe {
            if let Some(f) = self.fbo.take() {
                self.gl.delete_framebuffer(f);
            }
            if let Some(t) = self.tex.take() {
                self.gl.delete_texture(t);
            }
        }
    }

    /// Draw the contour for `obj`. Leaves GL in the ported renderer's default state.
    pub fn draw(&self, gl: &glow::Context, cam: &Camera, obj: &dyn ObjectT) {
        let base = obj.base();
        let Some(mesh) = base.mesh.as_ref() else { return };
        let (width, height) = (cam.width, cam.height);
        if width <= 0 || height <= 0 {
            return;
        }
        self.ensure_target(width, height);
        let (Some(fbo), Some(tex)) = (self.fbo.get(), self.tex.get()) else { return };

        unsafe {
            // ── Pass 1: silhouette mask ────────────────────────────────────────────────
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.viewport(0, 0, width, height);
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::CULL_FACE); // both faces: a single-sided quad must still mask
            let mvp = cam.matrix() * base.local_to_world();
            self.mask_shader.use_program();
            self.mask_shader.set_mvp(Some(&mvp), None);
            mesh.draw();

            // ── Pass 2: edge detection onto the screen ─────────────────────────────────
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.viewport(0, 0, width, height);
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            self.edge_shader.use_program();
            self.edge_shader.set_vec4(
                "params",
                [1.0 / width as f32, 1.0 / height as f32, WIDTH_PX, 0.0],
            );
            self.quad.draw();

            // ── Restore the ported renderer's state (Engine.cpp:420-425) ──────────────
            gl.disable(glow::BLEND);
            gl.enable(glow::CULL_FACE);
            gl.enable(glow::DEPTH_TEST);
            gl.clear_color(0.6, 0.9, 1.0, 1.0);
        }
    }
}

impl Drop for Outline {
    fn drop(&mut self) {
        self.release();
    }
}
