"""Generate the UI atlases: cursor sprites and the two bitmap faces.

Emits 32-bit BMPs (BGRA, real alpha) -- the engine's loader gained an EXT branch for those --
plus `src/ext/ui_atlas.rs` with the sprite/glyph rectangles as Rust consts, so nothing is parsed
at runtime.

Run `cargo fmt` afterwards: the Rust here is emitted compactly, line by line, and rustfmt owns
its final shape -- `cargo fmt --check` is what CI runs, and it sees this file like any other.

UV convention: the engine's BMP loader leaves GL t=0 at the image TOP (Texture.cpp:27-41 reads
rows bottom-first into img[height-1] downward). So all rects here are top-origin pixel rects and
ui.vert flips the quad's v before sampling. Keep both halves of that in step.
"""
from PIL import Image, ImageDraw, ImageFont
import json, struct, os

OUT_CURSORS = "Textures/ui_cursors.bmp"
OUT_FONT    = "Textures/ui_font.bmp"
OUT_TITLE   = "Textures/ui_title.bmp"
OUT_RS      = "src/ext/ui_atlas.rs"

def write_bmp32(path, im):
    """32-bit uncompressed BMP, BGRA, bottom-first rows, 54-byte header (data offset 54)."""
    im = im.convert("RGBA")
    w, h = im.size
    px = im.load()
    rows = []
    for y in range(h - 1, -1, -1):                # bottom-first
        row = bytearray()
        for x in range(w):
            r, g, b, a = px[x, y]
            row += bytes((b, g, r, a))
        rows.append(bytes(row))
    data = b"".join(rows)
    hdr  = struct.pack("<2sIHHI", b"BM", 54 + len(data), 0, 0, 54)
    hdr += struct.pack("<IiiHHIIiiII", 40, w, h, 1, 32, 0, len(data), 2835, 2835, 0, 0)
    open(path, "wb").write(hdr + data)

# ── Cursors ──────────────────────────────────────────────────────────────────────────────────
src = Image.open("assets/ui/cursors_src.png").convert("RGBA")
# (name, x, y, w, h) from the sheet analysis
SPRITES = {
    "DOT":    (1,   1,   128, 128),
    "OPEN":   (2,   404, 55,  41),
    "CLOSED": (138, 339, 49,  42),
}
# Key out the teal background (0,128,128) that the sheet uses in place of alpha.
def keyed(crop):
    crop = crop.copy(); p = crop.load()
    for y in range(crop.height):
        for x in range(crop.width):
            r, g, b, a = p[x, y]
            if abs(r) < 10 and abs(g - 128) < 12 and abs(b - 128) < 12:
                p[x, y] = (0, 0, 0, 0)
    return crop

atlas = Image.new("RGBA", (256, 128), (0, 0, 0, 0))
rects = {}
cx = 0
for name, (x, y, w, h) in SPRITES.items():
    crop = keyed(src.crop((x, y, x + w, y + h)))
    atlas.paste(crop, (cx, 0))
    rects[name] = (cx, 0, w, h)
    cx += w + 2
write_bmp32(OUT_CURSORS, atlas)
print(f"cursors -> {OUT_CURSORS} {atlas.size}: {rects}")

# ── The faces ────────────────────────────────────────────────────────────────────────────────
# Two faces, two atlases, because they do two different jobs.
#
# The INTERFACE face is Playpen Sans Bold and sets everything the menus say: headings, rows,
# values, the key map, the credits, the footer hints. The TITLE face is Henny Penny and sets one
# string -- the game's name on the title screen -- which is why its atlas carries only the
# characters that string uses.
#
# Both ship under assets/fonts/ (SIL OFL, the licence sits beside each) rather than being taken
# from the system, because a baked atlas has to be reproducible: whatever font this picks is
# frozen into the BMP and into ui_atlas.rs, and a machine without it would silently regenerate
# the UI in something else. The system faces stay as a fallback for the interface face so the
# tool still runs on a checkout without the asset. The title face has none: a display face has
# no stand-in, and a wordmark set in Arial is worse than a tool that stops.
PLAYPEN = "assets/fonts/PlaypenSans[wght].ttf"
UI_CANDIDATES = [
    PLAYPEN,
    "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "/System/Library/Fonts/Helvetica.ttc",
]
HENNY = "assets/fonts/HennyPenny-Regular.ttf"

# The game's name. It lives here, and is written into ui_atlas.rs, so that the string and the
# glyphs baked for it cannot drift apart: the title atlas holds exactly these characters, and a
# name edited in the Rust instead of here would be a name with no glyphs to draw it.
TITLE_TEXT = "Hide 'N Dream"

# Baked sizes. This is the resolution the glyphs actually have; `Ui::draw_text` scales the atlas
# rects to whatever size a screen asks for, and scaling UP is what makes text look rasterised.
#
# The largest thing the interface face draws is a screen heading at 0.11 of the drawable height
# -- 158 px on a 1440p panel, 238 px on a 4K one. Playpen Sans sets its caps at 0.81 of its
# nominal size, so 176 px carries that heading at better than 1:1 up to 1440p and keeps the
# whole of printable ASCII inside one 2048x1024 atlas; 192 px would have spilled into a second
# row band and doubled the file for nothing. The title is drawn far larger -- POSTER_TITLE_SIZE
# is 0.13 of the height -- so its face is baked at 224, which costs almost nothing because
# eleven glyphs fit a 1024x512 atlas whatever the size.
UI_SIZE = 176
TITLE_SIZE = 224
UI_ATLAS_W = 2048
TITLE_ATLAS_W = 1024
PAD = 2


def rust_char(ch):
    """`ch` as a Rust char literal. Every character here is printable ASCII, so the only two
    that need an escape are the quote and the backslash."""
    return {"'": r"'\''", "\\": r"'\\'"}.get(ch, f"'{ch}'")


def bake(font, chars, atlas_w):
    """Rasterise `chars` white-on-transparent into a row-packed atlas.

    Returns `(image, {ch: (x, y, w, h, bx, by, adv)})`. Rects are top-origin; `by` is measured
    down from the top of the line box, because PIL's default "la" anchor draws from the
    ascender line and that is the y `Ui::draw_text` is handed. The atlas is filled with white
    at zero alpha so that filtering toward an empty texel fades a glyph out rather than
    darkening its edge.
    """
    tmp = Image.new("L", (256, 256))
    d = ImageDraw.Draw(tmp)
    glyphs, x, y, row_h = {}, PAD, PAD, 0
    for ch in chars:
        l, t, r, b = d.textbbox((0, 0), ch, font=font)
        w, h = max(1, r - l), max(1, b - t)
        adv = int(round(d.textlength(ch, font=font)))
        if x + w + PAD > atlas_w:
            x = PAD; y += row_h + PAD; row_h = 0
        glyphs[ch] = (x, y, w, h, l, t, adv)
        x += w + PAD; row_h = max(row_h, h)
    atlas_h = 1
    while atlas_h < y + row_h + PAD:
        atlas_h *= 2
    im = Image.new("RGBA", (atlas_w, atlas_h), (255, 255, 255, 0))
    dd = ImageDraw.Draw(im)
    for ch, (gx, gy, _, _, l, t, _) in glyphs.items():
        dd.text((gx - l, gy - t), ch, font=font, fill=(255, 255, 255, 255))
    return im, glyphs


ui_path = next(p for p in UI_CANDIDATES if os.path.exists(p))
ui_font = ImageFont.truetype(ui_path, UI_SIZE)
# Playpen Sans ships as a single variable font with a 100..800 weight axis whose default is
# Regular. The UI is bold throughout -- white text over a photographed scene needs the weight --
# so pin the Bold instance before anything is measured or drawn. Measurement and rasterisation
# both read the current variation, so this must happen before `bake`.
if ui_path == PLAYPEN:
    ui_font.set_variation_by_name("Bold")
CHARS = [chr(c) for c in range(32, 127)]
fimg, glyphs = bake(ui_font, CHARS, UI_ATLAS_W)
write_bmp32(OUT_FONT, fimg)
ui_ascent, ui_descent = ui_font.getmetrics()
print(f"font -> {OUT_FONT} {fimg.size} from {os.path.basename(ui_path)} @ {UI_SIZE}px, "
      f"ascent {ui_ascent} descent {ui_descent}")

title_font = ImageFont.truetype(HENNY, TITLE_SIZE)
TITLE_CHARS = sorted(set(TITLE_TEXT))
timg, tglyphs = bake(title_font, TITLE_CHARS, TITLE_ATLAS_W)
write_bmp32(OUT_TITLE, timg)
title_ascent, title_descent = title_font.getmetrics()
print(f"title -> {OUT_TITLE} {timg.size} from {os.path.basename(HENNY)} @ {TITLE_SIZE}px, "
      f"{len(TITLE_CHARS)} glyphs for {TITLE_TEXT!r}, ascent {title_ascent} descent {title_descent}")

# ── Rust consts ──────────────────────────────────────────────────────────────────────────────
with open(OUT_RS, "w") as f:
    f.write("//! EXT: generated by tools/gen_ui.py -- DO NOT EDIT.\n")
    f.write("//! Pixel rectangles into the UI atlases, top-origin (see the tool's UV note).\n\n")
    f.write("/// (x, y, w, h) in `Textures/ui_cursors.bmp`.\n")
    f.write("pub struct Sprite { pub x: u32, pub y: u32, pub w: u32, pub h: u32 }\n")
    f.write(f"pub const CURSOR_ATLAS: (u32, u32) = ({atlas.width}, {atlas.height});\n")
    for name, (sx, sy, sw, sh) in rects.items():
        f.write(f"pub const CURSOR_{name}: Sprite = Sprite {{ x: {sx}, y: {sy}, w: {sw}, h: {sh} }};\n")
    f.write("\n/// One glyph: atlas rect, then bearing (left, top) and advance, all in atlas pixels\n")
    f.write("/// at its face's baked size. Scale by (target_px / FONT_SIZE) when drawing.\n")
    f.write("#[derive(Clone, Copy)]\n")
    f.write("pub struct Glyph { pub x: u32, pub y: u32, pub w: u32, pub h: u32, pub bx: i32, pub by: i32, pub adv: i32 }\n")
    f.write("\n// ── The interface face: every heading, row, hint and credit ──\n")
    f.write(f"pub const FONT_ATLAS: (u32, u32) = ({fimg.width}, {fimg.height});\n")
    f.write(f"pub const FONT_SIZE: f32 = {UI_SIZE}.0;\n")
    f.write("/// Where the baseline sits below the top of the line box, in baked pixels.\n")
    f.write("///\n")
    f.write("/// Nothing places a baseline directly -- every glyph carries its own offset from the\n")
    f.write("/// line box's top, which is what `Ui::draw_in` adds to the y it is given -- but this is\n")
    f.write("/// the other half of what that offset means, and setting the two faces on one line\n")
    f.write("/// would need it.\n")
    f.write("#[allow(dead_code)] // the atlas describing itself, not a call site\n")
    f.write(f"pub const FONT_ASCENT: f32 = {ui_ascent}.0;\n")
    f.write("/// Indexed by (codepoint - 32) for ASCII 32..=126.\n")
    f.write("pub const GLYPHS: [Glyph; 95] = [\n")
    for ch in CHARS:
        gx, gy, gw, gh, l, t, adv = glyphs[ch]
        f.write(f"    Glyph {{ x: {gx}, y: {gy}, w: {gw}, h: {gh}, bx: {l}, by: {t}, adv: {adv} }}, // {repr(ch)}\n")
    f.write("];\n")
    f.write("\n// ── The title face: the wordmark and nothing else ──\n")
    f.write("/// The game's name, as the title screen sets it. Baked here rather than written in\n")
    f.write("/// `ext::menu` because `TITLE_GLYPHS` holds exactly the characters it uses: the two\n")
    f.write("/// are one edit in `tools/gen_ui.py`, and cannot come apart.\n")
    f.write(f"pub const TITLE_TEXT: &str = {json.dumps(TITLE_TEXT)};\n")
    f.write(f"pub const TITLE_ATLAS: (u32, u32) = ({timg.width}, {timg.height});\n")
    f.write(f"pub const TITLE_FONT_SIZE: f32 = {TITLE_SIZE}.0;\n")
    f.write("#[allow(dead_code)] // see FONT_ASCENT\n")
    f.write(f"pub const TITLE_ASCENT: f32 = {title_ascent}.0;\n")
    f.write("/// Sorted by character, so a lookup is a scan of at most this many entries.\n")
    f.write(f"pub const TITLE_GLYPHS: [(char, Glyph); {len(TITLE_CHARS)}] = [\n")
    for ch in TITLE_CHARS:
        gx, gy, gw, gh, l, t, adv = tglyphs[ch]
        f.write(f"    ({rust_char(ch)}, "
                f"Glyph {{ x: {gx}, y: {gy}, w: {gw}, h: {gh}, bx: {l}, by: {t}, adv: {adv} }}),\n")
    f.write("];\n")
print(f"rust -> {OUT_RS} ({len(CHARS)} interface glyphs, {len(TITLE_CHARS)} title glyphs)")
