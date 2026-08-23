//! EXT: player-facing settings, and the small file they survive in. Not part of the C++ port.
//!
//! The original engine has no settings of any kind: `GH_MOUSE_SENSITIVITY` is a compile-time
//! constant read straight out of `Player::Look` (Player.cpp:74,82), so the only way to change
//! how the game feels is to rebuild it. That is fine for a demo and not fine for a game --
//! mouse sensitivity in particular is the one setting nobody's hands agree on.
//!
//! # Why thread-local cells rather than a field on `Engine`
//!
//! Exactly the reasoning in `view.rs`: the values are read from *ported* code (`Player::Look`
//! and the gamepad's look conversion), which has no path to the extension state and no
//! extension point to grow one. The engine is single-threaded throughout, so a thread-local is
//! precisely as shared as it needs to be, and the ported call sites stay one-line hooks.
//!
//! # Levels, not multipliers
//!
//! Sensitivity is stored as an integer 1..=[`LEVELS`] and converted to a multiplier by
//! [`scale_of`]. A menu that steps through ten labelled notches is navigable with a D-pad in a
//! way that a continuous float is not, and the geometric ladder means every notch changes the
//! feel by the same *proportion* -- the difference between 1 and 2 is as big a change as the
//! difference between 9 and 10, which is not true of a linear one.
//!
//! # Degrading
//!
//! Like `audio.rs`, everything here is a no-op on failure. An unreadable or corrupt settings
//! file leaves the defaults in place, and an unwritable one loses the change at exit rather
//! than interrupting the game to say so. A game must not refuse to run over its own preferences.

use std::cell::Cell;
use std::path::Path;

/// Where preferences live, relative to the working directory -- the same place `Meshes/`,
/// `Shaders/` and `assets/` are read from, so the game keeps all of its files in one place.
const FILE: &str = "settings.cfg";

/// Number of sensitivity notches, and the one that means "unchanged from the ported default".
pub const LEVELS: i32 = 10;
pub const DEFAULT_LEVEL: i32 = 5;

/// Ratio between adjacent notches. 1.25 spans 0.41x to 3.05x across the ten, which brackets
/// both the players who play on a mousepad corner and the ones who play on a desk.
const STEP: f32 = 1.25;

thread_local! {
    static MOUSE: Cell<i32> = const { Cell::new(DEFAULT_LEVEL) };
    static PAD: Cell<i32> = const { Cell::new(DEFAULT_LEVEL) };
    static MUTED: Cell<bool> = const { Cell::new(false) };
    /// Set by every change, cleared by [`flush`]. The menu changes settings from inside input
    /// handling, which is no place to touch the filesystem: a held D-pad direction would put a
    /// write between the player and the value they are trying to land on. Marking instead, and
    /// letting the engine flush once a frame, keeps the write off that path and coalesces a
    /// run of adjustments into one. It also keeps `Menu`'s unit tests from writing a file.
    static DIRTY: Cell<bool> = const { Cell::new(false) };
}

/// Multiplier for sensitivity notch `level`. `DEFAULT_LEVEL` is exactly 1.0, so a player who
/// never opens the menu gets the ported feel bit for bit.
pub fn scale_of(level: i32) -> f32 {
    STEP.powi(level.clamp(1, LEVELS) - DEFAULT_LEVEL)
}

pub fn mouse_level() -> i32 {
    MOUSE.with(|c| c.get())
}

pub fn pad_level() -> i32 {
    PAD.with(|c| c.get())
}

pub fn muted() -> bool {
    MUTED.with(|c| c.get())
}

/// Look multiplier for mouse motion. Read by `Player::update_player`.
pub fn mouse_scale() -> f32 {
    scale_of(mouse_level())
}

/// Look multiplier for right-stick motion. Read by `ext::gamepad::Gamepads::poll`.
pub fn pad_scale() -> f32 {
    scale_of(pad_level())
}

/// Nudge a sensitivity by `delta` notches, saturating at either end. Returns the new level.
///
/// Saturating rather than wrapping: rolling from the least sensitive straight to the most is
/// the kind of surprise a settings screen should never spring on anybody, and a value row that
/// simply stops is also how it reads on screen -- the arrow beside it goes away.
pub fn adjust_mouse(delta: i32) -> i32 {
    set_level(&MOUSE, mouse_level() + delta)
}

pub fn adjust_pad(delta: i32) -> i32 {
    set_level(&PAD, pad_level() + delta)
}

pub fn set_muted(on: bool) {
    MUTED.with(|c| c.set(on));
    DIRTY.with(|d| d.set(true));
}

fn set_level(cell: &'static std::thread::LocalKey<Cell<i32>>, v: i32) -> i32 {
    let v = v.clamp(1, LEVELS);
    cell.with(|c| c.set(v));
    DIRTY.with(|d| d.set(true));
    v
}

/// Read `settings.cfg` if it is there.
pub fn load() {
    if let Ok(text) = std::fs::read_to_string(Path::new(FILE)) {
        apply(&text);
        DIRTY.with(|d| d.set(false));
    }
}

/// Apply a settings file's contents. Silently keeps the default for anything missing or
/// malformed, so a hand-edited file with one bad line still applies its good ones.
fn apply(text: &str) {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let (key, value) = (key.trim(), value.trim());
        match key {
            "mouse_sensitivity" => {
                if let Ok(v) = value.parse::<i32>() {
                    MOUSE.with(|c| c.set(v.clamp(1, LEVELS)));
                }
            }
            "pad_sensitivity" => {
                if let Ok(v) = value.parse::<i32>() {
                    PAD.with(|c| c.set(v.clamp(1, LEVELS)));
                }
            }
            "muted" => MUTED.with(|c| c.set(value == "1" || value.eq_ignore_ascii_case("true"))),
            _ => {}
        }
    }
}

/// Write the settings back if any have changed since the last write. Called once per frame by
/// the engine; on the overwhelming majority of frames it is a single `Cell` read.
pub fn flush() {
    if !DIRTY.with(|d| d.replace(false)) {
        return;
    }
    // A failed write loses the preference, which is a smaller problem than interrupting a game
    // to report it -- and there is nowhere to report it to. Same bargain as `audio.rs`.
    let _ = std::fs::write(Path::new(FILE), serialize());
}

/// The file's contents for the current values.
fn serialize() -> String {
    format!(
        "# DayDreams settings. Sensitivity is a notch from 1 to {LEVELS}; {DEFAULT_LEVEL} is the default.\n\
         mouse_sensitivity = {}\n\
         pad_sensitivity = {}\n\
         muted = {}\n",
        mouse_level(),
        pad_level(),
        u8::from(muted()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default notch must be exactly 1.0, or every player who never opens the menu is
    /// silently playing at a different sensitivity from the ported original.
    #[test]
    fn default_level_is_unity() {
        assert!((scale_of(DEFAULT_LEVEL) - 1.0).abs() < 1e-6);
    }

    /// Monotonic, bounded, and spanning a range worth having: if the ends were much closer
    /// together the whole screen would be decoration.
    #[test]
    fn ladder_is_monotonic_and_wide() {
        for l in 2..=LEVELS {
            assert!(scale_of(l) > scale_of(l - 1), "notch {l} is not above {}", l - 1);
        }
        assert!(scale_of(1) < 0.5, "least sensitive is only {}x", scale_of(1));
        assert!(scale_of(LEVELS) > 2.5, "most sensitive is only {}x", scale_of(LEVELS));
    }

    /// Out-of-range levels clamp rather than producing an absurd multiplier -- `scale_of` is
    /// reached from a settings file a player may have edited by hand.
    #[test]
    fn scale_clamps_out_of_range_levels() {
        assert_eq!(scale_of(-100), scale_of(1));
        assert_eq!(scale_of(9999), scale_of(LEVELS));
    }

    /// What `flush` writes must be what `load` reads. These are separate pieces of formatting
    /// and parsing, and nothing else would notice them drifting apart until a player's settings
    /// quietly stopped surviving a restart.
    ///
    /// Each test gets its own copy of the thread-locals, so this cannot disturb its neighbours.
    #[test]
    fn settings_round_trip_through_the_file_format() {
        adjust_mouse(3);
        adjust_pad(-2);
        set_muted(true);
        let (m, p) = (mouse_level(), pad_level());
        let text = serialize();

        adjust_mouse(-99);
        adjust_pad(99);
        set_muted(false);
        apply(&text);
        assert_eq!((mouse_level(), pad_level(), muted()), (m, p, true));
    }

    /// A file that is partly nonsense must still apply the lines that are not, and must never
    /// leave a level outside the ladder however the file was edited.
    #[test]
    fn malformed_lines_are_skipped_and_levels_clamped() {
        apply("# comment\n\nnonsense\nmouse_sensitivity = banana\npad_sensitivity = 900\nmuted = 1\n");
        assert_eq!(mouse_level(), DEFAULT_LEVEL, "a bad value should leave the default alone");
        assert_eq!(pad_level(), LEVELS, "an out-of-range level should clamp, not stick");
        assert!(muted());
    }

    /// Adjusting stops at the ends instead of wrapping round to the opposite extreme.
    #[test]
    fn adjust_saturates_at_both_ends() {
        for _ in 0..LEVELS * 2 {
            adjust_mouse(1);
        }
        assert_eq!(mouse_level(), LEVELS);
        for _ in 0..LEVELS * 2 {
            adjust_mouse(-1);
        }
        assert_eq!(mouse_level(), 1);
    }

    /// `flush` must not write when nothing changed -- it runs every frame.
    #[test]
    fn flush_is_a_no_op_until_something_changes() {
        DIRTY.with(|d| d.set(false));
        assert!(!DIRTY.with(|d| d.get()));
        adjust_pad(1);
        assert!(DIRTY.with(|d| d.get()), "a change must mark the settings dirty");
    }
}
