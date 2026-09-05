//! EXT: what an open door throws into the meadow -- light shafts and drifting motes. Not part
//! of the C++ port.
//!
//! The intro's door is a hole cut into a sunset (`src/level15.rs`). The portal shows what is on
//! the other side of it, and the grass shaders already spill a warm pool onto the ground in
//! front (`ext/door.rs`). What was missing is the air in between: on the near side the door is
//! a lit rectangle with nothing coming out of it, which is exactly how a screen looks and not
//! at all how a doorway looks.
//!
//! Two objects fix that, both driven by the same door and both fading with how far it has
//! swung open (`view::glow()`'s w).
//!
//! # Shafts
//!
//! Not ray tracing, and not a screen-space blur either -- this engine renders straight to the
//! window and has no depth texture to march. It is the oldest volumetric trick there is: a
//! stack of transparent SLICES through the beam, parallel to the door and stepped outward
//! along the light, each one a little wider than the last and a little fainter, added
//! together. Looking along the beam you see through the whole stack at once and the sum is the
//! depth of air the light crossed to reach you; looking across it you see a cone. The slices
//! take the depth test, so the grass and the hills cut the far end of the beam off exactly
//! where they should, and they do not WRITE depth, so they never occlude each other.
//!
//! # Both draw LATE
//!
//! Additive, depth-write-off geometry cannot go in the object loop. The ported renderer draws
//! the sky after that loop, into everything the objects left uncovered, and the portal quads
//! after the sky -- so a mote in front of open sky, or a shaft over the doorway, is painted
//! straight over. `ObjectT::draw_late` exists for this: it runs once the frame is otherwise
//! finished. It is why the plume is visible at all.
//!
//! The beam's direction is the sun's, not the door's normal: light goes where the sun sends
//! it, and the doorway only decides the shape of the hole it comes through. That the two are
//! nearly aligned here is a fact about where `EVE_SUN` was aimed (`Shaders/sky.frag`), not an
//! assumption this makes.
//!
//! # Motes
//!
//! One `GL_POINTS` draw of a few hundred specks of dust, each one a closed loop in `time`
//! evaluated in the vertex shader from a per-mote seed -- so there is no simulation, no
//! buffer to update, and no state to reset when the scene reloads. They start spread across
//! the opening and drift out along the same beam, rising and wandering as they go, brightest
//! in the middle of their life and gone at the end of it.
//!
//! Both draw only in the main view. A portal pass is a small quad on screen and this is
//! additive haze; it would cost several times what it shows.

use std::rc::Rc;

use glow::HasContext;

use crate::camera::Camera;
use crate::ext::door::{HALF_H, HALF_W};
use crate::mesh::Mesh;
use crate::object::{Object, ObjectT, RenderCtx};
use crate::resources::Resources;
use crate::shader::Shader;
use crate::vector::{Matrix4, Vector3};

/// Direction the light TRAVELS, which is the negation of the direction the sun lies in.
/// `-EVE_SUN` from Shaders/sky.frag; that file is the definition, this is the copy, and
/// `the_beam_follows_the_suns_own_direction` fails if they drift apart.
pub const BEAM_DIR: Vector3 = Vector3 { x: 0.2929, y: -0.0140, z: 0.9560 };

/// How far the beam is drawn, and how many slices that is cut into.
///
/// The length stops short of the title screen's vantage on purpose (`ext::meadow::title_view`
/// stands 6.2 units from the opening): a slice drawn behind the near plane is a full-screen
/// wash of orange, and the last thing this should do is fog the camera it is being composed
/// for. The count is what decides whether the stack reads as a volume or as a venetian blind;
/// sixteen is the point where the steps stop being separable at this length.
const BEAM_LEN: f32 = 3.4;
const SLICES: usize = 16;
/// Fraction of the opening's size the beam gains per unit travelled. Light through a real
/// doorway spreads by the sun's own half-degree and no more; this spreads far faster, because
/// what is being drawn is not the geometric beam but the haze around it.
const SPREAD: f32 = 0.44;
/// Brightness of one slice at the opening, before the beam's own falloff. Sixteen of these
/// add up, so it is small -- and smaller again since the stack moved to the late pass, where
/// it lies over the portal instead of being cut off at the door frame. Light coming out of a
/// doorway does not veil the view back through it; at 0.155 it did.
const SLICE_GAIN: f32 = 0.042;

/// Motes: how many, how long one lives, and the three distances its motion is built from --
/// how far it rises, how far it is pushed out through the gap, and how wide it wanders.
///
/// The rise dominates on purpose. See the note at the top of `Shaders/mote.vert`: the sun is
/// aimed through the doorway at the camera, so anything drifting along the light travels down
/// the line of sight and appears not to move. Up is across the frame from every vantage.
const MOTES: usize = 340;
const MOTE_LIFE: f32 = 7.0;
const MOTE_RISE: f32 = 0.95;
const MOTE_PUSH: f32 = 1.20;
const MOTE_WANDER: f32 = 0.55;

/// The stack of slices. Positioned by the door it belongs to; it reads the door's openness
/// from the published glow, so it opens and closes with the leaf without being wired to it.
pub struct Shafts {
    base: Object,
    quad: Rc<Mesh>,
    shader: Rc<Shader>,
    /// Centre of the opening: the door's origin is at its foot, and the beam comes out of the
    /// middle of the gap.
    mouth: Vector3,
    yaw: f32,
}

impl Shafts {
    pub fn new(res: &Resources, pos: Vector3, yaw: f32) -> Shafts {
        let mut base = Object::new();
        base.pos = pos;
        Shafts {
            base,
            quad: res.acquire_mesh("quad.obj"),
            shader: res.acquire_shader("shaft"),
            mouth: pos + Vector3::new(0.0, HALF_H, 0.0),
            yaw,
        }
    }
}

impl ObjectT for Shafts {
    fn base(&self) -> &Object {
        &self.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base
    }

    /// LATE, not in the object loop: additive and depth-write-off, so drawn there the sky and
    /// the portal quad would both paint straight over it (`ObjectT::draw_late`).
    fn draw_late(&self, ctx: &RenderCtx, cam: &Camera, _fbo: Option<glow::Framebuffer>) {
        // Main view only, and only while the door is open enough to be throwing anything.
        if crate::ext::view::detail() < 0.5 {
            return;
        }
        let open = crate::ext::view::glow()[3];
        if open <= 0.01 {
            return;
        }
        let dir = BEAM_DIR.normalized_safe();
        let gl = ctx.gl;
        self.shader.use_program();
        self.shader.set_f32("time", crate::ext::view::time());
        let eye = ctx.eye;
        self.shader.set_vec4("cam_pos", [eye.x, eye.y, eye.z, 1.0]);
        unsafe {
            // Additive, and no depth WRITE: slices must sum rather than hide one another,
            // and must not stop the portal quad from filling the doorway behind them. This
            // runs after that quad is already down, so the beam lies over the sunset it comes
            // out of rather than being cut off at the door frame.
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE);
            gl.depth_mask(false);
            gl.disable(glow::CULL_FACE);
        }
        let vp = cam.matrix();
        for i in 0..SLICES {
            // Slice 0 sits just outside the door plane rather than in it, where it would
            // z-fight the portal quad.
            let t = (i as f32 + 0.5) / SLICES as f32;
            let d = t * BEAM_LEN;
            let grow = 1.0 + SPREAD * d;
            // Fade with distance from the mouth, and again at the very end, so the beam stops
            // by running out rather than by the last slice simply not being there.
            let fade = (1.0 - t) * (1.0 - t) * (1.0 - smoothstep01(0.72, 1.0, t));
            let m = Matrix4::trans(self.mouth + dir * d)
                * Matrix4::rot_y(self.yaw)
                * Matrix4::scale(Vector3::new(HALF_W * grow, HALF_H * grow, 1.0));
            self.shader.set_mat4("mvp", &(vp * m));
            self.shader.set_mat4("model", &m);
            self.shader.set_f32("gain", SLICE_GAIN * fade * open);
            self.quad.draw();
        }
        unsafe {
            gl.depth_mask(true);
            gl.disable(glow::BLEND);
            gl.enable(glow::CULL_FACE);
        }
    }
}

/// `smoothstep` on the CPU side, so a slice's tail matches the shader's own easing.
fn smoothstep01(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The dust. One buffer of seeds, uploaded once; every position is a function of `time`.
pub struct Motes {
    base: Object,
    shader: Rc<Shader>,
    gl: Rc<glow::Context>,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    mouth: Vector3,
    yaw: f32,
}

impl Motes {
    pub fn new(gl: &Rc<glow::Context>, res: &Resources, pos: Vector3, yaw: f32) -> Motes {
        // (u, v, seed): where in the opening it starts, and everything else about it. A plain
        // low-discrepancy pair rather than a random one, so the motes start evenly spread
        // across the gap instead of clumping the way a small random sample does.
        let mut seeds: Vec<[f32; 3]> = Vec::with_capacity(MOTES);
        for i in 0..MOTES {
            let n = i as f32;
            seeds.push([
                radical_inverse(i as u32, 2),
                radical_inverse(i as u32, 3),
                (n * 0.618_034).fract(),
            ]);
        }
        let (vao, vbo) = unsafe {
            let vao = gl.create_vertex_array().expect("motes vao");
            gl.bind_vertex_array(Some(vao));
            let vbo = gl.create_buffer().expect("motes vbo");
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&seeds),
                glow::STATIC_DRAW,
            );
            // Location 0 is what `Shader` gives the first `in` declaration in the vertex
            // source; mote.vert declares exactly one.
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 0, 0);
            gl.bind_vertex_array(None);
            (vao, vbo)
        };
        let mut base = Object::new();
        base.pos = pos;
        Motes {
            base,
            shader: res.acquire_shader("mote"),
            gl: Rc::clone(gl),
            vao,
            vbo,
            mouth: pos + Vector3::new(0.0, HALF_H, 0.0),
            yaw,
        }
    }
}

impl ObjectT for Motes {
    fn base(&self) -> &Object {
        &self.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base
    }

    /// LATE -- see `Shafts::draw_late`. Against open sky, which is where a rising plume
    /// mostly is, the sky pass would otherwise erase every mote.
    fn draw_late(&self, ctx: &RenderCtx, cam: &Camera, _fbo: Option<glow::Framebuffer>) {
        if crate::ext::view::detail() < 0.5 {
            return;
        }
        let open = crate::ext::view::glow()[3];
        if open <= 0.01 {
            return;
        }
        let gl = ctx.gl;
        self.shader.use_program();
        self.shader.set_mat4("vp", &cam.matrix());
        self.shader.set_mat4("basis", &Matrix4::rot_y(self.yaw));
        self.shader.set_vec4("mouth", [self.mouth.x, self.mouth.y, self.mouth.z, 1.0]);
        self.shader.set_vec4("drift", [MOTE_RISE, MOTE_PUSH, MOTE_WANDER, MOTE_LIFE]);
        self.shader.set_vec4("gap", [HALF_W, HALF_H, 0.0, open]);
        self.shader.set_f32("time", crate::ext::view::time());
        // Points are sized in pixels, so the shader needs the viewport height to turn a world
        // size into one.
        self.shader.set_f32("viewport_h", cam.height as f32);
        unsafe {
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE);
            gl.depth_mask(false);
            gl.enable(glow::PROGRAM_POINT_SIZE);
            gl.bind_vertex_array(Some(self.vao));
            gl.draw_arrays(glow::POINTS, 0, MOTES as i32);
            gl.bind_vertex_array(None);
            gl.disable(glow::PROGRAM_POINT_SIZE);
            gl.depth_mask(true);
            gl.disable(glow::BLEND);
        }
    }
}

impl Drop for Motes {
    fn drop(&mut self) {
        unsafe {
            self.gl.delete_buffer(self.vbo);
            self.gl.delete_vertex_array(self.vao);
        }
    }
}

/// The radical inverse of `i` in `base` -- the Halton sequence's building block. Spreads a
/// small number of samples evenly over [0, 1) without the clumps a random draw leaves.
fn radical_inverse(mut i: u32, base: u32) -> f32 {
    let (mut f, mut r) = (1.0f32 / base as f32, 0.0f32);
    while i > 0 {
        r += (i % base) as f32 * f;
        i /= base;
        f /= base as f32;
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The beam is the sun's direction reversed. If `EVE_SUN` in Shaders/sky.frag moves and
    /// this does not, the light shafts would come out of the door at an angle the sunset
    /// behind it disagrees with -- which is the kind of thing nobody notices until the whole
    /// shot looks subtly wrong.
    #[test]
    fn the_beam_follows_the_suns_own_direction() {
        // Shaders/sky.frag: #define EVE_SUN vec3(-0.2929, 0.0140, -0.9560)
        let sun = Vector3::new(-0.2929, 0.0140, -0.9560);
        let beam = BEAM_DIR;
        assert!((beam + sun).mag() < 1e-4, "beam {beam:?} is not -EVE_SUN");
        assert!((beam.mag() - 1.0).abs() < 1e-3, "beam is not a unit direction");
    }

    /// The sun has to sit in the DOORWAY as the title screen sees it, or the shot loses the
    /// thing it is composed around. The beam is the sun reversed, so the test is that the beam
    /// points back along the line from the title's eye through the opening.
    #[test]
    fn the_sun_is_framed_by_the_doorway() {
        let (eye, _, _) = crate::ext::meadow::title_view();
        let mouth = crate::ext::meadow::DOOR_POS + Vector3::new(0.0, HALF_H, 0.0);
        let through = (mouth - eye).normalized_safe();
        // The beam travels TOWARD the eye, so it is the line of sight reversed; the sun lies
        // at the far end of it. Within ten degrees, which is inside the angle the opening
        // subtends from this distance, so the disc lands in the gap and not beside it.
        let along = -BEAM_DIR.normalized_safe().dot(through);
        assert!(
            along > 0.985,
            "the sun is {:.1} degrees off the doorway's axis",
            along.acos().to_degrees()
        );
    }

    /// The beam has to stop before it reaches the eye the title screen is composed for, or the
    /// last slices are drawn across the whole frame.
    #[test]
    fn the_beam_stops_short_of_the_title_camera() {
        let (eye, _, _) = crate::ext::meadow::title_view();
        let mouth = crate::ext::meadow::DOOR_POS + Vector3::new(0.0, HALF_H, 0.0);
        let reach = (eye - mouth).mag();
        assert!(BEAM_LEN < reach - 1.0, "beam {BEAM_LEN} reaches the camera at {reach}");
        // The motes go up rather than out, so what has to clear the eye is the push plus the
        // wander -- and it does, several times over.
        assert!(MOTE_PUSH + MOTE_WANDER < reach - 1.0, "motes reach the camera at {reach}");
    }

    /// A low-discrepancy pair, not a random one: every mote must start inside the opening, and
    /// they must not all start in the same corner of it.
    #[test]
    fn mote_seeds_cover_the_opening() {
        let (mut lo, mut hi) = (1.0f32, 0.0f32);
        let mut left = 0;
        for i in 0..MOTES as u32 {
            let u = radical_inverse(i, 2);
            assert!((0.0..1.0).contains(&u));
            lo = lo.min(u);
            hi = hi.max(u);
            if u < 0.5 {
                left += 1;
            }
        }
        assert!(lo < 0.02 && hi > 0.98, "seeds span only {lo}..{hi} of the gap");
        assert_eq!(left, MOTES / 2, "the two halves of the opening are not equally seeded");
    }
}
