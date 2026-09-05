//! EXT: runtime field of view, for the dolly-zoom effect. Not part of the C++ port.
//!
//! `GH_FOV` is a compile-time constant read directly by `Camera::SetSize` (Camera.cpp:19), so the
//! original engine has no way to change it. The dolly zoom -- Hitchcock's vertigo shot, where
//! the FOV widens as you move forward so the corridor appears to stretch away from you -- needs
//! it to be animatable.
//!
//! A thread-local `Cell` rather than a field threaded through the camera: `Camera` is `Copy` and
//! gets duplicated per portal recursion level (`Portal::Draw`, Portal.cpp:35), and a per-instance
//! FOV would have to be propagated through every one of those copies to stay consistent. A single
//! ambient value is both simpler and more correct here -- every camera in a frame, including all
//! the recursive portal cameras, must agree on the projection or the portal views would not line
//! up with the geometry around them.
//!
//! The engine is single-threaded (the ported design uses `Rc`/`RefCell` throughout), so a
//! thread-local is exactly as shared as it needs to be.

use crate::game_header::GH_FOV;
use std::cell::Cell;

thread_local! {
    static FOV: Cell<f32> = const { Cell::new(GH_FOV) };
    /// EXT: seconds since the engine started, published once per rendered frame so shaders
    /// can animate (cloud drift, grass wind) without any per-object plumbing.
    static TIME: Cell<f32> = const { Cell::new(0.0) };
}

/// EXT: scenes that want two worlds with different weather put the second one beyond this x.
/// Every shader that cares (sky, grass, sea) receives `mood` = 0 (storm) on the near side and
/// the scene's far mood -- [`MOOD_SUNSET`] unless it says otherwise -- beyond it, based on
/// where the CAMERA of the current render pass is: a portal pass looking into the far region
/// grades itself as that world while the main pass stays stormy. Same baked clouds, two colour
/// grades, no extra cost. Scenes that never cross the line never see the far mood.
pub const MOOD_SPLIT_X: f32 = 250.0;
/// The far moods a scene can choose from. Sunset is the intro's sea; interior is the
/// Backrooms, whose "sky" is the black of an unlit building beyond its walls and whose door
/// paint must stay the colour it is -- nothing outdoors grades it.
pub const MOOD_SUNSET: f32 = 1.0;
pub const MOOD_INTERIOR: f32 = 2.0;
/// EXT: the near side's alternative to storm -- the same heavy overcast, at the end of the day
/// rather than in the middle of bad weather. Violet-blue overhead, the cloud bases catching
/// what is left of the sun near the horizon, and a meadow still lit coldly under it: all the
/// warmth in the frame comes out of the door (`ext/doorlight.rs`), which is the whole point of
/// the shot. The Backrooms wants its meadow grey and keeps it; only the intro asks for this.
pub const MOOD_DUSK: f32 = 3.0;
// Whether the current scene uses the split at all and what lies past it (both set by the
// scene on load, both reset by every load), and the door's warm light pool as
// (x, y, z, intensity).
thread_local! {
    static MOOD_ENABLED: Cell<bool> = const { Cell::new(false) };
    static FAR_MOOD: Cell<f32> = const { Cell::new(MOOD_SUNSET) };
    /// What a pass on THIS side of the split grades itself as. Storm unless a scene says
    /// otherwise; the intro says [`MOOD_DUSK`].
    static NEAR_MOOD: Cell<f32> = const { Cell::new(0.0) };
    /// One mood for every eye, split or no split: a scene that is all one world -- a level
    /// that IS an interior, with no meadow on the near side of anything -- sets it, and
    /// `mood_for` answers it without looking at the eye. `None` is the split logic below.
    static SCENE_MOOD: Cell<Option<f32>> = const { Cell::new(None) };
    static GLOW: Cell<[f32; 4]> = const { Cell::new([0.0; 4]) };
    /// EXT: the flashlight's cone (`ext/tool.rs`), as three vec4s a shader can take whole.
    ///
    /// `GLOW` is a point pool -- it lights a patch wherever the beam lands, which reads as a
    /// stain on a wall rather than a torch. These carry the cone itself: where it starts,
    /// which way it points, how far it reaches and how sharp its edge is. Off is `range == 0`,
    /// which is also the value GL hands a shader nobody wrote to, so a program that declares
    /// the uniforms and is drawn by a path that never sets them is dark rather than lit.
    static SPOT_POS: Cell<[f32; 4]> = const { Cell::new([0.0; 4]) };
    static SPOT_DIR: Cell<[f32; 4]> = const { Cell::new([0.0; 4]) };
    static SPOT_COL: Cell<[f32; 4]> = const { Cell::new([0.0; 4]) };
    /// 1.0 while drawing the main view, 0.0 inside a portal's framebuffer.
    static DETAIL: Cell<f32> = const { Cell::new(1.0) };
    /// EXT: whether this scene's portal passes draw at full detail (see
    /// [`set_seamless_portals`]). Reset on every scene load, like the rest.
    static SEAMLESS_PORTALS: Cell<bool> = const { Cell::new(false) };
    /// EXT: a scene's own distance fog, or `None` for the weather's (see [`set_fog`]).
    /// Reset on every scene load, like the rest.
    static FOG: Cell<Option<Fog>> = const { Cell::new(None) };
    /// EXT: side of the world's fundamental square, or 0 in an ordinary unbounded scene.
    ///
    /// The intro meadow is a flat torus (`ext/terrain.rs`): the player's position is wrapped
    /// every step, which is only invisible if every world-space pattern repeats over exactly
    /// that distance. Materials receive this as the `wrap` uniform and round their world-space
    /// frequencies to whole cycles per period; without it the grass patchiness would slide
    /// sideways each time the player crossed the seam.
    static WRAP: Cell<f32> = const { Cell::new(0.0) };
}

/// Set the world period, or 0 for scenes that do not wrap. Cleared on every scene load.
pub fn set_wrap(period: f32) {
    WRAP.with(|w| w.set(period));
}

pub fn wrap() -> f32 {
    WRAP.with(|w| w.get())
}

pub fn set_mood_enabled(on: bool) {
    MOOD_ENABLED.with(|m| m.set(on));
}

/// What a pass whose eye is past the split grades itself as. `MOOD_SUNSET` by default; the
/// Backrooms sets `MOOD_INTERIOR`. Reset on every scene load.
pub fn set_far_mood(mood: f32) {
    FAR_MOOD.with(|m| m.set(mood));
}

/// What a pass whose eye is on THIS side of the split grades itself as. Storm (`0.0`) by
/// default; the intro sets [`MOOD_DUSK`]. Reset on every scene load, like the rest.
pub fn set_near_mood(mood: f32) {
    NEAR_MOOD.with(|m| m.set(mood));
}

/// Grade the whole scene as `mood`, wherever a pass's eye is: for a scene without the meadow
/// split -- one that is an interior from the first step (`MOOD_INTERIOR`), say. Overrides the
/// split while set; `set_mood_enabled` / `set_far_mood` keep working for the split levels, which
/// never call this. Reset on every scene load, like the rest.
pub fn set_scene_mood(mood: f32) {
    SCENE_MOOD.with(|m| m.set(Some(mood)));
}

/// Back to the split logic (or plain daylight): what `load_scene` calls.
pub fn clear_scene_mood() {
    SCENE_MOOD.with(|m| m.set(None));
}

/// Mood for a render pass whose camera eye is at `eye`:
///   -1 = plain daylight (scenes without the split),
///    0 = storm or 3 = dusk (the meadow, whichever the scene asked for),
///    1 = sunset / 2 = interior (the world behind the door);
/// or the scene's one mood, if it set one (`set_scene_mood`).
pub fn mood_for(eye: crate::vector::Vector3) -> f32 {
    if let Some(scene) = SCENE_MOOD.with(|m| m.get()) {
        scene
    } else if !MOOD_ENABLED.with(|m| m.get()) {
        -1.0
    } else if eye.x > MOOD_SPLIT_X {
        FAR_MOOD.with(|m| m.get())
    } else {
        NEAR_MOOD.with(|m| m.get())
    }
}

pub fn set_glow(pos: crate::vector::Vector3, intensity: f32) {
    GLOW.with(|g| g.set([pos.x, pos.y, pos.z, intensity]));
}

pub fn glow() -> [f32; 4] {
    GLOW.with(|g| g.get())
}

/// EXT: aim the flashlight's cone. `range` in metres, the two angles in degrees (`inner` is
/// the full-strength core, `outer` where it has faded to nothing), `gain` its brightness.
///
/// Publish it from the player's EYE, never from `RenderCtx::eye`: with the third-person boom
/// the render camera stands metres behind the player (`ext/thirdperson.rs`), and a torch hung
/// off it would light the room from over their shoulder.
pub fn set_spot(
    pos: crate::vector::Vector3,
    dir: crate::vector::Vector3,
    range: f32,
    inner_deg: f32,
    outer_deg: f32,
    color: [f32; 3],
    gain: f32,
) {
    let d = dir.normalized_safe();
    SPOT_POS.with(|s| s.set([pos.x, pos.y, pos.z, range]));
    SPOT_DIR.with(|s| s.set([d.x, d.y, d.z, (outer_deg.to_radians()).cos()]));
    SPOT_COL.with(|s| {
        s.set([color[0] * gain, color[1] * gain, color[2] * gain, (inner_deg.to_radians()).cos()])
    });
}

thread_local! {
    /// EXT: how brightly the object being drawn RIGHT NOW answers the torch, as
    /// `(r, g, b, strength)`; zero for everything that is only scenery.
    ///
    /// Set immediately before a draw and cleared immediately after, the way `fog_color` and
    /// `model` already are by the materials that need them (`ext/rigid.rs`, `ext/key.rs`).
    /// It is what makes the beam a search tool rather than a lamp: the things a player can
    /// pick up, wear or be caught by answer it, and the wallpaper does not.
    static SHINE: Cell<[f32; 4]> = const { Cell::new([0.0; 4]) };
}

/// Mark what is drawn next as answering the torch. Pair every call with [`clear_shine`].
pub fn set_shine(color: [f32; 3], strength: f32) {
    SHINE.with(|s| s.set([color[0], color[1], color[2], strength]));
}

pub fn clear_shine() {
    SHINE.with(|s| s.set([0.0; 4]));
}

pub fn shine() -> [f32; 4] {
    SHINE.with(|s| s.get())
}

thread_local! {
    /// EXT: dev flag -- hold a lit torch whether or not one is in hand (`--torch`). For
    /// photographing dark interiors, and for proving the cone reaches a given shader without
    /// having to script a pickup first.
    static FORCE_TORCH: Cell<bool> = const { Cell::new(false) };
}

pub fn set_force_torch(on: bool) {
    FORCE_TORCH.with(|f| f.set(on));
}

pub fn force_torch() -> bool {
    FORCE_TORCH.with(|f| f.get())
}

/// Switch the cone off. `range == 0` is the off state, so this is also what a shader sees
/// before anything has ever been set.
pub fn clear_spot() {
    SPOT_POS.with(|s| s.set([0.0; 4]));
    SPOT_DIR.with(|s| s.set([0.0; 4]));
    SPOT_COL.with(|s| s.set([0.0; 4]));
}

/// The cone as the shaders take it: position+range, direction+cos(outer), colour+cos(inner).
pub fn spot() -> ([f32; 4], [f32; 4], [f32; 4]) {
    (SPOT_POS.with(|s| s.get()), SPOT_DIR.with(|s| s.get()), SPOT_COL.with(|s| s.get()))
}

/// Upload the cone onto whichever program is bound.
///
/// One helper rather than three lines pasted into each of the shared draw paths
/// (`Object::draw_impl`, `GltfModel::draw_part_pass`, the skinned instance, the grass field),
/// because those four blocks have already drifted from each other once -- `grassblade` never
/// got `detail`, `draw_impl` never got `fog_over`. `Shader::set_vec4` is a silent no-op on a
/// program that does not declare the uniform, so calling this everywhere is free.
pub fn upload_spot(shader: &crate::shader::Shader) {
    let (p, d, c) = spot();
    shader.set_vec4("spot_pos", p);
    shader.set_vec4("spot_dir", d);
    shader.set_vec4("spot_col", c);
    shader.set_vec4("shine", shine());
}

/// EXT: detail level for the render pass in flight. The ported renderer re-draws the whole
/// scene into every portal's framebuffer, up to four levels deep (Engine.cpp:207-270),
/// so anything expensive costs several times what the main view suggests. Materials use this to
/// take a cheap path in portal passes, where the result is a small quad on screen anyway.
pub fn detail() -> f32 {
    DETAIL.with(|d| d.get())
}

pub fn set_detail(v: f32) {
    DETAIL.with(|d| d.set(v));
}

/// EXT: a scene's own distance fog, in place of the weather's (`Shaders/gltfpbr.frag`).
///
/// The engine's fog is a thin haze: `1 - exp(-dist * 0.008)`, capped at 90% of the way to a
/// tone picked from the mood. It is atmosphere, not a curtain -- at the far plane (`GH_FAR`,
/// 100 units) it is only half applied, so a level big enough to reach that plane shows the cut,
/// and geometry crossing it pops into an unfaded scene.
///
/// A level whose whole subject is a street running past the far plane wants the other thing:
/// fog dense enough that the draw distance is *hidden*, the way Silent Hill hid its. Hence the
/// shape as well as the colour. `squareness` 1 makes the falloff `1 - exp(-t^2)`, which is what
/// that kind of fog needs and what `Shaders/gltfunlit.frag` has always used: nearly nothing
/// close up, then a hard close. And `cap` 1 lets it reach the colour exactly, which the
/// engine's 0.9 deliberately never does.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Fog {
    /// What distance fades to. Black is a fog too: this level's is the dark, not a mist.
    pub color: [f32; 3],
    /// Reciprocal of the distance at which the exponent's argument reaches 1.
    pub density: f32,
    /// 0 for `exp(-t)`, the engine's; 1 for `exp(-t*t)`; between them, a blend.
    pub squareness: f32,
    /// How much of the way to `color` the fade is allowed to go. 1 arrives.
    pub cap: f32,
}

/// Give this scene its own fog. Cleared on every scene load, so a level that says nothing gets
/// the weather's haze exactly as before.
pub fn set_fog(fog: Fog) {
    FOG.with(|f| f.set(Some(fog)));
}

pub fn clear_fog() {
    FOG.with(|f| f.set(None));
}

/// The scene's fog, or `None` for the weather's. `GltfModel::draw_part_pass` asks once a draw,
/// and `backrooms::GroundCap` asks so that the dark under the world is the colour the world
/// fades to.
pub fn fog() -> Option<Fog> {
    FOG.with(|f| f.get())
}

/// EXT: draw this scene's portal passes at full detail, giving up the saving [`detail`] exists
/// for. Reset on every scene load; only a scene whose portals are meant to be *invisible* asks
/// for it.
///
/// The default trade is the right one almost everywhere: a portal is a doorway, its quad is a
/// small part of the screen, the pass behind it is paid for up to four times over, and dropping
/// normal mapping and the specular lobe there (`Shaders/gltfpbr.frag`) is close to free to look
/// at. It is exactly wrong when the portal is not a doorway but a **seam**: the Liminal
/// Neighborhood's two cuts span the whole world and are meant never to be found, so the street
/// through one is most of the screen and is the same street the player is standing on. Shading
/// it differently draws the seam in -- the brick and the kerbs change the moment you step
/// across, which is the one thing the level cannot have (`level31.rs`, "The street loops").
pub fn set_seamless_portals(on: bool) {
    SEAMLESS_PORTALS.with(|s| s.set(on));
}

/// What [`set_seamless_portals`] was last told. `Engine::render` asks once per pass.
pub fn seamless_portals() -> bool {
    SEAMLESS_PORTALS.with(|s| s.get())
}

/// Seconds since start, as of the current frame.
pub fn time() -> f32 {
    TIME.with(|t| t.get())
}

pub fn set_time(t: f32) {
    TIME.with(|c| c.set(t));
}

/// Current vertical field of view in degrees. Defaults to `GH_FOV`.
pub fn fov() -> f32 {
    FOV.with(|f| f.get())
}

/// Override the field of view. Clamped to a sane range -- past roughly 150 degrees the
/// projection degenerates and the near plane math in `Camera::SetSize` stops being meaningful.
/// Driven every frame by the sprint kick (`ext/sprint.rs`); a dolly zoom would go through the
/// same call.
pub fn set_fov(deg: f32) {
    FOV.with(|f| f.set(deg.clamp(20.0, 150.0)));
}

/// Restore the ported default. Called on every scene load so an effect cannot leak between
/// scenes.
pub fn reset_fov() {
    FOV.with(|f| f.set(GH_FOV));
}

#[cfg(test)]
mod tests {
    /// The shader half of the bargain, read from the file rather than trusted.
    ///
    /// The whole safety of `Fog` is that a scene which sets none is bit-identical to before it
    /// existed, and that rests entirely on `Shaders/gltfpbr.frag` keeping the old constants as
    /// the `a = 0` end of its mixes. Nothing in Rust reads that file, `Shader::set_*` no-ops in
    /// silence on a uniform that is gone, and a wrong default would look exactly like today
    /// everywhere except the one scene nobody diffs. So the test reads it, as the grass and the
    /// paintings do with theirs.

    /// EXT: the varyings a vertex shader assigns must be the ones it declares, and a fragment
    /// shader that reads `ex_world` must be paired with a vertex shader that emits it.
    ///
    /// This test exists because both halves were broken at once, in the change that added the
    /// spotlight: `cutout.vert` got the `ex_world = ...` line without the `out` declaration,
    /// which is a link error the moment anything using that shader is loaded -- and nothing
    /// using it was loaded, because the scene being worked on did not. The compiler cannot see
    /// GLSL and `Shader::set_*` no-ops in silence, so the only guard available is to read the
    /// files, the way the fog and the door's beam direction are already guarded.
    #[test]
    fn every_vertex_shader_declares_what_it_assigns() {
        let dir = crate::app::assets::path("Shaders");
        let mut checked = 0;
        for entry in std::fs::read_dir(&dir).expect("Shaders/") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("vert") {
                continue;
            }
            let src = std::fs::read_to_string(&path).expect("vert");
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            for line in src.lines() {
                let body = line.trim();
                // An assignment to a varying at the start of a statement.
                let Some(rest) = body.strip_prefix("ex_") else { continue };
                let Some(var) = rest.split(['=', ' ', '.', '[']).next() else { continue };
                if var.is_empty() {
                    continue;
                }
                let declared = format!("out ");
                let has = src.lines().any(|l| {
                    l.trim_start().starts_with(&declared) && l.contains(&format!("ex_{var}"))
                });
                assert!(has, "{name} assigns ex_{var} without declaring it as an out");
                checked += 1;
            }
        }
        assert!(checked > 0, "no varyings were checked -- the scan is broken, not the shaders");

        // And the pairing: a fragment shader reading ex_world needs its vertex shader to emit one.
        for entry in std::fs::read_dir(&dir).expect("Shaders/") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("frag") {
                continue;
            }
            let frag = std::fs::read_to_string(&path).expect("frag");
            if !frag.contains("ex_world") {
                continue;
            }
            let vert = path.with_extension("vert");
            let Ok(vsrc) = std::fs::read_to_string(&vert) else { continue };
            assert!(
                vsrc.contains("out vec3 ex_world;"),
                "{} reads ex_world but {} never emits it",
                path.file_name().unwrap().to_string_lossy(),
                vert.file_name().unwrap().to_string_lossy()
            );
        }
    }

    #[test]
    fn the_shader_still_falls_back_to_the_engines_own_fog() {
        let frag = std::fs::read_to_string(crate::app::assets::path("Shaders/gltfpbr.frag"))
            .expect("Shaders/gltfpbr.frag");
        for needed in [
            "uniform vec4 fog_over;",
            "uniform vec4 fog_shape;",
            // The three defaults, each as the `x` end of a mix on `fog_over.a`.
            "mix(haze, fog_over.rgb, fog_over.a)",
            "mix(0.0080, fog_shape.x, fog_over.a)",
            "mix(0.9, clamp(fog_shape.z, 0.0, 1.0), fog_over.a)",
            // Squareness folded in the same way, so a = 0 leaves `t` alone.
            "mix(1.0, t, clamp(fog_shape.y, 0.0, 1.0) * fog_over.a)",
        ] {
            assert!(frag.contains(needed), "gltfpbr.frag no longer has {needed:?}");
        }
        // And nothing left that would fog unconditionally: the old line is gone.
        assert!(
            !frag.contains("exp(-dist * 0.0080)"),
            "gltfpbr.frag still fogs from `dist` directly, ignoring the scene's Fog"
        );
    }

    /// No fog unless a scene asks: the weather's haze is what every scene had before this
    /// existed, and `None` is what the shader reads as "use it". `Engine::load_scene` clears it.
    #[test]
    fn a_scene_has_the_weathers_fog_until_it_asks_for_its_own() {
        super::clear_fog();
        assert_eq!(super::fog(), None);
        let mine =
            super::Fog { color: [0.0, 0.0, 0.0], density: 1.0 / 45.0, squareness: 1.0, cap: 1.0 };
        super::set_fog(mine);
        assert_eq!(super::fog(), Some(mine));
        super::clear_fog();
        assert_eq!(super::fog(), None, "the next scene gets the weather's again");
    }

    /// Off unless a scene asks, because the saving it gives up is worth having everywhere a
    /// portal is a doorway. `Engine::load_scene` clears it, like every other per-scene flag.
    #[test]
    fn portal_passes_are_cheap_unless_a_scene_says_its_portals_are_seams() {
        super::set_seamless_portals(false);
        assert!(!super::seamless_portals());
        super::set_seamless_portals(true);
        assert!(super::seamless_portals());
        super::set_seamless_portals(false);
    }

    use super::*;

    #[test]
    fn defaults_to_the_ported_constant() {
        reset_fov();
        assert!((fov() - GH_FOV).abs() < 1e-6);
    }

    #[test]
    fn set_and_reset_round_trip() {
        set_fov(95.0);
        assert!((fov() - 95.0).abs() < 1e-6);
        reset_fov();
        assert!((fov() - GH_FOV).abs() < 1e-6);
    }

    /// The near side is the scene's choice too: storm unless it says dusk, and back to storm
    /// for the next scene (`load_scene` resets it).
    #[test]
    fn near_mood_is_the_scenes_choice_this_side_of_the_split() {
        use crate::vector::Vector3;
        let near = Vector3::new(0.0, 1.0, 0.0);
        let far = Vector3::new(MOOD_SPLIT_X + 10.0, 1.0, 0.0);
        set_mood_enabled(true);
        assert_eq!(mood_for(near), 0.0, "storm unless the scene says otherwise");
        set_near_mood(MOOD_DUSK);
        assert_eq!(mood_for(near), MOOD_DUSK);
        assert_eq!(mood_for(far), MOOD_SUNSET, "which says nothing about the far world");
        set_near_mood(0.0);
        set_mood_enabled(false);
        assert_eq!(mood_for(near), -1.0, "and nothing at all without the split");
    }

    #[test]
    fn far_mood_is_the_scenes_choice_past_the_split() {
        use crate::vector::Vector3;
        let near = Vector3::new(0.0, 1.0, 0.0);
        let far = Vector3::new(MOOD_SPLIT_X + 10.0, 1.0, 0.0);
        set_mood_enabled(true);
        assert_eq!(mood_for(far), MOOD_SUNSET, "sunset unless the scene says otherwise");
        set_far_mood(MOOD_INTERIOR);
        assert_eq!(mood_for(far), MOOD_INTERIOR);
        assert_eq!(mood_for(near), 0.0, "the near side is storm whatever lies beyond");
        set_far_mood(MOOD_SUNSET);
        set_mood_enabled(false);
        assert_eq!(mood_for(far), -1.0);
    }

    /// A whole-scene mood answers for every eye and steps aside when cleared, leaving the
    /// split logic as it was.
    #[test]
    fn scene_mood_overrides_the_split_for_every_eye() {
        use crate::vector::Vector3;
        let near = Vector3::new(0.0, 1.0, 0.0);
        let far = Vector3::new(MOOD_SPLIT_X + 10.0, 1.0, 0.0);
        set_mood_enabled(true);
        set_scene_mood(MOOD_INTERIOR);
        assert_eq!(mood_for(near), MOOD_INTERIOR);
        assert_eq!(mood_for(far), MOOD_INTERIOR);
        clear_scene_mood();
        assert_eq!(mood_for(near), 0.0, "the split is back");
        assert_eq!(mood_for(far), MOOD_SUNSET);
        set_mood_enabled(false);
        set_scene_mood(MOOD_INTERIOR);
        assert_eq!(mood_for(near), MOOD_INTERIOR, "with no split at all it still answers");
        clear_scene_mood();
        assert_eq!(mood_for(near), -1.0);
    }

    #[test]
    fn clamps_degenerate_values() {
        set_fov(1.0);
        assert!(fov() >= 20.0);
        set_fov(400.0);
        assert!(fov() <= 150.0);
        reset_fov();
    }
}
