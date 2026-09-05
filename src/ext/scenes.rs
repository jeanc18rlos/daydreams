//! EXT: the scene registry -- one table of name and constructor, in the order the game
//! presents them.
//!
//! The C++ registers its seven scenes as seven `push_back`s (Engine.cpp:41-47) and selects
//! them with seven `if` branches on keys '1'..'7' (Engine.cpp:90-104). The port grew that to
//! seventeen as three parallel literals -- the `v_scenes` vector, a `SCENE_KEYS` array and a
//! `NAMES` array -- which agreed with each other only by care. This is the one place a scene
//! is declared; `Engine` builds its vector from it and the level-select menu reads its names
//! from it (the number row picks inventory slots now -- there is no key loop).
//!
//! To add a scene: append a `SceneEntry` here. Nothing else is needed -- SWITCH LEVEL in the
//! pause menu lists whatever is in this table.

use std::rc::Rc;

use crate::level1::Level1;
use crate::level2::Level2;
use crate::level3::Level3;
use crate::level4::Level4;
use crate::level5::Level5;
use crate::level6::Level6;
use crate::scene::Scene;

pub struct SceneEntry {
    /// What the level-select menu shows.
    pub name: &'static str,
    pub make: fn() -> Rc<dyn Scene>,
}

/// Every scene: CodeParade's seven first, in their registration order (Engine.cpp:41-47),
/// then the extensions in the order they were built.
pub const SCENES: &[SceneEntry] = &[
    SceneEntry { name: "Tunnels", make: || Rc::new(Level1) },
    SceneEntry { name: "Three Rooms", make: || Rc::new(Level2::new(3)) },
    SceneEntry { name: "Six Rooms", make: || Rc::new(Level2::new(6)) },
    SceneEntry { name: "Pillar Rooms", make: || Rc::new(Level3) },
    SceneEntry { name: "Sloped Tunnel", make: || Rc::new(Level4) },
    SceneEntry { name: "Scaling Tunnel", make: || Rc::new(Level5) },
    SceneEntry { name: "Floorplan", make: || Rc::new(Level6) },
    SceneEntry { name: "Perspective Gallery", make: || Rc::new(crate::level7::Level7) },
    SceneEntry { name: "Penrose Ascent", make: || Rc::new(crate::level8::Level8) },
    SceneEntry { name: "Compound", make: || Rc::new(crate::level9::Level9) },
    SceneEntry { name: "Unobserved", make: || Rc::new(crate::level10::Level10) },
    SceneEntry { name: "Anamorphic Chamber", make: || Rc::new(crate::level11::Level11) },
    SceneEntry { name: "The Painted Cube", make: || Rc::new(crate::level12::Level12) },
    SceneEntry { name: "Relativity", make: || Rc::new(crate::level13::Level13) },
    SceneEntry { name: "Meadow", make: || Rc::new(crate::level14::Level14) },
    SceneEntry { name: "Intro", make: || Rc::new(crate::level15::Level15) },
    SceneEntry { name: "Backrooms", make: || Rc::new(crate::level16::Level16) },
    SceneEntry { name: "Pool Rooms", make: || Rc::new(crate::level17::Level17) },
    SceneEntry { name: "Overgrown", make: || Rc::new(crate::level18::Level18) },
    // EXT-pivot: Hide 'N Dream arenas start here (docs/hide-n-dream.md).
    SceneEntry { name: "The Ring", make: || Rc::new(crate::level30::Level30) },
    SceneEntry { name: "Liminal Neighborhood", make: || Rc::new(crate::level31::Level31) },
    SceneEntry { name: "Open House", make: || Rc::new(crate::level32::Level32) },
];

/// The registry index of the scene called `name`, or `None` if no scene is. A `const fn`, so
/// [`INTRO`] can be resolved by name at compile time; at runtime it is what the elevator
/// (`ext/elevator.rs`) turns its floor names into, which is how a floor whose scene is not
/// registered yet is simply skipped rather than mis-indexed.
pub const fn index_of(name: &str) -> Option<usize> {
    let mut i = 0;
    while i < SCENES.len() {
        if str_eq(SCENES[i].name, name) {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// `a == b` for a `const fn`: `str::eq` is not const, so the bytes are walked by hand.
const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// Index of the scene where NEW GAME begins: the Backrooms. Looked up by name rather than
/// written as a number so that reordering the table cannot quietly start the game somewhere
/// else.
pub const INTRO: usize = match index_of("Backrooms") {
    Some(i) => i,
    None => panic!("the scene registry has no Backrooms scene for NEW GAME to start in"),
};

/// Index of the scene the title screen runs live behind itself: the Intro -- the same meadow
/// and the same white door as the Backrooms, but opening on the sunset sea rather than on an
/// office.
///
/// A separate constant from [`INTRO`], because the two are separate jobs. NEW GAME opens where
/// the game is played; the title screen is a *photograph*, and what it wants behind the words
/// is the shot the game is named for -- a white door standing in a grey meadow with a sunset
/// coming through it. Both scenes are built on `ext::meadow`, so the door, the knoll and the
/// grass are the same objects either way; only what lies past the portal differs, which is why
/// swapping the backdrop costs nothing but this index. `Engine::new` wants it before anything
/// else exists, which the `const` lookup allows.
pub const TITLE: usize = match index_of("Intro") {
    Some(i) => i,
    None => panic!("the scene registry has no Intro scene for the title screen to stand in"),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twenty_two_scenes() {
        assert_eq!(SCENES.len(), 22);
    }

    /// NEW GAME opens in the Backrooms: on the meadow, facing the door into the hall.
    #[test]
    fn intro_is_the_backrooms() {
        assert_eq!(SCENES[INTRO].name, "Backrooms");
    }

    /// The title screen stands in the Intro instead -- the same meadow, with the sunset sea
    /// through the door rather than the office. Two indices, because they are two jobs.
    #[test]
    fn the_title_stands_in_the_sunset_scene() {
        assert_eq!(SCENES[TITLE].name, "Intro");
        assert_ne!(TITLE, INTRO, "the backdrop and the first level are chosen separately");
    }

    #[test]
    fn index_of_finds_every_name_and_nothing_else() {
        for (i, entry) in SCENES.iter().enumerate() {
            assert_eq!(index_of(entry.name), Some(i), "{:?}", entry.name);
        }
        assert_eq!(index_of("Intro"), Some(15));
        assert_eq!(index_of(""), None);
        assert_eq!(index_of("Backroom"), None, "a prefix is not a match");
        assert_eq!(index_of("Backrooms "), None, "nor is a longer string");
    }

    #[test]
    fn names_unique_and_present() {
        let mut names: Vec<&str> = SCENES.iter().map(|e| e.name).collect();
        assert!(names.iter().all(|n| !n.is_empty()));
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), SCENES.len(), "two scenes share a name");
    }

    #[test]
    fn every_constructor_builds() {
        for entry in SCENES {
            let _scene = (entry.make)();
        }
    }
}
