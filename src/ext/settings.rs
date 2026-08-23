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
//! # Where the file lives
//!
//! In the per-user config directory (`directories::ProjectDirs`), not beside the binary and
//! not in the working directory. The working directory is `/` when Finder launches a `.app`
//! and the bundle itself is read-only once signed, so a file relative to either is a file that
//! is never written. The legacy `./settings.cfg` is still read once, to carry a developer's
//! preferences across the move, and is not written again.
//!
//! # Degrading
//!
//! Like `audio.rs`, everything here is a no-op on failure. An unreadable or corrupt settings
//! file leaves the defaults in place (and says so in the log, once), a bad value keeps that one
//! field's default, an out-of-range notch is clamped, and an unwritable file loses the change
//! at exit rather than interrupting the game to say so. A game must not refuse to run over its
//! own preferences.

use std::cell::Cell;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize};

/// File name inside the per-user config directory.
const FILE: &str = "settings.toml";

/// The pre-TOML location, relative to the working directory. Read for migration only.
const LEGACY_FILE: &str = "settings.cfg";

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

/// The file's schema. Every field falls back to its default when missing or unreadable, so a
/// file from an older build, a hand-edited one with a typo in one line, or one with keys this
/// build does not know all load whatever they do say. Unknown keys are ignored (serde's
/// default), which is what lets an older build read a newer build's file.
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    #[serde(deserialize_with = "lenient_level")]
    mouse_sensitivity: i32,
    #[serde(deserialize_with = "lenient_level")]
    pad_sensitivity: i32,
    #[serde(deserialize_with = "lenient_bool")]
    muted: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { mouse_sensitivity: DEFAULT_LEVEL, pad_sensitivity: DEFAULT_LEVEL, muted: false }
    }
}

impl Settings {
    fn current() -> Settings {
        Settings { mouse_sensitivity: mouse_level(), pad_sensitivity: pad_level(), muted: muted() }
    }

    fn apply(&self) {
        MOUSE.with(|c| c.set(self.mouse_sensitivity));
        PAD.with(|c| c.set(self.pad_sensitivity));
        MUTED.with(|c| c.set(self.muted));
    }
}

/// A notch: a bad value keeps the default, an out-of-range one clamps. Deserialising through
/// `toml::Value` rather than straight to `i32` is what makes a type error in one field cost
/// only that field -- serde's own `i32` path would reject the whole file.
fn lenient_level<'de, D: Deserializer<'de>>(d: D) -> Result<i32, D::Error> {
    let v = toml::Value::deserialize(d)?;
    Ok(v.as_integer().map_or(DEFAULT_LEVEL, |i| i.clamp(1, i64::from(LEVELS)) as i32))
}

/// `muted`: a real boolean, or the `0`/`1` integer the pre-TOML format wrote, or a string
/// spelling of either -- the old parser accepted `1` and any case of `true`, and a file it
/// wrote must still read back the same.
fn lenient_bool<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(match toml::Value::deserialize(d)? {
        toml::Value::Boolean(b) => b,
        toml::Value::Integer(i) => i == 1,
        toml::Value::String(s) => s == "1" || s.eq_ignore_ascii_case("true"),
        _ => false,
    })
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

/// The settings file's path, or `None` on a machine with no home directory to put it in --
/// in which case nothing is persisted and the game runs on defaults.
fn path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "DayDreams").map(|d| d.config_dir().join(FILE))
}

/// Read the settings file if it is there. Called once, before anything reads a sensitivity.
pub fn load() {
    match path() {
        Some(p) => load_from(&p, Path::new(LEGACY_FILE)),
        None => log::warn!("[settings] no config directory; settings will not be saved"),
    }
}

/// [`load`] with the paths as parameters, so the tests can point it at a temp dir.
///
/// If `path` does not exist but `legacy` does, `legacy` is read once and `path` written
/// immediately: migrating lazily (marking dirty and waiting for a change) would re-read the
/// legacy file on every launch until the player touched a setting.
fn load_from(path: &Path, legacy: &Path) {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            apply(&text, path);
            DIRTY.with(|d| d.set(false));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Ok(text) = std::fs::read_to_string(legacy) {
                apply(&text, legacy);
                write(path);
            }
        }
        Err(e) => log::warn!("[settings] could not read {}: {e}; using defaults", path.display()),
    }
}

/// Apply a settings file's contents. A file that does not parse at all leaves every default
/// in place and says so; a file that parses applies what it can, field by field.
fn apply(text: &str, path: &Path) {
    match toml::from_str::<Settings>(text) {
        Ok(s) => s.apply(),
        Err(e) => {
            // `message()`, not `{e}`: Display renders the offending line with a caret under it,
            // three lines where the log wants one. The position is in the span the message
            // already names.
            log::warn!(
                "[settings] {} is not valid TOML; using defaults: {}",
                path.display(),
                e.message()
            );
            Settings::default().apply();
        }
    }
}

/// Write the settings back if any have changed since the last write. Called once per frame by
/// the engine; on the overwhelming majority of frames it is a single `Cell` read.
pub fn flush() {
    if !DIRTY.with(|d| d.replace(false)) {
        return;
    }
    if let Some(p) = path() {
        write(&p);
    }
}

/// [`flush`] with the path as a parameter, for the tests. Same dirty gate.
#[cfg(test)]
fn flush_to(path: &Path) {
    if DIRTY.with(|d| d.replace(false)) {
        write(path);
    }
}

/// Write the current values to `path`, creating its directory on the way. A failed write loses
/// the preference, which is a smaller problem than interrupting a game to report it -- and
/// there is nowhere to report it to. Same bargain as `audio.rs`.
fn write(path: &Path) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, serialize());
}

/// The file's contents for the current values.
fn serialize() -> String {
    // `to_string` on a struct of three plain fields cannot fail; the `unwrap_or_default`
    // is there so a serde bug could only ever cost the file, never the frame.
    let body = toml::to_string(&Settings::current()).unwrap_or_default();
    format!(
        "# DayDreams settings. Sensitivity is a notch from 1 to {LEVELS}; {DEFAULT_LEVEL} is the default.\n{body}"
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
    fn settings_round_trip_through_the_file() {
        let dir = tempfile::tempdir().unwrap();
        // A directory that does not exist yet: the first write has to create it.
        let file = dir.path().join("DayDreams").join(FILE);
        let missing = dir.path().join("no-such.cfg");

        adjust_mouse(3);
        adjust_pad(-2);
        set_muted(true);
        let (m, p) = (mouse_level(), pad_level());
        flush_to(&file);
        assert!(file.exists(), "flush should create the file and its directory");

        adjust_mouse(-99);
        adjust_pad(99);
        set_muted(false);
        load_from(&file, &missing);
        assert_eq!((mouse_level(), pad_level(), muted()), (m, p, true));
        assert!(!DIRTY.with(|d| d.get()), "a load must leave nothing to flush");
    }

    /// The pre-TOML file -- `muted` written as `0`/`1`, a `#` comment on top -- must load
    /// unchanged, both from the new location and from the legacy one.
    #[test]
    fn legacy_format_loads_unchanged() {
        apply(
            "# DayDreams settings. Sensitivity is a notch from 1 to 10; 5 is the default.\n\
             mouse_sensitivity = 8\n\
             pad_sensitivity = 2\n\
             muted = 1\n",
            Path::new("test"),
        );
        assert_eq!((mouse_level(), pad_level(), muted()), (8, 2, true));
        apply("muted = 0\n", Path::new("test"));
        assert!(!muted());
        apply("muted = \"True\"\n", Path::new("test"));
        assert!(muted());
    }

    /// No new file but a legacy one: it is read once and the new file written from it, so the
    /// second launch does not need the legacy file at all.
    #[test]
    fn legacy_file_is_migrated_on_first_load() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(FILE);
        let legacy = dir.path().join(LEGACY_FILE);
        std::fs::write(&legacy, "mouse_sensitivity = 7\nmuted = 1\n").unwrap();

        load_from(&file, &legacy);
        assert_eq!((mouse_level(), pad_level(), muted()), (7, DEFAULT_LEVEL, true));
        assert!(file.exists(), "migration should write the new file");

        std::fs::remove_file(&legacy).unwrap();
        adjust_mouse(-99);
        load_from(&file, &legacy);
        assert_eq!(mouse_level(), 7, "the migrated file should stand on its own");
    }

    /// The new file, when it exists, is the one that counts: a legacy `./settings.cfg` left
    /// behind in the working directory must not override it on every launch, or a developer
    /// who tunes a setting in the menu sees it snap back the next run.
    #[test]
    fn new_file_wins_over_a_lingering_legacy_one() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(FILE);
        let legacy = dir.path().join(LEGACY_FILE);
        std::fs::write(&file, "mouse_sensitivity = 9
muted = false
").unwrap();
        std::fs::write(&legacy, "mouse_sensitivity = 2
muted = 1
").unwrap();

        load_from(&file, &legacy);
        assert_eq!((mouse_level(), muted()), (9, false));
        assert!(legacy.exists(), "the legacy file is left alone, not consumed");
    }

    /// A read error that is not "no such file" -- here, the path is a directory -- takes the
    /// third branch of `load_from`: defaults, no migration, no panic. The legacy file beside
    /// it must NOT be read in that case; that branch is for a missing file only.
    #[test]
    fn unreadable_file_gives_defaults_without_migrating() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(FILE);
        let legacy = dir.path().join(LEGACY_FILE);
        std::fs::create_dir(&file).unwrap();
        std::fs::write(&legacy, "mouse_sensitivity = 2
").unwrap();

        adjust_mouse(3);
        let before = mouse_level();
        load_from(&file, &legacy);
        assert_eq!(mouse_level(), before, "a read error leaves the values as they were");
        assert!(file.is_dir(), "nothing was written over the directory");
    }

    /// A float where a notch is expected is a wrong type, not a rounding question: `3.5`
    /// keeps the default rather than becoming 3 or 4 by a rule nobody wrote down.
    #[test]
    fn float_level_keeps_the_default() {
        apply("mouse_sensitivity = 3.5
pad_sensitivity = 2
", Path::new("test"));
        assert_eq!((mouse_level(), pad_level()), (DEFAULT_LEVEL, 2));
    }

    /// A file that is not TOML at all leaves every default in place and does not panic.
    #[test]
    fn corrupt_file_gives_defaults() {
        adjust_mouse(2);
        set_muted(true);
        apply("mouse_sensitivity = = 3\n[[[\n", Path::new("test"));
        assert_eq!((mouse_level(), pad_level(), muted()), (DEFAULT_LEVEL, DEFAULT_LEVEL, false));
    }

    /// A file that is TOML but has a wrong-typed value must still apply the fields that are
    /// right, and must never leave a level outside the ladder however the file was edited.
    #[test]
    fn bad_values_default_and_out_of_range_clamp() {
        apply(
            "# comment\n\nmouse_sensitivity = \"banana\"\npad_sensitivity = 900\nmuted = 1\n",
            Path::new("test"),
        );
        assert_eq!(mouse_level(), DEFAULT_LEVEL, "a bad value should leave the default alone");
        assert_eq!(pad_level(), LEVELS, "an out-of-range level should clamp, not stick");
        assert!(muted());
        apply("pad_sensitivity = -4\n", Path::new("test"));
        assert_eq!(pad_level(), 1);
    }

    /// Keys this build does not know are skipped, not fatal: a newer build's file must load
    /// in an older one.
    #[test]
    fn unknown_keys_are_ignored() {
        apply("fov = 90\nmouse_sensitivity = 3\n[future]\nthing = true\n", Path::new("test"));
        assert_eq!(mouse_level(), 3);
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
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(FILE);
        DIRTY.with(|d| d.set(false));
        flush_to(&file);
        assert!(!file.exists(), "a clean flush must not touch the filesystem");
        adjust_pad(1);
        assert!(DIRTY.with(|d| d.get()), "a change must mark the settings dirty");
        flush_to(&file);
        assert!(file.exists());
        assert!(!DIRTY.with(|d| d.get()), "a flush must clear the mark");
    }
}
