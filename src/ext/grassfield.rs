//! EXT: a field of real grass blades. Not part of the C++ port.
//!
//! The blades are a 26x26 unit patch of 130,000 blades generated in-process by `ext::grassgen`
//! and uploaded once per run. Drawing a whole level's worth of blades is out of the question, so
//! the patch **follows the player**: every step its position snaps to a grid, keeping a
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
//! # What is drawn
//!
//! Only the cull cells the pass can see. The generator sorts the blades into a 13x13 grid of
//! 2-unit cells, each a contiguous index range, and `draw` tests every cell's world-space box
//! against the pass frustum, then merges runs of visible cells into as few `glDrawElements`
//! calls as possible. Looking across the patch from its centre that rejects roughly half the
//! blades -- the ones behind the camera -- for the price of 169 box tests.
//!
//! The draw disables back-face culling for its duration: a blade is one quad strip, seen from
//! either side, and the fragment shader shades it the same from both. The alternative the
//! engine left -- emitting every quad twice, once per winding -- is what the OBJ patch did, and
//! it doubled the geometry for nothing.
//!
//! # Height
//!
//! Blades sit on the plane the scene passes in (`ground_y`) PLUS the terrain height field, which
//! the vertex shader evaluates at each blade's root on a wrapping scene (`ext::terrain::height`,
//! mirrored in GLSL). The per-cell cull boxes take their y extent from the same function, sampled
//! on the CPU at the patch's current offset, so a cell on a hilltop is still found in frame.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use glow::HasContext;

use crate::camera::Camera;
use crate::ext::cull::Frustum;
use crate::ext::grassgen::{self, Cell};
use crate::object::{Object, ObjectT, RenderCtx, UpdateCtx};
use crate::resources::Resources;
use crate::vector::Vector3;

/// Snap cell. Small enough that the jump is never noticed, large enough that it is rare. A
/// whole multiple of the cull cell, so the cull grid lands on the same world phase every snap.
const SNAP: f32 = 2.0;

/// Slack added around each cull cell's rest-pose box, in world units. The box has to hold a
/// blade wherever the wind puts it, or a cell at the edge of the frame would pop in and out as
/// its blades waved across the boundary.
///
/// The bound comes from Shaders/grassblade.vert. The gust is `(g1 - 0.5) * 1.5 + (g2 - 0.5) *
/// 0.7` with both noises in [0, 1], so |gust| <= 1.1; the flutter is a sine times 0.12; the bend
/// at the tip is `gust * 0.55 + flutter` <= 0.725, applied along the wind and again at 0.35
/// along the lean, so a tip moves at most 0.725 * 1.35 = 0.98 units sideways -- under this pad.
/// Vertically a bent blade only drops, by `|sway| * 0.35` <= 0.34, under the half-pad used for
/// y. `sway_bound_matches_the_shader` fails if those factors change.
const SWAY_PAD: f32 = 1.2;

/// The uploaded patch: one VAO, one index buffer, and the cull cells. Shared by every
/// `GrassField` in a process through [`GrassMesh::acquire`].
pub struct GrassMesh {
    gl: Rc<glow::Context>,
    vao: glow::VertexArray,
    bufs: [glow::Buffer; 3],
    cells: Vec<Cell>,
}

thread_local! {
    /// Same acquire-or-build shape as `Resources` and `gltf_model::CACHE`: a `Weak`, so the
    /// mesh lives exactly as long as something holds it. `ExtState` holds one `Rc` for the life
    /// of the engine, which is what keeps title -> NEW GAME from regenerating and re-uploading
    /// the patch -- and what lets it be freed with the GL context still current, which a strong
    /// reference in a thread-local could not promise.
    static CACHE: RefCell<Weak<GrassMesh>> = const { RefCell::new(Weak::new()) };
}

impl GrassMesh {
    /// The process-wide patch, built on first use.
    pub fn acquire(gl: &Rc<glow::Context>) -> Rc<GrassMesh> {
        if let Some(hit) = CACHE.with(|c| c.borrow().upgrade()) {
            return hit;
        }
        let m = Rc::new(GrassMesh::build(gl));
        CACHE.with(|c| *c.borrow_mut() = Rc::downgrade(&m));
        m
    }

    fn build(gl: &Rc<glow::Context>) -> GrassMesh {
        let t0 = std::time::Instant::now();
        let patch = grassgen::generate();
        unsafe {
            let vao = gl.create_vertex_array().expect("create_vertex_array");
            gl.bind_vertex_array(Some(vao));
            let bufs = [
                gl.create_buffer().expect("create_buffer"),
                gl.create_buffer().expect("create_buffer"),
                gl.create_buffer().expect("create_buffer"),
            ];
            // Locations 0 and 1 are what `Shader` assigns to in_pos and in_uv: it scrapes the
            // vertex source for "\nin " declarations in order (shader.rs). grassblade.vert
            // declares exactly those two.
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(bufs[0]));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(&patch.pos), glow::STATIC_DRAW);
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 0, 0);
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(bufs[1]));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(&patch.uv), glow::STATIC_DRAW);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, 0, 0);
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(bufs[2]));
            gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, as_bytes(&patch.idx), glow::STATIC_DRAW);
            gl.bind_vertex_array(None);
            println!(
                "[grass] {} blades, {} vertices, {} indices in {:.0} ms",
                patch.pos.len() / grassgen::VERTS_PER_BLADE,
                patch.pos.len(),
                patch.idx.len(),
                t0.elapsed().as_secs_f32() * 1e3
            );
            GrassMesh { gl: Rc::clone(gl), vao, bufs, cells: patch.cells }
        }
    }
}

impl Drop for GrassMesh {
    fn drop(&mut self) {
        unsafe {
            for b in self.bufs {
                self.gl.delete_buffer(b);
            }
            self.gl.delete_vertex_array(self.vao);
        }
    }
}

/// Reinterpret a slice of POD arrays as bytes for `buffer_data_u8_slice`, as `mesh.rs` does.
fn as_bytes<T>(v: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

/// Ground height range under one cull cell at a given patch offset: the min and max of
/// `terrain::height` over the cell's footprint (padded by `SWAY_PAD`, since a swayed tip can
/// belong to a root that far away), sampled on a small lattice. The hills have wavelengths of
/// 85 units and more and the door knoll is 55 wide, so a 2-unit cell is very nearly planar and
/// a 3x3 lattice bounds it to millimetres; the slack in `SWAY_PAD` covers the rest.
fn ground_range(cell: &Cell, offset: Vector3) -> (f32, f32) {
    if crate::ext::view::wrap() <= 0.0 {
        return (0.0, 0.0);
    }
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for i in 0..3 {
        for j in 0..3 {
            let x = offset.x + cell.min[0] - SWAY_PAD + (cell.max[0] - cell.min[0] + 2.0 * SWAY_PAD) * i as f32 / 2.0;
            let z = offset.z + cell.min[2] - SWAY_PAD + (cell.max[2] - cell.min[2] + 2.0 * SWAY_PAD) * j as f32 / 2.0;
            let h = crate::ext::terrain::height(x, z);
            lo = lo.min(h);
            hi = hi.max(h);
        }
    }
    (lo, hi)
}

/// Visible cells of a grid at `offset`, as merged `(first index, count)` runs. Pure, so it can
/// be tested without a GL context.
///
/// Cells are tested in index order and consecutive visible cells are merged, so a fully visible
/// patch is ONE draw call and a typical view is a dozen or so (one per grid row that the frustum
/// crosses). `ground` gives each cell's terrain y range at this offset.
fn visible_runs(cells: &[Cell], ground: &[(f32, f32)], offset: Vector3, frustum: &Frustum) -> Vec<(u32, u32)> {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for (cell, &(g_lo, g_hi)) in cells.iter().zip(ground) {
        if cell.count == 0 {
            continue;
        }
        let min = Vector3::new(
            offset.x + cell.min[0] - SWAY_PAD,
            offset.y + cell.min[1] + g_lo - SWAY_PAD * 0.5,
            offset.z + cell.min[2] - SWAY_PAD,
        );
        let max = Vector3::new(
            offset.x + cell.max[0] + SWAY_PAD,
            offset.y + cell.max[1] + g_hi + SWAY_PAD * 0.5,
            offset.z + cell.max[2] + SWAY_PAD,
        );
        if !frustum.aabb(min, max) {
            continue;
        }
        match runs.last_mut() {
            Some(last) if last.0 + last.1 == cell.first => last.1 += cell.count,
            _ => runs.push((cell.first, cell.count)),
        }
    }
    runs
}

pub struct GrassField {
    base: Object,
    mesh: Rc<GrassMesh>,
    ground_y: f32,
    /// Terrain y range per cull cell at the patch's current offset. Recomputed on every snap
    /// in `update`, and read -- never written -- by `draw`, which portal recursion re-enters.
    ground: Vec<(f32, f32)>,
}

impl GrassField {
    pub fn new(gl: &Rc<glow::Context>, res: &Resources, ground_y: f32) -> GrassField {
        let mut base = Object::new();
        base.shader = Some(res.acquire_shader("grassblade"));
        // Same noise atlas the ground material uses, so patchiness agrees across the seam.
        base.texture = Some(res.acquire_texture("grass_noise.bmp", 1, 1));
        base.pos = Vector3::new(0.0, ground_y, 0.0);
        let mesh = GrassMesh::acquire(gl);
        let ground = mesh.cells.iter().map(|c| ground_range(c, base.pos)).collect();
        GrassField { base, mesh, ground_y, ground }
    }
}

impl ObjectT for GrassField {
    fn base(&self) -> &Object {
        &self.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base
    }

    /// Skip the blade field entirely in portal passes. A million triangles re-drawn into four
    /// levels of 2048x2048 framebuffer is the single most expensive thing in the game, and the
    /// ground's grass texture already covers that view.
    fn draw(&self, ctx: &RenderCtx, cam: &Camera, _fbo: Option<glow::Framebuffer>) {
        if crate::ext::view::detail() < 0.5 {
            return;
        }
        // Nothing to stand on past the mood split: the sea world has no meadow, and a patch
        // drawn there would be lifted by the periodic hill height and hang above the water.
        if ctx.eye.x > crate::ext::view::MOOD_SPLIT_X {
            return;
        }
        let runs = visible_runs(&self.mesh.cells, &self.ground, self.base.pos, &ctx.frustum);
        if runs.is_empty() {
            return;
        }
        let (Some(shader), Some(texture)) = (&self.base.shader, &self.base.texture) else { return };

        shader.use_program();
        texture.use_texture();
        // The same uniform set `Object::draw_impl` sends, minus `mvp`/`mv`: the blade shader
        // takes the view-projection and the patch's local-to-world separately, because it bends
        // blades in WORLD space (the wind field must not travel with the patch) and projecting
        // the result directly is two 4x4 inversions per vertex fewer than reconstructing it
        // from `mv` the way the original shader did.
        shader.set_mat4("vp", &cam.matrix());
        shader.set_mat4("l2w", &self.base.local_to_world());
        shader.set_f32("time", crate::ext::view::time());
        let eye = ctx.eye;
        shader.set_vec4("cam_pos", [eye.x, eye.y, eye.z, 1.0]);
        shader.set_f32("mood", crate::ext::view::mood_for(eye));
        shader.set_vec4("glow", crate::ext::view::glow());
        shader.set_f32("wrap", crate::ext::view::wrap());

        let gl = ctx.gl;
        unsafe {
            // Both sides of every blade; restored before returning because the engine enables
            // culling once at init (engine.rs) and everything else relies on it.
            gl.disable(glow::CULL_FACE);
            gl.bind_vertex_array(Some(self.mesh.vao));
            for (first, count) in runs {
                gl.draw_elements(
                    glow::TRIANGLES,
                    count as i32,
                    glow::UNSIGNED_INT,
                    (first as usize * std::mem::size_of::<u32>()) as i32,
                );
            }
            gl.bind_vertex_array(None);
            gl.enable(glow::CULL_FACE);
        }
    }

    fn update(&mut self, ctx: &UpdateCtx) {
        // Do not follow the player into the far world behind the door -- the sea, the backrooms
        // (see `draw`): the patch stays parked at its last meadow position, where the portal
        // pass never draws it anyway.
        if ctx.player_pos.x > crate::ext::view::MOOD_SPLIT_X {
            return;
        }
        let snap = |v: f32| (v / SNAP).round() * SNAP;
        let pos = Vector3::new(snap(ctx.player_pos.x), self.ground_y, snap(ctx.player_pos.z));
        if pos.x != self.base.pos.x || pos.z != self.base.pos.z {
            self.base.pos = pos;
            for (g, c) in self.ground.iter_mut().zip(&self.mesh.cells) {
                *g = ground_range(c, pos);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ext::grassgen::{CELL, CELLS, PATCH};

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

    /// The snap grid and the cull grid must agree, or the cells' world phase would change
    /// between snaps and a cell's box would no longer bound the same blades.
    #[test]
    fn snap_is_a_whole_number_of_cull_cells() {
        assert!(((SNAP / CELL) - (SNAP / CELL).round()).abs() < 1e-6);
        assert_eq!(CELLS * CELLS, grassgen::generate().cells.len());
    }

    /// A camera in the middle of the patch looking along -z sees the cells in front of it and
    /// none of the ones behind, and the visible cells merge into one run per grid row at most.
    #[test]
    fn culls_the_cells_behind_the_camera() {
        let patch = grassgen::generate();
        let offset = Vector3::new(0.0, 0.0, 0.0);
        let ground = vec![(0.0, 0.0); patch.cells.len()];
        let mut cam = Camera::new();
        cam.set_size(1600, 900, 0.1, 100.0);
        cam.set_position_orientation(Vector3::new(0.0, 1.5, 0.0), 0.0, 0.0);
        let f = Frustum::from_view_proj(&cam.matrix());
        let runs = visible_runs(&patch.cells, &ground, offset, &f);
        let drawn: u32 = runs.iter().map(|r| r.1).sum();
        let total = patch.idx.len() as u32;
        assert!(drawn > total / 4 && drawn < total * 3 / 4, "drew {drawn} of {total}");
        assert!(runs.len() <= CELLS, "{} runs", runs.len());
        // Every cell well inside the view -- in front of the camera and within the 45.75 degree
        // horizontal half-angle a 16:9 frame has at GH_FOV -- is in a run.
        for c in &patch.cells {
            let ahead = c.max[2] < -SWAY_PAD - 1.0;
            let wide = c.min[0].abs().max(c.max[0].abs()) + SWAY_PAD;
            if c.count > 0 && ahead && wide < c.max[2].abs() * 0.9 {
                assert!(runs.iter().any(|&(f, n)| c.first >= f && c.first + c.count <= f + n));
            }
        }
        // A camera well outside the patch with the patch behind it sees nothing of it.
        cam.set_position_orientation(Vector3::new(-60.0, 1.5, -60.0), 0.0, 0.0);
        let f = Frustum::from_view_proj(&cam.matrix());
        assert!(visible_runs(&patch.cells, &ground, offset, &f).is_empty());
    }

    /// `SWAY_PAD` is derived from the wind amplitudes in the vertex shader. If those change
    /// the pad may no longer contain a swayed tip, and the symptom -- a cell at the edge of
    /// the frame popping as its blades wave -- is the kind a screenshot does not show.
    #[test]
    fn sway_bound_matches_the_shader() {
        let src = std::fs::read_to_string("Shaders/grassblade.vert").expect("blade shader");
        for term in [
            "(g1 - 0.5) * 1.5 + (g2 - 0.5) * 0.7",
            "* 0.12;",
            "(gust * 0.55 + flutter) * t * t",
            "* bend * 0.35",
            "length(sway) * 0.35 * t",
        ] {
            assert!(src.contains(term), "grassblade.vert lost {term:?}; re-derive SWAY_PAD");
        }
        let tip = (1.1 * 0.55 + 0.12) * 1.35;
        assert!(tip < SWAY_PAD, "a tip can sway {tip} but the pad is {SWAY_PAD}");
        assert!(tip * 0.35 < SWAY_PAD * 0.5);
    }

    /// Runs are exact concatenations of whole cells.
    #[test]
    fn runs_are_contiguous_cells() {
        let patch = grassgen::generate();
        let ground = vec![(0.0, 0.0); patch.cells.len()];
        let mut cam = Camera::new();
        cam.set_size(1600, 900, 0.1, 100.0);
        cam.set_position_orientation(Vector3::new(3.0, 1.5, 2.0), -0.1, 0.7);
        let f = Frustum::from_view_proj(&cam.matrix());
        for (first, count) in visible_runs(&patch.cells, &ground, Vector3::zero(), &f) {
            let start = patch.cells.iter().position(|c| c.first == first).expect("run starts a cell");
            let mut n = 0;
            for c in &patch.cells[start..] {
                if n == count {
                    break;
                }
                n += c.count;
            }
            assert_eq!(n, count, "run at {first} is not whole cells");
        }
    }
}
