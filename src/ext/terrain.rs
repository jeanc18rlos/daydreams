//! EXT: the intro meadow's ground -- rolling hills on a **flat torus**. Not part of the C++ port.
//!
//! # What "walk any way and you come back" actually means here
//!
//! The meadow is a square of side [`PERIOD`] whose opposite edges are the same edge. Walk north
//! far enough and you arrive from the south; walk in any direction long enough and you return to
//! the door. That is a flat torus: **closed like a planet, but with no curvature anywhere**, which
//! is the point -- a sphere cannot be covered without stretching, and a hyperbolic plane would
//! show its curvature and never bring you back. A torus is the one closed surface you can walk on
//! and never see the trick.
//!
//! # How it is done, and why nothing gives it away
//!
//! Every step, the player's x and z are wrapped into `[-PERIOD/2, PERIOD/2)`. That is the whole
//! mechanism -- there are no boundary portals and no teleport effect. It is invisible for one
//! reason only: **everything you can see is exactly PERIOD-periodic**, so moving the player by
//! exactly one period changes nothing on screen. Concretely that requires
//!
//!   * this height field, which is built from integer harmonics of `PERIOD` so it tiles exactly;
//!   * the terrain drawn as one tile repeated on the lattice, out past the distance haze;
//!   * the door replicated on the same lattice, so a copy always stands where you are heading;
//!   * the grass materials snapping their world-space noise frequencies to whole cycles per
//!     period (`view::wrap`, and `snap_freq` in the grass shaders).
//!
//! Break any one of those and the wrap stops being invisible: the ground jumps, or the grass
//! patchiness slides, or a door appears out of nowhere.
//!
//! # One definition of the ground
//!
//! [`height`] is the single source of truth. The tile mesh is generated from it (`--gen-terrain`),
//! its colliders are generated from it, and `Shaders/grassblade.vert` re-implements it so blades
//! sit on the hills instead of hovering at y=0. `tile_matches_height_field` guards the mesh copy;
//! the shader copy is guarded by the blades visibly standing on the ground.

use crate::camera::Camera;
use crate::object::{Object, ObjectT, RenderCtx};
use crate::resources::Resources;
use crate::vector::Vector3;

/// Side of the fundamental square, in world units. Walking this far in x or z brings you back
/// to where you started -- about a minute and a half at walking pace.
///
/// It cannot be made small to shorten the walk. The camera's horizontal half-FOV is 45.75 deg at
/// 16:9 (`Camera::set_size` scales the 30 deg vertical by height/width), so at `GH_FAR` = 100 the
/// ground is drawn out to 143 units and the visible wedge is over 205 units across. **A period
/// shorter than that puts two copies of the same hill on screen at once**, and the illusion
/// collapses from "a closed world" into "a tiled texture". 256 clears it with margin.
///
/// It must also be a whole multiple of `ext::grassfield::SNAP`, or the blade patch would land on
/// a different phase of its snap grid after a wrap and every blade would visibly shift.
pub const PERIOD: f32 = 256.0;

/// Peak-to-trough hill height. Bounded by the player's ground test: `Player::on_collide` only
/// counts a surface as ground when the push normal's y exceeds 0.7, so the terrain must stay
/// gentler than that everywhere or there would be hillsides you slide down instead of climb.
/// `slopes_are_walkable` checks it.
const AMP: f32 = 8.0;

/// Where the door stands. The player spawns here, and the ground is shaped around it.
pub const DOOR_X: f32 = 0.0;
pub const DOOR_Z: f32 = -6.0;

/// The knoll the door stands on: its height, its radius, and the radius over which the hill
/// noise is faded out beneath it.
///
/// On a closed world the door is the only landmark there is, so it has to be the thing you can
/// always see -- but a door is two units tall and the hills are eight, so simply dropping it on
/// the meadow buries it. The first attempt raised it on a knoll and that was not enough either:
/// the noise kept running straight over the top, leaving the door in a saddle with a ridge to the
/// north of it that was taller than the knoll.
///
/// What works is CLEAR_R, not height. Fading the noise out across a wide area lets the smooth
/// dome win near the door, so the door really is the local summit; the hills resume outside it
/// and the meadow keeps its relief. Raising the knoll instead just makes a steeper mound that the
/// surrounding noise still climbs over. These three were chosen by sweeping them against the
/// fraction of sightlines that reach the door from sixteen directions at four ranges, subject to
/// slopes staying walkable and the far meadow staying hilly -- see `door_is_visible_across_the_meadow`.
const CREST_H: f32 = 8.0;
const CREST_R: f32 = 55.0;
const CLEAR_R: f32 = 70.0;

/// Ground height at the door, and so the y the door and the player spawn sit at.
pub const DOOR_Y: f32 = CREST_H;

/// Shortest signed distance on a circle of circumference `PERIOD`.
///
/// On a torus every pair of points has infinitely many separations, and the one that matters is
/// the shortest. Note that `ext::door` deliberately does NOT use this for its opening radius: the
/// door stands a half-period from the seam, so no approach to it ever wraps, and applying a
/// period to the sea-side door -- which lives outside the wrapped region entirely -- would fold
/// its distance to the player into something meaningless.
pub fn wrap_delta(d: f32) -> f32 {
    let h = PERIOD * 0.5;
    (d + h).rem_euclid(PERIOD) - h
}

/// Ground height at a world position.
///
/// A sum of sinusoids whose frequencies are whole numbers of cycles per `PERIOD`, so it is
/// exactly periodic by construction -- unlike sampled noise, which only tiles if its lattice
/// happens to line up and which cannot be re-evaluated in a shader.
pub fn height(x: f32, z: f32) -> f32 {
    let u = x * (std::f32::consts::TAU / PERIOD);
    let v = z * (std::f32::consts::TAU / PERIOD);
    let h = 0.55 * u.sin() * v.cos()
        + 0.28 * (2.0 * u + 1.7).sin() * (3.0 * v - 0.4).cos()
        + 0.17 * (3.0 * u - 0.9).sin() * (v + 2.1).cos();
    // Both features around the door are measured with WRAPPED deltas, so each is one patch on the
    // torus rather than a disc cut in half at the seam.
    let dx = wrap_delta(x - DOOR_X);
    let dz = wrap_delta(z - DOOR_Z);
    let d2 = dx * dx + dz * dz;
    // Fade the hills out under the door, and raise a smooth dome in their place. Both are
    // Gaussians centred on the door, so both have zero gradient exactly at it -- which is what
    // leaves the door standing level without needing a flat pad cut into the ground.
    let clear = (-d2 / (CLEAR_R * CLEAR_R)).exp();
    let crest = CREST_H * (-d2 / (CREST_R * CREST_R)).exp();
    AMP * h * (1.0 - clear) + crest
}

/// Surface normal, by central differences on [`height`].
pub fn normal(x: f32, z: f32) -> Vector3 {
    let e = 0.35;
    let gx = (height(x + e, z) - height(x - e, z)) / (2.0 * e);
    let gz = (height(x, z + e) - height(x, z - e)) / (2.0 * e);
    Vector3::new(-gx, 1.0, -gz).normalized_safe()
}

/// Render grid of the tile mesh. 128 cells over 256 units is 2.0 units per quad, which subtends
/// about 1.2 degrees on the horizon at the far plane -- fine enough that a hill's silhouette
/// reads as a curve rather than a row of facets.
const VIS_N: usize = 128;
/// Collider grid. Coarser than the render grid on purpose: the collision pass is
/// `objects x spheres x colliders` at 500 Hz, so this is the number that has to stay small.
///
/// But not too coarse. The walk shell is a field of flat plates, one per cell, each tilted to the
/// local gradient -- so where the ground curves faster than a cell can follow, neighbouring plates
/// end up at different heights and leave a vertical gap between them. At 8 units per cell the
/// player fell straight through the ground beside the door, where the level pad bends the surface
/// by several units inside one cell. 4 units per cell, plus the overlap below, closes it;
/// `walk_shell_tracks_the_ground` is the check that keeps it closed.
const COL_N: usize = 64;

/// Tiles drawn each way from the origin: a fixed 3x3 lattice.
///
/// Fixed, not player-following: the wrap keeps the player inside the middle cell forever, so the
/// lattice never needs to move. It reaches +/-384, and the furthest the player can ever see is
/// 128 (their side of the cell) + 143 (the far plane's reach) = 271. The blade patch follows the
/// player because it is *smaller* than the view; this tile is not, and copying that pattern here
/// would be motion with nothing to show for it.
const RINGS: i32 = 1;

/// How much wider each collider plate is than its cell.
const OVERLAP: f32 = 1.35;

/// Height of the walk shell's plate at a world position, as the collision pass sees it: the plate
/// of the cell containing the point, evaluated at that point. Test-only -- it reproduces what the
/// engine's collider sweep will find, so `walk_shell_tracks_the_ground` can check it against the
/// surface actually being drawn without needing to run the engine.
#[cfg(test)]
fn shell_height(x: f32, z: f32) -> f32 {
    let cstep = PERIOD / COL_N as f32;
    let cj = ((x + PERIOD * 0.5) / cstep).floor();
    let ci = ((z + PERIOD * 0.5) / cstep).floor();
    let cx = -PERIOD * 0.5 + (cj + 0.5) * cstep;
    let cz = -PERIOD * 0.5 + (ci + 0.5) * cstep;
    let e = cstep * 0.5;
    let gx = (height(cx + e, cz) - height(cx - e, cz)) / (2.0 * e);
    let gz = (height(cx, cz + e) - height(cx, cz - e)) / (2.0 * e);
    height(cx, cz) + gx * (x - cx) + gz * (z - cz)
}

pub const TILE_MESH: &str = "meadow_tile.obj";

/// World-space bounding box of the lattice tile at `(i, j)`: the tile's square, and the full
/// range the height field can take -- the hills' `AMP` either way plus the door's knoll, with a
/// unit of slack. The ported renderer draws every object in every pass, and a portal pass from
/// the sea world a thousand units away was pushing all nine tiles through the vertex stage to
/// be discarded at the far plane; this is what lets `Terrain::draw` skip them.
pub fn tile_aabb(i: i32, j: i32) -> (Vector3, Vector3) {
    let h = PERIOD * 0.5;
    let (cx, cz) = (i as f32 * PERIOD, j as f32 * PERIOD);
    (
        Vector3::new(cx - h, -AMP - 1.0, cz - h),
        Vector3::new(cx + h, AMP + CREST_H + 1.0, cz + h),
    )
}

/// The visible ground: one tile mesh, drawn nine times on the lattice.
///
/// Deliberately ONE scene object drawing nine times, rather than nine objects. The collision pass
/// walks `v_objects` and sweeps every collider of every object's mesh with no broadphase and no
/// sharing check (engine.rs:669-690), so nine objects holding the same `Rc<Mesh>` would cost nine
/// full collider sweeps per hit sphere per 500 Hz step. As one object it is a single sweep at the
/// identity transform -- and because the player is wrapped, the middle tile is the only one they
/// can ever be standing on.
pub struct Terrain {
    base: Object,
}

impl Terrain {
    pub fn new(res: &Resources) -> Terrain {
        let mut base = Object::new();
        base.mesh = Some(res.acquire_mesh(TILE_MESH));
        base.shader = Some(res.acquire_shader("grass"));
        base.texture = Some(res.acquire_texture("grass_noise.bmp", 1, 1));
        Terrain { base }
    }
}

impl ObjectT for Terrain {
    fn base(&self) -> &Object {
        &self.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base
    }

    fn draw(&self, ctx: &RenderCtx, cam: &Camera, _fbo: Option<glow::Framebuffer>) {
        let rings = RINGS;
        // draw_impl reads the position off the Object, and `draw` takes &self because portal
        // recursion re-enters it -- so the lattice is walked on a LOCAL copy. A RefCell scratch
        // object would deadlock here: Portal::Draw re-enters Engine::Render, which re-enters
        // this very method, and the second mutable borrow would panic.
        let mut tile = Object::new();
        tile.mesh = self.base.mesh.clone();
        tile.shader = self.base.shader.clone();
        tile.texture = self.base.texture.clone();
        for i in -rings..=rings {
            for j in -rings..=rings {
                let (lo, hi) = tile_aabb(i, j);
                if !ctx.frustum.aabb(lo, hi) {
                    continue;
                }
                tile.pos = Vector3::new(i as f32 * PERIOD, 0.0, j as f32 * PERIOD);
                tile.draw_impl(ctx, cam);
            }
        }
    }
}

/// Generate `Meshes/meadow_tile.obj` from [`height`]. Run with `--gen-terrain`.
///
/// The mesh has to exist as a file because colliders live on `Mesh`, which only loads from disk
/// (mesh.rs) -- but it is derived, not authored, so it is generated from the same function the
/// shaders and the collision shell use rather than being drawn by hand in a modelling tool.
pub fn generate() -> std::io::Result<()> {
    use std::fmt::Write as _;
    let step = PERIOD / VIS_N as f32;
    let at = |i: usize, j: usize| {
        let x = -PERIOD * 0.5 + j as f32 * step;
        let z = -PERIOD * 0.5 + i as f32 * step;
        (x, z)
    };

    let mut s = String::new();
    s.push_str(
        "# EXT asset: one tile of the intro meadow's toroidal ground.\n\
         # GENERATED by `daydreams --gen-terrain` from ext::terrain::height -- do not edit.\n\
         # `vt` carries the SMOOTH NORMAL in 3 components: the engine discards `vn` and\n\
         # recomputes flat per-face normals (Mesh.cpp:187), so this is the only way to ship\n\
         # smooth shading. The grass shader reads in_uv as the normal.\n\
         # `c` lines: gradient-tilted rectangle colliders on a coarser grid (the walk shell).\n\
         # The height field is periodic over the tile, so copies tile seamlessly in x and z.\n",
    );
    // The edge row/column repeat the opposite edge exactly, because `height` is periodic --
    // that is what makes neighbouring tiles join without a crack.
    for i in 0..=VIS_N {
        for j in 0..=VIS_N {
            let (x, z) = at(i, j);
            let _ = writeln!(s, "v {:.4} {:.4} {:.4}", x, height(x, z), z);
        }
    }
    for i in 0..=VIS_N {
        for j in 0..=VIS_N {
            let (x, z) = at(i, j);
            let n = normal(x, z);
            let _ = writeln!(s, "vt {:.4} {:.4} {:.4}", n.x, n.y, n.z);
        }
    }
    let idx = |i: usize, j: usize| i * (VIS_N + 1) + j + 1;
    for i in 0..VIS_N {
        for j in 0..VIS_N {
            let (a, b, c, d) = (idx(i, j), idx(i, j + 1), idx(i + 1, j + 1), idx(i + 1, j));
            // CCW seen from above: the engine culls back faces.
            let _ = writeln!(s, "f {a}/{a} {d}/{d} {c}/{c}");
            let _ = writeln!(s, "f {a}/{a} {c}/{c} {b}/{b}");
        }
    }

    // Colliders: one tilted rectangle per coarse cell, its legs made exactly orthogonal so
    // Collider::new's "longest edge is the diagonal" rule picks the right corner (collider.rs).
    let cstep = PERIOD / COL_N as f32;
    let mut cverts: Vec<Vector3> = Vec::new();
    let base = (VIS_N + 1) * (VIS_N + 1);
    let mut worst: f32 = 0.0;
    // One extra ring beyond the tile: the player is wrapped AT the boundary, so the cell they
    // land in must already be covered rather than starting exactly under their feet.
    for ci in -1..=COL_N as i32 {
        for cj in -1..=COL_N as i32 {
            let cx = -PERIOD * 0.5 + (cj as f32 + 0.5) * cstep;
            let cz = -PERIOD * 0.5 + (ci as f32 + 0.5) * cstep;
            let e = cstep * 0.5;
            let gx = (height(cx + e, cz) - height(cx - e, cz)) / (2.0 * e);
            let gz = (height(cx, cz + e) - height(cx, cz - e)) / (2.0 * e);
            worst = worst.max((gx * gx + gz * gz).sqrt());
            let u = Vector3::new(1.0, gx, 0.0);
            let mut v = Vector3::new(0.0, gz, 1.0);
            // Gram-Schmidt, so the rectangle's legs are exactly perpendicular.
            let k = u.dot(v) / u.dot(u);
            v = v - u * k;
            // Overlap generously. Plates are flat and the ground is not, so neighbours meet at
            // slightly different heights; the overlap is what bridges the step between them.
            let h = cstep * 0.5 * OVERLAP;
            let u = u * h;
            let v = v * (h / v.z);
            let c = Vector3::new(cx, height(cx, cz), cz);
            cverts.extend([c - u - v, c + u - v, c - u + v]);
        }
    }
    for v in &cverts {
        let _ = writeln!(s, "v {:.4} {:.4} {:.4}", v.x, v.y, v.z);
    }
    for t in 0..cverts.len() / 3 {
        let k = base + t * 3 + 1;
        let _ = writeln!(s, "c {} {} {}", k, k + 1, k + 2);
    }

    let path = format!("Meshes/{TILE_MESH}");
    std::fs::write(&path, s)?;
    let ny = 1.0 / (1.0 + worst * worst).sqrt();
    println!(
        "{path}: {} verts, {} tris, {} colliders; steepest normal.y {ny:.2}",
        (VIS_N + 1) * (VIS_N + 1),
        2 * VIS_N * VIS_N,
        cverts.len() / 3
    );
    Ok(())
}

/// Wrap the player back into the fundamental square. Call once per fixed step, AFTER the portal
/// pass -- see the comment at the call site in `Engine::update`.
pub fn wrap_player(p: &mut crate::player::Player) {
    // Only on a wrapping scene, and never in the far world: after a door transit the player is
    // at x ~ 500, and wrapping that would fling them out of the sea and across the mood split.
    if crate::ext::view::wrap() <= 0.0 || p.base.base.pos.x > crate::ext::view::MOOD_SPLIT_X {
        return;
    }
    // A period is a fixed distance, so it only means anything at unit scale. Level 15's portal
    // warp is a pure translation, so this always holds -- but assert rather than assume, because
    // a scaled warp would make the wrap teleport the player somewhere arbitrary.
    debug_assert!(
        (p.base.base.p_scale - 1.0).abs() < 1e-3,
        "toroidal wrap at p_scale {}",
        p.base.base.p_scale
    );

    let pos = p.base.base.pos;
    let shift = Vector3::new(wrap_delta(pos.x) - pos.x, 0.0, wrap_delta(pos.z) - pos.z);
    if shift.x == 0.0 && shift.z == 0.0 {
        return;
    }
    p.base.base.pos = pos + shift;
    // prev_pos moves by the SAME amount. The portal pass tests the segment prev_pos -> pos for a
    // crossing (physical.rs, portal.rs), so leaving prev_pos behind would hand it a segment
    // stretching a whole period across the meadow -- which sweeps the doorway and teleports the
    // player into the sea. Shifting both leaves the segment identical, just relocated.
    p.base.prev_pos = p.base.prev_pos + shift;
    // Deliberately NOT Physical::set_position: that collapses prev_pos onto pos, which zeroes the
    // player's step distance for one frame and makes the head-bob stutter at every wrap.
    // Velocity, euler and the camera angles are untouched -- this is a change of chart, not of
    // motion, and the player must not be able to feel it.
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wrap is only invisible if the ground is EXACTLY periodic. A crack at the tile seam
    /// would be a visible line across the meadow.
    #[test]
    fn height_field_is_exactly_periodic() {
        for (x, z) in [(0.0f32, 0.0f32), (13.7, -41.2), (-60.0, 63.0), (5.5, 5.5)] {
            for (dx, dz) in [(PERIOD, 0.0), (0.0, PERIOD), (-PERIOD, PERIOD), (2.0 * PERIOD, 0.0)] {
                let a = height(x, z);
                let b = height(x + dx, z + dz);
                assert!((a - b).abs() < 1e-3, "({x},{z}) vs +({dx},{dz}): {a} != {b}");
            }
        }
    }

    /// `Player::on_collide` only treats a surface as ground when the push normal's y is above
    /// 0.7. A steeper hillside would be un-walkable, and on a torus there is no way around it.
    #[test]
    fn slopes_are_walkable() {
        let mut worst: f32 = 1.0;
        let n = 96;
        for i in 0..n {
            for j in 0..n {
                let x = -PERIOD * 0.5 + PERIOD * i as f32 / n as f32;
                let z = -PERIOD * 0.5 + PERIOD * j as f32 / n as f32;
                worst = worst.min(normal(x, z).y);
            }
        }
        assert!(worst > 0.78, "steepest slope has normal.y {worst}, too steep to walk");
    }

    /// The door stands on level ground, or it would hang crooked in its frame -- and that ground
    /// is [`DOOR_Y`], which level 15 sits the door and the player spawn on.
    #[test]
    fn door_stands_on_a_level_pad() {
        assert!((height(DOOR_X, DOOR_Z) - DOOR_Y).abs() < 0.02, "{}", height(DOOR_X, DOOR_Z));
        assert!(normal(DOOR_X, DOOR_Z).y > 0.999);
    }

    /// The door has to be findable. On a closed world it is the only landmark, so if the hills
    /// hide it the player has nothing to walk towards and the level stops working -- which is
    /// exactly what happened when the knoll was too small and left it sitting in a saddle.
    #[test]
    fn door_is_visible_across_the_meadow() {
        let top = height(DOOR_X, DOOR_Z) + 2.0; // the door's own height
        let (mut seen, mut total) = (0, 0);
        for a in 0..16 {
            let th = a as f32 * std::f32::consts::TAU / 16.0;
            for r in [25.0f32, 40.0, 60.0, 80.0] {
                let (x0, z0) = (DOOR_X + r * th.cos(), DOOR_Z + r * th.sin());
                let eye = height(x0, z0) + 1.5;
                let mut clear = true;
                for k in 1..160 {
                    let t = k as f32 / 160.0;
                    let x = x0 + t * (DOOR_X - x0);
                    let z = z0 + t * (DOOR_Z - z0);
                    if height(x, z) > eye + t * (top - eye) + 0.05 {
                        clear = false;
                        break;
                    }
                }
                total += 1;
                seen += clear as i32;
            }
        }
        assert!(seen * 10 >= total * 9, "door visible from only {seen}/{total} vantage points");

        // ...and the meadow beyond the knoll still has hills, or the world is a featureless dome.
        let mut relief: f32 = 0.0;
        for i in 0..96 {
            for j in 0..96 {
                let x = -PERIOD * 0.5 + PERIOD * i as f32 / 96.0;
                let z = -PERIOD * 0.5 + PERIOD * j as f32 / 96.0;
                let d = (wrap_delta(x - DOOR_X).powi(2) + wrap_delta(z - DOOR_Z).powi(2)).sqrt();
                if d > 80.0 {
                    relief = relief.max(height(x, z).abs());
                }
            }
        }
        assert!(relief > 3.0, "meadow away from the door is flat: relief {relief}");
        // ...and the pad is local: the meadow is not flattened everywhere.
        let mut peak: f32 = 0.0;
        for i in 0..64 {
            for j in 0..64 {
                let x = -PERIOD * 0.5 + PERIOD * i as f32 / 64.0;
                let z = -PERIOD * 0.5 + PERIOD * j as f32 / 64.0;
                peak = peak.max(height(x, z).abs());
            }
        }
        assert!(peak > 2.0, "meadow is flat: peak height {peak}");
    }

    /// `Shaders/grassblade.vert` re-implements `height` so blades stand on the hills. GLSL cannot
    /// include Rust, so the constants are duplicated -- and a silent drift between them would put
    /// the blades on a different surface than the one being drawn, which reads as grass sinking
    /// into the ground rather than as an obvious bug. Check the copy.
    #[test]
    fn terrain_constants_match_the_shader() {
        let src = std::fs::read_to_string("Shaders/grassblade.vert").expect("blade shader");
        for (name, want) in [
            ("#define T_AMP", AMP),
            ("#define T_CLEAR_R", CLEAR_R),
            ("#define T_CREST_H", CREST_H),
            ("#define T_CREST_R", CREST_R),
        ] {
            let line = src
                .lines()
                .find(|l| l.trim_start().starts_with(name))
                .unwrap_or_else(|| panic!("{name} missing from grassblade.vert"));
            let got: f32 = line.split_whitespace().nth(2).unwrap().parse().unwrap();
            assert!((got - want).abs() < 1e-4, "{name} is {got} in GLSL, {want} in Rust");
        }
        let door = src
            .lines()
            .find(|l| l.trim_start().starts_with("#define T_DOOR"))
            .expect("T_DOOR missing");
        assert!(
            door.contains(&format!("{DOOR_X:.1}")) && door.contains(&format!("{DOOR_Z:.1}")),
            "T_DOOR is {door:?}, Rust says ({DOOR_X}, {DOOR_Z})"
        );
        // The harmonics themselves, so a reshaped hill field cannot drift either.
        for term in ["0.55 * sin(u) * cos(v)", "0.28 * sin(2.0 * u + 1.7)", "0.17 * sin(3.0 * u - 0.9)"] {
            assert!(src.contains(term), "grassblade.vert lost the harmonic {term:?}");
        }
    }

    /// The player walks on flat plates, not on the drawn surface. Where the two diverge by more
    /// than a stride the player either floats above the grass or drops through it -- which is
    /// exactly what happened beside the door when the plates were 8 units across.
    #[test]
    fn walk_shell_tracks_the_ground() {
        let mut worst: f32 = 0.0;
        let mut at = (0.0, 0.0);
        let n = 240;
        for i in 0..n {
            for j in 0..n {
                let x = -PERIOD * 0.5 + PERIOD * (i as f32 + 0.37) / n as f32;
                let z = -PERIOD * 0.5 + PERIOD * (j as f32 + 0.71) / n as f32;
                let e = (shell_height(x, z) - height(x, z)).abs();
                if e > worst {
                    worst = e;
                    at = (x, z);
                }
            }
        }
        assert!(worst < 0.45, "walk shell is {worst:.2} off the ground at {at:?}");
    }

    /// The tile box must contain the whole height field over the tile, or a hilltop could be
    /// culled while its slopes are on screen.
    #[test]
    fn tile_aabb_contains_the_height_field() {
        let (lo, hi) = tile_aabb(0, 0);
        let n = 128;
        for i in 0..=n {
            for j in 0..=n {
                let x = -PERIOD * 0.5 + PERIOD * i as f32 / n as f32;
                let z = -PERIOD * 0.5 + PERIOD * j as f32 / n as f32;
                let y = height(x, z);
                assert!(x >= lo.x && x <= hi.x && z >= lo.z && z <= hi.z);
                assert!(y > lo.y && y < hi.y, "height {y} at ({x},{z}) outside [{}, {}]", lo.y, hi.y);
            }
        }
        // Neighbouring tiles tile the plane: shared edges, no gap.
        let (lo1, _) = tile_aabb(1, 0);
        assert!((lo1.x - hi.x).abs() < 1e-4);
        let (_, hi_m) = tile_aabb(0, -1);
        assert!((hi_m.z - lo.z).abs() < 1e-4);
    }

    /// The shortest way round is the one that counts, in both directions and across the seam.
    #[test]
    fn wrap_delta_takes_the_short_way() {
        assert!((wrap_delta(1.0) - 1.0).abs() < 1e-4);
        assert!((wrap_delta(PERIOD - 1.0) + 1.0).abs() < 1e-4);
        assert!((wrap_delta(-PERIOD + 1.0) - 1.0).abs() < 1e-4);
        assert!(wrap_delta(PERIOD * 3.0 + 2.0).abs() - 2.0 < 1e-4);
        for d in [-500.0f32, -63.0, 0.0, 63.0, 500.0] {
            assert!(wrap_delta(d).abs() <= PERIOD * 0.5 + 1e-4);
        }
    }

    /// The generated tile must still match the function everything else uses. If someone edits
    /// `height` and forgets `--gen-terrain`, the blades and the collision shell would sit on a
    /// different surface than the one being drawn.
    #[test]
    fn tile_matches_height_field() {
        let path = format!("Meshes/{TILE_MESH}");
        let src = std::fs::read_to_string(&path).expect("run --gen-terrain");
        let step = PERIOD / VIS_N as f32;
        let mut checked = 0;
        for (n, line) in src.lines().filter(|l| l.starts_with("v ")).enumerate() {
            // Only the render grid; the collider verts that follow are a different lattice.
            if n >= (VIS_N + 1) * (VIS_N + 1) {
                break;
            }
            if n % 97 != 0 {
                continue;
            }
            let f: Vec<f32> =
                line[2..].split_whitespace().map(|t| t.parse().unwrap()).collect();
            let (i, j) = (n / (VIS_N + 1), n % (VIS_N + 1));
            let x = -PERIOD * 0.5 + j as f32 * step;
            let z = -PERIOD * 0.5 + i as f32 * step;
            assert!((f[0] - x).abs() < 1e-3 && (f[2] - z).abs() < 1e-3, "grid moved at {n}");
            assert!((f[1] - height(x, z)).abs() < 2e-3, "stale tile at {n}: {} vs {}", f[1], height(x, z));
            checked += 1;
        }
        assert!(checked > 20, "only checked {checked} vertices");
    }
}
