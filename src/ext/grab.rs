//! EXT: Superliminal-style forced-perspective grabbing. Not part of the C++ port.
//!
//! # Why this is cheap in this engine
//!
//! Every `Object` already carries `p_scale` -- a uniform physical scale that the portal code
//! multiplies when an object warps through a resizing portal (`Physical::try_portal`,
//! Physical.cpp:62). Crucially it is a *physical* scale, not a cosmetic one: it already feeds
//! the transform chain (Object.cpp:38), gravity (Physical.cpp:21), walk speed (Player.cpp:105)
//! and the collision epsilon (Physical.cpp:31).
//!
//! So "the object really is bigger now" is already solved. All this module has to do is choose
//! the right `p_scale` each frame.
//!
//! # The math
//!
//! An object subtends a constant angle on screen exactly when its size and its distance from the
//! eye stay in a fixed ratio. So on pickup we record
//!
//! ```text
//!     k = p_scale / distance
//! ```
//!
//! and hold `k` fixed for as long as the object is carried. Every frame we cast a ray down the
//! crosshair, and the object is placed just short of whatever the ray hits, so it rests against
//! the surface rather than punching through it. Its scaled bounding radius is `r * p_scale`, so
//! the placement distance `d` has to satisfy
//!
//! ```text
//!     d = hit_dist - r * p_scale
//!       = hit_dist - r * k * d
//! ```
//!
//! which solves in closed form, with no iteration:
//!
//! ```text
//!     d = hit_dist / (1 + r * k)
//!     p_scale = k * d
//! ```
//!
//! Aim at a near wall and the object stays small; aim down a long corridor and the very same
//! object becomes enormous -- while never changing size on screen until you let go.
//!
//! # Fitting: "you cannot drag objects larger than the room"
//!
//! The closed form above only consults the *centre* ray. A wide object placed that way has its
//! flanks punched through the side walls, and one scaled past the room's width ends up as a
//! hollow cut-out. Superliminal forbids this outright: the object shrinks until it fits.
//!
//! So after the unconstrained `(dist, p_scale)` is known we FIT it. Both the centre
//! (`origin + dir*d`) and the world radius (`r * k * d`) grow with `d`, so "the bounding sphere
//! penetrates some wall" is (close enough to) monotone in `d`, and a binary search between a
//! tiny `d_min` and the unconstrained `dist` finds the largest penetration-free placement in
//! ~14 steps. The penetration test is the engine's own `Collider::collide` (Collider.cpp:23-45),
//! fed exactly the composition the collision pass uses (Engine.cpp:170-171): a unit-sphere
//! frame built from the candidate sphere, times each object's `local_to_world`.
//!
//! The player is a second constraint. The object must not swallow the camera, so the gap between
//! the eye and the sphere's near side has to stay above [`PLAYER_CLEARANCE`]:
//!
//! ```text
//!     d - r*k*d >= c      =>      d >= c / (1 - r*k)
//! ```
//!
//! which, because the gap *grows* with `d`, is a lower bound -- it becomes the floor of the
//! search interval rather than a predicate inside it.
//!
//! # The ghost
//!
//! Superliminal does not silently snap a big object down; it shows a translucent copy at the
//! placement you asked for while the real object sits, shrunk, where it fits. So `update` records
//! the unconstrained placement on [`GrabState`] (`target_dist`, `target_scale`, `ghost_pos`) and
//! sets `fit_shrunk` when the fit took more than 15% off the scale; `draw_ghost` then renders the
//! mesh a second time through `Shaders/ghost.*` with blending on and depth writes off. The real
//! object's `p_scale` eases toward the fitted value ([`SCALE_EASE`] per frame) so the shrink reads
//! as a motion rather than a pop; its position snaps, which is what keeps it out of the wall.
//!
//! # Flat placement, and what the object is told
//!
//! An object that answers `ObjectT::place_flat` -- a window, a picture -- is not a ball, and
//! standing it off the wall by its bounding sphere would leave it floating. It is placed ON
//! the hit point instead, a hair along the normal ([`FLAT_OFFSET`]) so it does not fight the
//! wall for pixels, and turned by [`flat_euler`] to face out of the surface; the sphere fit
//! is skipped, since its sphere penetrates the wall by construction. Its `p_scale` still
//! follows the ray, so it is resized by perspective like anything else.
//!
//! Such an object is also PICKED by its face, not by its sphere: the ray is tested against
//! the rectangle on the object's own plane ([`face_rect`], the face of its bounding sphere
//! at the object's scale), and the pin `k` is taken at the plane's distance -- which is the
//! distance the first carry will place it at, so nothing changes size on pickup. A sphere
//! would not do: a door-sized window's sphere is its half-diagonal, reaching nearly two
//! metres into the hall off a frame thirty centimetres deep, and a ray from inside a sphere
//! has only its far root to offer (`ray_sphere`), behind the wall, so picking the window from
//! within its sphere pinned `k` at twice the real distance and halved it on pickup -- which
//! is exactly where a player stands to adjust it. The hover hand and the hint follow the
//! same test, so they no longer fire anywhere within the sphere.
//!
//! The held object hears about its life through the `ObjectT` hooks: `on_grab` at pickup,
//! `on_rescale` whenever the carry changes its `p_scale`, `on_release` at the drop with the
//! hand's velocity over the last rendered frame -- the carried position's displacement over
//! the frame's length -- so a thrown thing can keep going. And whatever grabbable sits under
//! the crosshair, other than the held one, may offer a line for the HUD (`pick_hint`).

use crate::collider::Collider;
use crate::ext::raycast::{ray_collider, ray_sphere, raycast, raycast_ignoring};
use crate::object::{Object, ObjectT, RenderCtx, UpdateCtx};
use crate::physical::Physical;
use crate::sphere::Sphere;
use crate::vector::{Matrix4, Vector3};
use std::cell::RefCell;
use std::rc::Rc;

/// How far the player can reach to pick something up.
///
/// Generous on purpose. Superliminal lets you take things across a room, and the anamorphic
/// scenes require it outright: the cube in scene `[` is only "there" from a station point several
/// metres away from it, so a short reach would make the illusion unusable.
pub const GRAB_REACH: f32 = 15.0;

/// EXT: how much closer than the object a solid surface has to be before it counts as being
/// in the way ([`pick`]).
///
/// The candidate's hit is on its bounding sphere or its face, and the surface it rests on can
/// pass within centimetres of that at a grazing angle -- a die on the carpet is hit by the
/// same ray a hand's breadth after the carpet is. Requiring the blocker to be clearly in
/// front keeps those grabs while still refusing anything with a wall across it. The cost is
/// that a partition thinner than this, pressed against the object, does not block.
pub const LOS_SLACK: f32 = 0.05;
/// How far a held object may be pushed before the ray stops mattering.
pub const MAX_PLACE_DIST: f32 = 60.0;
/// Clamps so an object can never become microscopic or swallow the level.
pub const MIN_P_SCALE: f32 = 0.05;
pub const MAX_P_SCALE: f32 = 25.0;
/// Safety margin on the fitted bounding sphere, so the object rests *near* a wall rather than
/// exactly tangent to it (a tangent sphere is one float rounding away from a collision push).
pub const FIT_CLEARANCE: f32 = 1.05;
/// Minimum gap between the eye and the near side of a held object, in world units.
pub const PLAYER_CLEARANCE: f32 = 0.3;
/// Floor of the fit search: never place the centre closer than this to the eye.
pub const FIT_D_MIN: f32 = 0.15;
/// Binary-search steps for the fit; 14 halvings of a 60-unit span resolve to ~4 mm.
pub const FIT_ITERS: u32 = 14;
/// Fraction of the remaining scale error closed each rendered frame (~0.12 s time constant at
/// 60 fps). The real object eases toward its fitted size; its position snaps.
pub const SCALE_EASE: f32 = 0.25;
/// Below this ratio of fitted/unconstrained scale the shrink is shown with a ghost.
pub const GHOST_THRESHOLD: f32 = 0.85;
/// How far out from the surface a flat-placed object sits, in world units: enough to win the
/// depth test against the wall at any distance the near plane allows, too little to see.
pub const FLAT_OFFSET: f32 = 0.01;
/// A surface whose normal is steeper than this (|n.y|) is a floor or a ceiling for flat
/// placement; anything shallower is a wall. Half way: a 30-degree ramp is walked on, a
/// 60-degree one is leant against.
pub const FLAT_FLOOR_NY: f32 = 0.5;

/// A prop that can be picked up and resized by perspective.
///
/// It is a `Physical` so the ported collision pass (Engine.cpp:155-192) picks it up for free:
/// the loop dispatches on `as_physical()`, so a released object falls and collides with level
/// geometry using exactly the same code path the player does.
pub struct Grabbable {
    pub base: Physical,
    /// Bounding radius with `p_scale == 1`.
    ///
    /// Mirrored into `base.hit_spheres[0].radius`, which is what the picking code actually
    /// reads (a `dyn ObjectT` cannot be downcast without `Any`, so grabbables are identified
    /// by having exactly one hit sphere). Kept here as the authoritative value for scene code.
    #[allow(dead_code)]
    pub radius: f32,
}

#[allow(dead_code)] // EXT: `obj`/`radius` are used by scene builders.
impl Grabbable {
    pub fn new(mesh_radius: f32) -> Grabbable {
        let mut base = Physical::new();
        // A little drag so a dropped object settles instead of skittering forever.
        base.drag = 0.002;
        base.friction = 0.08;
        // Exactly ONE hit sphere. Two things depend on this:
        //  - the ported collision pass needs a hit sphere or the object falls through the floor
        //    (Engine.cpp:167-190 iterates `physical->hitSpheres`);
        //  - `as_grabbable` uses "exactly one sphere" as the marker for grabbability, which is
        //    what keeps the player (two spheres, Player.cpp:9-10) from picking itself up.
        base.hit_spheres.push(crate::sphere::Sphere::new_at(Vector3::zero(), mesh_radius));
        Grabbable { base, radius: mesh_radius }
    }

    pub fn obj(&self) -> &Object {
        &self.base.base
    }
    pub fn obj_mut(&mut self) -> &mut Object {
        &mut self.base.base
    }
}

impl ObjectT for Grabbable {
    fn base(&self) -> &Object {
        &self.base.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base.base
    }
    fn update(&mut self, _ctx: &UpdateCtx) {
        self.base.update();
    }
    fn on_collide(&mut self, push: Vector3) {
        self.base.on_collide(push);
    }
    fn as_physical(&self) -> Option<&Physical> {
        Some(&self.base)
    }
    fn as_physical_mut(&mut self) -> Option<&mut Physical> {
        Some(&mut self.base)
    }
    fn draw(&self, ctx: &RenderCtx, cam: &crate::camera::Camera, _fbo: Option<glow::Framebuffer>) {
        self.base.base.draw_impl(ctx, cam);
    }
}

/// Which object (if any) the player is carrying, and the perspective ratio that pins its
/// apparent size.
pub struct GrabState {
    /// Index into the engine's object vector.
    pub held: Option<usize>,
    /// `k` from the module docs: `p_scale` per unit of distance from the eye.
    pub ratio: f32,
    /// Bounding radius of the held object at `p_scale == 1`.
    pub radius: f32,
    /// Set while an object is held, so the HUD/audio layer can react to a pickup or release.
    pub just_grabbed: bool,
    pub just_released: bool,
    /// True when something grabbable is under the crosshair right now. Drives the HUD tint, and
    /// is what turns "nothing happened" into "you are not pointing at it".
    pub hover: bool,
    /// The UNCONSTRAINED placement this frame -- what the centre ray alone asked for, before the
    /// fit. Distance from the eye and the `p_scale` that would go with it.
    pub target_dist: f32,
    pub target_scale: f32,
    /// True when the fit had to shrink the object below `GHOST_THRESHOLD` of `target_scale`;
    /// this is what makes `draw_ghost` show the translucent "what you asked for" copy.
    pub fit_shrunk: bool,
    /// World position of the ghost: `eye + dir * target_dist`, stored so the overlay pass needs
    /// no access to the camera ray.
    pub ghost_pos: Vector3,
    /// The `p_scale` actually applied last frame; the eased value that chases the fitted one.
    pub eased_scale: f32,
    /// Where the carry put the object last frame, and when (`view::time`), so the next frame
    /// can measure the hand's velocity; and that velocity, handed to `on_release`.
    pub hand_pos: Vector3,
    pub hand_time: f32,
    pub hand_vel: Vector3,
}

impl Default for GrabState {
    fn default() -> Self {
        GrabState {
            held: None,
            ratio: 0.0,
            radius: 0.5,
            just_grabbed: false,
            just_released: false,
            hover: false,
            target_dist: 0.0,
            target_scale: 1.0,
            fit_shrunk: false,
            ghost_pos: Vector3::zero(),
            eased_scale: 1.0,
            hand_pos: Vector3::zero(),
            hand_time: 0.0,
            hand_vel: Vector3::zero(),
        }
    }
}

impl GrabState {
    pub fn clear(&mut self) {
        self.held = None;
        self.just_grabbed = false;
        self.just_released = false;
        self.fit_shrunk = false;
    }

    /// Objects at these indices (ascending, as they were before the removal) have just been
    /// taken out of the object vector (`room::apply_removes`). The held index moves down by
    /// the number removed in front of it; a held object that went is simply no longer held --
    /// no release sound, no `on_release`, there is nothing left to let go of. `hover` needs no
    /// fixing: it is recomputed from scratch every frame.
    pub fn on_removed(&mut self, gone: &[usize]) {
        let Some(held) = self.held else { return };
        if gone.contains(&held) {
            self.clear();
        } else {
            self.held = Some(held - gone.iter().filter(|&&i| i < held).count());
        }
    }
}

/// Scale an object would have at centre distance `d`, with the engine's hard clamps applied.
fn scale_at(k: f32, d: f32) -> f32 {
    (k * d).clamp(MIN_P_SCALE, MAX_P_SCALE)
}

/// Lower bound on the centre distance that keeps the eye outside the object.
///
/// `d - r*k*d >= c` solves to `d >= c/(1 - r*k)`. When `r*k >= 1` the object is wider than it is
/// distant at *every* distance (it was grabbed from inside its own bounding sphere), so there is
/// no solution; fall back to `FIT_D_MIN` and let the clearance go.
pub fn player_floor(k: f32, radius: f32) -> f32 {
    let slope = 1.0 - radius * k;
    if slope > 1e-4 {
        (PLAYER_CLEARANCE / slope).max(FIT_D_MIN)
    } else {
        FIT_D_MIN
    }
}

/// Largest centre distance `d' <= dist` at which the held object's bounding sphere fits, as
/// `(d', p_scale)`.
///
/// `penetrates(centre, radius)` answers whether a world-space sphere overlaps level geometry;
/// it is injected so the search itself is pure and testable. The predicate is assumed monotone
/// (clear at small `d`, blocked at large `d`): the search keeps `lo` on the clear side and `hi`
/// on the blocked side and halves the gap `FIT_ITERS` times. When the unconstrained placement is
/// already clear it is returned unchanged, and when even the floor penetrates the floor is
/// returned -- the object is jammed against the player and cannot shrink further.
pub fn fit_distance(
    dist: f32,
    k: f32,
    radius: f32,
    mut penetrates: impl FnMut(Vector3, f32) -> bool,
    origin: Vector3,
    dir: Vector3,
) -> (f32, f32) {
    let world_r = |d: f32| radius * scale_at(k, d) * FIT_CLEARANCE;
    let d_min = player_floor(k, radius).min(dist);

    if !penetrates(origin + dir * dist, world_r(dist)) {
        return (dist, scale_at(k, dist));
    }
    if penetrates(origin + dir * d_min, world_r(d_min)) {
        return (d_min, scale_at(k, d_min));
    }

    let (mut lo, mut hi) = (d_min, dist);
    for _ in 0..FIT_ITERS {
        let mid = 0.5 * (lo + hi);
        if penetrates(origin + dir * mid, world_r(mid)) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    (lo, scale_at(k, lo))
}

/// Does a world-space sphere overlap any collider in the scene, ignoring object `skip`?
///
/// Mirrors the ported collision pass (Engine.cpp:165-178) with the sphere already in world
/// space, so `worldToUnit` is just the sphere's own `local_to_unit` and the physical's
/// `world_to_local` drops out of the product.
fn sphere_penetrates(
    objects: &[Rc<RefCell<dyn ObjectT>>],
    skip: usize,
    centre: Vector3,
    radius: f32,
) -> bool {
    if radius <= 0.0 {
        return false;
    }
    let world_to_unit = Sphere::new_at(centre, radius).local_to_unit();
    for (j, handle) in objects.iter().enumerate() {
        if j == skip {
            continue;
        }
        let Ok(obj) = handle.try_borrow() else { continue };

        // (a) Triangle-mesh scenery (ext/trimesh.rs), which is what every glTF level is made
        // of -- the Backrooms' walls, the pool's tiles, the overgrown room. It is tested
        // FIRST and before the bounding-sphere reject below, because such an object carries
        // its geometry in the trimesh and not in `base.mesh`: with only the mesh path here, a
        // carried object sank straight into every wall in the game, since there was nothing
        // for the fit to find. `push_sphere` is parry's, over the same BVH the collision pass
        // uses, so this costs microseconds however large the level is.
        if let Some(tm) = obj.trimesh() {
            if tm.push_sphere(centre, radius).is_some() {
                return true;
            }
        }

        let base = obj.base();
        let Some(mesh) = base.mesh.as_ref() else { continue };

        // Cheap reject: the object's own world bounding sphere vs the candidate sphere.
        let to_world = base.local_to_world();
        let obj_centre = to_world.translation();
        let obj_r = bound_radius(base) * base.p_scale;
        if (obj_centre - centre).mag() > obj_r + radius {
            continue;
        }

        // (b) Rectangle colliders, exactly as the ported collision pass tests them.
        if !mesh.colliders.is_empty() {
            let local_to_unit = world_to_unit * to_world;
            if mesh.colliders.iter().any(|c| c.collide(&local_to_unit).is_some()) {
                return true;
            }
        }

        // (c) The visible triangles. Colliders alone are not enough: bunny, teapot, suzanne
        // and every ceiling in the original rooms carry none, and a held object must not
        // pass through what the player can see. Triangles go to world space per test so
        // non-uniform object scale (room.obj is scaled per axis) stays exact, and the
        // per-object reject above keeps this from running on distant geometry.
        //
        // No size guard here any more: `Mesh` only builds this list for meshes at or under
        // FIT_TRI_CAP (mesh.rs), so anything past it -- the 660k-triangle Relativity model,
        // the two-million-triangle grass patch -- arrives with an empty list and falls back to
        // colliders on its own, exactly as the guard used to arrange.
        let r2 = radius * radius;
        for t in &mesh.tris {
            let a = to_world.mul_point(t[0]);
            let b = to_world.mul_point(t[1]);
            let c = to_world.mul_point(t[2]);
            if dist_sq_point_triangle(centre, a, b, c) < r2 {
                return true;
            }
        }
    }
    false
}

/// Meshes with more triangles than this are tested by colliders only -- and, because that makes
/// their triangle list unreadable, do not keep one. `Mesh::new` enforces it at load (mesh.rs).
pub const FIT_TRI_CAP: usize = 60_000;

/// Squared distance from `p` to triangle `abc` (Ericson, Real-Time Collision Detection 5.1.5).
pub fn dist_sq_point_triangle(p: Vector3, a: Vector3, b: Vector3, c: Vector3) -> f32 {
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return ap.mag_sq(); // vertex a
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return bp.mag_sq(); // vertex b
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return (p - (a + ab * v)).mag_sq(); // edge ab
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return cp.mag_sq(); // vertex c
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return (p - (a + ac * w)).mag_sq(); // edge ac
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return (p - (b + (c - b) * w)).mag_sq(); // edge bc
    }
    // inside the face
    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    (p - (a + ab * v + ac * w)).mag_sq()
}

/// Camera ray for this frame, derived from the player's `CamToWorld` (Player.cpp:133-135).
fn eye_ray(cam_to_world: &Matrix4) -> (Vector3, Vector3) {
    let origin = cam_to_world.translation();
    // The camera looks down its own -Z, same convention as `Object::Forward` (Object.cpp:33-35).
    let dir = cam_to_world.mul_direction(Vector3::new(0.0, 0.0, -1.0)).normalized_safe();
    (origin, dir)
}

/// Run one frame of grab logic.
///
/// Called from `Engine::update` after the ported physics and portal passes, so the held object's
/// position is authoritative and overrides whatever gravity did to it this step.
pub fn update(
    objects: &[Rc<RefCell<dyn ObjectT>>],
    cam_to_world: &Matrix4,
    grab_pressed: bool,
    state: &mut GrabState,
) {
    state.just_grabbed = false;
    state.just_released = false;

    let (origin, dir) = eye_ray(cam_to_world);

    // ── Toggle: press once to pick up, again to drop. ────────────────────────────────────────
    if grab_pressed {
        if state.held.is_some() {
            release(objects, state);
        } else {
            try_grab(objects, origin, dir, state);
        }
    }

    // Crosshair feedback: is something grabbable under the reticle right now? The held
    // object is skipped -- it sits on the ray by construction -- so the prompt that shows is
    // the one from whatever lies beyond it (a window's, with the key in hand).
    let picked = pick(objects, origin, dir, state.held);
    state.hover = state.held.is_some() || picked.is_some();
    // EXT: and say so on a standing channel, so a verb that claims E BEFORE the grab can
    // stand down when the grab has something to do with it (src/ext/disguise.rs). One frame
    // stale by construction -- this runs after the claim chain -- which is the right side to
    // err on: it costs a hide one press, never a pickup.
    set_busy(state.hover);
    if let Some((i, _, _)) = picked {
        if let Some(text) = objects[i].try_borrow().ok().and_then(|o| o.pick_hint()) {
            crate::ext::hint::set(text);
        }
    }

    // ── Carry: reposition and rescale whatever is held. ──────────────────────────────────────
    let Some(idx) = state.held else { return };
    let Some(handle) = objects.get(idx) else {
        state.clear();
        return;
    };
    let Ok(mut held) = handle.try_borrow_mut() else {
        return;
    };

    // ── The instrument's carry: held, not placed (`ObjectT::carry_fixed`). ───────────────────
    //
    // Taken before the perspective solve below, because none of it applies: there is no ray to
    // rest against, no distance to read a size from, and nothing for the room to shrink. The
    // pose comes straight off the camera, so it neither swells with the corridor behind it nor
    // tumbles as the player turns.
    if let Some(carry) = held.carry_fixed() {
        // The player's own scale, read off the camera: `Object::local_to_world` folds
        // `scale * p_scale`, so a unit direction through it comes back that long. Saves
        // threading a parameter through a ported signature for one caller.
        let p_scale = cam_to_world.mul_direction(Vector3::new(1.0, 0.0, 0.0)).mag().max(1e-4);
        let base = held.base_mut();
        base.pos = cam_to_world.mul_point(carry * p_scale);
        // Face the way the player faces. `euler.y` is the engine's yaw convention (the one
        // `Physical::try_portal` writes on a crossing) and pitch follows the look, so the torch
        // points where the beam goes.
        base.euler.y = (-dir.x).atan2(-dir.z);
        base.euler.x = dir.y.asin();
        base.euler.z = 0.0;
        base.rot = None;
        let fixed = CARRY_P_SCALE * p_scale;
        let rescaled = base.p_scale != fixed;
        base.p_scale = fixed;
        state.eased_scale = fixed;
        state.fit_shrunk = false;
        let pos = base.pos;
        if rescaled {
            held.on_rescale(fixed);
        }
        // The hand's velocity still matters: letting go of a tool mid-turn should throw it.
        let now = crate::ext::view::time();
        if state.just_grabbed {
            state.hand_vel = Vector3::zero();
        } else {
            let dt = now - state.hand_time;
            if dt > 0.0 {
                state.hand_vel = (pos - state.hand_pos) / dt;
            }
        }
        state.hand_pos = pos;
        state.hand_time = now;
        if let Some(phys) = held.as_physical_mut() {
            phys.gravity.set_zero();
            phys.velocity.set_zero();
            phys.prev_pos = phys.base.pos;
        }
        state.just_grabbed = false;
        return;
    }

    // Where would the object come to rest if pushed straight down the crosshair? With no hit
    // it hangs at the reach limit, facing the player.
    let hit = raycast(objects, origin, dir, MAX_PLACE_DIST, Some(idx));
    let hit_dist = hit.map_or(MAX_PLACE_DIST, |h| h.dist);
    let hit_normal = hit.map_or(-dir, |h| h.normal);

    let k = state.ratio;
    let flat = held.place_flat();
    let (fit_dist, fit_scale) = if flat {
        // Flat placement (module docs): on the surface, no sphere to stand off by, no fit.
        let dist = (hit_dist - FLAT_OFFSET).max(FIT_D_MIN);
        state.target_dist = dist;
        state.target_scale = scale_at(k, dist);
        state.ghost_pos = origin + dir * dist;
        state.fit_shrunk = false;
        (dist, state.target_scale)
    } else {
        // Closed-form solve from the module docs: d = hit_dist / (1 + r*k).
        let denom = 1.0 + state.radius * k;
        let mut dist = if denom > 1e-6 { hit_dist / denom } else { hit_dist };
        dist = dist.max(0.15);

        let mut p_scale = k * dist;
        // Clamp scale, then re-derive the distance so the object still rests on the surface
        // rather than floating or intersecting once the clamp bites.
        if p_scale < MIN_P_SCALE {
            p_scale = MIN_P_SCALE;
            dist = (hit_dist - state.radius * p_scale).max(0.15);
        } else if p_scale > MAX_P_SCALE {
            p_scale = MAX_P_SCALE;
            dist = (hit_dist - state.radius * p_scale).max(0.15);
        }

        // Record what the ray alone asked for; this is where the ghost goes.
        state.target_dist = dist;
        state.target_scale = p_scale;
        state.ghost_pos = origin + dir * dist;

        // Fit it against the room and the player (module docs, "Fitting").
        let (fit_dist, fit_scale) = fit_distance(
            dist,
            k,
            state.radius,
            |c, r| sphere_penetrates(objects, idx, c, r),
            origin,
            dir,
        );
        state.fit_shrunk = fit_scale < GHOST_THRESHOLD * p_scale;
        (fit_dist, fit_scale)
    };

    // Ease the scale toward the fitted value so a forced shrink reads as motion, not a pop.
    // The frame of pickup starts the ease from the object's current size.
    let base = held.base_mut();
    if state.just_grabbed {
        state.eased_scale = base.p_scale;
    }
    state.eased_scale += (fit_scale - state.eased_scale) * SCALE_EASE;
    if (state.eased_scale - fit_scale).abs() < 1e-4 {
        state.eased_scale = fit_scale;
    }
    let pos = origin + dir * fit_dist;
    base.pos = pos;
    if flat {
        base.euler = flat_euler(hit_normal, dir);
    }
    let rescaled = base.p_scale != state.eased_scale;
    base.p_scale = state.eased_scale;
    if rescaled {
        held.on_rescale(state.eased_scale);
    }

    // The hand's velocity over this frame, for `on_release`. The pickup frame has no previous
    // carried position and measures nothing.
    let now = crate::ext::view::time();
    if state.just_grabbed {
        state.hand_vel = Vector3::zero();
    } else {
        let dt = now - state.hand_time;
        if dt > 0.0 {
            state.hand_vel = (pos - state.hand_pos) / dt;
        }
    }
    state.hand_pos = pos;
    state.hand_time = now;

    // A carried object is not falling. Suspend gravity and zero the velocity, otherwise the
    // 500 Hz physics loop accumulates fall speed between frames and the object rockets away the
    // instant it is released. Gravity is restored in `release`.
    if let Some(phys) = held.as_physical_mut() {
        phys.gravity.set_zero();
        phys.velocity.set_zero();
        phys.prev_pos = phys.base.pos;
    }
}

/// Euler angles that lay an object flat on a surface of normal `n`, looked at along `view`.
///
/// The object's local -Z (its face, `Object::forward` for a yaw-only object) points out of
/// the surface along `n`. On a wall that is all there is to choose: the yaw that turns -Z onto
/// `n`, by `door::yaw_facing`'s convention. On a floor or a ceiling (|n.y| past
/// [`FLAT_FLOOR_NY`]) the face is turned up or down by a quarter turn about X, and the yaw is
/// spent on the object's top (local +Y) instead: it points away from the viewer along the
/// horizontal view direction, so a picture on the floor reads the right way up to whoever
/// put it there, and one on the ceiling too (looking up, "away" is the top of the view).
pub fn flat_euler(n: Vector3, view: Vector3) -> Vector3 {
    if n.y.abs() < FLAT_FLOOR_NY {
        return Vector3::new(0.0, crate::ext::door::yaw_facing(-n), 0.0);
    }
    let away = Vector3::new(view.x, 0.0, view.z);
    // Straight down or up there is no horizontal view direction; any yaw will do.
    let away = if away.mag_sq() > 1e-8 { away.normalized() } else { Vector3::new(0.0, 0.0, -1.0) };
    let pitch = std::f32::consts::FRAC_PI_2 * n.y.signum();
    // rot_y(yaw) * rot_x(pitch) carries local +Y to (0, 0, sin pitch) turned by yaw, so the
    // floor's top (+Z before the yaw) wants `yaw_facing(away)` and the ceiling's (-Z) the
    // opposite heading.
    let yaw = crate::ext::door::yaw_facing(if n.y > 0.0 { away } else { -away });
    Vector3::new(pitch, yaw, 0.0)
}

/// The face a flat-placed object is picked by (module docs): the square inscribed in the
/// great circle of its mesh's bounding sphere, on the object's own XY plane, in the mesh's
/// units -- the object's `local_to_world` scales it to the face's size. A quad mesh's sphere
/// is its half-diagonal, so the square is the quad itself; the window's collider-only mesh
/// carries the frame's half-diagonal for the same reason.
pub fn face_rect(base: &Object) -> Collider {
    let half = base.mesh.as_ref().map_or(0.5, |m| m.bound_radius) / std::f32::consts::SQRT_2;
    Collider::rect(Vector3::zero(), Vector3::unit_x() * half, Vector3::unit_y() * half)
}

/// Nearest grabbable under the crosshair, as (index, hit distance, bounding radius), whether
/// or not anything stands in the way. [`pick`] is this plus the line-of-sight gate; the two
/// are separate so a refused grab can say which of the two reasons it was.
fn aim(
    objects: &[Rc<RefCell<dyn ObjectT>>],
    origin: Vector3,
    dir: Vector3,
    skip: Option<usize>,
) -> Option<(usize, f32, f32)> {
    let mut best: Option<(usize, f32, f32)> = None;
    for (i, handle) in objects.iter().enumerate() {
        if Some(i) == skip {
            continue;
        }
        let Ok(obj) = handle.try_borrow() else { continue };
        let Some(g) = as_grabbable(&*obj) else { continue };
        let base = obj.base();
        let hit = if obj.place_flat() {
            ray_collider(origin, dir, &base.local_to_world(), &face_rect(base)).map(|(t, _)| t)
        } else {
            let world_radius = g * base.p_scale;
            ray_sphere(origin, dir, base.pos, world_radius.max(0.05))
        };
        let Some(t) = hit else { continue };
        if t > GRAB_REACH {
            continue;
        }
        if best.is_none_or(|(_, bt, _)| t < bt) {
            best = Some((i, t, g));
        }
    }

    best
}

/// Nearest grabbable under the crosshair that the player can actually see.
///
/// Shared by the hover test and the actual pick so the crosshair can never disagree with what
/// pressing E will do. [`aim`] asks each grabbable's own sphere or face whether the crosshair
/// crosses it, which a wall between the two does not change -- so on its own it reached
/// through walls, and the hover hint with it. Only the nearest grabbable needs the test:
/// anything solid in front of it is in front of the ones behind it too.
fn pick(
    objects: &[Rc<RefCell<dyn ObjectT>>],
    origin: Vector3,
    dir: Vector3,
    skip: Option<usize>,
) -> Option<(usize, f32, f32)> {
    let best = aim(objects, origin, dir, skip)?;
    if blocker(objects, origin, dir, best.0, best.1, skip).is_some() {
        return None;
    }
    Some(best)
}

/// What stands between the eye and the grabbable at `idx`, `dist` away, if anything.
///
/// Neither the candidate nor whatever is already in hand may block the view of it.
fn blocker(
    objects: &[Rc<RefCell<dyn ObjectT>>],
    origin: Vector3,
    dir: Vector3,
    idx: usize,
    dist: f32,
    skip: Option<usize>,
) -> Option<usize> {
    let mut ignore = [idx; 2];
    if let Some(held) = skip {
        ignore[1] = held;
    }
    raycast_ignoring(objects, origin, dir, dist - LOS_SLACK, &ignore).map(|h| h.object)
}

/// Pick the grabbable nearest along the crosshair, within `GRAB_REACH`.
fn try_grab(
    objects: &[Rc<RefCell<dyn ObjectT>>],
    origin: Vector3,
    dir: Vector3,
    state: &mut GrabState,
) {
    // Sphere-picked rather than collider-picked, because several display meshes carry no
    // colliders at all (bunny/teapot/suzanne all have zero).
    let Some((idx, dist, radius)) = pick(objects, origin, dir, None) else {
        // EXT: tell the player why nothing happened -- otherwise a missed grab is
        // indistinguishable from a broken key binding, and something in the way looks
        // identical to bad aim.
        match aim(objects, origin, dir, None) {
            Some((i, d, _)) => {
                let by = blocker(objects, origin, dir, i, d, None);
                log::debug!(
                    "[grab] object #{i} is {d:.2} units away but object #{} stands in the way",
                    by.map_or(usize::MAX, |b| b)
                );
            }
            None => {
                let n = objects
                    .iter()
                    .filter(|o| o.try_borrow().ok().is_some_and(|o| as_grabbable(&*o).is_some()))
                    .count();
                log::debug!("[grab] nothing in reach (crosshair missed; {n} grabbable object(s) in scene, reach {GRAB_REACH})");
            }
        }
        return;
    };
    let Ok(mut obj) = objects[idx].try_borrow_mut() else { return };
    let p_scale = obj.base().p_scale;
    obj.on_grab();
    drop(obj);

    // Pin the apparent size: k = p_scale / distance, held constant from here on.
    let dist = dist.max(0.2);
    state.held = Some(idx);
    state.ratio = p_scale / dist;
    state.radius = radius;
    state.just_grabbed = true;
    log::debug!(
        "[grab] picked up object #{idx} at {dist:.2} units, p_scale {p_scale:.2} (aim and press E again to place it)"
    );
}

/// EXT: the physical size a `carry_fixed` instrument is held at -- 1.0 is the size it was
/// authored, times whatever scale the player themself currently is.
const CARRY_P_SCALE: f32 = 1.0;

/// EXT-pivot: cap on the hand velocity a simple grabbable keeps at release, matching the
/// rigid props' MAX_THROW (ext/rigid.rs) so both families throw alike.
const SIMPLE_THROW_CAP: f32 = 12.0;

fn release(objects: &[Rc<RefCell<dyn ObjectT>>], state: &mut GrabState) {
    if let Some(idx) = state.held {
        if let Some(handle) = objects.get(idx) {
            if let Ok(mut obj) = handle.try_borrow_mut() {
                if let Some(phys) = obj.as_physical_mut() {
                    // Restore gravity (suspended while carried). EXT-pivot: the object
                    // inherits the hand's velocity, capped -- releasing while panning is a
                    // throw. The throw is a seeker's free testing tool in Hide 'N Dream
                    // (docs/hide-n-dream.md), so simple grabbables no longer drop dead the
                    // way the campaign's did; rigid props already threw via `on_release`.
                    phys.gravity = Vector3::new(0.0, crate::game_header::GH_GRAVITY, 0.0);
                    let mag = state.hand_vel.mag();
                    phys.velocity = if mag > SIMPLE_THROW_CAP {
                        state.hand_vel * (SIMPLE_THROW_CAP / mag)
                    } else {
                        state.hand_vel
                    };
                    phys.prev_pos = phys.base.pos;
                }
                // The object itself may want the throw (`ObjectT::on_release`).
                obj.on_release(state.hand_vel);
            }
        }
    }
    state.held = None;
    state.just_released = true;
    log::debug!("[grab] released");
}

// ── The inventory's seam (src/ext/inventory.rs) ──────────────────────────────────────────────
//
// Three small doors beside the pick/release pair above, so that stowing and retrieving need no
// copy of the carry's invariants. `GrabState::held` is an index into the object vector, which
// `on_removed` already fixes when objects leave the scene, so none of these may leave it
// pointing at something else.

/// The object in hand right now, as its index and a handle on it.
///
/// The handle so the caller can hold the object alive after it leaves the scene (a slot owns
/// its item), the index because that is what `GrabState` names it by.
pub fn held_object(
    objects: &[Rc<RefCell<dyn ObjectT>>],
    state: &GrabState,
) -> Option<(usize, Rc<RefCell<dyn ObjectT>>)> {
    let idx = state.held?;
    Some((idx, Rc::clone(objects.get(idx)?)))
}

/// Let go WITHOUT dropping the object into the world: no `on_release`, no gravity given back,
/// no release edge for the HUD or the sounds to see.
///
/// [`release`] is the drop; this is what a stow does, where the object is not being let go of
/// but taken away -- the inventory calls it in the same breath as `room::request_remove`, and
/// the object's own `ObjectT::on_stow` is what puts its affairs in order.
pub fn release_without_dropping(state: &mut GrabState) {
    state.held = None;
    state.fit_shrunk = false;
}

/// Take up an object the inventory has just put back into the scene, at the apparent size it
/// was stowed at.
///
/// The object is already placed and already told (`ObjectT::on_unstow`, then `on_grab`) by the
/// retrieve that asked for its spawn a frame ago, so this only pins the carry: `ratio` is the
/// `k` it was carried with, `radius` its bounding radius and `p_scale` the size to start the
/// ease from. [`try_grab`] cannot serve here -- it derives both numbers from where the
/// crosshair hit, which would resize the object on the way out of the pocket.
///
/// The hand's last position and time are seeded from where the object is, as the first frame of
/// a real carry leaves them: without them the frame's `on_release` velocity would be the
/// object's position divided by the seconds since launch.
pub fn hold_retrieved(
    objects: &[Rc<RefCell<dyn ObjectT>>],
    idx: usize,
    ratio: f32,
    radius: f32,
    p_scale: f32,
    state: &mut GrabState,
) {
    let Some(pos) = objects.get(idx).and_then(|o| o.try_borrow().ok()).map(|o| o.base().pos) else {
        return;
    };
    state.held = Some(idx);
    state.ratio = ratio;
    state.radius = radius;
    state.eased_scale = p_scale;
    state.fit_shrunk = false;
    state.hand_pos = pos;
    state.hand_time = crate::ext::view::time();
    state.hand_vel = Vector3::zero();
    log::debug!("[grab] object #{idx} out of the inventory at p_scale {p_scale:.2}");
}

/// Whether this scene holds anything at all that could be picked up.
///
/// Asked once per load, by `Engine::load_scene_from`: the HUD's inventory row is drawn only where
/// one of the two keys could ever do something, so the fifteen ported NonEuclidean scenes -- which
/// contain nothing grabbable -- do not gain permanently empty chrome for a mechanic they have not
/// got. A scene that spawned its first grabbable later would be missed; none does, and the row
/// comes back anyway as soon as anything is in a slot.
pub fn any_grabbable(objects: &[std::rc::Rc<std::cell::RefCell<dyn ObjectT>>]) -> bool {
    objects.iter().any(|o| o.try_borrow().ok().is_some_and(|o| as_grabbable(&*o).is_some()))
}

/// Recover the bounding radius of an object if it is grabbable.
///
/// `dyn ObjectT` gives no downcast without `Any`, so grabbables advertise themselves through
/// the physical hit-sphere the engine already understands: `Grabbable::new` seeds exactly one
/// hit sphere whose radius is the bounding radius. Anything else -- including the player, which
/// seeds two (Player.cpp:9-10) -- is not grabbable.
fn as_grabbable(obj: &dyn ObjectT) -> Option<f32> {
    let phys = obj.as_physical()?;
    if phys.hit_spheres.len() == 1 {
        Some(bound_radius(obj.base()))
    } else {
        None
    }
}

thread_local! {
    /// Whether the grab would act on the next press: something held, or something
    /// grabbable under the crosshair. Written once per rendered frame by [`update`].
    static BUSY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn set_busy(on: bool) {
    BUSY.with(|c| c.set(on));
}

/// Whether the grab would act on the next E press. A standing answer: read it before
/// claiming E for anything that is NOT a pickup, or picking a thing up becomes impossible
/// wherever the other verb is also offered.
pub fn busy() -> bool {
    BUSY.with(std::cell::Cell::get)
}

/// Scene-load hygiene: nothing is held or hovered in a scene that has not drawn yet.
pub fn reset_busy() {
    set_busy(false);
}

/// Conservative bounding radius of an object at `p_scale == 1`: the mesh's max vertex
/// distance times the largest scale component. Derived from geometry, never hand-typed --
/// the first version of this file trusted scene authors' radii and the teapot's real radius
/// turned out to be 2.2x the declared one, so the "fit" sphere was never conservative and
/// held objects clipped through walls by construction.
pub fn bound_radius(base: &Object) -> f32 {
    let mesh_r = base.mesh.as_ref().map_or(0.5, |m| m.bound_radius);
    let s = base.scale.x.max(base.scale.y).max(base.scale.z);
    (mesh_r * s).max(0.05)
}

/// Draw a translucent copy of the held object at the placement the ray alone would give, when
/// the fit logic shrank the real object away from it (see module docs, "The ghost").
///
/// Runs in the main pass only, after the scene, so it never lands in a portal framebuffer.
/// Blending is switched on and depth writes off for the single draw, then both are restored to
/// the state the ported renderer assumes (BLEND off, depth mask on).
pub fn draw_ghost(
    gl: &glow::Context,
    cam: &crate::camera::Camera,
    objects: &[Rc<RefCell<dyn ObjectT>>],
    state: &GrabState,
    shader: &crate::shader::Shader,
) {
    use glow::HasContext;

    let Some(idx) = state.held else { return };
    if !state.fit_shrunk {
        return;
    }
    let Some(handle) = objects.get(idx) else { return };
    let Ok(obj) = handle.try_borrow() else { return };
    let base = obj.base();
    let Some(mesh) = base.mesh.clone() else { return };

    // A throw-away Object with the ghost's transform, so the exact ported transform chain
    // (Object.cpp:38) is reused rather than re-derived. `shader` is left None: the ghost is
    // drawn by hand below with the ghost shader, not through `draw_impl`.
    let ghost = Object {
        pos: state.ghost_pos,
        euler: base.euler,
        scale: base.scale,
        p_scale: state.target_scale,
        mesh: None,
        texture: None,
        shader: None,
        rot: base.rot,
    };
    let mv = ghost.world_to_local().transposed();
    let mvp = cam.matrix() * ghost.local_to_world();

    shader.use_program();
    if let Some(tex) = &base.texture {
        tex.use_texture();
    }
    shader.set_mvp(Some(&mvp), Some(&mv));

    unsafe {
        gl.enable(glow::BLEND);
        gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
        gl.depth_mask(false);
    }
    mesh.draw();
    unsafe {
        gl.depth_mask(true);
        gl.disable(glow::BLEND);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wall between the eye and a grabbable puts it out of reach: the sphere test alone
    /// said yes through anything, which let the player pick props up through the hall's walls.
    #[test]
    fn a_wall_between_hides_a_grabbable() {
        use crate::ext::trimesh::TriMeshCollider;
        use crate::object::UpdateCtx;

        /// Scenery: one quad, as `ext/interior.rs` hands the engine its levels.
        struct Wall {
            base: Object,
            solid: Rc<TriMeshCollider>,
        }
        impl Wall {
            fn at(z: f32) -> Wall {
                let pos = [[-2.0, -2.0, z], [2.0, -2.0, z], [2.0, 2.0, z], [-2.0, 2.0, z]];
                let idx = [0, 1, 2, 0, 2, 3];
                let solid = TriMeshCollider::new(&pos, &idx, &Matrix4::identity());
                Wall { base: Object::new(), solid: Rc::new(solid) }
            }
        }
        impl ObjectT for Wall {
            fn base(&self) -> &Object {
                &self.base
            }
            fn base_mut(&mut self) -> &mut Object {
                &mut self.base
            }
            fn update(&mut self, _ctx: &UpdateCtx) {}
            fn trimesh(&self) -> Option<Rc<TriMeshCollider>> {
                Some(self.solid.clone())
            }
        }

        // Eye at the origin looking down -Z, a grabbable four metres along it.
        let eye = Vector3::zero();
        let dir = Vector3::new(0.0, 0.0, -1.0);
        let mut prop = Grabbable::new(0.2);
        prop.obj_mut().pos = Vector3::new(0.0, 0.0, -4.0);
        let prop: Rc<RefCell<dyn ObjectT>> = Rc::new(RefCell::new(prop));

        let clear: Vec<Rc<RefCell<dyn ObjectT>>> = vec![prop.clone()];
        assert!(pick(&clear, eye, dir, None).is_some(), "nothing in the way, so it is pickable");

        let walled: Vec<Rc<RefCell<dyn ObjectT>>> =
            vec![prop.clone(), Rc::new(RefCell::new(Wall::at(-2.0)))];
        assert!(pick(&walled, eye, dir, None).is_none(), "the wall stands between them");

        // Behind the player is not in the way.
        let behind: Vec<Rc<RefCell<dyn ObjectT>>> =
            vec![prop, Rc::new(RefCell::new(Wall::at(2.0)))];
        assert!(pick(&behind, eye, dir, None).is_some(), "a wall behind the eye blocks nothing");

        // And the ray the carry places against still finds that wall, with and without a
        // skipped object: the placement distance is what sizes a carried object, so a raycast
        // that quietly stopped hitting would leave it hanging at arm's length through the wall.
        // ... and the fit sees it too. A level's walls are triangle meshes, not colliders and
        // not a drawn `Mesh`, so a fit that only knew those two let a carried object sink
        // into every wall in the game.
        assert!(
            sphere_penetrates(&walled, 0, Vector3::new(0.0, 0.0, -2.0), 0.3),
            "a sphere on the wall penetrates it"
        );
        assert!(
            !sphere_penetrates(&walled, 0, Vector3::new(0.0, 0.0, -1.0), 0.3),
            "a sphere a metre in front of it does not"
        );

        let hit = raycast(&walled, eye, dir, MAX_PLACE_DIST, None).expect("the wall is hit");
        assert!((hit.dist - 2.0).abs() < 1e-3, "hit at {}", hit.dist);
        let skipped = raycast(&walled, eye, dir, MAX_PLACE_DIST, Some(0)).expect("still hit");
        assert!((skipped.dist - 2.0).abs() < 1e-3, "hit at {}", skipped.dist);
    }

    #[test]
    fn point_triangle_distance_regions() {
        let a = Vector3::new(0.0, 0.0, 0.0);
        let b = Vector3::new(2.0, 0.0, 0.0);
        let c = Vector3::new(0.0, 2.0, 0.0);
        // Above the face: pure perpendicular distance.
        assert!((dist_sq_point_triangle(Vector3::new(0.5, 0.5, 3.0), a, b, c) - 9.0).abs() < 1e-5);
        // Beyond vertex b.
        assert!((dist_sq_point_triangle(Vector3::new(4.0, 0.0, 0.0), a, b, c) - 4.0).abs() < 1e-5);
        // Off edge ab.
        assert!((dist_sq_point_triangle(Vector3::new(1.0, -1.0, 0.0), a, b, c) - 1.0).abs() < 1e-5);
        // On the face: zero.
        assert!(dist_sq_point_triangle(Vector3::new(0.5, 0.5, 0.0), a, b, c) < 1e-9);
    }

    /// The core promise: apparent size is constant, so p_scale must track distance linearly.
    #[test]
    fn apparent_size_is_preserved() {
        // Grab at 2 units with p_scale 1 => k = 0.5.
        let k = 1.0f32 / 2.0;
        let radius = 0.0; // point-sized, so the surface offset drops out
        for hit_dist in [1.0f32, 4.0, 10.0, 40.0] {
            let d = hit_dist / (1.0 + radius * k);
            let p_scale = k * d;
            // p_scale / distance is the angular size; it must not drift.
            assert!((p_scale / d - k).abs() < 1e-6, "apparent size drifted at hit_dist={hit_dist}");
        }
    }

    /// Placing further away must make the object physically larger.
    #[test]
    fn further_is_bigger() {
        let k = 0.5f32;
        let radius = 0.5f32;
        let near = 2.0f32 / (1.0 + radius * k);
        let far = 20.0f32 / (1.0 + radius * k);
        assert!(k * far > k * near * 5.0, "distant placement should scale up");
    }

    /// The surface offset must keep the object clear of what the ray hit.
    #[test]
    fn object_rests_clear_of_surface() {
        let k = 0.5f32;
        let radius = 0.5f32;
        let hit_dist = 10.0f32;
        let d = hit_dist / (1.0 + radius * k);
        let p_scale = k * d;
        // centre distance + scaled radius should land exactly on the surface
        assert!(
            (d + radius * p_scale - hit_dist).abs() < 1e-4,
            "object should touch the surface, not overlap it"
        );
    }

    // ── Picking ──────────────────────────────────────────────────────────────────────────────

    /// A flat-placed grabbable with no mesh: its bounding radius is `bound_radius`'s default
    /// half unit, so its face is a 0.707-unit square at unit scale.
    struct Frame {
        base: Physical,
    }
    impl ObjectT for Frame {
        fn base(&self) -> &Object {
            &self.base.base
        }
        fn base_mut(&mut self) -> &mut Object {
            &mut self.base.base
        }
        fn as_physical(&self) -> Option<&Physical> {
            Some(&self.base)
        }
        fn as_physical_mut(&mut self) -> Option<&mut Physical> {
            Some(&mut self.base)
        }
        fn place_flat(&self) -> bool {
            true
        }
    }

    /// A door-sized frame on a wall, picked from inside its sphere: the pick is the plane's
    /// distance, not the sphere's far root, and the pin leaves its scale alone; off the
    /// face the pick misses even though the sphere would not.
    #[test]
    fn a_flat_object_is_picked_on_its_face_at_the_planes_distance() {
        let mut base = Physical::new();
        base.hit_spheres.push(Sphere::new_at(Vector3::zero(), 0.5));
        // On a wall at z = 2.06 facing -z (the window's pose), seven times its size.
        base.base.pos = Vector3::new(0.0, 1.35, 2.06);
        base.base.euler.y = crate::ext::door::yaw_facing(Vector3::new(0.0, 0.0, 1.0));
        base.base.p_scale = 7.0;
        let frame: Rc<RefCell<dyn ObjectT>> = Rc::new(RefCell::new(Frame { base }));
        let objects = vec![frame];
        // The eye 1.76 m off the wall, well inside the 3.5 m sphere, looking at the frame.
        let origin = Vector3::new(0.0, 1.5, 0.3);
        let dir = Vector3::new(0.0, 0.0, 1.0);
        let (idx, t, r) = pick(&objects, origin, dir, None).expect("on the face");
        assert_eq!(idx, 0);
        assert!((t - 1.76).abs() < 1e-4, "the plane's distance, not the sphere's: {t}");
        assert_eq!(r, 0.5);
        let sphere_t = ray_sphere(origin, dir, Vector3::new(0.0, 1.35, 2.06), 3.5).unwrap();
        assert!(sphere_t > 5.0, "the sphere test would have given the far root: {sphere_t}");
        // The pin: k = p_scale / distance, so the first carry at that distance keeps 7.
        let mut state = GrabState::default();
        try_grab(&objects, origin, dir, &mut state);
        assert_eq!(state.held, Some(0));
        assert!((state.ratio * 1.76 - 7.0).abs() < 1e-3, "k {}", state.ratio);
        assert!((scale_at(state.ratio, 1.76) - 7.0).abs() < 1e-3);
        // Past the face's edge (0.707 * 7 / 2 = 2.47 m half-side) the pick misses, although the
        // point is still inside the sphere.
        let aside = Vector3::new(2.6, 1.5, 0.3);
        assert!(pick(&objects, aside, dir, None).is_none());
        assert!(ray_sphere(aside, dir, Vector3::new(0.0, 1.35, 2.06), 3.5).is_some());
    }

    // ── Fit maths ────────────────────────────────────────────────────────────────────────────

    /// A synthetic room: the half-space `y >= -1` is free; anything dipping below the floor
    /// at `y = -1` or past the side walls at `|x| = 2` penetrates.
    fn room(c: Vector3, r: f32) -> bool {
        c.y - r < -1.0 || c.x.abs() + r > 2.0
    }

    fn eye() -> Vector3 {
        Vector3::new(0.0, 0.0, 0.0)
    }

    #[test]
    fn fit_never_exceeds_unconstrained() {
        let dir = Vector3::new(0.0, 0.0, -1.0);
        for k in [0.05f32, 0.2, 0.5, 1.0] {
            for dist in [0.5f32, 2.0, 10.0, 40.0] {
                let (d, s) = fit_distance(dist, k, 0.5, room, eye(), dir);
                assert!(d <= dist + 1e-6, "k={k} dist={dist}: fitted {d} > {dist}");
                assert!(s <= scale_at(k, dist) + 1e-6);
            }
        }
    }

    #[test]
    fn unobstructed_placement_is_returned_unchanged() {
        // Straight down a corridor wide enough for the object: k small, so the sphere never
        // reaches the walls before MAX_PLACE_DIST.
        let dir = Vector3::new(0.0, 0.0, -1.0);
        let (d, s) = fit_distance(10.0, 0.05, 0.5, room, eye(), dir);
        assert!((d - 10.0).abs() < 1e-6);
        assert!((s - 0.5).abs() < 1e-6);
    }

    #[test]
    fn fit_stops_short_of_the_wall() {
        // Sideways at the wall x=2: the sphere must shrink so centre+radius stays inside.
        let dir = Vector3::new(1.0, 0.0, 0.0);
        let k = 0.5;
        let r = 0.5;
        let (d, s) = fit_distance(10.0, k, r, room, eye(), dir);
        assert!(d < 10.0);
        assert!(!room(eye() + dir * d, r * s * FIT_CLEARANCE), "fitted placement penetrates");
        // and it is the *largest* such distance, to within the search resolution
        let step = 10.0 / (1u32 << FIT_ITERS) as f32;
        let d2 = d + 4.0 * step;
        assert!(room(eye() + dir * d2, r * scale_at(k, d2) * FIT_CLEARANCE), "fit left slack");
    }

    #[test]
    fn binary_search_converges_to_analytic_answer() {
        // Wall at x=2, ray along +x, radius r, clearance f: the exact bound is
        //   d + r*k*d*f = 2   =>   d = 2 / (1 + r*k*f)
        let dir = Vector3::new(1.0, 0.0, 0.0);
        let (k, r) = (0.4f32, 0.5f32);
        let exact = 2.0 / (1.0 + r * k * FIT_CLEARANCE);
        let (d, _) = fit_distance(30.0, k, r, |c, rad| c.x + rad > 2.0, eye(), dir);
        let tol = 30.0 / (1u32 << FIT_ITERS) as f32 * 2.0;
        assert!((d - exact).abs() < tol, "got {d}, expected {exact} +- {tol}");
    }

    #[test]
    fn player_clearance_holds() {
        // Aim straight down at the floor one unit below: the fit must shrink hard, but never
        // so close that the eye ends up inside the object.
        let dir = Vector3::new(0.0, -1.0, 0.0);
        for k in [0.3f32, 0.6, 0.9] {
            let r = 1.0;
            let (d, s) = fit_distance(5.0, k, r, room, eye(), dir);
            let gap = d - r * s;
            assert!(
                gap >= PLAYER_CLEARANCE - 1e-3,
                "k={k}: gap {gap} below clearance (d={d}, s={s})"
            );
        }
    }

    #[test]
    fn player_floor_is_consistent_with_its_definition() {
        let (k, r) = (0.5f32, 1.0f32);
        let d = player_floor(k, r);
        assert!((d - r * k * d - PLAYER_CLEARANCE).abs() < 1e-5);
        // Degenerate: grabbed from inside -- no solution, falls back to the hard floor.
        assert_eq!(player_floor(2.0, 1.0), FIT_D_MIN);
    }

    #[test]
    fn jammed_against_player_returns_floor() {
        // Everything penetrates: the search cannot help and must hand back its floor, not
        // something smaller or NaN.
        let dir = Vector3::new(0.0, 0.0, -1.0);
        let (k, r) = (0.5f32, 0.5f32);
        let (d, _) = fit_distance(10.0, k, r, |_, _| true, eye(), dir);
        assert!((d - player_floor(k, r)).abs() < 1e-6);
    }

    /// The object's face (local -Z) in world, through the real transform chain.
    fn face(euler: Vector3) -> Vector3 {
        let o = Object { euler, ..Object::new() };
        o.local_to_world().mul_direction(Vector3::new(0.0, 0.0, -1.0))
    }

    /// And its top (local +Y).
    fn top(euler: Vector3) -> Vector3 {
        let o = Object { euler, ..Object::new() };
        o.local_to_world().mul_direction(Vector3::new(0.0, 1.0, 0.0))
    }

    fn near(a: Vector3, b: Vector3) -> bool {
        (a - b).mag() < 1e-5
    }

    #[test]
    fn flat_on_a_wall_faces_out_of_it_and_stays_upright() {
        let view = Vector3::new(0.3, -0.1, -0.9).normalized();
        for n in [
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(-1.0, 0.0, 0.0),
            Vector3::new(0.6, 0.0, 0.8),
            // A lean short of FLAT_FLOOR_NY is still a wall: its horizontal heading is used.
            Vector3::new(0.0, 0.4, 1.0).normalized(),
        ] {
            let e = flat_euler(n, view);
            assert_eq!(e.x, 0.0);
            assert_eq!(e.z, 0.0);
            let horizontal = Vector3::new(n.x, 0.0, n.z).normalized();
            assert!(near(face(e), horizontal), "n={n:?}: face {:?}", face(e));
            assert!(near(top(e), Vector3::new(0.0, 1.0, 0.0)));
            // What `Object::forward` reports, for a yaw-only object, is the same face.
            let o = Object { euler: e, ..Object::new() };
            assert!(near(o.forward(), horizontal));
        }
    }

    #[test]
    fn flat_on_a_floor_or_ceiling_faces_it_with_the_top_away_from_the_viewer() {
        let view = Vector3::new(0.6, -0.7, 0.4).normalized();
        let away = Vector3::new(view.x, 0.0, view.z).normalized();
        let floor = flat_euler(Vector3::new(0.0, 1.0, 0.0), view);
        assert!(near(face(floor), Vector3::new(0.0, 1.0, 0.0)), "{:?}", face(floor));
        assert!(near(top(floor), away), "{:?} vs {away:?}", top(floor));
        let ceiling = flat_euler(Vector3::new(0.0, -1.0, 0.0), view);
        assert!(near(face(ceiling), Vector3::new(0.0, -1.0, 0.0)), "{:?}", face(ceiling));
        assert!(near(top(ceiling), away), "{:?} vs {away:?}", top(ceiling));
        // Looking straight down there is no "away"; the result is still a flat, finite pose.
        let e = flat_euler(Vector3::new(0.0, 1.0, 0.0), Vector3::new(0.0, -1.0, 0.0));
        assert!(near(face(e), Vector3::new(0.0, 1.0, 0.0)));
        assert!(e.x.is_finite() && e.y.is_finite());
    }

    #[test]
    fn a_removal_shifts_or_drops_the_held_index() {
        let mut s = GrabState { held: Some(5), ..GrabState::default() };
        s.on_removed(&[1, 3]);
        assert_eq!(s.held, Some(3), "two removed in front");
        s.on_removed(&[7, 9]);
        assert_eq!(s.held, Some(3), "none in front");
        s.on_removed(&[0, 3]);
        assert_eq!(s.held, None, "the held object itself went");
        assert!(!s.just_released, "nothing to let go of");
        s.on_removed(&[0]);
        assert_eq!(s.held, None, "holding nothing stays nothing");
    }

    #[test]
    fn ease_converges() {
        let mut x = 10.0f32;
        for _ in 0..40 {
            x += (1.0 - x) * SCALE_EASE;
        }
        assert!((x - 1.0).abs() < 1e-3);
    }
}
