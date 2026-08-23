//! EXT: everything in this directory is **new work**, not part of the C++ port.
//!
//! The port proper lives in `src/*.rs` and is a line-for-line transcription of
//! HackerPoet/NonEuclidean, carrying 218 `// PORT:` comments that each cite the original source
//! line they deviate from. Nothing here has a C++ counterpart.
//!
//! The split is deliberate: the ported files stay diffable against the C++, and additions stay
//! auditable as additions. Where an extension genuinely needs a hook inside a ported file, that
//! hook is a few lines tagged `// EXT:` so it is instantly distinguishable from a `// PORT:`
//! comment.
//!
//! | Module     | Adds                                                            |
//! |------------|-----------------------------------------------------------------|
//! | `bounds`   | Invisible collision walls that keep scenes in-bounds            |
//! | `raycast`  | Ray/rectangle intersection on the ported `Collider`             |
//! | `grab`     | Superliminal-style forced-perspective resizing                  |
//! | `audio`    | Music and sound effects (the original engine is silent)         |
//! | `gamepad`  | DualSense support, finishing CodeParade's `//TODO:` stubs       |
//! | `room`     | Per-frame room logic (the ported `Scene` trait has load-only)   |
//! | `visibility` | Analytic "is the player looking at this" test                 |
//! | `view`     | Runtime field of view, for the dolly zoom                        |
//! | `ui`       | 2D sprites, text and fills (the engine has no 2D layer)          |
//! | `hud`      | The cursor: dot / open hand / closed hand                        |
//! | `outline`  | White silhouette around the held object                           |
//! | `rotate`   | Rotate the held object with R1 + right stick / RMB + mouse        |
//! | `menu`     | Title screen and pause menu                                       |
//! | `settings` | Sensitivity and mute, and the file they survive in                |
//! | `skybake`  | Clouds baked once into a panorama; the sky is one texture tap     |
//! | `door`     | A freestanding door with a swinging leaf (the intro level)       |
//! | `grassfield` | Real grass blades, in a patch that follows the player          |
//! | `frametime` | Frame-time statistics for the `--shot` dev path                  |
//! | `cull`     | View-frustum culling, computed once per render pass              |
//! | `grassgen` | The blade patch itself, generated in-process and bucketed for culling |
//! | `trimesh`  | Triangle-mesh collision (parry3d) beside the ported rectangles   |
//! | `backrooms` | The scanned, light-baked Backrooms as solid scenery             |
//! | `sprint`   | Running: Shift to hold, L3 to toggle, with an FOV kick and footsteps |
//! | `meadow`   | The intro's meadow, door and title vantage, shared by the scenes that open on it |

pub mod audio;
pub mod backrooms;
pub mod bounds;
pub mod cull;
pub mod door;
pub mod frametime;
pub mod gamepad;
pub mod gltf_model;
pub mod grab;
pub mod grassfield;
pub mod grassgen;
pub mod hud;
pub mod meadow;
pub mod menu;
pub mod outline;
pub mod raycast;
pub mod room;
pub mod rotate;
pub mod settings;
pub mod skybake;
pub mod sprint;
pub mod terrain;
pub mod trimesh;
pub mod ui;
pub mod ui_atlas;
pub mod view;
pub mod visibility;

use audio::{Audio, Sfx};
use grab::GrabState;
use menu::Menu;
use outline::Outline;
use rotate::Rotate;
use sprint::Sprint;
use ui::Ui;

/// All extension state, owned by `Engine` behind a single `RefCell`.
///
/// Kept in one struct so the hook inside the ported engine is a single field and a single call
/// rather than a scattering of new members.
pub struct ExtState {
    pub grab: GrabState,
    pub audio: Audio,
    pub ui: Ui,
    pub outline: Outline,
    pub rotate: Rotate,
    pub menu: Menu,
    /// Shader for the translucent "ghost" copy drawn while a held object is being shrunk to fit.
    pub ghost_shader: std::rc::Rc<crate::shader::Shader>,
    /// Baked cloud panorama the sky shader samples (see skybake.rs).
    pub sky: skybake::SkyBake,
    /// The grass blade patch, pinned for the life of the engine so that leaving the intro and
    /// coming back (title -> NEW GAME, MAIN MENU) never regenerates or re-uploads it. The
    /// scenes reach it through `GrassMesh::acquire`'s weak cache; this is what keeps that
    /// cache warm. Dropped with the engine, while the GL context is still current.
    #[allow(dead_code)] // held, never read: its whole job is to keep the Rc count above zero
    pub grass: std::rc::Rc<grassfield::GrassMesh>,
    pub sprint: Sprint,
    /// `Player::steps()` as of the last footstep fired, so each footfall sounds once. Never
    /// reset: the counter only ever grows, and equality is the test, so a scene load (which
    /// leaves the counter alone) cannot fire a stale step.
    footsteps_heard: u32,
}

impl ExtState {
    pub fn new(gl: &std::rc::Rc<glow::Context>, res: &crate::resources::Resources) -> ExtState {
        ExtState {
            grab: GrabState::default(),
            audio: Audio::new(),
            ui: Ui::new(gl, res),
            outline: Outline::new(gl, res),
            rotate: Rotate::default(),
            menu: Menu::new(),
            ghost_shader: res.acquire_shader("ghost"),
            sky: skybake::SkyBake::new(gl, res),
            grass: grassfield::GrassMesh::acquire(gl),
            sprint: Sprint::new(crate::ext::view::time()),
            footsteps_heard: 0,
        }
    }

    /// Called when the engine loads a scene, so carried objects do not survive the transition
    /// (`Engine::LoadScene` clears the object vector, which would leave a dangling index).
    pub fn on_scene_loaded(&mut self, scene: usize) {
        self.grab.clear();
        // EXT: an effect must never leak into the next scene.
        crate::ext::view::reset_fov();
        self.sprint.reset(crate::ext::view::time());
        self.audio.set_scene(scene);
    }

    /// One footstep sound per footfall the player has taken since the last call -- at most
    /// one per rendered frame, which is the cadence this is called at. Two footfalls in one
    /// frame would need a frame longer than a bob half-period (~200 ms), where a second
    /// identical sample a few milliseconds later would be noise rather than information.
    pub fn fire_footstep_sfx(&mut self, steps: u32) {
        if steps != self.footsteps_heard {
            self.footsteps_heard = steps;
            self.audio.play(Sfx::Footstep);
        }
    }

    /// Convert grab state changes into one-shot sounds.
    pub fn fire_grab_sfx(&mut self) {
        if self.grab.just_grabbed {
            self.audio.play(Sfx::Grab);
        }
        if self.grab.just_released {
            self.audio.play(Sfx::Release);
        }
    }
}

