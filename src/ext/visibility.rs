//! EXT: "is the player looking at this?" Not part of the C++ port.
//!
//! Powers the observation-dependent geometry in scene `-` (Level10), where statues may only move
//! while unobserved.
//!
//! The engine does run GL occlusion queries every frame, but only for portals, and only to prune
//! recursion (`Engine::Render`, Engine.cpp:233-251). Reusing that would mean threading new
//! queries through the ported render path. Doing it analytically instead keeps the whole
//! mechanic inside `ext/` and has a useful property besides: the answer is available during
//! `update`, a frame *before* the render pass could report it.
//!
//! Two tests, both cheap:
//!   1. a view-cone test -- is the point within the player's field of view at all;
//!   2. a line-of-sight raycast -- is anything solid between the eye and the point.

use crate::ext::raycast::raycast;
use crate::object::ObjectT;
use crate::vector::{Matrix4, Vector3};
use std::cell::RefCell;
use std::rc::Rc;

/// Half-angle of the "being watched" cone, in radians.
///
/// Deliberately wider than the render frustum's half-FOV (`GH_FOV` is 60 degrees, so 30 degrees
/// vertical): peripheral vision should count as watching. A statue that creeps forward while
/// technically just off-screen reads as a bug, not as a mechanic.
pub const WATCH_HALF_ANGLE: f32 = 1.05; // ~60 degrees

/// Is `point` inside the player's view cone?
pub fn in_view_cone(cam_to_world: &Matrix4, point: Vector3, half_angle: f32) -> bool {
    let eye = cam_to_world.translation();
    // The camera looks down its own -Z (Object.cpp:33-35).
    let forward = cam_to_world.mul_direction(Vector3::new(0.0, 0.0, -1.0)).normalized_safe();
    let to_point = point - eye;
    let dist = to_point.mag();
    if dist < 1e-4 {
        return true; // standing inside it
    }
    let cos_angle = to_point.dot(forward) / dist;
    cos_angle >= half_angle.cos()
}

/// Is the straight line from the player's eye to `point` clear of level geometry?
///
/// `skip` should be the index of the object being tested, so it does not occlude itself.
pub fn has_line_of_sight(
    objects: &[Rc<RefCell<dyn ObjectT>>],
    cam_to_world: &Matrix4,
    point: Vector3,
    skip: Option<usize>,
) -> bool {
    let eye = cam_to_world.translation();
    let delta = point - eye;
    let dist = delta.mag();
    if dist < 1e-4 {
        return true;
    }
    let dir = delta / dist;
    // Stop just short of the target so its own surface does not count as a blocker.
    raycast(objects, eye, dir, dist - 0.05, skip).is_none()
}

/// Full "is it being watched" test: inside the cone *and* not hidden behind something.
pub fn is_observed(
    objects: &[Rc<RefCell<dyn ObjectT>>],
    cam_to_world: &Matrix4,
    point: Vector3,
    skip: Option<usize>,
) -> bool {
    in_view_cone(cam_to_world, point, WATCH_HALF_ANGLE)
        && has_line_of_sight(objects, cam_to_world, point, skip)
}

/// EXT-pivot: the multi-viewer form of [`is_observed`] -- watched if ANY of the given eyes
/// observes the point.
///
/// Hide 'N Dream's core query: a hidden dreamer may act only while unobserved by *every*
/// seeker (the Chameleon Rule, docs/hide-n-dream.md). With one camera this is exactly
/// [`is_observed`]; netplay hands it every seeker's reconstructed eye.
pub fn is_observed_by_any(
    objects: &[Rc<RefCell<dyn ObjectT>>],
    cams: &[Matrix4],
    point: Vector3,
    skip: Option<usize>,
) -> bool {
    cams.iter().any(|cam| is_observed(objects, cam, point, skip))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::Matrix4;

    /// Identity camera sits at the origin looking down -Z.
    #[test]
    fn point_straight_ahead_is_in_cone() {
        let cam = Matrix4::identity();
        assert!(in_view_cone(&cam, Vector3::new(0.0, 0.0, -10.0), WATCH_HALF_ANGLE));
    }

    #[test]
    fn point_behind_is_not_in_cone() {
        let cam = Matrix4::identity();
        assert!(!in_view_cone(&cam, Vector3::new(0.0, 0.0, 10.0), WATCH_HALF_ANGLE));
    }

    #[test]
    fn point_far_to_the_side_is_not_in_cone() {
        let cam = Matrix4::identity();
        // 90 degrees off-axis is outside a 60 degree half-angle.
        assert!(!in_view_cone(&cam, Vector3::new(10.0, 0.0, 0.0), WATCH_HALF_ANGLE));
    }

    #[test]
    fn cone_follows_the_camera() {
        // Turn the camera 180 degrees; what was behind is now ahead.
        let cam = Matrix4::rot_y(std::f32::consts::PI);
        assert!(in_view_cone(&cam, Vector3::new(0.0, 0.0, 10.0), WATCH_HALF_ANGLE));
        assert!(!in_view_cone(&cam, Vector3::new(0.0, 0.0, -10.0), WATCH_HALF_ANGLE));
    }

    /// Two eyes facing opposite ways leave nowhere on the axis unwatched.
    #[test]
    fn any_of_two_opposed_eyes_sees_both_sides() {
        let ahead = Matrix4::identity();
        let behind = Matrix4::rot_y(std::f32::consts::PI);
        let both = [ahead, behind];
        // No blockers: an empty object list means line of sight always passes.
        let none: Vec<Rc<RefCell<dyn ObjectT>>> = Vec::new();
        assert!(is_observed_by_any(&none, &both, Vector3::new(0.0, 0.0, -10.0), None));
        assert!(is_observed_by_any(&none, &both, Vector3::new(0.0, 0.0, 10.0), None));
        assert!(!is_observed_by_any(&none, &both[..1], Vector3::new(0.0, 0.0, 10.0), None));
        assert!(!is_observed_by_any(&none, &[], Vector3::new(0.0, 0.0, -10.0), None));
    }

    #[test]
    fn camera_position_is_respected() {
        // Camera pushed to z=+50 still looks down -Z, so the origin is ahead of it.
        let cam = Matrix4::trans(Vector3::new(0.0, 0.0, 50.0));
        assert!(in_view_cone(&cam, Vector3::zero(), WATCH_HALF_ANGLE));
        assert!(!in_view_cone(&cam, Vector3::new(0.0, 0.0, 100.0), WATCH_HALF_ANGLE));
    }
}
