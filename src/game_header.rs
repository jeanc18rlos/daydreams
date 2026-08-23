//! Port of GameHeader.h -- global compile-time configuration.
//!
//! PORT: the four `extern` globals GH_ENGINE / GH_PLAYER / GH_INPUT / GH_REC_LEVEL /
//! GH_FRAME (GameHeader.h:48-52) are NOT ported as statics. GH_ENGINE and GH_INPUT are
//! threaded through as &RenderCtx / &UpdateCtx parameters, GH_PLAYER lives in Engine,
//! and GH_REC_LEVEL / GH_FRAME become Cell fields on Engine.
//!
//! PORT: the two `#pragma warning(disable: ...)` lines (GameHeader.h:3-4) are MSVC-only
//! and have no Rust equivalent.

//Windows
// EXT: the game's name. The C++ ships as a tech demo called "NonEuclideanDemo"; this is a game
// called DayDreams, and this string is what the OS window bar and the dock/taskbar show. The
// title SCREEN draws `ext::menu::TITLE_TEXT` instead -- same name, set separately because it is
// typeset rather than labelled.
pub const GH_TITLE: &str = "DayDreams";
// PORT: GH_CLASS is the Win32 window-class name registered by RegisterClassEx; winit owns
// the window class, so nothing consumes it (was: static const char GH_CLASS[] = "NED", GameHeader.h:8).
#[allow(dead_code)]
pub const GH_CLASS: &str = "NED";

//General
// PORT: GameHeader.h's literal `3.141592653589793` rounds to the f32 nearest pi, which is
// what `f32::consts::PI` is, bit for bit (the test below pins it); the name stays because the
// ported files read it (was: static const float GH_PI = 3.141592653589793f, GameHeader.h:11).
pub const GH_PI: f32 = std::f32::consts::PI;
// PORT: `const int` -> usize, because it is only ever used to size/limit a Vec of portals
// (was: static const int GH_MAX_PORTALS = 16, GameHeader.h:12).
pub const GH_MAX_PORTALS: usize = 16;

//Graphics
// EXT: true, where the C++ ships false (GameHeader.h:16). A game opens fullscreen; the demo it
// was ported from opened in a 1280x720 window beside your editor. Alt+Enter and the pad's PS
// button still toggle back, and the windowed size below is what they restore to.
pub const GH_START_FULLSCREEN: bool = true;
pub const GH_HIDE_MOUSE: bool = true;
pub const GH_USE_SKY: bool = true;
// PORT: `const int` -> u32, to match winit's PhysicalSize<u32>
// (was: static const int GH_SCREEN_WIDTH = 1280, GameHeader.h:18).
pub const GH_SCREEN_WIDTH: u32 = 1280;
// PORT: `const int` -> u32, see GH_SCREEN_WIDTH (was: static const int GH_SCREEN_HEIGHT = 720, GameHeader.h:19).
pub const GH_SCREEN_HEIGHT: u32 = 720;
pub const GH_SCREEN_X: i32 = 50;
pub const GH_SCREEN_Y: i32 = 50;
pub const GH_FOV: f32 = 60.0;
pub const GH_NEAR_MIN: f32 = 1e-3;
pub const GH_NEAR_MAX: f32 = 1e-1;
pub const GH_FAR: f32 = 100.0;
// EXT: was 2048, the fixed square size of every portal framebuffer (GameHeader.h:30). The
// portal framebuffers are now sized to the drawable (`Engine::portal_fbos`), so a door is
// sampled pixel for pixel at any window size; this is the cap on either side, which a 6K
// display would reach.
pub const GH_FBO_SIZE: i32 = 4096;
pub const GH_MAX_RECURSION: i32 = 4;

//Gameplay
pub const GH_MOUSE_SENSITIVITY: f32 = 0.005;
pub const GH_MOUSE_SMOOTH: f32 = 0.5;
pub const GH_WALK_SPEED: f32 = 2.9;
pub const GH_WALK_ACCEL: f32 = 50.0;
pub const GH_BOB_FREQ: f32 = 8.0;
pub const GH_BOB_OFFS: f32 = 0.015;
pub const GH_BOB_DAMP: f32 = 0.04;
pub const GH_BOB_MIN: f32 = 0.1;
pub const GH_DT: f32 = 0.002;
pub const GH_MAX_STEPS: i32 = 30;
pub const GH_PLAYER_HEIGHT: f32 = 1.5;
pub const GH_PLAYER_RADIUS: f32 = 0.2;
pub const GH_GRAVITY: f32 = -9.8;

//Functions
// PORT: `template<class T>` with C++'s implicit `operator<` requirement becomes an
// explicit `T: PartialOrd` bound (was: template<class T> inline T GH_CLAMP(T a, T mn, T mx), GameHeader.h:55-58).
#[inline]
pub fn gh_clamp<T: PartialOrd>(a: T, mn: T, mx: T) -> T {
    if a < mn {
        mn
    } else if a > mx {
        mx
    } else {
        a
    }
}
#[inline]
pub fn gh_min<T: PartialOrd>(a: T, b: T) -> T {
    if a < b {
        a
    } else {
        b
    }
}
// Preserved for fidelity with GameHeader.h; only GH_Min/GH_Clamp are used by the scenes.
#[allow(dead_code)]
#[inline]
pub fn gh_max<T: PartialOrd>(a: T, b: T) -> T {
    if a > b {
        a
    } else {
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The original's `3.141592653589793f` and `f32::consts::PI` are the same 32 bits: the
    /// nearest float to pi, which the literal (pi to sixteen digits) rounds to. Computed
    /// rather than written out, because the literal itself is what clippy objects to.
    #[test]
    fn gh_pi_is_the_originals_literal() {
        let from_literal = (4.0f64 * 1.0f64.atan()) as f32;
        assert_eq!(GH_PI.to_bits(), from_literal.to_bits());
        assert_eq!(GH_PI.to_bits(), 0x4049_0fdb);
    }

    #[test]
    fn clamp_min_and_max() {
        assert_eq!(gh_clamp(5, 0, 3), 3);
        assert_eq!(gh_clamp(-1, 0, 3), 0);
        assert_eq!(gh_clamp(2, 0, 3), 2);
        assert_eq!(gh_min(1.5, 2.5), 1.5);
        assert_eq!(gh_max(1.5, 2.5), 2.5);
    }
}
