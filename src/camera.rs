// Port of Camera.h / Camera.cpp

use crate::game_header::GH_PI;
use crate::vector::{Matrix4, Vector3, Vector4};

#[derive(Clone, Copy)]
pub struct Camera {
    pub projection: Matrix4,
    pub world_view: Matrix4,

    pub width: i32,
    pub height: i32,
    pub near: f32,
    pub far: f32,
}

// Preserved verbatim from the C++ API surface; not every member is reachable from the
// scenes this port ships. Kept for fidelity rather than deleted.
#[allow(dead_code)]
impl Camera {
    // Camera::Camera()   (Camera.cpp:6-11)
    pub fn new() -> Camera {
        Camera {
            // PORT: near/far zero-initialized; the C++ constructor leaves them uninitialized
            // (was: Camera::Camera() : width(256), height(256) { ... }, Camera.cpp:6-8)
            near: 0.0,
            far: 0.0,
            width: 256,
            height: 256,
            world_view: Matrix4::identity(), // worldView.MakeIdentity();
            projection: Matrix4::identity(), // projection.MakeIdentity();
        }
    }

    // Camera::SetSize(int w, int h, float n, float f)   (Camera.cpp:13-39)
    pub fn set_size(&mut self, w: i32, h: i32, n: f32, f: f32) {
        self.width = w;
        self.height = h;
        self.near = n;
        self.far = f;

        // EXT: was `GH_FOV` (Camera.cpp:19), a compile-time constant. Read at runtime instead
        // so the dolly-zoom effect can animate it; `ext::view::fov()` returns GH_FOV unless a
        // scene overrides it, so ported behaviour is unchanged.
        let e: f32 = 1.0 / (crate::ext::view::fov() * GH_PI / 360.0).tan();
        let a: f32 = (self.height as f32) / (self.width as f32);
        let d: f32 = self.near - self.far;

        self.projection.m[0] = e * a;
        self.projection.m[1] = 0.0;
        self.projection.m[2] = 0.0;
        self.projection.m[3] = 0.0;
        self.projection.m[4] = 0.0;
        self.projection.m[5] = e;
        self.projection.m[6] = 0.0;
        self.projection.m[7] = 0.0;
        self.projection.m[8] = 0.0;
        self.projection.m[9] = 0.0;
        self.projection.m[10] = (self.near + self.far) / d;
        self.projection.m[11] = (2.0 * self.near * self.far) / d;
        self.projection.m[12] = 0.0;
        self.projection.m[13] = 0.0;
        self.projection.m[14] = -1.0;
        self.projection.m[15] = 0.0;
    }

    // Camera::SetPositionOrientation(const Vector3& pos, float rotX, float rotY)   (Camera.cpp:41-43)
    pub fn set_position_orientation(&mut self, pos: Vector3, rot_x: f32, rot_y: f32) {
        self.world_view = Matrix4::rot_x(rot_x) * Matrix4::rot_y(rot_y) * Matrix4::trans(-pos);
    }

    // Camera::InverseProjection() const   (Camera.cpp:45-58)
    pub fn inverse_projection(&self) -> Matrix4 {
        let mut inv_projection = Matrix4::zero();
        let a: f32 = self.projection.m[0];
        let b: f32 = self.projection.m[5];
        let c: f32 = self.projection.m[10];
        let d: f32 = self.projection.m[11];
        let e: f32 = self.projection.m[14];
        inv_projection.m[0] = 1.0 / a;
        inv_projection.m[5] = 1.0 / b;
        inv_projection.m[11] = 1.0 / e;
        inv_projection.m[14] = 1.0 / d;
        inv_projection.m[15] = -c / (d * e);
        inv_projection
    }

    // Camera::Matrix() const   (Camera.cpp:60-62)
    pub fn matrix(&self) -> Matrix4 {
        self.projection * self.world_view
    }

    // Camera::UseViewport() const   (Camera.cpp:64-66)
    pub fn use_viewport(&self, gl: &glow::Context) {
        use glow::HasContext;
        // PORT: glow's viewport is unsafe and takes the context explicitly
        // (was: glViewport(0, 0, width, height), Camera.cpp:65)
        unsafe {
            gl.viewport(0, 0, self.width, self.height);
        }
    }

    // Camera::ClipOblique(const Vector3& pos, const Vector3& normal)   (Camera.cpp:68-85)
    pub fn clip_oblique(&mut self, pos: Vector3, normal: Vector3) {
        let cpos: Vector3 = (self.world_view * Vector4::from_vec3(pos, 1.0)).xyz();
        let cnormal: Vector3 = (self.world_view * Vector4::from_vec3(normal, 0.0)).xyz();
        let cplane = Vector4::new(cnormal.x, cnormal.y, cnormal.z, -cpos.dot(cnormal));

        let q: Vector4 = self.projection.inverse()
            * Vector4::new(
                if cplane.x < 0.0 { 1.0 } else { -1.0 },
                if cplane.y < 0.0 { 1.0 } else { -1.0 },
                1.0,
                1.0,
            );
        let c: Vector4 = cplane * (2.0 / cplane.dot(q));

        self.projection.m[8] = c.x - self.projection.m[12];
        self.projection.m[9] = c.y - self.projection.m[13];
        self.projection.m[10] = c.z - self.projection.m[14];
        self.projection.m[11] = c.w - self.projection.m[15];
    }
}

impl Default for Camera {
    fn default() -> Camera {
        Camera::new()
    }
}
