//! EXT: real rigid-body physics for props, on rapier3d. Not part of the C++ port.
//!
//! The ported engine has one kind of motion: a `Physical` falls under its own gravity and is
//! pushed out of rectangles and triangle meshes sphere by sphere (Engine.cpp:155-192). That
//! is all a walking player needs and nothing a thrown die needs -- no rotation, no friction
//! worth the name, no body ever at rest on another. So props that are meant to tumble, roll
//! and knock into each other are owned by a rapier world instead (`ext/rigid.rs`), and the
//! engine's own passes leave them alone (`ObjectT::engine_collision`).
//!
//! # What rapier owns and what the port keeps
//!
//! Rapier owns the props' bodies and nothing else. The player stays on the ported physics --
//! the feel of the walk, the head bob, the portal warp and the collision epsilon all hang off
//! it -- and is mirrored into this world as a **kinematic cylinder** the height of the
//! player's two hit spheres ([`PLAYER_RADIUS`], [`FOOT_CLEARANCE`]), moved to the player's
//! eye every step, so a prop on the carpet is shoved aside by someone walking into it while
//! the player never feels the prop. The static world is
//! rebuilt on every scene load from what the scene's objects already declare for the ported
//! collision pass: every `ObjectT::trimesh()` as a fixed triangle mesh, and every rectangle
//! collider as a thin fixed box -- except on meshes carrying more than [`RECT_CAP`] of them,
//! which is a generated terrain shell (the meadow tiles carry 4,356 each), nine of which a
//! load would turn into forty thousand boxes for ground no prop ever reaches. The scenes that
//! have props are scanned interiors, and those are triangle meshes. It is a snapshot of what
//! the objects answer at load: the elevator's shut leaves, which `ElevatorDoors::trimesh`
//! offers only while the doors are closing, are not in it.
//!
//! # Step rate
//!
//! The world steps once per engine fixed step, 500 Hz, with `dt = GH_DT`, from
//! `Engine::update` after the ported collision pass and before the portal pass. Measured with
//! the three Backrooms props and the 70k-triangle scan as the static world, a step costs a
//! few microseconds once the props sleep and some tens while they move -- well under the
//! budget a substep scheme would be worth its complexity for; `step_cost` reports it on the
//! `[phys]` line at shot time so the decision can be re-made on numbers.
//!
//! # One world, reachable from anywhere
//!
//! The world is a thread-local, reached through [`with`], for the same reason the elevator's
//! ride and the HUD hint are: a prop is built inside `Scene::load`, which cannot see the
//! engine, registers its body there, and unregisters it in `Drop` wherever the object vector
//! lets go of it -- a scene load, a `room::request_remove`. An engine-owned world would need
//! every one of those to be threaded a handle, or a lazy registration that leaves ghost
//! colliders behind removed props. The engine itself only calls [`rebuild_static`] and
//! [`step`].

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use rapier3d::dynamics::{RigidBodyBuilder, RigidBodyHandle, RigidBodyType};
use rapier3d::geometry::{ColliderBuilder, ColliderHandle, SharedShape};
use rapier3d::math::{Mat3, Pose, Rot3, Vec3 as PVec};
use rapier3d::pipeline::PhysicsWorld as RapierWorld;

use crate::ext::trimesh::TriMeshCollider;
use crate::game_header::{GH_DT, GH_GRAVITY, GH_PLAYER_HEIGHT, GH_PLAYER_RADIUS};
use crate::object::ObjectT;
use crate::vector::{Matrix4, Vector3};

/// A mesh with more rectangle colliders than this is a terrain shell, not a room, and is left
/// out of the static world (module docs). The hand-placed rooms top out at 85 (`floorplan`),
/// the Relativity walk shell at 532; the meadow tiles start at 1,936.
pub const RECT_CAP: usize = 1024;
/// Half the thickness of the box a rectangle collider becomes, centred on the rectangle's
/// plane: a prop resting on one sits this far above the drawn surface, below what the eye
/// can tell at any distance the near plane allows. The ported collider has no thickness at
/// all and pushes to whichever side is nearer; a box keeps that for anything outside it.
pub const RECT_HALF_THICKNESS: f32 = 0.01;
/// A player displacement longer than this in one step is a teleport -- a respawn, a portal
/// warp, a `--pos` -- and the capsule is moved there outright rather than swept, which would
/// fling every prop on the line. A sprinting step covers 7 mm.
pub const TELEPORT_DIST: f32 = 0.5;
/// The player's mirror is a cylinder standing this far above the feet, not the capsule over
/// the two hit spheres the brief asked for: a capsule's round foot, bottom at floor level,
/// met a ball on the carpet with a contact normal pointing down into the floor, trod it 3 cm
/// into the carpet and spat it out BEHIND the player (the review's push probe). A flat-sided
/// body that starts above a floor-level prop's centre meets it sideways, and the contact
/// pushes it ahead. Three centimetres: under the apple's centre (r 0.045) and the die's
/// (half 0.03), and the scenes with props are flat scanned floors -- the static world has
/// no say in where a kinematic body goes, so nothing the player walks over matters here.
pub const FOOT_CLEARANCE: f32 = 0.03;
/// The cylinder's radius: wider than the hit spheres' `GH_PLAYER_RADIUS`, so a prop is
/// pushed clear of the feet rather than rolled under them.
pub const PLAYER_RADIUS: f32 = 0.28;
/// How far `lift_all` tips a prop, in radians, about the floor diagonal: past the 54.7
/// degrees at which a cube balances on a corner, so a die lands on one and has to roll
/// over onto another face rather than rock back onto the one it left with.
pub const DROP_TILT: f32 = 1.0;

/// A prop's collision shape, in metres at `p_scale == 1`, about the prop's own origin.
///
/// `Ball`, `Cuboid` and `RoundCuboid` are centred on the origin. `Cylinder` and `Capsule`
/// STAND on it: the origin is the centre of the base, as a lathe is built, so a scene places
/// a chess piece by the point it rests on and the collider is raised by half its height.
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // EXT: the full set a prop can take; the three shipped props use three.
pub enum Shape {
    Ball { radius: f32 },
    Cuboid { half: Vector3 },
    RoundCuboid { half: Vector3, radius: f32 },
    Cylinder { radius: f32, height: f32 },
    Capsule { radius: f32, height: f32 },
}

impl Shape {
    /// The same shape at `s` times the size.
    pub fn scaled(&self, s: f32) -> Shape {
        match *self {
            Shape::Ball { radius } => Shape::Ball { radius: radius * s },
            Shape::Cuboid { half } => Shape::Cuboid { half: half * s },
            Shape::RoundCuboid { half, radius } => {
                Shape::RoundCuboid { half: half * s, radius: radius * s }
            }
            Shape::Cylinder { radius, height } => {
                Shape::Cylinder { radius: radius * s, height: height * s }
            }
            Shape::Capsule { radius, height } => {
                Shape::Capsule { radius: radius * s, height: height * s }
            }
        }
    }

    /// The rapier shape and where its centre sits relative to the prop's origin.
    fn build(&self) -> (SharedShape, PVec) {
        match *self {
            Shape::Ball { radius } => (SharedShape::ball(radius), PVec::ZERO),
            Shape::Cuboid { half } => (SharedShape::cuboid(half.x, half.y, half.z), PVec::ZERO),
            Shape::RoundCuboid { half, radius } => {
                (SharedShape::round_cuboid(half.x, half.y, half.z, radius), PVec::ZERO)
            }
            Shape::Cylinder { radius, height } => {
                (SharedShape::cylinder(0.5 * height, radius), PVec::new(0.0, 0.5 * height, 0.0))
            }
            Shape::Capsule { radius, height } => {
                // rapier's half-height is the straight segment's, not the whole capsule's.
                let seg = (0.5 * height - radius).max(0.0);
                (SharedShape::capsule_y(seg, radius), PVec::new(0.0, 0.5 * height, 0.0))
            }
        }
    }
}

/// Surface and mass properties of a prop. `density` is kg per cubic metre, so the mass
/// follows the volume -- and a prop resized by the grab weighs what its new size should.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    pub friction: f32,
    pub restitution: f32,
    pub density: f32,
}

/// A prop's body and its one collider, as the world knows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BodyId {
    body: RigidBodyHandle,
    collider: ColliderHandle,
}

/// What a prop's body looks like from outside: where it is and whether it has come to rest.
#[derive(Clone, Copy, Debug)]
pub struct BodyReport {
    pub name: &'static str,
    pub pos: Vector3,
    /// How far the body has turned from the orientation it was added with, in degrees:
    /// the angle of its rotation, `acos((trace - 1) / 2)`.
    pub turned: f32,
    /// Kinematic: in someone's hand.
    pub held: bool,
    pub asleep: bool,
}

struct Prop {
    name: &'static str,
    id: BodyId,
}

/// The rapier world, the player's cylinder in it, the static colliders of the current scene
/// and the props' bodies. See the module docs.
pub struct PhysicsWorld {
    world: RapierWorld,
    player: RigidBodyHandle,
    /// Where the cylinder was last put, to tell a walk from a teleport (`TELEPORT_DIST`).
    player_at: Option<Vector3>,
    statics: Vec<ColliderHandle>,
    props: Vec<Prop>,
    /// Wall time spent in `step` since the last `step_cost` read, and how many steps.
    cost: Duration,
    cost_max: Duration,
    steps: u32,
}

fn to_p(v: Vector3) -> PVec {
    PVec::new(v.x, v.y, v.z)
}

fn from_p(v: PVec) -> Vector3 {
    Vector3::new(v.x, v.y, v.z)
}

/// The rotation part of an engine matrix (row-major, axes in the columns) as a quaternion.
/// The axes must be orthonormal: `Object::rot` is, and so is every Euler product.
fn to_quat(m: &Matrix4) -> Rot3 {
    Rot3::from_mat3(&Mat3::from_cols(to_p(m.x_axis()), to_p(m.y_axis()), to_p(m.z_axis())))
}

/// A pure rotation matrix in the engine's layout, from a quaternion.
fn from_quat(q: Rot3) -> Matrix4 {
    let m = Mat3::from_quat(q);
    let mut out = Matrix4::identity();
    out.set_x_axis(from_p(m.x_axis));
    out.set_y_axis(from_p(m.y_axis));
    out.set_z_axis(from_p(m.z_axis));
    out
}

fn pose(pos: Vector3, rot: &Matrix4) -> Pose {
    Pose::from_parts(to_p(pos), to_quat(rot))
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        PhysicsWorld::new()
    }
}

impl PhysicsWorld {
    /// An empty world at the engine's step and gravity, holding only the player's cylinder.
    pub fn new() -> PhysicsWorld {
        let mut world = RapierWorld::new();
        world.gravity = PVec::new(0.0, GH_GRAVITY, 0.0);
        world.integration_parameters.dt = GH_DT;
        // The cylinder covers the player's two hit spheres (Player.cpp:9-10): from the top of
        // the head sphere at the eye down to `FOOT_CLEARANCE` above the feet, which are a
        // player's height below the eye. Built at `p_scale` 1: the scale tunnels shrink the
        // player, but no prop lives in a scaled scene, so the mirror is not rescaled.
        let (top, bottom) = (GH_PLAYER_RADIUS, FOOT_CLEARANCE - GH_PLAYER_HEIGHT);
        let (player, _) =
            world.insert(
                RigidBodyBuilder::kinematic_position_based(),
                ColliderBuilder::cylinder(0.5 * (top - bottom), PLAYER_RADIUS)
                    .translation(PVec::new(0.0, 0.5 * (top + bottom), 0.0)),
            );
        PhysicsWorld {
            world,
            player,
            player_at: None,
            statics: Vec::new(),
            props: Vec::new(),
            cost: Duration::ZERO,
            cost_max: Duration::ZERO,
            steps: 0,
        }
    }

    // ── The static world ─────────────────────────────────────────────────────────────────

    /// Forget the static world: the scene it described is gone.
    pub fn clear_static(&mut self) {
        for h in self.statics.drain(..) {
            self.world.remove_collider(h);
        }
    }

    /// Add a triangle mesh the ported pass already collides with, as a fixed collider: the
    /// collider's own mesh, shared, not copied -- it was built with the edge pseudo-normals
    /// a rolling body needs (`TriMeshCollider::new`), and its BVH is the expensive part of
    /// a scene load.
    pub fn add_static_trimesh(&mut self, mesh: &TriMeshCollider) {
        let shape = SharedShape(mesh.shape());
        let h = self.world.insert_collider(ColliderBuilder::new(shape), None);
        self.statics.push(h);
    }

    /// Add a rectangle collider, given in world space as its centre and two half-extent
    /// vectors, as a thin fixed box (`RECT_HALF_THICKNESS`). A degenerate rectangle -- a
    /// zero axis, which a scaled-flat object can produce -- is skipped.
    pub fn add_static_rect(&mut self, centre: Vector3, half_u: Vector3, half_v: Vector3) {
        let (lu, lv) = (half_u.mag(), half_v.mag());
        if lu < 1e-5 || lv < 1e-5 {
            return;
        }
        let (u, v) = (half_u / lu, half_v / lv);
        let n = u.cross(v).normalized_safe();
        let rot = Rot3::from_mat3(&Mat3::from_cols(to_p(u), to_p(v), to_p(n)));
        let collider = ColliderBuilder::cuboid(lu, lv, RECT_HALF_THICKNESS)
            .position(Pose::from_parts(to_p(centre), rot));
        let h = self.world.insert_collider(collider, None);
        self.statics.push(h);
    }

    /// Rebuild the static world from a freshly loaded scene: every object's triangle mesh
    /// and every rectangle collider under `RECT_CAP` per mesh, in world space (module docs).
    pub fn rebuild_static(&mut self, objects: &[Rc<RefCell<dyn ObjectT>>]) {
        let t0 = Instant::now();
        self.clear_static();
        let (mut triangles, mut boxes) = (0usize, 0usize);
        for handle in objects {
            let Ok(obj) = handle.try_borrow() else { continue };
            if let Some(mesh) = obj.trimesh() {
                triangles += mesh.num_triangles();
                self.add_static_trimesh(&mesh);
            }
            let base = obj.base();
            let Some(mesh) = base.mesh.as_ref() else { continue };
            if mesh.colliders.is_empty() || mesh.colliders.len() > RECT_CAP {
                continue;
            }
            let l2w = base.local_to_world();
            for c in &mesh.colliders {
                let m = c.mat();
                self.add_static_rect(
                    l2w.mul_point(m.translation()),
                    l2w.mul_direction(m.x_axis()),
                    l2w.mul_direction(m.y_axis()),
                );
            }
            boxes += mesh.colliders.len();
        }
        // The cylinder goes wherever the player is next seen, not swept there from the old
        // scene.
        self.player_at = None;
        log::debug!(
            "[phys] static world: {triangles} triangles, {boxes} boxes in {:.1} ms",
            t0.elapsed().as_secs_f32() * 1e3
        );
    }

    // ── Bodies ───────────────────────────────────────────────────────────────────────────

    /// A new prop body at `pos` with rotation `rot`, dynamic, awake. `name` is for the
    /// `[prop]` report.
    pub fn add_body(
        &mut self,
        name: &'static str,
        shape: &Shape,
        material: Material,
        pos: Vector3,
        rot: &Matrix4,
    ) -> BodyId {
        let (collider_shape, centre) = shape.build();
        let (body, collider) = self.world.insert(
            RigidBodyBuilder::dynamic().pose(pose(pos, rot)),
            ColliderBuilder::new(collider_shape)
                .translation(centre)
                .friction(material.friction)
                .restitution(material.restitution)
                .density(material.density),
        );
        let id = BodyId { body, collider };
        self.props.push(Prop { name, id });
        id
    }

    /// Take a prop's body out of the world. A handle the world no longer knows -- a prop
    /// outliving a rebuild -- is ignored.
    pub fn remove_body(&mut self, id: BodyId) {
        self.props.retain(|p| p.id != id);
        self.world.remove_body(id.body);
    }

    /// Hand the body to its holder: it follows `set_pose` and pushes what it meets.
    pub fn set_kinematic(&mut self, id: BodyId) {
        if let Some(rb) = self.world.bodies.get_mut(id.body) {
            rb.set_body_type(RigidBodyType::KinematicPositionBased, true);
        }
    }

    /// Give the body back to gravity.
    pub fn set_dynamic(&mut self, id: BodyId) {
        if let Some(rb) = self.world.bodies.get_mut(id.body) {
            rb.set_body_type(RigidBodyType::Dynamic, true);
        }
    }

    /// Where the body should be. A kinematic body is moved there over the next step, so the
    /// solver sees its velocity and what it runs into is pushed; a dynamic one is put there.
    pub fn set_pose(&mut self, id: BodyId, pos: Vector3, rot: &Matrix4) {
        if let Some(rb) = self.world.bodies.get_mut(id.body) {
            let p = pose(pos, rot);
            if rb.is_kinematic() {
                rb.set_next_kinematic_position(p);
            } else {
                rb.set_position(p, true);
            }
        }
    }

    /// The body's origin and rotation, or `None` for a handle the world does not know.
    pub fn pose(&self, id: BodyId) -> Option<(Vector3, Matrix4)> {
        let rb = self.world.bodies.get(id.body)?;
        let p = rb.position();
        Some((from_p(p.translation), from_quat(p.rotation)))
    }

    /// Linear and angular velocity, and a wake-up: what a throw sets.
    pub fn set_velocity(&mut self, id: BodyId, linear: Vector3, angular: Vector3) {
        if let Some(rb) = self.world.bodies.get_mut(id.body) {
            rb.set_linvel(to_p(linear), true);
            rb.set_angvel(to_p(angular), true);
        }
    }

    /// Replace the collider's shape -- the grab has resized the prop. The mass follows: the
    /// density stays and the volume is the new shape's.
    pub fn rescale_collider(&mut self, id: BodyId, shape: &Shape) {
        if let Some(c) = self.world.colliders.get_mut(id.collider) {
            let (collider_shape, centre) = shape.build();
            c.set_shape(collider_shape);
            c.set_translation_wrt_parent(centre);
        }
    }

    /// Whether the body has come to rest (rapier's own sleep rule).
    pub fn asleep(&self, id: BodyId) -> bool {
        self.world.bodies.get(id.body).is_some_and(|rb| rb.is_sleeping())
    }

    /// Raise every prop by `h` metres, tip it by [`DROP_TILT`] and wake it: the
    /// `--drop-props` test. Tipped, because a thing let go of by a hand is never square to
    /// the floor, and a die dropped square lands square -- the test wants to see it roll.
    pub fn lift_all(&mut self, h: f32) {
        let tip = Rot3::from_axis_angle(PVec::new(1.0, 0.0, 1.0).normalize(), DROP_TILT);
        for p in &self.props {
            if let Some(rb) = self.world.bodies.get_mut(p.id.body) {
                let t = rb.translation() + PVec::new(0.0, h, 0.0);
                rb.set_position(Pose::from_parts(t, tip * rb.rotation()), true);
            }
        }
    }

    /// Every prop's name, position and rest state, in registration order.
    pub fn report(&self) -> Vec<BodyReport> {
        self.props
            .iter()
            .filter_map(|p| {
                let rb = self.world.bodies.get(p.id.body)?;
                let m = from_quat(*rb.rotation());
                let trace = m.m[0] + m.m[5] + m.m[10];
                Some(BodyReport {
                    name: p.name,
                    pos: from_p(rb.translation()),
                    turned: (0.5 * (trace - 1.0)).clamp(-1.0, 1.0).acos().to_degrees(),
                    held: rb.is_kinematic(),
                    asleep: self.asleep(p.id),
                })
            })
            .collect()
    }

    // ── The step ─────────────────────────────────────────────────────────────────────────

    /// One fixed step of `GH_DT`, with the player's cylinder moved to `eye` first -- swept
    /// there if the move is a step's worth, put there if it is a teleport (`TELEPORT_DIST`).
    pub fn step(&mut self, eye: Vector3) {
        let t0 = Instant::now();
        if let Some(rb) = self.world.bodies.get_mut(self.player) {
            let walked = self.player_at.is_some_and(|at| (eye - at).mag() < TELEPORT_DIST);
            if walked {
                rb.set_next_kinematic_translation(to_p(eye));
            } else {
                rb.set_translation(to_p(eye), false);
            }
        }
        self.player_at = Some(eye);
        self.world.step();
        let dt = t0.elapsed();
        self.cost += dt;
        self.cost_max = self.cost_max.max(dt);
        self.steps += 1;
    }

    /// The step's cost since the last call, `(average microseconds, worst, steps)`, and a
    /// reset. For the `[phys]` line.
    pub fn step_cost(&mut self) -> (f32, f32, u32) {
        let n = self.steps;
        let avg = if n == 0 { 0.0 } else { self.cost.as_secs_f32() * 1e6 / n as f32 };
        let max = self.cost_max.as_secs_f32() * 1e6;
        self.cost = Duration::ZERO;
        self.cost_max = Duration::ZERO;
        self.steps = 0;
        (avg, max, n)
    }

    /// Test-only: the bodies the world holds besides the player's.
    #[cfg(test)]
    fn prop_count(&self) -> usize {
        self.props.len()
    }
}

thread_local! {
    /// The one world (module docs, "One world, reachable from anywhere").
    static WORLD: RefCell<PhysicsWorld> = RefCell::new(PhysicsWorld::new());
}

/// Run `f` on the world. Panics if called while another call is in progress -- nothing here
/// re-enters, and a prop's `Drop` goes through [`try_with`] for the one case that can.
pub fn with<R>(f: impl FnOnce(&mut PhysicsWorld) -> R) -> R {
    WORLD.with(|w| f(&mut w.borrow_mut()))
}

/// [`with`] for a prop's `Drop`: a thread-local is gone while the thread's own thread-locals
/// are being destroyed, and a prop dropped then has nothing to unregister from.
pub fn try_with(f: impl FnOnce(&mut PhysicsWorld)) {
    let _ = WORLD.try_with(|w| f(&mut w.borrow_mut()));
}

/// `Engine::load_scene`: the new scene's static world.
pub fn rebuild_static(objects: &[Rc<RefCell<dyn ObjectT>>]) {
    with(|w| w.rebuild_static(objects));
}

/// `Engine::update`: one fixed step.
pub fn step(eye: Vector3) {
    with(|w| w.step(eye));
}

/// The `[phys]` and `[prop]` lines, at shot time.
pub fn log_report() {
    with(|w| {
        let (avg, max, n) = w.step_cost();
        log::info!("[phys] step avg {avg:.1} us, max {max:.1} us over {n} steps");
        for r in w.report() {
            log::info!(
                "[prop] {} at ({:.3}, {:.3}, {:.3}) turned {:.1} deg, {}",
                r.name,
                r.pos.x,
                r.pos.y,
                r.pos.z,
                r.turned,
                if r.held {
                    "held"
                } else if r.asleep {
                    "asleep"
                } else {
                    "awake"
                }
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 10 x 10 m floor at y = 0, two triangles, as a `TriMeshCollider`.
    fn floor() -> TriMeshCollider {
        let p = [[-5.0, 0.0, -5.0], [5.0, 0.0, -5.0], [5.0, 0.0, 5.0], [-5.0, 0.0, 5.0]];
        let idx = [0u32, 2, 1, 0, 3, 2];
        TriMeshCollider::new(&p, &idx, &Matrix4::identity())
    }

    fn rubber() -> Material {
        Material { friction: 0.5, restitution: 0.0, density: 1000.0 }
    }

    fn run(w: &mut PhysicsWorld, secs: f32) {
        let n = (secs / GH_DT).round() as u32;
        for _ in 0..n {
            w.step(Vector3::new(100.0, GH_PLAYER_HEIGHT, 100.0));
        }
    }

    #[test]
    fn a_ball_dropped_on_a_floor_settles_at_its_radius_and_sleeps() {
        let mut w = PhysicsWorld::new();
        w.add_static_trimesh(&floor());
        let r = 0.045;
        let id = w.add_body(
            "ball",
            &Shape::Ball { radius: r },
            rubber(),
            Vector3::new(0.0, 1.0, 0.0),
            &Matrix4::identity(),
        );
        run(&mut w, 2.0);
        let (p, _) = w.pose(id).unwrap();
        assert!((p.y - r).abs() < 2e-3, "rests at y = {} for r = {r}", p.y);
        assert!(p.x.abs() < 1e-3 && p.z.abs() < 1e-3, "drifted to {p:?}");
        assert!(w.asleep(id), "still awake after two seconds");
        let rep = w.report();
        assert_eq!(rep.len(), 1);
        assert_eq!(rep[0].name, "ball");
        assert!(rep[0].asleep);
        assert!(rep[0].turned < 1.0, "a straight drop does not turn it: {}", rep[0].turned);
        let (avg, _, n) = w.step_cost();
        assert_eq!(n, 1000);
        assert!(avg > 0.0);
        assert_eq!(w.step_cost().2, 0, "reset by the read");
    }

    #[test]
    fn a_standing_cylinder_rests_on_its_base() {
        // The origin is the base's centre, so a cylinder placed ON the floor stays put.
        let mut w = PhysicsWorld::new();
        w.add_static_trimesh(&floor());
        let id = w.add_body(
            "king",
            &Shape::Cylinder { radius: 0.03, height: 0.14 },
            rubber(),
            Vector3::new(0.0, 0.0, 0.0),
            &Matrix4::identity(),
        );
        run(&mut w, 1.0);
        let (p, rot) = w.pose(id).unwrap();
        assert!(p.y.abs() < 2e-3, "base at y = {}", p.y);
        assert!(rot.y_axis().y > 0.999, "tilted: {:?}", rot.y_axis());
        assert!(w.asleep(id));
    }

    #[test]
    fn a_kinematic_body_holds_its_pose_and_a_release_throws_it() {
        let mut w = PhysicsWorld::new();
        w.add_static_trimesh(&floor());
        let id = w.add_body(
            "dice",
            &Shape::RoundCuboid { half: Vector3::splat(0.03), radius: 0.004 },
            rubber(),
            Vector3::new(0.0, 1.0, 0.0),
            &Matrix4::identity(),
        );
        w.set_kinematic(id);
        assert!(w.report()[0].held);
        let held = Vector3::new(0.5, 1.2, -0.3);
        let tilt = Matrix4::rot_y(0.4) * Matrix4::rot_x(0.2);
        for _ in 0..200 {
            w.set_pose(id, held, &tilt);
            w.step(Vector3::new(100.0, GH_PLAYER_HEIGHT, 100.0));
        }
        let (p, rot) = w.pose(id).unwrap();
        assert!((p - held).mag() < 1e-4, "held body wandered to {p:?}");
        assert!(
            rot.m.iter().zip(tilt.m.iter()).all(|(a, b)| (a - b).abs() < 1e-4),
            "held body turned"
        );

        // The throw: dynamic with the hand's velocity. One step later it has moved by
        // v * dt (plus a whisker of gravity) and is awake.
        w.set_dynamic(id);
        assert!(!w.report()[0].held);
        let v = Vector3::new(3.0, 1.0, -2.0);
        w.set_velocity(id, v, Vector3::zero());
        w.step(Vector3::new(100.0, GH_PLAYER_HEIGHT, 100.0));
        let (p2, _) = w.pose(id).unwrap();
        let moved = (p2 - held) / GH_DT;
        assert!((moved - v).mag() < 0.1, "velocity after one step: {moved:?}");
        assert!(!w.asleep(id));
    }

    #[test]
    fn rescaling_changes_the_collider_and_the_mass_with_it() {
        let mut w = PhysicsWorld::new();
        w.add_static_trimesh(&floor());
        let id = w.add_body(
            "ball",
            &Shape::Ball { radius: 0.05 },
            rubber(),
            Vector3::new(0.0, 0.5, 0.0),
            &Matrix4::identity(),
        );
        let mass_before = w.world.bodies[id.body].mass();
        w.rescale_collider(id, &Shape::Ball { radius: 0.05 }.scaled(2.0));
        let ball = w.world.colliders[id.collider].shape().as_ball().expect("still a ball");
        assert!((ball.radius - 0.1).abs() < 1e-6);
        // Mass follows the volume: eight times for twice the radius. Recomputed by the
        // next step.
        run(&mut w, 0.01);
        let mass_after = w.world.bodies[id.body].mass();
        assert!((mass_after / mass_before - 8.0).abs() < 1e-3, "{mass_before} -> {mass_after}");
        // And it rests at the new radius.
        run(&mut w, 2.0);
        assert!((w.pose(id).unwrap().0.y - 0.1).abs() < 2e-3);
    }

    #[test]
    fn the_player_pushes_a_ball_it_walks_into_ahead_and_never_into_the_floor() {
        let mut w = PhysicsWorld::new();
        w.add_static_trimesh(&floor());
        let id = w.add_body(
            "ball",
            &Shape::Ball { radius: 0.045 },
            Material { friction: 0.7, restitution: 0.25, density: 800.0 },
            Vector3::new(0.0, 0.045, 0.0),
            &Matrix4::identity(),
        );
        // Stand a metre off, let the ball settle, then walk through it at 3 m/s. At every
        // step the ball is ahead of the player's front face and never under the carpet: the
        // capsule this replaced trod it into the floor and dropped it behind (FOOT_CLEARANCE).
        let mut eye = Vector3::new(-1.0, GH_PLAYER_HEIGHT, 0.0);
        for _ in 0..500 {
            w.step(eye);
        }
        let mut lowest = f32::MAX;
        for _ in 0..400 {
            eye.x += 3.0 * GH_DT;
            w.step(eye);
            let (p, _) = w.pose(id).unwrap();
            lowest = lowest.min(p.y);
            assert!(p.x > eye.x + PLAYER_RADIUS - 0.01, "ball fell behind: {p:?} vs eye {eye:?}");
        }
        let (p, _) = w.pose(id).unwrap();
        assert!(p.x > 0.3, "ball was not pushed: {p:?}");
        assert!(p.y < 0.3, "ball should stay near the floor: {p:?}");
        assert!(lowest > 0.04, "ball was pressed into the floor: lowest centre {lowest}");
    }

    #[test]
    fn a_teleport_does_not_sweep_the_player_through_a_prop() {
        let mut w = PhysicsWorld::new();
        w.add_static_trimesh(&floor());
        let id = w.add_body(
            "ball",
            &Shape::Ball { radius: 0.045 },
            rubber(),
            Vector3::new(0.0, 0.045, 0.0),
            &Matrix4::identity(),
        );
        for _ in 0..500 {
            w.step(Vector3::new(-3.0, GH_PLAYER_HEIGHT, 0.0));
        }
        // Five metres in one step, straight through the ball's position.
        for _ in 0..100 {
            w.step(Vector3::new(2.0, GH_PLAYER_HEIGHT, 0.0));
        }
        let (p, _) = w.pose(id).unwrap();
        assert!(p.x.abs() < 0.01, "ball was swept: {p:?}");
    }

    #[test]
    fn rect_colliders_become_boxes_a_prop_can_rest_on() {
        let mut w = PhysicsWorld::new();
        // A 4 x 4 m rectangle at y = 1, then a ball dropped onto it.
        w.add_static_rect(
            Vector3::new(0.0, 1.0, 0.0),
            Vector3::new(2.0, 0.0, 0.0),
            Vector3::new(0.0, 0.0, 2.0),
        );
        // A degenerate one is skipped, not built.
        w.add_static_rect(Vector3::zero(), Vector3::zero(), Vector3::new(0.0, 0.0, 1.0));
        assert_eq!(w.statics.len(), 1);
        let id = w.add_body(
            "ball",
            &Shape::Ball { radius: 0.1 },
            rubber(),
            Vector3::new(0.5, 2.0, 0.5),
            &Matrix4::identity(),
        );
        run(&mut w, 2.0);
        let (p, _) = w.pose(id).unwrap();
        assert!((p.y - (1.0 + RECT_HALF_THICKNESS + 0.1)).abs() < 2e-3, "rests at {}", p.y);
        w.clear_static();
        assert!(w.statics.is_empty());
        run(&mut w, 0.5);
        assert!(w.pose(id).unwrap().0.y < 0.5, "nothing left to rest on");
    }

    #[test]
    fn lift_and_remove() {
        let mut w = PhysicsWorld::new();
        w.add_static_trimesh(&floor());
        let a = w.add_body(
            "a",
            &Shape::Ball { radius: 0.05 },
            rubber(),
            Vector3::new(0.0, 0.05, 0.0),
            &Matrix4::identity(),
        );
        let b = w.add_body(
            "b",
            &Shape::Cuboid { half: Vector3::splat(0.05) },
            rubber(),
            Vector3::new(1.0, 0.05, 0.0),
            &Matrix4::identity(),
        );
        run(&mut w, 1.0);
        assert!(w.asleep(a) && w.asleep(b));
        let rest = w.pose(a).unwrap().0.y;
        w.lift_all(2.0);
        assert!((w.pose(a).unwrap().0.y - (rest + 2.0)).abs() < 1e-5);
        assert!(!w.asleep(a) && !w.asleep(b), "lifted bodies are awake");
        let tipped = w.report()[1].turned;
        assert!((tipped - DROP_TILT.to_degrees()).abs() < 0.1, "tipped by {tipped} deg");
        run(&mut w, 2.0);
        assert!((w.pose(a).unwrap().0.y - 0.05).abs() < 2e-3, "back on the floor");
        w.remove_body(a);
        assert_eq!(w.prop_count(), 1);
        assert!(w.pose(a).is_none());
        // Removing again, or a handle from before a rebuild, is harmless.
        w.remove_body(a);
        assert_eq!(w.report().len(), 1);
    }

    #[test]
    fn rotation_round_trips_through_the_quaternion() {
        let m = Matrix4::rot_y(0.7) * Matrix4::rot_x(-0.4) * Matrix4::rot_z(2.5);
        let back = from_quat(to_quat(&m));
        assert!(m.m.iter().zip(back.m.iter()).all(|(a, b)| (a - b).abs() < 1e-5), "{back:?}");
    }
}
