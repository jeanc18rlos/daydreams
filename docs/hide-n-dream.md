# Hide 'N Dream — working design doc (the pivot)

> **Status: approved direction, 2026-08-25.** DayDreams pivots to an exclusively-multiplayer
> hide-and-seek game. This doc is the working GDD of the pivot; `docs/level-design-pack.md`
> remains the mechanics reference; `docs/GDD.md` documents the engine and the shelved campaign.
> Full rationale and engineering detail: the approved plan (netcode architecture, refactor
> list, phases) — summarized in §5–6 here.

**One sentence:** *a group lucid dream in which hiding is a physics, finding is a viewpoint,
and the house itself is on the hiders' side.*

Working title **Hide 'N Dream** (pending Steam/trademark/domain search; fallbacks: Hide &
Dream, DreamSeek). Frame: a 2–5 min skippable corporate-induction video — "The Somnarium,
Group Sessions" — group dream therapy prescribing hide-and-seek as exposure therapy for the
fear of being perceived. Made with a video model; doubles as the trailer. No other story.

---

## 1. The round

```
LOBBY (elevator + Directory board)
  └─ PREP 120 s  — hiders place traps/decoys/blocks, then hide; seekers wait in the elevator
  └─ LINT <1 s   — auto-fix illegal placements ("MANAGEMENT HAS REARRANGED YOUR FURNITURE")
  └─ SEEK 240 s  — seekers hunt, 3 shared lives; final 60 s: hiders whisper every 10 s
  └─ RESOLVE 10 s — score card
  └─ ROLE SWAP → next round (match = 4 rounds)
```

| Parameter | v0 default |
|---|---|
| Players | 2–8 · 1 seeker ≤5 players, 2 for 6–7, 3 for 8 |
| Trap slots | 5 per hider · team cap 4 life-takers |
| Tag | E within 2.5 m (USE_REACH) · wrong tag = 4 s freeze, never a life |
| Chameleon Rule | hidden hiders creep 0.6 u/s ONLY while unobserved by every seeker |
| Trap death | fall to the Wake Room, −1 shared life, respawn at elevator in 5 s |
| Wins | 3 lives spent OR timeout → hiders · all tagged → seekers |
| Duel (2p) | 180 s seek, 7 slots, 2 lives, first to 12 — the playtest workhorse |
| Echoes | found hiders free-spectate + Nudge (move a prop ≤0.5 m, 20 s cd, scored lie) |

Scoring: hider +3 survive, +1/min alive if found (max 3) · seeker +2 per tag, +3 clear-all ·
trap owner +1 per spring · echo +1 per induced wrong tag.

## 2. Hiding verbs and their fair finds

Law: **every hide has a find.** Tells are grammar — consistent across all arenas forever.

| Verb | Hide | The find |
|---|---|---|
| Prop disguise (core) | you render as a catalog prop | pulse, totem, brochure, creep audio |
| Anamorphic flattening | bake into a wall decal, coherent only from its station tube (v0: 3 pegged walls/arena) | moving breaks the monocular illusion — find the viewpoint |
| Scale hiding | 0.6×1.0 mouth into a micro-interior | follow small (speed tax) or throw a prop through (traveler rescale flushes) — 2 watchable exits always |
| Key-portal room | LOCKED_TINT glass room, key hidden in 1 of 3 sockets | the glass shows the room; two-stage: spot, then hunt the key |
| Invisible room | the Level2 orphan bubble via one-way ring | audit door faces, breadcrumbs, step counting; audio leaks |
| Block stairs | prep-placed blocks bake as static ramps | the stair is evidence; decoys cost the shared 6-block budget |
| Pillar pocket | frameless back-pocket portal | walk the same pocket; breadcrumbs vanish there |

**Seeker tools** (suspicion only — tag is the only kill verb): Throw (infinite; tests
physics), Wake Pulse (2+1, 15 m through the portal graph, outlines what moved in 20 s),
Die Totem (3 scans; seven pips = dreamer; target hears the wind-up), Brochure (one wing's
baseline photo, spot-the-difference).

## 3. Traps (register: non-violent; a life lost = "waking up a bit")

| Trap | Slots | Effect | Tell |
|---|---|---|---|
| Thin Ice | 3 | fall → Wake Room, −1 life (the ONLY life-taker) | no runner carpet; throw-testable; tears visibly once sprung |
| Doorframe to Nowhere | 2 | bonk, 3 s stumble + gong to hiders | prop bounces off; 0.1 tint film |
| The Door That Minds | 2 | closes while watched | it breathes with your gaze |
| Alarm Clock | 3 | shrink-mouth detour, ~20 s lost | child-height frame, honest geometry |
| Wind Chime | 1 | pings hiders | visible string, duckable |

Rules: arm at seek-start; own team immune; one-shot; sprung state visible to all. Lint: no
two life-takers within 8 m; nothing within 6 m of elevator/Wake-Room exit; a trap-free route
to every room must exist (flood-fill).

## 4. Validation (King of Thieves × Clear Check)

- **Per-round lint** (<1 s, never blocks): portal-graph reachability of every hider and trap;
  taggability (2.5 m approach at legal p_scale); visibility floor per ambience preset;
  density caps; ≤16 portals counting placements; co-vis ≤2.
- **The Rehearsal** (publish gate for persistent layouts/arenas): author plays seeker,
  retrieves 3 auto-planted tokens from the hardest hide zones, 3 lives, 75% of the timer.
  Their time = the **Architect's Medal** (solvability + difficulty + async target). Any edit
  invalidates it.
- Social: host kick, 3-reason reports, fair-rate flagging, curated Featured shelf, 9-symbol
  codes. No public Workshop until the flywheel demands it.

## 5. Arenas v0 (≤8 fixed portals of 16 each; authored rebuilds pre-approved)

1. **The Ring** (Floorplan/level6; 2–5 players; native orphan-bubble) — BUILD FIRST, Duel
   arena, cheapest (texture + prop pass).
2. **The Ground Floor** (Backrooms authored rebuild ≤20 k tris — kills the scan licence risk).
3. **The Deep End** (Pool Rooms; splash audio showcase).

Prop-disguise catalog: 30–40 pieces; fastest source = the Miro board's Sketchfab furniture
collection (record licences in THIRD_PARTY.md at import — hard rule). Miro arena candidates:
abandoned metro tunnel, lowpoly bus station.

## 6. Engineering skeleton (full detail in the approved plan)

- **The prep phase IS the editor** — one grab-based placement UI serves prep, Traphouse
  (v0.5, persistent layouts by code), and the Dream Workshop (v1: room modules +
  any-door-to-any-door portal graph, live lint from GDD 3.10, Rehearsal gate). v2:
  text-to-3D (premium).
- **Netcode:** listen-server on `renet`+`renet_netcode` (UDP), polled on the main thread;
  `bincode` over existing serde. Clients own their own 500 Hz movement; host owns round
  state/traps/grabs/props (rapier host-only). 30 Hz player snapshots with a **warp_epoch**
  counter — never interpolate across a portal crossing, snap at the plane. NetId =
  load-order index (never `Portal::id`). `renet_steam` (SDR + lobbies) in P2; Tailscale as
  the interim family bridge. Steamworks paperwork starts with P1.
- **Observation semantics:** `is_observed_by_any` (multi-camera loop) for freeze mechanics;
  anamorphosis is strictly per-viewer, viewer reports the find.
- **Avatars:** Mixamo humanoids (owner decision) → skeletal animation milestone in
  `ext/gltf_model.rs` (skins + sampling + GPU skinning), lands with P1; P0 uses placeholders.
  Hidden hiders render as props regardless.
- **Phases:** OFFLINE DUEL (prove fun, zero sockets) → P0 LAN see-each-other (~2–3 wk) →
  P1 full round loop (~6–10 wk) → P2 Steam online (~4–8 wk) → P3 editor/UGC (months,
  gated on scenes-as-data).
- **Monetization:** sell compute and vanity (hosting, generation credits, cosmetics, crew
  portraits); playing + sharing stays free forever.

## 6.5 Built so far (2026-08-25)

**Third person** (`ext/thirdperson.rs`, `ext/avatar.rs`). `V` toggles; `--third-person` for a
screenshot. The RENDER camera goes on a boom over the right shoulder; the EYE does not move,
so the observation cone, the grab ray and the flashlight all still come from the player's
head. The boom casts against the scene every step and pulls in at walls, snapping in and
easing out. The player's body is the sleepwalkers' Mixamo mannequin, clip chosen by speed,
drawn only in third person and never collidable (it must never occlude its owner from a
watcher).

**The flashlight is an instrument, not cargo** (`ObjectT::carry_fixed`, `ext/tool.rs`). A held
tool keeps a fixed size and a fixed pose pointing down the view axis: the forced-perspective
carry is right for a thing you are moving and wrong for a thing you are using.

**A real light cone** (`view::set_spot`, five fragment shaders). `glow` was a point pool at the
raycast hit -- a stain on a wall. The cone carries origin, direction, range and two angles, and
is uploaded by all four shared draw paths, so it lights `gltfpbr`, `gltfunlit` (the baked
interiors, deliberately reversing that shader's "no lighting" rule for the one light the bake
cannot contain), `prop`, `texture` and `texture_array` -- which is every arena. `--torch`
lights a scene as if one were held, for photographing interiors.

**Interactive things answer the beam** (`view::set_shine`). An object marked shiny gets a rim
term scaled by how much cone is on it, so sweeping a dark room finds the kit rather than the
wallpaper. Tools are shiny; scenery is not.

**The Open House is a round** (`ext/hunt.rs`). Player is the HIDER; two sleepwalkers become
SEEKERS. HIDE 40 s (they wait on the porch, backs turned) -> SEEK 150 s (they sweep the house
room by room; being seen within 2.5 m is a tag) -> RESOLVE 6 s -> the same house again, reset
in place in under a millisecond, with a running tally. `--hide-seconds` / `--seek-seconds`
tune it. Also fixed: the round spawns on the arena's porch instead of 48 m away in fog, the
scene finally has a moon, a torch lies at the player's feet, and a patrolling NPC that walks
into a doorframe now gives up on that leg instead of standing there for the rest of the night.

**The round talks on its own HUD line** (`hint::status`, `hud::draw_status`, gold, top of
screen). It has to: a held tool and a worn disguise each insist on the ranked hint line every
frame, which is the whole of a round, so a clock written there would never be seen.

### Still open
- The seekers cannot open closed interior doors, and only the arena's front door is forced.
- One `glow` slot means a seeker cannot carry a torch of their own.
- The flat anamorphic hide is still unlosable (`disguise::hides_from`'s `Worn::Flat` arm
  returns true at any range) -- it needs a find, per this doc's own law.
- The instruments still read the street walker, not the seekers, so they are decoration
  during a round.

## 7. Offline Duel v0 (the Ring arena)

Pass-the-Mac, zero netcode, mostly shipped systems:
1. **Hider setup:** place traps + choose a disguise prop and hide spot; the hidden "dreamer"
   prop then behaves autonomously — it creeps toward an exit zone ONLY while unobserved
   (Level10 statue machinery, verbatim).
2. **Seeker run:** find and tag the dreamer (E) before it reaches the exit or the timer ends;
   Thin Ice costs lives; throw tests everything.
3. Exit test: the family laughs.

What survives the campaign pack: dream-depth dial → 3 ambience presets; Directory + Lift →
lobby; signs → brand voice; die → totem; Doubling Door → decoy mirror; Spiral/Show Home →
scale mouths; Stairwell → future flagship editor arena. Shelved: Apartment, acts, false
awakenings, Ninth Frame, campaign narrative.

---

## 8. What the Open House scene implements (2026-08-30)

`src/level32.rs` ("Open House", scene 21) is the second offline testbed after the Duel, and
the first place the hiding verbs are playable. Shipped:

| §2 verb | Status | Where |
|---|---|---|
| Prop disguise | **shipped** | `ext/disguise.rs` — E on any furniture the seed placed; find = hold still, and a sleepwalker inside `RUMBLE_RANGE` sees through it |
| Anamorphic flattening | **shipped**, 3 pegged walls | `ext/disguise.rs` stations; find = the station tube |
| Scale hiding | **shipped as the scale mouth** | `ext/warphouse.rs`; a 2:1 pair in the near house's lounge |
| Pillar pocket | **shipped**, 2 of them | `ext/warphouse.rs::pocket` |
| Invisible room | **shipped as the ghost house** | a furnished house at z = +3000, reachable only through the two pockets, exactly two exits |
| Key-portal room | not started | — |
| Block stairs | not started | prep-phase editor work |

**One deliberate departure from this doc.** The GDD has no patrols and no hostile AI: its only
autonomous agent is the Chameleon-Rule creature, and `docs/level-design-pack.md` forbids
agents in liminal space outright. The Open House nevertheless has five patrolling
sleepwalkers (`ext/npc.rs`), because a single-player build has no human seeker and the hiding
verbs are unplayable without someone to hide FROM. They are held to the doc's register: the
whole of their aggression is noticing. They never pursue (they stop `KEEP_BACK` = 2.6 m
short and stand), never harm, and the only thing being seen costs is the hide. If and when
real seekers arrive over the wire, the sleepwalkers become scenery or ambience -- nothing
else depends on them.

Portal budget for the arena: `GH_MAX_PORTALS` is 16 and a seventeenth is a release-mode
index panic every frame. The street loop spends 2, the fittings 6, leaving 8.
