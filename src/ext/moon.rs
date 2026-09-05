//! EXT: a moon in an otherwise empty sky. Not part of the C++ port.
//!
//! luckass333's "Moon" (`Meshes/moon.glb`, CC-BY-4.0; licence beside it, credit in
//! `THIRD_PARTY.md`): a 960-triangle sphere carrying one 1024x512 photograph of the near side,
//! used as its base colour map and again as its emissive one at
//! `KHR_materials_emissive_strength` 1.9. That emissive term is the whole reason nothing here
//! has to touch a shader: `Shaders/gltfpbr.frag` adds it after the interior hemisphere, so the
//! sphere lights itself, comes out pale on black, and does not care that the scene it stands in
//! has no sun and no sky (`level31.rs`).
//!
//! # Why it follows the eye
//!
//! The engine's far plane is 100 units (`GH_FAR`) and the Liminal Neighborhood is 500 m across,
//! so a moon left at a fixed world point would swell, swing and then vanish as the player walked
//! the street. Instead its centre is held at a constant offset from the eye -- the skybox trick,
//! and the honest one for a body at infinity: fixed distance, fixed direction, no parallax,
//! never clipped. What it is NOT is a skybox: it is an ordinary opaque object in the depth
//! buffer, so it is [`ELEVATION`] degrees up precisely so that nothing can ever be between the
//! eye and it. At that angle the highest roof in the file passes 20 m underneath.
//!
//! The eye it follows is **this render pass's** (`RenderCtx::eye`), decided at draw time rather
//! than kept as a position and stepped. That distinction is the whole of the level's portals
//! (`level31.rs`): a portal pass renders from a camera 50 m along the street from the real one,
//! and a moon standing at a world position would be seen from there 50 m off -- 32 degrees at
//! this distance -- so the sky through the portal would not be the sky beside it, and the moon
//! would jump as the player stepped through. Hung off whichever eye is drawing, it is in the
//! same place in every pass, which is what a body at infinity does. It also means the moon has
//! no state and no `update`: nothing to step, and nothing to be stale on a headless frame that
//! runs no fixed steps at all.
//!
//! # Size and haze
//!
//! Two numbers set the look and they are not independent, because the interior grade's distance
//! haze (`Shaders/gltfpbr.frag`: `1 - exp(-dist * 0.008)`, mixed 0.9 of the way toward a dark
//! warm tone) is what turns a white sphere pale. [`DISTANCE`] chooses how much of that it gets
//! -- a constant amount, the distance never changing -- and the model's own 4.43-unit radius
//! then fixes the angular size at [`APPARENT_DIAMETER`]. Roughly twelve times the real moon's
//! half-degree: stylised, as every moon in every game is, and small enough to read as sky
//! rather than as an approaching planet.

use std::rc::Rc;

use crate::camera::Camera;
use crate::ext::gltf_model::{Anchor, Fit, Frame, GltfModel, Load, PartSpec};
use crate::ext::view;
use crate::object::{Object, ObjectT, RenderCtx};
use crate::resources::Resources;
use crate::shader::Shader;
use crate::vector::Vector3;

const MODEL: &str = "Meshes/moon.glb";
const PART: &str = "moon";
/// The file's one map is 1024x512: nothing is resampled.
const MAP: u32 = 1024;

const PARTS: [PartSpec<'static>; 1] =
    [PartSpec { name: PART, roots: &[], skip: &[], frame: Frame::Scene, anchor: Anchor::Hinge }];

fn load_spec() -> Load<'static> {
    Load {
        path: MODEL,
        parts: &PARTS,
        fit: Fit::Identity,
        max_map: MAP,
        translucent: &[],
        metallic_override: &[],
        cut_boxes: &[],
    }
}

/// Where the sphere's centre sits in the file, and how big it is: the exporter hung it at
/// y = 2 under a node scaled 4.4312, and the two rotations above that cancel. A test reads
/// both back out of the GLB, because everything below is measured from them.
const MODEL_CENTRE: Vector3 = Vector3 { x: 0.0, y: 2.0, z: 0.0 };
const MODEL_RADIUS: f32 = 4.4312;

/// How far from the eye the moon hangs. Inside the far plane with the sphere's own radius and
/// a margin to spare (`GH_FAR` is 100), and far enough back that the haze has made it pale.
const DISTANCE: f32 = 80.0;
/// Degrees above the horizon. High enough that no roof, tree or power line in the file can pass
/// in front of it -- at this angle the moon is 30 m up and the tallest thing in the
/// neighbourhood is 10 -- which is what lets an ordinary depth-tested object stand in for a sky.
const ELEVATION: f32 = 22.0;
/// Degrees off the scene's default heading (-z), negative to the left of it. Left of centre
/// rather than on it: the spawn's own frame has the van and a street light to the right
/// (`level31.rs`), and the moon balances them instead of sitting on the crosshair.
const AZIMUTH: f32 = -28.0;

/// What the moon subtends at [`DISTANCE`], in degrees. Derived, not tuned: the model's radius
/// and the distance decide it, and a test holds it to the value the docs quote.
pub const APPARENT_DIAMETER: f32 = 6.34;

/// The moon's centre relative to the eye: [`DISTANCE`] along the direction [`ELEVATION`] and
/// [`AZIMUTH`] pick out, in a world whose default heading is -z.
fn offset() -> Vector3 {
    let (e, a) = (ELEVATION.to_radians(), AZIMUTH.to_radians());
    // forward = -z, right = +x, up = +y.
    Vector3::new(a.sin() * e.cos(), e.sin(), -a.cos() * e.cos()) * DISTANCE
}

/// Where the model's ORIGIN goes for an eye at `eye`: the sphere's centre [`offset`] away,
/// less the [`MODEL_CENTRE`] the exporter left between the two.
fn origin_for(eye: Vector3) -> Vector3 {
    eye + offset() - MODEL_CENTRE
}

/// A moon that keeps station on the eye (see the module docs). No collider, no state, no
/// `update`: it asks nothing of the scene but a place in the object vector, and it is drawn
/// wherever the pass that is drawing it happens to be looking from.
pub struct Moon {
    /// Never moved. `ObjectT` wants one, and nothing reads it: the sphere is placed at draw
    /// time from `RenderCtx::eye`, not from here.
    base: Object,
    model: Rc<GltfModel>,
    /// The material is PBR (`unlit=false`), and its emissive term is what makes the moon a
    /// moon; `gltfpbr` is the shader that reads it.
    shader: Rc<Shader>,
}

impl Moon {
    pub fn new(gl: &Rc<glow::Context>, res: &Resources) -> Moon {
        log::info!(
            "[moon] {APPARENT_DIAMETER:.2} deg across, {DISTANCE} out at {ELEVATION} deg up \
             ({:.0} m over the eye, radius {MODEL_RADIUS})",
            offset().y
        );
        Moon {
            base: Object::new(),
            model: GltfModel::acquire(gl, &load_spec()),
            shader: res.acquire_shader("gltfpbr"),
        }
    }
}

impl ObjectT for Moon {
    fn base(&self) -> &Object {
        &self.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base
    }

    fn draw(&self, ctx: &RenderCtx, cam: &Camera, _fbo: Option<glow::Framebuffer>) {
        // THIS pass's eye, not the player's: see the module docs, and `level31.rs`'s portals.
        let mut obj = Object::new();
        obj.pos = origin_for(ctx.eye);
        // The moon is not in the air, so it does not get the air's fog. A level that hides its
        // draw distance behind a curtain (`view::Fog`) would otherwise put the curtain in front
        // of the moon as well -- the sphere stands [`DISTANCE`] out, which is exactly where such
        // a fog is closing -- and erase the one thing in the sky. Dropped for this draw only,
        // and put back: it leaves the moon on the engine's own thin haze, which is what dims it
        // to a pale disc rather than a white hole.
        let scene_fog = view::fog();
        view::clear_fog();
        self.model.draw_part(PART, &obj, &self.shader, cam, ctx);
        if let Some(fog) = scene_fog {
            view::set_fog(fog);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ext::gltf_model::GltfModel;
    use crate::game_header::GH_FAR;

    /// The two numbers everything else is measured from, read back out of the file.
    #[test]
    fn the_file_is_the_sphere_the_constants_describe() {
        let b = GltfModel::probe_bounds(&load_spec(), PART);
        let (lo, hi) = (Vector3::new(b[0], b[2], b[4]), Vector3::new(b[1], b[3], b[5]));
        let centre = (lo + hi) * 0.5;
        assert!((centre - MODEL_CENTRE).mag() < 0.01, "centred at {centre:?}");
        for r in [hi.x - centre.x, hi.y - centre.y, hi.z - centre.z] {
            assert!((r - MODEL_RADIUS).abs() < 0.01, "radius {r}, not {MODEL_RADIUS}");
        }
    }

    /// The whole sphere is inside the far plane, with room left over: past it the moon would
    /// be clipped away rather than drawn, which is the failure the eye-following is for.
    #[test]
    fn the_moon_fits_inside_the_far_plane() {
        const { assert!(DISTANCE + MODEL_RADIUS < GH_FAR * 0.9, "the moon is past the far plane") };
        assert!(offset().mag() - DISTANCE < 1e-3, "the offset is not DISTANCE long");
    }

    /// The size the docs quote is the size the constants give.
    #[test]
    fn apparent_diameter_is_what_the_docs_say() {
        let subtended = 2.0 * (MODEL_RADIUS / DISTANCE).atan().to_degrees();
        assert!((subtended - APPARENT_DIAMETER).abs() < 0.02, "{subtended} degrees");
        // Bigger than the real moon's half degree, and nowhere near a planet filling the sky.
        assert!((3.0..=12.0).contains(&subtended));
    }

    /// High enough that nothing in the neighbourhood can pass in front of it -- the reason a
    /// plain depth-tested object can stand in for a sky at all. The tallest thing in
    /// `Meshes/abandoned_house.glb` is the house's ridge at 10 m over the road.
    #[test]
    fn the_moon_clears_every_roof_in_the_neighbourhood() {
        let up = offset().y - MODEL_RADIUS;
        assert!(up > 20.0, "the moon's lowest edge is only {up} m up");
    }

    /// The offset is measured to the sphere's CENTRE, not to the model's origin, which the
    /// exporter left two units under it. Miss this and the moon hangs two metres low -- which
    /// at this distance is nothing, and is exactly why it would never have been noticed.
    #[test]
    fn the_offset_is_measured_to_the_centre_not_the_origin() {
        for eye in [Vector3::zero(), Vector3::new(3.0, 1.5, -7.0), Vector3::new(-90.0, 2.3, 40.0)] {
            let centre = origin_for(eye) + MODEL_CENTRE;
            assert!((centre - eye - offset()).mag() < 1e-4, "centre at {centre:?} for {eye:?}");
            // ...and the direction and distance from the eye are the ones asked for, wherever
            // the player is standing.
            let d = centre - eye;
            assert!((d.mag() - DISTANCE).abs() < 1e-3, "{} from the eye", d.mag());
            let elevation = (d.y / d.mag()).asin().to_degrees();
            assert!((elevation - ELEVATION).abs() < 1e-3, "{elevation} degrees up");
        }
    }
}
