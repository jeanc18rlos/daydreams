//! EXT: Scene "The Ring" -- Hide 'N Dream's first arena, and the offline Duel it hosts.
//!
//! The Floorplan (level6's four-quadrant office with its six doorway portals) dressed as a
//! hide-and-seek arena: a dozen innocent props scattered through the rooms, one of which is
//! the **Dreamer** (`ext/hideseek.rs`) -- alive, creeping toward the exit pillar whenever no
//! eye is on it, and caught the moment the seeker's hand closes around it.
//!
//! Numbering note: level19-24 stay reserved for the shelved campaign sheets (GDD 7.6),
//! level25-29 for the shelved design pack; pivot arenas start at 30.
//!
//! Every round reshuffles which prop dreams and where it starts, so the arena stays a shell
//! game across the automatic reloads `ext/hideseek::duel_logic` requests.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ext::grab::Grabbable;
use crate::ext::hideseek::{duel_logic, Dreamer};
use crate::ext::room;
use crate::ext::scenes;
use crate::game_header::GH_PLAYER_HEIGHT;
use crate::object::ObjectT;
use crate::player::Player;
use crate::props::{floorplan, floorplan_add_portals, pillar};
use crate::resources::Resources;
use crate::scene::{PObjectVec, PPortalVec, Scene};
use crate::vector::Vector3;

pub struct Level30;

/// The disguise catalog: mesh, texture, visual scale, and the reveal name the hint line uses.
/// Scales and the 0.5 pick radius follow level7's proven prop set.
const KINDS: &[(&str, &str, f32, &str)] = &[
    ("teapot.obj", "gold.bmp", 0.30, "TEAPOT"),
    ("suzanne.obj", "gold.bmp", 0.45, "MONKEY"),
    ("bunny.obj", "white.bmp", 9.0, "RABBIT"),
];

/// Rest spots spread over the four quadrants (interior spans ~13 m square; quadrant centers
/// near (3.2, 3.2) and mirrors). Props settle onto the floor from y = 0.5.
const SPOTS: &[(f32, f32)] = &[
    (2.2, 3.4),
    (4.4, 2.0),
    (3.0, 5.2),
    (9.2, 2.4),
    (10.8, 4.2),
    (8.6, 5.0),
    (2.4, 9.0),
    (4.6, 10.6),
    (3.4, 11.8),
    (9.0, 9.2),
    (11.2, 10.4),
    (8.4, 11.6),
];

/// Where the Dreamer is trying to go: the pillar in the far corner from the seeker's spawn.
const EXIT: Vector3 = Vector3 { x: 11.8, y: 0.0, z: 11.8 };

/// A tiny LCG over a wall-clock seed: enough randomness for a shell game, no new crates.
fn shuffle_seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64 ^ d.as_secs())
        .unwrap_or(0x9E3779B9);
    nanos.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)
}

impl Scene for Level30 {
    fn load(
        &self,
        gl: &Rc<glow::Context>,
        res: &Resources,
        objs: &mut PObjectVec,
        portals: &mut PPortalVec,
        player: &mut Player,
    ) {
        // ── The arena: the Floorplan and its six doorway portals, untouched. ────────────────
        let fp = Rc::new(RefCell::new(floorplan(gl, res)));
        objs.push(fp.clone());
        floorplan_add_portals(&fp.borrow(), res, portals);

        // The level shell is what the Chameleon Rule raycasts against.
        let blockers: Vec<Rc<RefCell<dyn ObjectT>>> = vec![fp.clone() as Rc<RefCell<dyn ObjectT>>];

        // ── The exit: a pillar in the far corner. Reaching it is the Dreamer's win. ─────────
        let mut exit_marker = pillar(gl, res);
        exit_marker.pos = EXIT;
        exit_marker.scale = Vector3::splat(0.10);
        objs.push(Rc::new(RefCell::new(exit_marker)) as Rc<RefCell<dyn ObjectT>>);

        // ── The shell game: pick who dreams, and where. ─────────────────────────────────────
        let seed = shuffle_seed();
        // The Dreamer takes one of the six spots farthest from the exit, so every round
        // starts with a real journey ahead of it.
        let far_half: Vec<usize> = {
            let mut ix: Vec<usize> = (0..SPOTS.len()).collect();
            ix.sort_by(|&a, &b| {
                let d = |i: usize| {
                    let (x, z) = SPOTS[i];
                    let (dx, dz) = (x - EXIT.x, z - EXIT.z);
                    dz.mul_add(dz, dx * dx)
                };
                d(b).total_cmp(&d(a))
            });
            ix.truncate(SPOTS.len() / 2);
            ix
        };
        let dreamer_spot = far_half[(seed as usize) % far_half.len()];
        let dreamer_kind = ((seed >> 16) as usize) % KINDS.len();

        for (i, &(x, z)) in SPOTS.iter().enumerate() {
            let (mesh, tex, scale, _) = KINDS[i % KINDS.len()];
            let pos = Vector3::new(x, 0.5, z);
            if i == dreamer_spot {
                // The Dreamer wears the same catalog skin as its spot would have worn --
                // nothing about the lineup betrays which prop is alive.
                let (mesh, tex, scale, _) = KINDS[dreamer_kind];
                let mut d = Dreamer::new(0.5, EXIT, blockers.clone());
                {
                    let o = d.obj_mut();
                    o.mesh = Some(res.acquire_mesh(mesh));
                    o.shader = Some(res.acquire_shader("texture"));
                    o.texture = Some(res.acquire_texture(tex, 1, 1));
                    o.scale = Vector3::splat(scale);
                }
                d.set_position(pos);
                objs.push(Rc::new(RefCell::new(d)) as Rc<RefCell<dyn ObjectT>>);
                continue;
            }
            let mut g = Grabbable::new(0.5);
            {
                let o = g.obj_mut();
                o.mesh = Some(res.acquire_mesh(mesh));
                o.shader = Some(res.acquire_shader("texture"));
                o.texture = Some(res.acquire_texture(tex, 1, 1));
                o.scale = Vector3::splat(scale);
            }
            g.base.set_position(pos);
            objs.push(Rc::new(RefCell::new(g)) as Rc<RefCell<dyn ObjectT>>);
        }

        // ── The clock, the result card, and the reload into the next round. ─────────────────
        let ring = scenes::index_of("The Ring").expect("The Ring is registered");
        let (_, _, _, reveal) = KINDS[dreamer_kind];
        objs.push(Rc::new(RefCell::new(duel_logic(ring, reveal))) as Rc<RefCell<dyn ObjectT>>);

        // Spawn in the near quadrant facing the interior diagonal, so the first frame reads
        // as a room full of suspects rather than a wall. The heading rides the respawn
        // channel (applied after the first step) because a scene cannot reach the camera yaw.
        let spawn = Vector3::new(2.0, GH_PLAYER_HEIGHT, 2.0);
        player.base.set_position(spawn);
        room::request_respawn(room::Respawn::facing(spawn, Vector3::new(1.0, 0.0, 1.0)));
    }
}
