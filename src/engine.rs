//! Port of Engine.h / Engine.cpp.
//!
//! PORT: everything Win32 is gone from this file. `CreateGLWindow` (Engine.cpp:330-413),
//! `WindowProc` (Engine.cpp:272-328), `SetupInputs` (Engine.cpp:440-467), `ConfineCursor`
//! (Engine.cpp:469-475) and `ToggleFullscreen` (Engine.cpp:485-501) all move to main.rs, which
//! owns the winit window and the glutin GL context. What is left here -- `InitGLObjects`,
//! `Run`'s body, `Update`, `Render`, `LoadScene`, `NearestPortalDist`, `DestroyGLObjects` -- is
//! transcribed as-is.
//!
//! PORT: the four C++ globals are not ported as statics. `GH_ENGINE` (Engine.cpp:14) and
//! `GH_INPUT` (Engine.cpp:16) are threaded through as `RenderCtx` / `UpdateCtx`; `GH_REC_LEVEL`
//! (Engine.cpp:17) and `GH_FRAME` (Engine.cpp:18) become `Cell` fields below so that
//! `render(&self)` can still mutate the recursion counter. `GH_PLAYER` (Engine.cpp:15) is
//! unused outside Engine itself and is simply the `player` field.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use glow::HasContext;

use crate::camera::Camera;
use crate::game_header::{
    gh_clamp, gh_min, GH_DT, GH_FAR, GH_FBO_SIZE, GH_MAX_PORTALS, GH_MAX_RECURSION, GH_MAX_STEPS,
    GH_NEAR_MAX, GH_NEAR_MIN, GH_USE_SKY,
};
// EXT: the scene registry -- every scene's key, name and constructor, in key order.
use crate::ext::scenes::{INTRO, SCENES};
use crate::input::Input;
use crate::object::{ObjectT, RenderCtx, UpdateCtx};
use crate::player::Player;
use crate::props::Sky;
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec, Scene};
use crate::timer::Timer;

pub struct Engine {
    // PORT: hDC / hRC / hWnd / hInstance (Engine.h:39-42) and iWidth / iHeight / isFullscreen
    // (Engine.h:44-46) live in main.rs now. The window size arrives as parameters to
    // `run_frame` instead, because winit owns it.
    //
    // PORT: glow needs an explicit context where C++ used GLEW's global function pointers.
    gl: Rc<glow::Context>,
    // PORT: the C++ resource caches are function-local statics in Resources.cpp; they become a
    // member here so that scenes can reach them without a global.
    res: Resources,

    // PORT: `Camera main_cam` (Engine.h:48) -> RefCell, because Run mutates it through the
    // otherwise-shared `&self`.
    main_cam: RefCell<Camera>,
    // PORT: `Input input` (Engine.h:49) -> RefCell; main.rs writes key/mouse state into it.
    input: RefCell<Input>,
    timer: Timer,

    v_objects: RefCell<PObjectVec>,
    v_portals: RefCell<PPortalVec>,
    // PORT: `std::shared_ptr<Sky> sky` (Engine.h:54) -> a plain field. C++ needs the indirection
    // only because Sky is constructed after the GL context exists; here `Engine::new` already
    // has one.
    sky: Sky,
    player: Rc<RefCell<Player>>,

    // PORT: `GLint occlusionCullingSupported` (Engine.h:57) -> bool; see `new` for why it is
    // not queried.
    occlusion_supported: bool,

    v_scenes: Vec<Rc<dyn Scene>>,
    cur_scene: RefCell<Option<Rc<dyn Scene>>>,

    // PORT: `extern int GH_REC_LEVEL` (GameHeader.h:51) -> a Cell field (Engine.cpp:17).
    rec_level: Cell<i32>,
    // PORT: `extern int64_t GH_FRAME` (GameHeader.h:52) -> a Cell field (Engine.cpp:18).
    // Nothing in the codebase ever reads it; it is kept because the original keeps it.
    frame: Cell<i64>,

    // PORT: `Run`'s two loop locals (Engine.cpp:71-72) become fields, because the fixed-step
    // loop is now driven one frame at a time from winit's `about_to_wait` and cannot keep
    // them on the stack.
    ticks_per_step: i64,
    cur_ticks: Cell<i64>,

    // EXT: all non-port state (forced-perspective grab, audio) lives behind this one field so
    // the hook into the ported engine stays a single member. See src/ext/mod.rs.
    ext: RefCell<crate::ext::ExtState>,
    // EXT: set by the platform layer when the gamepad's grab button goes down; OR-ed with the
    // keyboard's E key in run_frame.
    pad_grab: Cell<bool>,
    // EXT: current scene, so the gamepad shoulder buttons can cycle relative to it.
    cur_scene_ix: Cell<usize>,
    // EXT: set by the menu's Exit; main.rs polls it and ends the event loop.
    quit_requested: Cell<bool>,
    // EXT: dev tooling -- `--shot path` saves the next rendered frame here, then quits.
    shot_path: RefCell<Option<String>>,
    shot_after_frames: Cell<i32>,
    // EXT: dev tooling -- key slots `--forward` / `--strafe` / `--sprint` hold down for the
    // whole run, re-asserted at the top of every frame rather than set once, because a focus
    // change drops every key level (main.rs) and a headless window may never be focused at all.
    dev_hold: RefCell<Vec<usize>>,
    // EXT: this frame's gamepad edges, handed in by main.rs before run_frame.
    pad_events: Cell<crate::ext::gamepad::PadEvents>,
    // EXT: dev tooling -- wall time per rendered frame, reported on the `[shot]` line.
    frame_clock: RefCell<crate::ext::frametime::FrameClock>,
    // EXT: the portal occlusion queries, kept for the life of the engine (the C++ generates
    // and deletes a set per pass, Engine.cpp:221/250) and read one frame late rather than
    // synchronously; see src/ext/occlusion.rs. Freed in `destroy_gl_objects`.
    occlusion: RefCell<crate::ext::occlusion::Occlusion>,
    // EXT: the name of the render pass in flight -- the chain of portals it is seen through --
    // which is what the occlusion slots are keyed on. ROOT for the main view; pushed and
    // popped around each nested `Portal::draw` in `render`.
    pass_path: Cell<crate::ext::occlusion::Path>,
    // EXT: the portal framebuffers, one per recursion level, shared by every portal in the
    // scene. The C++ gives each Portal its own three 2048-square FrameBuffers (Portal.h:43):
    // ~20 MB apiece, 360 MB for the floorplan's six portals, a gigabyte for a twelve-portal
    // scene -- and on a drawable wider than 2048 the door was under-sampled. One per level
    // suffices because a portal renders its framebuffer and draws from it before any sibling
    // at the same level renders, and the nested levels use the other indices. Sized to the
    // drawable (capped at GH_FBO_SIZE a side) by `ensure_portal_fbos`, which recreates them on
    // a resize; borrowed immutably through the RenderCtx, since `render` is re-entrant.
    portal_fbos: RefCell<Vec<crate::frame_buffer::FrameBuffer>>,
}

impl Engine {
    // PORT: `Engine::Engine()` (Engine.cpp:28-52) minus the window/GL/raw-input setup, which
    // main.rs performs before calling this. The GL context arrives as a parameter.
    pub fn new(gl: &Rc<glow::Context>) -> Engine {
        // EXT: first, before anything reads a sensitivity or builds the menu that displays one.
        crate::ext::settings::load();
        let res = Resources::new(gl);

        // Engine::InitGLObjects()   (Engine.cpp:415-432)
        // PORT: glewInit() (Engine.cpp:417) has no equivalent -- glow resolves its function
        // pointers when the context is built in main.rs.
        unsafe {
            //Basic global variables
            gl.clear_color(0.6, 0.9, 1.0, 1.0);
            gl.enable(glow::CULL_FACE);
            gl.cull_face(glow::BACK);
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LESS);
            gl.depth_mask(true);
        }

        //Check GL functionality
        // PORT: glow exposes no glGetQueryiv, so
        // `glGetQueryiv(GL_SAMPLES_PASSED_ARB, GL_QUERY_COUNTER_BITS_ARB, &occlusionCullingSupported)`
        // (Engine.cpp:428) cannot be transcribed. Occlusion queries on GL_SAMPLES_PASSED are
        // core since 1.5 and mandatory in a 3.3+ core profile, so the answer is unconditionally
        // "supported" on any context this port can run on.
        let occlusion_supported = true;

        // PORT: wglSwapIntervalEXT(1) (Engine.cpp:431) moves to main.rs, where glutin owns the
        // surface (`set_swap_interval(SwapInterval::Wait(1))`).

        let player = Rc::new(RefCell::new(Player::new()));

        // PORT: `sky.reset(new Sky)` runs AFTER LoadScene(0) in C++ (Engine.cpp:51); here it must
        // run before, because `sky` is a plain field of the struct being built. The only
        // observable difference is the insertion order of "quad.obj"/"sky" in the resource
        // caches, which nothing depends on.
        let sky = Sky::new(gl, &res);

        // PORT: `std::shared_ptr<Scene>` -> `Rc<dyn Scene>`; the seven registrations keep their
        // order, which is what makes keys '1'..'7' select them (Engine.cpp:41-47).
        // EXT: the registrations, the extension scenes' included, are the one table in
        // src/ext/scenes.rs; this builds them in its order.
        let v_scenes: Vec<Rc<dyn Scene>> = SCENES.iter().map(|entry| (entry.make)()).collect();

        let timer = Timer::new();
        // PORT: `const int64_t ticks_per_step = timer.SecondsToTicks(GH_DT)` is a local at the
        // top of Run (Engine.cpp:71); it is constant, so it is computed once here.
        let ticks_per_step = timer.seconds_to_ticks(GH_DT);

        // EXT: built here because it borrows `res`, which the literal below moves.
        let ext_state = crate::ext::ExtState::new(gl, &res);

        let engine = Engine {
            gl: Rc::clone(gl),
            res,
            main_cam: RefCell::new(Camera::new()),
            input: RefCell::new(Input::new()),
            timer,
            v_objects: RefCell::new(Vec::new()),
            v_portals: RefCell::new(Vec::new()),
            sky,
            player,
            occlusion_supported,
            v_scenes,
            cur_scene: RefCell::new(None),
            rec_level: Cell::new(0),
            frame: Cell::new(0),
            ticks_per_step,
            cur_ticks: Cell::new(0),
            // EXT:
            ext: RefCell::new(ext_state),
            pad_grab: Cell::new(false),
            cur_scene_ix: Cell::new(0),
            quit_requested: Cell::new(false),
            shot_path: RefCell::new(None),
            shot_after_frames: Cell::new(0),
            dev_hold: RefCell::new(Vec::new()),
            pad_events: Cell::new(crate::ext::gamepad::PadEvents::default()),
            frame_clock: RefCell::new(crate::ext::frametime::FrameClock::new()),
            occlusion: RefCell::new(crate::ext::occlusion::Occlusion::new(gl)),
            pass_path: Cell::new(crate::ext::occlusion::ROOT),
            portal_fbos: RefCell::new(Vec::new()),
        };

        // EXT: the title screen draws the intro level behind it (`render_menu_frame`), so the
        // game boots into that scene rather than the ported scene 0. Nothing is played until
        // NEW GAME closes the menu; until then the level is only ever a backdrop.
        engine.load_scene(INTRO);
        // EXT: a mute saved from a previous session applies to the music the load above just
        // started. Done after the load rather than before, so the toggle has something to stop.
        if crate::ext::settings::muted() {
            engine.ext.borrow_mut().audio.toggle_mute();
        }
        engine
    }

    // PORT: accessor for `GH_REC_LEVEL`, which Portal::Draw reads directly in C++
    // (Portal.cpp:17/44).
    pub fn rec_level(&self) -> i32 {
        self.rec_level.get()
    }

    // PORT: added so main.rs can feed winit keyboard/mouse events into the Input that used to
    // be filled by WindowProc / UpdateRaw (Engine.cpp:295-320).
    pub fn input(&self) -> &RefCell<Input> {
        &self.input
    }

    // const Player& GetPlayer() const   (Engine.h:28)
    // PORT: nothing in the codebase calls it; kept for completeness.
    #[allow(dead_code)]
    pub fn get_player(&self) -> &Rc<RefCell<Player>> {
        &self.player
    }

    // PORT: `Run`'s prologue (Engine.cpp:70-73). Split out because the message pump is winit's.
    pub fn start_run(&self) {
        //Setup the timer
        self.cur_ticks.set(self.timer.get_ticks());
        self.frame.set(0);
    }

    // PORT: the body of Run's `else` branch -- everything the C++ does on a frame with no
    // pending window message (Engine.cpp:86-126). ConfineCursor (Engine.cpp:88) and
    // SwapBuffers (Engine.cpp:125) stay in main.rs with the window; the window size that used
    // to be the `iWidth`/`iHeight` members arrives as parameters.
    pub fn run_frame(&self, i_width: i32, i_height: i32) {
        // EXT: frame clock for shaders.
        crate::ext::view::set_time(self.timer.get_ticks() as f32 * 1e-9);
        // EXT: dev tooling -- the keys a direct run holds down (see `dev_hold`).
        for &k in self.dev_hold.borrow().iter() {
            self.input.borrow_mut().key[k] = true;
        }
        // EXT: and the dev frame-time record. Ticked here, at the top, so one interval spans a
        // whole frame including the swap main.rs does after run_frame returns.
        self.frame_clock.borrow_mut().tick();
        // EXT: and the occlusion frame stamp, for the same reason: every render pass below --
        // the menu's as much as the game's -- must carry one frame's stamp, and a stored
        // query result is only believed when its stamp is exactly one frame old.
        self.occlusion.borrow_mut().next_frame();
        // EXT: portal framebuffers at the drawable's size, before either render path below.
        self.ensure_portal_fbos(i_width, i_height);
        // EXT: surface any error the streamed music has queued. Here rather than further down
        // because the menu branch below returns early, and music plays under the menus too.
        self.ext.borrow_mut().audio.tick();
        // EXT: and write any settings the menu changed. A single Cell read on the frames where
        // nothing did, which is nearly all of them; see ext/settings.rs for why the write is
        // deferred to here rather than done where the value changes.
        crate::ext::settings::flush();
        // EXT: menus. Escape (key slot 27) opens the pause menu while playing; while any menu
        // is open the world is frozen -- no physics, no look, no grab -- and only the menu
        // reads input. The menu's own update consumes its navigation keys.
        // EXT: this frame's pad edges. Taken out here, once, because two readers share them:
        // the menu block directly below, and the sprint toggle after it -- which must not see
        // an L3 press made while a menu was open, and does not, because that path returns.
        let pad = self.pad_events.replace(crate::ext::gamepad::PadEvents::default());
        let menu_open = {
            let action = {
                // EXT: the RefMut must be released before apply_menu_action, which re-borrows
                // self.ext mutably (a shadowed `&mut *guard` would outlive a drop of itself).
                let mut guard = self.ext.borrow_mut();
                let input = self.input.borrow();
                // EXT: the Escape press that opens the pause menu must NOT also be fed to
                // Menu::update on the same frame -- Nav::read treats slot 27 as `back`, and
                // `back` on the pause root is Continue, which would close the menu again
                // before it was ever drawn. The opening frame therefore skips the update.
                //
                // The pad's Options button is the same story with the same fix. It is NOT
                // routed through `Nav::read` like the other pad edges: those are things to do
                // *inside* a menu, and this is the one that decides a menu exists at all.
                // While one is open it means "close it", which is what the pause root's `back`
                // already does -- hence the `menu_back` below rather than a second code path.
                if !guard.menu.is_open() && (input.key_press[27] || pad.pause) {
                    guard.menu.open_pause();
                    crate::ext::menu::MenuAction::None
                } else {
                    let mut pad = pad;
                    pad.menu_back |= pad.pause;
                    guard.menu.update(&input, &pad, self.v_scenes.len())
                }
            };
            self.apply_menu_action(action);
            self.ext.borrow().menu.is_open()
        };
        if menu_open {
            if self.ext.borrow().menu.is_title() {
                self.step_title_backdrop();
            } else {
                // Keep the clock from accumulating a burst of catch-up steps on resume.
                self.cur_ticks.set(self.timer.get_ticks());
            }
            self.input.borrow_mut().end_frame();
            // EXT: the clouds evolve under the menus too (they only drifted before: this
            // was gameplay-only, and the title can sit for minutes). Same rule as below --
            // before the viewport is set, because a bake leaves it at the panorama's size.
            self.ext.borrow().sky.maybe_rebake(crate::ext::view::time());
            self.render_menu_frame(i_width, i_height);
            // EXT: menu frames are screenshot-able too -- `--shot` with no `--scene` is how the
            // title screen and its backdrop get photographed, and this is the only path it runs.
            self.maybe_screenshot(i_width, i_height);
            return;
        }

        // EXT: the C++ hard-codes seven if/else branches for keys 1-7 (Engine.cpp:90-104).
        // With extension scenes added there are more than nine, so the mapping is table-driven
        // by the registry's keys (src/ext/scenes.rs).
        for (i, entry) in SCENES.iter().enumerate() {
            if self.input.borrow().key_press[entry.key as usize] {
                self.load_scene(i);
                break;
            }
        }

        // EXT: latch the edge-triggered extension keys HERE, before the fixed-step loop below
        // calls Input::EndFrame -- which memsets key_press to zero (Input.cpp:11). Anything that
        // reads key_press after that loop always sees false. The ported scene keys above avoid
        // this only by being checked first.
        if self.input.borrow().key_press[b'E' as usize] {
            self.pad_grab.set(true);
        }
        if self.input.borrow().key_press[b'M' as usize] {
            let muted = self.ext.borrow_mut().audio.toggle_mute();
            println!("[audio] {}", if muted { "muted" } else { "unmuted" });
        }

        // EXT: object rotation. While the modifier is held, this frame's look input is taken
        // away from the camera (so it freezes) and handed to the held object instead. Must run
        // before the fixed-step loop, which is where the player would otherwise consume it.
        {
            let holding = self.ext.borrow().grab.held.is_some();
            let mut ext = self.ext.borrow_mut();
            let delta = ext.rotate.begin_frame(&mut self.input.borrow_mut(), holding);
            if let (Some((yaw, pitch)), Some(idx)) = (delta, ext.grab.held) {
                if let Some(obj) = self.v_objects.borrow().get(idx) {
                    if let Ok(mut o) = obj.try_borrow_mut() {
                        let b = o.base_mut();
                        b.euler.y += yaw;
                        b.euler.x += pitch;
                    }
                }
            }
        }

        // EXT: sprint. Resolved once per rendered frame, here, for the same reason as the
        // rotate block: `key_press[16]` (the Shift edge) is zeroed by the first `end_frame`
        // inside the loop below. The multipliers are written into `Input::sprint`, which is
        // what the ported `Player::update_player` reads on every step of this frame.
        {
            let mut input = self.input.borrow_mut();
            let factors =
                self.ext.borrow_mut().sprint.resolve(&input, pad.sprint, crate::ext::view::time());
            input.sprint = factors;
        }

        //Used fixed time steps for updates
        let new_ticks = self.timer.get_ticks();
        // PORT: `for (int i = 0; cur_ticks < new_ticks && i < GH_MAX_STEPS; ++i)` -- Rust has no
        // C-style for, so the counter is manual (Engine.cpp:108).
        let mut i = 0;
        while self.cur_ticks.get() < new_ticks && i < GH_MAX_STEPS {
            self.update();
            self.cur_ticks.set(self.cur_ticks.get() + self.ticks_per_step);
            self.frame.set(self.frame.get() + 1);
            // NOTE (original behaviour, load-bearing): EndFrame is inside the fixed-step loop,
            // so mouse smoothing runs once per 500 Hz step rather than once per rendered frame.
            self.input.borrow_mut().end_frame();
            i += 1;
        }
        // cur_ticks = (cur_ticks < new_ticks ? new_ticks : cur_ticks);   (Engine.cpp:114)
        if self.cur_ticks.get() < new_ticks {
            self.cur_ticks.set(new_ticks);
        }

        // EXT: forced-perspective grab. Runs once per rendered frame rather than per 500 Hz
        // physics step -- it casts a ray, and the held object's transform only needs to be
        // right at draw time.
        self.ext_update();

        // EXT: advance the cloud cross-fade and, every few seconds, re-bake one panorama. This
        // MUST come before the camera's use_viewport below: a bake leaves the viewport at the
        // panorama's size.
        self.ext.borrow().sky.maybe_rebake(crate::ext::view::time());

        //Setup camera for rendering
        let n = gh_clamp(self.nearest_portal_dist() * 0.5, GH_NEAR_MIN, GH_NEAR_MAX);
        {
            let mut main_cam = self.main_cam.borrow_mut();
            main_cam.world_view = self.player.borrow().world_to_cam();
            main_cam.set_size(i_width, i_height, n, GH_FAR);
            main_cam.use_viewport(&self.gl);
        }

        //Render scene
        self.rec_level.set(GH_MAX_RECURSION);
        // PORT: Camera is Copy, so the camera is copied out of the RefCell before rendering;
        // Render recurses arbitrarily deep and must not hold a borrow of main_cam
        // (was: Render(main_cam, 0, nullptr), Engine.cpp:124).
        let cam = *self.main_cam.borrow();
        // PORT: `GLuint curFBO = 0` (the default framebuffer) -> None; `const Portal* skipPortal
        // = nullptr` -> None.
        self.render(&cam, None, None);

        // EXT: overlays, drawn last and only in the main pass. Putting them inside `render`
        // would paint them into every portal's framebuffer as well.
        {
            let mut ext = self.ext.borrow_mut();
            let ext = &mut *ext;
            // Translucent ghost of the held object at its unconstrained placement, when the
            // fit logic had to shrink it (see ext/grab.rs).
            crate::ext::grab::draw_ghost(
                &self.gl,
                &cam,
                &self.v_objects.borrow(),
                &ext.grab,
                &ext.ghost_shader,
            );
            // Silhouette outline on the held object.
            if let Some(idx) = ext.grab.held {
                if let Some(obj) = self.v_objects.borrow().get(idx) {
                    if let Ok(o) = obj.try_borrow() {
                        ext.outline.draw(&self.gl, &cam, &*o);
                    }
                }
            }
            // Cursor.
            let cursor = if ext.grab.held.is_some() {
                crate::ext::hud::Cursor::Closed
            } else if ext.grab.hover {
                crate::ext::hud::Cursor::Open
            } else {
                crate::ext::hud::Cursor::Dot
            };
            ext.ui.begin(i_width, i_height);
            crate::ext::hud::draw(&ext.ui, cursor);
            ext.ui.end();
        }

        // EXT: dev screenshot. Reads the back buffer after everything is drawn.
        self.maybe_screenshot(i_width, i_height);
    }

    /// EXT: `--scene N --shot path [--frames K] [--yaw deg] [--pitch deg]` support. With a
    /// scene: skips the title menu, loads it, optionally aims the camera, renders K frames (so
    /// the fixed-step physics settles the player on the ground), saves a 24-bit BMP, and quits.
    ///
    /// Without a scene it only arms the screenshot and leaves the menu where it is, which is
    /// the only way to photograph the title screen and its backdrop -- loading a scene would
    /// close the menu that is the thing being looked at.
    ///
    /// `hold` lists key slots to keep down every frame (`--forward`, `--strafe`, `--sprint`),
    /// so the shot can photograph the player walking or running and the `[shot]` position
    /// print how far they travelled.
    #[allow(clippy::too_many_arguments)] // one flag each; a struct would only rename them
    pub fn start_direct(
        &self,
        scene: Option<usize>,
        shot: Option<String>,
        frames: i32,
        yaw: f32,
        pitch: f32,
        pos: Option<[f32; 3]>,
        hold: &[usize],
    ) {
        *self.dev_hold.borrow_mut() = hold.to_vec();
        if let Some(scene) = scene {
            self.ext.borrow_mut().menu.close();
            if scene < self.v_scenes.len() {
                self.load_scene(scene);
            }
            self.player.borrow_mut().set_look(yaw.to_radians(), pitch.to_radians());
            if let Some(p) = pos {
                self.player
                    .borrow_mut()
                    .base
                    .set_position(crate::vector::Vector3::new(p[0], p[1], p[2]));
            }
        }
        *self.shot_path.borrow_mut() = shot;
        self.shot_after_frames.set(frames.max(1));
    }

    fn maybe_screenshot(&self, width: i32, height: i32) {
        let Some(path) = self.shot_path.borrow().clone() else { return };
        let left = self.shot_after_frames.get() - 1;
        self.shot_after_frames.set(left);
        if left > 0 {
            return;
        }
        let mut px = vec![0u8; (width * height * 3) as usize];
        unsafe {
            self.gl.read_buffer(glow::BACK);
            self.gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
            self.gl.read_pixels(
                0, 0, width, height,
                glow::BGR, glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut px)),
            );
        }
        // GL rows are bottom-first, which is exactly BMP row order.
        let row = (width * 3) as usize;
        let pad = (4 - row % 4) % 4;
        let mut out = Vec::with_capacity(54 + (row + pad) * height as usize);
        let size = 54 + (row + pad) * height as usize;
        out.extend_from_slice(b"BM");
        out.extend_from_slice(&(size as u32).to_le_bytes());
        out.extend_from_slice(&[0, 0, 0, 0]);
        out.extend_from_slice(&54u32.to_le_bytes());
        out.extend_from_slice(&40u32.to_le_bytes());
        out.extend_from_slice(&width.to_le_bytes());
        out.extend_from_slice(&height.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&24u16.to_le_bytes());
        out.extend_from_slice(&[0u8; 24]);
        for y in 0..height as usize {
            out.extend_from_slice(&px[y * row..(y + 1) * row]);
            out.extend(std::iter::repeat(0u8).take(pad));
        }
        match std::fs::write(&path, &out) {
            Ok(()) => {
                let p = self.player.borrow().obj().pos;
                // The FOV as well: it is how `--sprint` is checked headlessly (ext/sprint.rs).
                println!(
                    "[shot] wrote {path} ({width}x{height}) player at ({:.2}, {:.2}, {:.2}) fov {:.1}",
                    p.x,
                    p.y,
                    p.z,
                    crate::ext::view::fov()
                );
                // EXT: frame cost over the frames after the scene settled. Only meaningful with
                // `--no-vsync`; under the display cap every frame measures the refresh period.
                if let Some((avg, p95, n)) = self.frame_clock.borrow().stats() {
                    println!("[shot] avg frame {avg:.2} ms, p95 {p95:.2} ms over {n} frames");
                }
            }
            Err(e) => eprintln!("[shot] could not write {path}: {e}"),
        }
        *self.shot_path.borrow_mut() = None;
        self.quit_requested.set(true);
    }

    /// EXT: a frame while a menu is open. Both menus draw the world and then the menu over it;
    /// what differs is which world. The pause menu shows the game the player is standing in,
    /// frozen where they left it. The title screen shows the intro level as a backdrop from the
    /// vantage composed for it (`ext::meadow::title_view`), stepped rather than frozen.
    /// The black wash between the two is drawn by `Menu::draw`, which knows how much its
    /// current screen needs.
    fn render_menu_frame(&self, i_width: i32, i_height: i32) {
        let n = gh_clamp(self.nearest_portal_dist() * 0.5, GH_NEAR_MIN, GH_NEAR_MAX);
        {
            let mut main_cam = self.main_cam.borrow_mut();
            // The backdrop's camera is the player's: `step_title_backdrop` has parked them at
            // the composed vantage, which also keeps the blade field centred on the shot.
            main_cam.world_view = self.player.borrow().world_to_cam();
            main_cam.set_size(i_width, i_height, n, GH_FAR);
            main_cam.use_viewport(&self.gl);
        }
        self.rec_level.set(GH_MAX_RECURSION);
        let cam = *self.main_cam.borrow();
        self.render(&cam, None, None);

        let names: Vec<String> = self.scene_names();
        let name_refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        let mut ext = self.ext.borrow_mut();
        let ext = &mut *ext;
        ext.ui.begin(i_width, i_height);
        ext.menu.draw(&ext.ui, &name_refs);
        ext.ui.end();
    }

    /// EXT: carry out whatever the menu decided this frame.
    fn apply_menu_action(&self, action: crate::ext::menu::MenuAction) {
        use crate::ext::menu::MenuAction;
        match action {
            MenuAction::None => {}
            // EXT: every action that closes the menu also drops a stale gamepad grab latch.
            // gilrs Button::South is bound to both `grab` and `menu_confirm`, so confirming a
            // menu item would otherwise fire a grab on the first frame after resuming.
            MenuAction::NewGame => {
                self.pad_grab.set(false);
                // EXT: a new game opens on the intro level.
                self.load_scene(INTRO);
                self.ext.borrow_mut().menu.close();
            }
            MenuAction::Continue => {
                self.pad_grab.set(false);
                self.ext.borrow_mut().menu.close();
            }
            MenuAction::RestartLevel => {
                self.pad_grab.set(false);
                self.load_scene(self.cur_scene_ix.get());
                self.ext.borrow_mut().menu.close();
            }
            MenuAction::SwitchLevel(i) => {
                self.pad_grab.set(false);
                if i < self.v_scenes.len() {
                    self.load_scene(i);
                }
                self.ext.borrow_mut().menu.close();
            }
            MenuAction::MainMenu => {
                // The title screen's backdrop IS the intro level, so leaving a game reloads it;
                // otherwise the title would sit in front of whatever level was being played,
                // seen from a vantage composed for a different world.
                self.load_scene(INTRO);
                self.ext.borrow_mut().menu = crate::ext::menu::Menu::new();
            }
            MenuAction::Quit => self.quit_requested.set(true),
            MenuAction::ToggleMute => {
                self.ext.borrow_mut().audio.toggle_mute();
            }
        }
    }

    /// EXT: human-readable scene names, in key order, for the level-select menu.
    pub fn scene_names(&self) -> Vec<String> {
        SCENES.iter().map(|entry| entry.name.to_string()).collect()
    }

    /// EXT: main.rs polls this each frame.
    pub fn quit_requested(&self) -> bool {
        self.quit_requested.get()
    }

    /// EXT: main.rs hands in this frame's gamepad edges so the menu can use them.
    pub fn set_pad_events(&self, pad: crate::ext::gamepad::PadEvents) {
        self.pad_events.set(pad);
    }

    // void Engine::LoadScene(int ix)   (Engine.cpp:133-144)
    pub fn load_scene(&self, ix: usize) {
        // EXT: timed, because the title -> NEW GAME transition reloads this same scene and the
        // stall it costs is the number the resource caches are measured by.
        let t0 = std::time::Instant::now();
        //Clear out old scene
        {
            let cur_scene = self.cur_scene.borrow();
            if let Some(cur_scene) = cur_scene.as_ref() {
                cur_scene.unload();
            }
        }
        // EXT: the old scene's objects are moved out here and dropped only AFTER the new scene
        // has loaded (was: vObjects.clear(); vPortals.clear(), Engine.cpp:138-139). The
        // resource caches hold `Weak` references that expire with the last object using them,
        // so clearing first meant title -> NEW GAME -- which reloads the very scene on screen
        // -- re-parsed every mesh, re-decoded the door and re-uploaded the lot. Kept alive
        // across the load, every `acquire_*` upgrades instead. The RefCell is only borrowed
        // for the length of the take, and nothing in the old vector is touched again, so the
        // later drop cannot collide with the load's own borrows.
        //
        // The PORTALS are not kept: there is nothing in one worth keeping warm. The eager
        // framebuffers each used to own (which would have doubled the scene's GPU memory for
        // the duration of a load) live on the engine now, shared (`portal_fbos`), and the mesh
        // and two shaders a new portal re-acquires are pinned in `ExtState`. So they go first,
        // and the reload stays warm without them.
        self.v_portals.borrow_mut().clear();
        let old_objects = std::mem::take(&mut *self.v_objects.borrow_mut());
        self.player.borrow_mut().reset();

        // EXT: per-scene shader state starts clean; a scene that wants it sets it in load().
        crate::ext::view::set_mood_enabled(false);
        crate::ext::view::set_far_mood(crate::ext::view::MOOD_SUNSET);
        crate::ext::view::set_glow(crate::vector::Vector3::zero(), 0.0);
        crate::ext::view::set_wrap(0.0);
        // EXT: and so does the title screen's hold on the doors -- `run_frame` sets it again
        // every frame the title is up, so clearing it here cannot strand a door open.
        crate::ext::door::set_hold_open(false);
        // EXT: and the one-frame-late occlusion results, which are about to name different
        // portals.
        self.occlusion.borrow_mut().reset();

        //Create new scene
        let cur_scene = Rc::clone(&self.v_scenes[ix]);
        *self.cur_scene.borrow_mut() = Some(Rc::clone(&cur_scene));
        // PORT: the level code cannot reach the GL context or the resource caches through
        // globals, so both are passed to Load (was: curScene->Load(vObjects, vPortals, *player),
        // Engine.cpp:142).
        cur_scene.load(
            &self.gl,
            &self.res,
            &mut self.v_objects.borrow_mut(),
            &mut self.v_portals.borrow_mut(),
            &mut self.player.borrow_mut(),
        );
        // PORT: the Rc<RefCell<Player>> unsize-coerces to Rc<RefCell<dyn ObjectT>>, which is the
        // equivalent of pushing a shared_ptr<Player> into a vector<shared_ptr<Object>>
        // (was: vObjects.push_back(player), Engine.cpp:143).
        self.v_objects
            .borrow_mut()
            .push(Rc::clone(&self.player) as Rc<RefCell<dyn ObjectT>>);

        // EXT: drop anything being carried (the object vector was just replaced, so a held
        // index would dangle) and cross-fade to this scene's music.
        self.cur_scene_ix.set(ix);
        self.ext.borrow_mut().on_scene_loaded(ix);
        // EXT: now the old scene can go. Anything the new one did not re-acquire is freed here,
        // GL objects included, while the context is current.
        drop(old_objects);
        println!("[load] scene {ix} in {:.0} ms", t0.elapsed().as_secs_f32() * 1e3);
    }

    // void Engine::Update()   (Engine.cpp:146-205)
    pub fn update(&self) {
        //Update
        {
            let input = self.input.borrow();
            // EXT: sample the player's eye transform BEFORE the loop, while nothing else holds
            // a borrow on it.
            let (cam_to_world, player_pos) = {
                let p = self.player.borrow();
                (p.cam_to_world(), p.obj().pos)
            };
            let ctx = UpdateCtx {
                input: &input,
                cam_to_world,
                player_pos,
            };
            let v_objects = self.v_objects.borrow();
            for i in 0..v_objects.len() {
                // PORT: `assert(vObjects[i].get())` (Engine.cpp:149) is unnecessary -- an Rc is
                // never null.
                v_objects[i].borrow_mut().update(&ctx);
            }
        }

        //Collisions
        {
            let v_objects = self.v_objects.borrow();
            // EXT: scratch copy of the current object's hit spheres, reused across the objects
            // of this step (it is a local of the step, so each step allocates it once rather
            // than once per physical object). The copy itself is forced by the borrow rules
            // below; allocating a fresh Vec per object 500 times a second was not.
            let mut hit_spheres: Vec<crate::sphere::Sphere> = Vec::new();
            //For each physics object
            for i in 0..v_objects.len() {
                // PORT: `Physical* physical = vObjects[i]->AsPhysical()` cannot be held across
                // the loop body -- it would keep `vObjects[i]` borrowed while `vObjects[j]` is
                // borrowed mutably and while `on_collide` re-borrows it. Instead the two things
                // the C++ reads through that pointer (the hit spheres and worldToLocal) are
                // pulled out here, and every write goes back through a fresh borrow
                // (was: Engine.cpp:156-158).
                let physical_state = {
                    let obj = v_objects[i].borrow();
                    obj.as_physical().map(|p| {
                        hit_spheres.clear();
                        hit_spheres.extend_from_slice(&p.hit_spheres);
                        p.world_to_local()
                    })
                };
                let Some(mut world_to_local) = physical_state else {
                    continue;
                };

                //For each object to collide with
                for j in 0..v_objects.len() {
                    if i == j {
                        continue;
                    }
                    // PORT: `Object& obj = *vObjects[j]; if (!obj.mesh) { continue; }` -- the Rc
                    // to the mesh is cloned out so the cell can be re-borrowed mutably below
                    // (was: Engine.cpp:163-164).
                    let mesh = v_objects[j].borrow().base().mesh.clone();
                    // EXT: an object may collide through a triangle mesh (src/ext/trimesh.rs)
                    // instead of, or as well as, rectangle colliders -- so a mesh-less object is
                    // only skipped when it has neither.
                    let trimesh = v_objects[j].borrow().trimesh();
                    if mesh.is_none() && trimesh.is_none() {
                        continue;
                    }
                    let colliders: &[crate::collider::Collider] =
                        mesh.as_ref().map_or(&[], |m| &m.colliders);

                    //For each hit sphere
                    for s in 0..hit_spheres.len() {
                        //Brings point from collider's local coordinates to hits's local coordinates.
                        let sphere = hit_spheres[s];
                        let mut world_to_unit = sphere.local_to_unit() * world_to_local;
                        let mut local_to_unit =
                            world_to_unit * v_objects[j].borrow().base().local_to_world();
                        let mut unit_to_world = world_to_unit.inverse();

                        //For each collider
                        for collider in colliders {
                            // PORT: `bool Collide(const Matrix4&, Vector3& push)` -> the push is
                            // returned in an Option (was: Engine.cpp:176-178).
                            if let Some(push) = collider.collide(&local_to_unit) {
                                //If push is too small, just ignore
                                let push = unit_to_world.mul_direction(push);
                                // PORT: OnHit / OnCollide lose their unused `Object& other`
                                // argument (was: vObjects[j]->OnHit(*physical, push);
                                // physical->OnCollide(*vObjects[j], push);, Engine.cpp:181-182).
                                // OnCollide goes through the trait so Player's override wins.
                                v_objects[j].borrow_mut().on_hit(push);
                                v_objects[i].borrow_mut().on_collide(push);

                                world_to_local = v_objects[i]
                                    .borrow()
                                    .as_physical()
                                    .expect("as_physical changed mid-collision")
                                    .world_to_local();
                                world_to_unit = sphere.local_to_unit() * world_to_local;
                                local_to_unit =
                                    world_to_unit * v_objects[j].borrow().base().local_to_world();
                                unit_to_world = world_to_unit.inverse();
                            }
                        }

                        // EXT: then the triangle mesh, one push per round exactly as a rectangle
                        // is applied above -- on_hit, on_collide, matrices rebuilt -- until the
                        // sphere is clear or the round cap is hit. The sphere's world centre and
                        // radius fall out of unit_to_world: its translation and x-axis length.
                        if let Some(trimesh) = &trimesh {
                            for _ in 0..crate::ext::trimesh::MAX_PUSHES {
                                let centre = unit_to_world.translation();
                                let radius = unit_to_world.x_axis().mag();
                                let Some(push) = trimesh.push_sphere(centre, radius) else { break };
                                v_objects[j].borrow_mut().on_hit(push);
                                v_objects[i].borrow_mut().on_collide(push);

                                world_to_local = v_objects[i]
                                    .borrow()
                                    .as_physical()
                                    .expect("as_physical changed mid-collision")
                                    .world_to_local();
                                world_to_unit = sphere.local_to_unit() * world_to_local;
                                unit_to_world = world_to_unit.inverse();
                            }
                        }
                    }
                }
            }
        }

        //Portals
        {
            let v_objects = self.v_objects.borrow();
            let v_portals = self.v_portals.borrow();
            for i in 0..v_objects.len() {
                let mut obj = v_objects[i].borrow_mut();
                if let Some(physical) = obj.as_physical_mut() {
                    for j in 0..v_portals.len() {
                        if physical.try_portal(&v_portals[j].borrow()) {
                            break;
                        }
                    }
                }
            }
        }

        // EXT: a room's request to move the player (src/ext/room.rs), then close the world on
        // scenes that declare a period (the intro meadow's flat torus, src/ext/terrain.rs).
        // Both no-ops nearly everywhere.
        //
        // LAST in the step, and specifically AFTER the portal pass: try_portal has just consumed
        // a continuous prev_pos -> pos segment, and moving the player before it would hand it a
        // segment stretching a whole period across the meadow -- which sweeps the doorway and
        // teleports the player into the sea. Collision has also finished by here, so the wrap
        // cannot fight a push out of a hillside either.
        crate::ext::room::apply_respawn(&mut self.player.borrow_mut());
        crate::ext::terrain::wrap_player(&mut self.player.borrow_mut());
    }

    // void Engine::Render(const Camera& cam, GLuint curFBO, const Portal* skipPortal)
    // (Engine.cpp:207-270)
    //
    // PORT: `GLuint curFBO` -> Option<glow::Framebuffer> (None is the default framebuffer);
    // `const Portal* skipPortal` -> Option<u32>, the portal's identity id.
    pub fn render(
        &self,
        cam: &Camera,
        cur_fbo: Option<glow::Framebuffer>,
        skip_portal: Option<u32>,
    ) {
        // EXT: tell materials whether this is the main view or a portal pass, so expensive
        // shaders can drop detail where it costs the most and shows the least.
        crate::ext::view::set_detail(if self.rec_level.get() >= GH_MAX_RECURSION { 1.0 } else { 0.0 });

        let gl: &glow::Context = &self.gl;
        // EXT: one frustum and one eye per pass, shared by every draw below; and the shared
        // portal framebuffers (a nested `render` takes its own shared borrow of them).
        let portal_fbos = self.portal_fbos.borrow();
        let ctx = RenderCtx {
            gl,
            engine: self,
            frustum: crate::ext::cull::Frustum::from_view_proj(&cam.matrix()),
            eye: cam.world_view.inverse().translation(),
            portal_fbos: &portal_fbos,
        };

        //Clear buffers
        if GH_USE_SKY {
            unsafe {
                gl.clear(glow::DEPTH_BUFFER_BIT);
            }
            // EXT: `sky->Draw(cam)` (Engine.cpp:211) moves below the object loop -- see
            // `Sky::draw` for why.
        } else {
            unsafe {
                gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            }
        }

        let v_portals = self.v_portals.borrow();
        //Create queries (if applicable)
        // PORT: `GLuint drawTest[GH_MAX_PORTALS]` is left uninitialized in C++ and is only ever
        // read on the path that also writes it; zeroing it here changes nothing
        // (was: Engine.cpp:218).
        // EXT: it now holds 1/0 for "passed samples last frame" rather than this frame's
        // sample count; the test below is `== 0` either way.
        let mut draw_test = [0u32; GH_MAX_PORTALS];
        debug_assert!(v_portals.len() <= GH_MAX_PORTALS);
        // PORT: `glGenQueriesARB((GLsizei)vPortals.size(), queries)` (Engine.cpp:220-222) sits
        // here in C++, outside the `GH_REC_LEVEL > 0` block that deletes them -- so at the
        // deepest recursion level the queries are generated and never deleted, leaking one GL
        // query per portal per frame. Creation is moved down into the same block as the
        // deletion; the queries are unused anywhere else.

        //Draw scene
        {
            let v_objects = self.v_objects.borrow();
            for i in 0..v_objects.len() {
                v_objects[i].borrow().draw(&ctx, cam, cur_fbo);
            }
        }

        // EXT: the sky, last, into whatever the scene left uncovered (was first, Engine.cpp:211).
        // Before the portals: their quads are drawn over it like any other surface.
        if GH_USE_SKY {
            self.sky.draw(gl, cam);
        }

        //Draw portals if possible
        if self.rec_level.get() > 0 {
            //Draw portals
            self.rec_level.set(self.rec_level.get() - 1);
            // EXT: settle portals whose quad lies wholly outside this pass's frustum here,
            // without a query. A quad that rasterises no fragment passes no sample, so this is
            // the answer the query would have given -- but the query's readback is a full
            // CPU-GPU round trip (the driver must finish everything queued so far before it can
            // answer), and it was being paid in every pass, including nested ones whose only
            // portal was a thousand units behind the far plane. When nothing is left to ask,
            // the whole query block is skipped.
            let mut in_view = [false; GH_MAX_PORTALS];
            let mut any_in_view = false;
            for i in 0..v_portals.len() {
                let portal = v_portals[i].borrow();
                if Some(portal.id) != skip_portal {
                    in_view[i] = crate::ext::cull::object_sphere(&portal.base)
                        .is_none_or(|(c, r)| ctx.frustum.sphere(c, r));
                    any_in_view |= in_view[i];
                }
            }
            if self.occlusion_supported && self.rec_level.get() > 0 && any_in_view {
                // PORT: see above -- generated here instead of at Engine.cpp:221.
                // EXT: from a persistent set keyed on (pass, portal) rather than
                // glGenQueries/glDeleteQueries per pass, and the readback loop that followed
                // (was: glGetQueryObjectuivARB(queries[i], GL_QUERY_RESULT_ARB, &drawTest[i]),
                // Engine.cpp:243-247) is gone: that was a CPU-GPU stall per pass. `drawTest[i]`
                // is LAST frame's answer for this slot instead -- and only last frame's: an
                // older one, left while the portal was out of the frustum, counts as "visible"
                // -- read before this frame's query is issued; see src/ext/occlusion.rs for
                // the rule and what it changes.
                let mut occ = self.occlusion.borrow_mut();
                let path = self.pass_path.get();
                unsafe {
                    gl.color_mask(false, false, false, false);
                    gl.depth_mask(false);
                }
                for i in 0..v_portals.len() {
                    let portal = v_portals[i].borrow();
                    if Some(portal.id) != skip_portal && in_view[i] {
                        draw_test[i] = occ.visible(path, i, v_portals.len()) as u32;
                        // PORT: the *ARB query entry points and GL_SAMPLES_PASSED_ARB become
                        // their core equivalents (was: glBeginQueryARB(GL_SAMPLES_PASSED_ARB,
                        // queries[i]), Engine.cpp:238).
                        occ.begin(path, i);
                        portal.draw_pink(&ctx, cam);
                        occ.end();
                    }
                }
                unsafe {
                    gl.color_mask(true, true, true, true);
                    gl.depth_mask(true);
                }
                // PORT: glDeleteQueriesARB(n, queries) (Engine.cpp:250) -- the set keeps them.
            }
            for i in 0..v_portals.len() {
                let portal = v_portals[i].borrow();
                if Some(portal.id) != skip_portal {
                    // EXT: out of view -- nothing to draw, at any recursion level.
                    if !in_view[i] {
                        continue;
                    }
                    if self.occlusion_supported && (self.rec_level.get() > 0) && (draw_test[i] == 0)
                    {
                        continue;
                    } else {
                        // EXT: name the nested pass by the portal it is seen through, for its
                        // own occlusion slots; the outer name is put back when it returns.
                        let outer = self.pass_path.get();
                        self.pass_path.set(crate::ext::occlusion::push(outer, i));
                        portal.draw(&ctx, cam, cur_fbo);
                        self.pass_path.set(outer);
                    }
                }
            }
            self.rec_level.set(self.rec_level.get() + 1);
        }

        // PORT: the `#if 0` debug-collider block (Engine.cpp:264-269) is dropped along with
        // Object::DebugDraw, which is immediate mode and cannot exist in a core profile.
    }

    // void Engine::DestroyGLObjects()   (Engine.cpp:434-438)
    // PORT: called from main.rs when the event loop exits (was: Engine.cpp:129).
    pub fn destroy_gl_objects(&self) {
        // PORT: C++ dereferences curScene unconditionally; it is always set by then, but the
        // Option makes that explicit (was: curScene->Unload(), Engine.cpp:435).
        if let Some(cur_scene) = self.cur_scene.borrow().as_ref() {
            cur_scene.unload();
        }
        self.v_objects.borrow_mut().clear();
        self.v_portals.borrow_mut().clear();
        // EXT: the occlusion queries and the portal framebuffers go with them, while the
        // context is still current.
        self.occlusion.borrow_mut().destroy();
        self.portal_fbos.borrow_mut().clear();
    }

    // float Engine::NearestPortalDist() const   (Engine.cpp:477-483)
    pub fn nearest_portal_dist(&self) -> f32 {
        // PORT: FLT_MAX -> f32::MAX (was: float dist = FLT_MAX, Engine.cpp:478).
        let mut dist = f32::MAX;
        let v_portals = self.v_portals.borrow();
        let player = self.player.borrow();
        for i in 0..v_portals.len() {
            dist = gh_min(dist, v_portals[i].borrow().dist_to(player.obj().pos));
        }
        dist
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// EXT: additions beyond the C++ port. Everything above this line transcribes
// Engine.cpp / Engine.h; everything below is new.
// ─────────────────────────────────────────────────────────────────────────────


#[allow(dead_code)] // EXT: scene_count / is_holding are for the HUD, added next.
impl Engine {
    /// EXT: one rendered frame of the title screen's backdrop.
    ///
    /// The intro level has to be alive behind the title -- the door swings itself open while
    /// the menu fades in, the sea moves through it, the blade field settles -- so this runs the
    /// same fixed-step loop `run_frame` does. It differs in the two ways a backdrop differs
    /// from a game:
    ///
    /// * **Nothing drives it.** The loop sees a blank `Input`, so a key still held from the
    ///   level just left, or a mouse swept across the window, cannot walk the shot away. The
    ///   real input is put back afterwards untouched; the menu has already read this frame's
    ///   edges from it, and `run_frame` still ends the frame on it.
    /// * **The camera is parked, not simulated.** `ext::meadow::title_view` composes the shot, and
    ///   the player is pinned there after every step. Pinning is not belt-and-braces: the door
    ///   stands on a knoll, and a player left standing on it slides gently down the slope --
    ///   over the minutes a title screen can be left up, the composed shot would drift off it.
    ///   The pin runs after `update` rather than before so the position that is drawn is the
    ///   pinned one, and it zeroes the velocity that gravity accumulates against it.
    ///
    /// Doors are held open for the same reason the camera is placed by hand: the vantage is
    /// further from the door than any player would have to be to open it.
    fn step_title_backdrop(&self) {
        crate::ext::door::set_hold_open(true);
        let held_input = self.input.replace(Input::new());

        // Once before the loop as well as after every step inside it: a frame rendered faster
        // than the 2 ms step runs no steps at all, and the first frame after a scene loads is
        // always one of those -- the shot has to be composed on that frame too, not on the
        // level's spawn point.
        self.park_backdrop_camera();
        let new_ticks = self.timer.get_ticks();
        let mut i = 0;
        while self.cur_ticks.get() < new_ticks && i < GH_MAX_STEPS {
            self.update();
            self.park_backdrop_camera();
            self.cur_ticks.set(self.cur_ticks.get() + self.ticks_per_step);
            self.frame.set(self.frame.get() + 1);
            self.input.borrow_mut().end_frame();
            i += 1;
        }
        if self.cur_ticks.get() < new_ticks {
            self.cur_ticks.set(new_ticks);
        }

        self.input.replace(held_input);
    }

    /// EXT: put the player back on the backdrop's tripod. See `step_title_backdrop`.
    fn park_backdrop_camera(&self) {
        let (eye, yaw, pitch) = crate::ext::meadow::title_view();
        let mut player = self.player.borrow_mut();
        // set_position, not a bare write: it moves prev_pos with pos, which is what stops the
        // step counting the pin as motion -- otherwise the portal pass would see a segment from
        // wherever physics took the player to here, and the head bob would ride on it.
        player.base.set_position(eye);
        player.base.velocity = crate::vector::Vector3::zero();
        player.set_look(yaw, pitch);
    }

    /// EXT: one frame of forced-perspective grab logic, plus the sounds it triggers, the
    /// footsteps the physics loop just took, and the sprint's field-of-view ease.
    ///
    /// Called from `run_frame` after the fixed-step physics loop, so the held object's
    /// transform is authoritative for this frame's draw -- and so the FOV set here is the one
    /// `Camera::set_size` picks up a few lines later for the same draw.
    fn ext_update(&self) {
        // The grab latch, set either by the E key at the top of run_frame or by the gamepad.
        // NOT read from key_press directly: EndFrame has already cleared it by this point.
        let grab_pressed = self.pad_grab.replace(false);

        let (cam_to_world, steps) = {
            let p = self.player.borrow();
            (p.cam_to_world(), p.steps())
        };
        let objects = self.v_objects.borrow();

        let mut ext = self.ext.borrow_mut();
        let ext = &mut *ext;
        crate::ext::grab::update(&objects, &cam_to_world, grab_pressed, &mut ext.grab);
        ext.fire_grab_sfx();
        ext.fire_footstep_sfx(steps);
        crate::ext::view::set_fov(ext.sprint.ease_fov(crate::ext::view::time()));
    }

    /// EXT: (re)create the portal framebuffers at the drawable's size -- see `portal_fbos`.
    /// A no-op on every frame the size has not changed. The old set is dropped before the new
    /// one is built so the two never coexist.
    fn ensure_portal_fbos(&self, width: i32, height: i32) {
        let w = width.clamp(1, GH_FBO_SIZE);
        let h = height.clamp(1, GH_FBO_SIZE);
        let mut fbos = self.portal_fbos.borrow_mut();
        if fbos.first().is_some_and(|f| f.width == w && f.height == h) {
            return;
        }
        fbos.clear();
        // PORT: `GH_MAX_RECURSION <= 1 ? 1 : GH_MAX_RECURSION - 1` (Portal.h:43): level 1's
        // portals draw pink and need no target.
        let n = if GH_MAX_RECURSION <= 1 { 1 } else { GH_MAX_RECURSION - 1 };
        for _ in 0..n {
            fbos.push(crate::frame_buffer::FrameBuffer::new(&self.gl, w, h));
        }
    }

    /// EXT: reach the ported `Input` so the platform layer can write gamepad axes into it.
    pub fn with_input<R>(&self, f: impl FnOnce(&mut Input) -> R) -> R {
        f(&mut self.input.borrow_mut())
    }

    /// EXT: whether a menu (title or pause) currently owns input. The platform layer uses
    /// this to keep gameplay-only gamepad effects (grab, scene cycling, mute) from leaking
    /// underneath an open menu.
    pub fn menu_is_open(&self) -> bool {
        self.ext.borrow().menu.is_open()
    }

    /// EXT: latch a gamepad grab press until the next frame consumes it.
    pub fn set_pad_grab(&self) {
        self.pad_grab.set(true);
    }

    /// EXT: how many scenes are registered, so the platform layer can bound its cycling.
    pub fn scene_count(&self) -> usize {
        self.v_scenes.len()
    }

    /// EXT: step to the next or previous scene, wrapping at both ends.
    /// Bound to the DualSense shoulder buttons and D-pad.
    pub fn cycle_scene(&self, delta: i32) {
        let n = self.v_scenes.len() as i32;
        if n == 0 {
            return;
        }
        let cur = self.cur_scene_ix.get() as i32;
        let next = (cur + delta).rem_euclid(n);
        self.load_scene(next as usize);
    }

    /// EXT: reach the audio mixer (mute toggle, volume).
    pub fn with_audio<R>(&self, f: impl FnOnce(&mut crate::ext::audio::Audio) -> R) -> R {
        f(&mut self.ext.borrow_mut().audio)
    }

    /// EXT: whether the player is currently carrying something -- used for the HUD hint.
    pub fn is_holding(&self) -> bool {
        self.ext.borrow().grab.held.is_some()
    }
}
