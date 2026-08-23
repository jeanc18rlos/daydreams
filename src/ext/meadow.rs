//! EXT: the intro's meadow -- the half of a level that the door stands in. Not part of the
//! C++ port.
//!
//! Two scenes open on the same stormy meadow with the same white door and differ only in what
//! the door opens onto: the intro (`level15`) on a sunset sea, the Backrooms (`level16`) on a
//! scanned office. Everything on the near side of the door is one place, built here once:
//! the flat-torus terrain and its wrap, the blade field, the door with its portal and the
//! near half of the `DoorLink`, the spawn in front of it -- and the vantage the title screen
//! watches it from. A level calls [`load_meadow`], builds its far world around [`FAR`], and
//! hangs the far door on the link it gets back.
//!
//! # How two weathers share one sky
//!
//! The far world lives 1000 units away along +x, past `view::MOOD_SPLIT_X`. Every material
//! receives a `mood` uniform chosen by where the *camera of the current render pass* is, so
//! the portal pass that draws the view through the door grades its sky and ground as sunset
//! while the main pass grades the meadow as storm. The clouds are the same baked panorama both
//! times; only the colour grade differs. Cost: nothing.
//!
//! # The light pool
//!
//! While the door is open it publishes a warm glow position (`ext/door.rs`), and the grass
//! shader spills that onto the ground in front of the opening -- the reference's lit patch of
//! grass under a dark sky.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ext::door::{yaw_facing, Door, DoorLink};
use crate::ext::grassfield::GrassField;
use crate::ext::terrain;
use crate::ext::view;
use crate::game_header::GH_PLAYER_HEIGHT;
use crate::object::ObjectT;
use crate::player::Player;
use crate::portal::Portal;
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec};
use crate::vector::Vector3;

/// The far world's origin: where its return door stands, and what it is built around. Safely
/// past the mood split, and far enough that the meadow's 3x3 tile lattice (which reaches
/// +/-384) cannot reach a 400x400 sea quad or surface through a carpet. The same point for
/// every far world, so they are interchangeable from the meadow side.
pub const FAR: Vector3 = Vector3 { x: 1000.0, y: 0.0, z: 0.0 };

/// Where the meadow door stands; the player spawns a few strides in front of it.
pub const DOOR_POS: Vector3 =
    Vector3 { x: terrain::DOOR_X, y: terrain::DOOR_Y, z: terrain::DOOR_Z };

/// The meadow door faces the player, on its +z side.
const DOOR_FACING: Vector3 = Vector3 { x: 0.0, y: 0.0, z: 1.0 };

/// What a level gets back from [`load_meadow`]: the handles its far world needs.
pub struct MeadowSide {
    /// The far half of the door's link. The far door is built with it, so the two leaves
    /// open and close together.
    pub link_there: DoorLink,
    /// The meadow door's portal, for `connect(&here, &there)` once the far door exists.
    pub here: Rc<RefCell<Portal>>,
}

/// The vantage the title screen watches the meadow from, as (eye, yaw, pitch).
///
/// The menu draws the intro level live behind it (engine.rs `render_menu_frame`), so unlike
/// the spawn -- which is a place to stand -- this is a shot to be composed. Two things decide
/// it:
///
/// * The door sits **off centre**, because the menu's own column of options is centred and a
///   door dead ahead would end up behind the text with its sunset showing through the letters.
///   Standing to the door's right and turning back toward it puts the opening in the left third
///   and leaves the middle of the frame as plain meadow for the words to sit on.
/// * The eye is **further back than the spawn** (about nine strides out rather than four), for
///   the meadow and the storm above it, which the spawn's close-up crops away. That is past the
///   door's own opening radius, which is why the title holds doors open explicitly
///   (`ext::door::set_hold_open`) instead of relying on the camera to trigger this one.
///
/// Eye height is standing height above the knoll rather than anything cinematic: the horizon it
/// puts behind the door is the one the player will see a second later when the game starts.
pub fn title_view() -> (Vector3, f32, f32) {
    /// Strides back from the door's face and to its right. The pair sets both how big the door
    /// reads and how obliquely the opening is seen -- keep SIDE well under BACK or the sunset
    /// through it foreshortens to a slot.
    const BACK: f32 = 6.6;
    const SIDE: f32 = 3.0;
    /// How far right of the door the shot then aims, in radians. Screen position, not distance:
    /// an angular offset holds the door in the left third however close the camera stands.
    const OFF_CENTRE: f32 = 0.38;
    /// A few degrees down, so the meadow carries the option list rather than the sky.
    const PITCH: f32 = -0.05;

    let eye = Vector3::new(
        DOOR_POS.x + SIDE,
        terrain::DOOR_Y + GH_PLAYER_HEIGHT,
        DOOR_POS.z + BACK,
    );
    // Aim at the door, then turn further right so it falls out of the centred option list.
    // The yaw convention is the engine's own: `Physical::try_portal` re-aims a warped object
    // with exactly this atan2, and `Player::look` decreases the angle when the view turns right.
    let to_door = DOOR_POS - eye;
    let yaw = -to_door.x.atan2(-to_door.z) - OFF_CENTRE;
    (eye, yaw, PITCH)
}

/// A door plus the portal that fills it.
#[allow(clippy::too_many_arguments)] // a scene-construction helper; every argument is a placement
pub fn door_with_portal(
    gl: &Rc<glow::Context>,
    res: &Resources,
    pos: Vector3,
    facing: Vector3,
    glows: bool,
    link: DoorLink,
    objs: &mut PObjectVec,
    portals: &mut PPortalVec,
) -> Rc<RefCell<Portal>> {
    let door = Door::new(gl, res, pos, yaw_facing(facing), glows, Some(link));
    let (centre, euler, scale) = door.portal_transform();
    objs.push(Rc::new(RefCell::new(door)) as Rc<RefCell<dyn ObjectT>>);

    let portal = Rc::new(RefCell::new(Portal::new(res)));
    {
        let mut p = portal.borrow_mut();
        p.base.pos = centre;
        p.base.euler = euler;
        p.base.scale = scale;
    }
    portals.push(portal.clone());
    portal
}

/// Build the meadow, its door and the spawn in front of it, and turn the weather split on.
/// The level then builds its far world around [`FAR`] and connects its door to `here`.
pub fn load_meadow(
    gl: &Rc<glow::Context>,
    res: &Resources,
    objs: &mut PObjectVec,
    portals: &mut PPortalVec,
    player: &mut Player,
) -> MeadowSide {
    view::set_mood_enabled(true);

    // ── A flat torus of rolling grass ─────────────────────────────────────────────────────
    // Declaring the period is what turns the wrap on: Engine::update wraps the player into
    // the fundamental square each step, and every material quantises its world-space noise
    // to whole cycles per period so that wrap cannot be seen. See src/ext/terrain.rs.
    view::set_wrap(terrain::PERIOD);
    // One object drawing a 3x3 lattice of one tile -- and carrying the single set of
    // colliders the player ever walks on. NOT the flat `ground` quad any more: at y=0 it
    // would z-fight the terrain along every valley floor, and its own 800x800 collider plate
    // would fight the terrain's.
    objs.push(Rc::new(RefCell::new(terrain::Terrain::new(res))) as Rc<RefCell<dyn ObjectT>>);
    // Real blades in a patch that follows the player; the ground texture covers the rest.
    // The blades read the same height field in their vertex shader, so they stand on the
    // hills rather than hovering in a flat sheet.
    objs.push(Rc::new(RefCell::new(GrassField::new(gl, res, 0.0))) as Rc<RefCell<dyn ObjectT>>);
    // No bounds box on this side: walls at +/-120 would stop the player 8 units short of the
    // wrap line and the torus would never close.

    // One door, two frames: the link keeps both leaves in step.
    let (link_here, link_there) = DoorLink::pair();

    // The door faces the player, and is the one that lights the grass.
    let here = door_with_portal(gl, res, DOOR_POS, DOOR_FACING, true, link_here, objs, portals);

    // Four strides from the door: inside its opening radius, so the game opens on the door
    // swinging open.
    player
        .base
        .set_position(Vector3::new(0.0, terrain::DOOR_Y + GH_PLAYER_HEIGHT, -2.0));

    MeadowSide { link_there, here }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole weather trick depends on the far world sitting past the split.
    #[test]
    fn far_world_is_past_the_mood_split() {
        assert!(FAR.x > view::MOOD_SPLIT_X + 100.0);
        assert!(DOOR_POS.x < view::MOOD_SPLIT_X);
    }

    /// The title's vantage is composed by hand against constants that live elsewhere, and
    /// nothing at runtime would notice if the door moved out from under it: the menu would
    /// simply draw itself over an empty meadow. So the framing is asserted here.
    #[test]
    fn title_view_frames_the_door() {
        let (eye, yaw, _) = title_view();

        // On the door's +z face -- the side its leaf swings clear of and its portal shows the
        // far world through. Behind it there would be nothing to see but a blank back.
        assert!(eye.z > DOOR_POS.z, "title camera is behind the door");

        // Far enough out that proximity alone would let the leaf shut, which is the whole
        // reason the title screen holds doors open instead of trusting the camera to do it.
        let mut flat = DOOR_POS - eye;
        flat.y = 0.0;
        assert!(
            flat.mag() > crate::ext::door::CLOSE_DIST,
            "vantage is {} out, inside the door's own opening radius",
            flat.mag()
        );

        // Off the view axis on purpose -- the menu's option list owns the middle of the frame.
        // Measured in the horizontal plane, because that is the direction being composed for;
        // the pitch is a couple of degrees and says nothing about where the door lands.
        let forward = Vector3::new(-yaw.sin(), 0.0, -yaw.cos());
        let off = forward.dot(flat.normalized()).acos();
        assert!(off > 0.20, "door is near centred, and the option list will sit on it");
        // GH_FOV is the VERTICAL 60 degrees; at any widescreen aspect the horizontal half-angle
        // is past 0.75 rad, so this leaves the door a wide margin from the edge of the frame.
        assert!(off < 0.50, "door is {off} rad off axis and drifting out of frame");
    }

    /// mood_for must answer per region when the scene enables it, and "daylight" otherwise.
    #[test]
    fn mood_regions() {
        view::set_mood_enabled(true);
        assert_eq!(view::mood_for(Vector3::new(0.0, 1.0, 0.0)), 0.0);
        assert_eq!(view::mood_for(FAR), 1.0);
        view::set_mood_enabled(false);
        assert_eq!(view::mood_for(FAR), -1.0);
    }
}
