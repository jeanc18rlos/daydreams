// PORT: Main.cpp's WinMain (Main.cpp:4-16) is replaced by a plain `fn main`; the
// _DEBUG-only AllocConsole/AttachConsole/freopen block (Main.cpp:6-11) has no equivalent
// -- a Rust binary already has stdout attached.
//
// PORT: this file also absorbs the whole Win32 platform layer that used to live in Engine.cpp:
// CreateGLWindow (Engine.cpp:330-413), WindowProc (Engine.cpp:272-328), SetupInputs
// (Engine.cpp:440-467), ConfineCursor (Engine.cpp:469-475), ToggleFullscreen
// (Engine.cpp:485-501) and the PeekMessage pump inside Run (Engine.cpp:77-127). It is the one
// part of the port that is a genuine rewrite rather than a transcription: winit owns the window
// and the event loop, glutin owns the GL context and the swap chain.

mod vector;
mod game_header;
mod timer;
mod sphere;
mod input;
mod camera;
mod collider;
mod mesh;
mod shader;
mod texture;
mod frame_buffer;
mod resources;
mod object;
mod physical;
mod player;
mod portal;
mod scene;
mod props;
mod engine;
mod level1;
mod level2;
mod level3;
mod level4;
mod level5;
mod level6;
// EXT: extension scenes (key 8 onward).
mod level7;
mod level8;
mod level9;
mod level10;
mod level11;
mod level12;
mod level13;
mod level14;
mod level15;
mod level16;

// EXT: new work beyond the port -- grab mechanic, audio, gamepad.
mod ext;

use std::num::NonZeroU32;
use std::rc::Rc;

use glutin::config::{Config, ConfigTemplateBuilder, GlConfig};
use glutin::context::{
    ContextApi, ContextAttributesBuilder, GlProfile, NotCurrentContext, PossiblyCurrentContext,
    Version,
};
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::{Surface, SwapInterval, WindowSurface};
use glutin_winit::{DisplayBuilder, GlWindow};
use raw_window_handle::HasWindowHandle;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize, PhysicalPosition};
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowAttributes, WindowId};

use crate::engine::Engine;
use crate::game_header::{
    GH_HIDE_MOUSE, GH_SCREEN_HEIGHT, GH_SCREEN_WIDTH, GH_SCREEN_X, GH_SCREEN_Y,
    GH_START_FULLSCREEN, GH_TITLE,
};
use crate::input::key_index;

// PORT: replaces the WNDCLASSEX registration plus the CreateWindowEx call
// (was: Engine.cpp:331-368). GH_CLASS has no winit counterpart; the window class is a Win32
// concept. The size is LOGICAL, matching CreateWindowEx's DPI-aware pixel size after
// SetProcessDPIAware (Engine.cpp:33).
fn window_attributes() -> WindowAttributes {
    let attributes = Window::default_attributes()
        .with_title(GH_TITLE)
        .with_inner_size(LogicalSize::new(GH_SCREEN_WIDTH, GH_SCREEN_HEIGHT))
        .with_position(LogicalPosition::new(GH_SCREEN_X, GH_SCREEN_Y));

    // if (GH_START_FULLSCREEN) { ToggleFullscreen(); }   (Engine.cpp:403-405)
    // EXT: `--windowed` overrides the constant so dev runs (and parallel headless screenshot
    // jobs) do not each take the whole display.
    if start_fullscreen() {
        attributes.with_fullscreen(Some(Fullscreen::Borderless(None)))
    } else {
        attributes
    }
}

// PORT: replaces ChoosePixelFormat + the PIXELFORMATDESCRIPTOR (Engine.cpp:377-391). The
// original asks for a 32-bit color, 32-bit depth, double-buffered RGBA format; here the template
// asks for the same double-buffered RGBA with a depth buffer and the picker prefers the deepest
// depth buffer offered. 32-bit depth is not an option on every driver (macOS CGL tops out at 24
// plus 8 stencil), so the request is for 24 and the picker takes whatever is largest.
fn gl_config_picker(configs: Box<dyn Iterator<Item = Config> + '_>) -> Config {
    configs
        .reduce(|accum, config| {
            if config.depth_size() > accum.depth_size() {
                config
            } else {
                accum
            }
        })
        .expect("no suitable GL config found")
}

// PORT: replaces wglCreateContext (Engine.cpp:400), which on Windows hands back a legacy
// compatibility context. This asks for 3.3 Core explicitly.
//
// NOTE: glutin's macOS CGL backend ignores both the requested version and the profile and always
// returns a 4.1 Core context. That is expected and fine -- 4.1 Core is a superset of 3.3 Core.
// There is deliberately no lower-version fallback chain here: on macOS a "successful" fallback
// would prove nothing, since the backend never honoured the request in the first place.
fn create_gl_context(window: &Window, gl_config: &Config) -> NotCurrentContext {
    let raw_window_handle = window.window_handle().ok().map(|handle| handle.as_raw());

    let context_attributes = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
        .with_profile(GlProfile::Core)
        .build(raw_window_handle);

    unsafe {
        gl_config
            .display()
            .create_context(gl_config, &context_attributes)
            .expect("failed to create an OpenGL 3.3 core context")
    }
}

// PORT: replaces `if (GH_HIDE_MOUSE) { ShowCursor(FALSE); }` (Engine.cpp:406-408) together with
// the ClipCursor half of cursor confinement. Returns true when the pointer was LOCKED rather
// than merely confined -- the distinction matters in `confine_cursor` below.
//
// macOS supports Locked but not Confined; X11 is the reverse; Windows wants Confined.
fn grab_cursor(window: &Window) -> bool {
    match window.set_cursor_grab(CursorGrabMode::Locked) {
        Ok(()) => true,
        Err(_) => {
            if let Err(err) = window.set_cursor_grab(CursorGrabMode::Confined) {
                eprintln!("Warning: could not grab the cursor: {err}");
            }
            false
        }
    }
}

// void Engine::ConfineCursor()   (Engine.cpp:469-475)
//
// PORT: the C++ warps the pointer to the window centre every single frame. That is only done
// here on the Confined fallback path. On macOS, winit's `set_cursor_position` finishes by
// re-associating the mouse cursor with the pointer, which SILENTLY UNDOES a prior Locked grab --
// so warping on the Locked path would break the mouse look after exactly one frame.
fn confine_cursor(window: &Window, cursor_locked: bool) {
    if GH_HIDE_MOUSE && !cursor_locked {
        let size = window.inner_size();
        let _ = window.set_cursor_position(PhysicalPosition::new(
            size.width as f64 / 2.0,
            size.height as f64 / 2.0,
        ));
    }
}

// PORT: the glutin `DisplayBuilder`/`Init` two-phase dance. On most platforms `resumed` fires
// once, but Android tears the surface down and re-creates it, so the display and the context are
// built only the first time through.
enum GlDisplayCreationState {
    Builder(DisplayBuilder),
    Init,
}

struct AppState {
    gl_surface: Surface<WindowSurface>,
    // NOTE: the Window must be dropped after every resource created from its raw window handle.
    window: Window,
}

struct App {
    // PORT: Engine holds every GL-owning object, and their Drop impls issue real GL calls, so it
    // is declared first and therefore dropped first -- while the context is still current.
    engine: Option<Engine>,
    gl: Option<Rc<glow::Context>>,
    state: Option<AppState>,
    gl_context: Option<PossiblyCurrentContext>,

    template: ConfigTemplateBuilder,
    display_state: GlDisplayCreationState,
    gl_config: Option<Config>,
    not_current_gl_context: Option<NotCurrentContext>,

    // PORT: `bool isFullscreen` (Engine.h:46).
    is_fullscreen: bool,
    // PORT: no C++ counterpart; see grab_cursor / confine_cursor.
    cursor_locked: bool,
    // PORT: replaces reading VK_MENU state for the WM_SYSKEYDOWN path (Engine.cpp:305-310).
    modifiers: ModifiersState,

    // PORT: `LONG iWidth / iHeight` (Engine.h:44-45), which WM_SIZE keeps up to date
    // (Engine.cpp:289-293). These are PHYSICAL pixels: glutin's macOS surface sets
    // setWantsBestResolutionOpenGLSurface(true), so the GL drawable is the physical size --
    // typically 2x the logical window on a retina display. Feeding logical sizes to glViewport
    // and Camera::SetSize would render into the bottom-left quarter of the window.
    i_width: i32,
    i_height: i32,

    // EXT: DualSense / gamepad polling. Finishes the `//TODO:` CodeParade left in
    // Input::UpdateRaw (Input.cpp:43-45) after registering the devices (Engine.cpp:454-465).
    gamepads: ext::gamepad::Gamepads,
    // EXT: dev flags (see parse_dev_args).
    dev: DevArgs,
}

impl App {
    fn new(template: ConfigTemplateBuilder, display_builder: DisplayBuilder) -> App {
        App {
            engine: None,
            gl: None,
            state: None,
            gl_context: None,
            template,
            display_state: GlDisplayCreationState::Builder(display_builder),
            gl_config: None,
            not_current_gl_context: None,
            // PORT: seeded from GH_START_FULLSCREEN rather than hard-coded false, because
            // `window_attributes` honours the same constant. If the two disagreed, the first
            // Alt+Enter would try to ENTER a fullscreen the window was already in and do
            // nothing visible.
            is_fullscreen: start_fullscreen(),
            cursor_locked: false,
            modifiers: ModifiersState::empty(),
            i_width: GH_SCREEN_WIDTH as i32,
            i_height: GH_SCREEN_HEIGHT as i32,
            // EXT:
            gamepads: ext::gamepad::Gamepads::new(),
            dev: parse_dev_args(),
        }
    }

    // void Engine::ToggleFullscreen()   (Engine.cpp:485-501)
    //
    // PORT: SetWindowLong(GWL_STYLE, WS_POPUP) + SetWindowPos(HWND_TOPMOST, 0, 0, screen) is
    // winit's borderless fullscreen; the windowed branch restores the original size and
    // position. iWidth/iHeight are NOT written here -- winit reports the new size through
    // WindowEvent::Resized, which is also where the GL surface gets resized.
    fn toggle_fullscreen(&mut self) {
        self.is_fullscreen = !self.is_fullscreen;
        let Some(state) = self.state.as_ref() else {
            return;
        };
        if self.is_fullscreen {
            state
                .window
                .set_fullscreen(Some(Fullscreen::Borderless(None)));
        } else {
            state.window.set_fullscreen(None);
            let _ = state
                .window
                .request_inner_size(LogicalSize::new(GH_SCREEN_WIDTH, GH_SCREEN_HEIGHT));
            state
                .window
                .set_outer_position(LogicalPosition::new(GH_SCREEN_X, GH_SCREEN_Y));
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // PORT: replaces CreateGLWindow (Engine.cpp:330-413) in full.
        let (window, gl_config) =
            match std::mem::replace(&mut self.display_state, GlDisplayCreationState::Init) {
                GlDisplayCreationState::Builder(display_builder) => {
                    let (window, gl_config) = match display_builder.build(
                        event_loop,
                        self.template.clone(),
                        gl_config_picker,
                    ) {
                        Ok((window, gl_config)) => (
                            window.expect("DisplayBuilder returned no window"),
                            gl_config,
                        ),
                        Err(err) => {
                            eprintln!("Failed to create a window: {err}");
                            event_loop.exit();
                            return;
                        }
                    };
                    self.not_current_gl_context = Some(create_gl_context(&window, &gl_config));
                    self.gl_config = Some(gl_config.clone());
                    (window, gl_config)
                }
                GlDisplayCreationState::Init => {
                    let gl_config = self.gl_config.clone().expect("no stored GL config");
                    let window =
                        glutin_winit::finalize_window(event_loop, window_attributes(), &gl_config)
                            .expect("failed to re-create the window");
                    (window, gl_config)
                }
            };

        let attrs = window
            .build_surface_attributes(Default::default())
            .expect("failed to build the surface attributes");
        let gl_surface = unsafe {
            gl_config
                .display()
                .create_window_surface(&gl_config, &attrs)
                .expect("failed to create the window surface")
        };

        // PORT: replaces wglMakeCurrent(hDC, hRC) (Engine.cpp:401).
        let gl_context = self
            .not_current_gl_context
            .take()
            .unwrap_or_else(|| create_gl_context(&window, &gl_config))
            .make_current(&gl_surface)
            .expect("failed to make the GL context current");

        // PORT: replaces glewInit() (Engine.cpp:417) -- glow resolves the entry points itself.
        if self.gl.is_none() {
            let gl_display = gl_config.display();
            let gl = unsafe {
                glow::Context::from_loader_function_cstr(|s| gl_display.get_proc_address(s))
            };
            self.gl = Some(Rc::new(gl));
        }

        //Attempt to enalbe vsync (if failure then oh well)
        // PORT: wglSwapIntervalEXT(1) (Engine.cpp:431); the original's typo is kept above.
        if let Err(err) = gl_surface.set_swap_interval(
            &gl_context,
            SwapInterval::Wait(NonZeroU32::new(1).unwrap()),
        ) {
            eprintln!("Error setting vsync: {err:?}");
        }

        if GH_HIDE_MOUSE {
            window.set_cursor_visible(false);
            self.cursor_locked = grab_cursor(&window);
        }

        // PORT: the initial iWidth/iHeight, which WM_SIZE would otherwise deliver first
        // (Engine.cpp:352-353 sets them to the LOGICAL constants; these are physical).
        let size = window.inner_size();
        if size.width != 0 && size.height != 0 {
            self.i_width = size.width as i32;
            self.i_height = size.height as i32;
        }

        // PORT: the tail of the Engine ctor (Engine.cpp:38-51) plus Run's prologue
        // (Engine.cpp:70-73). SetupInputs (Engine.cpp:440-467) has no equivalent: winit's
        // DeviceEvent::MouseMotion already delivers what RegisterRawInputDevices asked for.
        if self.engine.is_none() {
            let engine = Engine::new(self.gl.as_ref().unwrap());
            engine.start_run();
            // EXT: dev flags -- direct scene start and/or screenshot-and-quit.
            if self.dev.scene.is_some() || self.dev.shot.is_some() {
                engine.start_direct(
                    self.dev.scene,
                    self.dev.shot.clone(),
                    self.dev.frames,
                    self.dev.yaw,
                    self.dev.pitch,
                    self.dev.pos,
                );
            }
            self.engine = Some(engine);
        }

        self.gl_context = Some(gl_context);
        self.state = Some(AppState { gl_surface, window });
    }

    // PORT: this is the whole of Engine::WindowProc (Engine.cpp:272-328). WM_SYSCOMMAND /
    // WM_PAINT have no winit equivalent and are dropped.
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            // case WM_SIZE:   (Engine.cpp:289-293)
            WindowEvent::Resized(size) => {
                // PORT: guard against a zero dimension; the first frame (and a minimised
                // window) can report 0, and NonZeroU32::new would panic.
                if size.width != 0 && size.height != 0 {
                    self.i_width = size.width as i32;
                    self.i_height = size.height as i32;
                    if let (Some(state), Some(gl_context)) =
                        (self.state.as_ref(), self.gl_context.as_ref())
                    {
                        state.gl_surface.resize(
                            gl_context,
                            NonZeroU32::new(size.width).unwrap(),
                            NonZeroU32::new(size.height).unwrap(),
                        );
                    }
                }
            }

            // case WM_CLOSE:   (Engine.cpp:322-324)
            WindowEvent::CloseRequested => event_loop.exit(),

            // PORT: no C++ counterpart -- alt-tabbing away drops the grab on every platform,
            // so it is re-applied when the window regains focus.
            WindowEvent::Focused(focused) => {
                if focused && GH_HIDE_MOUSE {
                    let locked = self.state.as_ref().map(|state| {
                        state.window.set_cursor_visible(false);
                        grab_cursor(&state.window)
                    });
                    if let Some(locked) = locked {
                        self.cursor_locked = locked;
                    }
                }
            }

            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),

            // case WM_KEYDOWN / WM_SYSKEYDOWN / WM_KEYUP:   (Engine.cpp:295-314)
            WindowEvent::KeyboardInput { event, .. } => {
                //Ignore repeat keys
                // PORT: `if (lParam & 0x40000000) { return 0; }` (Engine.cpp:297).
                if event.repeat {
                    return;
                }
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                match event.state {
                    ElementState::Pressed => {
                        // if (wParam == VK_ESCAPE) { PostQuitMessage(0); }   (Engine.cpp:300-302)
                        // EXT: Escape no longer quits directly (Engine.cpp:300-302); it is
                        // routed into key slot 27 and opens the pause menu, which owns Exit.
                        if code == KeyCode::Escape {
                            if let Some(engine) = self.engine.as_ref() {
                                engine.with_input(|inp| {
                                    if event.state == ElementState::Pressed {
                                        inp.key[27] = true;
                                        inp.key_press[27] = true;
                                    } else {
                                        inp.key[27] = false;
                                    }
                                });
                            }
                            return;
                        }
                        // case WM_SYSKEYDOWN: if (wParam == VK_RETURN) { ToggleFullscreen(); }
                        // (Engine.cpp:305-310) -- WM_SYSKEYDOWN means alt is held.
                        if (code == KeyCode::Enter || code == KeyCode::NumpadEnter)
                            && self.modifiers.alt_key()
                        {
                            self.toggle_fullscreen();
                            return;
                        }
                        if let (Some(ix), Some(engine)) = (key_index(code), self.engine.as_ref()) {
                            let mut input = engine.input().borrow_mut();
                            input.key[ix] = true;
                            input.key_press[ix] = true;
                        }
                    }
                    ElementState::Released => {
                        if let (Some(ix), Some(engine)) = (key_index(code), self.engine.as_ref()) {
                            engine.input().borrow_mut().key[ix] = false;
                        }
                    }
                }
            }

            // PORT: replaces the RI_MOUSE_*_BUTTON_DOWN/UP half of Input::UpdateRaw
            // (Input.cpp:28-42).
            WindowEvent::MouseInput { state, button, .. } => {
                let ix = match button {
                    MouseButton::Left => Some(0usize),
                    MouseButton::Middle => Some(1usize),
                    MouseButton::Right => Some(2usize),
                    _ => None,
                };
                if let (Some(ix), Some(engine)) = (ix, self.engine.as_ref()) {
                    engine
                        .input()
                        .borrow_mut()
                        .set_mouse_button(ix, state == ElementState::Pressed);
                }
            }

            // PORT: WindowEvent::CursorMoved is deliberately IGNORED. It reports an absolute
            // position that stops changing once the cursor is locked or warped back to the
            // centre; the mouse look must be driven by raw relative deltas, which is what
            // DeviceEvent::MouseMotion delivers (and what RegisterRawInputDevices asked for).
            _ => (),
        }
    }

    // case WM_INPUT: ... input.UpdateRaw(...)   (Engine.cpp:316-320)
    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event {
            if let Some(engine) = self.engine.as_ref() {
                engine
                    .input()
                    .borrow_mut()
                    .add_mouse_motion(delta.0 as f32, delta.1 as f32);
            }
        }
    }

    // PORT: the `else` branch of Run's message loop (Engine.cpp:86-126). ControlFlow::Poll makes
    // winit call this every time the queue drains, which is exactly when PeekMessage returns
    // false.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // EXT: poll the gamepad before the frame runs, so its axes are visible to this frame's
        // fixed-step updates. Polled here rather than in device_event because gilrs keeps its
        // own event queue, independent of winit's.
        let pad = match self.engine.as_ref() {
            Some(engine) => {
                let pad = {
                    let pads = &mut self.gamepads;
                    engine.with_input(|input| pads.poll(input))
                };
                // EXT: gameplay-only pad effects are gated on the menu being closed. South is
                // bound to both `grab` and `menu_confirm`, and the D-pad to both scene cycling
                // and menu navigation; the menu's own copy of these events arrives via
                // set_pad_events below and is the only path that may act while it is open.
                if !engine.menu_is_open() {
                    if pad.grab {
                        engine.set_pad_grab();
                    }
                    if pad.next_scene {
                        engine.cycle_scene(1);
                    }
                    if pad.prev_scene {
                        engine.cycle_scene(-1);
                    }
                    if pad.toggle_mute {
                        engine.with_audio(|a| a.toggle_mute());
                    }
                }
                engine.set_pad_events(pad);
                if engine.quit_requested() {
                    event_loop.exit();
                    return;
                }
                pad
            }
            None => return,
        };
        // EXT: the pad no longer carries a quit of its own -- it reaches EXIT through the pause
        // menu now (see the binding notes in ext/gamepad.rs). `quit_requested` above is still
        // the one exit path, and the menu is what sets it.
        if pad.toggle_fullscreen {
            self.toggle_fullscreen();
        }

        let (Some(state), Some(gl_context), Some(engine)) = (
            self.state.as_ref(),
            self.gl_context.as_ref(),
            self.engine.as_ref(),
        ) else {
            return;
        };

        //Confine the cursor
        confine_cursor(&state.window, self.cursor_locked);

        engine.run_frame(self.i_width, self.i_height);

        // SwapBuffers(hDC);   (Engine.cpp:125)
        if let Err(err) = state.gl_surface.swap_buffers(gl_context) {
            eprintln!("swap_buffers failed: {err}");
        }
    }

    // DestroyGLObjects();   (Engine.cpp:129)
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(engine) = self.engine.as_ref() {
            engine.destroy_gl_objects();
        }
    }
}

/// EXT: dev-tooling flags. `--scene N` skips the title and loads scene N directly;
/// `--shot path.bmp` saves a screenshot after `--frames K` (default 90) frames and quits;
/// `--yaw deg` / `--pitch deg` aim the camera. Used to iterate on shaders without a human.
/// `--shot` on its own photographs whatever the game boots into -- the title screen.
#[derive(Clone, Debug, Default)]
struct DevArgs {
    scene: Option<usize>,
    shot: Option<String>,
    frames: i32,
    yaw: f32,
    pitch: f32,
    pos: Option<[f32; 3]>,
}

/// EXT: whether the window opens fullscreen: `GH_START_FULLSCREEN` unless `--windowed` is given.
fn start_fullscreen() -> bool {
    GH_START_FULLSCREEN && !std::env::args().any(|a| a == "--windowed")
}

fn parse_dev_args() -> DevArgs {
    let mut out = DevArgs { frames: 90, ..Default::default() };
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        let next = args.get(i + 1).cloned();
        match args[i].as_str() {
            // EXT: regenerate the toroidal meadow's tile mesh from ext::terrain::height and
            // exit. The mesh must be a file because colliders live on Mesh, which only loads
            // from disk -- but it is derived, so it is generated rather than authored.
            "--gen-terrain" => {
                if let Err(e) = crate::ext::terrain::generate() {
                    eprintln!("--gen-terrain: {e}");
                    std::process::exit(1);
                }
                std::process::exit(0);
            }
            "--scene" => out.scene = next.as_deref().and_then(|v| v.parse().ok()),
            "--shot" => out.shot = next.clone(),
            "--frames" => out.frames = next.as_deref().and_then(|v| v.parse().ok()).unwrap_or(90),
            "--yaw" => out.yaw = next.as_deref().and_then(|v| v.parse().ok()).unwrap_or(0.0),
            "--pitch" => out.pitch = next.as_deref().and_then(|v| v.parse().ok()).unwrap_or(0.0),
            "--pos" => {
                // --pos x,y,z
                out.pos = next.as_deref().and_then(|v| {
                    let p: Vec<f32> = v.split(',').filter_map(|t| t.trim().parse().ok()).collect();
                    (p.len() == 3).then(|| [p[0], p[1], p[2]])
                });
            }
            _ => { i += 1; continue; }
        }
        i += 2;
    }
    out
}

fn main() {
    let event_loop = EventLoop::new().expect("failed to create the event loop");
    // PORT: Poll, not Wait -- the C++ loop renders whenever PeekMessage finds nothing to do
    // (Engine.cpp:78/86).
    event_loop.set_control_flow(ControlFlow::Poll);

    let template = ConfigTemplateBuilder::new()
        .with_alpha_size(8)
        .with_depth_size(24)
        .with_transparency(false);
    let display_builder = DisplayBuilder::new().with_window_attributes(Some(window_attributes()));

    let mut app = App::new(template, display_builder);
    if let Err(err) = event_loop.run_app(&mut app) {
        eprintln!("{err}");
    }
}
