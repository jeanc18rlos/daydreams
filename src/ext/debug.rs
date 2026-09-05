//! EXT: developer mode -- an on-screen readout of where the camera is, and a key that saves the
//! frame to the user's Documents folder. Not part of the C++ port.
//!
//! The two halves answer one question between them: **"what command reproduces this shot?"**
//! `--scene N --pos X,Y,Z --yaw D --pitch D` is how every screenshot in the docs is taken
//! (`src/app/cli.rs`), and until now those numbers had to be guessed, walked to and read out of
//! the `[shot]` log line afterwards. The overlay prints the line while you stand there, so a
//! good vantage found by hand becomes a command that can be run again after the level changes.
//!
//! # The save key
//!
//! `Cmd`+`S`+`C` on macOS, `Ctrl`+`S`+`C` elsewhere: hold the modifier, hold `S`, tap `C`.
//! The chord is read in `src/main.rs` straight off winit's key events rather than through
//! `Input`, for a reason that matters: `S` is *walk backwards*. Routed through `Input` the way
//! every other key is, holding it to arm the chord would walk the camera away from the very
//! shot being framed. So while the modifier is down the movement keys are swallowed -- the
//! player stands still, and `S` means only "the chord is armed".
//!
//! The file lands in `<Documents>/DayDreams/<scene>-NNN.png`, numbered from whatever is already
//! there so two sessions never overwrite each other. PNG rather than the BMP `--shot` writes:
//! these are files a person opens and shares, and the `image` crate is already a dependency for
//! the glTF loader's textures.
//!
//! The key works whether or not the overlay is showing -- including on the title screen, which
//! has no overlay of its own -- because wanting a picture of something is not the same as
//! wanting numbers printed over it.

use image::ImageEncoder as _;

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::ext::ui::{Align, Color, Ui, GOLD, WHITE};
use crate::vector::Vector3;

/// How long the "saved" toast stays up, in seconds.
const TOAST_SECS: f32 = 3.0;

/// Overlay geometry, as fractions of the drawable's height so the panel keeps its proportions
/// on any window -- the same convention `ext/hud.rs` and `ext/menu.rs` are written in.
const PAD: f32 = 0.018;
const LINE_H: f32 = 0.030;
const TEXT: f32 = 0.021;
const LABEL_W: f32 = 0.115;
const PANEL_W: f32 = 0.62;

/// Panel fill: dark and translucent, because the overlay hangs over whatever the level's floor
/// happens to be and the Backrooms' carpet is very nearly the gold the values are drawn in.
const PANEL: Color = [0.0, 0.0, 0.0, 0.62];
const LABEL: Color = [1.0, 1.0, 1.0, 0.45];

/// Developer mode: the overlay's on/off switch, plus the last thing the save key did.
///
/// `F3` toggles it and `--debug` starts with it on (`src/app/cli.rs`). It is deliberately not a
/// setting: nothing here survives a restart, because a readout left on by accident in a build
/// handed to somebody else is worse than having to press `F3` again.
#[derive(Default)]
pub struct DebugMode {
    pub on: bool,
    /// What the save key last reported, and when -- the toast under the panel. `Err` is kept as
    /// well as `Ok`: a save that failed because Documents is not writable has to say so on the
    /// screen, since the person pressing the key is looking at the game, not at the log.
    last: Option<(Result<PathBuf, String>, Instant)>,
}

impl DebugMode {
    pub fn toggle(&mut self) {
        self.on = !self.on;
        log::info!("[debug] overlay {}", if self.on { "on" } else { "off" });
    }

    /// Record the outcome of a save so the overlay can show it.
    pub fn report(&mut self, outcome: Result<PathBuf, String>) {
        match &outcome {
            Ok(p) => log::info!("[debug] saved {}", p.display()),
            Err(e) => log::warn!("[debug] save failed: {e}"),
        }
        self.last = Some((outcome, Instant::now()));
    }

    /// The toast line, or `None` once it has aged out.
    fn toast(&self) -> Option<(String, Color)> {
        let (outcome, at) = self.last.as_ref()?;
        if at.elapsed().as_secs_f32() > TOAST_SECS {
            return None;
        }
        Some(match outcome {
            Ok(p) => (format!("SAVED  {}", pretty(p)), GOLD),
            Err(e) => (format!("SAVE FAILED  {e}"), [1.0, 0.45, 0.4, 1.0]),
        })
    }
}

/// What the overlay prints. Assembled by `Engine::render` from the state it already has, so
/// this module reads nothing and owns nothing -- it is a formatter with a switch on it.
pub struct Info<'a> {
    pub scene_ix: usize,
    pub scene_name: &'a str,
    pub pos: Vector3,
    /// Yaw and pitch in degrees, in the same convention `--yaw` / `--pitch` take.
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub p_scale: f32,
    pub fov_deg: f32,
    /// Mean milliseconds per frame over the recent window, if enough frames have gone by.
    pub frame_ms: Option<f32>,
    pub held: Option<usize>,
}

impl Info<'_> {
    /// The line the whole overlay exists for: paste it after the binary and the shot comes back.
    ///
    /// Negative angles are written `--yaw=-90` rather than `--yaw -90`, because clap reads a
    /// leading `-` as the start of the next flag and the space form fails at the command line.
    pub fn command(&self) -> String {
        let mut s = format!(
            "--scene {} --pos {:.2},{:.2},{:.2} {} {}",
            self.scene_ix,
            self.pos.x,
            self.pos.y,
            self.pos.z,
            angle_flag("yaw", self.yaw_deg),
            angle_flag("pitch", self.pitch_deg),
        );
        // A scaling portal leaves the player at some other size, and a run always starts at 1:
        // without this the line reproduced the position of a half-size view at full size, which
        // is a different picture of a different place. Printed only when it is not 1, so the
        // ordinary line stays short.
        if (self.p_scale - 1.0).abs() > 1e-3 {
            s.push_str(&format!(" --p-scale {:.3}", self.p_scale));
        }
        s
    }
}

/// Fold an angle into (-180, 180].
///
/// The player's yaw accumulates without bound -- `Player::look` subtracts from it every mouse
/// step and nothing ever wraps it -- so after a few spins the raw value is several thousand
/// degrees. It aims the camera identically either way, but a `--yaw 3787.4` in the pasteable
/// line reads as a bug in the readout, and the shortest equivalent is what a person would have
/// written down themselves.
pub fn wrap_deg(deg: f32) -> f32 {
    let d = deg % 360.0;
    if d > 180.0 {
        d - 360.0
    } else if d <= -180.0 {
        d + 360.0
    } else {
        d
    }
}

/// `--yaw 90` or `--yaw=-90`, whichever the shell will accept.
fn angle_flag(name: &str, deg: f32) -> String {
    let deg = wrap_deg(deg);
    if deg < 0.0 {
        format!("--{name}={deg:.1}")
    } else {
        format!("--{name} {deg:.1}")
    }
}

/// Draw the panel. Call inside a `Ui::begin`/`end` bracket, in the main pass only -- inside
/// `Engine::render` it would be painted into every portal's framebuffer as well.
pub fn draw(ui: &Ui, dbg: &DebugMode, info: &Info) {
    let (w, h) = ui.size();
    let rows: [(&str, String); 5] = [
        ("SCENE", format!("{}  {}", info.scene_ix, info.scene_name)),
        ("POS", format!("{:.2}, {:.2}, {:.2}", info.pos.x, info.pos.y, info.pos.z)),
        ("LOOK", format!("yaw {:.1}   pitch {:.1}", wrap_deg(info.yaw_deg), info.pitch_deg)),
        (
            "SCALE",
            match info.held {
                Some(i) => {
                    format!("p_scale {:.3}   fov {:.0}   holding #{i}", info.p_scale, info.fov_deg)
                }
                None => format!("p_scale {:.3}   fov {:.0}", info.p_scale, info.fov_deg),
            },
        ),
        (
            "FRAME",
            match info.frame_ms {
                Some(ms) => format!("{ms:.2} ms   {:.0} fps", 1000.0 / ms.max(0.001)),
                None => "--".to_string(),
            },
        ),
    ];

    let toast = dbg.toast();
    let line_h = h * LINE_H;
    let pad = h * PAD;
    // Five rows, the command line, the key reminder, and the toast when there is one.
    let body = rows.len() as f32 + 1.0 + 1.0 + if toast.is_some() { 1.0 } else { 0.0 };
    let panel_h = pad * 2.0 + line_h * body;
    ui.fill_rect(pad, pad, w.min(h * PANEL_W), panel_h, PANEL);

    let x = pad * 2.0;
    let vx = x + h * LABEL_W;
    let mut y = pad * 2.0;
    for (label, value) in &rows {
        ui.draw_text(label, x, y, h * TEXT, LABEL, Align::Left);
        ui.draw_text(value, vx, y, h * TEXT, WHITE, Align::Left);
        y += line_h;
    }
    ui.draw_text("SHOT", x, y, h * TEXT, LABEL, Align::Left);
    ui.draw_text(&info.command(), vx, y, h * TEXT, GOLD, Align::Left);
    y += line_h;
    ui.draw_text(SAVE_HINT, x, y, h * TEXT, LABEL, Align::Left);
    y += line_h;
    if let Some((line, color)) = toast {
        ui.draw_text(&line, x, y, h * TEXT, color, Align::Left);
    }
}

/// The chord, spelled the way the platform spells it.
#[cfg(target_os = "macos")]
pub const SAVE_HINT: &str = "CMD+S+C  SAVE PNG TO DOCUMENTS      F3  HIDE";
#[cfg(not(target_os = "macos"))]
pub const SAVE_HINT: &str = "CTRL+S+C  SAVE PNG TO DOCUMENTS      F3  HIDE";

/// Read the back buffer and write it to `<Documents>/DayDreams/<scene>-NNN.png`.
///
/// Returns the path written, or a message fit to put on the screen. Called from `Engine::render`
/// after everything has been drawn -- the overlay included, so a saved frame shows the numbers
/// that produced it.
pub fn save(
    gl: &glow::Context,
    width: i32,
    height: i32,
    scene_name: &str,
) -> Result<PathBuf, String> {
    use glow::HasContext as _;

    if width <= 0 || height <= 0 {
        return Err("the window has no pixels".to_string());
    }
    let dir = shots_dir()?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(next_name(&dir, &slug(scene_name)));

    let mut px = vec![0u8; (width as usize) * (height as usize) * 3];
    unsafe {
        gl.read_buffer(glow::BACK);
        gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
        gl.read_pixels(
            0,
            0,
            width,
            height,
            glow::RGB,
            glow::UNSIGNED_BYTE,
            glow::PixelPackData::Slice(Some(&mut px)),
        );
    }
    // GL hands back rows bottom-first; PNG wants them top-first. `--shot` writes BMP, whose row
    // order is GL's already, which is why that path has no flip and this one does.
    let row = width as usize * 3;
    let mut flipped = Vec::with_capacity(px.len());
    for y in (0..height as usize).rev() {
        flipped.extend_from_slice(&px[y * row..(y + 1) * row]);
    }

    let file = std::fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    image::codecs::png::PngEncoder::new(std::io::BufWriter::new(file))
        .write_image(&flipped, width as u32, height as u32, image::ExtendedColorType::Rgb8)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// Where saved frames go: the user's Documents folder, in a `DayDreams` subfolder of its own.
///
/// A subfolder rather than Documents itself because a screenshot key is pressed dozens of times
/// in an afternoon, and the first thing a person does with a folder full of loose PNGs is stop
/// using the key. `directories` resolves the localised, redirected Documents path per OS -- the
/// same crate the settings file already goes through -- and the home directory stands in where
/// there is no Documents at all.
fn shots_dir() -> Result<PathBuf, String> {
    let dirs = directories::UserDirs::new().ok_or("no home directory")?;
    let base = dirs.document_dir().unwrap_or_else(|| dirs.home_dir());
    Ok(base.join("DayDreams"))
}

/// A scene name as a filename stem: lowercase, spaces to hyphens, nothing else kept.
fn slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut hyphen = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            hyphen = false;
        } else if !hyphen && !out.is_empty() {
            out.push('-');
            hyphen = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "scene".to_string()
    } else {
        out
    }
}

/// `<slug>-NNN.png`, with `NNN` one past the highest already in `dir`.
///
/// Highest rather than a count, so deleting a file from the middle never makes the next save
/// overwrite one that is still there.
fn next_name(dir: &Path, slug: &str) -> String {
    let mut max = 0u32;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for name in entries.flatten().map(|e| e.file_name()) {
            let Some(name) = name.to_str() else { continue };
            let Some(rest) = name.strip_prefix(slug) else { continue };
            let Some(rest) = rest.strip_prefix('-') else { continue };
            let Some(num) = rest.strip_suffix(".png") else { continue };
            if let Ok(n) = num.parse::<u32>() {
                max = max.max(n);
            }
        }
    }
    format!("{slug}-{:03}.png", max + 1)
}

/// A path with the home directory folded back to `~`, for the toast: the full path of a shot in
/// Documents is wider than the panel and the interesting half is the end of it.
fn pretty(p: &Path) -> String {
    let full = p.display().to_string();
    match directories::UserDirs::new() {
        Some(d) => match full.strip_prefix(&d.home_dir().display().to_string()) {
            Some(rest) => format!("~{rest}"),
            None => full,
        },
        None => full,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_filenames() {
        assert_eq!(slug("Backrooms"), "backrooms");
        assert_eq!(slug("Pool Rooms"), "pool-rooms");
        assert_eq!(slug("The Painted Cube"), "the-painted-cube");
        assert_eq!(slug("Three Rooms"), "three-rooms");
        // Punctuation collapses into the same single hyphen a space does, and never trails.
        assert_eq!(slug("Six  Rooms!"), "six-rooms");
        assert_eq!(slug("!!!"), "scene");
        assert_eq!(slug(""), "scene");
    }

    #[test]
    fn numbering_starts_at_one_in_an_empty_folder() {
        let dir = std::env::temp_dir().join("daydreams-debug-test-empty");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(next_name(&dir, "backrooms"), "backrooms-001.png");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// One past the HIGHEST, not one past the count: a gap left by a deleted file must not send
    /// the next save on top of one that is still there.
    #[test]
    fn numbering_goes_past_the_highest_not_the_count() {
        let dir = std::env::temp_dir().join("daydreams-debug-test-gap");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for n in ["backrooms-001.png", "backrooms-007.png", "pool-rooms-009.png"] {
            std::fs::write(dir.join(n), b"").unwrap();
        }
        assert_eq!(next_name(&dir, "backrooms"), "backrooms-008.png");
        // Another scene's numbering is its own.
        assert_eq!(next_name(&dir, "pool-rooms"), "pool-rooms-010.png");
        // And a name that only looks like one of ours is ignored.
        std::fs::write(dir.join("backrooms-final.png"), b"").unwrap();
        std::fs::write(dir.join("backrooms-012.bmp"), b"").unwrap();
        assert_eq!(next_name(&dir, "backrooms"), "backrooms-008.png");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_folder_numbers_from_one() {
        let dir = std::env::temp_dir().join("daydreams-debug-test-absent");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(next_name(&dir, "backrooms"), "backrooms-001.png");
    }

    /// The whole point of the overlay: the line it prints has to be pasteable. A negative angle
    /// needs the `=` form, because clap reads a bare leading `-` as the next flag.
    #[test]
    fn the_command_line_is_pasteable() {
        let info = Info {
            scene_ix: 16,
            scene_name: "Backrooms",
            pos: Vector3::new(999.4, 1.5, 0.6),
            yaw_deg: 90.0,
            pitch_deg: -36.0,
            p_scale: 1.0,
            fov_deg: 60.0,
            frame_ms: None,
            held: None,
        };
        assert_eq!(info.command(), "--scene 16 --pos 999.40,1.50,0.60 --yaw 90.0 --pitch=-36.0");

        let info = Info { yaw_deg: -100.4, pitch_deg: 2.0, ..info };
        assert_eq!(info.command(), "--scene 16 --pos 999.40,1.50,0.60 --yaw=-100.4 --pitch 2.0");
    }

    /// The yaw the player carries is unbounded; the line printed must not be.
    /// A scaled view has to carry its scale, or the line reproduces the wrong picture.
    #[test]
    fn a_scaled_view_prints_its_scale() {
        let base = Info {
            scene_ix: 5,
            scene_name: "Scaling Tunnel",
            pos: Vector3::new(1.35, 0.75, -3.5),
            yaw_deg: 85.7,
            pitch_deg: -6.5,
            p_scale: 0.5,
            fov_deg: 60.0,
            frame_ms: None,
            held: None,
        };
        assert_eq!(
            base.command(),
            "--scene 5 --pos 1.35,0.75,-3.50 --yaw 85.7 --pitch=-6.5 --p-scale 0.500"
        );
        // At the ordinary scale the flag is left off, so the usual line stays short.
        let plain = Info { p_scale: 1.0, ..base };
        assert!(!plain.command().contains("--p-scale"), "{}", plain.command());
        // And a rounding wobble around 1 is still "ordinary".
        let wobble = Info { p_scale: 1.0002, ..base };
        assert!(!wobble.command().contains("--p-scale"), "{}", wobble.command());
    }

    #[test]
    fn yaw_folds_into_one_turn() {
        assert_eq!(wrap_deg(90.0), 90.0);
        assert_eq!(wrap_deg(-90.0), -90.0);
        assert_eq!(wrap_deg(180.0), 180.0);
        assert_eq!(wrap_deg(-180.0), 180.0, "the two ends of the turn agree on one name");
        assert!((wrap_deg(450.0) - 90.0).abs() < 1e-3);
        assert!((wrap_deg(-450.0) + 90.0).abs() < 1e-3);
        assert!((wrap_deg(3787.4) - 187.4 + 360.0).abs() < 1e-2, "{}", wrap_deg(3787.4));

        let info = Info {
            scene_ix: 0,
            scene_name: "Tunnels",
            pos: Vector3::new(0.0, 1.5, 0.0),
            yaw_deg: 450.0,
            pitch_deg: 0.0,
            p_scale: 1.0,
            fov_deg: 60.0,
            frame_ms: None,
            held: None,
        };
        assert!(info.command().contains("--yaw 90.0"), "{}", info.command());
    }

    #[test]
    fn the_toast_ages_out() {
        let mut dbg = DebugMode::default();
        assert!(dbg.toast().is_none(), "nothing saved yet");
        dbg.report(Ok(PathBuf::from("/tmp/x.png")));
        assert!(dbg.toast().is_some());
        dbg.report(Err("no room".to_string()));
        let (line, _) = dbg.toast().unwrap();
        assert!(line.contains("FAILED"), "{line}");
    }

    #[test]
    fn the_switch_starts_off() {
        let mut dbg = DebugMode::default();
        assert!(!dbg.on);
        dbg.toggle();
        assert!(dbg.on);
        dbg.toggle();
        assert!(!dbg.on);
    }
}
