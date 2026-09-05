# DayDreams — Level & Puzzle Design Pack (v1)

> **Status: [PROPUESTA] throughout, except where a block cites implemented systems.**
> Written 2026-08-24 against GDD v0.1 and the real repo state (19 scenes, 419 tests green).
> Written in English; it slots into the GDD as an extension of chapter 7.6 (sheets 7–11), chapter 3
> (two new mechanic families), and chapter 5 (macro-structure amendments). Everything here was
> checked against the engine's hard rules (GDD 2.6, 3.10, 7.4); where a design needs new code,
> the cost is stated explicitly and is deliberately small.
>
> Sources: the full GDD, code autopsies of the broken levels, Superliminal's local install and
> developer commentary, Bruce's GDC material, and the exotic-geometry / dream-logic / UGC
> research corpus. Every sheet then survived an adversarial feasibility/consistency/design
> review against the actual source; the fixes are folded in.

---

## 0. The thesis: attention is physics

DayDreams already has three verbs, all implemented:

| Verb | System | What it means to the player |
|---|---|---|
| **Scale** | `p_scale` written by grab and by portals, compounding | *Where you release a thing decides how big it truly is.* |
| **Connection** | portal warps, passable/tint, one-way wiring | *Space is a rumor; only doorways tell the truth.* |
| **Attention** | `ext/visibility.rs`, portraits, anamorphosis, station points | *Where you stand — and where you look — decides what is real.* |

The game's identity problem ("technical demos that work incredibly, but no connective tissue")
is solved by promoting the third verb to equal rank. Superliminal owns scale. CodeParade owns
portals. **Nobody owns attention as physics** — and the engine already computes it every frame,
through portals, wider than the screen, with the discipline that nothing ever changes while
rendered. The catalog research confirms the pairing: Antichamber's most-loved tricks are all
attention tricks, and its least-loved content is what it fell back on when it ran out of them.
DayDreams has scale portals to carry the late game — the one thing Antichamber lacked.

Scoping note so the sheets read honestly against the thesis: the GDD's own Act III material
(sheet 5 "Nobody Watching", the anamorphic scenes 10–12) *already is* the attention act — this
pack deliberately does not duplicate it. The new sheets fill the scale/connection gaps
(cloning, the Spiral, the player-built portals, the Turns), thread attention through as
recurring beats (the Apartment's print, the runner grammar, the Directory), and Act IV (§3.6)
is where all three verbs compose.

**One sentence for the wall:** *DayDreams is a lucid dream in which looking, standing, and
letting go are the three ways the world listens to you.*

Two findings from this research round shape everything below:

1. **The rotated-world portal is unshipped design space.** No first-person game fakes Escher
   wall-walking with rotated duplicate worlds behind upright portals under fixed gravity —
   Manifold Garden and Fragments of Euclid rotate gravity, Monument Valley uses an orthographic
   camera, Portal rotates the player. The code autopsy confirms the portal and collision math
   deliver it **today, with no core-engine changes** (two small ext/tool tasks adapt GLB
   interiors — costed in §6): build the far copy pre-rotated (the `window.rs` FAR2 idiom),
   `TriMeshCollider` bakes the rotated transform at build time, the former wall genuinely pushes
   with `push.y ≈ 1` and *is* floor, and both portal quads stay upright in their own worlds so
   the yaw-only re-aim in `physical.rs:111-113` is exactly correct. This is the spine of the
   Escher replacement (Sheet 11).
2. **Cloning does not exist anywhere in the GDD** — it is genuinely new. The design below makes
   it a *property of a doorway*, not a new input, exploiting the engine's load-bearing
   cargo-vs-traveler asymmetry: held objects never cross portals; loose objects do.
   **What you carry stays one; what you let go multiplies.** (Sheet 8.)

---

## 1. The dream-depth dial (atmosphere spine)

One scalar, `dream_depth ∈ [0,1]`, owned by the engine and fed to every presentation system at
once, so the whole game slides on a single axis and each level sits at a fixed depth. Everything
it drives already exists or is one uniform away:

| Channel | System (exists) | depth 0 (awake) | depth 1 (deep) |
|---|---|---|---|
| Grade | mood per render pass (`ext/view.rs`) | plain daylight | scene mood, decoupled fog hue |
| Fog | squared-distance fog in `gltfunlit/prop/painting.frag` | tracks sky horizon (weather) | unmotivated hue — distance becomes *a wall of the dream* |
| Bloom/veil | `ext/postfx.rs` (title-only today; the bracket is reusable) | off — clean blacks | veil lifts shadows toward bloom color; no true black anywhere |
| Light shafts | `ext/doorlight.rs` | absent | reserved for *meaning*: shafts mark exits/tears only |
| FOV | runtime FOV (`ext/view.rs`) | rock stable | ±1.5° sine drift over 20–30 s, consciously invisible |
| Head bob | `Factors::bob` | crisp | eased toward glide (float) |
| Audio bed | `tools/gen_sfx.py` motif | one motif at 1.0×, dry, small room | same motif at 0.5×, detuned ±5 cents, 0.1–0.3 Hz LFO, long tails |
| Footsteps | `Surface::SplitX/Tile` | dry | softened set, splash/moss |
| Portal tint | `Portal::tint` | — | link grammar: warm tint = surfacing, cool = sinking |

Depth assignments: Apartment 0.0→0.2 · Meadow 0.25 · Act I Backrooms 0.5→0.65 ·
Act II Pool Rooms 0.75 · Act III floors 0.85–0.95 · Act IV 1.0 → final credits meadow snaps
to 0.15 (the only time the player *feels* the dial move down).

Rules, from the Kon/liminal research, all engine-cheap:

- **Every portal is a match cut.** Hold exactly ONE channel continuous across a threshold — the
  doorway silhouette, the light-shaft angle, the fog color, or a sustained tone — and change
  everything else. Never change all channels at once; the held channel is what makes a crossing
  feel dreamt instead of loaded. (The title door already does this: same frame, storm meadow,
  sunset through the opening.)
- **Kenopsia is the fear object.** Liminal levels stay empty of agents. Ambience may settle
  (water finding its level, ductwork ticking as it cools) but must never read as *someone* —
  GDD 6.5 is strict that the Tenant never makes its own sound and never reacts to the player,
  so no sound may be agentive or player-triggered-by-proximity. The dial must never violate it.
- **One channel, three grammars.** `Portal::tint` now carries three meanings; reserve hue bands
  so they never collide: **green** = locked (GDD 3.1.6's LOCKED_TINT, unchanged), **neutral
  dark alpha** = spent budget (Sheet 8's dimming doors), **warm/cool cast** = dream depth.
  Lock-state always wins when combined; depth cast never exceeds 0.25 alpha.
- **Repetition with one defect.** Tile a space almost perfectly, then break the pattern exactly
  once (one doorway 15% taller, one tile of meadow grass indoors, one recurring painting).
  Reuse existing props deliberately as cross-level motifs, not as asset economy.
- **Silence is the loudest stinger.** Drop all audio for 2–3 s before any tell is allowed to fail.

### 1.1 The totem: a die with seven pips

The player needs one repeatable, self-initiated reality test (Inception's totem rule). The die
already exists (`ext/rigid.rs`, on the Backrooms carpet, stowable). **In the dream, every face
of the die shows seven pips.** Implementation: dream scenes load a seven-pip atlas variant —
no rest-time texture swap, so nothing ever mutates on screen (the pack's own law, GDD 3.9);
in-hand inspection *is* the test, and the grab solves legibility (an enlarged die has readable
pips — enlarging your totem to check it is a beautifully in-genre gesture). In the Apartment
(depth 0) it is an honest 1–6 die, seeded by a dice tray on the nightstand so the toy gets
handled before the dream ever starts. The tell is **cashed** at the false awakenings (§2):
the corridor looks like home — the die in your pocket says otherwise. No text ever names it.
Late-game license (Paprika mode): the tell may lie **exactly once**, in Act IV, after two acts
of reliability — never before the rule is learned.

---

## 2. The Directory: the structure that connects everything

Antichamber's deepest lesson is structural, not spatial: a hub that owns a self-drawing **graph
map**, progression gated ~70% by **knowledge** rather than items, and **signs that confirm
lessons after they are embodied**. DayDreams adopts all three in diegetic form — no HUD map
(GDD P3-10 stands), no Esc-teleport (camera never cuts), no state-wiping resets (Antichamber's
own documented mistake).

**The hub is the Lift.** It already rides a one-way ring and tolerates unregistered floor names
(`elevator::FLOORS`). Three amendments:

1. **The Directory board** — a framed building cross-section in the Backrooms lobby, drawn by
   the portrait machinery (`ext/painting.rs` cutout layers). Floors appear on it *as visited*,
   changing only while unobserved (the map literally draws itself behind your back). One-way
   connections are drawn **as arrows** — fixing the single most-documented flaw of Antichamber's
   map, and honestly declaring the elevator ring's direction.
2. **Floor selection is a knowledge gate** (GDD 4.5.1's proposal, scheduled): buttons stay inert
   through Acts I–II (the ring *is* the dream's current). Each button lights the first time the
   ring delivers the player to its floor; **full selection unlocks after first arrival at
   ARCHIVE** — the moment the building admits it is a building. One hard ordering constraint
   the ring must enforce: ARCHIVE before STAIRWELL (the Stairwell's reverse-direction rescue
   leans on grammar the Spiral teaches).
3. **The Ninth Hook is the exit-behind-glass.** Antichamber shows its exit door from the first
   corridor; DayDreams shows the RESERVED hook (sign S10) among the eight portraits in the first
   ten minutes. The player will not know what it is for until Act IV. Cost: zero — it is set
   dressing already scripted by GDD 5.10.

**Signs are the corporate voice, double-coded.** The GDD's S1–S10 signage already lies
beautifully ("IN CASE OF FIRE USE THE STAIRS" in a game with no stairs). Extend the register:
every sign is simultaneously bureaucratic furniture *and* a literal mechanical hint, placed —
per Antichamber — at the **end** of the room that teaches the rule, generalizing what the player
just did. New signs proposed in the sheets below follow the ≤40-char, lying-corporate register.

**Knowledge-gate inventory** (what a returning player can skip, which is what makes replay feel
like lucid dreaming): the anamorphic station points, the mirror's budget, the Spiral's lap
arithmetic, the Stairwell's runner-carpet rule, the turn-around corridors. Item gates stay
few and physical (the key, the window) per GDD 5.8's taxonomy — and per Antichamber's reception:
its item-tier half is the half nobody loved.

**Act structure — the amended 5.5 table.** GDD 5.5 demands act changes happen in its table
first, so here is the amendment stated as a table, not slipped in sideways. The one-rule-per-act
law is kept, with one declared exception and one clarification: *composition levels* (which
recombine taught rules without introducing any) may appear in any act, and flat placement folds
into Act I where the 5.6 script already teaches it (the window beat at 6:00).

| Act | Levels | New rule introduced | Spaces |
|---|---|---|---|
| 0 La postal | **2** (Apartment · Meadow crossing) | the threshold: crossing is irreversible (look-away seeded, not named) | Sheet 7 + Intro (15) |
| I La invitación | 3 | scale = distance (grab; window/flat placement per the 5.6 script) | Backrooms (16) + GDD sheets 1–3 |
| II El edificio tiene pisos | 3 (The Deep End · Show Home · Copy Room) | **the doorway is an object**: a frame you own (Sheet 10), a door with a property (Sheet 8's Doubling Door) | Pool Rooms (17) + authored floors |
| III Los pisos que no caben | **4**, 50–60 min (Nobody Watching · The Spiral · The Stairwell · anamorphic gallery floors) | **attention decides** (observation + anamorphosis, per the GDD). The Spiral and the Stairwell are declared *composition* levels — scale×connection and connection×orientation; the Turn is a portal property like width, not a new verb | Overgrown (18) + Sheets 9, 11 + scenes 10–12 |
| IV El noveno marco | 2 | none — composition only (now specified, §3.6) | return to Backrooms |

Act III grows by one level and ~15 min over the GDD's budget — that is the explicit amendment,
traded against the retirement of scene 13 from its floor list. The asset reset (§6) also
loosens 5.5's space assignments: authored floors replace "sisters of the scan" wherever a sheet
needs its own architecture. Two further insertions:

- **Act 0 gains a prologue room** before the meadow: the Apartment (Sheet 7) — the reality
  anchor the dream needs to be measured against.
- **Act interstitials: false awakenings.** Twice in the campaign (end of Act II, mid Act III),
  the elevator opens onto the Apartment corridor instead of the requested floor. Everything is
  as it was, minus one defect per visit (LSD Dream Emulator's accumulating corruption; Perfect
  Blue's doubling — the second visit's bathroom mirror shows the room *without* the things you
  carry). Mirror constraint, stated once for the whole pack: a portal cannot produce a
  chirality-flipped reflection (warps are rigid; a mirrored copy would draw inside-out), so
  every "mirror" is a **180°-rotated near-symmetric duplicate room** — no text, no chiral props
  inside the mirrored volume, and the duplicate room is a budgeted asset per mirror. Cost: the
  Apartment is one small scene, reloaded with a variant flag, plus one dressed duplicate
  bathroom.

---

## 3. The level sheets

Format follows GDD 7.5. Scene files continue the GDD's reservations (sheets 1–6 claim
`src/level19..24.rs`), so this pack claims `src/level25..29.rs`. All captures follow the house
rules: `--windowed --mute --no-gamepad`.

How the requested scale trio maps to sheets, so nothing reads as missing: **portal
enlargement** → Sheet 10 (widen-the-opening; the player-built scale portal); **shrinkage and
world enlargement** → Sheet 9 — in this engine they are the same variable (`p_scale` is
physical scale), so enlarging the world *is* shrinking the walker, and Sheet 9 stages its aha
exactly on that ambiguity. Sheet durations follow the GDD's 5–9 min band except the Stairwell
(12 min), a declared centerpiece exemption — with a stated fallback if playtests call it
overloaded (§ Sheet 11, note on 7.2).

---

### SHEET 7 — "The Apartment" (`src/level25.rs`) — Act 0 prologue

**Duration** 3–4 min · **Route** (a) authored geometry, tiny · **Depth** 0.0→0.2

**One question:** *what does it feel like to fall asleep without a cut?*

The game currently begins inside the dream (a meadow that wraps). Yume Nikki's lesson: reality's
tell is that it is **small, fixed, and boring** — the dream's tell is that it is vast and cannot
be held still. A three-room studio apartment gives every dream tell a baseline, makes the meadow
*read* as a dream instead of a screensaver, anchors the false awakenings (§2), and gives the
Ninth Frame ending a home to ache for.

**Space.** Bedroom + kitchenette + a 9 m corridor to a bathroom. Night, rain on glass (the only
weather audio in the game at depth 0: dry, close, small reverb). Deliberate mundanity: no long
sightlines — the longest is the 9 m corridor, so the grab verb works but **scale barely does**
(k ≈ p_scale/d stays near flat; a grabbed mug releases mug-sized). Forced perspective isn't
locked out; the room is simply too honest to permit it. *The dream is where there is room to
misplace things.*

**Beats.**
1. *(0:00)* Wake standing (no cutscene; first 5 s nothing moves — GDD 7.2). Grab taught on
   domestic objects: kettle, mug, the die (honest 1–6 here). A framed print over the desk shows
   a grey meadow — empty, no door.
2. *(1:00)* The corridor: walking to the bathroom and back, the corridor silently loops once
   (portal pair, Level1 wiring). Nothing announces it. On the return leg, one thing has changed
   *behind you*: the print now shows the meadow **with a white door** (unobserved swap —
   Antichamber's gentlest instance: a decoration that moved when you looked back).
3. *(1:40)* Second lap (the apartment door now opens onto the same corridor — the loop is the
   only path). Per lap, one defect: the fluorescent hum detunes; the wall clock runs backward;
   a strip of carpet is meadow grass (Surface::SplitX mid-corridor — feet hear it first).
4. *(2:30)* Third lap: the corridor's far end is **open air** — the meadow, at the corridor's
   own width, widening as you approach (the doorframe is the held channel of the match cut;
   grade splits at the threshold like the title door). Crossing fires the vanish idiom
   (`DoorLink::vanish` pattern): looking back, a bare hillside. The apartment is gone.
5. Meadow → white door → Backrooms: **unchanged** (the existing intro is correct).

**Teaches without text:** grab; the look-away rule (print swap); the loop-as-space rule
(corridor); the one-way rule (vanish) — all four before the first puzzle, all failure-free.

**Declared 7.2 deviation:** the exit is not visible from arrival as geometry — it is visible
as *image*: the framed meadow print over the desk IS the exit, shown from beat 1 and updated
by beat 2. The level's whole point is that the picture becomes the place.

**Intro delta (scene 16) — the only changes to the existing intro, collected here so Sheet 7
alone is actionable:** (1) the die on the carpet uses the seven-pip dream atlas (§1.1);
(2) the ninth hook with its `RESERVED` sign is visible among the eight portraits (§2.3);
(3) dream-depth 0.5→0.65 channels active (§1); (4) nothing else in the first ten minutes is
touched — the GDD 5.6 teaching script stands as written.

**Props:** kettle, mug, die (totem baseline), framed print (2 cutout states), wall clock
(reversed hands = texture anim). **New assets:** one small interior (≤20 k tris), 1 print atlas.
**Audio:** rain loop, detuning hum (2 variants), existing footstep sets.

**Done when:** the loop is undetectable on lap 1 with `--forward` at all four yaws; the print
swap can never be photographed mid-change (cone test per GDD 3.9); crossing to the meadow is
seamless (no load hitch — meadow is the same scene, apartment at a far offset like FAR2).

**Capture:** `--scene 25 --pos 0,1.5,3 --yaw 0 --forward --frames 900 --shot sleep.bmp`

---

### SHEET 8 — "The Copy Room" (`src/level26.rs`) — Act II — CLONING

**Duration** 8 min · **Route** (b) over the Backrooms scan (a "Records" sister floor) ·
**Depth** 0.7 · **Elevator label** `RECORDS`

**One question:** *if you let go of something at a doorway, how many of it are there?*

**The mechanic — the Doubling Door.** No new input, no new verb. A special portal pair where a
**loose** object crossing leaves a copy behind at its entry point. It rides two rules the engine
already enforces:

- Held objects are cargo — `grab` pins `prev_pos = pos` every frame, so a carried object never
  triggers `try_portal`. **Carrying through a Doubling Door does not clone.**
- Loose simple props are travelers — they cross on their own (they roll down ramps per GDD 3.6;
  release restores gravity), and the crossing is detectable by a `RoomLogic` watching the
  portal plane.

Implementation: a `CloneLink` RoomLogic per door with a stated contract (this contract is the
design — an implementer should copy it verbatim):

- On detected crossing of a **whitelisted** prop, `room::request_spawn` a factory-built copy at
  the entry-side homologous point (`target.x − far + near`, the Level10 remapping), on **flat
  ground** (never on the intake ramp), carrying a *refractory* flag — a spawned copy never
  re-triggers the door that made it, so no cascade is possible. Whitelisting means inventory
  props and puzzle keys from other rooms cannot spend a door's budget by accident.
- The **traveler** is what the warp touches (`physical.rs:116`): the object that crossed keeps
  going, rescaled if the pair's widths differ; the copy appears at the entry side at the
  pre-crossing state. Puzzles below exploit exactly this asymmetry.
- **Cap: the door grants four**, per scene load (RESTART LEVEL resets it — GDD 5.6's law that
  no failure closes a door stays intact). Each copy raises the portal's tint alpha one step
  (the glass dims — HUD-free state, GDD 3.1.6's grammar); at four it goes dark and **stops
  cloning but never stops passage** — `tint` and `passable` are independent flags, and an
  exhaustible door is never the sole route (authoring rule).
- Factory = the prop's own data constructor (`Grabbable::new` / `RigidProp::new` specs are
  plain data; meshes/textures are Rc-shared, copies nearly free). *Engine cost: one RoomLogic
  type + per-type factory fns + the refractory flag. No core-engine edits.* Constraint stated
  honestly: rigid props never cross portals (`engine_collision = false`), so clone puzzles use
  **simple grabbables**; the apple/die/king stay decorative here unless the §6 task lands.
  Sheet prerequisite: the known `try_portal`-skips-`on_rescale` bug fix (§6) — beat 3's
  rescaled traveler must be physically real, not cosmetic.

**Reading (dream logic).** The Doubling Doors are full-length **mirrors** — a mirror is the one
doorway everyone has seen a copy in. Per the pack-wide mirror constraint (§2): each mirror is a
180°-rotated, near-symmetric **dressed duplicate room** (one budgeted per mirror — a portal
shows geometry, not a reflection, and cannot flip chirality), which lands the intended wrongness
anyway: the room in the glass is *almost* the reflection, and it never contains you (the player
renders in no pass). Perfect Blue's double, inverted. In the Records floor, all eight portraits
are the *same sitter*.

**Beats.**
1. *Vestibule — teach.* A mirror at the end of a short intake ramp; one map tube (a paper
   cylinder — a prop whose slide reads honestly, since simple Physicals slide without angular
   integration) on a stand; two floor plates (collider + RoomLogic) behind you, and the exit
   door they open. Release the tube on the ramp → it slides through the mirror and keeps going
   on the far side → **a second tube now rests at the foot of the ramp, on the flat** (the
   CloneLink spawn — refractory, so it stays put). Two tubes, two plates. Sign at the exit
   (end-of-room, per Antichamber): `THE ORIGINAL ANSWERS FOR ITS COPIES`.
2. *Development — the budget.* Four plates, one tube, one mirror with four lives. The player
   watches the glass dim per copy and prices their mistakes (a wasted clone can be re-rolled —
   plates accept any of them; failing is free, the budget just gets tight enough to feel; a
   spent mirror still opens like any door).
3. *Development — carry vs release.* The exit key hangs beyond a mirror corridor. Carried
   through: one key, works (cargo rule — the mirror can't touch what you hold). Rolled through,
   out of curiosity: two keys — and the asymmetry lands the lesson exactly backwards from
   intuition: **the traveler is what the warp rescales** (`physical.rs:116`), so the key that
   slid through arrives at the pair's 0.5 ratio — half-size, and it no longer fits the lock
   (hint at the lock, diagnosis: `TOO SMALL FOR THIS LOCK`). The full-size **copy** waits back
   at the entry side and must be **carried** through — the cargo rule is the rescue, re-taught
   at the moment it matters. Nothing soft-locks. One room braids cloning, cargo, and
   scale-through-portals — the Compound thesis restated by a new door. (Needs the lock-side
   scale tolerance, §6.)
4. *Spectacle.* The records hall: two facing Doubling Doors at the ends of one aisle — the
   deliberate infinite-mirror corridor (4 recursion levels; the GDD flags this exact shot as
   unexercised and the worst cost case, so it is measured, not assumed — see Done-when). The
   recursion cap cannot be fogged (`pink.frag` is flat and fog-free): the doors' rising tint
   alpha is the dressing — the deepest visible level fades into the glass color before the cap,
   the dream's edge drawn by the budget grammar itself.
5. *Closing.* "The census": four identical weights on four scales calibrated to one size —
   the mirror grants exactly four, but the room's release sightlines are wrong for the needed
   scale; the player must clone first, then re-scale each copy by grab (clones are ordinary
   grabbables). Composition gate: cloning × forced perspective.

**Props:** map tubes (simple Grabbable, new mesh ≤500 tris), plate markers, key (existing
system + the §6 lock-side scale tolerance — one lock in the scene, respecting the one-shot
unlock channel). **Audio:** a *paper-flutter* clone sfx (gen_sfx), the dimming door needs no
sound — its glass darkening is the message.

**Done when:** clone count can never exceed cap and a spawned copy never re-triggers its own
door (tests); a clone in the inventory survives scene load like any object; the half-scale
traveler key visibly fails at the lock (`TOO SMALL FOR THIS LOCK`) and its full-size copy
carried through succeeds; the facing-mirrors aisle is measured four-yaw `--no-vsync` and stays
under the 10 ms worst-shot cap with the tint dressing verified by shot; no puzzle requires
stowing (Pillar 3 / gamepad risk R6 — pads have no inventory bindings).

**Capture:** `--scene 26 --pos 0,1.5,4 --yaw 0 --e-at 120 --frames 600 --shot mirror.bmp`

---

### SHEET 9 — "The Spiral" (`src/level27.rs`) — Act III — WORLD SCALE

**Duration** 9 min · **Route** (a) Floorplan-derived ring + authored dressing ·
**Depth** 0.85 · **Elevator label** `ARCHIVE`

**One question:** *if the loop shrinks you, what grew?*

Executes the GDD's own untried combination 3 (3.11) as a full level: a Floorplan-style ring in
which **one** portal pair has a 0.5 width ratio. Each lap halves the player; the same corridor
walked four times is a cathedral. Two design deviations from the raw combo, both mandated by
its stated danger (nothing bounds player scale):

- **The ring is bidirectional** (`connect`, not `connect_warps`): the same doorway crossed the
  other way is ×2. The rescue mouth is not a place, it is *the direction you came from* — walk
  the ring backwards to surface. (Reverse laps are also the second half of the puzzle grammar.)
- **Binary factors only** (0.5/2.0 exact per GDD 3.4); the level's usable scale band is
  1.0 → 0.125, enforced by which openings admit which eye heights (the 0.6×1.0 mouth
  grammar — a scale *floor* built from level geometry, not code).

**Beats.**
1. *Vestibule — the measuring stick.* Before the ring: one doorway crossed **both ways** in
   sight of a fixed-size authored reference (Level5's `tunnel3` idiom — here, a fire
   extinguisher on a hook). Through: the extinguisher towers. Back: normal. The symmetry
   (reverse crossing = ×2, i.e. the way home) is learned *safely, in one glance*, before it is
   ever the rescue. One extinguisher then remains visible from everywhere on the ring, so
   current scale is a look, not a lap-count — and each quadrant of shelving carries one defect
   (a fallen folder, a wrong-color drawer) for wayfinding, per the §1 repetition rule.
2. *Development — the loop, then the mousehole.* Classic Floorplan counter-mechanics: drop a
   prop, walk "straight", find it again — the third encounter it is *bigger than you remember*.
   No: **you** changed; dropped props are absolute (the world didn't grow; the doorframes say
   otherwise). Then the exit: a knee-high arch (passable below p_scale 0.66, the built-in
   gate). Beyond it, the *real* archive — a filing-cabinet canyon only a small player can
   enter. What shrinking buys is **access**, not new grab math: the keyhole sits at the end of
   a canyon whose walls a 1.0-scale body cannot pass, and the giant-door key must be released
   from *inside* it, where the release sightline exists at all. Shrinking yourself is how you
   enlarge the world's usable space.
3. *Development — cargo across laps.* A carried object ignores the ring (cargo never crosses).
   The die carried down two laps is now a boulder relative to you — set it down and it is a
   table. One object, many scales, many uses (bridge at 10×, key at 0.1× — the Superliminal
   grammar executed through the player's own size).
4. *Spectacle.* At 0.125, the office doorway is a nave. Dust motes (doorlight's GL_POINTS) hang
   at eye level like snow that forgot to fall — and a small prop knocked from a shelf falls
   *slowly* (gravity scales with the prop's own p_scale: a 0.125-scale folder takes ~2.8× as
   long from the same height; full-scale props fall exactly as ever, which is its own wrongness
   here). Nothing to solve. `THE ARCHIVE THANKS YOU FOR YOUR PATIENCE`.
5. *Closing.* Surface by walking the ring in reverse — the vestibule taught the way home — and
   the honest exit demands **exact change**, enforced by two real colliders, not a fictional
   check: the cab door has a **0.19 m threshold step** (mounts at p_scale 1.0, a wall below
   ~0.95 — the foot-sphere rule, 0.2 × p_scale) and a **colliding lintel** that excludes 2.0.
   Both failures are diagnoses in geometry. Also honesty about the engine: a scene load resets
   `p_scale` to 1.0 (`player.reset()`), so riding out shrunk would silently cheat the rule —
   the gate exists to keep the fiction diegetic. Glance at the extinguisher; take your laps.
   `PLEASE LEAVE THE ARCHIVE AS YOU FOUND IT`.

**Perf & pacing:** the differently-scaled pair means duplicated corridor geometry per the
200 u idiom — two copies, within budget. Portal count: ring of three doorways (6) + the
fire-exit shaft (4) = **10 of 16**; sightlines elbow at each quadrant so ≤2 portals are ever
co-visible and the recursion chain dies off-screen (the level8 lesson, applied). Wall-clock
law: walk speed scales down with you, so laps compound — budget **≤20 s per lap at scale 1**,
give small players knee-high mousehole shortcuts that cut lap length (the 0.6×1.0 grammar,
usable only when small), and provide a **fire-exit shaft**: two chained ×2 doorways in short
segments so surfacing from 0.125 is ~30 s, never seven shrunk-speed laps.

**Done when:** every reachable scale has a walkable route back to 1.0 (four-yaw `--forward`
sweep at each binary scale); `POS.y == 1.5 × p_scale` invariant asserted at each checkpoint
(GDD 3.4.6 table extended); no non-binary p_scale ever observable (test); a ride can only
begin at p_scale 1.0 (test on the threshold gate); total forced walking at any solve path
under 3 min.

**Capture:** `--scene 27 --p-scale 0.25 --pos 0,0.375,6 --yaw 0 --frames 300 --shot nave.bmp`

---

### SHEET 10 — "Show Home" (`src/level28.rs`) — Act II — PLAYER-BUILT PORTALS

**Duration** 8 min · **Route** (a) + one small diorama interior · **Depth** 0.9 ·
**Elevator label** `SHOW HOME`

**One question:** *what happens when the window maker gives you both frames?*

Executes GDD 4.3.1's "second portable frame" proposal as its own level — the grab becomes a
**topology editor**. The estate-agent floor: a furnished show home whose centerpiece is a
dollhouse on a table — a 1:12 copy of the show home itself (a real interior 200 u away; the
dollhouse's facade window is a `passable=false` portal into it, tinted like display glass).

Frame A hangs locked on the lobby wall (the familiar window). Frame B leans beside the
dollhouse, grabbable — the dollhouse facade, the level's destination, is in view from arrival
(the 7.2 exit rule, satisfied by image). Honesty about `window.rs`: the *mechanisms* generalize
(per-step reconnect, size gate, park-when-flat), but the destination wiring is hardcoded
(FAR2/PARTNER constants, `level18::CEILING`) — extracting it is part of the §6 M-sized task,
along with the unlock channel's merge rule (Cell → map-by-id, the GDD's own prerequisite), the
explicit frame-through-frame rejection (`park + hint`, the window's own precedent), and one
rule the small-frame beats require: **passability gates on fit-at-exit** (opening height ≥
`PASS_HEIGHT` × *arrival-side* player scale), not on the absolute 1.9 m — otherwise a
dollhouse-scale doorway could never admit even a dollhouse-scale walker.

**Beats.**
1. *Vestibule.* Frame B placed on any wall opens onto wherever Frame A looks. Widen-the-
   opening: the lobby is 2 m deep — a 2 m room cannot make a door (max scale needs step-back
   room; GDD's numbers). Carry B to the hall, enlarge, return through it. Room depth is the
   resource; the player learns it with their feet.
2. *Development — the ceiling antagonist.* The destination caps the size (`max_scale` from the
   *far* ceiling): to make a walk-through door into the low mezzanine, B must be placed where
   the mezzanine's ceiling allows it — the puzzle is on the far side of the glass.
3. *Development — your own scaling tunnel.* Frame B carries its own `p_scale` (it is a prop).
   Held small and placed against the dollhouse facade, then A enlarged on the lobby wall:
   the pair's end-to-end ratio is a hand-made scale portal (exactly scene 5's tunnel, built by
   the player — scene 9's composition, authored live). Step through A into the dollhouse
   interior **at dollhouse scale**.
4. *Spectacle (the GDD's own proposed impact shot, delivered).* Inside the dollhouse, its
   little window looks back out at the show home — where a mug you left on the table is a
   water tower on the horizon. The final room seen *from inside an object the player resized.*
5. *Closing.* The exit elevator is dollhouse-sized: it only resolves as rideable from in here
   at this scale (`CAPACITY: 9 PERSONS` never lied — it just never said which size of person).
   The ride's scene load resets `p_scale` to 1.0 (`player.reset()`), so the elevator is also
   the game's **normalizing doorway** — Superliminal needed to invent one; this engine ships
   it. State the rule here once: *every elevator ride delivers you at scale 1.0.*

**Done when:** frame-through-frame is rejected with a hint, never a crash; both frames re-park
correctly when floored; a small-scale player can pass a proportionally small opening (fit-at-
exit test) and a full-scale player still cannot pass a 0.3 m window; the dollhouse crossing
hand-off follows the FAR2/CROSSING one-way idiom with no return window; unlock channel
converted to map-by-id **before** this level is written (GDD 4.9's stated rule). Portal count:
frame pair (2) + dollhouse display window (2) = 4 of 16.

**Capture:** `--scene 28 --window-scale 4 --pos 0,1.5,5 --yaw 20 --frames 400 --shot dollhouse.bmp`

---

### SHEET 11 — "The Stairwell" (`src/level29.rs`) — Act III centerpiece — the ESCHER REPLACEMENT

**Duration** 12 min · **Route** (a) authored, one sculpture asset rebuilt · **Depth** 0.95 ·
**Elevator label** `STAIRWELL` (the name bible already reserves "The Stairwell / La Escalera")

**One question:** *which way is the floor?*

Retires scene 13 (see §6) and replaces its *concept*: not a walkable sculpture, but a building
where **the vertical is negotiable at doorways**. Three movements plus a coda, each one an
implemented-primitive assembly.

*Declared 7.2 exemption:* the level carries one question but two mechanic families (the Turns,
the blend). Justification: the blend's pieces are each taught before they are load-bearing
(runner rule and one optional yielding balustrade in Movement I — a prop visibly parked
*behind* a balustrade that gives when touched — then Turns in Movement II), so Movement III is
composition, not a second introduction. Fallback if playtests disagree: Movement III spins off
as its own Act III level with no redesign — the movements are already self-contained.

**Movement I — the Gallery (arrival, read the route).** An atrium; at its center, the
Relativity sculpture — rebuilt authored at ≤15 k tris (the 660 k/47 MB Sketchfab mesh does not
survive multiplication and is retired with the level). Around it, a gallery ring: *la galería
como mapa* — the whole route is legible from the floor before any of it can be walked. At the
entrance, the game's best sign finally lands: `IN CASE OF FIRE USE THE STAIRS` (S4) — this is
the only level with stairs, and none of them are stairs.

**Movement II — the Turns (rotated-world doors).** Four landings connect through doorways to
**pre-rotated copies** of the atrium (90° about the corridor axis, at 200 u offsets, the FAR2
idiom; `interior`/object placement takes the rotation — `trimesh.rs` bakes it). Rules that make
it work, verified against the code:

- Both portal quads upright in their own worlds; the yaw-only re-aim is then *exactly correct*
  (`physical.rs:111-113`). Gravity, camera, and ground contact never change — the **world**
  turned, not you.
- The doorway on the far side is carved where the base architecture has a *floor hatch* — a
  hole that is a wall opening after rotation. One constraint this creates, planned for rather
  than discovered: every portal-bearing opening exists in **all four orientations**, so in the
  orientations where it is a floor pit or ceiling gap it needs a diegetic cover (a grate, a
  rope barrier, or the net room the Done-when already plans as a fall catch).
- What the crossing actually delivers — stated honestly, because the naive version is
  geometrically impossible (yaw-only warps, roll-less camera, seamless portals mean nothing
  "rights" or "tilts" at the plane): looking through a Turn door, the room beyond visibly lies
  on its side (furniture bolted to what reads as wall). Crossing puts you **inside** that
  sideways room, standing normally on its wall-as-floor; the look-back shows the gallery you
  left *level with your feet* but 90° off its own architecture — a doorway standing in what
  the local walls insist is the floor. The wrongness is sold by the copy's decor and the EXIT
  compass, not by a camera event — which is exactly why it reads as dream, not transition.
  The first look-back is structurally forced, not hoped for: landing 1 is a shallow dead-end
  alcove whose only exit faces the Turn door — turning around *is* the route.
- Per copy, `tools/gen_walkable.py` bakes a collision shell **on that orientation's up-facing
  surfaces** — so a different third of the sculpture is walkable from each Turn. Level 13's
  structural lie ("only one of three gravities is real") becomes the level's whole truth:
  across three Turns the player walks all three gravity systems of the print, one per world.
- Props dropped in one copy rest on *that copy's* floor and exist only there — so through any
  single Turn door, the previous copy's dropped props are seen resting "on the wall". One
  look-back view per prop, no cross-copy state mirroring (rotated copies stay stateless — the
  hazard the prior-art research flags).
- One red `EXIT` sign hangs in the atrium, present in every copy, rotating with each Turn —
  the persistent orientation compass (4D Golf's lesson). Its arc across the level is also its
  lie detector: when the player is "upside down" relative to entry, the sign is too.

**Movement III — the Blend (the walkable path that shouldn't be).** The user-facing core:
*a path that reads walkable, blends through stairs and walls, and ends somewhere standing
shouldn't be possible.* Assembly, all existing primitives:

- **Stairs are ramps in costume** (riser < 0.2 × p_scale visual dressing — the Relativity
  recipe, now deliberate). **False stairs are real stairs** — risers ≥ 0.25 block. Visually
  near-identical; the honest tell is the **runner carpet**: where the runner runs, the ramp is
  real. Taught silently in Movement I (three chances), load-bearing by Movement III.
- **Walls that yield**: sections of balustrade and wall along the true path are collider-less
  visible geometry (`c`-lines simply absent — the `solid_triangles` idiom foliage already
  uses). The runner dives *into* the wall; walking after it passes through. Antichamber's law
  respected: every blended path is discoverable **by touch** — a stuck player brute-forcing
  walls finds it; the runner just makes it findable by sight first.
- **Paths that end on the impossible**: a runner climbs a ramp-dressed-as-stairs, blends
  through a wall — and emerges in a rotated copy (the wall was a Turn without a doorframe:
  a frameless portal, its free edge hidden in a newel post per the 3.2.4 grammar). The player
  who follows the runner is now standing on what the gallery calls a wall, without having
  noticed a single doorway. *That* is the Escher sensation the broken levels never produced —
  and it is made of three shipped primitives glued by one carpet texture.
- **The fire stairs** (connective tissue between landings): the Penrose loop done right, with
  the wiring stated so nobody rebuilds level8's mistakes — a **real, contiguous square helix
  of four flights** (90° yaw each; genuine geometry, no per-flight copies) whose bottom
  connects to its top through **one one-way pair** (`connect_warps`, bottom→top; summed yaw
  = identity). Walking down is infinite (the single warp recycles you); sightlines break at
  every corner so the recursion chain dies unseen (what level8's collinear flights could
  never do); and because the reverse path crosses **no portal**, simply turning around and
  climbing surfaces in at most three flights. The tell: the `EXIT →` arrows accumulate
  clockwise — following them *backwards* is the way out. Direction as input (Antichamber);
  the arrows arc from trusted to liar and back to honest, once, in this one stairwell.

**Coda — the underside.** The last Turn lands in the **180° copy**: the player stands on the
atrium's coffered ceiling, and *within this one copy* the whole gallery hangs overhead —
benches bolted upward, the sculpture between, the print inhabited at last (no cross-copy views
needed; the inversion is this copy's own architecture). Through the Turn door behind them, one
framed look-back at the upright world they left. The exit elevator is placed in this copy as a
separately-oriented upright object — the one thing in the room that agrees with the player
about which way is down, which is exactly why it reads as the way home. Ride out (`p_scale`
and orientation debts all settle: the ride is the normalizing doorway).
`THANK YOU FOR VISITING. MIND THE STEP.`

**Perf honesty:** portal inventory, counted against the hard cap of 16: four Turn doors (8) +
one frameless Turn (2) + the fire-stairs' single one-way pair (2) = **12 of 16**, leaving
headroom for a net-room return door. Geometry: four atrium copies at ≤15 k tris each + shells
≈ 60 k solid triangles — inside the 70 k collision budget; portal co-visibility capped at 2 by
elbowed landings; load under 300 ms if copies share one parse (the level16 two-load trick).

**Done when:** every Turn's look-back is verified by shot at all four landings, and landing 1's
alcove makes the first look-back unavoidable (route test, not just shot test); no walkable
route dead-ends at unwalkable scale/orientation (four-yaw sweep per copy); runner-carpet rule
has zero exceptions before Movement III's blend; the frameless Turn's free edge is occluded
floor-to-ceiling (3.2.4 checklist); every floor-pit orientation of every portal opening is
covered or netted; fall from any ledge lands in a lower copy's net room and respawn never
fires during intended traversal.

**Capture:** `--scene 29 --pos 0,1.5,8 --yaw 0 --frames 400 --shot lookback.bmp` (landing 1,
facing the first Turn's reverse view).

---

### 3.6 Act IV composition note — where dream and reality finally blend

Act IV stays two levels and introduces nothing (the GDD's law) — but "pure recombination"
deserves its shot list, because this act is the Paprika of the game. Half a page, not sheets:

**IV-1 — "Return to the Ground Floor."** The Backrooms hall again — Act I's teaching space,
now at depth 1.0, rebuilt as **parallel slices** (catalog #7): each pass down the hall is a
permutation of itself (doorways miscounted, the portraits' sitters exchanged, the window
looking onto the *meadow* instead of Overgrown). The attention verbs cash out here: one
passage opens only under sustained gaze (#1), one bridge over the stripped floor holds only
from its station volume (#9), and the level's one **static link wall** (#10) — walked through,
it fades warm — surfaces the player into the Apartment corridor for the last false awakening.
There, the totem's single permitted lie (§1.1): the die rolls an honest four. The corridor is
NOT reality. The player, trained for two acts, feels the floor drop without one changed pixel.
This is Kon's contamination arc executed in mechanics: the tells decay only after they were
trusted.

**IV-2 — "The Ninth Frame."** The GDD 5.10 ending as specified — the inverted size gate, the
stow, the hanging of the frame on the ninth hook — plus this pack's one addition: at the hang,
the dial performs its only downward snap (1.0 → 0.15, §1), the sole moment the player *feels*
the depth axis exist, because the game is ending and the rule may finally show itself.
Credits over the live meadow, door open, nobody crossing.

---

## 4. New mechanics catalog (beyond the sheets)

Each entry: recipe against the real engine, cost, and one puzzle seed. These are the expansion
reservoir for Act III/IV rooms and the level editor's future vocabulary — every one lives in
topology or attention, none needs curved space, and none breaks the verticality law.

| # | Mechanic | Recipe (engine-true) | Cost | Puzzle seed |
|---|---|---|---|---|
| 1 | **Eye wall** (stare to dissolve) | Inverse of `is_observed` (the observed-movement family GDD 3.9.1/3.9.2 already proposes, generalized): dwell timer while in cone + LOS; at threshold, portal turns passable / object `request_remove`. Same code, condition flipped. | S | A wall with a painted eye that blinks as your gaze holds; the exit behind it. The one mechanic where *looking* is the verb — pairs against statues in one room: stare here, and they walk. |
| 2 | **Gaze-held door** | `Watch` verbatim: state A while observed, relaxes when not (GDD 3.9.2's "La sala que respira" / 3.9.1's "El interruptor vigilado", credited and generalized). | 0 | "Some doors close unless we hold them open": retreat backwards keeping eyes on it (backpedal is walk-speed — sprint's forward-only rule becomes level design). |
| 3 | **Direction-dependent corridor** | Two one-way pairs (`connect_warps`) on one doorway, per-face destinations (Floorplan's p1 proves per-face works). | 0 | The hallway to the lift is a different hallway on the way back. Taught with the Apartment's loop; weaponized in Act III. |
| 4 | **Turn-around reveal** | Unobserved swap of the corridor behind the player (portrait clocks: GRACE 0.4 s). | 0 | The dead-end that is only a dead end while you approach it. |
| 5 | **Four-faced cabinet** | One box, four outward portal quads, four diorama rooms at separate far offsets (the FAR2 pattern — this renderer has no visibility cells, so overlapping rooms would draw and collide simultaneously). One face passable. | S | Antichamber's cube gallery as a wardrobe: each face shows another floor's room; one admits you. A Directory in furniture form. |
| 6 | **Holonomy loop** | Ring corridor of yaw-rotation portal pairs summing to 90°: walk a "straight square", come home rotated — the window is where the door was. | 0 | The room you re-enter has turned; the desk's anamorphic smear now aligns from the entrance. Dose per Hyperbolica: once is wonder, every corridor is nausea. |
| 7 | **Parallel slices (4D fake)** | N copies of one room, walls differing; passage between homologous points via a specific doorway (or unobserved permutation swap à la MyHouse). | S | The locked cabinet is open two slices over; through-wall traversal reads as 4D (Miegakure's discrete cousin). Elevator mis-delivery = slice shift. |
| 8 | **Scale fling** | Velocity maps through the warp, so a ×2 portal doubles a traveler's exit speed. Needs the known `on_rescale` bug fixed + opt-in portal crossing for select props (§6). | M | A gap no throw can cross — until the throw goes through the shrink door and back out the grow door. The one *kinetic* scale toy; Portal never had it. |
| 9 | **Alignment-gated bridge** | `passable` driven by a RoomLogic testing player position/view volume (station-point tube from 3.8.5). Monument Valley in first person: the connection exists only while seen right. | S | The GDD's own "Cerradura de perspectiva" (Level11) promoted to architecture: a footbridge that holds while the chandelier's fragments align — cross it backwards, holding the view. |
| 10 | **Static link walls** | One ordinary surface per level is a silent portal (LSD's linking, static destination). Tint-fade as the link grammar. | 0 | The one wall in each act that was never a wall; finding them is the optional connoisseur layer, and they draw as edges on the Directory. |
| 11 | **Room permutations** | 2–3 variants of a key room swapped between visits, strictly unobserved (MyHouse). | S | The Records floor's aisle count disagrees with its own signage on revisit. Never on the critical path — pure dread seasoning. |
| 12 | **Failing-forward bridge** | Per-room attempt counter; each re-entry extends the bridge by one plank (spawned, unobserved). **Conflicts with GDD 5.9's no-automatic-aid law** — legal only as an optional, non-critical-path gag room, never as a gate. | S | An optional balcony over the Pool Rooms' deep end that persistence alone spans, leading to a view and nothing else. |

Cost key: **0** = wiring of shipped systems · **S** = small (a RoomLogic / a flag / an asset) ·
**M** = medium (touches an engine file; listed in §6).

**Explicitly rejected** (and why, so nobody re-litigates): floor/ceiling portals and any player
gravity rotation (verticality law — the Turns exist precisely to make this unnecessary);
true hyperbolic/spherical projection and continuous 4D (projection properties, unreachable by
flat quads; the fakeable payloads are #6, #7 above); Viewfinder photo-stamping (needs runtime
CSG; the authored subset is #5); Antichamber's matter-gun resource tiers (its least-loved half,
and it fights the one-verb pillar); jump-timing and reflex gates (contract violation);
Esc-to-hub teleport (camera never cuts).

---

## 5. Multiplayer, sandbox, editor — the honest roadmap

Research verdict (Portal 2 commentary, PeTI/Workshop, Trackmania, Mario Maker, Group Therapy,
Escape Simulator, Splitgate, Hypermine): for a solo-author Rust engine, the order is fixed by
prerequisites, and the early phases are worth shipping on their own.

**P0 — Scenes as data + the challenge layer** *(prerequisite for everything; useful alone).*
Migrate level definitions toward a serializable description (rooms, portal links with per-face
destinations and widths, props, spawn, exit, floor rows) — route (b) levels are already "a Load
+ a placement + constants". Then the free win from the CLI's determinism: **par time / par
grabs / par jumps** per scene with letter grades (Superliminal's Challenge Mode is exactly this
bolted onto nine scenes; the input-script flags already measure it). Replayability for the
15 sandbox scenes at near-zero cost.

**P1 — Asynchronous multiplayer: ghosts + codes** *(zero netcode).* Record input/transform
streams (the harness already replays inputs deterministically); render other players as ghosts.
Author-medal validation à la Trackmania: publishing a time requires the run to replay clean.
Share levels/runs as short codes (Baba Is You's 9-symbol pattern) + a hand-curated featured
list. Twenty years of Trackmania prove ghosts are a community, not a compromise.

**P2 — The Dream Workshop (editor).** Constrained vocabulary or nothing (PeTI's lesson):
place room modules on a grid, connect **any door to any door** — loops, one-ways, width ratios
(scale!), per-face destinations. Non-euclidean connection *is* the signature toy no mainstream
editor offers, and it is this engine's native primitive. Live lint = the GDD's own tables
(3.10.2 hard rules, 3.10.5 symptom→cause) turned into editor errors: verticality, eye-height
coverage, ≥0.8 widths, connect-after-place, co-visible-portal budget. **Publish gate = Clear
Check** (Mario Maker): the author must complete their own level; their time becomes the par
medal (Trackmania's three-birds mechanic). Frame it diegetically: the Workshop is a floor of
the building; custom levels are "dreams" (Superliminal's internal UGC name, as it happens).

**P3 — Realtime: the race, then hide-and-seek.** Group Therapy's shape, not Portal 2 co-op's:
players race a shuffled gauntlet of *existing* rooms as ghosts with name tags — position/state
replication only, no player-player physics, solo mechanics reused wholesale, "experimental"
label on the tin. The uniquely-fun follow-up in impossible space is **hide-and-seek** (the
geometry structurally favors the hider: bigger-inside rooms, portal ambushes) with convergence
pressure so play doesn't dissolve into solitude. Two hard rules from the research: interest
management and audio must follow the **portal/room graph, never Euclidean distance** (overlapping
interiors leak players through walls otherwise), and per-visible-portal render cost is the
budget that decides player counts. Anamorphosis is per-viewer and breaks under a second
viewpoint — in co-op that is a *feature* (one player calls the figure, the other acts) but it
must be chosen, not stumbled into. Portal-2-grade co-op puzzles (bespoke campaign, ping tools,
sync pings) are a second game's budget; defer until P3's reception argues for them.

---

## 6. Repairs, retirements, and the engine task list

**Repairs.**
- `level8` "Penrose Ascent": move the spawn inside the mouth (the one-line fix the GDD names);
  optionally rotate flights 90° in yaw as a sandbox preview of the Stairwell's fire stairs.
  Keep as sandbox; the campaign version is Sheet 11's Movement III.
- Register `Level2::new(4)` ("Four Rooms" — the honest control house) and `new(5)` per GDD
  3.3.4, so the menu reads 3→4→5→6 as a scale.
- Scene-16 SHOT yaw discrepancy (GDD 3.8.5 PARCIAL note) — verify before any new capture sheet
  depends on scene-16 repro lines.

**Retirement.** Scene 13 "Relativity" leaves the registry when Sheet 11 lands; the 47 MB mesh
and its LFS weight go with it (license attribution follows the asset out of THIRD_PARTY.md).
Its screen name, "The Stairwell", transfers to its successor — the name bible stays intact.

**The asset reset (owner decision, 2026-08-25).** All current 3D models are demo placeholders;
nothing needs preserving. Consequences this pack now assumes:
- Every interior is fair game for an **authored low-poly rebuild** (≤20 k tris per the GDD
  budget): it closes the licensing risks (R2/R3 — unrecorded Sketchfab/door licences) in one
  move, drops loads and LFS weight, and makes carve holes, elevator cuts, and door placements
  authored instead of negotiated with a scan.
- Sheet 11 stops being the exception: rotated copies and per-orientation collision shells are
  the *native* way to build once geometry is authored — model the atrium with its rotations in
  mind (hatch doorways placed where each orientation needs them).
- The liminal look survives the rebuild — the research is explicit that the uncanny lives in
  **lighting-without-source, emptiness, repetition, and ambiguous purpose**, not in scan
  fidelity. Baked-unlit low-poly with the fog/veil dial can read *more* liminal than a scan,
  because every defect becomes a decision.
- Rebuild order follows the campaign: Backrooms hall (Act I canon) → Pool Rooms nave →
  Overgrown → new sheets. Keep the scans in place per level until its authored replacement
  passes the four-yaw capture comparison, then delete asset + attribution together.

**Engine tasks** (each becomes due when its sheet enters production; the build order below
sequences them):

| Task | For | Size | Files |
|---|---|---|---|
| `CloneLink` RoomLogic (whitelist + refractory flag + per-scene-load cap) + prop factory fns | Sheet 8 | S | `ext/` new file |
| Fix `try_portal` not calling `on_rescale` (GDD known bug) | **Sheet 8 prerequisite**; also catalog #8 | M | `src/physical.rs`, `ext/rigid.rs` |
| Lock-side scale tolerance (`accepts_key` gated on key p_scale within ±10%; hint `TOO SMALL FOR THIS LOCK`) | Sheet 8 beat 3 | S | `ext/key.rs` |
| Unlock channel Cell → map-by-id | Sheet 10 (GDD's stated prerequisite) | S | `ext/room.rs`, `ext/key.rs`, `ext/window.rs` |
| Second window frame: extract hardcoded destination (FAR2/PARTNER/`level18::CEILING`), frame-through-frame rejection, **fit-at-exit passability** (opening ≥ PASS_HEIGHT × arrival p_scale) | Sheet 10 | M | `ext/window.rs` |
| Rotated interior placements (yaw-only today → full rotation) | Sheet 11 | S | `ext/interior.rs` (transform chain already supports it; `trimesh.rs` bakes it) |
| `gen_walkable.py --rotate` (bake shells per orientation) | Sheet 11 | S | `tools/` |
| Opt-in portal crossing for chosen rigid props | Catalog #8 (scale fling) | M | `ext/rigid.rs` |
| `dream_depth` uniform plumbed to mood/fog/veil/audio | §1 | S | `ext/view.rs`, `ext/postfx.rs`, `ext/audio.rs` |
| Die totem: seven-pip dream atlas variant (load-time, no runtime swap) | §1.1 | S | atlas + prop spec |
| Eye-wall dwell condition (inverse `is_observed`) | Catalog #1, Act IV | S | `ext/visibility.rs` |
| Directory board (painting-layer floor tiles + one-way arrows) | §2 | M | `ext/painting.rs` reuse |

**Build order** (the answer to "what do I do Monday"):

1. **Repairs** (hours): level8 spawn fix; register Level2 4/5-room variants; verify the
   scene-16 SHOT yaw note. Cheap wins that clean the sandbox shelf.
2. **`dream_depth` plumb + die atlas** — small, touches everything, and immediately makes the
   existing four levels feel like one game; validates §1 before any new level depends on it.
3. **Sheet 7 The Apartment** — smallest new scene; proves the dial, the look-away teach, and
   the match-cut crossing; its variant flag is the false-awakening machinery two acts need.
4. **Sheet 9 The Spiral** — zero new engine systems (level geometry + wiring only); the first
   new *puzzle* level, and the fastest one to playtest.
5. **`CloneLink` + on_rescale fix + lock tolerance → Sheet 8 The Copy Room.**
6. **Unlock-channel migration + window generalization → Sheet 10 Show Home.**
7. **Rotation tasks + authored atrium → Sheet 11 The Stairwell** (largest; by now every
   grammar it composes has shipped in a smaller room).
8. **§5 P0** (scenes-as-data + challenge layer) once the campaign spine above is playable.

**Deliberately not proposed:** dynamic player body / standing on props (the GDD costs it as a
separate project; nothing in this pack assumes it); gameplay postfx as a *requirement* (the
dream dial uses it where the budget allows — it is cut-order item 1 and every level here reads
without it); any new input binding (Pillar 3 holds: E, F, G, R, Space, Shift, and looking).

---

## 7. Playtest protocol (how we'll know it works)

- **Watch faces, don't log metrics** (Bruce): the intro's job is measured in held attention —
  his E3 build held players 5 minutes; shipping held 40–90. Expect to rebuild the first ten
  minutes repeatedly from silent observation.
- **Decompose, then test sub-skills** (Shih): each teaching room owns ONE sub-skill, and the
  list of sub-skills comes from watching failures, not from this document. The Apartment/intro
  beats above are the starting hypothesis, not the answer.
- **The chuckle test** (Fieth): toys and gags between puzzles earn their place by a laugh or a
  head-shake; anything that ejects the player from the fiction gets cut.
- **Wonder rationing** (Antichamber's reception): every third room a new rule-subversion; never
  two logistics rooms in a row; the second half escalates *spatially* (scale, the Turns) rather
  than through resource arithmetic.
- **GDD gates stand:** ≥70% no-help completion, no puzzle >6 min median, first minute textless,
  every action audible, and the VS canonical run unbroken.
