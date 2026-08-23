// Port of Collider.h / Collider.cpp

use crate::game_header::gh_clamp;
use crate::vector::{Matrix4, Vector3};

// PORT: `mat` stays private exactly as in the C++ (Collider.h:16). Copy is derived so
// colliders can be stored/passed by value the way the C++ stores them by value in
// std::vector<Collider> (Mesh.h:19).
#[derive(Clone, Copy)]
pub struct Collider {
    mat: Matrix4,
}

impl Collider {
    // Collider::Collider (Collider.cpp:7-21)
    pub fn new(a: Vector3, b: Vector3, c: Vector3) -> Collider {
        // PORT: the C++ constructor leaves `mat` default-constructed (i.e. uninitialized
        // garbage) and relies on CreateSorted's MakeIdentity() to fill it. Rust has no
        // uninitialized struct field, so it is seeded with identity here; CreateSorted
        // overwrites it unconditionally anyway.
        // (was: Matrix4 mat; // no initializer, Vector.h:165, Collider.cpp:7)
        let mut col = Collider { mat: Matrix4::identity() };
        let ab = b - a;
        let bc = c - b;
        let ca = a - c;
        let mag_ab = ab.mag_sq();
        let mag_bc = bc.mag_sq();
        let mag_ca = ca.mag_sq();
        if mag_ab >= mag_bc && mag_ab >= mag_ca {
            col.create_sorted(bc * 0.5, (a + b) * 0.5, ca * 0.5);
        } else if mag_bc >= mag_ab && mag_bc >= mag_ca {
            col.create_sorted(ca * 0.5, (b + c) * 0.5, ab * 0.5);
        } else {
            col.create_sorted(ab * 0.5, (c + a) * 0.5, bc * 0.5);
        }
        col
    }

    // Collider::Collide (Collider.cpp:23-45)
    // PORT: the out-parameter `Vector3& delta` plus bool return becomes Option<Vector3>
    // (was: bool Collide(const Matrix4& localToUnit, Vector3& delta) const, Collider.cpp:23).
    // PORT: parameter named `local_to_unit` after the .cpp definition; the declaration in
    // Collider.h:9 confusingly calls the same parameter `localToWorld`.
    pub fn collide(&self, local_to_unit: &Matrix4) -> Option<Vector3> {
        //Get world delta
        let local = *local_to_unit * self.mat;
        let v = -local.translation();

        //Get axes
        let x = local.x_axis();
        let y = local.y_axis();

        //Find closest point
        let px = gh_clamp(v.dot(x) / x.mag_sq(), -1.0f32, 1.0f32);
        let py = gh_clamp(v.dot(y) / y.mag_sq(), -1.0f32, 1.0f32);
        let closest = x * px + y * py;

        //Calculate distance to closest point
        let mut delta = v - closest;
        if delta.mag_sq() >= 1.0 {
            None
        } else {
            delta = delta.normalized() - delta;
            Some(delta)
        }
    }

    // PORT: Collider::DebugDraw (Collider.cpp:47-67) is dropped entirely. It is written in
    // immediate mode -- glBegin/glColor3f/glVertex4f/glEnd -- which does not exist in an
    // OpenGL 3.3+ Core profile (macOS only grants Core), and glow exposes no such entry
    // points. Nothing in the codebase calls it (Mesh::DebugDraw, its only caller, is itself
    // never called). (was: void DebugDraw(const Camera& cam, const Matrix4& objMat), Collider.h:11)

    // Collider::CreateSorted (Collider.cpp:69-75)
    fn create_sorted(&mut self, da: Vector3, c: Vector3, db: Vector3) {
        // PORT: C++ `assert` compiles out of release builds; `debug_assert!` is the exact
        // analogue (was: assert(...), Collider.cpp:70).
        debug_assert!(da.dot(db).abs() / (da.mag() * db.mag()) < 0.001);
        self.mat.make_identity();
        self.mat.set_translation(c);
        self.mat.set_x_axis(da);
        self.mat.set_y_axis(db);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// EXT: additions beyond the C++ port. Everything above this line is a faithful
// transcription of Collider.cpp; everything below is new.
// ─────────────────────────────────────────────────────────────────────────────

impl Collider {
    /// EXT: the rectangle's local matrix. The port keeps `mat` private (Collider.h:16),
    /// but ray casting needs to push the rectangle into world space, so expose it read-only.
    pub fn mat(&self) -> &Matrix4 {
        &self.mat
    }

    /// EXT: a rectangle by its centre and two half-extent vectors, which must be
    /// perpendicular. The ported `new` takes three CORNERS -- `a`, `b`, `c` in either
    /// winding, the longest of the three sides being the diagonal -- the way an `.obj` `c`
    /// line lists them (Mesh.cpp:85-89); code that has a centre and two axes in hand would
    /// have to build corners only to have them turned back into this, so the sorting is
    /// skipped and the matrix written as `create_sorted` would: translation `centre`, X axis
    /// `half_u`, Y axis `half_v`.
    #[allow(dead_code)] // EXT: scene code builds its in-memory colliders with it.
    pub fn rect(centre: Vector3, half_u: Vector3, half_v: Vector3) -> Collider {
        let mut col = Collider { mat: Matrix4::identity() };
        col.create_sorted(half_u, centre, half_v);
        col
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::Object;
    use crate::physical::Physical;
    use crate::sphere::Sphere;

    fn approx(a: Vector3, b: Vector3) -> bool {
        (a - b).mag() < 1e-5
    }

    /// The 2x2 floor rectangle at y = 0 a mesh `c` line of those three corners produces.
    fn floor() -> Collider {
        Collider::new(
            Vector3::new(-1.0, 0.0, -1.0),
            Vector3::new(1.0, 0.0, -1.0),
            Vector3::new(1.0, 0.0, 1.0),
        )
    }

    /// Exactly what Engine::update does per hit sphere (Engine.cpp:166-178): the push in the
    /// sphere's unit space, carried back to world as a direction.
    fn world_push(phys: &Physical, owner: &Object, collider: &Collider) -> Option<Vector3> {
        let world_to_local = phys.world_to_local();
        let sphere = phys.hit_spheres[0];
        let world_to_unit = sphere.local_to_unit() * world_to_local;
        let local_to_unit = world_to_unit * owner.local_to_world();
        let unit_to_world = world_to_unit.inverse();
        collider.collide(&local_to_unit).map(|push| unit_to_world.mul_direction(push))
    }

    fn unit_sphere_at(pos: Vector3) -> Physical {
        let mut p = Physical::new();
        p.set_position(pos);
        p.hit_spheres.push(Sphere::new(1.0));
        p
    }

    #[test]
    fn the_longest_side_becomes_the_diagonal() {
        let m = *floor().mat();
        assert!(approx(m.translation(), Vector3::zero()));
        assert!(approx(m.x_axis(), Vector3::new(1.0, 0.0, 0.0)));
        assert!(approx(m.y_axis(), Vector3::new(0.0, 0.0, 1.0)));
        // Any corner order gives the same rectangle up to the sign of its axes.
        let m2 = *Collider::new(
            Vector3::new(1.0, 0.0, 1.0),
            Vector3::new(-1.0, 0.0, -1.0),
            Vector3::new(1.0, 0.0, -1.0),
        )
        .mat();
        assert!(approx(m2.translation(), Vector3::zero()));
        assert!((m2.x_axis().mag() - 1.0).abs() < 1e-5 && (m2.y_axis().mag() - 1.0).abs() < 1e-5);
        assert!(m2.x_axis().y.abs() < 1e-6 && m2.y_axis().y.abs() < 1e-6);
    }

    /// EXT: `rect` is the corner constructor with the sorting already done.
    #[test]
    fn rect_is_the_same_rectangle_as_its_three_corners() {
        let c = Vector3::new(3.0, 1.0, -2.0);
        let u = Vector3::new(2.0, 0.0, 0.0);
        let v = Vector3::new(0.0, 0.0, 0.5);
        let r = Collider::rect(c, u, v);
        let by_corners = Collider::new(c - u - v, c + u - v, c + u + v);
        let (m, n) = (r.mat(), by_corners.mat());
        assert!(approx(m.translation(), n.translation()) && approx(m.translation(), c));
        assert!(approx(m.x_axis(), n.x_axis()) && approx(m.x_axis(), u));
        assert!(approx(m.y_axis(), n.y_axis()) && approx(m.y_axis(), v));
        // And it collides like one: a unit sphere half sunk into it is pushed up by half.
        let phys = unit_sphere_at(c + Vector3::new(0.0, 0.5, 0.0));
        let push = world_push(&phys, &Object::new(), &r).expect("overlapping");
        assert!(approx(push, Vector3::new(0.0, 0.5, 0.0)), "{push:?}");
    }

    #[test]
    fn a_sphere_sunk_into_the_floor_is_pushed_straight_up_by_the_overlap() {
        let floor = floor();
        let owner = Object::new();
        let phys = unit_sphere_at(Vector3::new(0.0, 0.5, 0.0));
        let push = world_push(&phys, &owner, &floor).expect("overlapping");
        assert!(approx(push, Vector3::new(0.0, 0.5, 0.0)), "{push:?}");
        // Applying it leaves the sphere resting on the floor, and a second test finds nothing.
        let mut phys = phys;
        phys.on_collide(push);
        assert!(approx(phys.base.pos, Vector3::new(0.0, 1.0, 0.0)));
        assert!(world_push(&phys, &owner, &floor).is_none_or(|p| p.mag() < 1e-5));
        // Off to the side of the rectangle's edge the push points diagonally away from it,
        // and a sphere clear of it gets none.
        // The nearest point is the corner (1, 0, 0), half a radius away: the push is the other
        // half of that radius along the same line.
        let phys = unit_sphere_at(Vector3::new(1.3, 0.4, 0.0));
        let push = world_push(&phys, &owner, &floor).expect("clipping the edge");
        assert!(approx(push, Vector3::new(0.3, 0.4, 0.0)), "{push:?}");
        assert!(world_push(&unit_sphere_at(Vector3::new(0.0, 1.5, 0.0)), &owner, &floor).is_none());
        assert!(world_push(&unit_sphere_at(Vector3::new(3.0, 0.5, 0.0)), &owner, &floor).is_none());
    }

    #[test]
    fn the_owners_transform_and_the_spheres_scale_go_through_the_matrices() {
        let floor = floor();
        // The floor's owner is raised to y = 2 and stretched to 4 wide: a sphere at x = 1.5
        // is over the slab rather than past its edge, so the push is straight up by the
        // overlap with the raised plane (unstretched, it would be diagonal, off the corner).
        let mut owner = Object::new();
        owner.pos = Vector3::new(0.0, 2.0, 0.0);
        owner.scale = Vector3::new(2.0, 1.0, 1.0);
        let phys = unit_sphere_at(Vector3::new(1.5, 2.25, 0.0));
        let push = world_push(&phys, &owner, &floor).expect("under the stretched floor");
        assert!(approx(push, Vector3::new(0.0, 0.75, 0.0)), "{push:?}");
        // A traveller that came through a scaling portal at p_scale 2 has a 2-unit sphere in
        // world: centred 1.5 above a floor it is pushed up by half a unit.
        let mut big = unit_sphere_at(Vector3::new(0.0, 1.5, 0.0));
        big.base.p_scale = 2.0;
        let push = world_push(&big, &Object::new(), &floor).expect("bigger sphere");
        assert!(approx(push, Vector3::new(0.0, 0.5, 0.0)), "{push:?}");
        // A floor turned on its side is a wall: the push is sideways.
        let mut wall = Object::new();
        wall.euler.z = std::f32::consts::FRAC_PI_2;
        let phys = unit_sphere_at(Vector3::new(0.5, 0.0, 0.0));
        let push = world_push(&phys, &wall, &floor).expect("against the wall");
        assert!(approx(push, Vector3::new(0.5, 0.0, 0.0)), "{push:?}");
    }
}
