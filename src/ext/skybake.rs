//! EXT: baked cloud panorama. Not part of the C++ port.
//!
//! # The performance trick
//!
//! A procedural cloud shader evaluated per pixel per frame is expensive anywhere, and ruinous
//! here: the ported renderer draws the sky at the start of **every** render pass, including each
//! portal's 2048x2048 framebuffer, up to four levels deep (Engine.cpp:209-211, Portal.cpp:42).
//! Six-octave FBM times several million pixels times several passes is not a 60 fps budget.
//!
//! So the clouds are computed once, into an equirectangular panorama texture, by a single
//! offscreen pass of `Shaders/cloudbake.*`. The runtime sky shader then does one texture
//! fetch per pixel. Two cheap illusions keep it from looking static:
//!
//! * the panorama **scrolls** slowly in longitude (one `+` in the sky shader), and
//! * **two** panoramas are kept, baked at successive noise times, and the sky shader
//!   cross-fades between them over `REBAKE_SECS`. When the fade completes, the older one is
//!   re-baked at the next time step and the fade restarts -- so the cloud shapes evolve
//!   continuously with no visible pop, for one 1536x768 pass every few seconds.
//!
//! The bake assumes the sun at `LIGHT` (Shaders/texture.frag), so cloud lighting stays
//! consistent with every other material; the scroll is slow enough that the lit edges never
//! visibly drift away from the sun.

use crate::mesh::Mesh;
use crate::resources::Resources;
use crate::shader::Shader;
use glow::HasContext;
use std::cell::Cell;
use std::rc::Rc;

/// Panorama resolution. 1536x768 is plenty for soft cumulus at a 60 degree FOV.
const PANO_W: i32 = 1536;
const PANO_H: i32 = 768;
/// Cross-fade length: each panorama lives for two of these (fading in, then fading out).
const REBAKE_SECS: f32 = 6.0;
/// How fast the cloud field evolves (noise-time per real second).
const EVOLVE_RATE: f32 = 0.05;

/// What the sky shader samples: the two panoramas and the fade between them (0 = all `a`).
#[derive(Clone, Copy)]
pub struct SkyInputs {
    pub a: glow::Texture,
    pub b: glow::Texture,
    pub blend: f32,
}

thread_local! {
    /// Published by `maybe_rebake` every frame, read by the sky draw (props.rs, EXT hook).
    static INPUTS: Cell<Option<SkyInputs>> = const { Cell::new(None) };
}

/// Textures and blend the sky shader should use, if a bake has happened.
pub fn inputs() -> Option<SkyInputs> {
    INPUTS.with(|p| p.get())
}

pub struct SkyBake {
    gl: Rc<glow::Context>,
    quad: Rc<Mesh>,
    shader: Rc<Shader>,
    fbo: glow::Framebuffer,
    tex: [glow::Texture; 2],
    /// Index of the panorama currently fading OUT (the older one).
    front: Cell<usize>,
    /// Real time at which the current fade started.
    fade_start: Cell<f32>,
    /// Noise time of the most recent bake.
    noise_t: Cell<f32>,
}

impl SkyBake {
    pub fn new(gl: &Rc<glow::Context>, res: &Resources) -> SkyBake {
        let make_tex = |gl: &Rc<glow::Context>| unsafe {
            let tex = gl.create_texture().expect("sky panorama texture");
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            // Longitude wraps; latitude must not (the poles would bleed into each other).
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGB8 as i32,
                PANO_W,
                PANO_H,
                0,
                glow::RGB,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            tex
        };
        let tex = [make_tex(gl), make_tex(gl)];
        let fbo = unsafe {
            let fbo = gl.create_framebuffer().expect("sky panorama fbo");
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(tex[0]),
                0,
            );
            if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
                eprintln!("[sky] panorama framebuffer incomplete");
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            fbo
        };
        let bake = SkyBake {
            gl: Rc::clone(gl),
            quad: res.acquire_mesh("quad.obj"),
            shader: res.acquire_shader("cloudbake"),
            fbo,
            tex,
            front: Cell::new(0),
            fade_start: Cell::new(0.0),
            noise_t: Cell::new(0.0),
        };
        // Both panoramas start populated so the first fade has two real endpoints.
        bake.bake_into(0, 0.0);
        bake.bake_into(1, REBAKE_SECS * EVOLVE_RATE);
        bake.noise_t.set(REBAKE_SECS * EVOLVE_RATE);
        bake.publish(0.0);
        bake
    }

    /// Render the cloud field at noise-time `t` into panorama `which`.
    fn bake_into(&self, which: usize, t: f32) {
        unsafe {
            let gl = &self.gl;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(self.tex[which]),
                0,
            );
            gl.viewport(0, 0, PANO_W, PANO_H);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::CULL_FACE);
            self.shader.use_program();
            self.shader.set_f32("time", t);
            self.quad.draw();
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.enable(glow::CULL_FACE);
            gl.enable(glow::DEPTH_TEST);
        }
    }

    fn publish(&self, blend: f32) {
        let f = self.front.get();
        INPUTS.with(|p| {
            p.set(Some(SkyInputs {
                a: self.tex[f],
                b: self.tex[1 - f],
                blend,
            }))
        });
    }

    /// Call once per rendered frame (main pass only, before the scene is drawn). Advances the
    /// cross-fade; when it completes, the faded-out panorama becomes the next bake target.
    pub fn maybe_rebake(&self, now: f32) {
        let mut blend = (now - self.fade_start.get()) / REBAKE_SECS;
        if blend >= 1.0 {
            // `b` has fully faded in: it becomes the new front, and the old front is re-baked
            // one step further along in noise time to serve as the next `b`.
            let old_front = self.front.get();
            self.front.set(1 - old_front);
            let t = self.noise_t.get() + REBAKE_SECS * EVOLVE_RATE;
            self.noise_t.set(t);
            self.bake_into(old_front, t);
            self.fade_start.set(now);
            blend = 0.0;
        }
        self.publish(blend.clamp(0.0, 1.0));
    }
}

impl Drop for SkyBake {
    fn drop(&mut self) {
        INPUTS.with(|p| p.set(None));
        unsafe {
            self.gl.delete_framebuffer(self.fbo);
            self.gl.delete_texture(self.tex[0]);
            self.gl.delete_texture(self.tex[1]);
        }
    }
}
