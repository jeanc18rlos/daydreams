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

/// Whether a player at `pos` is in the far world -- has been through the door. The warp is
/// instantaneous and the two worlds are a thousand units apart, so "past the mood split" is
/// the same fact as "crossed": nothing walks there. The title's vantage and the spawn are
/// both on the meadow side (`a_crossing_is_being_past_the_split` checks).
pub fn in_far_world(pos: Vector3) -> bool {
    pos.x > view::MOOD_SPLIT_X
}

/// What a level gets back from [`load_meadow`]: the handles its far world needs.
pub struct MeadowSide {
    /// The far half of the door's link. The far door is built with it, so the two leaves
    /// open and close together.
    pub link_there: DoorLink,
    /// The meadow door's portal, for `connect(&here, &there)` once the far door exists.
    pub here: Rc<RefCell<Portal>>,
}

/// EXT: the field of view the title screen is shot at, in degrees, against `GH_FOV`'s 60 for
/// play.
///
/// A menu backdrop is a photograph, and 60 degrees vertical is a first-person *playing* FOV:
/// wide, so that nothing creeps up on you, and with the perspective exaggeration that comes
/// with it -- a door six metres away is a small rectangle in the middle of a lot of meadow.
/// Pulling in to 36 does two things at once. The door reads twice as large without the
/// camera having to walk into it, and the compression flattens the hills behind it into bands,
/// which is what a real long lens does to a landscape at dusk and most of why the reference
/// image reads as a photograph rather than as a screenshot.
///
/// `Engine::step_title_backdrop` sets it every title frame and `load_scene` restores `GH_FOV`
/// (`ExtState::on_scene_loaded` -> `view::reset_fov`), so NEW GAME opens at the playing FOV
/// and nothing has to remember to put it back.
pub const TITLE_FOV: f32 = 36.0;

/// The vantage the title screen watches the meadow from, as (eye, yaw, pitch).
///
/// The menu draws the intro level live behind it (engine.rs `render_menu_frame`), so unlike
/// the spawn -- which is a place to stand -- this is a shot to be composed. Three things
/// decide it:
///
/// * The door sits **off centre, to the right**, because the menu's title and its column of
///   options are set against the left margin (`ext/menu.rs`) and a door in the middle would
///   end up behind the text with its sunset showing through the letters. Which side of the
///   frame the door lands on is set by the yaw offset alone, not by where the camera stands --
///   turning left carries it right, from either side.
/// * Which side the camera **stands** on is a separate decision, and it is the leaf that makes
///   it. The hinge is the opening's local -x edge and the leaf swings out past 90 degrees, so
///   at any open angle it ends up on the far side of the hinge from the opening. Stand on that
///   same side and the leaf is between you and the sunset, and the door reads as a slab with a
///   sliver of light beside it; stand on the OTHER side, as here, and the leaf swings clear --
///   the opening is unobstructed and the leaf's panelled face is seen at forty degrees, lit.
/// * The eye stands **four units out**, which with the lens below puts the door at nearly two
///   thirds of the frame's height -- the subject of the picture rather than a detail in it.
///   That is inside the door's own opening radius, so the leaf would swing for this camera
///   anyway; the title still holds doors open explicitly (`ext::door::set_hold_open`) rather
///   than depending on that, because the vantage is composed for the shot and not for the
///   trigger.
/// * The lens is [`TITLE_FOV`], not the playing one, which is what lets the first two hold at
///   a comfortable distance instead of forcing the camera onto the doorstep.
///
/// Eye height is NOT standing height -- see `EYE_H`.
pub fn title_view() -> (Vector3, f32, f32) {
    /// Strides back from the door's face and to its RIGHT (its local +x, the side the leaf
    /// swings away from). The pair sets both how big the door reads and how obliquely the
    /// opening is seen -- keep SIDE well under BACK or the sunset through it foreshortens to
    /// a slot.
    const BACK: f32 = 4.4;
    const SIDE: f32 = 1.35;
    /// How far LEFT of the door the shot then aims, in radians. Screen position, not distance:
    /// an angular offset holds the door in the right third however close the camera stands.
    /// Larger than it looks, because [`TITLE_FOV`] is narrow -- the same offset pushes a door
    /// further across a 42 degree frame than across a 60 degree one.
    const OFF_CENTRE: f32 = 0.245;
    /// Eight degrees down. It is what puts the skyline at two fifths of the frame's height,
    /// leaving the lower three fifths to the meadow the option column stands on.
    const PITCH: f32 = -0.075;
    /// Eye height above the knoll -- LOW, at under two thirds of standing height
    /// (`GH_PLAYER_HEIGHT` is 1.5, which is nearly the top of a door only 1.7 tall).
    ///
    /// It decides what the doorway shows, and that is the whole picture. From a standing eye
    /// you look DOWN through the opening and see water; the sea's horizon and the sun sitting
    /// on it come into the gap only once the eye is well below the lintel. It also brings the
    /// near grass up toward the lens, which is where the foreground comes from -- but not so
    /// far that the blades swallow the frame, which is what 0.8 did.
    const EYE_H: f32 = 1.05;

    let eye = Vector3::new(DOOR_POS.x + SIDE, terrain::DOOR_Y + EYE_H, DOOR_POS.z + BACK);
    // Aim at the door, then turn further left so it falls clear of the option column.
    // The yaw convention is the engine's own: `Physical::try_portal` re-aims a warped object
    // with exactly this atan2, and `Player::look` decreases the angle when the view turns right
    // -- so ADDING the offset turns left and carries the door to the right of frame.
    let to_door = DOOR_POS - eye;
    let yaw = -to_door.x.atan2(-to_door.z) + OFF_CENTRE;
    (eye, yaw, PITCH)
}

/// A door plus the portal that fills it.
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
    player.base.set_position(Vector3::new(0.0, terrain::DOOR_Y + GH_PLAYER_HEIGHT, -2.0));

    MeadowSide { link_there, here }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole weather trick depends on the far world sitting past the split.
    #[test]
    fn far_world_is_past_the_mood_split() {
        const { assert!(FAR.x > view::MOOD_SPLIT_X + 100.0) }
        const { assert!(DOOR_POS.x < view::MOOD_SPLIT_X) }
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

        // Close enough that the door is the subject and far enough that the frame still holds
        // meadow either side of it. The lower bound is the door's own collider: the camera is
        // parked, not simulated, so nothing would push it out of a doorpost it was standing in.
        let mut flat = DOOR_POS - eye;
        flat.y = 0.0;
        assert!(flat.mag() > 1.5, "vantage is {} out, inside the door frame", flat.mag());
        assert!(flat.mag() < 5.0, "vantage is {} out and the door is a detail", flat.mag());

        // Off the view axis on purpose, and to the RIGHT -- the menu's title and option column
        // own the left of the frame. Measured in the horizontal plane, because that is the
        // direction being composed for; the pitch is a couple of degrees and says nothing about
        // where the door lands.
        let forward = Vector3::new(-yaw.sin(), 0.0, -yaw.cos());
        let off = forward.dot(flat.normalized()).acos();
        assert!(off > 0.20, "door is near centred, and the option list will sit on it");
        // The horizontal half-angle at TITLE_FOV and 16:9 is atan(tan(21 deg) * 16/9) = 0.59
        // rad, so this leaves the door a clear margin from the edge of the frame.
        assert!(off < 0.42, "door is {off} rad off axis and drifting out of frame");
        // Which side of the frame: project the door's bearing onto the camera's right axis,
        // which for a Y-up camera looking along `forward` is `cross(forward, up)`.
        let right = Vector3::new(-forward.z, 0.0, forward.x);
        assert!(
            right.dot(flat.normalized()) > 0.15,
            "door is not clearly to the right of frame, where the title and options are not"
        );

        // And on the side of the door the leaf swings AWAY from, so the opening is not behind
        // it. The hinge is the opening's local -x edge (`ext::door::Door::hinge_world`) and the
        // leaf opens past 90 degrees, so it always ends up beyond that edge: an eye at local
        // +x sees past it, an eye at local -x sees the back of it.
        assert!(eye.x > DOOR_POS.x, "camera is on the leaf's side and the doorway is blocked");
    }

    /// The title is shot on a longer lens than the game is played on, and a plausible one:
    /// wide enough to keep the meadow, narrow enough to make the door the subject.
    #[test]
    fn the_title_lens_is_longer_than_the_playing_one() {
        const { assert!(TITLE_FOV < crate::game_header::GH_FOV) }
        const { assert!(TITLE_FOV > 25.0) }
    }

    /// The one-way door's trigger: on the meadow -- at the spawn, at the title's vantage,
    /// on the doorstep -- it has not fired; anywhere the far door can deliver you, it has.
    #[test]
    fn a_crossing_is_being_past_the_split() {
        assert!(!in_far_world(Vector3::new(0.0, terrain::DOOR_Y + GH_PLAYER_HEIGHT, -2.0)));
        assert!(!in_far_world(title_view().0));
        assert!(!in_far_world(DOOR_POS + Vector3::new(0.0, GH_PLAYER_HEIGHT, -0.01)));
        assert!(in_far_world(FAR + Vector3::new(-0.01, GH_PLAYER_HEIGHT, 0.0)));
        assert!(in_far_world(FAR + Vector3::new(-20.0, GH_PLAYER_HEIGHT, 0.0)));
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
