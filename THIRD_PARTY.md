# Third-party material

Everything DayDreams ships that was not written for it, with its source and licence as
recorded in this repository. Where nothing is recorded, this file says so rather than guessing;
those entries are marked **ACTION REQUIRED** and are collected at the end.

The project's own code is MIT (`Cargo.toml`; the text is the root [`LICENSE`](LICENSE)).
**The copyright holder named there, "DayDreams contributors", is a placeholder** until the
project decides who holds it -- a person, a company or that phrase on purpose. The in-game
Credits screen (`src/ext/menu.rs`, `CREDITS_TEXT`) names the engine, Escher Relativity and
Backrooms VR.

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

## Soundtrack — `assets/music/ost.mp3`

| | |
|---|---|
| Source | Not recorded in the repository. |
| Licence | **None recorded.** |
| What the file says | Its ID3 tags read title *"Demon's Souls OST - Character Creation Theme (extended)"*, artist *"Zombiesneglen"*, encoder `Lavf59.27.100` (an ffmpeg transcode, with a `dash` major brand — a download of a streamed upload). |

**ACTION REQUIRED:** on its own tags this is a track from the *Demon's Souls* soundtrack
(composed by Shunsuke Kida, © Sony Interactive Entertainment / FromSoftware), re-uploaded and
extended by a third party. That is not licensable for distribution in any form. Replace it
with a track whose licence is recorded here before any build leaves this machine. The audio
code needs nothing specific: drop any `ogg`/`mp3`/`wav`/`flac` into `assets/music/`
(README, "Audio").

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
| `Shaders/*` other than the five listed under the engine | Written for this project |

`Textures/cube_projection.bmp` is referenced by no code and no tool, and its origin is not
recorded; it is in the same position as the cursor art above (item 5 below) and should either
be accounted for or dropped from the shipped set.

## Crates

The licence of every crate in the dependency tree, as resolved for the four shipping
targets (`cargo deny list`, `deny.toml`'s `[graph] targets`). Counts are crate-versions; most
crates are dual-licensed `MIT OR Apache-2.0`, so they appear under both. `cargo deny check`
enforces the allowlist in `deny.toml` and fails CI on anything outside it.

| Licence | Crates |
|---|---|
| MIT | 189 |
| Apache-2.0 | 156 |
| Zlib | 16 |
| MPL-2.0 | 13 — `symphonia*` (kira's decoders), `audio_thread_priority`, `option-ext`, `triple_buffer` |
| Apache-2.0 WITH LLVM-exception | 4 — `rustix`, `linux-raw-sys` |
| BSD-2-Clause | 4 |
| BSD-3-Clause | 4 |
| ISC | 3 — `libloading`, `inotify`, `inotify-sys` |
| Unlicense | 3 — `byteorder`, `memchr`, `walkdir` (each also MIT) |
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

1. **`assets/music/ost.mp3`** — identifies itself as a *Demon's Souls* soundtrack re-upload.
   Not distributable. Replace.
2. **`Meshes/Classic_Interior_Door.glb`** — licence unconfirmed, author unrecorded. Find the
   Sketchfab page; record both in `Meshes/intro_door.ATTRIBUTION.txt`; credit or remove.
3. **`Meshes/backrooms_vr.glb`** — CC-BY-4.0 by carlcapu9 per the file's own metadata.
   Verify on Sketchfab, add a licence file beside the mesh and the credit line to the
   Credits screen. (The licence file and the credit line are in place; the Sketchfab
   check is what remains.)
4. **`Shaders/grassblade.frag`** — ideas credited to a CC BY-NC-SA 3.0 Shadertoy. Needs a
   legal read for a commercial build, or the three borrowed constants re-derived.
5. **`assets/ui/cursors_src.png`** and **`Textures/cube_projection.bmp`** — provenance not
   recorded. Confirm original (and drop the unreferenced texture, or say what it is).
6. **`LICENSE`** — the copyright holder reads "DayDreams contributors", a placeholder.
   Confirm the name (and the year) before a build leaves this machine.
