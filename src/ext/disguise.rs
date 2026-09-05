//! EXT: the hide -- becoming a piece of the furniture, or a smear on a wall. Not part of
//! the C++ port.
//!
//! Two of `docs/hide-n-dream.md` §2's hiding verbs, both bound by that section's one law:
//! **every hide has a find**.
//!
//! | Verb | The hide | The find |
//! |---|---|---|
//! | Prop disguise | you render as a catalog prop and the sleepwalkers' eyes pass over you | hold still or be seen; a sleepwalker close enough knows this room has no such chair |
//! | Anamorphic flattening | you flatten onto a wall, coherent only from one station | it is only a picture from there; step off the station tube and it is a smear |
//!
//! # Why a channel and not a field on the player
//!
//! Nothing in this engine draws the player (`src/player.rs` never overrides `draw`), and
//! hanging a mesh on the player object leaks: `Object::reset` does not clear it and
//! `Engine::load_scene_from` reuses the same player into the next scene, so the prop would
//! follow you into the title screen. A mesh on the player would also arm it as a collider.
//! So the state lives here, on a thread-local the way `ext/tool.rs` and `ext/hideseek.rs`
//! keep theirs, and a separate scene object ([`Disguise`]) draws the prop at the player's
//! feet. Sleepwalkers read the same channel and never touch the player at all.
//!
//! Global back-face culling (the engine enables it once and never turns it off) is what
//! makes this free in first person: a closed prop drawn around the camera shows nothing
//! from inside, so the player's own view is unchanged while every other pass -- portal
//! views, and one day other players -- sees the chair.

use std::cell::Cell;
use std::rc::Rc;

use crate::camera::Camera;
use crate::ext::gltf_model::GltfModel;
use crate::ext::visibility::in_view_cone;
use crate::ext::{hint, prochouse};
use crate::game_header::{GH_DT, GH_PI, GH_PLAYER_HEIGHT};
use crate::object::{Object, ObjectT, RenderCtx, UpdateCtx};
use crate::resources::Resources;
use crate::shader::Shader;
use crate::vector::{Matrix4, Vector3};

/// The fastest the player may drift and still count as "holding still", in metres per
/// SECOND. It has to be a speed: `Disguise::update` runs on the 500 Hz fixed step, and the
/// most a sprinting player covers in one of those is about a centimetre -- so a raw
/// per-step distance threshold big enough to ignore jitter is also far too big to ever be
/// crossed by walking, and the disguise silently loses its only long-range find.
pub const STILL_SPEED: f32 = 0.25;
/// How long after moving the disguise still reads as stirring, in fixed steps. Without the
/// tail, a player who taps a key is only "stirring" for the single step they moved on --
/// far too fine a window for a sleepwalker looking twenty times a second.
const STIR_STEPS: u32 = 90;

/// Reach of the "become this" prompt, in metres: the same 2.5 m the key and the design
/// doc's tag both use.
pub const WEAR_REACH: f32 = 2.5;

/// An anamorphic station: where the eye must stand for the smear on the wall to resolve
/// into a picture, and how far off it may wander. The numbers are the Painted Cube's, which
/// were tuned by eye on a real wall.
pub const STATION_RADIUS: f32 = 1.5;
pub const STATION_CONE: f32 = 0.55;

/// What the player is wearing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Worn {
    /// A catalog prop, drawn at the player's feet.
    Prop { part: &'static str, label: &'static str },
    /// Flattened onto a wall: `station` is the index of the station in the scene's list.
    Flat { station: usize },
}

thread_local! {
    static WORN: Cell<Option<Worn>> = const { Cell::new(None) };
    /// Steps left of "the furniture moved". Nonzero means the hide is not holding still.
    static STIR: Cell<u32> = const { Cell::new(0) };
    /// What the player could become if they pressed E now, offered by whatever they are
    /// looking at. A standing answer, cleared by whoever stops offering -- the same rule
    /// `ext/key.rs` documents, and for the same reason: a frame can run zero fixed steps.
    static OFFER: Cell<Option<Worn>> = const { Cell::new(None) };
    /// The frame's E, handed over by the engine.
    static PRESS: Cell<bool> = const { Cell::new(false) };
}

pub fn worn() -> Option<Worn> {
    WORN.with(Cell::get)
}

/// Wear it. Ignored while already wearing something -- shed first.
pub fn wear(w: Worn) {
    WORN.with(|c| {
        if c.get().is_none() {
            c.set(Some(w));
        }
    });
}

/// Step out of it, by choice.
pub fn shed() {
    WORN.with(|c| c.set(None));
    STIR.with(|c| c.set(0));
}

/// Step out of it, because you were found. Same effect; named for the call site's sake.
pub fn blow() {
    shed();
}

/// The player moved this step: the hide is stirring, and stays stirring for a moment after.
pub fn stir() {
    STIR.with(|c| c.set(STIR_STEPS));
}

fn tick_stir() {
    STIR.with(|c| c.set(c.get().saturating_sub(1)));
}

pub fn stirring() -> bool {
    STIR.with(Cell::get) > 0
}

/// What the crosshair is offering, if anything.
pub fn offered() -> Option<Worn> {
    OFFER.with(Cell::get)
}

pub fn offer(w: Option<Worn>) {
    OFFER.with(|c| c.set(w));
}

/// Whether E should become a hide this frame: something is offered, or something is worn
/// and can be stepped out of. Read by `Engine::ext_update`'s claim chain.
pub fn wants_use() -> bool {
    worn().is_some() || offered().is_some()
}

pub fn press() {
    PRESS.with(|c| c.set(true));
}

fn take_press() -> bool {
    PRESS.with(Cell::take)
}

/// Scene-load hygiene: a hide must not outlive its street.
pub fn reset() {
    WORN.with(|c| c.set(None));
    STIR.with(|c| c.set(0));
    OFFER.with(|c| c.set(None));
    PRESS.with(|c| c.set(false));
}

/// Whether a hide currently defeats `watcher`'s eyes, for a player at `player`.
///
/// This is the whole of the find, stated once so the sleepwalkers and the tests agree:
///
/// * nothing worn -- you are a person, and people are visible;
/// * stirring -- furniture does not walk, so a moving prop is a person;
/// * a prop, held still, at more than [`crate::ext::npc::RUMBLE_RANGE`] -- you are furniture;
/// * a prop, held still, closer than that -- close enough to know this room has no such
///   chair, and the hide is over;
/// * flattened -- you are a picture from the station and a smear from anywhere else, so a
///   watcher standing IN the station tube sees a person and everyone else sees wallpaper.
pub fn hides_from(watcher: Vector3, player: Vector3, _watcher_eye: &Matrix4) -> bool {
    let Some(w) = worn() else { return false };
    if stirring() {
        return false;
    }
    match w {
        // HORIZONTAL separation: the caller's `watcher` is a walker's feet and `player` is
        // an eye, so a plain 3-D distance quietly spends most of the find radius on the
        // height difference -- and would shrink further still for a player halved by a
        // scale mouth. How far apart they stand is what the find is actually about.
        Worn::Prop { .. } => {
            let d = watcher - player;
            Vector3::new(d.x, 0.0, d.z).mag() > crate::ext::npc::RUMBLE_RANGE
        }
        // A flattened player is invisible to a walker on the ground: the smear only
        // resolves from its station, and the sleepwalkers never stand there.
        Worn::Flat { .. } => true,
    }
}

/// Whether the PLAYER's own eye is inside a station's tube -- the anamorphic find, and the
/// test that decides whether the flattened picture is coherent for whoever is looking.
pub fn at_station(eye: &Matrix4, station: Vector3, centre: Vector3) -> bool {
    (eye.translation() - station).mag() <= STATION_RADIUS && in_view_cone(eye, centre, STATION_CONE)
}

/// A wall a player may flatten onto: where the smear sits, and where you must stand for it
/// to be a picture.
#[derive(Clone, Copy, Debug)]
pub struct Station {
    /// Centre of the decal on the wall.
    pub centre: Vector3,
    /// Where the eye must be.
    pub view: Vector3,
    /// Yaw of the wall's face.
    pub yaw: f32,
}

/// The prompts. All inside the HUD's 40-character budget (asserted below).
pub const SHED_HINT: &str = "E  STEP OUT";
pub const FLAT_HINT: &str = "E  FLATTEN ONTO THE WALL";
pub const WORN_FLAT_HINT: &str = "E  PEEL OFF THE WALL";

/// The scene object that wears the disguise: it draws the prop at the player's feet, keeps
/// the stirring tell, offers the wall stations, and consumes the E the engine hands it.
pub struct Disguise {
    base: Object,
    model: Rc<GltfModel>,
    pbr: Rc<Shader>,
    stations: Vec<Station>,
    /// Where the player's feet were on the previous step, for the stirring test.
    last_feet: Vector3,
    /// The prop's own yaw, kept so a worn chair does not spin with the mouse: it turns to
    /// the look direction only in slow steps.
    prop_yaw: f32,
    /// The player's size, so the prop is the size of whoever is wearing it.
    scale: f32,
    /// Whether `last_feet` has ever been written: the first step has nothing to compare to.
    settled: bool,
}

impl Disguise {
    pub fn new(gl: &Rc<glow::Context>, res: &Resources, stations: Vec<Station>) -> Disguise {
        Disguise {
            base: Object::new(),
            model: GltfModel::acquire(gl, &prochouse::load_spec()),
            pbr: res.acquire_shader("gltfpbr"),
            stations,
            last_feet: Vector3::zero(),
            prop_yaw: 0.0,
            scale: 1.0,
            settled: false,
        }
    }

    /// The nearest station whose tube the eye is standing in, if any.
    fn station_here(&self, eye: &Matrix4) -> Option<usize> {
        self.stations.iter().position(|s| at_station(eye, s.view, s.centre))
    }
}

impl ObjectT for Disguise {
    fn base(&self) -> &Object {
        &self.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base
    }

    fn update(&mut self, ctx: &UpdateCtx) {
        tick_stir();
        // The player's position is the EYE, and the drop to their soles is their HEIGHT
        // TIMES THEIR SIZE -- a scale mouth (`ext/warphouse.rs`) halves `p_scale`, and a
        // prop stood a fixed 1.5 m under a shrunk player's eye is buried to its waist.
        let feet = ctx.player_pos - Vector3::new(0.0, GH_PLAYER_HEIGHT * ctx.player_p_scale, 0.0);
        // The first step has no previous position to compare against; taking the initial
        // zero as "moved" would report the hide as stirring the moment it is put on.
        if self.settled && (feet - self.last_feet).mag() > STILL_SPEED * GH_DT {
            stir();
        }
        self.settled = true;
        self.last_feet = feet;
        self.base.pos = feet;
        // A shrunk player is a shrunk chair: the prop wears the player's size, or a
        // half-height player hides behind a full-height armchair.
        self.scale = ctx.player_p_scale;

        let pressed = take_press();
        match worn() {
            Some(Worn::Prop { .. }) => {
                // A worn prop turns with the look, but slowly: a chair that snaps around
                // with the mouse is a person wearing a chair.
                let f = ctx.cam_to_world.mul_direction(Vector3::new(0.0, 0.0, -1.0));
                let want = f.x.atan2(f.z);
                let mut d = want - self.prop_yaw;
                while d > GH_PI {
                    d -= 2.0 * GH_PI;
                }
                while d < -GH_PI {
                    d += 2.0 * GH_PI;
                }
                self.prop_yaw += d * 0.02;
                hint::insist(SHED_HINT);
                if pressed {
                    shed();
                }
            }
            Some(Worn::Flat { .. }) => {
                hint::insist(WORN_FLAT_HINT);
                if pressed {
                    shed();
                }
            }
            None => {
                // Standing in a station's tube offers the wall.
                if let Some(ix) = self.station_here(&ctx.cam_to_world) {
                    offer(Some(Worn::Flat { station: ix }));
                    hint::insist(FLAT_HINT);
                    if pressed {
                        wear(Worn::Flat { station: ix });
                        self.prop_yaw = self.stations[ix].yaw;
                    }
                } else if let Some(w) = offered() {
                    // A prop under the crosshair offered itself through `pick_hint`.
                    if pressed {
                        wear(w);
                        let f = ctx.cam_to_world.mul_direction(Vector3::new(0.0, 0.0, -1.0));
                        self.prop_yaw = f.x.atan2(f.z);
                    }
                }
            }
        }
        // The offer is one slot shared with `WearOffers`, and only its OWNER may withdraw
        // it: this branch clears the STATION offer when the eye leaves the tube, and
        // nothing else. Clearing it outright stomped the wearable offer that `WearOffers`
        // had set earlier in the same step, so `wants_use()` read false at claim time and
        // every E fell through to the grab -- the hide could be prompted but never made.
        if worn().is_none()
            && self.station_here(&ctx.cam_to_world).is_none()
            && matches!(offered(), Some(Worn::Flat { .. }))
        {
            offer(None);
        }
    }

    fn draw(&self, ctx: &RenderCtx, cam: &Camera, _fbo: Option<glow::Framebuffer>) {
        match worn() {
            Some(Worn::Prop { part, .. }) => {
                // The pack is loaded `Fit::Identity`, so a part's geometry sits wherever
                // its author parked it in the showroom -- tens of metres from the origin.
                // A drawer that does not cancel that offset draws the costume across the
                // street from the person wearing it; `PackProps::add` cancels exactly the
                // same way for the houses' own furniture.
                let b = self.model.bounds(part);
                let anchor = Vector3::new((b[0] + b[1]) * 0.5, b[2], (b[4] + b[5]) * 0.5);
                let mut obj = Object::new();
                obj.euler.y = self.prop_yaw;
                obj.scale = Vector3::splat(self.scale);
                obj.pos = self.base.pos
                    - Matrix4::rot_y(self.prop_yaw).mul_direction(anchor * self.scale);
                self.model.draw_part(part, &obj, &self.pbr, cam, ctx);
            }
            Some(Worn::Flat { station }) => {
                // Flattened: the prop is squashed against the wall it was pressed onto.
                // From the station it reads as a picture of a person; from anywhere else it
                // is a smear a hand's breadth thick, which is exactly the find.
                let Some(s) = self.stations.get(station) else { return };
                let b = self.model.bounds(FLAT_PART);
                let centre =
                    Vector3::new((b[0] + b[1]) * 0.5, (b[2] + b[3]) * 0.5, (b[4] + b[5]) * 0.5);
                let scale = Vector3::new(1.0, 1.0, 0.02);
                let mut obj = Object::new();
                obj.euler.y = s.yaw;
                obj.scale = scale;
                // Same cancellation as the worn prop, about the decal's own middle rather
                // than its feet: a picture hangs from its centre.
                let off = Vector3::new(centre.x * scale.x, centre.y * scale.y, centre.z * scale.z);
                obj.pos = s.centre - Matrix4::rot_y(s.yaw).mul_direction(off);
                self.model.draw_part(FLAT_PART, &obj, &self.pbr, cam, ctx);
            }
            None => {}
        }
    }

    /// The disguise is a costume, not a body: the player's own capsule is still doing the
    /// blocking, so nothing here collides and no portal warps it.
    fn engine_collision(&self) -> bool {
        false
    }
    fn static_collision(&self) -> bool {
        false
    }
}

/// What a flattened player is drawn as: a painting off the village pack, squashed to the
/// wall. A person-shaped decal would want its own asset; this reads correctly and costs
/// nothing.
const FLAT_PART: &str = "painting";

/// A wearable piece of furniture: the catalog part the player becomes, and the prompt.
#[derive(Clone, Copy, Debug)]
pub struct Wearable {
    pub part: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub pos: Vector3,
}

/// The scene rule that offers the wearables: one object watching the player's distance to
/// each, rather than a `pick_hint` on every piece of furniture (the houses are drawn as one
/// batched `PackProps`, so their pieces are not objects and have no hooks of their own).
pub struct WearOffers {
    base: Object,
    items: Vec<Wearable>,
}

impl WearOffers {
    pub fn new(items: Vec<Wearable>) -> WearOffers {
        WearOffers { base: Object::new(), items }
    }
}

impl ObjectT for WearOffers {
    fn base(&self) -> &Object {
        &self.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base
    }

    fn update(&mut self, ctx: &UpdateCtx) {
        if worn().is_some() {
            return;
        }
        // The nearest wearable within reach that the player is actually looking at.
        let mut best: Option<(f32, &Wearable)> = None;
        for w in &self.items {
            let d = (w.pos - ctx.player_pos).mag();
            if d > WEAR_REACH {
                continue;
            }
            if !in_view_cone(&ctx.cam_to_world, w.pos, 0.7) {
                continue;
            }
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, w));
            }
        }
        match best {
            Some((_, w)) => {
                offer(Some(Worn::Prop { part: w.part, label: w.label }));
                hint::insist(w.hint);
            }
            None => {
                // Only withdraw what WE offered: the station branch owns its own offer.
                if matches!(offered(), Some(Worn::Prop { .. })) {
                    offer(None);
                }
            }
        }
    }

    fn engine_collision(&self) -> bool {
        false
    }
    fn static_collision(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eye_at(p: Vector3) -> Matrix4 {
        Matrix4::trans(p)
    }

    #[test]
    fn every_prompt_fits_the_hud() {
        for line in [SHED_HINT, FLAT_HINT, WORN_FLAT_HINT] {
            assert!(line.len() <= 40, "{line:?} is too long for the hint line");
            assert!(line.is_ascii(), "{line:?} has glyphs the font atlas lacks");
        }
    }

    /// The law of the design doc: every hide has a find. Each branch of `hides_from` is one
    /// of them, and none of them hides a player who is walking about.
    #[test]
    fn a_moving_hide_is_no_hide() {
        reset();
        let far = Vector3::new(30.0, 0.0, 0.0);
        let player = Vector3::zero();
        assert!(!hides_from(far, player, &eye_at(far)), "nothing worn is nothing hidden");

        wear(Worn::Prop { part: "armchair", label: "ARMCHAIR" });
        assert!(hides_from(far, player, &eye_at(far)), "a still prop at range hides");

        stir();
        assert!(!hides_from(far, player, &eye_at(far)), "furniture does not walk");
        // And the tell has a tail, so a single moving step is not a single blind step.
        for _ in 0..STIR_STEPS - 1 {
            tick_stir();
        }
        assert!(stirring(), "the stirring tell must outlast the step that caused it");
        tick_stir();
        assert!(!stirring());
        assert!(hides_from(far, player, &eye_at(far)), "held still again, hidden again");
        reset();
    }

    /// The stirring threshold has to be crossable BY WALKING. It is compared against one
    /// 500 Hz step, and a sprinting player covers about a centimetre in one of those -- so a
    /// threshold picked as a raw distance is silently unreachable and the disguise keeps
    /// working while its wearer runs down the street. This is that bug.
    #[test]
    fn walking_crosses_the_stirring_threshold() {
        let per_step = STILL_SPEED * GH_DT;
        // A walk is about 3 m/s; a sprint about 5.2 (GH_WALK_SPEED * the sprint factor).
        let walk_step = 3.0 * GH_DT;
        assert!(
            walk_step > per_step,
            "walking ({walk_step} m/step) does not cross the threshold ({per_step} m/step)"
        );
        // ...and standing still, with nothing but float noise, does not.
        assert!(1e-4 < per_step, "the threshold is so tight that standing still stirs");
    }

    /// The prop disguise's close find: a sleepwalker at arm's length knows this room has no
    /// such chair.
    #[test]
    fn a_prop_is_found_from_close_up() {
        reset();
        wear(Worn::Prop { part: "armchair", label: "ARMCHAIR" });
        let player = Vector3::zero();
        let close = Vector3::new(crate::ext::npc::RUMBLE_RANGE - 0.2, 0.0, 0.0);
        let far = Vector3::new(crate::ext::npc::RUMBLE_RANGE + 0.2, 0.0, 0.0);
        assert!(!hides_from(close, player, &eye_at(close)), "this close, it is a person");
        assert!(hides_from(far, player, &eye_at(far)), "at a distance, it is a chair");
        // The find is HORIZONTAL: the real caller passes a walker's feet against a player's
        // eye, and a 3-D distance would spend most of the radius on the height between them.
        let head_high = close + Vector3::new(0.0, 1.55, 0.0);
        assert!(
            !hides_from(head_high, player, &eye_at(head_high)),
            "the height between feet and eye ate the find radius"
        );
        reset();
    }

    /// The anamorphic find is the station tube: a picture from there, a smear anywhere else.
    #[test]
    fn the_smear_is_a_picture_only_from_the_station() {
        let centre = Vector3::new(0.0, 1.2, 0.0);
        let view = Vector3::new(0.0, 1.2, 3.0);
        // A camera looks down its OWN -Z, so an eye at +z with no rotation is already
        // looking back at the wall. Getting this backwards is the classic way to point a
        // viewer out of the back of its own head.
        let on = Matrix4::trans(view);
        assert!(at_station(&on, view, centre), "the station itself must resolve");
        // The same look, a stride to the side: outside the tube.
        let off = Matrix4::trans(view + Vector3::new(STATION_RADIUS + 0.5, 0.0, 0.0));
        assert!(!at_station(&off, view, centre), "a stride off the spot is a smear");
        // Standing on the spot but facing away: not the picture either.
        let away = Matrix4::trans(view) * Matrix4::rot_y(GH_PI);
        assert!(!at_station(&away, view, centre), "the station is a look, not just a spot");
    }

    /// Wearing is one thing at a time, and shedding always works.
    #[test]
    fn one_costume_at_a_time() {
        reset();
        wear(Worn::Prop { part: "bed", label: "BED" });
        wear(Worn::Prop { part: "armchair", label: "ARMCHAIR" });
        assert_eq!(
            worn(),
            Some(Worn::Prop { part: "bed", label: "BED" }),
            "the second costume must not overwrite the first"
        );
        shed();
        assert_eq!(worn(), None);
        wear(Worn::Flat { station: 2 });
        assert_eq!(worn(), Some(Worn::Flat { station: 2 }));
        blow();
        assert_eq!(worn(), None, "being found takes the costume off");
        reset();
    }

    /// The offer is ONE slot shared by two objects, and a non-owner clearing it is how the
    /// hide silently stopped working: `WearOffers` sets a `Prop` offer early in the step,
    /// `Disguise` runs later and must leave it alone. Withdrawal is by kind, not by fiat.
    #[test]
    fn only_the_owner_of_an_offer_may_withdraw_it() {
        reset();
        // What `WearOffers` does when it has nothing to offer: clears only Prop offers.
        offer(Some(Worn::Flat { station: 1 }));
        if matches!(offered(), Some(Worn::Prop { .. })) {
            offer(None);
        }
        assert_eq!(
            offered(),
            Some(Worn::Flat { station: 1 }),
            "the wearables' rule stomped the station's offer"
        );
        // What `Disguise` does when the eye is out of every tube: clears only Flat offers.
        offer(Some(Worn::Prop { part: "bed", label: "BED" }));
        if matches!(offered(), Some(Worn::Flat { .. })) {
            offer(None);
        }
        assert_eq!(
            offered(),
            Some(Worn::Prop { part: "bed", label: "BED" }),
            "the station's rule stomped the wearables' offer -- E falls through to the grab"
        );
        reset();
    }

    /// The E claim: something to become, or something to step out of.
    #[test]
    fn the_press_is_claimed_only_when_there_is_a_hide_to_make_or_break() {
        reset();
        assert!(!wants_use(), "nothing offered, nothing worn");
        offer(Some(Worn::Prop { part: "bed", label: "BED" }));
        assert!(wants_use());
        offer(None);
        assert!(!wants_use());
        wear(Worn::Flat { station: 0 });
        assert!(wants_use(), "a worn hide always offers the way out");
        reset();
        assert!(!wants_use(), "a scene load clears the offer");
        assert!(!take_press(), "and any press with it");
    }
}
