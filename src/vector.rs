//! Port of Vector.h -- Vector3, Vector4, Matrix4.
//!
//! Matrix4 is ROW-MAJOR: element m[i*4 + j] is row i, column j, and the translation
//! lives at m[3], m[7], m[11].
//!
//! PORT: the std::ostream operator<< overloads (Vector.h:496-510) are replaced by
//! #[derive(Debug)] on each type (was: inline std::ostream& operator<<(...), Vector.h:496).

use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

//=============================================================================
// Vector3 (Vector.h:8-138)
//=============================================================================

// PORT: the C++ default ctor `Vector3() {}` leaves x/y/z UNINITIALIZED; Default here
// zero-initializes them (was: Vector3() {}, Vector.h:11).
#[derive(Clone, Copy, Debug, Default)]
pub struct Vector3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vector3 {
    //Constructors
    // PORT: the four overloaded ctors become distinctly-named fns; `new` is the
    // (x,y,z) form, `splat` the single-float form, `from_slice` the `const float*`
    // form (was: explicit Vector3(float b) / Vector3(const float* b) / Vector3(float x, float y, float z), Vector.h:12-14).
    #[inline]
    pub fn new(x: f32, y: f32, z: f32) -> Vector3 {
        Vector3 { x, y, z }
    }
    #[inline]
    pub fn splat(b: f32) -> Vector3 {
        Vector3 { x: b, y: b, z: b }
    }
    #[inline]
    pub fn from_slice(b: &[f32]) -> Vector3 {
        Vector3 { x: b[0], y: b[1], z: b[2] }
    }

    //General
    #[inline]
    pub fn zero() -> Vector3 {
        Vector3::splat(0.0)
    }
    #[inline]
    pub fn ones() -> Vector3 {
        Vector3::splat(1.0)
    }
    #[inline]
    pub fn unit_x() -> Vector3 {
        Vector3::new(1.0, 0.0, 0.0)
    }
    #[inline]
    pub fn unit_y() -> Vector3 {
        Vector3::new(0.0, 1.0, 0.0)
    }
    #[inline]
    pub fn unit_z() -> Vector3 {
        Vector3::new(0.0, 0.0, 1.0)
    }

    //Setters
    #[inline]
    pub fn set(&mut self, _x: f32, _y: f32, _z: f32) {
        self.x = _x;
        self.y = _y;
        self.z = _z;
    }
    #[inline]
    pub fn set_zero(&mut self) {
        self.x = 0.0;
        self.y = 0.0;
        self.z = 0.0;
    }
    #[inline]
    pub fn set_ones(&mut self) {
        self.x = 1.0;
        self.y = 1.0;
        self.z = 1.0;
    }
    #[inline]
    pub fn set_unit_x(&mut self) {
        self.x = 1.0;
        self.y = 0.0;
        self.z = 0.0;
    }
    #[inline]
    pub fn set_unit_y(&mut self) {
        self.x = 0.0;
        self.y = 1.0;
        self.z = 0.0;
    }
    #[inline]
    pub fn set_unit_z(&mut self) {
        self.x = 0.0;
        self.y = 0.0;
        self.z = 1.0;
    }

    //Vector algebra
    #[inline]
    pub fn dot(&self, b: Vector3) -> f32 {
        self.x * b.x + self.y * b.y + self.z * b.z
    }
    #[inline]
    pub fn cross(&self, b: Vector3) -> Vector3 {
        Vector3::new(
            self.y * b.z - self.z * b.y,
            self.z * b.x - self.x * b.z,
            self.x * b.y - self.y * b.x,
        )
    }
    #[inline]
    pub fn mag_sq(&self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }
    #[inline]
    pub fn mag(&self) -> f32 {
        self.mag_sq().sqrt()
    }

    //Normalization
    #[inline]
    pub fn normalize(&mut self) {
        *self /= self.mag();
    }
    // PORT: FLT_EPSILON -> f32::EPSILON (was: (*this) /= (Mag() + FLT_EPSILON), Vector.h:103).
    #[inline]
    pub fn normalize_safe(&mut self) {
        *self /= self.mag() + f32::EPSILON;
    }
    #[inline]
    pub fn normalized(&self) -> Vector3 {
        *self / self.mag()
    }
    // PORT: FLT_EPSILON -> f32::EPSILON (was: (*this) / (Mag() + FLT_EPSILON), Vector.h:109).
    #[inline]
    pub fn normalized_safe(&self) -> Vector3 {
        *self / (self.mag() + f32::EPSILON)
    }
    #[inline]
    pub fn angle(&self, b: Vector3) -> f32 {
        self.normalized().dot(b.normalized()).acos()
    }
    #[inline]
    pub fn angle_safe(&self, b: Vector3) -> f32 {
        self.normalized_safe().dot(b.normalized_safe()).acos()
    }
    // PORT: C++ `assert` (compiled out in release) -> debug_assert! (was: assert(m > 0.0f), Vector.h:118).
    #[inline]
    pub fn clip_mag(&mut self, m: f32) {
        debug_assert!(m > 0.0);
        let r = self.mag_sq() / (m * m);
        if r > 1.0 {
            *self /= r.sqrt();
        }
    }

    //Other
    #[inline]
    pub fn is_ndc(&self) -> bool {
        self.x > -1.0
            && self.x < 1.0
            && self.y > -1.0
            && self.y < 1.0
            && self.z > -1.0
            && self.z < 1.0
    }
}

//Basic operatios
impl Add<f32> for Vector3 {
    type Output = Vector3;
    #[inline]
    fn add(self, b: f32) -> Vector3 {
        Vector3::new(self.x + b, self.y + b, self.z + b)
    }
}
impl Sub<f32> for Vector3 {
    type Output = Vector3;
    #[inline]
    fn sub(self, b: f32) -> Vector3 {
        Vector3::new(self.x - b, self.y - b, self.z - b)
    }
}
impl Mul<f32> for Vector3 {
    type Output = Vector3;
    #[inline]
    fn mul(self, b: f32) -> Vector3 {
        Vector3::new(self.x * b, self.y * b, self.z * b)
    }
}
impl Div<f32> for Vector3 {
    type Output = Vector3;
    #[inline]
    fn div(self, b: f32) -> Vector3 {
        Vector3::new(self.x / b, self.y / b, self.z / b)
    }
}
impl Add<Vector3> for Vector3 {
    type Output = Vector3;
    #[inline]
    fn add(self, b: Vector3) -> Vector3 {
        Vector3::new(self.x + b.x, self.y + b.y, self.z + b.z)
    }
}
impl Sub<Vector3> for Vector3 {
    type Output = Vector3;
    #[inline]
    fn sub(self, b: Vector3) -> Vector3 {
        Vector3::new(self.x - b.x, self.y - b.y, self.z - b.z)
    }
}
impl Mul<Vector3> for Vector3 {
    type Output = Vector3;
    #[inline]
    fn mul(self, b: Vector3) -> Vector3 {
        Vector3::new(self.x * b.x, self.y * b.y, self.z * b.z)
    }
}
impl Div<Vector3> for Vector3 {
    type Output = Vector3;
    #[inline]
    fn div(self, b: Vector3) -> Vector3 {
        Vector3::new(self.x / b.x, self.y / b.y, self.z / b.z)
    }
}
impl AddAssign<f32> for Vector3 {
    #[inline]
    fn add_assign(&mut self, b: f32) {
        self.x += b;
        self.y += b;
        self.z += b;
    }
}
impl SubAssign<f32> for Vector3 {
    #[inline]
    fn sub_assign(&mut self, b: f32) {
        self.x -= b;
        self.y -= b;
        self.z -= b;
    }
}
impl MulAssign<f32> for Vector3 {
    #[inline]
    fn mul_assign(&mut self, b: f32) {
        self.x *= b;
        self.y *= b;
        self.z *= b;
    }
}
impl DivAssign<f32> for Vector3 {
    #[inline]
    fn div_assign(&mut self, b: f32) {
        self.x /= b;
        self.y /= b;
        self.z /= b;
    }
}
impl AddAssign<Vector3> for Vector3 {
    #[inline]
    fn add_assign(&mut self, b: Vector3) {
        self.x += b.x;
        self.y += b.y;
        self.z += b.z;
    }
}
impl SubAssign<Vector3> for Vector3 {
    #[inline]
    fn sub_assign(&mut self, b: Vector3) {
        self.x -= b.x;
        self.y -= b.y;
        self.z -= b.z;
    }
}
impl MulAssign<Vector3> for Vector3 {
    #[inline]
    fn mul_assign(&mut self, b: Vector3) {
        self.x *= b.x;
        self.y *= b.y;
        self.z *= b.z;
    }
}
impl DivAssign<Vector3> for Vector3 {
    #[inline]
    fn div_assign(&mut self, b: Vector3) {
        self.x /= b.x;
        self.y /= b.y;
        self.z /= b.z;
    }
}
impl Neg for Vector3 {
    type Output = Vector3;
    #[inline]
    fn neg(self) -> Vector3 {
        Vector3::new(-self.x, -self.y, -self.z)
    }
}

// Free function `operator/(float b, const Vector3& v)` (Vector.h:133-135). Needed by
// Object::WorldToLocal -- `Matrix4::Scale(1.0f / (scale * p_scale))`, Object.cpp:42.
impl Div<Vector3> for f32 {
    type Output = Vector3;
    #[inline]
    fn div(self, v: Vector3) -> Vector3 {
        Vector3::new(self / v.x, self / v.y, self / v.z)
    }
}
// PORT: the free `operator/=(float b, Vector3& v)` (Vector.h:136-138) is dropped -- it
// assigns into its *second* operand, which Rust's DivAssign cannot express, and nothing
// in the codebase calls it.

//=============================================================================
// Vector4 (Vector.h:140-168)
//=============================================================================

// PORT: as with Vector3, the C++ default ctor leaves the components uninitialized
// (was: Vector4() {}, Vector.h:142).
#[derive(Clone, Copy, Debug, Default)]
pub struct Vector4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

// Preserved verbatim from the C++ API surface; not every member is reachable from the
// scenes this port ships. Kept for fidelity rather than deleted.
#[allow(dead_code)]
impl Vector4 {
    // PORT: overloaded ctors split by name, as for Vector3 (was: Vector4(float b) /
    // Vector4(const Vector3& xyz, float w) / Vector4(float,float,float,float), Vector.h:143-145).
    #[inline]
    pub fn new(x: f32, y: f32, z: f32, w: f32) -> Vector4 {
        Vector4 { x, y, z, w }
    }
    #[inline]
    pub fn splat(b: f32) -> Vector4 {
        Vector4 { x: b, y: b, z: b, w: b }
    }
    #[inline]
    pub fn from_vec3(xyz: Vector3, w: f32) -> Vector4 {
        Vector4 { x: xyz.x, y: xyz.y, z: xyz.z, w }
    }

    #[inline]
    pub fn xyz(&self) -> Vector3 {
        Vector3::new(self.x, self.y, self.z)
    }
    #[inline]
    pub fn xyz_normalized(&self) -> Vector3 {
        Vector3::new(self.x, self.y, self.z).normalized()
    }
    #[inline]
    pub fn homogenized(&self) -> Vector3 {
        Vector3::new(self.x / self.w, self.y / self.w, self.z / self.w)
    }

    #[inline]
    pub fn dot(&self, b: Vector4) -> f32 {
        self.x * b.x + self.y * b.y + self.z * b.z + self.w * b.w
    }
}

impl Mul<f32> for Vector4 {
    type Output = Vector4;
    #[inline]
    fn mul(self, b: f32) -> Vector4 {
        Vector4::new(self.x * b, self.y * b, self.z * b, self.w * b)
    }
}
impl Div<f32> for Vector4 {
    type Output = Vector4;
    #[inline]
    fn div(self, b: f32) -> Vector4 {
        Vector4::new(self.x / b, self.y / b, self.z / b, self.w / b)
    }
}
impl MulAssign<f32> for Vector4 {
    #[inline]
    fn mul_assign(&mut self, b: f32) {
        self.x *= b;
        self.y *= b;
        self.z *= b;
        self.w *= b;
    }
}
impl DivAssign<f32> for Vector4 {
    #[inline]
    fn div_assign(&mut self, b: f32) {
        self.x /= b;
        self.y /= b;
        self.z /= b;
        self.w /= b;
    }
}

//=============================================================================
// Matrix4 (Vector.h:170-493) -- ROW MAJOR
//=============================================================================

#[derive(Clone, Copy, Debug)]
pub struct Matrix4 {
    pub m: [f32; 16],
}

// Preserved verbatim from the C++ API surface; not every member is reachable from the
// scenes this port ships. Kept for fidelity rather than deleted.
#[allow(dead_code)]
impl Matrix4 {
    //Constructors
    // PORT: `Matrix4()` leaves all 16 floats uninitialized; every Make*/static below
    // starts from a zeroed array instead, then overwrites every element exactly as the
    // C++ does (was: Matrix4() {}, Vector.h:173).
    #[inline]
    fn uninit() -> Matrix4 {
        Matrix4 { m: [0.0; 16] }
    }
    // `explicit Matrix4(float b) { Fill(b); }` (Vector.h:174)
    #[inline]
    pub fn splat(b: f32) -> Matrix4 {
        let mut m = Matrix4::uninit();
        m.fill(b);
        m
    }

    //General
    #[inline]
    pub fn fill(&mut self, b: f32) {
        self.m.fill(b);
    }
    #[inline]
    pub fn make_zero(&mut self) {
        self.fill(0.0);
    }
    #[inline]
    #[rustfmt::skip]
    pub fn make_identity(&mut self) {
        let m = &mut self.m;
        m[0]  = 1.0; m[1]  = 0.0; m[2]  = 0.0; m[3]  = 0.0;
        m[4]  = 0.0; m[5]  = 1.0; m[6]  = 0.0; m[7]  = 0.0;
        m[8]  = 0.0; m[9]  = 0.0; m[10] = 1.0; m[11] = 0.0;
        m[12] = 0.0; m[13] = 0.0; m[14] = 0.0; m[15] = 1.0;
    }
    #[inline]
    #[rustfmt::skip]
    pub fn make_rot_x(&mut self, a: f32) {
        let m = &mut self.m;
        m[0]  = 1.0; m[1]  = 0.0;     m[2]  = 0.0;      m[3]  = 0.0;
        m[4]  = 0.0; m[5]  = a.cos(); m[6]  = -a.sin(); m[7]  = 0.0;
        m[8]  = 0.0; m[9]  = a.sin(); m[10] = a.cos();  m[11] = 0.0;
        m[12] = 0.0; m[13] = 0.0;     m[14] = 0.0;      m[15] = 1.0;
    }
    #[inline]
    #[rustfmt::skip]
    pub fn make_rot_y(&mut self, a: f32) {
        let m = &mut self.m;
        m[0]  = a.cos();  m[1]  = 0.0; m[2]  = a.sin(); m[3]  = 0.0;
        m[4]  = 0.0;      m[5]  = 1.0; m[6]  = 0.0;     m[7]  = 0.0;
        m[8]  = -a.sin(); m[9]  = 0.0; m[10] = a.cos(); m[11] = 0.0;
        m[12] = 0.0;      m[13] = 0.0; m[14] = 0.0;     m[15] = 1.0;
    }
    #[inline]
    #[rustfmt::skip]
    pub fn make_rot_z(&mut self, a: f32) {
        let m = &mut self.m;
        m[0]  = a.cos(); m[1]  = -a.sin(); m[2]  = 0.0; m[3]  = 0.0;
        m[4]  = a.sin(); m[5]  = a.cos();  m[6]  = 0.0; m[7]  = 0.0;
        m[8]  = 0.0;     m[9]  = 0.0;      m[10] = 1.0; m[11] = 0.0;
        m[12] = 0.0;     m[13] = 0.0;      m[14] = 0.0; m[15] = 1.0;
    }
    #[inline]
    #[rustfmt::skip]
    pub fn make_trans(&mut self, t: Vector3) {
        let m = &mut self.m;
        m[0]  = 1.0; m[1]  = 0.0; m[2]  = 0.0; m[3]  = t.x;
        m[4]  = 0.0; m[5]  = 1.0; m[6]  = 0.0; m[7]  = t.y;
        m[8]  = 0.0; m[9]  = 0.0; m[10] = 1.0; m[11] = t.z;
        m[12] = 0.0; m[13] = 0.0; m[14] = 0.0; m[15] = 1.0;
    }
    #[inline]
    #[rustfmt::skip]
    pub fn make_scale(&mut self, s: Vector3) {
        let m = &mut self.m;
        m[0]  = s.x; m[1]  = 0.0; m[2]  = 0.0; m[3]  = 0.0;
        m[4]  = 0.0; m[5]  = s.y; m[6]  = 0.0; m[7]  = 0.0;
        m[8]  = 0.0; m[9]  = 0.0; m[10] = s.z; m[11] = 0.0;
        m[12] = 0.0; m[13] = 0.0; m[14] = 0.0; m[15] = 1.0;
    }

    //Statics
    #[inline]
    pub fn zero() -> Matrix4 {
        let mut m = Matrix4::uninit();
        m.make_zero();
        m
    }
    #[inline]
    pub fn identity() -> Matrix4 {
        let mut m = Matrix4::uninit();
        m.make_identity();
        m
    }
    #[inline]
    pub fn rot_x(a: f32) -> Matrix4 {
        let mut m = Matrix4::uninit();
        m.make_rot_x(a);
        m
    }
    #[inline]
    pub fn rot_y(a: f32) -> Matrix4 {
        let mut m = Matrix4::uninit();
        m.make_rot_y(a);
        m
    }
    #[inline]
    pub fn rot_z(a: f32) -> Matrix4 {
        let mut m = Matrix4::uninit();
        m.make_rot_z(a);
        m
    }
    #[inline]
    pub fn trans(t: Vector3) -> Matrix4 {
        let mut m = Matrix4::uninit();
        m.make_trans(t);
        m
    }
    // PORT: `Matrix4::Scale(float)` and `Matrix4::Scale(const Vector3&)` are overloads;
    // the scalar one becomes scale_uniform (was: static Matrix4 Scale(float s), Vector.h:227).
    #[inline]
    pub fn scale_uniform(s: f32) -> Matrix4 {
        let mut m = Matrix4::uninit();
        m.make_scale(Vector3::splat(s));
        m
    }
    #[inline]
    pub fn scale(s: Vector3) -> Matrix4 {
        let mut m = Matrix4::uninit();
        m.make_scale(s);
        m
    }

    //Some getters
    #[inline]
    pub fn x_axis(&self) -> Vector3 {
        Vector3::new(self.m[0], self.m[4], self.m[8])
    }
    #[inline]
    pub fn y_axis(&self) -> Vector3 {
        Vector3::new(self.m[1], self.m[5], self.m[9])
    }
    #[inline]
    pub fn z_axis(&self) -> Vector3 {
        Vector3::new(self.m[2], self.m[6], self.m[10])
    }
    #[inline]
    pub fn translation(&self) -> Vector3 {
        Vector3::new(self.m[3], self.m[7], self.m[11])
    }
    // PORT: renamed from `Scale()` to avoid clashing with the static `scale()` ctor,
    // which in C++ was resolved by overloading on constness/arity (was: Vector3 Scale() const, Vector.h:243).
    #[inline]
    pub fn get_scale(&self) -> Vector3 {
        Vector3::new(self.m[0], self.m[5], self.m[10])
    }

    //Setters
    #[inline]
    pub fn set_translation(&mut self, t: Vector3) {
        self.m[3] = t.x;
        self.m[7] = t.y;
        self.m[11] = t.z;
    }
    #[inline]
    pub fn set_x_axis(&mut self, t: Vector3) {
        self.m[0] = t.x;
        self.m[4] = t.y;
        self.m[8] = t.z;
    }
    #[inline]
    pub fn set_y_axis(&mut self, t: Vector3) {
        self.m[1] = t.x;
        self.m[5] = t.y;
        self.m[9] = t.z;
    }
    #[inline]
    pub fn set_z_axis(&mut self, t: Vector3) {
        self.m[2] = t.x;
        self.m[6] = t.y;
        self.m[10] = t.z;
    }
    #[inline]
    pub fn set_scale(&mut self, s: Vector3) {
        self.m[0] = s.x;
        self.m[5] = s.y;
        self.m[10] = s.z;
    }

    //Transformations
    #[inline]
    #[rustfmt::skip]
    pub fn transposed(&self) -> Matrix4 {
        let m = &self.m;
        let mut out = Matrix4::uninit();
        out.m[0]  = m[0]; out.m[1]  = m[4]; out.m[2]  = m[8];  out.m[3]  = m[12];
        out.m[4]  = m[1]; out.m[5]  = m[5]; out.m[6]  = m[9];  out.m[7]  = m[13];
        out.m[8]  = m[2]; out.m[9]  = m[6]; out.m[10] = m[10]; out.m[11] = m[14];
        out.m[12] = m[3]; out.m[13] = m[7]; out.m[14] = m[11]; out.m[15] = m[15];
        out
    }
    #[inline]
    pub fn translate(&mut self, t: Vector3) {
        self.m[3] += t.x;
        self.m[7] += t.y;
        self.m[11] += t.z;
    }
    #[inline]
    pub fn stretch(&mut self, s: Vector3) {
        self.m[0] *= s.x;
        self.m[5] *= s.y;
        self.m[10] *= s.z;
    }

    //Multiplication
    #[inline]
    #[rustfmt::skip]
    pub fn mul_point(&self, b: Vector3) -> Vector3 {
        let m = &self.m;
        let p = Vector3::new(
            m[0] * b.x + m[1] * b.y + m[2] * b.z + m[3],
            m[4] * b.x + m[5] * b.y + m[6] * b.z + m[7],
            m[8] * b.x + m[9] * b.y + m[10] * b.z + m[11],
        );
        let w = m[12] * b.x + m[13] * b.y + m[14] * b.z + m[15];
        p / w
    }
    #[inline]
    #[rustfmt::skip]
    pub fn mul_direction(&self, b: Vector3) -> Vector3 {
        let m = &self.m;
        Vector3::new(
            m[0] * b.x + m[1] * b.y + m[2] * b.z,
            m[4] * b.x + m[5] * b.y + m[6] * b.z,
            m[8] * b.x + m[9] * b.y + m[10] * b.z,
        )
    }

    //Inverse (Vector.h:371-489)
    #[rustfmt::skip]
    pub fn inverse(&self) -> Matrix4 {
        let m = &self.m;
        let mut inv = Matrix4::uninit();

        inv.m[0] = m[5] * m[10] * m[15] -
            m[5] * m[11] * m[14] -
            m[9] * m[6] * m[15] +
            m[9] * m[7] * m[14] +
            m[13] * m[6] * m[11] -
            m[13] * m[7] * m[10];

        inv.m[4] = -m[4] * m[10] * m[15] +
            m[4] * m[11] * m[14] +
            m[8] * m[6] * m[15] -
            m[8] * m[7] * m[14] -
            m[12] * m[6] * m[11] +
            m[12] * m[7] * m[10];

        inv.m[8] = m[4] * m[9] * m[15] -
            m[4] * m[11] * m[13] -
            m[8] * m[5] * m[15] +
            m[8] * m[7] * m[13] +
            m[12] * m[5] * m[11] -
            m[12] * m[7] * m[9];

        inv.m[12] = -m[4] * m[9] * m[14] +
            m[4] * m[10] * m[13] +
            m[8] * m[5] * m[14] -
            m[8] * m[6] * m[13] -
            m[12] * m[5] * m[10] +
            m[12] * m[6] * m[9];

        inv.m[1] = -m[1] * m[10] * m[15] +
            m[1] * m[11] * m[14] +
            m[9] * m[2] * m[15] -
            m[9] * m[3] * m[14] -
            m[13] * m[2] * m[11] +
            m[13] * m[3] * m[10];

        inv.m[5] = m[0] * m[10] * m[15] -
            m[0] * m[11] * m[14] -
            m[8] * m[2] * m[15] +
            m[8] * m[3] * m[14] +
            m[12] * m[2] * m[11] -
            m[12] * m[3] * m[10];

        inv.m[9] = -m[0] * m[9] * m[15] +
            m[0] * m[11] * m[13] +
            m[8] * m[1] * m[15] -
            m[8] * m[3] * m[13] -
            m[12] * m[1] * m[11] +
            m[12] * m[3] * m[9];

        inv.m[13] = m[0] * m[9] * m[14] -
            m[0] * m[10] * m[13] -
            m[8] * m[1] * m[14] +
            m[8] * m[2] * m[13] +
            m[12] * m[1] * m[10] -
            m[12] * m[2] * m[9];

        inv.m[2] = m[1] * m[6] * m[15] -
            m[1] * m[7] * m[14] -
            m[5] * m[2] * m[15] +
            m[5] * m[3] * m[14] +
            m[13] * m[2] * m[7] -
            m[13] * m[3] * m[6];

        inv.m[6] = -m[0] * m[6] * m[15] +
            m[0] * m[7] * m[14] +
            m[4] * m[2] * m[15] -
            m[4] * m[3] * m[14] -
            m[12] * m[2] * m[7] +
            m[12] * m[3] * m[6];

        inv.m[10] = m[0] * m[5] * m[15] -
            m[0] * m[7] * m[13] -
            m[4] * m[1] * m[15] +
            m[4] * m[3] * m[13] +
            m[12] * m[1] * m[7] -
            m[12] * m[3] * m[5];

        inv.m[14] = -m[0] * m[5] * m[14] +
            m[0] * m[6] * m[13] +
            m[4] * m[1] * m[14] -
            m[4] * m[2] * m[13] -
            m[12] * m[1] * m[6] +
            m[12] * m[2] * m[5];

        inv.m[3] = -m[1] * m[6] * m[11] +
            m[1] * m[7] * m[10] +
            m[5] * m[2] * m[11] -
            m[5] * m[3] * m[10] -
            m[9] * m[2] * m[7] +
            m[9] * m[3] * m[6];

        inv.m[7] = m[0] * m[6] * m[11] -
            m[0] * m[7] * m[10] -
            m[4] * m[2] * m[11] +
            m[4] * m[3] * m[10] +
            m[8] * m[2] * m[7] -
            m[8] * m[3] * m[6];

        inv.m[11] = -m[0] * m[5] * m[11] +
            m[0] * m[7] * m[9] +
            m[4] * m[1] * m[11] -
            m[4] * m[3] * m[9] -
            m[8] * m[1] * m[7] +
            m[8] * m[3] * m[5];

        inv.m[15] = m[0] * m[5] * m[10] -
            m[0] * m[6] * m[9] -
            m[4] * m[1] * m[10] +
            m[4] * m[2] * m[9] +
            m[8] * m[1] * m[6] -
            m[8] * m[2] * m[5];

        let det = m[0] * inv.m[0] + m[1] * inv.m[4] + m[2] * inv.m[8] + m[3] * inv.m[12];
        inv /= det;
        inv
    }
}

//Basic operatios
impl Add<Matrix4> for Matrix4 {
    type Output = Matrix4;
    #[inline]
    fn add(self, b: Matrix4) -> Matrix4 {
        let mut out = Matrix4::uninit();
        for i in 0..16 {
            out.m[i] = self.m[i] + b.m[i];
        }
        out
    }
}
impl Sub<Matrix4> for Matrix4 {
    type Output = Matrix4;
    #[inline]
    fn sub(self, b: Matrix4) -> Matrix4 {
        let mut out = Matrix4::uninit();
        for i in 0..16 {
            out.m[i] = self.m[i] - b.m[i];
        }
        out
    }
}
impl AddAssign<Matrix4> for Matrix4 {
    #[inline]
    fn add_assign(&mut self, b: Matrix4) {
        for i in 0..16 {
            self.m[i] += b.m[i];
        }
    }
}
impl SubAssign<Matrix4> for Matrix4 {
    #[inline]
    fn sub_assign(&mut self, b: Matrix4) {
        for i in 0..16 {
            self.m[i] -= b.m[i];
        }
    }
}
impl MulAssign<f32> for Matrix4 {
    #[inline]
    fn mul_assign(&mut self, b: f32) {
        for i in 0..16 {
            self.m[i] *= b;
        }
    }
}
impl DivAssign<f32> for Matrix4 {
    #[inline]
    fn div_assign(&mut self, b: f32) {
        *self *= 1.0 / b;
    }
}

//Multiplication (Vector.h:319-341)
impl Mul<Matrix4> for Matrix4 {
    type Output = Matrix4;
    #[rustfmt::skip]
    fn mul(self, b: Matrix4) -> Matrix4 {
        let m = &self.m;
        let b = &b.m;
        let mut out = Matrix4::uninit();
        out.m[0]  = b[0]*m[0]  + b[4]*m[1]  + b[8] *m[2]  + b[12]*m[3];
        out.m[1]  = b[1]*m[0]  + b[5]*m[1]  + b[9] *m[2]  + b[13]*m[3];
        out.m[2]  = b[2]*m[0]  + b[6]*m[1]  + b[10]*m[2]  + b[14]*m[3];
        out.m[3]  = b[3]*m[0]  + b[7]*m[1]  + b[11]*m[2]  + b[15]*m[3];

        out.m[4]  = b[0]*m[4]  + b[4]*m[5]  + b[8] *m[6]  + b[12]*m[7];
        out.m[5]  = b[1]*m[4]  + b[5]*m[5]  + b[9] *m[6]  + b[13]*m[7];
        out.m[6]  = b[2]*m[4]  + b[6]*m[5]  + b[10]*m[6]  + b[14]*m[7];
        out.m[7]  = b[3]*m[4]  + b[7]*m[5]  + b[11]*m[6]  + b[15]*m[7];

        out.m[8]  = b[0]*m[8]  + b[4]*m[9]  + b[8] *m[10] + b[12]*m[11];
        out.m[9]  = b[1]*m[8]  + b[5]*m[9]  + b[9] *m[10] + b[13]*m[11];
        out.m[10] = b[2]*m[8]  + b[6]*m[9]  + b[10]*m[10] + b[14]*m[11];
        out.m[11] = b[3]*m[8]  + b[7]*m[9]  + b[11]*m[10] + b[15]*m[11];

        out.m[12] = b[0]*m[12] + b[4]*m[13] + b[8] *m[14] + b[12]*m[15];
        out.m[13] = b[1]*m[12] + b[5]*m[13] + b[9] *m[14] + b[13]*m[15];
        out.m[14] = b[2]*m[12] + b[6]*m[13] + b[10]*m[14] + b[14]*m[15];
        out.m[15] = b[3]*m[12] + b[7]*m[13] + b[11]*m[14] + b[15]*m[15];
        out
    }
}
impl MulAssign<Matrix4> for Matrix4 {
    #[inline]
    fn mul_assign(&mut self, b: Matrix4) {
        *self = *self * b;
    }
}
impl Mul<Vector4> for Matrix4 {
    type Output = Vector4;
    #[inline]
    #[rustfmt::skip]
    fn mul(self, b: Vector4) -> Vector4 {
        let m = &self.m;
        Vector4::new(
            m[0]*b.x  + m[1]*b.y  + m[2]*b.z  + m[3]*b.w,
            m[4]*b.x  + m[5]*b.y  + m[6]*b.z  + m[7]*b.w,
            m[8]*b.x  + m[9]*b.y  + m[10]*b.z + m[11]*b.w,
            m[12]*b.x + m[13]*b.y + m[14]*b.z + m[15]*b.w,
        )
    }
}

//=============================================================================
// Tests -- not part of the C++ port; they pin down the row-major conventions.
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    const TOL: f32 = 1e-5;

    fn approx_eq_f(a: f32, b: f32) -> bool {
        (a - b).abs() <= TOL
    }
    fn approx_eq_v3(a: Vector3, b: Vector3) -> bool {
        approx_eq_f(a.x, b.x) && approx_eq_f(a.y, b.y) && approx_eq_f(a.z, b.z)
    }
    fn approx_eq_m4(a: &Matrix4, b: &Matrix4) -> bool {
        (0..16).all(|i| approx_eq_f(a.m[i], b.m[i]))
    }

    fn sample() -> Matrix4 {
        // trans * rot_y * rot_x * scale -- a non-trivial invertible transform.
        Matrix4::trans(Vector3::new(1.5, -2.25, 3.0))
            * Matrix4::rot_y(0.7)
            * Matrix4::rot_x(-0.4)
            * Matrix4::scale(Vector3::new(2.0, 0.5, 1.75))
    }

    #[test]
    fn identity_is_neutral() {
        let m = sample();
        assert!(approx_eq_m4(&(Matrix4::identity() * m), &m));
        assert!(approx_eq_m4(&(m * Matrix4::identity()), &m));
    }

    #[test]
    fn inverse_times_self_is_identity() {
        let m = sample();
        assert!(approx_eq_m4(&(m.inverse() * m), &Matrix4::identity()));
        assert!(approx_eq_m4(&(m * m.inverse()), &Matrix4::identity()));
    }

    #[test]
    fn trans_mul_point_offsets() {
        let t = Vector3::new(3.0, -4.0, 5.5);
        let p = Vector3::new(0.25, 1.0, -2.0);
        assert!(approx_eq_v3(Matrix4::trans(t).mul_point(p), p + t));
    }

    #[test]
    fn rot_y_quarter_turn_maps_x_to_minus_z() {
        // MakeRotY (Vector.h:195-200): m[0]=cos, m[2]=sin, m[8]=-sin, m[10]=cos.
        // MulDirection reads rows (m[0],m[1],m[2]) / (m[4],m[5],m[6]) / (m[8],m[9],m[10]),
        // so unit_x -> (cos a, 0, -sin a) -> (0, 0, -1) at a = pi/2.
        let r = Matrix4::rot_y(std::f32::consts::PI / 2.0);
        assert!(approx_eq_v3(r.mul_direction(Vector3::unit_x()), -Vector3::unit_z()));
    }

    #[test]
    fn transpose_reverses_product() {
        let a = sample();
        let b = Matrix4::rot_z(1.1) * Matrix4::trans(Vector3::new(-1.0, 2.0, 0.5));
        assert!(approx_eq_m4(&(a * b).transposed(), &(b.transposed() * a.transposed())));
    }

    #[test]
    fn translation_and_scale_roundtrip() {
        let v = Vector3::new(-7.0, 0.5, 12.25);
        assert!(approx_eq_v3(Matrix4::trans(v).translation(), v));
        let s = Vector3::new(2.0, 3.0, 4.0);
        assert!(approx_eq_v3(Matrix4::scale(s).get_scale(), s));
    }
}
