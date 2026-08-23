//! EXT: the command line. Replaces the hand-rolled `parse_dev_args` loop that main.rs grew
//! one flag at a time, which silently defaulted anything it could not parse (`--frames ten`
//! ran 90 frames, `--pos 1,2` placed nobody) and had to be read to be known.
//!
//! Every flag keeps its old name and meaning, so the README's examples still work:
//! `--shot` on its own photographs the title screen; `--yaw`, `--pitch`, `--pos` and the held
//! keys only mean something with `--scene`.

use std::ffi::OsString;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use log::LevelFilter;

#[derive(Parser, Debug)]
#[command(name = "daydreams", version, about = "DayDreams", disable_help_subcommand = true)]
pub struct Args {
    /// Open a 1280x720 window instead of taking the whole display.
    #[arg(long)]
    pub windowed: bool,

    /// Swap interval 0, so `[shot]` frame times measure the renderer rather than the panel.
    #[arg(long)]
    pub no_vsync: bool,

    /// No sound for this run, whatever the saved setting says (also: DAYDREAMS_MUTE=1).
    /// The `M` key cannot lift it and nothing is written to the settings file.
    #[arg(long)]
    pub mute: bool,

    /// Directory holding Shaders/, Meshes/, Textures/ and assets/ (also: DAYDREAMS_ASSETS).
    #[arg(long, value_name = "DIR")]
    pub assets: Option<PathBuf>,

    /// off, error, warn, info, debug or trace (also: DAYDREAMS_LOG). Default info.
    #[arg(long, value_name = "LEVEL")]
    pub log_level: Option<LevelFilter>,

    /// Log to the terminal only; do not write the per-user log file.
    #[arg(long)]
    pub no_log_file: bool,

    /// Skip the title and load scene N (0-based, in key order).
    #[arg(long, value_name = "N", value_parser = parse_scene)]
    pub scene: Option<usize>,

    /// Save a screenshot here after --frames frames and quit. Alone: the title screen.
    #[arg(long, value_name = "FILE")]
    pub shot: Option<PathBuf>,

    /// Frames to render before the screenshot, so physics settles.
    #[arg(long, default_value_t = 90, value_name = "K")]
    pub frames: u32,

    /// Camera yaw in degrees (with --scene). Default 0.
    #[arg(long, allow_negative_numbers = true, value_name = "DEG")]
    pub yaw: Option<f32>,

    /// Camera pitch in degrees (with --scene). Default 0.
    #[arg(long, allow_negative_numbers = true, value_name = "DEG")]
    pub pitch: Option<f32>,

    /// Player position (with --scene).
    #[arg(long, value_parser = parse_pos, allow_hyphen_values = true, value_name = "X,Y,Z")]
    pub pos: Option<[f32; 3]>,

    /// Hold W for the whole run.
    #[arg(long)]
    pub forward: bool,

    /// Hold A for the whole run.
    #[arg(long)]
    pub strafe: bool,

    /// Hold Shift for the whole run.
    #[arg(long)]
    pub sprint: bool,

    /// With `--scene`: load it as the elevator would deliver you -- doors shut, screen black,
    /// stood in the cabin, facing its doors -- so an arrival can be photographed. Hidden: dev
    /// tooling.
    #[arg(long, hide = true, requires = "scene")]
    pub arrive: bool,

    /// With `--scene`: on rendered frame N, press E once -- as the key would be, so it rides
    /// only with the player stood in an idle cabin (`--arrive`, or `--pos` inside one) and
    /// otherwise grabs, which is to say does nothing. With enough `--frames` after it, the
    /// `[load]` line and the screenshot show the far floor with the doors open again. Hidden:
    /// dev tooling; the README's "Elevator" section has the commands.
    #[arg(long, hide = true, requires = "scene", value_name = "FRAME")]
    pub ride_at: Option<u32>,

    /// With `--scene`: the Backrooms' window is built at this physical scale
    /// (`ext/window.rs`), so a screenshot can show it at door size without a grab. Hidden:
    /// dev tooling; the README's "The window" section has the commands.
    #[arg(long, hide = true, requires = "scene", value_name = "S")]
    pub window_scale: Option<f32>,

    /// With `--scene`: the window is built already unlocked, as if its key had been used.
    /// Hidden: dev tooling.
    #[arg(long, hide = true, requires = "scene")]
    pub unlock_window: bool,

    /// Panic after the first frame, to exercise the crash dialog. Hidden: it is a test of the
    /// platform layer, not a feature.
    #[arg(long, hide = true)]
    pub panic_test: bool,

    /// With `--scene`: lift every rigid-body prop by H metres once the scene has loaded, so
    /// the `[prop]` lines at shot time show where they landed and the screenshot how they
    /// lie (src/ext/physics.rs). Hidden: dev tooling.
    #[arg(long, hide = true, requires = "scene", value_name = "H")]
    pub drop_props: Option<f32>,

    /// Skip the title and open a scene holding only this glTF model, at its own scale, under
    /// interior lighting (src/ext/glbview.rs): a screenshot of any file is one command. The
    /// path is taken relative to the working directory when it exists there, else relative
    /// to the asset root. Hidden: a loader tool, not a feature. Excludes `--scene`.
    #[arg(long, hide = true, value_name = "PATH", conflicts_with = "scene", value_parser = parse_glb)]
    pub view_glb: Option<String>,

    /// With `--view-glb`: material names, comma-separated, to draw translucent rather than
    /// alpha-tested (`gltf_model::Load::translucent`).
    #[arg(long, hide = true, value_name = "NAMES", requires = "view_glb", value_delimiter = ',')]
    pub view_translucent: Vec<String>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug, PartialEq, Eq)]
pub enum Command {
    /// Regenerate Meshes/meadow_tile.obj from ext::terrain::height and exit.
    GenTerrain,
}

/// What a dev run asks of the engine (`Engine::start_direct`): the scene to start on, the
/// screenshot to take and when, the camera, the keys held down, and the elevator flags. Built
/// by [`Args::direct_run`] when any of `--scene`, `--view-glb` or `--shot` is given.
#[derive(Debug, PartialEq)]
pub struct DirectRun {
    pub scene: Option<DirectScene>,
    pub shot: Option<PathBuf>,
    /// Frames to render before the screenshot; the engine counts down in an `i32`.
    pub frames: i32,
    /// `--yaw` / `--pitch`, in degrees; `None` where the flag was not given.
    pub yaw: Option<f32>,
    pub pitch: Option<f32>,
    pub pos: Option<[f32; 3]>,
    /// Key slots held down every frame (`--forward`, `--strafe`, `--sprint`).
    pub hold: Vec<usize>,
    pub arrive: bool,
    /// The rendered frame on which E is pressed once (`--ride-at`).
    pub ride_at: Option<i32>,
    /// Metres to lift the rigid-body props by at the start (`--drop-props`).
    pub drop_props: Option<f32>,
}

/// What the game starts on instead of the title, when a dev flag says so.
#[derive(Debug, PartialEq, Eq)]
pub enum DirectScene {
    /// `--scene N`: the registry's scene N.
    Index(usize),
    /// `--view-glb PATH [--view-translucent A,B]`: a scene of that one model.
    Glb { path: String, translucent: Vec<String> },
}

/// A scene index, checked against the registry at parse time so that `--scene 99` is a usage
/// error (exit 2) naming the range, rather than a run that silently stayed on the intro because
/// `Engine::start_direct` skipped the load. A custom parser instead of
/// `value_parser!(usize).range(..)` so the message can say what the range is the index of.
fn parse_scene(s: &str) -> Result<usize, String> {
    let last = crate::ext::scenes::SCENES.len() - 1;
    let n: usize = s.parse().map_err(|e| format!("'{s}': {e}"))?;
    if n > last {
        return Err(format!("{n} is not a scene; the registry has 0..={last}"));
    }
    Ok(n)
}

/// A model path for `--view-glb`: made absolute when it names a file under the working
/// directory, so that `--view-glb ~/Downloads/x.glb` and `--view-glb Meshes/x.glb` both work
/// -- the loader joins a relative path onto the asset root, which a download is not under.
fn parse_glb(s: &str) -> Result<String, String> {
    let p = std::path::Path::new(s);
    if p.is_relative() && p.is_file() {
        let abs = std::path::absolute(p).map_err(|e| format!("'{s}': {e}"))?;
        return abs.into_os_string().into_string().map_err(|_| format!("'{s}': not UTF-8"));
    }
    Ok(s.to_string())
}

/// `x,y,z`, each an f32. The old parser kept whichever components parsed and then dropped
/// the whole flag when fewer than three survived; this rejects the token instead.
fn parse_pos(s: &str) -> Result<[f32; 3], String> {
    let parts: Vec<&str> = s.split(',').map(str::trim).collect();
    if parts.len() != 3 {
        return Err(format!("expected three comma-separated numbers, got {}", parts.len()));
    }
    let mut out = [0.0f32; 3];
    for (slot, part) in out.iter_mut().zip(&parts) {
        *slot = part.parse().map_err(|e| format!("'{part}': {e}"))?;
    }
    Ok(out)
}

/// Finder launches a bundle with `-psn_0_NNNNN` (a process serial number) as its one
/// argument, which no clap definition would accept. Dropped before parsing; nothing else
/// starts with `-psn`.
fn is_process_serial(arg: &OsString) -> bool {
    arg.to_str().is_some_and(|s| s.starts_with("-psn"))
}

impl Args {
    /// Parse the process's own arguments, exiting with clap's usage message on a bad one
    /// (and with the help or version text for `--help` / `--version`).
    pub fn from_env() -> Args {
        Args::parse_from(std::env::args_os().filter(|a| !is_process_serial(a)))
    }

    /// The argument list as a test sees it: same filtering as `from_env`, errors returned.
    #[cfg(test)]
    fn try_from_tokens(tokens: &[&str]) -> Result<Args, clap::Error> {
        Args::try_parse_from(
            std::iter::once("daydreams")
                .chain(tokens.iter().copied())
                .map(OsString::from)
                .filter(|a| !is_process_serial(a)),
        )
    }

    /// The scene to start on instead of the title, if a dev flag asked for one.
    pub fn direct_scene(&self) -> Option<DirectScene> {
        if let Some(n) = self.scene {
            return Some(DirectScene::Index(n));
        }
        self.view_glb.as_ref().map(|path| DirectScene::Glb {
            path: path.clone(),
            translucent: self.view_translucent.clone(),
        })
    }

    /// The dev run the flags describe, or `None` when none of `--scene`, `--view-glb` and
    /// `--shot` was given and the game starts on its title as usual.
    pub fn direct_run(&self) -> Option<DirectRun> {
        let scene = self.direct_scene();
        if scene.is_none() && self.shot.is_none() {
            return None;
        }
        // `u32` on the command line (a negative count is a parse error there).
        let count = |n: u32| n.min(i32::MAX as u32) as i32;
        Some(DirectRun {
            scene,
            shot: self.shot.clone(),
            frames: count(self.frames),
            yaw: self.yaw,
            pitch: self.pitch,
            pos: self.pos,
            hold: self.held_keys(),
            arrive: self.arrive,
            ride_at: self.ride_at.map(count),
            drop_props: self.drop_props,
        })
    }

    /// How the Backrooms' window is built (`window::set_preset`): the default unless
    /// `--window-scale` / `--unlock-window` say otherwise.
    pub fn window_preset(&self) -> crate::ext::window::Preset {
        let default = crate::ext::window::Preset::default();
        crate::ext::window::Preset {
            p_scale: self.window_scale.unwrap_or(default.p_scale),
            unlocked: self.unlock_window,
        }
    }

    /// The key slots `--forward` / `--strafe` / `--sprint` hold down every frame
    /// (see `Engine::start_direct`).
    pub fn held_keys(&self) -> Vec<usize> {
        let mut hold = Vec::new();
        if self.forward {
            hold.push(b'W' as usize);
        }
        if self.strafe {
            hold.push(b'A' as usize);
        }
        if self.sprint {
            hold.push(crate::ext::sprint::KEY_SPRINT);
        }
        hold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_old_parser() {
        let a = Args::try_from_tokens(&[]).unwrap();
        assert!(!a.windowed && !a.no_vsync && !a.forward && !a.strafe && !a.sprint);
        assert_eq!(a.frames, 90);
        assert!(a.yaw.is_none() && a.pitch.is_none());
        assert!(a.scene.is_none() && a.shot.is_none() && a.pos.is_none());
        assert!(a.assets.is_none() && a.log_level.is_none());
        assert!(a.command.is_none());
        assert!(a.held_keys().is_empty());
        assert!(a.direct_run().is_none(), "no dev flags: the title");
    }

    #[test]
    fn the_readme_example_parses() {
        let a = Args::try_from_tokens(&[
            "--scene", "14", "--shot", "out.bmp", "--frames", "120", "--yaw", "30", "--pitch", "-5",
        ])
        .unwrap();
        assert_eq!(a.scene, Some(14));
        assert_eq!(a.shot.as_deref(), Some(std::path::Path::new("out.bmp")));
        assert_eq!(a.frames, 120);
        assert_eq!((a.yaw, a.pitch), (Some(30.0), Some(-5.0)));
        let run = a.direct_run().expect("a dev run");
        assert_eq!(run.scene, Some(DirectScene::Index(14)));
        assert_eq!((run.frames, run.yaw, run.pitch), (120, Some(30.0), Some(-5.0)));
        assert!(run.hold.is_empty() && !run.arrive && run.ride_at.is_none());
        // `--shot` alone is a run too: the title screen's photograph.
        let a = Args::try_from_tokens(&["--shot", "title.bmp"]).unwrap();
        let run = a.direct_run().expect("a dev run");
        assert!(run.scene.is_none() && run.shot.is_some());
    }

    #[test]
    fn pos_parses_three_components_including_negatives() {
        let a = Args::try_from_tokens(&["--pos", "-1.5, 2,3e1"]).unwrap();
        assert_eq!(a.pos, Some([-1.5, 2.0, 30.0]));
    }

    #[test]
    fn bad_values_are_errors_not_defaults() {
        assert!(Args::try_from_tokens(&["--pos", "1,2"]).is_err());
        assert!(Args::try_from_tokens(&["--pos", "1,two,3"]).is_err());
        assert!(Args::try_from_tokens(&["--frames", "ten"]).is_err());
        assert!(Args::try_from_tokens(&["--frames", "-1"]).is_err());
        assert!(Args::try_from_tokens(&["--scene", "x"]).is_err());
        assert!(Args::try_from_tokens(&["--scene", "-1"]).is_err());
        assert!(Args::try_from_tokens(&["--log-level", "loud"]).is_err());
        assert!(Args::try_from_tokens(&["--bogus"]).is_err());
    }

    #[test]
    fn scene_must_be_in_the_registry() {
        let n = crate::ext::scenes::SCENES.len();
        assert_eq!(Args::try_from_tokens(&["--scene", "0"]).unwrap().scene, Some(0));
        let last = (n - 1).to_string();
        assert_eq!(Args::try_from_tokens(&["--scene", &last]).unwrap().scene, Some(n - 1));
        let err = Args::try_from_tokens(&["--scene", &n.to_string()]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
        assert!(err.to_string().contains(&format!("0..={last}")), "{err}");
        assert!(Args::try_from_tokens(&["--scene", "99"]).is_err());
    }

    #[test]
    fn view_glb_is_a_direct_scene_and_excludes_scene() {
        let a = Args::try_from_tokens(&["--view-glb", "/x/y.glb"]).unwrap();
        assert_eq!(
            a.direct_scene(),
            Some(DirectScene::Glb { path: "/x/y.glb".into(), translucent: vec![] })
        );
        let a = Args::try_from_tokens(&[
            "--view-glb",
            "/x/y.glb",
            "--view-translucent",
            "Water.002,Glass",
        ])
        .unwrap();
        assert_eq!(a.view_translucent, vec!["Water.002", "Glass"]);
        assert!(matches!(a.direct_scene(), Some(DirectScene::Glb { .. })));
        // A path that exists under the working directory is made absolute; one that does
        // not is passed through for the asset root to resolve.
        let a = Args::try_from_tokens(&["--view-glb", "Cargo.toml"]).unwrap();
        assert!(std::path::Path::new(a.view_glb.as_deref().unwrap()).is_absolute());
        let a = Args::try_from_tokens(&["--view-glb", "Meshes/nope.glb"]).unwrap();
        assert_eq!(a.view_glb.as_deref(), Some("Meshes/nope.glb"));
        assert!(Args::try_from_tokens(&["--view-glb", "/x.glb", "--scene", "1"]).is_err());
        assert!(Args::try_from_tokens(&["--view-translucent", "Water"]).is_err());
        assert_eq!(
            Args::try_from_tokens(&["--scene", "3"]).unwrap().direct_scene(),
            Some(DirectScene::Index(3))
        );
        assert_eq!(Args::try_from_tokens(&[]).unwrap().direct_scene(), None);
    }

    #[test]
    fn finder_process_serial_is_ignored() {
        let a = Args::try_from_tokens(&["-psn_0_1234567", "--windowed"]).unwrap();
        assert!(a.windowed);
    }

    #[test]
    fn arrive_and_ride_at_need_a_scene() {
        assert!(Args::try_from_tokens(&["--arrive"]).is_err());
        assert!(Args::try_from_tokens(&["--ride-at", "30"]).is_err());
        assert!(Args::try_from_tokens(&["--scene", "16", "--arrive"]).unwrap().arrive);
        assert!(!Args::try_from_tokens(&["--scene", "16"]).unwrap().arrive);
        let a = Args::try_from_tokens(&["--scene", "16", "--arrive", "--ride-at", "30"]).unwrap();
        assert_eq!(a.ride_at, Some(30));
        assert_eq!(a.direct_run().unwrap().ride_at, Some(30));
        assert!(Args::try_from_tokens(&["--scene", "16", "--ride-at", "-1"]).is_err());
    }

    #[test]
    fn drop_props_needs_a_scene_and_a_height() {
        assert!(Args::try_from_tokens(&["--drop-props", "1"]).is_err());
        assert!(Args::try_from_tokens(&["--scene", "16", "--drop-props", "x"]).is_err());
        let a = Args::try_from_tokens(&["--scene", "16", "--drop-props", "1.5"]).unwrap();
        assert_eq!(a.drop_props, Some(1.5));
        assert_eq!(a.direct_run().unwrap().drop_props, Some(1.5));
        assert!(Args::try_from_tokens(&["--scene", "16"]).unwrap().drop_props.is_none());
    }

    #[test]
    fn the_window_flags_need_a_scene_and_make_a_preset() {
        use crate::ext::window::Preset;
        assert!(Args::try_from_tokens(&["--window-scale", "7"]).is_err());
        assert!(Args::try_from_tokens(&["--unlock-window"]).is_err());
        assert!(Args::try_from_tokens(&["--scene", "16", "--window-scale", "big"]).is_err());
        let a = Args::try_from_tokens(&["--scene", "16"]).unwrap();
        assert_eq!(a.window_preset(), Preset::default());
        let a = Args::try_from_tokens(&["--scene", "16", "--window-scale", "7", "--unlock-window"])
            .unwrap();
        assert_eq!(a.window_preset(), Preset { p_scale: 7.0, unlocked: true });
        let a = Args::try_from_tokens(&["--scene", "16", "--unlock-window"]).unwrap();
        assert_eq!(a.window_preset(), Preset { p_scale: 1.0, unlocked: true });
    }

    #[test]
    fn held_keys_follow_the_flags() {
        let a = Args::try_from_tokens(&["--forward", "--sprint"]).unwrap();
        assert_eq!(a.held_keys(), vec![b'W' as usize, crate::ext::sprint::KEY_SPRINT]);
        let a = Args::try_from_tokens(&["--strafe"]).unwrap();
        assert_eq!(a.held_keys(), vec![b'A' as usize]);
    }

    #[test]
    fn log_level_and_subcommand() {
        let a = Args::try_from_tokens(&["--log-level", "Debug"]).unwrap();
        assert_eq!(a.log_level, Some(LevelFilter::Debug));
        let a = Args::try_from_tokens(&["--windowed", "gen-terrain"]).unwrap();
        assert_eq!(a.command, Some(Command::GenTerrain));
        assert!(a.windowed);
    }

    #[test]
    fn version_comes_from_cargo() {
        let err = Args::try_from_tokens(&["--version"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayVersion);
        assert!(err.to_string().contains(env!("CARGO_PKG_VERSION")));
    }
}
