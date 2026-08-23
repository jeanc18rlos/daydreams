# Portrait variation sheets

Source sheets for the Backrooms' portraits. `tools/gen_portraits.py` cuts them into
`Textures/portrait_*.bmp` and `src/ext/portrait_atlas.rs`; nothing here is read at runtime.

Provenance: each sheet is the **project owner's own edit** (supplied 2026-08) of a painting
that is in the public domain worldwide — the underlying works are:

| File | Underlying work | Status |
|------|-----------------|--------|
| `mona_sheet.png` | *Mona Lisa*, Leonardo da Vinci, c. 1503–1506 (Musée du Louvre) | Public domain (author died 1519) |
| `mona_sheet_simple.png` | The same — a simpler sheet, kept as a cross-check of the base | Public domain |
| `vermeer_sheet.png` | *Girl with a Pearl Earring*, Johannes Vermeer, c. 1665 (Mauritshuis) | Public domain (author died 1675) |

Each sheet holds the base portrait with the eye sockets blanked and the mouth removed,
plus cutouts of the eyes (looking at the viewer / looking to the viewer's left) and of three
mouths (smile, sad, angry). A fourth sheet — Frans Hals' *Laughing Cavalier* (also public
domain; author died 1666) — is expected but not yet saved here; the tool's docstring says how
to add it.

The generated textures are derived works of these sheets and ship with the game; see
THIRD_PARTY.md at the repository root.
