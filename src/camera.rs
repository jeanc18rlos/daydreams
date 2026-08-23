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
        // so an effect can animate it -- the sprint's field-of-view kick (ext/sprint.rs) does,
        // through `ext::view::set_fov`. `ext::view::fov()` equals GH_FOV whenever no effect is
        // active, so ported behaviour is unchanged.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_m(a: &Matrix4, b: &Matrix4, tol: f32) -> bool {
        a.m.iter().zip(b.m.iter()).all(|(x, y)| (x - y).abs() < tol)
    }

    #[test]
    fn inverse_projection_is_the_full_inverse_of_projection() {
        for (w, h, n, f) in [
            (1280, 720, 0.001, 100.0),
            (256, 256, 0.1, 10.0),
            (2560, 1440, 0.5, 1000.0),
            (640, 480, 1.0, 2.0),
        ] {
            let mut cam = Camera::new();
            cam.set_size(w, h, n, f);
            let closed = cam.inverse_projection();
            let brute = cam.projection.inverse();
            // The closed form fills five entries; everything else must be zero in both.
            assert!(
                approx_m(
                    &closed,
                    &brute,
                    1e-4 * brute.m.iter().fold(1.0f32, |a, &b| a.max(b.abs()))
                ),
                "{w}x{h} n={n} f={f}\n{:?}\n{:?}",
                closed.m,
                brute.m
            );
            assert!(approx_m(&(cam.projection * closed), &Matrix4::identity(), 1e-4));
            assert!(approx_m(&(closed * cam.projection), &Matrix4::identity(), 1e-4));
        }
    }

    #[test]
    fn set_size_maps_near_and_far_to_the_ndc_ends() {
        let mut cam = Camera::new();
        cam.set_size(800, 600, 0.25, 50.0);
        let ndc = |z: f32| cam.projection * Vector4::new(0.0, 0.0, z, 1.0);
        let near = ndc(-0.25);
        let far = ndc(-50.0);
        assert!((near.z / near.w + 1.0).abs() < 1e-5, "near -> {}", near.z / near.w);
        assert!((far.z / far.w - 1.0).abs() < 1e-4, "far -> {}", far.z / far.w);
        assert!(cam.width == 800 && cam.height == 600 && cam.near == 0.25 && cam.far == 50.0);
    }

    #[test]
    fn clip_oblique_puts_the_given_plane_at_the_near_end() {
        let mut cam = Camera::new();
        cam.set_size(1280, 720, 0.001, 100.0);
        cam.set_position_orientation(Vector3::new(1.0, 1.5, 4.0), 0.1, 0.7);
        let pos = Vector3::new(0.5, 1.0, -2.0);
        let normal = Vector3::new(0.3, 0.1, 1.0).normalized();
        cam.clip_oblique(pos, normal);
        let clip = |p: Vector3| cam.matrix() * Vector4::from_vec3(p, 1.0);
        // The point itself, and any other point of the plane, is at z = -w: the near end.
        let c = clip(pos);
        assert!((c.z + c.w).abs() < 1e-3 * c.w.abs(), "{:?}", c);
        let along = Vector3::new(1.0, 0.0, -0.3); // perpendicular to the normal
        let c = clip(pos + along * 0.8);
        assert!((c.z + c.w).abs() < 1e-3 * c.w.abs(), "{:?}", c);
        // The normal points at the half-space that is cut (z < -w) and the other side is kept:
        // Portal::draw hands in the normal facing back at the camera, so the nested pass keeps
        // what lies beyond the portal and loses the camera's own side.
        let c = clip(pos + normal * 0.5);
        assert!(c.z < -c.w, "on the normal's side: {c:?}");
        let c = clip(pos - normal * 0.5);
        assert!(c.z > -c.w, "beyond the plane: {c:?}");
        // Only the third row changes.
        let mut plain = Camera::new();
        plain.set_size(1280, 720, 0.001, 100.0);
        for i in (0..16).filter(|i| !(8..12).contains(i)) {
            assert_eq!(cam.projection.m[i], plain.projection.m[i], "m[{i}]");
        }
    }
}
