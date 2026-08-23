//! EXT: the grass blade patch, generated in-process. Not part of the C++ port.
//!
//! This replaces `Meshes/grass_patch.obj`, a 124 MB text file that `tools/gen_grass_houdini.py`
//! wrote and the OBJ parser spent 0.37 s re-reading on every intro load. The scatter it encoded
//! is a few hundred lines of arithmetic on a seeded random stream; running that here takes a
//! few milliseconds, ships nothing, and produces the mesh in the shape the renderer actually
//! wants, which the OBJ dialect could not express:
//!
//! * **Indexed.** Ten vertices per blade, shared by its four quads, instead of the parser's
//!   de-indexed nine floats per triangle -- 1.3 M vertices in place of 6.2 M.
//! * **One winding.** The OBJ carried every quad twice, once per winding, because the engine
//!   enables back-face culling globally and a scene has no way to turn it off; the blade field
//!   is drawn by its own code now, which disables culling for the duration of its draw. That
//!   alone halves the triangle count.
//! * **Bucketed into cells** so the field can be frustum-culled. Blades are sorted into a
//!   [`CELLS`] x [`CELLS`] grid over the patch, each cell's blades occupying one contiguous
//!   index range, and `ext::grassfield` draws only the cells a pass can see.
//!
//! The look is unchanged: same patch size, blade count, density falloff, heights, widths, arch
//! and taper as the script -- and the same per-vertex `(t, phase, lean)` channel the vertex
//! shader animates from. Only the random stream differs (a PCG32 rather than Python's Mersenne
//! Twister), which no one can tell apart. What does matter is that it is DETERMINISTIC: the
//! patch snaps to a grid around the player (grassfield.rs), and that illusion holds only if the
//! blades at a given offset are the same blades every frame.

/// Patch side in world units. Small, so the blade budget buys density where the player stands.
pub const PATCH: f32 = 26.0;
/// Full density within this radius of the centre; it thins out linearly past it.
const DENSE_R: f32 = 8.0;
/// A lawn, not bristles: real grass is dense enough to hide the soil.
pub const BLADES: usize = 130_000;
/// Quads per blade. Four lets a blade actually arch over.
pub const SEGMENTS: usize = 4;
const BLADE_H: (f32, f32) = (0.22, 0.55);
/// Wider than real grass: thin blades alias into wire at distance.
const BLADE_W: (f32, f32) = (0.020, 0.038);
const SEED: u64 = 11;

/// Cull cells per side. 2 units each over the 26-unit patch: small enough that a cell behind
/// the camera is rejected cleanly, few enough that testing them all is nothing.
pub const CELLS: usize = 13;
pub const CELL: f32 = PATCH / CELLS as f32;

/// Vertices per blade: two per ring, one ring per segment boundary.
pub const VERTS_PER_BLADE: usize = (SEGMENTS + 1) * 2;
/// Indices per blade: two triangles per segment.
pub const INDICES_PER_BLADE: usize = SEGMENTS * 6;

/// One cull cell: its blades' index range and their rest-pose bounds in patch-local space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    /// First index and index count, in the shared index buffer.
    pub first: u32,
    pub count: u32,
    /// Bounds of the vertices in this cell's blades, patch-local, at rest. The xz extent runs
    /// a little past the cell itself because an arched blade's tip reaches over the boundary;
    /// `y_max` is the tallest point. Empty cells have an inverted box (min > max).
    pub min: [f32; 3],
    pub max: [f32; 3],
}

/// The generated patch, ready for upload.
pub struct Patch {
    /// Positions, patch-local.
    pub pos: Vec<[f32; 3]>,
    /// `(t along blade, per-blade phase, lean angle)` -- `in_uv` in Shaders/grassblade.vert.
    pub uv: Vec<[f32; 3]>,
    /// Triangle list, one winding.
    pub idx: Vec<u32>,
    /// Row-major `CELLS x CELLS`, `cells[row * CELLS + col]`, row along z and col along x.
    pub cells: Vec<Cell>,
}

/// PCG32 (O'Neill), the minimal generator: 64-bit LCG state, 32-bit xorshift-rotate output.
/// Chosen over xorshift because a poor seed cannot degenerate it, and because its stream is
/// fixed for life by this constant -- the snap illusion depends on the same blades every run.
struct Pcg32 {
    state: u64,
}

impl Pcg32 {
    const MUL: u64 = 6364136223846793005;
    const INC: u64 = 1442695040888963407;

    fn new(seed: u64) -> Pcg32 {
        let mut g = Pcg32 { state: 0 };
        g.next_u32();
        g.state = g.state.wrapping_add(seed);
        g.next_u32();
        g
    }

    fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(Self::MUL).wrapping_add(Self::INC);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Uniform in `[0, 1)`, from the top 24 bits so every value is exactly representable.
    fn unit(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (1.0 / 16_777_216.0)
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }
}

/// Which cell a patch-local xz lands in. Positions are inside the patch by construction, but
/// clamp anyway: a point exactly on the +edge would otherwise index one past the grid.
pub fn cell_of(x: f32, z: f32) -> (usize, usize) {
    let col = (((x + PATCH * 0.5) / CELL).floor() as isize).clamp(0, CELLS as isize - 1) as usize;
    let row = (((z + PATCH * 0.5) / CELL).floor() as isize).clamp(0, CELLS as isize - 1) as usize;
    (row, col)
}

/// Scatter blade roots with the radial density falloff, in the disc that covers the patch.
/// Returns `(x, z, height random, width random, lean random)` per blade.
fn scatter(rng: &mut Pcg32) -> Vec<[f32; 5]> {
    let half = PATCH / 2.0;
    let mut out = Vec::with_capacity(BLADES);
    let mut tries = 0;
    while out.len() < BLADES && tries < BLADES * 40 {
        tries += 1;
        let x = rng.range(-half, half);
        let z = rng.range(-half, half);
        let r = (x * x + z * z).sqrt();
        if r > half {
            continue;
        }
        // Keep everything close in; thin out with distance so the far half costs little.
        let keep = if r <= DENSE_R {
            1.0
        } else {
            (1.0 - (r - DENSE_R) / (half - DENSE_R)).max(0.06)
        };
        if rng.unit() <= keep {
            out.push([x, z, rng.unit(), rng.unit(), rng.unit()]);
        }
    }
    out
}

/// Build the patch. Deterministic: two calls produce identical buffers.
pub fn generate() -> Patch {
    let mut rng = Pcg32::new(SEED);
    let roots = scatter(&mut rng);

    // Bucket by cell, keeping scatter order inside a cell. Counting sort: the grid is tiny and
    // the blade list is not.
    let mut counts = vec![0usize; CELLS * CELLS];
    for r in &roots {
        let (row, col) = cell_of(r[0], r[1]);
        counts[row * CELLS + col] += 1;
    }
    let mut starts = vec![0usize; CELLS * CELLS + 1];
    for i in 0..CELLS * CELLS {
        starts[i + 1] = starts[i] + counts[i];
    }
    let mut order = vec![0usize; roots.len()];
    let mut fill = starts.clone();
    for (i, r) in roots.iter().enumerate() {
        let (row, col) = cell_of(r[0], r[1]);
        let c = row * CELLS + col;
        order[fill[c]] = i;
        fill[c] += 1;
    }

    let n_blades = roots.len();
    let mut pos: Vec<[f32; 3]> = Vec::with_capacity(n_blades * VERTS_PER_BLADE);
    let mut uv: Vec<[f32; 3]> = Vec::with_capacity(n_blades * VERTS_PER_BLADE);
    let mut idx: Vec<u32> = Vec::with_capacity(n_blades * INDICES_PER_BLADE);
    let mut cells = Vec::with_capacity(CELLS * CELLS);

    // Per-blade randoms (phase, curve) are drawn in SCATTER order, not cell order, so that the
    // stream a blade sees does not depend on which cell it happened to fall in.
    let extra: Vec<(f32, f32)> = (0..n_blades)
        .map(|_| {
            let phase = rng.unit();
            // Real grass ARCHES -- blades fall away from vertical under their own weight.
            // Straight blades are what made the first version read as a bed of nails.
            let curve = 0.45 + 0.85 * rng.unit();
            (phase, curve)
        })
        .collect();

    for c in 0..CELLS * CELLS {
        let first = idx.len() as u32;
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];
        for &i in &order[starts[c]..starts[c + 1]] {
            let [x, z, ra, rb, rc] = roots[i];
            let (phase, curve) = extra[i];
            let h = BLADE_H.0 + (BLADE_H.1 - BLADE_H.0) * ra;
            let w = BLADE_W.0 + (BLADE_W.1 - BLADE_W.0) * rb;
            let lean = rc * std::f32::consts::TAU;
            let (dx, dz) = (lean.cos(), lean.sin());
            // Width axis is perpendicular to the lean, so blades present a face when bent.
            let (px, pz) = (-dz, dx);

            let base = pos.len() as u32;
            for s in 0..=SEGMENTS {
                let t = s as f32 / SEGMENTS as f32;
                let y = h * t;
                // Rest curve: quadratic in t, so the tip arches over and the base stays planted.
                let off = curve * h * t * t;
                let (cx, cz) = (x + dx * off, z + dz * off);
                // Taper toward the tip but keep some width most of the way, like a real blade.
                let half_w = w * (1.0 - 0.85 * t * t) * 0.5;
                for p in [
                    [cx - px * half_w, y, cz - pz * half_w],
                    [cx + px * half_w, y, cz + pz * half_w],
                ] {
                    for k in 0..3 {
                        min[k] = min[k].min(p[k]);
                        max[k] = max[k].max(p[k]);
                    }
                    pos.push(p);
                    uv.push([t, phase, lean]);
                }
            }
            for s in 0..SEGMENTS as u32 {
                let i0 = base + s * 2;
                idx.extend_from_slice(&[i0, i0 + 1, i0 + 3, i0, i0 + 3, i0 + 2]);
            }
        }
        cells.push(Cell { first, count: idx.len() as u32 - first, min, max });
    }

    Patch { pos, uv, idx, cells }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = generate();
        let b = generate();
        assert_eq!(a.pos, b.pos);
        assert_eq!(a.uv, b.uv);
        assert_eq!(a.idx, b.idx);
        assert_eq!(a.cells, b.cells);
    }

    #[test]
    fn blade_count_and_layout() {
        let p = generate();
        let blades = p.pos.len() / VERTS_PER_BLADE;
        // The scatter aims for BLADES and the rejection loop has 40x the tries it needs.
        assert_eq!(blades, BLADES, "scatter fell short");
        assert_eq!(p.pos.len(), BLADES * VERTS_PER_BLADE);
        assert_eq!(p.uv.len(), p.pos.len());
        assert_eq!(p.idx.len(), BLADES * INDICES_PER_BLADE);
        assert_eq!(p.cells.len(), CELLS * CELLS);
        for i in &p.idx {
            assert!((*i as usize) < p.pos.len());
        }
        // t runs 0..1 along each blade and the root is on the ground.
        for b in 0..blades {
            let v = &p.uv[b * VERTS_PER_BLADE..(b + 1) * VERTS_PER_BLADE];
            assert_eq!(v[0][0], 0.0);
            assert_eq!(v[VERTS_PER_BLADE - 1][0], 1.0);
            assert_eq!(p.pos[b * VERTS_PER_BLADE][1], 0.0);
        }
    }

    /// The cells must partition the index buffer exactly: every index in exactly one cell, in
    /// order, with no gaps -- that is what lets adjacent visible cells merge into one draw.
    #[test]
    fn cells_cover_all_indices_exactly_once() {
        let p = generate();
        let mut next = 0u32;
        let mut non_empty = 0;
        for c in &p.cells {
            assert_eq!(c.first, next, "gap or overlap before cell at {next}");
            assert_eq!(c.count % INDICES_PER_BLADE as u32, 0, "cell splits a blade");
            next += c.count;
            if c.count > 0 {
                non_empty += 1;
            }
        }
        assert_eq!(next as usize, p.idx.len());
        // The scatter is a disc inscribed in the square, so the corner cells are empty --
        // five per corner -- and every other cell has blades.
        assert_eq!(non_empty, CELLS * CELLS - 20, "{non_empty} cells have blades");
    }

    /// Every vertex a cell claims lies inside that cell's box, and the box does not stray far
    /// beyond the cell's own footprint -- the culling is only as tight as this.
    #[test]
    fn cell_bounds_contain_their_blades() {
        let p = generate();
        let max_reach = BLADE_H.1 * 1.3 + BLADE_W.1; // arch at the steepest curve, plus width
        for (c, cell) in p.cells.iter().enumerate() {
            if cell.count == 0 {
                continue;
            }
            let (row, col) = (c / CELLS, c % CELLS);
            let x0 = -PATCH * 0.5 + col as f32 * CELL;
            let z0 = -PATCH * 0.5 + row as f32 * CELL;
            for i in &p.idx[cell.first as usize..(cell.first + cell.count) as usize] {
                let v = p.pos[*i as usize];
                for (k, &vk) in v.iter().enumerate() {
                    assert!(vk >= cell.min[k] - 1e-6 && vk <= cell.max[k] + 1e-6);
                }
            }
            assert!(cell.min[0] >= x0 - max_reach && cell.max[0] <= x0 + CELL + max_reach);
            assert!(cell.min[2] >= z0 - max_reach && cell.max[2] <= z0 + CELL + max_reach);
            assert!(cell.min[1] >= 0.0 && cell.max[1] <= BLADE_H.1 + 1e-6);
        }
    }

    /// Density falls off toward the edge: the centre cell must hold many more blades than a
    /// corner cell, or the falloff was lost in the port.
    #[test]
    fn density_falls_off_with_radius() {
        let p = generate();
        let mid = p.cells[(CELLS / 2) * CELLS + CELLS / 2].count;
        let edge = p.cells[CELLS / 2].count; // middle of the first row: 12 units out
        assert!(mid > edge * 4, "centre {mid} vs edge {edge}");
    }

    #[test]
    fn cell_of_clamps_to_the_grid() {
        assert_eq!(cell_of(-PATCH * 0.5, -PATCH * 0.5), (0, 0));
        assert_eq!(cell_of(PATCH * 0.5, PATCH * 0.5), (CELLS - 1, CELLS - 1));
        assert_eq!(cell_of(0.0, 0.0), (CELLS / 2, CELLS / 2));
    }
}
