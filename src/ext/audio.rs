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
//! # Adding music
//!
//! Drop files into `assets/music/`. They are matched to scenes **by filename prefix**:
//!
//! ```text
//!   assets/music/01-tunnels.ogg     -> scene 1  (key 1)
//!   assets/music/03-pillars.mp3     -> scene 3  (key 3)
//!   assets/music/ambient.ogg        -> fallback for any scene with no specific track
//! ```
//!
//! Supported formats are whatever kira's symphonia backend decodes: ogg, mp3, wav, flac.
//!
//! # Adding sound effects
//!
//! Drop files into `assets/sfx/` named after the `Sfx` variants below (`grab.wav`,
//! `release.wav`, `portal.wav`, `land.wav`, `footstep.wav`). They are preloaded at startup and
//! fired by name. `Grab`, `Release` and `Footstep` have call sites (src/ext/mod.rs); `Portal`
//! and `Land` are indexed and loadable but nothing fires them yet. None of the files ship.

use kira::sound::static_sound::StaticSoundData;
use kira::sound::streaming::{StreamingSoundData, StreamingSoundHandle};
use kira::sound::{FromFileError, PlaybackState};
use kira::{AudioManager, AudioManagerSettings, DefaultBackend, Decibels, Easing, StartTime, Tween};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

const MUSIC_DIR: &str = "assets/music";
const SFX_DIR: &str = "assets/sfx";

/// Named one-shot effects. Drop matching files into `assets/sfx/` to hear the ones with call
/// sites (see the module docs for which).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Sfx {
    /// Player picked an object up.
    Grab,
    /// Player let an object go.
    Release,
    /// Something passed through a portal.
    Portal,
    /// A falling object hit the ground.
    Land,
    /// Footfall, driven by the head-bob phase (`Player::steps`, fired from `Engine::ext_update`).
    Footstep,
}

impl Sfx {
    fn file_stem(self) -> &'static str {
        match self {
            Sfx::Grab => "grab",
            Sfx::Release => "release",
            Sfx::Portal => "portal",
            Sfx::Land => "land",
            Sfx::Footstep => "footstep",
        }
    }

    pub const ALL: [Sfx; 5] = [Sfx::Grab, Sfx::Release, Sfx::Portal, Sfx::Land, Sfx::Footstep];
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
    /// scene index -> music file. Index `usize::MAX` holds the fallback track.
    music_files: HashMap<usize, PathBuf>,
    fallback_music: Option<PathBuf>,
    sfx: HashMap<Sfx, StaticSoundData>,
    current_music: Option<StreamingSoundHandle<FromFileError>>,
    /// Path of the track currently playing, so re-selecting the same track does not restart it.
    playing_path: Option<PathBuf>,
    current_scene: Option<usize>,
    music_volume: f32,
    sfx_volume: f32,
    muted: bool,
}

#[allow(dead_code)] // EXT: volume/inventory API is public for later use.
impl Audio {
    /// Initialise the mixer and index the asset directories. Never fails in a way that stops the
    /// engine -- a missing device just yields a silent `Audio`.
    pub fn new() -> Audio {
        let manager = match AudioManager::<DefaultBackend>::new(AudioManagerSettings::default()) {
            Ok(m) => Some(m),
            Err(e) => {
                log::warn!("[audio] no output device ({e}); running silent");
                None
            }
        };

        // EXT: under the resolved asset root (src/app/assets.rs), not the working directory.
        let (music_files, fallback_music) = index_music(&crate::app::assets::path(MUSIC_DIR));
        let sfx = load_sfx(&crate::app::assets::path(SFX_DIR));

        if manager.is_some() && music_files.is_empty() && fallback_music.is_none() {
            log::info!(
                "[audio] no music found in {MUSIC_DIR}/ -- drop ogg/mp3/wav/flac files there. \
                 Prefix a filename with a scene number (e.g. 01-tunnels.ogg) to bind it to that scene."
            );
        } else if manager.is_some() {
            // Say what was actually found, the way the gamepad module names the pad it sees.
            // Silence at startup is ambiguous -- it reads the same whether a track is queued or
            // the file was never picked up -- and the answer is one line.
            let tracks = music_files.len() + usize::from(fallback_music.is_some());
            log::info!("[audio] {tracks} track(s), {} effect(s)", sfx.len());
        }

        Audio {
            manager,
            music_files,
            fallback_music,
            sfx,
            current_music: None,
            playing_path: None,
            current_scene: None,
            music_volume: 0.7,
            sfx_volume: 0.9,
            muted: false,
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

        let path = self
            .music_files
            .get(&scene)
            .or(self.fallback_music.as_ref())
            .cloned();

        // Same track already playing for the previous scene? Let it run rather than restarting.
        let Some(path) = path else {
            self.stop_music(1.0);
            return;
        };

        if let Some(handle) = self.current_music.as_ref() {
            if handle.state() == PlaybackState::Playing && self.playing_path.as_deref() == Some(path.as_path()) {
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
        // `StaticSoundData` decodes the whole file into f32 samples and holds them: the eight
        // minutes of `ost.mp3` come to roughly 275 MB resident and a third of a second of stall
        // at the scene load that starts them -- measurable, and on the title screen it is the
        // first thing the game does. A streaming sound decodes ahead on kira's own thread into
        // a small buffer instead, so a track costs about the same whether it runs one minute or
        // twenty. Effects stay static: they are short, and they have to fire without a decode.
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

    /// Once per rendered frame. Streaming is the reason this exists: a static sound cannot fail
    /// after it starts playing, but a streamed one decodes as it goes and can hit a bad frame
    /// halfway through a track, which kira reports by queueing an error on the handle rather
    /// than by any louder means. Unpolled, that is music that simply stops with no explanation.
    pub fn tick(&mut self) {
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

    /// Fire a one-shot effect. Silently does nothing if the file is not present.
    pub fn play(&mut self, sfx: Sfx) {
        if self.muted {
            return;
        }
        let Some(manager) = self.manager.as_mut() else { return };
        let Some(data) = self.sfx.get(&sfx) else { return };
        let data = data.volume(Decibels(linear_to_db(self.sfx_volume)));
        let _ = manager.play(data);
    }

    /// Toggle mute; returns the new state. Bound to `M` and to the DualSense mute button.
    pub fn toggle_mute(&mut self) -> bool {
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
        (
            self.music_files.len() + usize::from(self.fallback_music.is_some()),
            self.sfx.len(),
        )
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
    matches!(
        p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(),
        Some("ogg" | "mp3" | "wav" | "flac")
    )
}

/// Index `assets/music/`, binding `NN-*` files to scene `NN` and treating the rest as fallback.
fn index_music(dir: &Path) -> (HashMap<usize, PathBuf>, Option<PathBuf>) {
    let mut by_scene = HashMap::new();
    let mut fallback = None;

    let Ok(entries) = std::fs::read_dir(dir) else {
        return (by_scene, fallback);
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_audio_ext(p))
        .collect();
    paths.sort();

    for path in paths {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        // Leading digits bind the track to a scene number (1-based, as the keys are).
        let digits: String = stem.chars().take_while(|c| c.is_ascii_digit()).collect();
        match digits.parse::<usize>() {
            Ok(n) if n >= 1 => {
                by_scene.insert(n - 1, path);
            }
            _ => {
                if fallback.is_none() {
                    fallback = Some(path);
                }
            }
        }
    }

    (by_scene, fallback)
}

fn load_sfx(dir: &Path) -> HashMap<Sfx, StaticSoundData> {
    let mut map = HashMap::new();
    if !dir.is_dir() {
        return map;
    }
    for sfx in Sfx::ALL {
        for ext in ["wav", "ogg", "mp3", "flac"] {
            let path = dir.join(format!("{}.{ext}", sfx.file_stem()));
            if path.is_file() {
                match StaticSoundData::from_file(&path) {
                    Ok(d) => {
                        map.insert(sfx, d);
                    }
                    Err(e) => log::error!("[audio] could not load {}: {e}", path.display()),
                }
                break;
            }
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
        assert!(is_audio_ext(Path::new("a/b/track.ogg")));
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
}
