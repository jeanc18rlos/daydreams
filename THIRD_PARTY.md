# Third-party material

Everything Hide 'N Dream ships that was not written for it, with its source and licence as
recorded in this repository. Where nothing is recorded, this file says so rather than guessing;
those entries are marked **ACTION REQUIRED** and are collected at the end.

The project's own code is MIT (`Cargo.toml`; the text is the root [`LICENSE`](LICENSE)).
**The copyright holder named there, "Hide 'N Dream contributors", is a placeholder** until the
project decides who holds it -- a person, a company or that phrase on purpose. The in-game
Credits screen (`src/ext/menu.rs`, the scrolling `ROLL`) names the engine and every credited
model author; the test `every_cc_by_author_is_credited` holds it to that.

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

## Playpen Sans — `assets/fonts/PlaypenSans[wght].ttf`

| | |
|---|---|
| What | The interface face: every heading, row, value, hint and credit the menus set. |
| Source | The Playpen Sans Project, <https://github.com/TypeTogether/Playpen-Sans> |
| Author | © 2023 The Playpen Sans Project Authors (TypeTogether — Laura Meseguer, Veronika Burian, José Scaglione, Kostas Bartsokas, Vera Evstafieva, Tom Grace, Yorlmar Campos) |
| Licence | SIL Open Font License 1.1 — [`assets/fonts/PlaypenSans.LICENSE.txt`](assets/fonts/PlaypenSans.LICENSE.txt), which ships beside the font. |
| Notes | `tools/gen_ui.py` pins the Bold instance of its 100..800 weight axis and rasterises it into `Textures/ui_font.bmp`; that atlas is a derived work of the font and ships under the same licence. The OFL permits bundling with software and forbids selling the font on its own. |

## Henny Penny — `assets/fonts/HennyPenny-Regular.ttf`

| | |
|---|---|
| What | The title face: the game's name on the title screen, and nothing else in the game. |
| Source | Brownfox, <https://fonts.google.com/specimen/Henny+Penny> |
| Author | © 2012 Brownfox — drawn by Olga Umpeleva |
| Licence | SIL Open Font License 1.1 — [`assets/fonts/HennyPenny.LICENSE.txt`](assets/fonts/HennyPenny.LICENSE.txt), which ships beside the font. |
| Notes | `tools/gen_ui.py` rasterises the eleven characters of the game's name into `Textures/ui_title.bmp`; that atlas is a derived work of the font and ships under the same licence. "Henny Penny" is a Reserved Font Name under the OFL and a Brownfox trademark: the font is shipped unmodified and under its own filename, so neither is engaged, and any *modified* copy would have to be renamed. |

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

## Abandoned House — `Meshes/abandoned_house.glb`

| | |
|---|---|
| Source | <https://sketchfab.com/3d-models/abandoned-house-a876537d2ef24ac1b3108a7fd6ae3b4c> |
| Author | Elbolillo (<https://sketchfab.com/Elbolilloduro>) |
| Licence | CC-BY-4.0 — [`Meshes/abandoned_house.LICENSE.txt`](Meshes/abandoned_house.LICENSE.txt), which ships beside the mesh. |
| Where recorded | The licence file is a transcription of the GLB's `asset.extras` block (Sketchfab exporter `Sketchfab-16.12.0`). Shipped as downloaded: 70k triangles, 74 JPEG/PNG maps, 77 PBR materials. Credited on the in-game Credits screen. |

Required credit: *This work is based on "Abandoned_House"
(<https://sketchfab.com/3d-models/abandoned-house-a876537d2ef24ac1b3108a7fd6ae3b4c>)
by Elbolillo (<https://sketchfab.com/Elbolilloduro>) licensed under CC-BY-4.0
(<http://creativecommons.org/licenses/by/4.0/>).*

**ACTION REQUIRED:** verify on Sketchfab, as for Backrooms VR above.

## Exploration objects — `Meshes/exploration_tools.glb`

| | |
|---|---|
| Source | <https://sketchfab.com/models/fa99705c1afa4e7da5186938709f0231>, and the page the licence is quoted from, <https://elbolilloduro.itch.io/exploration-objects> |
| Author | Elbolillo (<https://sketchfab.com/Elbolilloduro>) |
| Licence | CC0 1.0 (<https://creativecommons.org/publicdomain/zero/1.0/>) — a public domain dedication, the one asset here under it: no attribution is required for any use, commercial included. [`Meshes/exploration_tools.LICENSE.txt`](Meshes/exploration_tools.LICENSE.txt) ships beside the mesh. |
| Where recorded | The pack's itch.io page states it verbatim — "The models in this package are under the CC0 license, you can find more information about the license here" — read there at import (2026-08). Nothing to transcribe: the GLB is built here from the pack's FBX by `tools/build_props.py`, so it carries no Sketchfab `asset.extras` block; the licence file is the record, and no page check remains owed. |
| Notes | **Not shipped as downloaded.** `tools/build_props.py` (headless Blender) converts the pack's FBX into this GLB: the one unnamed mesh (`Cone.001`, the spirit box) is renamed `Spirit_Box`, every material is set metallic 0 / roughness 1 (PSX diffuse textures — glTF's default metallic 1 would render them as dark metal), and four materials get the pack's `*_Emissor.png` wired as the glTF emissive texture (the EMF detector ships five display states; the mid `_Emissor_02` is the one wired — a fixed mid reading for a prop that cannot swap maps at runtime). Geometry, UVs and the pack's own layout are otherwise as authored. Reproduce with `/Applications/Blender.app/Contents/MacOS/Blender --background --python tools/build_props.py`. |
| What is used | All 8 handheld tools — EMF_Detector, Flashlight, Flashlight_Poquet, Photo_Camera, Spirit_Box, Thermal_Camera, Thermometer, Voice_Recorder — as takeable, usable items in the scene derived from Liminal Neighborhood. |

No credit is required — CC0 waives it. One is given anyway: the in-game Credits screen carries
**EXPLORATION TOOLS - ELBOLILLO** as a courtesy, placed below the note that says the model lines
above it are CC-BY-4.0, so that note stays true.

## Objects Interior(Village) Alpha — `Meshes/village_objects.glb`

| | |
|---|---|
| Source | <https://sketchfab.com/3d-models/objects-interiorvillage-alpha-c640f60b970a48648a158c91c1c45b2b> (also on itch.io, <https://elbolilloduro.itch.io/objects-interiorvillage-alpha>, and Fab). Published 2023-01-30. |
| Author | Elbolillo (<https://sketchfab.com/Elbolilloduro>) |
| Licence | CC-BY-4.0 — [`Meshes/village_objects.LICENSE.txt`](Meshes/village_objects.LICENSE.txt), which ships beside the mesh. |
| Where recorded | The licence was read on the Sketchfab page at import (2026-08), where it shows as "CC Attribution" — not transcribed from embedded metadata: the GLB is built here from the pack's FBX files by `tools/build_props.py` and carries no `asset.extras` block. The licence file is the record, and the page check the meshes above still owe has already been done for this one. Credited on the in-game Credits screen. |
| Notes | **Not shipped as downloaded.** `tools/build_props.py` (headless Blender) converts the pack's FBX into this GLB: four empty stub meshes (0 vertices) are dropped, the fridge shelf that shipped named bare `House_Demo` is renamed `Fridge_Shelf` (so `House_Demo_*` stays an unambiguous shell prefix), every material is set metallic 0 / roughness 1, base-colour textures are re-wired by material name (the FBX references them at authoring-machine paths), eleven lamp and clock materials get their `*_Emissor` maps wired as emissive, and ONE mesh is added — `Lot_Ground`, a 38.6 x 26.7 m grass quad whose texture is pulled byte-for-byte out of `Meshes/abandoned_house.glb` (same author, CC-BY-4.0, covered by that file's own licence record) to patch the lot the game carves out of that stage. Geometry, UVs and the pack's own layout positions are otherwise as authored. Reproduce with the same command as above. |
| What is used | The complete PSX demo house (`House_Demo_*` shell, roof, windows, door) and some 150 furniture, food and clutter objects: the scene derived from Liminal Neighborhood carves the stage's baked house out and builds seeded procedural houses from this pack. |

Required credit: *This work is based on "Objects Interior(Village) Alpha"
(<https://sketchfab.com/3d-models/objects-interiorvillage-alpha-c640f60b970a48648a158c91c1c45b2b>)
by Elbolillo (<https://sketchfab.com/Elbolilloduro>) licensed under CC-BY-4.0
(<http://creativecommons.org/licenses/by/4.0/>).*

## Moon — `Meshes/moon.glb`

| | |
|---|---|
| Source | <https://sketchfab.com/3d-models/moon-2b2374581162435a95753b9053ffefe4> |
| Author | luckass333 (<https://sketchfab.com/luckass333>) |
| Licence | CC-BY-4.0 — [`Meshes/moon.LICENSE.txt`](Meshes/moon.LICENSE.txt), which ships beside the mesh. |
| Where recorded | The licence file is a transcription of the GLB's `asset.extras` block (Sketchfab exporter `Sketchfab-16.61.0`). Shipped as downloaded: 960 triangles, one 1024x512 PNG, one PBR material. Credited on the in-game Credits screen. |

Required credit: *This work is based on "Moon"
(<https://sketchfab.com/3d-models/moon-2b2374581162435a95753b9053ffefe4>)
by luckass333 (<https://sketchfab.com/luckass333>) licensed under CC-BY-4.0
(<http://creativecommons.org/licenses/by/4.0/>).*

**ACTION REQUIRED:** verify on Sketchfab, as for Backrooms VR above.

## Low-Poly PSX Style Essential Doors Pack — `Meshes/psx_essential_doors_pack.glb`

| | |
|---|---|
| Source | <https://sketchfab.com/3d-models/low-poly-psx-style-essential-doors-pack-20d55059056044b885a5267c2de1ec18> (short link <https://skfb.ly/pFOP7>) |
| Author | Icevanilla — `vanillao03` (<https://sketchfab.com/vanillao03>) |
| Licence | CC-BY-4.0 — [`Meshes/psx_essential_doors_pack.LICENSE.txt`](Meshes/psx_essential_doors_pack.LICENSE.txt), which ships beside the mesh. |
| Where recorded | The source and credit line are the ones the project owner supplied at import; the GLB's own `asset.extras` block (Sketchfab exporter `Sketchfab-16.99.0`) carries the same source and licence and gives the author as `vanillao03`. "Icevanilla" is the display name on the model's page. Credited on the in-game Credits screen. |
| Notes | **Not shipped as downloaded.** `tools/turn_white_door.py` bakes a half-turn about Y onto the two nodes the game uses, and nothing else: every door in the pack is modelled with its knob at the low-x end, so its hinge is the high-x edge, and `Anchor::Hinge` (`src/ext/gltf_model.rs`) hangs a leaf by its low-x edge — imported as it comes, the door would swing about its own doorknob. A rotation rather than a mirror, so winding, normals and tangents survive; free to look at because the door is symmetric front to back. Geometry, materials, UVs and the shared 1024x256 colour map are untouched. Re-run the tool on a fresh download to reproduce the file. |
| What is used | One of the seven pairs: the white `Bathroom Door_002.001` with its knob (`Circle.002`) and `Bathroom Doorframe_001.001`, as the intro meadow's door and the Backrooms' (`src/ext/door.rs`). The other six ship unused — the colour map is shared by all of them, so cutting them would save kilobytes of geometry and no texture. |

Required credit, as the licence gives it: *"Low-Poly PSX Style Essential Doors Pack"
(<https://skfb.ly/pFOP7>) by Icevanilla is licensed under Creative Commons Attribution
(<http://creativecommons.org/licenses/by/4.0/>).*

**ACTION REQUIRED:** verify on Sketchfab, as for Backrooms VR above.

## Mannequin and its animation — `Meshes/mannequin.glb`

| | |
|---|---|
| What | The game's one rigged character and everything it can do: Mixamo's own stock character "Mannequin" (internal id `Ch36_nonPBR`, 65-bone `mixamorig1:` skeleton, 1.77 m, four maps), and 31 animation clips Mixamo had already fitted to it. |
| Source | Adobe Mixamo, <https://www.mixamo.com>. Downloaded by the project owner while signed in to their own Adobe account — the character once, and each clip as an animation-only ("Without Skin") FBX with the Mannequin selected. |
| Author | Adobe Inc. (Mixamo). Stock content, not a community upload: there is no Sketchfab page behind this file and no third-party author to credit. |
| Licence | **Not Creative Commons — the one asset here that is not.** Adobe Mixamo content, used under the owner's Mixamo/Adobe account. What is known is that Mixamo provides its characters and animation clips to account holders for use in their projects. What is *not* recorded is whether redistribution inside a shipped game is permitted, whether crediting Adobe is required, allowed or forbidden, and what changes if the game is sold. [`Meshes/mannequin.LICENSE.txt`](Meshes/mannequin.LICENSE.txt) ships beside the mesh and says exactly that, without quoting terms nobody has read. |
| Where recorded | Nothing to transcribe: this file is built here rather than downloaded whole, so its `asset` block carries only the exporter (`Khronos glTF Blender I/O v5.0.21`) and the character's Mixamo id survives only in the texture names (`Ch36_1001_*`). The licence file is the record, from what the owner supplied at import. Credited on the in-game Credits screen as **MANNEQUIN AND ANIMATION - ADOBE MIXAMO**, placed after the line that says the models above it are CC-BY so that line stays true. |
| Notes | **Not shipped as downloaded.** `tools/build_mannequin.py` (headless Blender) merges the character FBX and the 31 animation-only FBXs into one GLB — one mesh, one skeleton, 31 clips named after their files — and downscales the four 4096px maps to 1024px, which is the only change to any pixel and most of why the file is 14.6 MB rather than far more. Geometry, UVs, materials and keyframes are otherwise untouched, and nothing is retargeted: every clip was fitted by Mixamo to this same character, so merging is a direct action copy with no rigging step. Reproduce with `/Applications/Blender.app/Contents/MacOS/Blender --background --python tools/build_mannequin.py`. |
| What is used | One clip of the 31: `walking`, in the Mannequin Test scene (`src/level32.rs`), through the CPU skinner (`src/ext/skinned.rs`). The other 30 ship unused — they are keyframes on a skeleton the file already carries, and cost little beside the four maps. |

No required credit is recorded, because no licence text was read at import. The Credits line above
is offered rather than owed: it costs nothing if attribution turns out to be optional, and covers
the case where it is not.

**ACTION REQUIRED:** verify the current Mixamo/Adobe terms before distributing. This is a wider
question than the Sketchfab checks above — there the licence is known and only the page needs
confirming; here the terms themselves have never been read against this use, and the asset is
14.6 MB of Adobe content in a shipped build.

## Audio — `assets/sfx/*.flac`, `assets/music/*.flac`

| | |
|---|---|
| What | Every sound in the game: 42 effects and 5 ambience loops. |
| Source | **Generated.** `tools/gen_sfx.py` synthesises all of them from filtered noise, sine partials, envelopes and a synthetic room impulse. No samples, no recordings, no libraries of either. |
| Licence | The project's own (MIT), like any other file the repository's tools produce. |
| Notes | Seeded and deterministic: `python3 tools/gen_sfx.py` rewrites the same bytes. The tool's docstring is the manifest — one line per sound saying how it is made. Nothing here needs crediting to anyone. |

## Placeholder soundtrack — `assets/music/ost.mp3`

| | |
|---|---|
| What | The track that plays on the title screen and in any scene without room tone of its own. |
| Source | A *Demon's Souls* soundtrack track re-uploaded by a third party — its own ID3 tags say so ("Demon's Souls OST - Character Creation Theme (extended)", uploader "Zombiesneglen"). |
| Licence | **None that permits distribution.** It is a placeholder kept deliberately, for demos only. |
| Notes | The generated `ambient.flac` stands ready to take its place: whichever unprefixed track is not the generated one wins (`ext/audio.rs`, `index_music`), so deleting this file is the whole of the change — no code, no rename. |

**ACTION REQUIRED — before anything ships:** delete `assets/music/ost.mp3`, and rewrite it out of
history as well (README "Shipping": `git lfs migrate` / `git filter-repo`), because deleting a
file does not remove the 20 MB blob from the repository and any clone or push carries it.

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
| `Textures/ui_font.bmp`, `Textures/ui_title.bmp`, `Textures/ui_cursors.bmp` | `tools/gen_ui.py` (derived from the two faces and the cursor art above) |
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

1. **`Meshes/backrooms_vr.glb`**, **`Meshes/elevator_with_animation_lowpoly.glb`**,
   **`Meshes/backrooms_room_with_plants_overgrown.glb`**,
   **`Meshes/level_37_flooded_tiled_complex.glb`**, **`Meshes/abandoned_house.glb`**,
   **`Meshes/moon.glb`**, **`Meshes/psx_essential_doors_pack.glb`** — CC-BY-4.0 per each
   file's own metadata (carlcapu9, EFX, Blenderust, Blenderust, Elbolillo, luckass333,
   Icevanilla). The licence files and the credit lines are in place; the Sketchfab check of
   each page is what remains. The two Elbolillo object packs `tools/build_props.py` converts
   (`exploration_tools.glb`, CC0; `village_objects.glb`, CC-BY-4.0) are not in this list:
   their pages were read at import, so no check remains for them. Elbolillo's five character
   packs (Characters_psx, Characters_Extras, Character Ghost/Pumpkin/Witch) are being rigged
   externally via Mixamo and are not currently shipped; record their licences again at
   re-import — hard rule.
2. **`Meshes/mannequin.glb`** — Adobe Mixamo's stock Mannequin and 31 Mixamo clips, from the
   owner's own Adobe account. Not CC-BY, not covered by any licence text in this repository:
   what Mixamo's terms permit for a game that is distributed, or sold, has never been read
   against this use. Verify the current Mixamo/Adobe terms before distributing, and record the
   answer in `Meshes/mannequin.LICENSE.txt`, which currently says only what is known.
3. **`Shaders/grassblade.frag`** — ideas credited to a CC BY-NC-SA 3.0 Shadertoy. Needs a
   legal read for a commercial build, or the three borrowed constants re-derived.
4. **`assets/ui/cursors_src.png`** and **`Textures/cube_projection.bmp`** — provenance not
   recorded. Confirm original (and drop the unreferenced texture, or say what it is).
5. **`LICENSE`** — the copyright holder reads "Hide 'N Dream contributors", a placeholder.
   Confirm the name (and the year) before a build leaves this machine.
6. **`assets/music/ost.mp3`** — a *Demon's Souls* track, kept **on purpose** as a demo
   placeholder and not distributable. Delete the file (the generated `ambient.flac` takes over
   by itself) and rewrite it out of history before the repository is pushed anywhere.
