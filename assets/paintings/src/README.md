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
| `cavalier_sheet.png` | *The Laughing Cavalier*, Frans Hals, 1624 (The Wallace Collection) | Public domain (author died 1666) |

Each sheet holds the base portrait with the eye sockets blanked and the mouth removed, plus
the eyes looking at the viewer, the eyes looking to the viewer's left, and three mouths
(smile, sad, angry). How those variants are drawn differs by sheet and decides how the tool
cuts them: the Mona's are cutouts with their own alpha, the Vermeer's rectangular crops of the
face on a black field, the Cavalier's rectangular crops with alpha on the painting's own
painted ground. The Cavalier sheet also carries each mouth again as four splayed PNG cutouts
in its right column; the tool does not use them (a moustache out of its place has nothing to
register against — see `tools/gen_portraits.py`'s docstring, which says how a sheet is added).

The generated textures are derived works of these sheets and ship with the game; see
THIRD_PARTY.md at the repository root.
