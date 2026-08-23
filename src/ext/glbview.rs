//! EXT: `--view-glb PATH` -- a scene holding one glTF model and nothing else, so that a
//! screenshot of any file is one command. Not part of the C++ port.
//!
//! The model is loaded whole at its own scale (`Fit::Identity`), under the interior lighting
//! every Sketchfab room wants (`view::MOOD_INTERIOR`: no weather grade, the hemisphere in
//! `Shaders/gltfpbr.frag`, a dark sky), on a dark ground cap so the void below its floor is not
//! the meadow's horizon. The player is stood at the floor of the model's bounding box, in its
//! middle; `--pos`, `--yaw` and `--pitch` move them, as with `--scene`.
//!
//! If the file has animation, its first clip plays, looping, on the nodes it moves: each of
//! those is split out of the body into a part of its own (`PartSpec::skip`, `Frame::Scene`) and
//! drawn at `GltfModel::node_delta`, which is exactly what a level would do with the elevator's
//! doors -- so the viewer exercises the same path a scene will.
//!
//! A dev tool: it is not in the scene registry (the level-select menu and RESTART LEVEL know
//! nothing of it), and the pause menu's MAIN MENU leaves it for the title.

use std::cell::RefCell;
use std::rc::Rc;

use crate::app::crash::fatal;
use crate::ext::backrooms::GroundCap;
use crate::ext::gltf_model::{Anchor, Fit, Frame, GltfModel, Load, PartSpec};
use crate::ext::gltf_prop::GltfProp;
use crate::ext::view;
use crate::game_header::GH_PLAYER_HEIGHT;
use crate::object::{Object, ObjectT};
use crate::player::Player;
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec, Scene};
use crate::vector::Vector3;

/// The part the whole model, less any moving nodes, is gathered as.
const BODY: &str = "body";
/// Map cap: the size these rooms bake their maps at, so nothing is resampled.
const MAP: u32 = 1024;
/// What an unlit model's far end fades to: the Backrooms' own wall fog, a neutral enough dark.
const FOG: [f32; 4] = [0.40, 0.33, 0.16, 1.0];
/// How far past the model's footprint the invisible floor reaches: room to step back from a
/// facade and look at it.
const FLOOR_MARGIN: f32 = 30.0;

pub struct GlbViewer {
    /// Absolute, or relative to the asset root (see `Load::path`).
    path: String,
    /// Material names drawn in the translucent pass (`Load::translucent`).
    translucent: Vec<String>,
}

impl GlbViewer {
    pub fn new(path: String, translucent: Vec<String>) -> GlbViewer {
        GlbViewer { path, translucent }
    }
}

impl Scene for GlbViewer {
    fn load(
        &self,
        gl: &Rc<glow::Context>,
        res: &Resources,
        objs: &mut PObjectVec,
        _portals: &mut PPortalVec,
        player: &mut Player,
    ) {
        view::set_scene_mood(view::MOOD_INTERIOR);

        // The first clip's nodes come out of the body and into parts of their own. A moving
        // node nested under another moving node must not be gathered twice, so each part
        // skips every moving node but its own root.
        let clips = GltfModel::clips(&self.path).unwrap_or_else(|e| fatal(&e));
        let moving: Vec<String> = clips.first().map(|(_, nodes)| nodes.clone()).unwrap_or_default();
        let skips: Vec<Vec<&str>> = (0..=moving.len())
            .map(|i| {
                moving
                    .iter()
                    .enumerate()
                    .filter(|&(j, _)| j + 1 != i)
                    .map(|(_, n)| n.as_str())
                    .collect()
            })
            .collect();
        let roots: Vec<[&str; 1]> = moving.iter().map(|n| [n.as_str()]).collect();
        let mut parts = vec![PartSpec {
            name: BODY,
            roots: &[],
            skip: &skips[0],
            frame: Frame::Scene,
            anchor: Anchor::Hinge,
        }];
        for (i, node) in moving.iter().enumerate() {
            parts.push(PartSpec {
                name: node,
                roots: &roots[i],
                skip: &skips[i + 1],
                frame: Frame::Scene,
                anchor: Anchor::Hinge,
            });
        }
        let translucent: Vec<&str> = self.translucent.iter().map(String::as_str).collect();
        let spec = Load {
            path: &self.path,
            parts: &parts,
            fit: Fit::Identity,
            max_map: MAP,
            translucent: &translucent,
            metallic_override: &[],
            cut_boxes: &[],
        };
        let followers: Vec<(&str, &str)> =
            moving.iter().map(|n| (n.as_str(), n.as_str())).collect();
        let mut prop = GltfProp::new(gl, res, &spec, Vector3::zero(), 0.0, &followers, true);
        prop.set_fog_color(FOG);
        if let Some((clip, _)) = clips.first() {
            prop.play(clip, true);
        }

        // The model's extent over every part, at rest.
        let mut lo = Vector3::splat(f32::MAX);
        let mut hi = Vector3::splat(f32::MIN);
        for part in parts.iter().map(|p| p.name) {
            let b = prop.model().bounds(part);
            lo = Vector3::new(lo.x.min(b[0]), lo.y.min(b[2]), lo.z.min(b[4]));
            hi = Vector3::new(hi.x.max(b[1]), hi.y.max(b[3]), hi.z.max(b[5]));
        }
        log::info!(
            "[view-glb] {}: x[{:.2}, {:.2}] y[{:.2}, {:.2}] z[{:.2}, {:.2}]",
            self.path,
            lo.x,
            hi.x,
            lo.y,
            hi.y,
            lo.z,
            hi.z,
        );
        for (i, clip) in prop.model().animations().enumerate() {
            log::info!(
                "[view-glb] clip {:?}: {:.2} s, moves {:?}{}",
                clip.name(),
                clip.duration(),
                clip.nodes().collect::<Vec<_>>(),
                if i == 0 { " (playing)" } else { "" }
            );
        }

        // Eye height above the lowest floor, in the middle of the footprint: a sane spawn for
        // a single-storey room, and a starting point for `--pos` in anything taller.
        player.base.set_position(Vector3::new(
            0.5 * (lo.x + hi.x),
            lo.y + GH_PLAYER_HEIGHT,
            0.5 * (lo.z + hi.z),
        ));

        objs.push(Rc::new(RefCell::new(GroundCap::under(res, (lo, hi), lo.y)))
            as Rc<RefCell<dyn ObjectT>>);
        // An invisible floor under the cap's whole reach, so a player who walks off the model
        // -- or spawns over a hole in it -- stands in the dark rather than falling for ever.
        // `ground.obj` is a quad with one collider over its face; with no shader it is never
        // drawn (Object.cpp:21) but still collides.
        let mut floor = Object::new();
        floor.mesh = Some(res.acquire_mesh("ground.obj"));
        floor.pos = Vector3::new(0.5 * (lo.x + hi.x), lo.y, 0.5 * (lo.z + hi.z));
        floor.scale = Vector3::new(
            0.5 * (hi.x - lo.x) + FLOOR_MARGIN,
            1.0,
            0.5 * (hi.z - lo.z) + FLOOR_MARGIN,
        );
        objs.push(Rc::new(RefCell::new(floor)) as Rc<RefCell<dyn ObjectT>>);
        objs.push(Rc::new(RefCell::new(prop)) as Rc<RefCell<dyn ObjectT>>);
    }
}
