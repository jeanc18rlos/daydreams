//! EXT: title screen and pause menu. Not part of the C++ port.
//!
//! The original engine boots straight into scene 0 and has no menu of any kind: Escape quits
//! (Engine.cpp:183) and the number keys swap scenes. This module adds a small screen stack
//! drawn with the 2D layer from `ui.rs`, matching the title mockup: black background, a big
//! bold white title in the upper-middle, and a centred column of bold white options with the
//! selected row in gold.
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

/// The game's display name on the title screen. Upper case because the whole menu is: the
/// atlas bakes both cases, but caps hold their own over a photographed backdrop where mixed
/// case at this size starts to swim. `GH_TITLE` carries the same name for the OS window bar.
pub const TITLE_TEXT: &str = "DAYDREAMS";

// Layout, as fractions of the drawable height so the screen scales with the window.
/// Top of the title's line box.
const TITLE_Y: f32 = 0.14;
/// Title glyph size.
const TITLE_SIZE: f32 = 0.11;
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
/// Marker drawn on the selected row.
const MARKER: &str = "> ";
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
const KEYMAP_Y: f32 = 0.31;
const KEYMAP_LINE_H: f32 = 0.039;
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
/// Where the credits screen's BACK row goes: a small line's gap under the last credit line.
const CREDITS_BACK_Y: f32 = SUB_LIST_Y + (CREDITS_TEXT.len() as f32 + 1.0) * SMALL_LINE_H;

/// The key map, as (action, keyboard, gamepad). Kept here rather than derived from the input
/// code because there is nothing to derive it from: the bindings live as literal key slots in
/// `Nav::read`, `Player::update_player` and `Gamepads::poll`, and inventing a binding table
/// just so this screen could read it would be a large refactor in service of one list. The
/// cost is that this table is documentation, and goes stale if a binding moves without it.
const KEYMAP: [(&str, &str, &str); 13] = [
    ("MOVE", "W A S D", "LEFT STICK"),
    ("SPRINT", "HOLD SHIFT", "L3 (STICK CLICK) TOGGLES"),
    ("JUMP", "SPACE", "CROSS"),
    ("LOOK", "MOUSE", "RIGHT STICK"),
    ("GRAB / RELEASE", "E", "SQUARE / R2"),
    ("ROTATE HELD", "HOLD R + MOUSE", "HOLD R1 + RIGHT STICK"),
    ("STOW / TAKE OUT", "F", "-"),
    ("PUT DOWN / PICK SLOT", "G / MOUSE WHEEL", "-"),
    ("PAUSE MENU", "ESC", "OPTIONS"),
    ("MENU: MOVE", "ARROWS / W A S D", "D-PAD"),
    ("MENU: CONFIRM", "ENTER / SPACE", "CROSS"),
    ("MUTE", "M", "CREATE"),
    ("FULLSCREEN", "ALT + ENTER", "PS BUTTON"),
];
const CREDITS_TEXT: [&str; 9] = [
    "ORIGINAL ENGINE: CODEPARADE (NONEUCLIDEAN, MIT)",
    "ESCHER RELATIVITY: BENOIT GAGNIER (CC-BY-4.0)",
    "BACKROOMS VR: CARLCAPU9 (CC-BY-4.0)",
    "ELEVATOR: EFX (CC-BY-4.0)",
    "POOL ROOMS, OVERGROWN ROOM: BLENDERUST (CC-BY-4.0)",
    "",
    "RUST PORT AND EXTENSIONS:",
    "GLOW + GLUTIN + WINIT, PORTAL RENDERER,",
    "SUPERLIMINAL-STYLE GRAB, RESIZE AND MENUS",
];

pub struct Menu {
    open: bool,
    screen: Screen,
    sel: usize,
    /// Local view of the audio mute flag, flipped whenever `ToggleMute` fires.
    muted: bool,
}

impl Menu {
    pub fn new() -> Menu {
        // The mute row mirrors the saved setting, not a fresh `false`: `MAIN MENU` builds a new
        // `Menu`, and a screen that claims audio is on while it is off is worse than no screen.
        Menu { open: true, screen: Screen::Title, sel: 0, muted: settings::muted() }
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

    /// Draw the current screen. The engine has already called `Ui::begin` and, for the pause
    /// menu, dimmed the scene; this only lays out text.
    pub fn draw(&self, ui: &Ui, scene_names: &[&str]) {
        let (w, h) = ui.size();
        let cx = w * 0.5;
        // The wash belongs to the menu, not to the engine: which screen is up is exactly what
        // decides how much of it there should be, and only this module knows that.
        let dim = match self.screen {
            Screen::Title | Screen::Options => DIM_SHOWCASE,
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
            Screen::Credits => {
                for (i, line) in CREDITS_TEXT.iter().enumerate() {
                    let y = h * (SUB_LIST_Y + i as f32 * SMALL_LINE_H);
                    ui.draw_text(line, cx, y, h * SMALL_SIZE, WHITE, Align::Center);
                }
                // BACK sits a clear small line under the last credit. The text column is
                // `CREDITS_TEXT.len()` rows of `SMALL_LINE_H` from `SUB_LIST_Y`, so a credit
                // added to the table pushes the row down rather than into the text; the
                // `credits_fit_above_the_footer` test holds the whole column above the footer.
                self.draw_rows(ui, &CREDITS_ROWS, CREDITS_BACK_Y, LINE_H, ITEM_SIZE);
            }
            Screen::Levels => self.draw_levels(ui, scene_names),
        }

        // Footer: navigation hints hug the left edge, the back hint the right edge, so the
        // line reads as two groups instead of one run of words.
        let margin = w * HINT_MARGIN;
        ui.draw_text(
            "UP/DOWN  SELECT     ENTER  CONFIRM",
            margin,
            h * HINT_Y,
            h * HINT_SIZE,
            DIM,
            Align::Left,
        );
        ui.draw_text("ESC  BACK", w - margin, h * HINT_Y, h * HINT_SIZE, DIM, Align::Right);
    }

    /// A centred column of rows starting at `y0` (fraction of height); the selected one is
    /// gold and carries the marker.
    fn draw_rows(&self, ui: &Ui, rows: &[&str], y0: f32, line_h: f32, size: f32) {
        let (w, h) = ui.size();
        for (i, row) in rows.iter().enumerate() {
            let y = h * (y0 + i as f32 * line_h);
            if i == self.sel {
                let text = format!("{MARKER}{row}");
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
                    if i == self.sel { format!("{MARKER}{label}") } else { label.to_string() };
                ui.draw_text(&text, w * 0.5, y, size, colour, Align::Center);
                continue;
            };
            let gutter = w * VALUE_GUTTER;
            let label = if i == self.sel { format!("{MARKER}{label}") } else { label.to_string() };
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
                let text = format!("{MARKER}{name}");
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

    /// The credits column has a row budget: the BACK row is placed under the last credit
    /// and the footer hints are at `HINT_Y`, so a credit line too many would put the two on
    /// top of each other. Hold BACK's whole glyph box above the footer with a line to spare.
    #[test]
    fn credits_fit_above_the_footer() {
        const { assert!(CREDITS_BACK_Y + ITEM_SIZE + SMALL_LINE_H < HINT_Y) }
    }

    /// The key map has the same budget at the bottom and a heading to clear at the top. Jumping
    /// and the inventory took it to thirteen rows, which is why it has its own `KEYMAP_Y` /
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
