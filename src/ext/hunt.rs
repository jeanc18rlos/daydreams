//! EXT: the Open House round -- one house, one hider, two seekers. Not part of the C++ port.
//!
//! The player is the HIDER. Two of the street's sleepwalkers (`ext/npc.rs`) wake up for the
//! seek phase and come through the house room by room; every hiding verb the scene already
//! shipped -- wear a chair, flatten onto a wall, take the hole in the back of the bedroom,
//! shrink through the lounge mouth -- is a way to survive them.
//!
//! ```text
//! HIDE  40 s   seekers wait on the porch, backs turned; find a spot, take the torch
//! SEEK 150 s   they sweep the house; being seen within arm's length is a tag
//! RESOLVE 6 s  the result, then the same house again with everything put back
//! ```
//!
//! # Why this is not `ext/hideseek.rs`
//!
//! That module is the Ring's Duel, where the player is the SEEKER and the quarry is a prop
//! that creeps while unobserved. This is the other side of the same game and it needs none
//! of the same parts: the hider here is the player, so there is nothing to animate, and the
//! finding is done by machines that already know how to look (`Npc::can_see`, the cone and
//! the line-of-sight raycast the sleepwalkers have always used). Keeping them apart leaves
//! the Ring's contract alone.
//!
//! # Two rules learned from the code around it
//!
//! * **The round talks on its own HUD line.** A held tool insists its reading every frame and
//!   a worn disguise insists how to shed it, so the ranked hint line is occupied for exactly
//!   the whole of a round. The clock goes through `hint::status` (`ext/hint.rs`).
//! * **The reset is in place, never a scene load.** Reloading this scene re-runs the
//!   procedural furnishing of seventeen houses and rebuilds their colliders. Between rounds
//!   the round instead puts everything back by hand, which costs a fraction of a millisecond
//!   and is what makes "again" instant.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::ext::npc::Npc;
use crate::ext::room::{self, Respawn, RoomLogic};
use crate::ext::{disguise, hint, npc, tool};
use crate::vector::Vector3;

/// Fixed steps per phase, at the engine's 500 Hz. The first two are defaults: a lobby (and
/// `--hide-seconds` / `--seek-seconds`) can override them, because how long is long enough is
/// the one number only playing can answer.
const HIDE_STEPS: u32 = 40 * 500;
const SEEK_STEPS: u32 = 150 * 500;
const RESOLVE_STEPS: u32 = 6 * 500;
/// Grace before wandering out of the house ends the round, so brushing the garden is free.
const STRAY_STEPS: u32 = 8 * 500;

/// Which part of the round is running.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Hide,
    Seek,
    Resolve,
}

/// How a round ended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum End {
    /// A seeker's hand fell on the player.
    Found,
    /// The clock ran out with the player still hidden.
    Survived,
    /// The player left the house and stayed out.
    Strayed,
}

thread_local! {
    /// Raised by a hunting sleepwalker that has the player at arm's length (`ext/npc.rs`),
    /// taken by the round. A flag rather than a call, for the reason every other channel here
    /// is one: the NPC is inside the object vector the round's own closure is being iterated
    /// from, so it cannot reach it directly.
    static TAG: Cell<bool> = const { Cell::new(false) };
    static PHASE: Cell<Phase> = const { Cell::new(Phase::Hide) };
    /// Rounds won by the hider and by the seekers, for the line between rounds.
    static TALLY: Cell<(u32, u32)> = const { Cell::new((0, 0)) };
    /// Phase lengths in fixed steps, overridable before the scene loads.
    static HIDE_LEN: Cell<u32> = const { Cell::new(HIDE_STEPS) };
    static SEEK_LEN: Cell<u32> = const { Cell::new(SEEK_STEPS) };
}

/// Override how long the hider gets, in seconds. Clamped to something a round can survive.
pub fn set_hide_seconds(secs: u32) {
    HIDE_LEN.with(|c| c.set(secs.clamp(1, 600) * 500));
}

/// Override how long the seekers get, in seconds.
pub fn set_seek_seconds(secs: u32) {
    SEEK_LEN.with(|c| c.set(secs.clamp(1, 900) * 500));
}

fn hide_len() -> u32 {
    HIDE_LEN.with(Cell::get)
}

fn seek_len() -> u32 {
    SEEK_LEN.with(Cell::get)
}

/// A hunting sleepwalker has the player. First one of the step wins; the round takes it.
pub fn report_tag() {
    TAG.with(|t| t.set(true));
}

/// Scene-load hygiene, alongside `npc::reset` and `disguise::reset`.
pub fn reset() {
    TAG.with(|t| t.set(false));
    PHASE.with(|p| p.set(Phase::Hide));
    TALLY.with(|t| t.set((0, 0)));
}

/// Everything the round needs to run and to put back afterwards.
pub struct Setup {
    /// The seekers, by handle, so the round can re-route them between phases.
    pub seekers: Vec<Rc<RefCell<Npc>>>,
    /// Where each seeker waits out the hide phase (a loop on the porch, backs to the house).
    pub posts: Vec<Vec<Vector3>>,
    /// Where each seeker goes when the hunt starts (a room-by-room sweep, one per seeker,
    /// started at different rooms so they split the house).
    pub sweeps: Vec<Vec<Vector3>>,
    /// Where the hider starts each round.
    pub start: Respawn,
    /// The house, as a world-space box. Leaving it starts the stray clock.
    pub house_lo: Vector3,
    pub house_hi: Vector3,
}

fn clock(steps: u32) -> String {
    let s = steps / 500;
    format!("{}:{:02}", s / 60, s % 60)
}

/// The round, as a `RoomLogic` the scene pushes into its object vector.
pub fn round_logic(setup: Setup) -> RoomLogic {
    reset();
    let Setup { seekers, posts, sweeps, start, house_lo, house_hi } = setup;

    let mut steps: u32 = hide_len();
    let mut stray: u32 = STRAY_STEPS;
    let mut over: Option<(End, u32)> = None;

    // Park the seekers on their posts for the first hide phase.
    for (i, s) in seekers.iter().enumerate() {
        if let Ok(mut s) = s.try_borrow_mut() {
            s.set_hunting(false);
            if let Some(post) = posts.get(i) {
                s.set_route(post.clone());
            }
        }
    }

    RoomLogic::new(move |ctx| {
        // One reader for the attention channel, here, so nothing fights over it: the round
        // needs it for its own line, and passes it straight on to the hint line so the
        // sleepwalkers keep their voice ("SOMEONE IS LOOKING AT YOU").
        let attention = npc::take_attention();
        if let Some(a) = attention {
            hint::notice(a.line());
        }

        // ── Resolve: hold the result up, then put the house back. ───────────────────────
        if let Some((end, ref mut wait)) = over {
            let (hider, seekers_won) = TALLY.with(Cell::get);
            hint::status(match end {
                End::Found => format!("FOUND!  THEM {seekers_won}  -  YOU {hider}"),
                End::Survived => format!("YOU SURVIVED  YOU {hider}  -  THEM {seekers_won}"),
                End::Strayed => format!("YOU LEFT THE HOUSE  -  YOU {hider}"),
            });
            *wait = wait.saturating_sub(1);
            if *wait == 0 {
                // In place, never `request_scene_load`: see the module docs.
                disguise::reset();
                npc::reset();
                tool::reset();
                room::request_respawn(start);
                for (i, s) in seekers.iter().enumerate() {
                    if let Ok(mut s) = s.try_borrow_mut() {
                        s.set_hunting(false);
                        if let Some(post) = posts.get(i) {
                            s.set_route(post.clone());
                        }
                    }
                }
                TAG.with(|t| t.set(false));
                PHASE.with(|p| p.set(Phase::Hide));
                steps = hide_len();
                stray = STRAY_STEPS;
                over = None;
            }
            return;
        }

        let phase = PHASE.with(Cell::get);
        steps = steps.saturating_sub(1);

        match phase {
            Phase::Hide => {
                hint::status(if steps > 10 * 500 {
                    format!("HIDE  {}", clock(steps))
                } else {
                    format!("THEY ARE COMING  {}", clock(steps))
                });
                if steps == 0 {
                    // Wake them: onto the sweep, and hunting.
                    for (i, s) in seekers.iter().enumerate() {
                        if let Ok(mut s) = s.try_borrow_mut() {
                            if let Some(sweep) = sweeps.get(i) {
                                s.set_route(sweep.clone());
                            }
                            s.set_hunting(true);
                        }
                    }
                    PHASE.with(|p| p.set(Phase::Seek));
                    steps = seek_len();
                }
            }
            Phase::Seek => {
                if TAG.with(Cell::take) {
                    TALLY.with(|t| {
                        let (h, s) = t.get();
                        t.set((h, s + 1));
                    });
                    PHASE.with(|p| p.set(Phase::Resolve));
                    over = Some((End::Found, RESOLVE_STEPS));
                    return;
                }
                // The leash: the arena is the house, and the street outside it loops for
                // hundreds of metres. Wandering off is answered, not punished silently.
                let p = ctx.player_pos;
                let inside = p.x >= house_lo.x
                    && p.x <= house_hi.x
                    && p.z >= house_lo.z
                    && p.z <= house_hi.z;
                if inside {
                    stray = STRAY_STEPS;
                } else {
                    stray = stray.saturating_sub(1);
                    if stray == 0 {
                        PHASE.with(|p| p.set(Phase::Resolve));
                        over = Some((End::Strayed, RESOLVE_STEPS));
                        return;
                    }
                    hint::status(format!("GET BACK IN THE HOUSE  {}", clock(stray)));
                    return;
                }
                hint::status(match attention {
                    Some(npc::Attention::Seen) => {
                        format!("SEEK  {}  -  THEY SEE YOU", clock(steps))
                    }
                    Some(npc::Attention::Near) => format!("SEEK  {}  -  CLOSE", clock(steps)),
                    None => format!("SEEK  {}", clock(steps)),
                });
                if steps == 0 {
                    TALLY.with(|t| {
                        let (h, s) = t.get();
                        t.set((h + 1, s));
                    });
                    PHASE.with(|p| p.set(Phase::Resolve));
                    over = Some((End::Survived, RESOLVE_STEPS));
                }
            }
            Phase::Resolve => {}
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_is_taken_once() {
        reset();
        report_tag();
        assert!(TAG.with(Cell::take));
        assert!(!TAG.with(Cell::take), "the round takes the flag, so it cannot fire twice");
    }

    #[test]
    fn the_clock_reads_as_minutes_and_seconds() {
        assert_eq!(clock(150 * 500), "2:30");
        assert_eq!(clock(7 * 500), "0:07");
        assert_eq!(clock(0), "0:00");
    }

    /// Every status line has to fit the HUD's legibility budget of 40 characters, and the
    /// font atlas has no glyphs outside ASCII 32..=126.
    #[test]
    fn status_lines_fit_the_hud_budget() {
        let lines = [
            format!("HIDE  {}", clock(HIDE_STEPS)),
            format!("THEY ARE COMING  {}", clock(9 * 500)),
            format!("SEEK  {}  -  THEY SEE YOU", clock(SEEK_STEPS)),
            format!("SEEK  {}  -  CLOSE", clock(SEEK_STEPS)),
            format!("GET BACK IN THE HOUSE  {}", clock(STRAY_STEPS)),
            format!("FOUND!  THEM {}  -  YOU {}", 88, 88),
            format!("YOU SURVIVED  YOU {}  -  THEM {}", 88, 88),
            format!("YOU LEFT THE HOUSE  -  YOU {}", 88),
        ];
        for l in lines {
            assert!(l.len() <= 40, "{l:?} is {} chars", l.len());
            assert!(l.is_ascii(), "{l:?} has a glyph the atlas does not carry");
        }
    }

    #[test]
    fn reset_clears_the_round() {
        report_tag();
        PHASE.with(|p| p.set(Phase::Seek));
        TALLY.with(|t| t.set((3, 4)));
        reset();
        assert!(!TAG.with(Cell::get));
        assert_eq!(PHASE.with(Cell::get), Phase::Hide);
        assert_eq!(TALLY.with(Cell::get), (0, 0));
    }
}
