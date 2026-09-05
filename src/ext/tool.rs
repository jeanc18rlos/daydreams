//! EXT: the exploration tools -- eight handheld instruments the player can take, pocket and
//! use. Not part of the C++ port.
//!
//! A [`Tool`] is a `Grabbable`-shaped prop exactly as the key is (`ext/key.rs`): a
//! `Physical` with one hit sphere, found by the grab down the crosshair, carried by the
//! engine's physics while loose, stowable into the inventory (`F`) and handed back out of
//! it. Its mesh is a named part of `Meshes/exploration_tools.glb` (Elbolillo's CC0
//! "Exploration objects" pack), drawn through the shared `gltfpbr` shader so it grades with
//! the scene like every other GLB part.
//!
//! # Use
//!
//! While a tool is in hand it claims the frame's E press on the same standing-offer channel
//! the key uses -- the engine asks [`wants_use`] after the elevator and the key and before
//! the grab, so E operates the tool instead of dropping it (put it away with `F`). What the
//! press does is the tool's kind:
//!
//! * flashlights toggle a beam: while on, the tool raycasts down the crosshair each step and
//!   publishes the scene's light pool (`view::set_glow`) at whatever the beam lands on;
//! * the photo camera takes a photo -- a white burst of the same glow at the aim point, and
//!   a running count;
//! * everything else toggles a READING, shown on the hint line (`hint::insist`, the
//!   held-item channel): EMF bars, a falling temperature, the spirit box's words, the
//!   thermal camera's signature, the recorder's tape clock.
//!
//! The readings all measure one thing: the PRESENCE -- whatever the scene said walks its
//! ground ([`report_presence`], fed by the mannequin in level32). Out on the street the
//! instruments read calm; near the walker the bars fill, the air drops toward freezing and
//! the spirit box starts choosing words. None of it is a mechanic yet -- the tools are
//! honest instruments over one signal, and that is all they claim to be.
//!
//! # The channel rules (learned the hard way by the key)
//!
//! `WANTS_USE` is a STANDING answer cleared by whoever stops offering -- a frame that runs
//! zero fixed steps must still read the last true answer (key.rs:79-87 documents the bug).
//! Every way of leaving the hand -- release, stow -- clears the offer and any pending press,
//! and turns the tool off, so a pocketed flashlight is not still claiming E or lighting the
//! lawn from inside a pocket.

use crate::camera::Camera;
use crate::ext::audio::{self, Sfx};
use crate::ext::gltf_model::{Anchor, Frame, GltfModel, Load, PartSpec};
use crate::ext::{hint, raycast, view};
use crate::game_header::GH_DT;
use crate::object::{Object, ObjectT, RenderCtx, UpdateCtx};
use crate::physical::Physical;
use crate::resources::Resources;
use crate::shader::Shader;
use crate::sphere::Sphere;
use crate::vector::{Matrix4, Vector3};
use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

pub const MODEL: &str = "Meshes/exploration_tools.glb";

/// Every tool of the GLB in one spec: the acquire cache keys on the whole `Load`, so all
/// eight tools of a scene share one parse. Each part names its own node -- an empty `roots`
/// would gather the whole file into every part.
const SPEC_PARTS: [PartSpec<'static>; 8] = [
    named("Flashlight", &["Flashlight"]),
    named("Flashlight_Poquet", &["Flashlight_Poquet"]),
    named("EMF_Detector", &["EMF_Detector"]),
    named("Thermometer", &["Thermometer"]),
    named("Spirit_Box", &["Spirit_Box"]),
    named("Thermal_Camera", &["Thermal_Camera"]),
    named("Photo_Camera", &["Photo_Camera"]),
    named("Voice_Recorder", &["Voice_Recorder"]),
];

const fn named(name: &'static str, roots: &'static [&'static str]) -> PartSpec<'static> {
    PartSpec { name, roots, skip: &[], frame: Frame::Scene, anchor: Anchor::Hinge }
}

pub fn load_spec() -> Load<'static> {
    Load {
        path: MODEL,
        parts: &SPEC_PARTS,
        fit: crate::ext::gltf_model::Fit::Identity,
        max_map: 512,
        translucent: &[],
        metallic_override: &[],
        cut_boxes: &[],
    }
}

/// How far the beam and the aim reach, in metres.
const BEAM_REACH: f32 = 18.0;
/// The cone: full brightness inside `SPOT_INNER` degrees off the aim, gone by `SPOT_OUTER`.
/// A real torch has a hot centre and a soft corona, and the gap between the two is what
/// makes the beam legible on a wall rather than a disc of paint.
const SPOT_INNER: f32 = 13.0;
const SPOT_OUTER: f32 = 27.0;
/// Slightly warm, like every torch anyone has held.
const SPOT_COLOR: [f32; 3] = [1.00, 0.94, 0.82];
/// Where a held tool sits, in CAMERA space at `p_scale == 1`: right of the crosshair, below
/// it, and a little ahead of the eye. It is a carry pose, not a placement -- see
/// [`Tool::carry_fixed`].
const CARRY: Vector3 = Vector3 { x: 0.30, y: -0.24, z: -0.62 };

/// How long the camera's flash lights the aim point, in seconds.
const FLASH_SECS: f32 = 0.35;
/// Within this range the presence registers on the instruments at all.
const SENSE_RANGE: f32 = 24.0;

thread_local! {
    /// Whether the held tool wants the frame's E press. Standing answer, key.rs rules.
    static WANTS_USE: Cell<bool> = const { Cell::new(false) };
    /// The frame's E, handed over by the engine; taken by the tool's next step.
    static PRESS: Cell<bool> = const { Cell::new(false) };
    /// Where the scene's presence last stood, if the scene has one. The walker re-reports
    /// every step; a scene without one clears it at load.
    static PRESENCE: Cell<Option<[f32; 3]>> = const { Cell::new(None) };
}

pub fn wants_use() -> bool {
    WANTS_USE.with(Cell::get)
}

pub fn press() {
    PRESS.with(|p| p.set(true));
}

fn offer_use(on: bool) {
    WANTS_USE.with(|w| w.set(on));
}

fn take_press() -> bool {
    PRESS.with(Cell::take)
}

/// The walker's step says where it stands; the instruments read it.
pub fn report_presence(pos: Vector3) {
    PRESENCE.with(|p| p.set(Some([pos.x, pos.y, pos.z])));
}

/// Scene load hygiene: clear the presence (or the last scene's ghost lingers on the
/// instruments) and the use channel (a tool held at scene switch has no step left to
/// withdraw its own offer).
pub fn reset() {
    PRESENCE.with(|p| p.set(None));
    offer_use(false);
    take_press();
}

fn presence() -> Option<Vector3> {
    PRESENCE.with(Cell::get).map(|p| Vector3::new(p[0], p[1], p[2]))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToolKind {
    Flashlight,
    PocketLight,
    Emf,
    Thermometer,
    SpiritBox,
    ThermalCamera,
    PhotoCamera,
    Recorder,
}

impl ToolKind {
    pub const ALL: [ToolKind; 8] = [
        ToolKind::Flashlight,
        ToolKind::PocketLight,
        ToolKind::Emf,
        ToolKind::Thermometer,
        ToolKind::SpiritBox,
        ToolKind::ThermalCamera,
        ToolKind::PhotoCamera,
        ToolKind::Recorder,
    ];

    pub fn part(self) -> &'static str {
        match self {
            ToolKind::Flashlight => "Flashlight",
            ToolKind::PocketLight => "Flashlight_Poquet",
            ToolKind::Emf => "EMF_Detector",
            ToolKind::Thermometer => "Thermometer",
            ToolKind::SpiritBox => "Spirit_Box",
            ToolKind::ThermalCamera => "Thermal_Camera",
            ToolKind::PhotoCamera => "Photo_Camera",
            ToolKind::Recorder => "Voice_Recorder",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ToolKind::Flashlight => "TORCH",
            ToolKind::PocketLight => "PENLIGHT",
            ToolKind::Emf => "EMF",
            ToolKind::Thermometer => "THERMO",
            ToolKind::SpiritBox => "SPIRITBOX",
            ToolKind::ThermalCamera => "THERMAL",
            ToolKind::PhotoCamera => "CAMERA",
            ToolKind::Recorder => "RECORDER",
        }
    }

    fn take_hint(self) -> &'static str {
        match self {
            ToolKind::Flashlight => "E  TAKE THE FLASHLIGHT",
            ToolKind::PocketLight => "E  TAKE THE PENLIGHT",
            ToolKind::Emf => "E  TAKE THE EMF READER",
            ToolKind::Thermometer => "E  TAKE THE THERMOMETER",
            ToolKind::SpiritBox => "E  TAKE THE SPIRIT BOX",
            ToolKind::ThermalCamera => "E  TAKE THE THERMAL CAMERA",
            ToolKind::PhotoCamera => "E  TAKE THE CAMERA",
            ToolKind::Recorder => "E  TAKE THE RECORDER",
        }
    }

    fn on_hint(self) -> &'static str {
        match self {
            ToolKind::Flashlight => "E  SWITCH ON THE FLASHLIGHT",
            ToolKind::PocketLight => "E  SWITCH ON THE PENLIGHT",
            ToolKind::Emf => "E  SWITCH ON THE EMF READER",
            ToolKind::Thermometer => "E  READ THE THERMOMETER",
            ToolKind::SpiritBox => "E  SWITCH ON THE SPIRIT BOX",
            ToolKind::ThermalCamera => "E  SWITCH ON THE THERMAL CAMERA",
            ToolKind::PhotoCamera => "E  TAKE A PHOTO",
            ToolKind::Recorder => "E  START RECORDING",
        }
    }

    fn off_hint(self) -> &'static str {
        match self {
            ToolKind::Flashlight => "E  SWITCH OFF THE FLASHLIGHT",
            ToolKind::PocketLight => "E  SWITCH OFF THE PENLIGHT",
            _ => "E  SWITCH OFF",
        }
    }

    /// The beam strength a lit tool pushes into `view::set_glow`; zero for the rest.
    fn beam(self) -> f32 {
        match self {
            ToolKind::Flashlight => 1.1,
            ToolKind::PocketLight => 0.55,
            _ => 0.0,
        }
    }
}

/// What the spirit box says, when it says anything. Chosen by the clock, held long enough
/// to read; the pool is small on purpose -- a PSX radio does not have a vocabulary.
const WORDS: [&str; 8] = ["COLD", "BEHIND", "STAY", "DOOR", "BELOW", "NO", "HELLO", "LEAVE"];

pub struct Tool {
    base: Physical,
    kind: ToolKind,
    model: Rc<GltfModel>,
    shader: Rc<Shader>,
    /// Model-space centre of this tool's part: the pack parks each tool at its own showroom
    /// spot, and the draw cancels it so the mesh sits on `base.pos`.
    anchor: Vector3,
    held: bool,
    active: bool,
    /// Photos taken and tape recorded survive pocketing; they are the tool's own.
    photos: u32,
    tape_secs: f32,
    /// When the camera last flashed, on the frame clock; NAN long ago.
    flash_at: f32,
    /// Whether the flash is currently lighting the glow slot: a latch, not a clock
    /// window, so a stow mid-flash or a hitched frame still gets the clear.
    flash_lit: bool,
    /// Which word the spirit box is on and when it changed.
    word_ix: usize,
    word_at: f32,
    /// The hint line the instrument is showing, rebuilt by `update`, published by `draw`:
    /// a fixed step writes at most 500 times a second, but an uncapped frame can render
    /// between steps, and a hint published only from steps flickers there.
    line: String,
}

impl Tool {
    pub fn new(gl: &Rc<glow::Context>, res: &Resources, kind: ToolKind) -> Rc<RefCell<Tool>> {
        let model = GltfModel::acquire(gl, &load_spec());
        let b = model.bounds(kind.part());
        let centre = Vector3::new((b[0] + b[1]) * 0.5, (b[2] + b[3]) * 0.5, (b[4] + b[5]) * 0.5);
        let half = Vector3::new(b[1] - b[0], b[3] - b[2], b[5] - b[4]) * 0.5;
        let mut base = Physical::new();
        base.drag = 0.002;
        base.friction = 0.1;
        // Exactly ONE hit sphere: the grab's marker for a grabbable, and what rests the
        // tool on whatever it is put down on. Radius from the part's own bulk.
        let radius = half.x.max(half.y).max(half.z).max(0.06);
        base.hit_spheres.push(Sphere::new_at(Vector3::zero(), radius));
        Rc::new(RefCell::new(Tool {
            base,
            kind,
            model,
            shader: res.acquire_shader("gltfpbr"),
            anchor: centre,
            held: false,
            active: false,
            photos: 0,
            tape_secs: 0.0,
            flash_at: f32::NAN,
            flash_lit: false,
            word_ix: 0,
            word_at: 0.0,
            line: String::new(),
        }))
    }

    /// Stand the tool at `pos` (its centre, so half its bulk above a surface it lies on).
    pub fn place(&mut self, pos: Vector3, yaw: f32) {
        self.base.set_position(pos);
        self.base.base.euler.y = yaw;
    }

    fn switch_off(&mut self) {
        if self.active && self.kind.beam() > 0.0 {
            view::set_glow(Vector3::zero(), 0.0);
            view::clear_spot();
        }
        if self.flash_lit {
            view::set_glow(Vector3::zero(), 0.0);
            self.flash_lit = false;
        }
        self.active = false;
    }

    /// The instrument line for this step, given how far the presence is.
    fn reading(&mut self, dist: Option<f32>, now: f32) -> String {
        match self.kind {
            ToolKind::Emf => {
                let bars = match dist {
                    Some(d) if d < SENSE_RANGE => {
                        (5.0 - (d / SENSE_RANGE) * 5.0).ceil().clamp(1.0, 5.0) as usize
                    }
                    _ => 0,
                };
                let mut s = String::from("EMF  ");
                for i in 0..5 {
                    s.push(if i < bars { '#' } else { '-' });
                }
                s
            }
            ToolKind::Thermometer => {
                // A mild night that curdles near the presence.
                let t = match dist {
                    Some(d) if d < SENSE_RANGE => 12.0 - 13.0 * (1.0 - d / SENSE_RANGE),
                    _ => 12.0,
                };
                format!("{t:.1} C")
            }
            ToolKind::SpiritBox => match dist {
                Some(d) if d < SENSE_RANGE * 0.5 => {
                    if now - self.word_at > 1.4 {
                        self.word_at = now;
                        // The clock picks; the box only remembers not to repeat itself.
                        self.word_ix =
                            (self.word_ix + 1 + (now * 7.0) as usize % (WORDS.len() - 1))
                                % WORDS.len();
                    }
                    format!("SPIRIT BOX  \"{}\"", WORDS[self.word_ix])
                }
                _ => String::from("SPIRIT BOX  ..STATIC.."),
            },
            ToolKind::ThermalCamera => match dist {
                Some(d) if d < SENSE_RANGE => {
                    format!("THERMAL  WARM SIGNATURE {d:.0} M")
                }
                _ => String::from("THERMAL  NO SIGNATURE"),
            },
            ToolKind::Recorder => {
                let m = (self.tape_secs / 60.0) as u32;
                let s = self.tape_secs as u32 % 60;
                format!("REC  {m:02}:{s:02}")
            }
            // The lights and the camera have no reading; their state shows elsewhere.
            _ => String::new(),
        }
    }
}

impl ObjectT for Tool {
    fn base(&self) -> &Object {
        &self.base.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base.base
    }

    /// An instrument is HELD, not placed: fixed size, fixed pose, pointing where you look
    /// (`ObjectT::carry_fixed`). The perspective carry belongs to cargo, not to a torch.
    fn carry_fixed(&self) -> Option<Vector3> {
        Some(CARRY)
    }

    fn update(&mut self, ctx: &UpdateCtx) {
        self.base.update();
        if !self.held {
            // NOT `offer_use(false)`: the channel is shared by every tool in the scene, and
            // an idle tool updating after the held one would stomp its standing offer --
            // last writer wins. Only whoever OWNS the offer withdraws it: `on_release` and
            // `on_stow` run precisely on the tool that was held.
            return;
        }
        // In hand, the tool always wants E -- that is what a tool is for. Dropping it is
        // the inventory's G, not the grab's release.
        offer_use(true);
        let pressed = take_press();
        let now = view::time();
        if pressed {
            match self.kind {
                ToolKind::PhotoCamera => {
                    self.photos += 1;
                    self.flash_at = now;
                    audio::request(Sfx::CameraSnap);
                }
                _ => {
                    audio::request(Sfx::ToolClick);
                    if self.active {
                        // switch_off must see active==true, or its glow clear is skipped
                        // and a full-strength pool stays frozen on the lawn.
                        self.switch_off();
                    } else {
                        self.active = true;
                    }
                }
            }
        }
        let dist = presence().map(|p| (p - ctx.player_pos).mag());
        // The beam: light wherever the crosshair lands, from the eye so the pool cannot be
        // cast behind a wall the player faces.
        if self.active && self.kind.beam() > 0.0 {
            // From the EYE, never from `RenderCtx::eye`: the third-person boom stands the
            // render camera metres behind the player (`ext/thirdperson.rs`), and a torch hung
            // off that would light the room over their own shoulder.
            let origin = ctx.cam_to_world.translation();
            let dir =
                ctx.cam_to_world.mul_direction(Vector3::new(0.0, 0.0, -1.0)).normalized_safe();
            // The cone itself -- what makes it read as a torch rather than a stain on a wall.
            view::set_spot(
                origin,
                dir,
                BEAM_REACH,
                SPOT_INNER,
                SPOT_OUTER,
                SPOT_COLOR,
                self.kind.beam(),
            );
            // And the old pool where the beam lands, kept: it is what puts a bright disc on
            // the surface the cone is aimed at, and the four shaders that read `glow` and not
            // the cone (the grasses) still get something.
            let lit = match raycast::raycast(ctx.scene, origin, dir, BEAM_REACH, None) {
                Some(hit) => hit.point + hit.normal * 0.05,
                None => origin + dir * BEAM_REACH,
            };
            view::set_glow(lit, self.kind.beam() * 0.55);
        }
        // The camera's flash is the same pool, brighter and briefer.
        if self.kind == ToolKind::PhotoCamera {
            let since = now - self.flash_at;
            if since >= 0.0 && since < FLASH_SECS {
                let origin = ctx.cam_to_world.translation();
                let dir =
                    ctx.cam_to_world.mul_direction(Vector3::new(0.0, 0.0, -1.0)).normalized_safe();
                let lit = match raycast::raycast(ctx.scene, origin, dir, BEAM_REACH, None) {
                    Some(hit) => hit.point + hit.normal * 0.05,
                    None => origin + dir * BEAM_REACH,
                };
                view::set_glow(lit, 2.2 * (1.0 - since / FLASH_SECS));
                self.flash_lit = true;
            } else if self.flash_lit {
                // A latch, not a clock window: however far the frame clock jumped, the
                // first step past the flash puts the night back.
                view::set_glow(Vector3::zero(), 0.0);
                self.flash_lit = false;
            }
        }
        if self.active && self.kind == ToolKind::Recorder {
            self.tape_secs += GH_DT;
        }
        // The hint line: a reading while the instrument runs, the switch prompt otherwise.
        // Built here, PUBLISHED from `draw` -- hints are per-rendered-frame, steps are not.
        self.line = if self.active {
            let r = self.reading(dist, now);
            if r.is_empty() {
                // A lit flashlight has no reading, but the offer still owns E -- say what
                // E does, or the crosshair shows TAKE prompts the press cannot perform.
                self.kind.off_hint().to_string()
            } else {
                r
            }
        } else if self.kind == ToolKind::PhotoCamera && self.photos > 0 {
            format!("E  TAKE A PHOTO  ({:02} TAKEN)", self.photos)
        } else {
            self.kind.on_hint().to_string()
        };
    }

    fn on_grab(&mut self) {
        self.held = true;
        self.base.velocity = Vector3::zero();
    }

    fn on_release(&mut self, velocity: Vector3) {
        self.held = false;
        self.line.clear();
        self.switch_off();
        offer_use(false);
        take_press();
        self.base.velocity = velocity;
    }

    fn on_stow(&mut self) {
        self.held = false;
        self.line.clear();
        self.switch_off();
        offer_use(false);
        take_press();
    }

    fn stow_label(&self) -> &'static str {
        self.kind.label()
    }

    fn pick_hint(&self) -> Option<&'static str> {
        Some(self.kind.take_hint())
    }

    fn on_collide(&mut self, push: Vector3) {
        self.base.on_collide(push);
    }
    fn as_physical(&self) -> Option<&Physical> {
        Some(&self.base)
    }
    fn as_physical_mut(&mut self) -> Option<&mut Physical> {
        Some(&mut self.base)
    }

    fn draw(&self, ctx: &RenderCtx, cam: &Camera, _fbo: Option<glow::Framebuffer>) {
        // The instrument line, once per rendered frame whatever the step count. insist()
        // is the held-item channel and outranks the crosshair text, so a live reading wins
        // over "E TAKE" prompts of whatever the player looks at -- intended. Last write
        // wins and portal passes rewrite the same string, which is harmless.
        if self.held && !self.line.is_empty() {
            hint::insist(self.line.clone());
        }
        // The pack parks each tool at its own showroom offset; draw about the part's centre
        // so the mesh sits on the hit sphere the grab and the physics use.
        let b = &self.base.base;
        let mut obj = Object::new();
        obj.euler = b.euler;
        obj.scale = b.scale * b.p_scale;
        obj.rot = b.rot;
        let rot = match b.rot {
            Some(r) => r,
            None => Matrix4::rot_y(b.euler.y) * Matrix4::rot_x(b.euler.x),
        };
        obj.pos = b.pos - rot.mul_direction(self.anchor * b.p_scale * b.scale.x);
        // An instrument lying in a dark room is what the beam is FOR: it answers the torch
        // with a rim of its own, so sweeping a room finds the kit rather than the wallpaper.
        // Not while held -- a torch does not light itself.
        if !self.held {
            view::set_shine([1.00, 0.86, 0.45], 1.0);
        }
        self.model.draw_part(self.kind.part(), &obj, &self.shader, cam, ctx);
        view::clear_shine();
    }
}
