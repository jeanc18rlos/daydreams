//! EXT: the scene registry -- one table of key, name and constructor, in key order.
//!
//! The C++ registers its seven scenes as seven `push_back`s (Engine.cpp:41-47) and selects
//! them with seven `if` branches on keys '1'..'7' (Engine.cpp:90-104). The port grew that to
//! seventeen as three parallel literals -- the `v_scenes` vector, a `SCENE_KEYS` array and a
//! `NAMES` array -- which agreed with each other only by care. This is the one place a scene
//! is declared; `Engine` builds its vector from it, the level-select menu reads its names from
//! it, and the key loop walks it.
//!
//! To add a scene: append a `SceneEntry` here and make sure `input::key_index` maps its key
//! (the `every_key_is_typeable` test below fails until it does).

use std::rc::Rc;

use crate::level1::Level1;
use crate::level2::Level2;
use crate::level3::Level3;
use crate::level4::Level4;
use crate::level5::Level5;
use crate::level6::Level6;
use crate::scene::Scene;

pub struct SceneEntry {
    /// The key slot that loads it (see `input::key_index`).
    pub key: u8,
    /// What the level-select menu shows.
    pub name: &'static str,
    pub make: fn() -> Rc<dyn Scene>,
}

/// Every scene, in key order: CodeParade's seven first, in their registration order
/// (Engine.cpp:41-47), then the extensions along the top row of the keyboard.
pub const SCENES: &[SceneEntry] = &[
    SceneEntry { key: b'1', name: "Tunnels", make: || Rc::new(Level1) },
    SceneEntry { key: b'2', name: "Three Rooms", make: || Rc::new(Level2::new(3)) },
    SceneEntry { key: b'3', name: "Six Rooms", make: || Rc::new(Level2::new(6)) },
    SceneEntry { key: b'4', name: "Pillar Rooms", make: || Rc::new(Level3) },
    SceneEntry { key: b'5', name: "Sloped Tunnel", make: || Rc::new(Level4) },
    SceneEntry { key: b'6', name: "Scaling Tunnel", make: || Rc::new(Level5) },
    SceneEntry { key: b'7', name: "Floorplan", make: || Rc::new(Level6) },
    SceneEntry { key: b'8', name: "Perspective Gallery", make: || Rc::new(crate::level7::Level7) },
    SceneEntry { key: b'9', name: "Penrose Ascent", make: || Rc::new(crate::level8::Level8) },
    SceneEntry { key: b'0', name: "Compound", make: || Rc::new(crate::level9::Level9) },
    SceneEntry { key: b'-', name: "Unobserved", make: || Rc::new(crate::level10::Level10) },
    SceneEntry { key: b'=', name: "Anamorphic Chamber", make: || Rc::new(crate::level11::Level11) },
    SceneEntry { key: b'[', name: "The Painted Cube", make: || Rc::new(crate::level12::Level12) },
    SceneEntry { key: b']', name: "Relativity", make: || Rc::new(crate::level13::Level13) },
    SceneEntry { key: b'\\', name: "Meadow", make: || Rc::new(crate::level14::Level14) },
    SceneEntry { key: b';', name: "Intro", make: || Rc::new(crate::level15::Level15) },
    SceneEntry { key: b'\'', name: "Backrooms", make: || Rc::new(crate::level16::Level16) },
];

/// Index of the intro scene: where NEW GAME begins and what the title screen shows behind
/// itself. A constant rather than a lookup because `Engine::new` wants it before anything
/// else exists; `intro_is_the_intro` pins it to the name.
pub const INTRO: usize = 15;

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::KeyCode;

    #[test]
    fn seventeen_scenes() {
        assert_eq!(SCENES.len(), 17);
    }

    #[test]
    fn intro_is_the_intro() {
        assert_eq!(SCENES[INTRO].name, "Intro");
    }

    #[test]
    fn keys_unique() {
        let mut keys: Vec<u8> = SCENES.iter().map(|e| e.key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), SCENES.len(), "two scenes share a key");
    }

    #[test]
    fn names_unique_and_present() {
        let mut names: Vec<&str> = SCENES.iter().map(|e| e.name).collect();
        assert!(names.iter().all(|n| !n.is_empty()));
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), SCENES.len(), "two scenes share a name");
    }

    /// A scene whose key no physical key reaches is unselectable from the keyboard. The
    /// candidates are every code on the top row of a US layout plus the letters, which is
    /// more than the registry uses and is where any new scene key would come from.
    #[test]
    fn every_key_is_typeable() {
        #[rustfmt::skip]
        let candidates = [
            KeyCode::Digit0, KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4,
            KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8, KeyCode::Digit9,
            KeyCode::Minus, KeyCode::Equal, KeyCode::BracketLeft, KeyCode::BracketRight,
            KeyCode::Backslash, KeyCode::Semicolon, KeyCode::Quote, KeyCode::Comma,
            KeyCode::Period, KeyCode::Slash, KeyCode::Backquote,
            KeyCode::KeyA, KeyCode::KeyB, KeyCode::KeyC, KeyCode::KeyD, KeyCode::KeyE,
            KeyCode::KeyF, KeyCode::KeyG, KeyCode::KeyH, KeyCode::KeyI, KeyCode::KeyJ,
            KeyCode::KeyK, KeyCode::KeyL, KeyCode::KeyM, KeyCode::KeyN, KeyCode::KeyO,
            KeyCode::KeyP, KeyCode::KeyQ, KeyCode::KeyR, KeyCode::KeyS, KeyCode::KeyT,
            KeyCode::KeyU, KeyCode::KeyV, KeyCode::KeyW, KeyCode::KeyX, KeyCode::KeyY,
            KeyCode::KeyZ,
        ];
        let reachable: Vec<usize> =
            candidates.iter().filter_map(|&k| crate::input::key_index(k)).collect();
        for entry in SCENES {
            assert!(
                reachable.contains(&(entry.key as usize)),
                "scene {:?} is on key {:?}, which input::key_index does not map",
                entry.name,
                entry.key as char
            );
        }
    }

    #[test]
    fn every_constructor_builds() {
        for entry in SCENES {
            let _scene = (entry.make)();
        }
    }
}
