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
        let mut col = Collider {
            mat: Matrix4::identity(),
        };
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
}
