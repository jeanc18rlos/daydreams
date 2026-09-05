//! EXT: title screen and pause menu. Not part of the C++ port.
//!
//! The original engine boots straight into scene 0 and has no menu of any kind: Escape quits
//! (Engine.cpp:183) and the number keys swap scenes -- which is what SWITCH LEVEL below
//! replaced. This module adds a small screen stack drawn with the 2D layer from `ui.rs`.
//!
//! # Two layouts, because there are two jobs
//!
//! The **title screen** is a poster. Its backdrop is a live scene composed as a photograph
//! (`ext::meadow::title_view` puts the open door in the right third), so the type is set
//! against the LEFT margin -- name high on the left, options in a column under it, EXIT alone
//! in the far corner -- and the door is left in the clear. It is washed by a gradient rather
//! than a flat dim, dark where the words are and transparent over the door, so the picture
//! survives being written on.
//!
//! Every **other** screen is a list to be read: heading centred, rows centred, an even wash
//! behind the lot. Nothing there is composed against anything, and centring is what makes a
//! list of settings read as a list of settings.
//!
//! # Contract with the engine (engine.rs `run_frame` / `render_menu_frame`)
//!
//! * `Menu::new()` starts on the TITLE screen -- the game opens to the menu, not to a level --
//!   and `is_title()` is true there, which is the engine's cue to run the intro level behind the
//!   menu as a live backdrop: door held open, camera parked at the shot that level composes.
//! * `open_pause()` is called by the engine when Escape is pressed while playing; on that
//!   screen `is_title()` is false so the frozen scene is drawn and dimmed behind the text.
//! * `update()` is called once per rendered frame with this frame's edge-triggered input
//!   (`Input::key_press` and `PadEvents`) and returns what the engine should do. It is the
//!   ONLY place state changes; `draw()` is pure.
//! * `draw()` runs between `Ui::begin`/`Ui::end`, which the engine brackets itself.
//!
//! # Why a screen enum plus a selection index
//!
//! Every screen is a fixed list of rows except level-select, whose row count is the scene
//! count handed in each frame. Keeping `Screen` as plain data and deriving the row list from it
//! (`row_count()` and the `*_ROWS` tables) means navigation, wrapping and drawing share one definition of "what is on the
//! screen", so they cannot disagree. The title screen and pause menu are *roots* (Back does
//! nothing / means Continue); Options and Credits are only reachable from the title and
//! return there, and the level list is only reachable from the pause menu and returns there.
//! That is also what `is_title()` reports: not the one screen, but which of the two stacks is
//! on top -- every screen in a stack wants the same world behind it.
//!
//! # Input edges
//!
//! `key_press` is an edge latch cleared by `Input::end_frame`, which runs inside the 500 Hz
//! fixed-step loop (Engine.cpp:114). The engine therefore calls `update` at the top of the
//! frame before that loop, which is the only moment the latches are valid for a whole frame.

use crate::ext::audio::{self, Sfx};
use crate::ext::gamepad::PadEvents;
use crate::ext::settings;
use crate::ext::ui::{Align, Ui, DIM, GOLD, WHITE};
use crate::input::Input;

/// The game's display name on the title screen.
///
/// Mixed case, alone among everything the menus set: the rest is upper case because Playpen
/// Sans has to hold its own as small white type over a photographed backdrop, and the name is
/// set in a display face at ten times the size, where the caps-only trick buys nothing and
/// costs the shape of the word.
///
/// It comes from `ui_atlas` rather than being written here, because the title atlas is baked
/// for exactly the characters of this string -- both are one edit in `tools/gen_ui.py`, and a
/// name changed on this side alone would have no glyphs to be drawn with. `GH_TITLE` reads the
/// same const for the OS window bar.
pub use crate::ext::ui_atlas::TITLE_TEXT;

// Layout, as fractions of the drawable height so the screen scales with the window.
/// Top of the title's line box.
const TITLE_Y: f32 = 0.14;
/// Title glyph size.
const TITLE_SIZE: f32 = 0.11;

// ── The title screen's own layout. Fractions of HEIGHT except `POSTER_LEFT`, which is a
// fraction of width: the left margin has to hold its distance from the edge of the frame at
// any aspect, while the type sizes scale with height like everything else here.
/// Left margin the name, the options and the marker all hang off.
const POSTER_LEFT: f32 = 0.105;
/// Top of the name's line box and its glyph size.
///
/// A poster's size, not a heading's, and set in the title face rather than the interface one.
/// Thirteen letters is a long name to run across a frame whose right third is the door, so the
/// size is what puts the last `m` a little past the middle: at 0.126 the wordmark is 0.80 of
/// the frame's height wide, so from the left margin it reaches 55% of the way across at 16:9
/// and stops a letter's width short of the doorframe, which begins at 58%. Henny Penny bounces
/// its letters off the baseline, and the ink of this name runs from 0.14 to 1.35 of the size
/// below `POSTER_TITLE_Y` rather than from the cap line to the baseline -- the `Y` is chosen to
/// leave that block centred where the old name's was, clear of the option column below it.
const POSTER_TITLE_Y: f32 = 0.263;
const POSTER_TITLE_SIZE: f32 = 0.126;
/// Top of the first option, the step between them, and their glyph size.
const POSTER_LIST_Y: f32 = 0.505;
const POSTER_LINE_H: f32 = 0.063;
const POSTER_ITEM_SIZE: f32 = 0.048;
/// The last row -- EXIT -- is drawn alone in the bottom-right corner instead of at the foot of
/// the column. It is the one option that leaves rather than goes somewhere, and the corner is
/// where a player looks for it; it stays in the same selection cycle, so the marker and the
/// gold still say where you are.
const POSTER_EXIT_Y: f32 = 0.925;
/// Gap between the marker and the row it points at, as a fraction of width. The marker is
/// drawn to the LEFT of the margin rather than prefixed to the text, so selecting a row does
/// not shove its label sideways.
const MARKER_GAP: f32 = 0.018;
/// The title screen's wash, as alpha at the left edge of the frame and at the right. Dark
/// enough on the left to carry white type over a sunlit meadow, gone by the time it reaches
/// the door.
const POSTER_DIM_LEFT: f32 = 0.55;
const POSTER_DIM_RIGHT: f32 = 0.04;
/// And a second wash up from the bottom edge, over the near grass, which is the brightest
/// thing in the frame and sits directly under the option column.
const POSTER_FOOT_DIM: f32 = 0.42;
const POSTER_FOOT_H: f32 = 0.34;
/// Top of the first option row.
const LIST_Y: f32 = 0.62;
/// Distance between option rows.
const LINE_H: f32 = 0.065;
/// Option glyph size.
const ITEM_SIZE: f32 = 0.052;
/// Smaller rows for the credits text and the level list.
const SMALL_SIZE: f32 = 0.034;
const SMALL_LINE_H: f32 = 0.042;
/// Sub-screens start their list higher so longer lists fit.
const SUB_LIST_Y: f32 = 0.36;
/// Level-select shows this many rows around the selection.
const LEVEL_WINDOW: usize = 10;
/// Marker drawn on the selected row. The centred screens prefix it to the text; the title
/// screen draws it in the margin beside the row (`MARKER_GAP`), where it cannot move the row
/// it points at.
const MARKER: &str = ">";
/// Footer hint line.
const HINT_Y: f32 = 0.94;
const HINT_SIZE: f32 = 0.022;
/// Horizontal inset of the footer hints (fraction of width).
const HINT_MARGIN: f32 = 0.04;
/// Value rows are two columns straddling the centre: the label ends this far left of it, the
/// value begins this far right. A fixed gutter rather than one centred string, so a value does
/// not shove its own label sideways as it changes width.
const VALUE_GUTTER: f32 = 0.012;
/// How far the black wash under the text knocks the world back.
///
/// Two values, because the screens divide in two. The title and its short settings list are a
/// handful of centred rows that sit clear of the backdrop, and the backdrop is the intro level
/// running live -- it is meant to be looked at, so it takes the lightest wash that still carries
/// white text. Every other screen is a block of small type spread across the frame: the key map
/// and the credits both run straight over the door, and the pause menu is over a level lit for
/// play rather than for reading. Those take enough wash to read against anything.
const DIM_SHOWCASE: f32 = 0.35;
const DIM_READING: f32 = 0.62;

/// Column positions of the key-map table, as fractions of width.
const KEYMAP_COLS: [f32; 3] = [0.10, 0.42, 0.68];
/// The key map's own top, line height and text size, rather than the sub-screens' shared ones.
///
/// It is the longest list on any screen -- thirteen rows once jumping and the inventory had
/// theirs -- and at `SUB_LIST_Y` / `SMALL_LINE_H` its BACK row ran off the bottom into the
/// footer hints. Starting higher and setting tighter buys the rows without shrinking any other
/// screen; `the_key_map_fits_between_the_heading_and_the_footer` pins both ends at compile time.
const KEYMAP_Y: f32 = 0.305;
const KEYMAP_LINE_H: f32 = 0.035;
const KEYMAP_SIZE: f32 = 0.030;

// Key slots (VK codes) read from `Input::key_press`.
const KEY_BACKSPACE: usize = 8;
const KEY_ENTER: usize = 13;
const KEY_ESCAPE: usize = 27;
const KEY_UP: usize = 38;
const KEY_DOWN: usize = 40;
const KEY_LEFT: usize = 37;
const KEY_RIGHT: usize = 39;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MenuAction {
    None,
    /// Start a fresh game at scene 0.
    NewGame,
    /// Close the pause menu and resume.
    Continue,
    /// Reload the current scene.
    RestartLevel,
    /// Load this scene index.
    SwitchLevel(usize),
    /// Back to the title screen.
    MainMenu,
    /// Exit the program.
    Quit,
    /// Options screen: flip audio mute. The menu stays open.
    ToggleMute,
}

impl MenuAction {
    /// Whether this action begins somewhere new rather than carrying on where the player was.
    ///
    /// The distinction the engine cannot make for itself: every one of these loads a scene, and
    /// so do an elevator ride and a window crossing, but only these mean "a different run". The
    /// inventory survives travel and not the menu (`ext::inventory::Inventory::clear`), which is
    /// why NEW GAME does not start carrying the last game's loot and RESTART LEVEL does not put
    /// a pocketed apple in the hall beside the one the level rebuilds.
    pub fn starts_fresh(self) -> bool {
        match self {
            MenuAction::NewGame
            | MenuAction::RestartLevel
            | MenuAction::SwitchLevel(_)
            | MenuAction::MainMenu => true,
            MenuAction::None | MenuAction::Continue | MenuAction::Quit | MenuAction::ToggleMute => {
                false
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Screen {
    Title,
    Pause,
    Options,
    Controls,
    Credits,
    Levels,
}

/// The directional/confirm/back edges for one frame, merged from keyboard and pad.
///
/// Left and right exist for the Options screen's value rows, where up/down still moves between
/// settings and left/right changes the one you are on -- the arrangement every console settings
/// screen uses, and the reason `A`/`D` join the arrow keys here the way `W`/`S` do above.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
struct Nav {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    confirm: bool,
    back: bool,
}

impl Nav {
    fn read(input: &Input, pad: &PadEvents) -> Nav {
        let k = &input.key_press;
        Nav {
            up: k[KEY_UP] || k[b'W' as usize] || pad.menu_up,
            down: k[KEY_DOWN] || k[b'S' as usize] || pad.menu_down,
            left: k[KEY_LEFT] || k[b'A' as usize] || pad.menu_left,
            right: k[KEY_RIGHT] || k[b'D' as usize] || pad.menu_right,
            confirm: k[KEY_ENTER] || k[b' ' as usize] || k[b'E' as usize] || pad.menu_confirm,
            back: k[KEY_BACKSPACE] || k[KEY_ESCAPE] || pad.menu_back,
        }
    }
}

const TITLE_ROWS: [&str; 4] = ["NEW GAME", "OPTIONS", "CREDITS", "EXIT"];
const PAUSE_ROWS: [&str; 4] = ["CONTINUE", "RESTART LEVEL", "SWITCH LEVEL", "MAIN MENU"];
/// Options rows, in order. The first three are *value* rows -- left/right changes them in
/// place and confirm is a shortcut for "next value" -- and the last two are ordinary ones.
const OPT_MOUSE: usize = 0;
const OPT_PAD: usize = 1;
const OPT_MUTE: usize = 2;
const OPT_CONTROLS: usize = 3;
#[allow(dead_code)] // Completes the row list; only the tests need to name the last one.
const OPT_BACK: usize = 4;
const OPTIONS_LABELS: [&str; 5] =
    ["MOUSE SENSITIVITY", "GAMEPAD SENSITIVITY", "MUTE AUDIO", "CONTROLS", "BACK"];

const CONTROLS_ROWS: [&str; 1] = ["BACK"];
const CREDITS_ROWS: [&str; 1] = ["BACK"];
/// Where the BACK row sits on the credits screen. Fixed, unlike every other list here: the
/// credits roll, so there is no last line for it to follow.
const CREDITS_BACK_Y: f32 = 0.830;

// ── The roll ──────────────────────────────────────────────────────────────────────────────
/// The band the credits scroll through: under the heading, above BACK.
const ROLL_TOP: f32 = 0.27;
const ROLL_BOTTOM: f32 = 0.78;
/// How far from each end a line is faded out, so nothing pops in or off at a hard edge.
const ROLL_FADE: f32 = 0.09;
/// Fractions of the frame's height per second. The whole roll is about 2.9 of them, and the
/// band is another 0.53, so a pass takes a little under a minute -- slow enough to read a line
/// twice, and it comes round again for whoever looked away.
const ROLL_SPEED: f32 = 0.062;
/// How dim the small print under a credit is against the credit itself.
const NOTE_ALPHA: f32 = 0.62;

/// One line of the roll.
#[derive(Clone, Copy)]
enum Roll {
    /// A section rule. Gold, and the only thing set larger than the body.
    Head(&'static str),
    /// A credit.
    Line(&'static str),
    /// The small print under one: a licence, a note about where the rest is written down.
    Note(&'static str),
    /// Air.
    Gap,
}

impl Roll {
    /// How much of the frame's height this line occupies, including the space over a heading.
    fn height(self) -> f32 {
        match self {
            Roll::Head(_) => SMALL_LINE_H * 1.7,
            Roll::Line(_) => SMALL_LINE_H,
            Roll::Note(_) => SMALL_LINE_H * 0.92,
            Roll::Gap => SMALL_LINE_H * 0.8,
        }
    }

    fn text(self) -> &'static str {
        match self {
            Roll::Head(t) | Roll::Line(t) | Roll::Note(t) => t,
            Roll::Gap => "",
        }
    }
}

/// How far in a line at `y` is faded, 0 at either end of the band and 1 across the middle.
fn roll_fade(y: f32) -> f32 {
    let head = ((y - ROLL_TOP) / ROLL_FADE).clamp(0.0, 1.0);
    let foot = ((ROLL_BOTTOM - y) / ROLL_FADE).clamp(0.0, 1.0);
    head.min(foot)
}

/// The credits, in the order they roll.
///
/// A roll rather than a column, which is not only presentation: the column it replaced was
/// eleven lines against a budget of eleven, and the last two assets added to the game had to be
/// merged onto one line to fit. Attribution is a licence condition for every model here -- CC-BY
/// asks for the author by name -- so a screen that can only hold so many of them is a screen
/// that will eventually have to drop one. This one has no such limit, and
/// `every_cc_by_author_is_credited` holds it to naming all of them.
const ROLL: &[Roll] = &[
    Roll::Head("THE ENGINE"),
    Roll::Line("NONEUCLIDEAN, BY CODEPARADE"),
    Roll::Note("MIT. THE PORTAL RENDERER AND THE FIRST SEVEN SCENES"),
    Roll::Gap,
    Roll::Head("MODELS"),
    Roll::Line("BACKROOMS VR - CARLCAPU9"),
    Roll::Line("POOL ROOMS AND THE OVERGROWN ROOM - BLENDERUST"),
    Roll::Line("ELEVATOR - EFX"),
    Roll::Line("ABANDONED HOUSE - ELBOLILLO"),
    Roll::Line("VILLAGE INTERIOR OBJECTS - ELBOLILLO"),
    Roll::Line("MOON - LUCKASS333"),
    Roll::Line("DOORS - ICEVANILLA"),
    Roll::Line("ESCHER RELATIVITY - BENOIT GAGNIER"),
    Roll::Note("EVERY ONE OF THEM CC-BY-4.0"),
    // After that note rather than among the lines it covers: the mannequin is Adobe Mixamo
    // content, not CC-BY, and a note here reads back over what precedes it. Credited anyway --
    // Mixamo's terms have not been read against this use (THIRD_PARTY.md), and a credit costs
    // nothing if attribution turns out to be optional.
    Roll::Line("MANNEQUIN AND ANIMATION - ADOBE MIXAMO"),
    // Down here for the same reason: the exploration tools pack is CC0, which waives
    // attribution altogether -- among the lines above, the note would call it CC-BY.
    // Offered, not owed.
    Roll::Line("EXPLORATION TOOLS - ELBOLILLO"),
    Roll::Gap,
    Roll::Head("TYPE"),
    Roll::Line("PLAYPEN SANS - TYPETOGETHER"),
    Roll::Line("HENNY PENNY - BROWNFOX"),
    Roll::Note("BOTH SIL OPEN FONT LICENSE 1.1"),
    Roll::Gap,
    Roll::Head("SOUND"),
    Roll::Line("EVERY EFFECT AND EVERY LOOP IS SYNTHESISED"),
    Roll::Note("NO SAMPLES, NO RECORDINGS, NO LIBRARIES OF EITHER"),
    Roll::Gap,
    Roll::Head("BUILT WITH"),
    Roll::Line("RUST, GLOW, GLUTIN AND WINIT"),
    Roll::Line("RAPIER, KIRA, GILRS, CLAP, IMAGE, GLTF"),
    Roll::Note("THIRD_PARTY.MD CARRIES EVERY CRATE AND ITS LICENCE"),
    Roll::Gap,
    Roll::Head("AND"),
    Roll::Line("A PORTAL RENDERER, A MEADOW AND A DOOR"),
    Roll::Line("SUPERLIMINAL-STYLE GRABBING, AND ROOMS THAT DO NOT ADD UP"),
    Roll::Gap,
    Roll::Gap,
    Roll::Line("THANK YOU FOR PLAYING"),
];

/// The key map, as (action, keyboard, gamepad). Kept here rather than derived from the input
/// code because there is nothing to derive it from: the bindings live as literal key slots in
/// `Nav::read`, `Player::update_player` and `Gamepads::poll`, and inventing a binding table
/// just so this screen could read it would be a large refactor in service of one list. The
/// cost is that this table is documentation, and goes stale if a binding moves without it.
const KEYMAP: [(&str, &str, &str); 14] = [
    ("MOVE", "W A S D", "LEFT STICK"),
    ("SPRINT", "HOLD SHIFT", "L3 (STICK CLICK) TOGGLES"),
    ("JUMP", "SPACE", "CROSS"),
    ("LOOK", "MOUSE", "RIGHT STICK"),
    ("GRAB / RELEASE", "E", "SQUARE / R2"),
    ("ROTATE HELD", "HOLD R + MOUSE", "HOLD R1 + RIGHT STICK"),
    ("STOW / TAKE OUT", "F", "-"),
    ("PUT DOWN", "G", "-"),
    ("PICK SLOT", "1 - 6 / MOUSE WHEEL", "-"),
    ("PAUSE MENU", "ESC", "OPTIONS"),
    ("MENU: MOVE", "ARROWS / W A S D", "D-PAD"),
    ("MENU: CONFIRM", "ENTER / SPACE", "CROSS"),
    ("MUTE", "M", "CREATE"),
    ("FULLSCREEN", "ALT + ENTER", "PS BUTTON"),
];

pub struct Menu {
    open: bool,
    /// When the credits screen was last opened (`ext::view::time`), which is where the roll
    /// counts from. Meaningless on every other screen.
    credits_t0: f32,
    screen: Screen,
    sel: usize,
    /// Local view of the audio mute flag, flipped whenever `ToggleMute` fires.
    muted: bool,
}

impl Menu {
    pub fn new() -> Menu {
        // The mute row mirrors the saved setting, not a fresh `false`: `MAIN MENU` builds a new
        // `Menu`, and a screen that claims audio is on while it is off is worse than no screen.
        Menu {
            open: true,
            credits_t0: 0.0,
            screen: Screen::Title,
            sel: 0,
            muted: settings::muted(),
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// True on the title screen and its sub-screens, where the world behind the menu is the
    /// intro level running as a backdrop rather than a paused game the player is standing in.
    pub fn is_title(&self) -> bool {
        matches!(self.screen, Screen::Title | Screen::Options | Screen::Controls | Screen::Credits)
    }

    pub fn open_pause(&mut self) {
        self.open = true;
        self.goto(Screen::Pause);
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    fn goto(&mut self, s: Screen) {
        self.screen = s;
        self.sel = 0;
        // The roll starts from the bottom every time the screen is opened, rather than from
        // wherever it had got to when it was last closed.
        if s == Screen::Credits {
            self.credits_t0 = crate::ext::view::time();
        }
    }

    /// Number of selectable rows on the current screen.
    fn row_count(&self, scene_count: usize) -> usize {
        match self.screen {
            Screen::Title => TITLE_ROWS.len(),
            Screen::Pause => PAUSE_ROWS.len(),
            Screen::Options => OPTIONS_LABELS.len(),
            Screen::Controls => CONTROLS_ROWS.len(),
            Screen::Credits => CREDITS_ROWS.len(),
            Screen::Levels => scene_count,
        }
    }

    /// Advance the state machine with this frame's edges. Returns what the engine should do.
    pub fn update(&mut self, input: &Input, pad: &PadEvents, scene_count: usize) -> MenuAction {
        if !self.open {
            return MenuAction::None;
        }
        let nav = Nav::read(input, pad);
        let n = self.row_count(scene_count).max(1);
        self.sel = self.sel.min(n - 1);
        if nav.up {
            self.sel = (self.sel + n - 1) % n;
        }
        if nav.down {
            self.sel = (self.sel + 1) % n;
        }
        // EXT: the menu's three sounds. This whole function is skipped on the frame a menu
        // opens (`Engine::run_frame`), so opening one is silent and the first tick a player
        // hears is a row they moved to themselves. Fired on a selection that actually moved, the
        // same rule the left/right branch below keeps: a one-row screen has nowhere to go, and
        // the modulo above would leave it where it was.
        if (nav.up || nav.down) && n > 1 {
            audio::request(Sfx::UiMove);
        }
        // Left/right belong to whichever value row the selection is on; everywhere else they
        // are simply ignored rather than falling through to something else.
        if nav.left || nav.right {
            let delta = i32::from(nav.right) - i32::from(nav.left);
            if let Some(action) = self.adjust(delta) {
                // Only a row that took the press answers: a row with nothing to change stays
                // silent, which is the difference the player needs to hear.
                audio::request(Sfx::UiMove);
                return action;
            }
        }
        if nav.back {
            audio::request(Sfx::UiBack);
            return self.back();
        }
        if nav.confirm {
            audio::request(Sfx::UiConfirm);
            return self.confirm(scene_count);
        }
        MenuAction::None
    }

    /// Change the value row under the selection by `delta` notches. `None` if this row has no
    /// value to change, which is what tells `update` that left/right meant nothing here.
    fn adjust(&mut self, delta: i32) -> Option<MenuAction> {
        if self.screen != Screen::Options || delta == 0 {
            return None;
        }
        match self.sel {
            OPT_MOUSE => {
                settings::adjust_mouse(delta);
                Some(MenuAction::None)
            }
            OPT_PAD => {
                settings::adjust_pad(delta);
                Some(MenuAction::None)
            }
            // Two states, so either direction is the same flip.
            OPT_MUTE => Some(self.toggle_mute()),
            _ => None,
        }
    }

    fn toggle_mute(&mut self) -> MenuAction {
        self.muted = !self.muted;
        settings::set_muted(self.muted);
        MenuAction::ToggleMute
    }

    fn back(&mut self) -> MenuAction {
        match self.screen {
            Screen::Title => MenuAction::None,
            Screen::Pause => MenuAction::Continue,
            Screen::Options | Screen::Credits => {
                self.goto(Screen::Title);
                MenuAction::None
            }
            // Controls hangs off Options, so Back returns there rather than to the title --
            // and lands on the row that opened it, not on the top of the list.
            Screen::Controls => {
                self.screen = Screen::Options;
                self.sel = OPT_CONTROLS;
                MenuAction::None
            }
            Screen::Levels => {
                self.goto(Screen::Pause);
                MenuAction::None
            }
        }
    }

    fn confirm(&mut self, scene_count: usize) -> MenuAction {
        match (self.screen, self.sel) {
            (Screen::Title, 0) => MenuAction::NewGame,
            (Screen::Title, 1) => {
                self.goto(Screen::Options);
                MenuAction::None
            }
            (Screen::Title, 2) => {
                self.goto(Screen::Credits);
                MenuAction::None
            }
            (Screen::Title, _) => MenuAction::Quit,

            (Screen::Pause, 0) => MenuAction::Continue,
            (Screen::Pause, 1) => MenuAction::RestartLevel,
            (Screen::Pause, 2) => {
                if scene_count > 0 {
                    self.goto(Screen::Levels);
                }
                MenuAction::None
            }
            (Screen::Pause, _) => MenuAction::MainMenu,

            // On a value row, confirm is a shortcut for "next value" -- one button to try a
            // setting, rather than having to find left/right first.
            (Screen::Options, OPT_MOUSE) | (Screen::Options, OPT_PAD) => {
                self.adjust(1);
                MenuAction::None
            }
            (Screen::Options, OPT_MUTE) => self.toggle_mute(),
            (Screen::Options, OPT_CONTROLS) => {
                self.goto(Screen::Controls);
                MenuAction::None
            }
            (Screen::Options, _) | (Screen::Controls, _) | (Screen::Credits, _) => self.back(),

            (Screen::Levels, i) if i < scene_count => MenuAction::SwitchLevel(i),
            (Screen::Levels, _) => MenuAction::None,
        }
    }

    /// Draw the current screen. The engine has already called `Ui::begin`; this lays out the
    /// wash and the text.
    pub fn draw(&self, ui: &Ui, scene_names: &[&str]) {
        let (w, h) = ui.size();
        let cx = w * 0.5;
        // The wash belongs to the menu, not to the engine: which screen is up is exactly what
        // decides how much of it there should be, and only this module knows that.
        if self.screen == Screen::Title {
            self.draw_poster(ui);
            self.draw_footer(ui);
            return;
        }
        let dim = match self.screen {
            Screen::Options => DIM_SHOWCASE,
            _ => DIM_READING,
        };
        ui.fill_rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, dim]);
        let heading = match self.screen {
            Screen::Title => TITLE_TEXT,
            Screen::Pause => "PAUSED",
            Screen::Options => "OPTIONS",
            Screen::Controls => "CONTROLS",
            Screen::Credits => "CREDITS",
            Screen::Levels => "SWITCH LEVEL",
        };
        ui.draw_text(heading, cx, h * TITLE_Y, h * TITLE_SIZE, WHITE, Align::Center);

        match self.screen {
            Screen::Title => self.draw_rows(ui, &TITLE_ROWS, LIST_Y, LINE_H, ITEM_SIZE),
            Screen::Pause => self.draw_rows(ui, &PAUSE_ROWS, LIST_Y, LINE_H, ITEM_SIZE),
            Screen::Options => self.draw_options(ui),
            Screen::Controls => self.draw_controls(ui),
            Screen::Credits => self.draw_credits(ui),
            Screen::Levels => self.draw_levels(ui, scene_names),
        }

        self.draw_footer(ui);
    }

    /// The credits, rolling: the block scrolls up through a band between the heading and BACK,
    /// fading out at both ends, and comes round again when the last line has left the top.
    ///
    /// Driven by the clock rather than by a per-frame step, so it cannot drift with the frame
    /// rate and needs nothing mutable at draw time: `Menu::goto` stamps `credits_t0` when the
    /// screen opens and the offset is a function of how long it has been up.
    fn draw_credits(&self, ui: &Ui) {
        let (w, h) = ui.size();
        let cx = w * 0.5;
        let elapsed = (crate::ext::view::time() - self.credits_t0).max(0.0);
        let total: f32 = ROLL.iter().map(|r| r.height()).sum();
        // One pass carries the first line from the foot of the band to where the LAST line has
        // just left its head, which is the block's own height plus the band's. Taken modulo, so
        // the roll repeats for as long as the screen is up.
        let period = total + (ROLL_BOTTOM - ROLL_TOP);
        let mut y = ROLL_BOTTOM - (elapsed * ROLL_SPEED) % period;
        for item in ROLL {
            let line_h = item.height();
            // Only what is inside the band, and only what has text: a Gap is height and
            // nothing else.
            if !matches!(item, Roll::Gap) && y < ROLL_BOTTOM && y > ROLL_TOP - line_h {
                let a = roll_fade(y);
                let (size, color) = match item {
                    Roll::Head(_) => (SMALL_SIZE * 1.15, [GOLD[0], GOLD[1], GOLD[2], a]),
                    Roll::Line(_) => (SMALL_SIZE, [1.0, 1.0, 1.0, a]),
                    Roll::Note(_) | Roll::Gap => {
                        (SMALL_SIZE * 0.82, [1.0, 1.0, 1.0, a * NOTE_ALPHA])
                    }
                };
                ui.draw_text(item.text(), cx, h * y, h * size, color, Align::Center);
            }
            y += line_h;
        }
        self.draw_rows(ui, &CREDITS_ROWS, CREDITS_BACK_Y, LINE_H, ITEM_SIZE);
    }

    /// Footer: navigation hints hug the left edge, the back hint the right edge, so the line
    /// reads as two groups instead of one run of words. The title screen has no back hint --
    /// Back does nothing there, and the corner is EXIT's.
    fn draw_footer(&self, ui: &Ui) {
        let (w, h) = ui.size();
        let margin = w * HINT_MARGIN;
        ui.draw_text(
            "UP/DOWN  SELECT     ENTER  CONFIRM",
            margin,
            h * HINT_Y,
            h * HINT_SIZE,
            DIM,
            Align::Left,
        );
        if self.screen != Screen::Title {
            ui.draw_text("ESC  BACK", w - margin, h * HINT_Y, h * HINT_SIZE, DIM, Align::Right);
        }
    }

    /// The title screen: the game's name and its options set against the left margin, EXIT
    /// alone in the far corner, over a wash that darkens toward the words and clears over the
    /// door. See the module docs; the shot it is composed against is `ext::meadow::title_view`.
    fn draw_poster(&self, ui: &Ui) {
        let (w, h) = ui.size();
        let clear = [0.0, 0.0, 0.0, POSTER_DIM_RIGHT];
        ui.fill_rect_grad(0.0, 0.0, w, h, [0.0, 0.0, 0.0, POSTER_DIM_LEFT], clear, false);
        // Up from the bottom edge as well: the near grass is the brightest thing in the frame
        // and the option column sits on it.
        let foot = h * POSTER_FOOT_H;
        ui.fill_rect_grad(
            0.0,
            h - foot,
            w,
            foot,
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, POSTER_FOOT_DIM],
            true,
        );

        let left = w * POSTER_LEFT;
        let title_y = h * POSTER_TITLE_Y;
        let title_size = h * POSTER_TITLE_SIZE;
        ui.draw_title(TITLE_TEXT, left, title_y, title_size, WHITE, Align::Left);

        // The column, then EXIT in the corner. `last` is drawn from the same table and the
        // same selection index, so the two never disagree about which row is which.
        let size = h * POSTER_ITEM_SIZE;
        let gap = w * MARKER_GAP;
        let (last, column) = TITLE_ROWS.split_last().expect("the title screen has rows");
        for (i, row) in column.iter().enumerate() {
            let y = h * (POSTER_LIST_Y + i as f32 * POSTER_LINE_H);
            let colour = if i == self.sel { GOLD } else { WHITE };
            ui.draw_text(row, left, y, size, colour, Align::Left);
            if i == self.sel {
                ui.draw_text(MARKER, left - gap, y, size, GOLD, Align::Right);
            }
        }
        let exit_sel = self.sel == TITLE_ROWS.len() - 1;
        let ey = h * POSTER_EXIT_Y;
        let ex = w - w * HINT_MARGIN;
        ui.draw_text(last, ex, ey, size, if exit_sel { GOLD } else { WHITE }, Align::Right);
        if exit_sel {
            let width = ui.measure(last, size);
            ui.draw_text(MARKER, ex - width - gap, ey, size, GOLD, Align::Right);
        }
    }

    /// A centred column of rows starting at `y0` (fraction of height); the selected one is
    /// gold and carries the marker.
    fn draw_rows(&self, ui: &Ui, rows: &[&str], y0: f32, line_h: f32, size: f32) {
        let (w, h) = ui.size();
        for (i, row) in rows.iter().enumerate() {
            let y = h * (y0 + i as f32 * line_h);
            if i == self.sel {
                let text = format!("{MARKER} {row}");
                ui.draw_text(&text, w * 0.5, y, h * size, GOLD, Align::Center);
            } else {
                ui.draw_text(row, w * 0.5, y, h * size, WHITE, Align::Center);
            }
        }
    }

    /// Options: three value rows over two plain ones.
    ///
    /// Value rows are drawn as two columns straddling the centre line rather than as one
    /// centred string, because a centred `LABEL  < 10 >` shifts its label every time the number
    /// gains a digit. The arrows are drawn only on the selected row, and only on the side that
    /// still has somewhere to go -- a missing arrow is how the screen says a value is at its end.
    fn draw_options(&self, ui: &Ui) {
        let (w, h) = ui.size();
        let (mouse, pad) = (settings::mouse_level(), settings::pad_level());
        let values: [Option<(String, bool, bool)>; OPTIONS_LABELS.len()] = [
            Some((format!("{mouse}"), mouse > 1, mouse < settings::LEVELS)),
            Some((format!("{pad}"), pad > 1, pad < settings::LEVELS)),
            Some((if self.muted { "ON".into() } else { "OFF".into() }, true, true)),
            None,
            None,
        ];

        for (i, label) in OPTIONS_LABELS.iter().enumerate() {
            let y = h * (LIST_Y - LINE_H + i as f32 * LINE_H);
            let size = h * ITEM_SIZE;
            let colour = if i == self.sel { GOLD } else { WHITE };
            let Some((value, can_less, can_more)) = &values[i] else {
                // A plain row still centres, so CONTROLS and BACK read as buttons rather than
                // as settings with the value missing.
                let text =
                    if i == self.sel { format!("{MARKER} {label}") } else { label.to_string() };
                ui.draw_text(&text, w * 0.5, y, size, colour, Align::Center);
                continue;
            };
            let gutter = w * VALUE_GUTTER;
            let label = if i == self.sel { format!("{MARKER} {label}") } else { label.to_string() };
            ui.draw_text(&label, w * 0.5 - gutter, y, size, colour, Align::Right);
            let shown = if i == self.sel {
                format!(
                    "{} {value} {}",
                    if *can_less { "<" } else { " " },
                    if *can_more { ">" } else { " " }
                )
            } else {
                value.clone()
            };
            ui.draw_text(&shown, w * 0.5 + gutter, y, size, colour, Align::Left);
        }

        let hint = "LEFT/RIGHT  CHANGE";
        ui.draw_text(hint, w * 0.5, h * (HINT_Y - 0.04), h * HINT_SIZE, DIM, Align::Center);
    }

    /// Controls: the key map, one row per action, in three columns.
    fn draw_controls(&self, ui: &Ui) {
        let (w, h) = ui.size();
        let head_y = h * (KEYMAP_Y - KEYMAP_LINE_H * 1.4);
        for (col, title) in ["ACTION", "KEYBOARD", "GAMEPAD"].iter().enumerate() {
            ui.draw_text(title, w * KEYMAP_COLS[col], head_y, h * HINT_SIZE, DIM, Align::Left);
        }
        for (row, (action, key, pad)) in KEYMAP.iter().enumerate() {
            let y = h * (KEYMAP_Y + row as f32 * KEYMAP_LINE_H);
            // The action in white and its bindings dimmed: the eye finds the row by what it
            // does, then reads across to how.
            ui.draw_text(action, w * KEYMAP_COLS[0], y, h * KEYMAP_SIZE, WHITE, Align::Left);
            ui.draw_text(key, w * KEYMAP_COLS[1], y, h * KEYMAP_SIZE, DIM, Align::Left);
            ui.draw_text(pad, w * KEYMAP_COLS[2], y, h * KEYMAP_SIZE, DIM, Align::Left);
        }
        let after = KEYMAP_Y + (KEYMAP.len() as f32 + 0.8) * KEYMAP_LINE_H;
        self.draw_rows(ui, &CONTROLS_ROWS, after, LINE_H, ITEM_SIZE);
    }

    /// Level-select: a window of `LEVEL_WINDOW` rows that follows the selection, with the
    /// one-based scene number in front of each name.
    fn draw_levels(&self, ui: &Ui, scene_names: &[&str]) {
        let (w, h) = ui.size();
        let n = scene_names.len();
        let start = level_window_start(self.sel, n, LEVEL_WINDOW);
        let end = (start + LEVEL_WINDOW).min(n);
        for (row, i) in (start..end).enumerate() {
            let y = h * (SUB_LIST_Y + row as f32 * SMALL_LINE_H);
            let name = format!("{}. {}", i + 1, scene_names[i]);
            if i == self.sel {
                let text = format!("{MARKER} {name}");
                ui.draw_text(&text, w * 0.5, y, h * SMALL_SIZE, GOLD, Align::Center);
            } else {
                ui.draw_text(&name, w * 0.5, y, h * SMALL_SIZE, WHITE, Align::Center);
            }
        }
        if start > 0 {
            let y = h * (SUB_LIST_Y - SMALL_LINE_H);
            ui.draw_text("...", w * 0.5, y, h * SMALL_SIZE, DIM, Align::Center);
        }
        if end < n {
            let y = h * (SUB_LIST_Y + LEVEL_WINDOW as f32 * SMALL_LINE_H);
            ui.draw_text("...", w * 0.5, y, h * SMALL_SIZE, DIM, Align::Center);
        }
    }
}

/// First visible row of a `window`-row view over `n` rows, keeping `sel` roughly centred and
/// never scrolling past either end.
fn level_window_start(sel: usize, n: usize, window: usize) -> usize {
    if n <= window {
        return 0;
    }
    let half = window / 2;
    sel.saturating_sub(half).min(n - window)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(slot: usize) -> Input {
        let mut i = Input::new();
        i.key_press[slot] = true;
        i
    }

    fn step(m: &mut Menu, slot: usize, scenes: usize) -> MenuAction {
        m.update(&press(slot), &PadEvents::default(), scenes)
    }

    #[test]
    fn boots_on_title_with_the_backdrop_live() {
        let m = Menu::new();
        assert!(m.is_open());
        assert!(m.is_title());
        assert_eq!(m.screen, Screen::Title);
    }

    #[test]
    fn title_options_back_returns_to_title() {
        let mut m = Menu::new();
        assert_eq!(step(&mut m, KEY_DOWN, 14), MenuAction::None);
        assert_eq!(step(&mut m, KEY_ENTER, 14), MenuAction::None);
        assert_eq!(m.screen, Screen::Options);
        assert!(m.is_title());
        assert_eq!(step(&mut m, KEY_ESCAPE, 14), MenuAction::None);
        assert_eq!(m.screen, Screen::Title);
        assert_eq!(m.sel, 0);
    }

    #[test]
    fn title_back_does_nothing_and_exit_quits() {
        let mut m = Menu::new();
        assert_eq!(step(&mut m, KEY_BACKSPACE, 14), MenuAction::None);
        assert_eq!(m.screen, Screen::Title);
        assert_eq!(step(&mut m, KEY_UP, 14), MenuAction::None); // wraps to EXIT
        assert_eq!(step(&mut m, b' ' as usize, 14), MenuAction::Quit);
    }

    /// Models the engine's run_frame sequence on the frame Escape is pressed: open_pause()
    /// fires and the SAME Escape press must not reach update(), otherwise `back` on the pause
    /// root turns into Continue and the menu closes before it is ever drawn.
    #[test]
    fn escape_frame_opens_pause_without_closing_it() {
        let mut m = Menu::new();
        m.close();
        let input = press(KEY_ESCAPE);
        let pad = PadEvents::default();
        let action = if !m.is_open() && input.key_press[KEY_ESCAPE] {
            m.open_pause();
            MenuAction::None
        } else {
            m.update(&input, &pad, 14)
        };
        assert_eq!(action, MenuAction::None);
        assert!(m.is_open());
        assert_eq!(m.screen, Screen::Pause);
        // The next frame's Escape (a fresh press) is the one that resumes.
        assert_eq!(step(&mut m, KEY_ESCAPE, 14), MenuAction::Continue);
    }

    #[test]
    fn new_game_fires() {
        let mut m = Menu::new();
        assert_eq!(step(&mut m, b'E' as usize, 14), MenuAction::NewGame);
    }

    #[test]
    fn pause_root_escape_is_continue() {
        let mut m = Menu::new();
        m.close();
        assert!(!m.is_open());
        assert_eq!(step(&mut m, KEY_ESCAPE, 14), MenuAction::None);
        m.open_pause();
        assert!(m.is_open());
        assert!(!m.is_title());
        assert_eq!(step(&mut m, KEY_ESCAPE, 14), MenuAction::Continue);
        let pad = PadEvents { menu_back: true, ..PadEvents::default() };
        assert_eq!(m.update(&Input::new(), &pad, 14), MenuAction::Continue);
    }

    #[test]
    fn selection_wraps_both_ways() {
        let mut m = Menu::new();
        assert_eq!(m.sel, 0);
        step(&mut m, KEY_UP, 14);
        assert_eq!(m.sel, TITLE_ROWS.len() - 1);
        step(&mut m, KEY_DOWN, 14);
        assert_eq!(m.sel, 0);
        step(&mut m, b'S' as usize, 14);
        assert_eq!(m.sel, 1);
        step(&mut m, b'W' as usize, 14);
        assert_eq!(m.sel, 0);
    }

    #[test]
    fn switch_level_returns_index() {
        let mut m = Menu::new();
        m.open_pause();
        step(&mut m, KEY_DOWN, 14);
        step(&mut m, KEY_DOWN, 14);
        assert_eq!(step(&mut m, KEY_ENTER, 14), MenuAction::None);
        assert_eq!(m.screen, Screen::Levels);
        for _ in 0..5 {
            step(&mut m, KEY_DOWN, 14);
        }
        assert_eq!(step(&mut m, KEY_ENTER, 14), MenuAction::SwitchLevel(5));
        // Wrap on the level list uses the scene count.
        step(&mut m, KEY_UP, 14);
        for _ in 0..5 {
            step(&mut m, KEY_UP, 14);
        }
        assert_eq!(m.sel, 13);
        // Back from the level list lands on the pause root.
        assert_eq!(step(&mut m, KEY_ESCAPE, 14), MenuAction::None);
        assert_eq!(m.screen, Screen::Pause);
    }

    #[test]
    fn pause_actions() {
        let mut m = Menu::new();
        m.open_pause();
        assert_eq!(step(&mut m, KEY_ENTER, 14), MenuAction::Continue);
        step(&mut m, KEY_DOWN, 14);
        assert_eq!(step(&mut m, KEY_ENTER, 14), MenuAction::RestartLevel);
        step(&mut m, KEY_UP, 14);
        step(&mut m, KEY_UP, 14);
        assert_eq!(step(&mut m, KEY_ENTER, 14), MenuAction::MainMenu);
    }

    /// Which actions empty the pockets, driven through the screens a player would use rather
    /// than asserted on the enum: NEW GAME off the title and MAIN MENU out of the pause menu
    /// are the two the inventory's `clear` exists for, and CONTINUE is the one that must not
    /// touch it -- a pause menu is not a new game.
    #[test]
    fn a_new_game_and_the_main_menu_start_fresh_and_continue_does_not() {
        let mut m = Menu::new();
        assert_eq!(m.sel, 0, "NEW GAME is the title's first row");
        let action = step(&mut m, KEY_ENTER, 14);
        assert_eq!(action, MenuAction::NewGame);
        assert!(action.starts_fresh());

        let mut m = Menu::new();
        m.open_pause();
        let action = step(&mut m, KEY_ENTER, 14);
        assert_eq!(action, MenuAction::Continue);
        assert!(!action.starts_fresh(), "resuming is not a new game");

        let mut m = Menu::new();
        m.open_pause();
        step(&mut m, KEY_UP, 14); // CONTINUE -> MAIN MENU, wrapping backwards
        let action = step(&mut m, KEY_ENTER, 14);
        assert_eq!(action, MenuAction::MainMenu);
        assert!(action.starts_fresh());

        // And the rest of the table, so a new variant has to choose a side.
        assert!(MenuAction::RestartLevel.starts_fresh());
        assert!(MenuAction::SwitchLevel(3).starts_fresh());
        assert!(!MenuAction::None.starts_fresh());
        assert!(!MenuAction::Quit.starts_fresh());
        assert!(!MenuAction::ToggleMute.starts_fresh());
    }

    /// Walk from the title into Options and land the selection on a given row.
    fn open_options_at(row: usize) -> Menu {
        let mut m = Menu::new();
        step(&mut m, KEY_DOWN, 14); // TITLE: NEW GAME -> OPTIONS
        step(&mut m, KEY_ENTER, 14);
        assert_eq!(m.screen, Screen::Options);
        for _ in 0..row {
            step(&mut m, KEY_DOWN, 14);
        }
        assert_eq!(m.sel, row);
        m
    }

    #[test]
    fn options_toggle_mute_tracks_state() {
        let mut m = open_options_at(OPT_MUTE);
        let was = m.muted;
        assert_eq!(step(&mut m, KEY_ENTER, 14), MenuAction::ToggleMute);
        assert_eq!(m.muted, !was);
        assert_eq!(m.screen, Screen::Options);
        // Left/right flip it too -- it is a value row with two values.
        assert_eq!(step(&mut m, KEY_RIGHT, 14), MenuAction::ToggleMute);
        assert_eq!(m.muted, was);

        let mut m = open_options_at(OPT_BACK);
        assert_eq!(step(&mut m, KEY_ENTER, 14), MenuAction::None);
        assert_eq!(m.screen, Screen::Title);
    }

    /// Left and right move a sensitivity a notch at a time and stop at the ends rather than
    /// wrapping, and up/down still change rows while they do it.
    #[test]
    fn options_sensitivity_rows_adjust_and_saturate() {
        let mut m = open_options_at(OPT_MOUSE);
        let start = settings::mouse_level();
        step(&mut m, KEY_RIGHT, 14);
        assert_eq!(settings::mouse_level(), start + 1);
        assert_eq!(m.sel, OPT_MOUSE, "changing a value must not move the selection");
        step(&mut m, KEY_LEFT, 14);
        step(&mut m, KEY_LEFT, 14);
        assert_eq!(settings::mouse_level(), start - 1);

        for _ in 0..settings::LEVELS * 2 {
            step(&mut m, KEY_LEFT, 14);
        }
        assert_eq!(settings::mouse_level(), 1, "left should saturate, not wrap");

        // The neighbouring row is a different setting, and moving to it must not disturb this one.
        step(&mut m, KEY_DOWN, 14);
        assert_eq!(m.sel, OPT_PAD);
        step(&mut m, KEY_RIGHT, 14);
        assert_eq!(settings::mouse_level(), 1, "the pad row changed the mouse setting");
    }

    /// Left/right on a row that carries no value must do nothing at all -- not scroll, not
    /// confirm, not fall through to the row above.
    #[test]
    fn left_right_is_inert_off_the_value_rows() {
        let mut m = open_options_at(OPT_CONTROLS);
        assert_eq!(step(&mut m, KEY_RIGHT, 14), MenuAction::None);
        assert_eq!(m.screen, Screen::Options);
        assert_eq!(m.sel, OPT_CONTROLS);

        let mut m = Menu::new(); // the title screen has no value rows either
        assert_eq!(step(&mut m, KEY_LEFT, 14), MenuAction::None);
        assert_eq!(m.sel, 0);
    }

    /// Controls hangs off Options and returns to the row that opened it, so a player checking
    /// the key map does not lose their place in the settings list.
    #[test]
    fn controls_opens_from_options_and_returns_to_its_row() {
        let mut m = open_options_at(OPT_CONTROLS);
        assert_eq!(step(&mut m, KEY_ENTER, 14), MenuAction::None);
        assert_eq!(m.screen, Screen::Controls);
        assert!(m.is_title(), "the key map is a title-stack screen, so the backdrop keeps running");

        assert_eq!(step(&mut m, KEY_ESCAPE, 14), MenuAction::None);
        assert_eq!(m.screen, Screen::Options);
        assert_eq!(m.sel, OPT_CONTROLS);

        // ...and via its own BACK row, which is the only row it has.
        step(&mut m, KEY_ENTER, 14);
        assert_eq!(m.screen, Screen::Controls);
        assert_eq!(step(&mut m, KEY_ENTER, 14), MenuAction::None);
        assert_eq!(m.screen, Screen::Options);
    }

    /// The roll has no row budget -- that is the point of it -- but it still has to stay
    /// between the heading and the BACK row, and BACK still has to clear the footer.
    #[test]
    fn the_roll_stays_between_the_heading_and_the_back_row() {
        const { assert!(TITLE_Y + TITLE_SIZE < ROLL_TOP) }
        const { assert!(ROLL_BOTTOM < CREDITS_BACK_Y) }
        const { assert!(CREDITS_BACK_Y + ITEM_SIZE + SMALL_LINE_H < HINT_Y) }
        // A band at least a few lines deep, and a fade that fits inside it twice over -- one
        // that met in the middle would leave nothing at full brightness.
        const { assert!(ROLL_BOTTOM - ROLL_TOP > 4.0 * SMALL_LINE_H) }
        const { assert!(2.0 * ROLL_FADE < ROLL_BOTTOM - ROLL_TOP) }
    }

    /// The fade is 0 at both ends of the band, 1 across the middle, and never outside 0..=1 --
    /// an alpha over 1 would look no different, but a negative one is a colour the shader has
    /// no meaning for.
    #[test]
    fn the_roll_fades_in_at_the_foot_and_out_at_the_head() {
        assert_eq!(roll_fade(ROLL_BOTTOM), 0.0);
        assert_eq!(roll_fade(ROLL_TOP), 0.0);
        assert_eq!(roll_fade(0.5 * (ROLL_TOP + ROLL_BOTTOM)), 1.0);
        // Monotonic up from the foot, and clamped outside the band on both sides.
        let mut prev = 0.0;
        let mut y = ROLL_BOTTOM;
        while y > 0.5 * (ROLL_TOP + ROLL_BOTTOM) {
            let a = roll_fade(y);
            assert!(a >= prev - 1e-6, "fade fell from {prev} to {a} at y = {y}");
            prev = a;
            y -= 0.005;
        }
        for y in [-1.0, ROLL_TOP - 0.1, ROLL_BOTTOM + 0.1, 2.0] {
            assert!((0.0..=1.0).contains(&roll_fade(y)), "fade is {} at {y}", roll_fade(y));
        }
    }

    /// The roll comes round: a pass is the block's height plus the band's, and the line laid out
    /// at the end of one is the line laid out at the start of the next. Computed the way
    /// `draw_credits` computes it, so the two cannot drift.
    #[test]
    fn the_roll_loops_without_a_jump() {
        let total: f32 = ROLL.iter().map(|r| r.height()).sum();
        let period = total + (ROLL_BOTTOM - ROLL_TOP);
        let first_at = |elapsed: f32| ROLL_BOTTOM - (elapsed * ROLL_SPEED) % period;
        // At the moment it wraps, the first line is back where it started.
        let wrap = period / ROLL_SPEED;
        assert!((first_at(0.0) - ROLL_BOTTOM).abs() < 1e-5);
        assert!((first_at(wrap) - ROLL_BOTTOM).abs() < 1e-3, "{}", first_at(wrap));
        // Just before it, the LAST line has cleared the head of the band -- so nothing is
        // still on screen when the roll starts over.
        let last_h = ROLL.last().map_or(0.0, |r| r.height());
        let last_at_end = first_at(wrap - 1e-3) + total - last_h;
        assert!(last_at_end <= ROLL_TOP + 1e-2, "the last line is still at {last_at_end}");
        // And a pass is long enough to read: roughly a minute, not five seconds.
        assert!((30.0..=120.0).contains(&wrap), "a pass takes {wrap} s");
    }

    /// Every author whose licence requires the credit is named in the roll -- and the two that
    /// nothing requires: the one whose terms have not been read, credited in case it does, and
    /// the CC0 pack, credited as a courtesy.
    ///
    /// This is the test the old fixed column could not have: it ran out of room at eleven lines,
    /// and the eleventh asset had to be merged onto another's line to fit. CC-BY-4.0 asks for
    /// attribution by name for each of these, so the screen has to name them, and a roll is what
    /// makes that a promise rather than a budget.
    #[test]
    fn every_cc_by_author_is_credited() {
        let roll: String = ROLL.iter().map(|r| r.text()).collect::<Vec<_>>().join("\n");
        for author in [
            "CARLCAPU9",      // Backrooms VR
            "BLENDERUST",     // Pool Rooms, Overgrown room
            "EFX",            // Elevator
            "ELBOLILLO",      // Abandoned house, village objects
            "LUCKASS333",     // Moon
            "ICEVANILLA",     // The doors
            "BENOIT GAGNIER", // Escher Relativity
        ] {
            assert!(roll.contains(author), "the roll does not credit {author}");
        }
        // And the licence they are all under, plus the four that are not CC-BY.
        assert!(roll.contains("CC-BY-4.0"));
        assert!(roll.contains("CODEPARADE") && roll.contains("MIT"));
        assert!(roll.contains("SIL OPEN FONT LICENSE"));
        // The mannequin and its 31 clips are Adobe Mixamo's, under the account they were
        // downloaded with -- no CC licence at all. Its line has to be there, and it has to sit
        // AFTER the note covering the models above it: moved up, the note would claim CC-BY-4.0
        // for content that is not, and the screen would be lying rather than merely thin.
        let mixamo = roll.find("ADOBE MIXAMO").expect("the roll does not credit ADOBE MIXAMO");
        let cc = roll.find("EVERY ONE OF THEM CC-BY-4.0").expect("the CC-BY note is gone");
        assert!(cc < mixamo, "the Mixamo credit sits above the note that says CC-BY-4.0");
        // The village objects pack is CC-BY-4.0 like its neighbours, so its line must sit
        // ABOVE the note for the note to cover it.
        let village = roll
            .find("VILLAGE INTERIOR OBJECTS - ELBOLILLO")
            .expect("the roll does not credit the village objects pack");
        assert!(village < cc, "the village pack sits below the note that should cover it");
        // The exploration tools pack is CC0 -- no attribution owed at all -- so its line,
        // like Mixamo's, sits BELOW the note: above it, the note would call it CC-BY.
        let tools = roll
            .find("EXPLORATION TOOLS - ELBOLILLO")
            .expect("the roll does not credit the exploration tools pack");
        assert!(cc < tools, "the CC0 tools credit sits above the note that says CC-BY-4.0");
    }

    /// The key map has the same budget at the bottom and a heading to clear at the top. Jumping
    /// and the inventory took it to fourteen rows, which is why it has its own `KEYMAP_Y` /
    /// `KEYMAP_LINE_H` instead of the sub-screens' shared ones -- at those it ran into the
    /// footer. Both ends are held here at compile time rather than in a screenshot: BACK's whole
    /// glyph box stays above the footer hints, and the column headings stay below the heading.
    #[test]
    fn the_key_map_fits_between_the_heading_and_the_footer() {
        const BACK_Y: f32 = KEYMAP_Y + (KEYMAP.len() as f32 + 0.8) * KEYMAP_LINE_H;
        const { assert!(BACK_Y + ITEM_SIZE + HINT_SIZE < HINT_Y) }
        const HEAD_Y: f32 = KEYMAP_Y - KEYMAP_LINE_H * 1.4;
        const { assert!(TITLE_Y + TITLE_SIZE < HEAD_Y) }
        // And the rows are still leaded, not overlapping.
        const { assert!(KEYMAP_SIZE < KEYMAP_LINE_H) }
    }

    #[test]
    fn credits_back_via_row_and_key() {
        let mut m = Menu::new();
        step(&mut m, KEY_DOWN, 14);
        step(&mut m, KEY_DOWN, 14);
        step(&mut m, KEY_ENTER, 14);
        assert_eq!(m.screen, Screen::Credits);
        step(&mut m, KEY_ENTER, 14);
        assert_eq!(m.screen, Screen::Title);
    }

    #[test]
    fn closed_menu_ignores_input() {
        let mut m = Menu::new();
        m.close();
        assert_eq!(step(&mut m, KEY_ENTER, 14), MenuAction::None);
        assert_eq!(m.sel, 0);
    }

    #[test]
    fn level_window_follows_selection() {
        assert_eq!(level_window_start(0, 14, 10), 0);
        assert_eq!(level_window_start(5, 14, 10), 0);
        assert_eq!(level_window_start(7, 14, 10), 2);
        assert_eq!(level_window_start(13, 14, 10), 4);
        assert_eq!(level_window_start(3, 4, 10), 0);
    }
}
