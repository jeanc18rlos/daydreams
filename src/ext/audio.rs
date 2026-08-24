//! EXT: audio. Not part of the C++ port -- the original engine is completely silent.
//!
//! Built on `kira`, chosen over `rodio` because scene switching wants real crossfades and kira
//! exposes fade tweens on both start and stop.
//!
//! # Design notes
//!
//! Everything here **degrades to a no-op**. If there is no audio device, no `assets/` directory,
//! or no files in it, `Audio::new` still succeeds and every call quietly does nothing. A demo
//! that renders non-Euclidean geometry should not refuse to start because a sound file is
//! missing, and the engine has no error-reporting path to surface it through anyway.
//!
//! # Where the sounds come from
//!
//! All of them are synthesised by `tools/gen_sfx.py` -- filtered noise, sine partials and
//! envelopes, no recordings from anywhere (THIRD_PARTY.md). Regenerating writes the same
//! bytes, so `python3 tools/gen_sfx.py` is a no-op in `git status`.
//!
//! # Music
//!
//! Files in `assets/music/` are matched to scenes **by filename prefix**, and the number is the
//! scene's position in the registry counted from ONE -- the same numbering as the keys `1`..`7`
//! that select the ported levels:
//!
//! ```text
//!   assets/music/01-tunnels.flac    -> SCENES[0]  (key 1)
//!   assets/music/17-backrooms.flac  -> SCENES[16] (the Backrooms, src/level16.rs)
//!   assets/music/ambient.flac       -> fallback for any scene with no track of its own
//! ```
//!
//! The off-by-one is a trap worth stating twice: `src/level16.rs` is the seventeenth entry in
//! [`crate::ext::scenes::SCENES`], so its track is `17-`. `music_names_bind_to_their_scenes`
//! below asserts every shipped name against the registry so a rename cannot break it quietly.
//!
//! Supported formats are whatever kira's symphonia backend decodes: flac, ogg, mp3, wav.
//!
//! # Effects
//!
//! A [`Sfx`] names a file stem in `assets/sfx/`. A stem may be a **set**: `footstep_carpet_1`
//! through `_4` are one set, and playing it picks one at random -- never the one played last,
//! so a step never repeats itself -- with a small per-play wobble on gain and playback rate
//! ([`JITTER_DB`], [`JITTER_RATE`]). Four files and a wobble is the difference between a
//! corridor that sounds walked down and a corridor that sounds like a loop of one sample.
//!
//! Which footstep set plays is the level's business: each declares a [`Surface`] in its `load`
//! ([`set_surface`]), and the engine clears it before every load so a level that declares
//! nothing is silent underfoot rather than inheriting the last one's carpet.
//!
//! # Asking for a sound from inside the world
//!
//! `Audio` lives behind the engine's one `RefCell` (`ext::ExtState`), which an object being
//! updated cannot reach -- the engine is already holding it. Those callers use [`request`],
//! which queues a one-shot for [`Audio::tick`] to play on the next frame: one frame late, and
//! the alternative is a second `RefCell` on the hot path.

use kira::sound::static_sound::StaticSoundData;
use kira::sound::streaming::{StreamingSoundData, StreamingSoundHandle};
use kira::sound::{FromFileError, PlaybackState};
use kira::{
    AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Easing, StartTime, Tween,
};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// EXT: a mute that nothing in the game can undo -- `--mute` / `DAYDREAMS_MUTE=1`.
///
/// Test runs and screenshot jobs start the soundtrack like any other launch, which is exactly
/// wrong on a machine where someone is watching something else. Unlike the saved setting this
/// is per process, is never written to the settings file, and the `M` key cannot lift it.
static FORCE_MUTE: AtomicBool = AtomicBool::new(false);

/// Mute this process for good. Call before `Audio::new` (main.rs does, from the CLI/env).
pub fn force_mute() {
    FORCE_MUTE.store(true, Ordering::Relaxed);
}

fn forced() -> bool {
    FORCE_MUTE.load(Ordering::Relaxed)
}

const MUSIC_DIR: &str = "assets/music";
const SFX_DIR: &str = "assets/sfx";

/// Extensions tried for a stem, in the order the shipped set uses them.
const EXTS: [&str; 4] = ["flac", "ogg", "wav", "mp3"];

/// The most variants a set can hold. Four is what `tools/gen_sfx.py` writes for the footsteps;
/// the loader stops at the first gap anyway, so this is only a bound on the search.
const MAX_VARIANTS: usize = 8;

/// Per-play gain wobble, decibels either way.
const JITTER_DB: f32 = 2.0;

/// Per-play playback-rate wobble, a fraction either way. Four percent is two thirds of a
/// semitone: it reads as a different footfall, not as a sample played at the wrong speed.
const JITTER_RATE: f64 = 0.04;

/// How far below a water line a footfall still splashes. Deeper than this and the water is
/// over the player's head, which no level allows; shallower is where the wading happens.
const WADE_DEPTH: f32 = 1.2;

// ─────────────────────────────────────────────────────────────────────────────
// What is underfoot.
// ─────────────────────────────────────────────────────────────────────────────

/// The ground a level's footsteps are taken on. Each level names one in its `load`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Surface {
    /// Nothing declared: no footstep sound at all. Every ported level is this.
    None,
    /// The Backrooms' carpet.
    Carpet,
    /// Hard tile, with the heights of any standing water over it. A footfall less than
    /// [`WADE_DEPTH`] below one of those lines is a splash; above them all it is the dry
    /// slap. The Pool Rooms are two flooded storeys (0.78 m and 4.54 m) with a dry spiral
    /// stair climbing between them, which is the whole reason this is a list and not a bool.
    Tile(&'static [f32]),
    /// The Overgrown room's moss.
    Moss,
    /// The meadow.
    Grass,
}

impl Surface {
    /// Every footstep set the game can load, whichever surfaces are in play.
    const SETS: [&'static str; 5] =
        ["footstep_carpet", "footstep_tile", "footstep_water", "footstep_moss", "footstep_grass"];

    /// The set a footfall with the feet at `feet_y` belongs to.
    fn footsteps(self, feet_y: f32) -> Option<&'static str> {
        match self {
            Surface::None => None,
            Surface::Carpet => Some("footstep_carpet"),
            Surface::Tile(water) => {
                Some(if water.iter().any(|&line| feet_y < line && line - feet_y < WADE_DEPTH) {
                    "footstep_water"
                } else {
                    "footstep_tile"
                })
            }
            Surface::Moss => Some("footstep_moss"),
            Surface::Grass => Some("footstep_grass"),
        }
    }
}

thread_local! {
    /// What the level now playing walks on. Cleared by the engine before every scene load.
    static SURFACE: Cell<Surface> = const { Cell::new(Surface::None) };
    /// One-shots asked for from inside the world, drained by [`Audio::tick`].
    static QUEUE: RefCell<Vec<Sfx>> = const { RefCell::new(Vec::new()) };
}

/// Declare what this level's floor is. One line in a level's `load`; the engine clears it
/// before each load, so forgetting means silence rather than the last level's floor.
pub fn set_surface(surface: Surface) {
    SURFACE.with(|s| s.set(surface));
}

/// Ask for `sfx` on the next frame, from code that cannot reach `Audio` (see the module docs).
///
/// A sound already queued this frame is not queued twice: three props warping through the same
/// portal on one step are one whoosh, not three on top of each other.
pub fn request(sfx: Sfx) {
    QUEUE.with(|q| {
        let mut q = q.borrow_mut();
        if !q.contains(&sfx) {
            q.push(sfx);
        }
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// The effects.
// ─────────────────────────────────────────────────────────────────────────────

/// Named one-shots. The name is the file stem under `assets/sfx/`.
///
/// Footfalls are not here: which file one is depends on the [`Surface`], so they go through
/// [`Audio::footstep`] instead.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Sfx {
    /// Player picked an object up.
    Grab,
    /// Player let an object go.
    Release,
    /// An object put into the inventory, and taken back out.
    Stow,
    Retrieve,
    /// An object dropped, and the dull "no" of something that will not go where it was put.
    Drop,
    Refuse,
    /// Leaving the ground, and arriving back on it at two weights.
    Jump,
    LandSoft,
    LandHard,
    /// The key coming off the painting, and turning in the window's lock.
    KeyTake,
    KeyUse,
    /// The window reaching a size that can be walked through.
    WindowGrow,
    /// Something passed through a portal.
    Portal,
    /// The elevator, in the order one ride uses them: the press, the doors, the motor while
    /// the screen is black, and the arrival.
    ElevatorButton,
    ElevatorDoors,
    ElevatorRide,
    ElevatorDing,
    /// Moving through a menu, choosing, and backing out.
    UiMove,
    UiConfirm,
    UiBack,
    /// The intro door's heavy wooden swing.
    DoorOpen,
    DoorClose,
}

impl Sfx {
    fn file_stem(self) -> &'static str {
        match self {
            Sfx::Grab => "grab",
            Sfx::Release => "release",
            Sfx::Stow => "stow",
            Sfx::Retrieve => "retrieve",
            Sfx::Drop => "drop",
            Sfx::Refuse => "refuse",
            Sfx::Jump => "jump",
            Sfx::LandSoft => "land_soft",
            Sfx::LandHard => "land_hard",
            Sfx::KeyTake => "key_take",
            Sfx::KeyUse => "key_use",
            Sfx::WindowGrow => "window_grow",
            Sfx::Portal => "portal",
            Sfx::ElevatorButton => "elevator_button",
            Sfx::ElevatorDoors => "elevator_doors",
            Sfx::ElevatorRide => "elevator_ride",
            Sfx::ElevatorDing => "elevator_ding",
            Sfx::UiMove => "ui_move",
            Sfx::UiConfirm => "ui_confirm",
            Sfx::UiBack => "ui_back",
            Sfx::DoorOpen => "door_open",
            Sfx::DoorClose => "door_close",
        }
    }

    pub const ALL: [Sfx; 22] = [
        Sfx::Grab,
        Sfx::Release,
        Sfx::Stow,
        Sfx::Retrieve,
        Sfx::Drop,
        Sfx::Refuse,
        Sfx::Jump,
        Sfx::LandSoft,
        Sfx::LandHard,
        Sfx::KeyTake,
        Sfx::KeyUse,
        Sfx::WindowGrow,
        Sfx::Portal,
        Sfx::ElevatorButton,
        Sfx::ElevatorDoors,
        Sfx::ElevatorRide,
        Sfx::ElevatorDing,
        Sfx::UiMove,
        Sfx::UiConfirm,
        Sfx::UiBack,
        Sfx::DoorOpen,
        Sfx::DoorClose,
    ];
}

/// One sound: the files it may be, and which of them went last.
struct SfxSet {
    files: Vec<StaticSoundData>,
    /// Index of the file played last, so the next play can avoid it. `None` until first use.
    last: Option<usize>,
}

/// Pick a file from a set of `n`, never `last`. Uniform over the other `n - 1`.
///
/// Pure, because "never twice running" is the whole point of the set and is worth a test.
fn pick(n: usize, last: Option<usize>, r: u32) -> usize {
    match last {
        Some(prev) if n > 1 && prev < n => {
            // Draw from the n-1 survivors and step over the hole where `prev` was.
            let k = (r as usize) % (n - 1);
            if k >= prev {
                k + 1
            } else {
                k
            }
        }
        _ => (r as usize) % n.max(1),
    }
}

/// The per-play wobble: a gain in +/- [`JITTER_DB`] and a rate in 1 +/- [`JITTER_RATE`].
fn jitter(r: u32) -> (f32, f64) {
    let low = f32::from(r as u16) / f32::from(u16::MAX) * 2.0 - 1.0;
    let high = f64::from((r >> 16) as u16) / f64::from(u16::MAX) * 2.0 - 1.0;
    (low * JITTER_DB, 1.0 + high * JITTER_RATE)
}

/// Xorshift32. Five lines is the whole requirement here -- choose between four files, wobble a
/// gain -- and it keeps the audio module from reaching into the terrain generator's PRNG.
fn next_rand(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

fn tween(secs: f32) -> Tween {
    Tween {
        start_time: StartTime::Immediate,
        duration: Duration::from_secs_f32(secs),
        easing: Easing::Linear,
    }
}

pub struct Audio {
    manager: Option<AudioManager<DefaultBackend>>,
    /// scene index -> music file.
    music_files: HashMap<usize, PathBuf>,
    fallback_music: Option<PathBuf>,
    /// Effect sets by file stem -- `Sfx::file_stem` and `Surface::SETS` between them name
    /// every key.
    sets: HashMap<&'static str, SfxSet>,
    current_music: Option<StreamingSoundHandle<FromFileError>>,
    /// Path of the track currently playing, so re-selecting the same track does not restart it.
    playing_path: Option<PathBuf>,
    current_scene: Option<usize>,
    music_volume: f32,
    sfx_volume: f32,
    muted: bool,
    /// State of the per-play variation PRNG.
    rand: u32,
}

#[allow(dead_code)] // EXT: the volume API is public for later use.
impl Audio {
    /// Initialise the mixer and index the asset directories. Never fails in a way that stops the
    /// engine -- a missing device just yields a silent `Audio`.
    pub fn new() -> Audio {
        // EXT: a forced mute (`--mute`) means this process must not TOUCH the audio device,
        // not merely play nothing through it. Opening an output stream claims the default
        // device and starts kira's mixer thread, which on this machine has hung a headless
        // run on shutdown while another application held the device -- and a muted run has
        // nothing to play anyway.
        let manager = if forced() {
            log::info!("[audio] muted for this run (--mute); no output device opened");
            None
        } else {
            match AudioManager::<DefaultBackend>::new(AudioManagerSettings::default()) {
                Ok(m) => Some(m),
                Err(e) => {
                    log::warn!("[audio] no output device ({e}); running silent");
                    None
                }
            }
        };

        // EXT: under the resolved asset root (src/app/assets.rs), not the working directory.
        let (music_files, fallback_music) = index_music(&crate::app::assets::path(MUSIC_DIR));
        let sets = load_sets(&crate::app::assets::path(SFX_DIR));

        // Said whether or not a device was opened: what is on disk is a fact about the
        // install, and a muted run -- which loads every effect anyway, it just never plays
        // one -- is the run that most needs to say what it found.
        if music_files.is_empty() && fallback_music.is_none() {
            log::info!(
                "[audio] no music found in {MUSIC_DIR}/ -- run `python3 tools/gen_sfx.py` to \
                 build the shipped set, or drop flac/ogg/mp3/wav files there. A filename \
                 prefixed with a scene's position in the registry (e.g. 17-backrooms.flac) \
                 binds it to that scene."
            );
        } else {
            // Say what was actually found, the way the gamepad module names the pad it sees.
            // Silence at startup is ambiguous -- it reads the same whether a track is queued or
            // the file was never picked up -- and the answer is one line.
            let tracks = music_files.len() + usize::from(fallback_music.is_some());
            let files: usize = sets.values().map(|s| s.files.len()).sum();
            log::info!("[audio] {tracks} track(s), {} effect(s) in {files} file(s)", sets.len());
        }

        Audio {
            manager,
            music_files,
            fallback_music,
            sets,
            current_music: None,
            playing_path: None,
            current_scene: None,
            music_volume: 0.7,
            sfx_volume: 0.9,
            // A forced mute starts muted and stays so; `toggle_mute` honours it.
            muted: forced(),
            // Any non-zero seed: xorshift is stuck at zero, and nothing here wants the same
            // sequence twice anyway.
            rand: 0x2545_F491,
        }
    }

    /// Switch to the track bound to `scene`, crossfading out whatever is playing.
    ///
    /// Re-selecting the same scene is a no-op, so the music survives a scene reload.
    pub fn set_scene(&mut self, scene: usize) {
        if self.current_scene == Some(scene) {
            return;
        }
        self.current_scene = Some(scene);

        let path = self.music_files.get(&scene).or(self.fallback_music.as_ref()).cloned();

        // Same track already playing for the previous scene? Let it run rather than restarting.
        let Some(path) = path else {
            log::info!("[audio] scene {scene}: no track");
            self.stop_music(1.0);
            return;
        };
        // Named, not counted: which track a scene got is the one thing about this that can be
        // silently wrong (the prefix counts scenes from one -- see the module docs), and the
        // line is written even under `--mute`, where nothing is decoded and nothing plays.
        log::info!("[audio] scene {scene}: {}", path.display());

        if let Some(handle) = self.current_music.as_ref() {
            if handle.state() == PlaybackState::Playing
                && self.playing_path.as_deref() == Some(path.as_path())
            {
                return;
            }
        }

        self.stop_music(0.8);
        self.start_music(&path);
    }

    fn start_music(&mut self, path: &Path) {
        let Some(manager) = self.manager.as_mut() else { return };
        if self.muted {
            return;
        }
        // STREAMED, where the sound effects below are decoded up front.
        //
        // `StaticSoundData` decodes the whole file into f32 samples and holds them. The loops
        // are thirty seconds each, which is only a few megabytes, but a stream costs the same
        // whether a track runs one minute or twenty and the effects are what has to fire
        // without a decode. A streaming sound decodes ahead on kira's own thread into a small
        // buffer instead.
        let data = match StreamingSoundData::from_file(path) {
            Ok(d) => d,
            Err(e) => {
                log::error!("[audio] could not load {}: {e}", path.display());
                return;
            }
        };
        let data = data
            .loop_region(0.0..)
            .volume(Decibels(linear_to_db(self.music_volume)))
            .fade_in_tween(tween(1.2));

        match manager.play(data) {
            Ok(handle) => {
                self.current_music = Some(handle);
                self.playing_path = Some(path.to_path_buf());
            }
            Err(e) => log::error!("[audio] could not play {}: {e}", path.display()),
        }
    }

    /// Once per rendered frame: play what the world asked for, then check on the music.
    ///
    /// Streaming is the reason the second half exists: a static sound cannot fail after it
    /// starts playing, but a streamed one decodes as it goes and can hit a bad frame halfway
    /// through a track, which kira reports by queueing an error on the handle rather than by
    /// any louder means. Unpolled, that is music that simply stops with no explanation.
    pub fn tick(&mut self) {
        // Drained even with no device and even while muted: `play` is the no-op, not this, and
        // a queue nobody empties would grow for the length of the run.
        for sfx in QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut())) {
            self.play(sfx);
        }
        let Some(handle) = self.current_music.as_mut() else { return };
        while let Some(e) = handle.pop_error() {
            log::warn!("[audio] music stream error: {e}");
        }
    }

    pub fn stop_music(&mut self, fade_secs: f32) {
        if let Some(mut handle) = self.current_music.take() {
            handle.stop(tween(fade_secs));
        }
        self.playing_path = None;
    }

    /// Fire a one-shot effect. Silently does nothing if the files are not present.
    pub fn play(&mut self, sfx: Sfx) {
        self.play_set(sfx.file_stem());
    }

    /// Fire a footfall: the set the level's [`Surface`] names, with the player's feet at
    /// `feet_y` (only the flooded Pool Rooms read it).
    pub fn footstep(&mut self, feet_y: f32) {
        if let Some(set) = SURFACE.with(Cell::get).footsteps(feet_y) {
            self.play_set(set);
        }
    }

    /// One play from a named set: a file that is not the one played last, at a gain and rate
    /// nudged off centre.
    fn play_set(&mut self, name: &str) {
        if self.muted {
            return;
        }
        // Destructured rather than reached through `self`, so the manager and the set can be
        // borrowed mutably at the same time.
        let Audio { manager, sets, rand, sfx_volume, .. } = self;
        let Some(manager) = manager.as_mut() else { return };
        let Some(set) = sets.get_mut(name) else { return };
        let index = pick(set.files.len(), set.last, next_rand(rand));
        set.last = Some(index);
        let (gain_db, rate) = jitter(next_rand(rand));
        let data = set.files[index]
            .volume(Decibels(linear_to_db(*sfx_volume) + gain_db))
            .playback_rate(rate);
        let _ = manager.play(data);
    }

    /// Toggle mute; returns the new state. Bound to `M` and to the DualSense mute button.
    pub fn toggle_mute(&mut self) -> bool {
        if forced() {
            log::debug!("[audio] muted for this run (--mute); the toggle is ignored");
            return true;
        }
        self.muted = !self.muted;
        if self.muted {
            self.stop_music(0.3);
        } else if let Some(scene) = self.current_scene.take() {
            // Force a reselect so the track restarts.
            self.set_scene(scene);
        }
        self.muted
    }

    pub fn is_muted(&self) -> bool {
        self.muted
    }

    pub fn set_music_volume(&mut self, v: f32) {
        self.music_volume = v.clamp(0.0, 1.0);
    }

    pub fn set_sfx_volume(&mut self, v: f32) {
        self.sfx_volume = v.clamp(0.0, 1.0);
    }

    /// How many assets were actually found -- used for the startup banner.
    pub fn inventory(&self) -> (usize, usize) {
        (self.music_files.len() + usize::from(self.fallback_music.is_some()), self.sets.len())
    }
}

/// Convert a 0..1 linear fader position to decibels, with 0 mapping to silence.
fn linear_to_db(linear: f32) -> f32 {
    if linear <= 0.0001 {
        -60.0
    } else {
        20.0 * linear.log10()
    }
}

fn is_audio_ext(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

/// The scene a music filename binds to: leading digits, counted from one. `None` for a name
/// that starts with anything else, which makes it the fallback.
fn scene_of_stem(stem: &str) -> Option<usize> {
    let digits: String = stem.chars().take_while(char::is_ascii_digit).collect();
    digits.parse::<usize>().ok().filter(|&n| n >= 1).map(|n| n - 1)
}

/// Index `assets/music/`, binding `NN-*` files to scene `NN` and treating the rest as fallback.
fn index_music(dir: &Path) -> (HashMap<usize, PathBuf>, Option<PathBuf>) {
    let mut by_scene = HashMap::new();
    let mut fallback = None;

    let Ok(entries) = std::fs::read_dir(dir) else {
        return (by_scene, fallback);
    };
    let mut paths: Vec<PathBuf> =
        entries.flatten().map(|e| e.path()).filter(|p| p.is_file() && is_audio_ext(p)).collect();
    paths.sort();

    for path in paths {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        match scene_of_stem(stem) {
            Some(scene) => {
                by_scene.insert(scene, path);
            }
            None => {
                if fallback.is_none() {
                    fallback = Some(path);
                }
            }
        }
    }

    (by_scene, fallback)
}

/// The files behind one stem: `<stem>.<ext>` if it exists, otherwise `<stem>_1`, `<stem>_2`
/// and so on up to the first gap.
fn set_files(dir: &Path, stem: &str) -> Vec<PathBuf> {
    let first = EXTS.iter().map(|e| dir.join(format!("{stem}.{e}"))).find(|p| p.is_file());
    if let Some(path) = first {
        return vec![path];
    }
    (1..=MAX_VARIANTS)
        .map_while(|i| {
            EXTS.iter().map(|e| dir.join(format!("{stem}_{i}.{e}"))).find(|p| p.is_file())
        })
        .collect()
}

/// Decode every effect up front. A stem with no files is simply absent from the map, and
/// playing it is a no-op.
fn load_sets(dir: &Path) -> HashMap<&'static str, SfxSet> {
    let mut map = HashMap::new();
    if !dir.is_dir() {
        return map;
    }
    let stems = Sfx::ALL.iter().map(|s| s.file_stem()).chain(Surface::SETS);
    for stem in stems {
        let mut files = Vec::new();
        for path in set_files(dir, stem) {
            match StaticSoundData::from_file(&path) {
                Ok(d) => files.push(d),
                Err(e) => log::error!("[audio] could not load {}: {e}", path.display()),
            }
        }
        if !files.is_empty() {
            map.insert(stem, SfxSet { files, last: None });
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_to_db_maps_unity_and_silence() {
        assert!((linear_to_db(1.0) - 0.0).abs() < 1e-5, "unity gain should be 0 dB");
        assert!(linear_to_db(0.0) <= -60.0, "zero should be silence");
        assert!(linear_to_db(0.5) < 0.0 && linear_to_db(0.5) > -12.0);
    }

    #[test]
    fn audio_extensions_recognised() {
        assert!(is_audio_ext(Path::new("a/b/track.flac")));
        assert!(is_audio_ext(Path::new("track.MP3")));
        assert!(!is_audio_ext(Path::new("readme.txt")));
        assert!(!is_audio_ext(Path::new("noext")));
    }

    #[test]
    fn missing_music_dir_is_not_an_error() {
        let (by_scene, fallback) = index_music(Path::new("assets/definitely-not-here"));
        assert!(by_scene.is_empty());
        assert!(fallback.is_none());
    }

    #[test]
    fn a_missing_effect_is_a_no_op() {
        let sets = load_sets(Path::new("assets/definitely-not-here"));
        assert!(sets.is_empty(), "no directory, no sets");
        assert!(set_files(Path::new("assets/definitely-not-here"), "grab").is_empty());
    }

    /// The one rule the sets exist for: whatever the random number, the file that just played
    /// is not the one that plays next.
    #[test]
    fn a_set_never_repeats_the_last_file() {
        for n in 2..=4usize {
            for last in 0..n {
                for r in 0..64u32 {
                    let i = pick(n, Some(last), r);
                    assert!(i < n, "picked {i} out of {n}");
                    assert_ne!(i, last, "n={n} last={last} r={r} repeated the last file");
                }
            }
        }
    }

    /// And every other file is reachable, so four variants are four variants and not two.
    #[test]
    fn a_set_reaches_every_other_file() {
        let mut seen = [false; 4];
        for r in 0..64u32 {
            seen[pick(4, Some(2), r)] = true;
        }
        assert_eq!(seen, [true, true, false, true], "2 is excluded, the rest are reachable");
    }

    #[test]
    fn a_set_of_one_always_picks_it() {
        for r in 0..8u32 {
            assert_eq!(pick(1, Some(0), r), 0);
            assert_eq!(pick(1, None, r), 0);
        }
    }

    #[test]
    fn jitter_stays_inside_its_bounds() {
        let mut state = 1;
        let (mut min_gain, mut max_gain) = (f32::MAX, f32::MIN);
        let (mut min_rate, mut max_rate) = (f64::MAX, f64::MIN);
        for _ in 0..10_000 {
            let (gain, rate) = jitter(next_rand(&mut state));
            // The bounds are reached exactly at the ends of the range, so the comparison
            // needs the slack that 1.0 + 0.04 not being 1.04 demands.
            assert!(gain.abs() <= JITTER_DB + 1e-5, "gain {gain} dB is outside +/-{JITTER_DB}");
            assert!((rate - 1.0).abs() <= JITTER_RATE + 1e-9, "rate {rate} is outside the wobble");
            min_gain = min_gain.min(gain);
            max_gain = max_gain.max(gain);
            min_rate = min_rate.min(rate);
            max_rate = max_rate.max(rate);
        }
        // And it actually uses the range it is given, rather than sitting near the middle.
        assert!(min_gain < -1.8 && max_gain > 1.8, "gain spread {min_gain}..{max_gain}");
        assert!(min_rate < 0.965 && max_rate > 1.035, "rate spread {min_rate}..{max_rate}");
    }

    #[test]
    fn xorshift_does_not_stall() {
        let mut state = 0x2545_F491;
        let first = next_rand(&mut state);
        assert_ne!(first, next_rand(&mut state));
        assert_ne!(state, 0);
    }

    /// The Pool Rooms' two storeys: wading on either floor, dry on the stair between them.
    #[test]
    fn surfaces_map_to_their_sets() {
        assert_eq!(Surface::None.footsteps(0.0), None);
        assert_eq!(Surface::Carpet.footsteps(0.0), Some("footstep_carpet"));
        assert_eq!(Surface::Moss.footsteps(0.0), Some("footstep_moss"));
        assert_eq!(Surface::Grass.footsteps(8.0), Some("footstep_grass"));

        let pool = Surface::Tile(&[0.78, 4.54]);
        assert_eq!(pool.footsteps(0.0), Some("footstep_water"), "the lower hall is flooded");
        assert_eq!(pool.footsteps(4.2), Some("footstep_water"), "so is the upper storey");
        assert_eq!(pool.footsteps(2.0), Some("footstep_tile"), "the stair between is dry");
        assert_eq!(pool.footsteps(5.0), Some("footstep_tile"), "and so is anything above");
        assert_eq!(Surface::Tile(&[]).footsteps(0.0), Some("footstep_tile"), "dry tile is tile");
    }

    #[test]
    fn every_stem_is_unique() {
        let mut stems: Vec<&str> =
            Sfx::ALL.iter().map(|s| s.file_stem()).chain(Surface::SETS).collect();
        let count = stems.len();
        stems.sort_unstable();
        stems.dedup();
        assert_eq!(stems.len(), count, "two sounds share a file stem");
    }

    /// The music prefix counts scenes from ONE, so the Backrooms -- `src/level16.rs`, the
    /// seventeenth entry in the registry -- is `17-backrooms`. Every shipped name is checked
    /// against the registry here, because the off-by-one is invisible in the filename.
    #[test]
    fn music_names_bind_to_their_scenes() {
        use crate::ext::scenes;
        for (stem, scene) in [
            ("16-meadow", "Intro"),
            ("17-backrooms", "Backrooms"),
            ("18-pool", "Pool Rooms"),
            ("19-overgrown", "Overgrown"),
        ] {
            assert_eq!(
                scene_of_stem(stem),
                scenes::index_of(scene),
                "{stem} does not name the {scene} scene"
            );
        }
        assert_eq!(scene_of_stem("ambient"), None, "an unnumbered name is the fallback");
        assert_eq!(scene_of_stem("01-tunnels"), Some(0), "scene 1 is the first entry");
    }

    /// What the game actually ships, read off the disk: every stem the code can ask for has
    /// files behind it, the footsteps come in fours, and the loops are where they should be.
    #[test]
    fn the_shipped_set_is_complete() {
        let sfx = crate::app::assets::path(SFX_DIR);
        let music = crate::app::assets::path(MUSIC_DIR);
        if !sfx.is_dir() {
            return; // a source checkout without assets/ still builds and still tests
        }
        for stem in Sfx::ALL.iter().map(|s| s.file_stem()) {
            assert_eq!(set_files(&sfx, stem).len(), 1, "{stem} should be one file");
        }
        for stem in Surface::SETS {
            assert_eq!(set_files(&sfx, stem).len(), 4, "{stem} should be four variants");
        }
        let (by_scene, fallback) = index_music(&music);
        assert_eq!(by_scene.len(), 4, "four scenes have their own track");
        assert!(fallback.is_some(), "and everything else has the fallback");
    }

    /// Every shipped file decodes. `StaticSoundData` decodes the whole effect the way the
    /// game does at startup; `StreamingSoundData` parses a loop's header and codec the way
    /// `start_music` does. Neither needs an output device, so this makes no sound.
    #[test]
    fn the_shipped_set_decodes() {
        let sfx = crate::app::assets::path(SFX_DIR);
        if !sfx.is_dir() {
            return;
        }
        let sets = load_sets(&sfx);
        assert_eq!(sets.len(), Sfx::ALL.len() + Surface::SETS.len(), "a stem failed to load");
        let (by_scene, fallback) = index_music(&crate::app::assets::path(MUSIC_DIR));
        for path in by_scene.values().chain(fallback.as_ref()) {
            let data = StreamingSoundData::from_file(path)
                .unwrap_or_else(|e| panic!("{} does not decode: {e}", path.display()));
            let secs = data.duration().as_secs_f32();
            assert!((25.0..=65.0).contains(&secs), "{} is {secs} s long", path.display());
        }
    }

    #[test]
    fn requests_queue_once_each() {
        QUEUE.with(|q| q.borrow_mut().clear());
        request(Sfx::Portal);
        request(Sfx::Portal);
        request(Sfx::UiMove);
        let queued = QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()));
        assert_eq!(queued, [Sfx::Portal, Sfx::UiMove], "a repeat within a frame is dropped");
    }

    #[test]
    fn the_surface_is_what_was_last_set() {
        set_surface(Surface::Carpet);
        assert_eq!(SURFACE.with(Cell::get), Surface::Carpet);
        set_surface(Surface::None);
        assert_eq!(SURFACE.with(Cell::get), Surface::None);
    }
}
