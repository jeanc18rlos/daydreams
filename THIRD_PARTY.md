# Third-party material

Everything DayDreams ships that was not written for it, with its source and licence as
recorded in this repository. Where nothing is recorded, this file says so rather than guessing;
those entries are marked **ACTION REQUIRED** and are collected at the end.

The project's own code is MIT (`Cargo.toml`; the text is the root [`LICENSE`](LICENSE)).
**The copyright holder named there, "DayDreams contributors", is a placeholder** until the
project decides who holds it -- a person, a company or that phrase on purpose. The in-game
Credits screen (`src/ext/menu.rs`, `CREDITS_TEXT`) names the engine, Escher Relativity,
Backrooms VR, the elevator and the two Blenderust rooms.

## Engine: HackerPoet/NonEuclidean

| | |
|---|---|
| What | The rendering engine, ported file-for-file from C++ (`src/*.rs` outside `src/ext/` and `src/level7..16.rs`), the core-profile rewrites of its shaders, and its meshes and textures. |
| Source | <https://github.com/HackerPoet/NonEuclidean> |
| Author | CodeParade, © 2018 |
| Licence | MIT — preserved verbatim as [`LICENSE-ORIGINAL-MIT`](LICENSE-ORIGINAL-MIT). |

Files from the original, byte-for-byte:

- `Meshes/`: `bunny.obj`, `cube.obj`, `double_quad.obj`, `floorplan.obj`, `ground.obj`,
  `ground_slope.obj`, `pillar.obj`, `pillar_room.obj`, `quad.obj`, `square_rooms.obj`,
  `suzanne.obj`, `teapot.obj`, `tunnel.obj`, `tunnel_scale.obj`, `tunnel_slope.obj`.
  Three of these are the classic test models (the Stanford bunny, the Utah teapot and
  Blender's Suzanne); they are shipped here exactly as the original repository distributes
  them under its MIT licence, and that repository records no separate terms for them.
- `Textures/`: `checker_gray.bmp`, `checker_green.bmp`, `floorplan_textures.bmp`, `gold.bmp`,
  `three_room.bmp`, `three_room2.bmp`, `white.bmp`.
- `Shaders/`: `pink`, `portal`, `sky`, `texture`, `texture_array` (`.vert`/`.frag`), rewritten
  for the core profile (README, "Shaders rewritten for the core profile") but the original's
  in substance.

## Escher Relativity — `Meshes/escher_relativity.obj`

| | |
|---|---|
| Source | <https://sketchfab.com/3d-models/escher-relativity-6dccd307edce45f19c032378b4d10933> |
| Author | Benoit Gagnier (<https://sketchfab.com/BenoitGagnier>) |
| Licence | CC-BY-4.0 — [`Meshes/escher_relativity.LICENSE.txt`](Meshes/escher_relativity.LICENSE.txt), which ships beside the mesh. |
| Notes | Converted from the Sketchfab glTF by `tools/convert_gltf.py`; the header of the `.obj` carries the credit. Credited on the in-game Credits screen. |

Required credit, as the licence file gives it: *This work is based on "Escher Relativity"
(<https://sketchfab.com/3d-models/escher-relativity-6dccd307edce45f19c032378b4d10933>) by
Benoit Gagnier (<https://sketchfab.com/BenoitGagnier>) licensed under CC-BY-4.0
(<http://creativecommons.org/licenses/by/4.0/>).*

## Roboto Condensed — `assets/fonts/RobotoCondensed[wght].ttf`

| | |
|---|---|
| Source | The Roboto Project, <https://github.com/googlefonts/roboto-classic> |
| Author | © 2011 The Roboto Project Authors |
| Licence | SIL Open Font License 1.1 — [`assets/fonts/RobotoCondensed.LICENSE.txt`](assets/fonts/RobotoCondensed.LICENSE.txt), which ships beside the font. |
| Notes | `tools/gen_ui.py` rasterises it into `Textures/ui_font.bmp`; that atlas is a derived work of the font and ships under the same licence. The OFL permits bundling with software and forbids selling the font on its own. |

## The portraits — `assets/paintings/src/*.png` → `Textures/portrait_*.bmp`

| | |
|---|---|
| What | The Backrooms' eight portraits: three variation sheets (base + eye and mouth variants) that `tools/gen_portraits.py` cuts into the shipped `Textures/portrait_mona*.bmp`, `portrait_vermeer*.bmp` and `portrait_cavalier*.bmp`. |
| Underlying works | Leonardo da Vinci, *Mona Lisa* (c. 1503–1506); Johannes Vermeer, *Girl with a Pearl Earring* (c. 1665); Frans Hals, *The Laughing Cavalier* (1624) — all public-domain worldwide (the authors died in 1519, 1675 and 1666). |
| The sheets | The project owner's own edits of those works, supplied 2026-08 and recorded as such in `assets/paintings/src/README.md`; the owner contributes them under the project licence. |
| Notes | Faithful reproductions of public-domain paintings carry no new copyright in most jurisdictions; the *edits* (the cutouts, the crops and the blanked base) are the owner's. |

## Backrooms VR — `Meshes/backrooms_vr.glb`

| | |
|---|---|
| Source | <https://sketchfab.com/3d-models/backrooms-vr-d9b98eca8d064d0eafcd7f5484bb61ed> |
| Author | carlcapu9 (<https://sketchfab.com/carlcapu9>) |
| Licence | CC-BY-4.0 — [`Meshes/backrooms_vr.LICENSE.txt`](Meshes/backrooms_vr.LICENSE.txt), which ships beside the mesh. |
| Where recorded | The licence file is a transcription of the GLB's `asset.extras` block, written by Sketchfab's exporter (`Sketchfab-17.15.0`), which carries the title "Backrooms VR", the author, the licence and the source URL above. Credited on the in-game Credits screen. |

Required credit, in the form the Escher file uses: *This work is based on "Backrooms VR"
(<https://sketchfab.com/3d-models/backrooms-vr-d9b98eca8d064d0eafcd7f5484bb61ed>) by carlcapu9
(<https://sketchfab.com/carlcapu9>) licensed under CC-BY-4.0
(<http://creativecommons.org/licenses/by/4.0/>).*

**ACTION REQUIRED:** verify on Sketchfab. The embedded metadata is what Sketchfab wrote at
download time; the page at the source URL is the authority, and the licence file says so.

## Elevator with Animation LOWPOLY — `Meshes/elevator_with_animation_lowpoly.glb`

| | |
|---|---|
| Source | <https://sketchfab.com/3d-models/elevator-with-animation-lowpoly-7c53a9a7db554e9da8dececfd3eac339> |
| Author | EFX (<https://sketchfab.com/evan4129>) |
| Licence | CC-BY-4.0 — [`Meshes/elevator_with_animation_lowpoly.LICENSE.txt`](Meshes/elevator_with_animation_lowpoly.LICENSE.txt), which ships beside the mesh. |
| Where recorded | The licence file is a transcription of the GLB's `asset.extras` block (Sketchfab exporter `Sketchfab-16.68.0`). Shipped as downloaded: 1,054 triangles, nine JPEG/PNG maps, one animation ("Doors open", 10.4 s). Credited on the in-game Credits screen. |

Required credit: *This work is based on "Elevator with Animation LOWPOLY"
(<https://sketchfab.com/3d-models/elevator-with-animation-lowpoly-7c53a9a7db554e9da8dececfd3eac339>)
by EFX (<https://sketchfab.com/evan4129>) licensed under CC-BY-4.0
(<http://creativecommons.org/licenses/by/4.0/>).*

**ACTION REQUIRED:** verify on Sketchfab, as for Backrooms VR above.

## Backrooms Room With Plants (Overgrown) — `Meshes/backrooms_room_with_plants_overgrown.glb`

| | |
|---|---|
| Source | <https://sketchfab.com/3d-models/backrooms-room-with-plants-overgrown-56a9d3e08160479aa287873a13fef455> |
| Author | Blenderust (<https://sketchfab.com/narighillya>) |
| Licence | CC-BY-4.0 — [`Meshes/backrooms_room_with_plants_overgrown.LICENSE.txt`](Meshes/backrooms_room_with_plants_overgrown.LICENSE.txt), which ships beside the mesh. |
| Where recorded | The licence file is a transcription of the GLB's `asset.extras` block (Sketchfab exporter `Sketchfab-0.3.0`). Shipped as downloaded: 8.6k triangles, 17 JPEG/PNG maps, 18 PBR materials. Credited on the in-game Credits screen. |

Required credit: *This work is based on "Backrooms Room With Plants (Overgrown)"
(<https://sketchfab.com/3d-models/backrooms-room-with-plants-overgrown-56a9d3e08160479aa287873a13fef455>)
by Blenderust (<https://sketchfab.com/narighillya>) licensed under CC-BY-4.0
(<http://creativecommons.org/licenses/by/4.0/>).*

**ACTION REQUIRED:** verify on Sketchfab, as for Backrooms VR above.

## Level 37: Flooded Tiled Complex — `Meshes/level_37_flooded_tiled_complex.glb`

| | |
|---|---|
| Source | <https://sketchfab.com/3d-models/level-37-flooded-tiled-complex-fdc730ebac5b4c6aa9f620e5e405e84f> |
| Author | Blenderust (<https://sketchfab.com/narighillya>) |
| Licence | CC-BY-4.0 — [`Meshes/level_37_flooded_tiled_complex.LICENSE.txt`](Meshes/level_37_flooded_tiled_complex.LICENSE.txt), which ships beside the mesh. |
| Where recorded | The licence file is a transcription of the GLB's `asset.extras` block (Sketchfab exporter `Sketchfab-0.3.0`). Shipped as downloaded: 15k triangles, 11 JPEG/PNG maps, 11 PBR materials. Credited on the in-game Credits screen. |

Required credit: *This work is based on "Level 37: Flooded Tiled Complex"
(<https://sketchfab.com/3d-models/level-37-flooded-tiled-complex-fdc730ebac5b4c6aa9f620e5e405e84f>)
by Blenderust (<https://sketchfab.com/narighillya>) licensed under CC-BY-4.0
(<http://creativecommons.org/licenses/by/4.0/>).*

**ACTION REQUIRED:** verify on Sketchfab, as for Backrooms VR above.

## Classic Interior Door — `Meshes/Classic_Interior_Door.glb`

| | |
|---|---|
| Source | Sketchfab, per [`Meshes/intro_door.ATTRIBUTION.txt`](Meshes/intro_door.ATTRIBUTION.txt); the model's page URL is not recorded. |
| Author | Not recorded. |
| Licence | **UNCONFIRMED.** The GLB's metadata (`generator: OpenSceneGraph 3.5.6`) carries no licence block. |
| Notes | Supplied by the project owner. Its embedded textures were resized from 4096 to 512 square by `tools/shrink_glb.py`; geometry and materials are the file as supplied. |

**ACTION REQUIRED:** find the model's Sketchfab page, record the author and licence in
`Meshes/intro_door.ATTRIBUTION.txt`, and credit it. If the licence turns out to be CC-BY-NC
or a Sketchfab Standard licence, the model cannot ship in a commercial build.

## Audio — `assets/sfx/*.flac`, `assets/music/*.flac`

| | |
|---|---|
| What | Every sound in the game: 42 effects and 5 ambience loops. |
| Source | **Generated.** `tools/gen_sfx.py` synthesises all of them from filtered noise, sine partials, envelopes and a synthetic room impulse. No samples, no recordings, no libraries of either. |
| Licence | The project's own (MIT), like any other file the repository's tools produce. |
| Notes | Seeded and deterministic: `python3 tools/gen_sfx.py` rewrites the same bytes. The tool's docstring is the manifest — one line per sound saying how it is made. Nothing here needs crediting to anyone. |

The soundtrack that used to sit here (`assets/music/ost.mp3`) was a *Demon's Souls* track
re-uploaded by a third party — its own ID3 tags said so — and was not licensable for
distribution in any form. It is **deleted** from the tree, and the generated loops replace it.

**ACTION REQUIRED:** deleting a file does not remove it from the repository. The 20 MB blob is
still reachable in git history, so any clone or push carries it. Run the history rewrite in
README "Shipping" (`git lfs migrate` / `git filter-repo`) before the first push, and only then is
this resolved.

## Grass shading — `Shaders/grassblade.frag`

| | |
|---|---|
| Reference | MonterMan, *"grass field with blades"*, <https://www.shadertoy.com/view/dd2cWh> |
| Licence of the reference | CC BY-NC-SA 3.0 (the Shadertoy default). |
| What was taken | Ideas, not code, per the shader's header and `tools/gen_grass_houdini.py`: the young/old grass colour mix, the `0.3 + 0.7*sqrt(h)` height-based ambient occlusion and the fresnel rim. The reference raymarches an SDF; this shader rasterises real blades, and none of the reference's code is present. |

**ACTION REQUIRED (legal read):** shading constants and a one-line AO formula are on the
thin side of what copyright protects, and the implementation is independent, but the
reference is NC and SA and the note saying so sits in a shader that would ship in a
commercial build. Have someone decide whether the three borrowed ideas need re-deriving
(trivial: they are two colours, a square root and a fresnel term) or whether the credit as
written is enough.

## Cursor art — `assets/ui/cursors_src.png` → `Textures/ui_cursors.bmp`

| | |
|---|---|
| Source | Not recorded. The README calls it "source art for the cursor atlas"; `tools/gen_ui.py` slices it into `Textures/ui_cursors.bmp`. |
| Licence | Not recorded. |

**ACTION REQUIRED:** confirm it is original work and say so here (one line), or record where
it came from.

## Generated and original assets

Made for this project by its tools; no third-party content, nothing to credit.

| File | Made by |
|---|---|
| `Meshes/meadow.obj` | `tools/gen_meadow.py` |
| `Meshes/meadow_tile.obj` | `daydreams gen-terrain` (`src/ext/terrain.rs`) |
| `Meshes/penrose_stairs.obj` | `tools/gen_escher.py` — original geometry after Penrose/Escher's *Ascending and Descending*, not derived from any model |
| `Meshes/bounds.obj`, `gallery_bounds.obj`, `intro_door_collide.obj`, `room.obj` | Hand-written collision and room geometry |
| `Textures/grass_noise.bmp` | `tools/gen_meadow.py` |
| `Textures/cube_sticker.bmp` | `tools/bake_cube.py` |
| `Textures/ui_font.bmp`, `Textures/ui_cursors.bmp` | `tools/gen_ui.py` (derived from the font and cursor art above) |
| `Textures/portrait_mona.bmp`, `portrait_mona_parts.bmp`, `portrait_vermeer.bmp`, `portrait_vermeer_parts.bmp`, `portrait_cavalier.bmp`, `portrait_cavalier_parts.bmp` | `tools/gen_portraits.py` (derived from the portrait sheets — see "The portraits" above) |
| `assets/sfx/*.flac` (42 effects), `assets/music/*.flac` (5 ambience loops) | `tools/gen_sfx.py` — synthesised from nothing, see above |
| `Shaders/*` other than the five listed under the engine | Written for this project |

`Textures/cube_projection.bmp` is referenced by no code and no tool, and its origin is not
recorded; it is in the same position as the cursor art above (item 4 below) and should either
be accounted for or dropped from the shipped set.

## Crates

The licence of every crate in the dependency tree, as resolved for the four shipping
targets (`cargo deny list`, `deny.toml`'s `[graph] targets`). Counts are crate-versions; most
crates are dual-licensed `MIT OR Apache-2.0`, so they appear under both. `cargo deny check`
enforces the allowlist in `deny.toml` and fails CI on anything outside it.

| Licence | Crates |
|---|---|
| MIT | 214 |
| Apache-2.0 | 181 — `rapier3d`, the rigid-body engine behind the props (src/ext/physics.rs), is Apache-2.0 only, as are `parry3d`, `nalgebra` and `simba` under it |
| Zlib | 18 — the two new ones are `zune-core` and `zune-jpeg`, the JPEG decoder behind `image`'s "jpeg" feature (each also MIT / Apache-2.0) |
| MPL-2.0 | 13 — `symphonia*` (kira's decoders), `audio_thread_priority`, `option-ext`, `triple_buffer` |
| Apache-2.0 WITH LLVM-exception | 4 — `rustix`, `linux-raw-sys` |
| BSD-2-Clause | 4 |
| BSD-3-Clause | 4 |
| ISC | 3 — `libloading`, `inotify`, `inotify-sys` |
| Unlicense | 5 — `byteorder`, `byteorder-lite`, `memchr`, `termcolor`, `winapi-util` (each also MIT) |
| 0BSD | 1 — `adler2` (also MIT / Apache-2.0) |
| Unicode-3.0 | 1 — `unicode-ident` |

MPL-2.0 is file-level copyleft: the MPL-licensed sources must remain available under the MPL,
which they are — the crates are used unmodified from crates.io. Nothing in the shipping
targets' tree is GPL or LGPL. (`r-efi`, in `Cargo.lock` for UEFI targets only, offers
LGPL-2.1-or-later as one of three options alongside MIT and Apache-2.0; it is not built.)

To regenerate the full per-crate listing:

```sh
cargo deny list -l license
# or, without cargo-deny:
cargo tree --format "{p} {l}" --prefix none | sort -u
```

## ACTION REQUIRED — summary

1. **`Meshes/Classic_Interior_Door.glb`** — licence unconfirmed, author unrecorded. Find the
   Sketchfab page; record both in `Meshes/intro_door.ATTRIBUTION.txt`; credit or remove.
2. **`Meshes/backrooms_vr.glb`**, **`Meshes/elevator_with_animation_lowpoly.glb`**,
   **`Meshes/backrooms_room_with_plants_overgrown.glb`**,
   **`Meshes/level_37_flooded_tiled_complex.glb`** — CC-BY-4.0 per each file's own
   metadata (carlcapu9, EFX, Blenderust, Blenderust). The licence files and the credit
   lines are in place; the Sketchfab check of each page is what remains.
3. **`Shaders/grassblade.frag`** — ideas credited to a CC BY-NC-SA 3.0 Shadertoy. Needs a
   legal read for a commercial build, or the three borrowed constants re-derived.
4. **`assets/ui/cursors_src.png`** and **`Textures/cube_projection.bmp`** — provenance not
   recorded. Confirm original (and drop the unreferenced texture, or say what it is).
5. **`LICENSE`** — the copyright holder reads "DayDreams contributors", a placeholder.
   Confirm the name (and the year) before a build leaves this machine.
6. **`assets/music/ost.mp3`** — deleted from the tree, still in history. Rewrite it out before
   the repository is pushed anywhere.
