"""Generate Textures/window_frame.bmp: painted wood for the window's frame (src/ext/window.rs).

The frame is four `cube.obj` bars, every face of that cube maps the whole texture, and the
window draws each bar with its length along the cube's y (`window::bar_layout`), which puts
the texture's v ACROSS the bar on the face toward the room and on the faces into the opening.
So: a flat, slightly yellowed off-white -- old gloss paint in a building lit like the
Backrooms, which the engine's flat `cutout` shader leaves as painted -- with a soft mottle
under it and a darker band along the v = 0 and v = 1 edges only. On a bar that band runs
along its length: the shadowed edge the shader would not light, and nothing stretched along
a two-metre bar the way a band at the ends would be. No grain: it would have to run along the
bar, and the mottle is too faint to have a way up, so the shader's V flip (see
Shaders/cutout.frag) changes nothing.

Emitted as a 32-bit BMP rather than 24-bit: the loader's 32-bit branch (src/texture.rs) is the
one that uploads with mipmaps and linear filtering, and a two-centimetre bar seen from across
the hall shimmers without them. The alpha is solid; `cutout` reads only the colour.

Row order follows the writer in tools/gen_ui.py: bottom-first, 54-byte header.
"""
import random
import struct

from PIL import Image

OUT = "Textures/window_frame.bmp"
SIZE = 128
# Old gloss paint, yellowed: the hall's wallpaper is a warmer yellow still, so the frame reads
# as a different material rather than a lighter patch of wall.
PAINT = (232, 222, 196)
# The edge band: the bevel in shadow.
EDGE = (150, 136, 104)
# Width of the band, in pixels of the 128 square, and how far its shading bleeds inward.
BAND = 4
BLEED = 10


def write_bmp32(path, im):
    """32-bit uncompressed BMP, BGRA, bottom-first rows, 54-byte header (data offset 54)."""
    im = im.convert("RGBA")
    w, h = im.size
    px = im.load()
    rows = []
    for y in range(h - 1, -1, -1):  # bottom-first
        row = bytearray()
        for x in range(w):
            r, g, b, a = px[x, y]
            row += bytes((b, g, r, a))
        rows.append(bytes(row))
    data = b"".join(rows)
    hdr = struct.pack("<2sIHHI", b"BM", 54 + len(data), 0, 0, 54)
    hdr += struct.pack("<IiiHHIIiiII", 40, w, h, 1, 32, 0, len(data), 2835, 2835, 0, 0)
    open(path, "wb").write(hdr + data)


def mottle(x, y):
    """A few octaves of smooth value noise, -1..1, seeded so the file is reproducible."""
    total, amp = 0.0, 1.0
    for octave in range(3):
        cells = 4 << octave
        fx, fy = x / SIZE * cells, y / SIZE * cells
        x0, y0 = int(fx) % cells, int(fy) % cells
        tx, ty = fx - int(fx), fy - int(fy)
        tx, ty = tx * tx * (3 - 2 * tx), ty * ty * (3 - 2 * ty)

        def v(i, j):
            rng = random.Random((octave * 1009 + (i % cells)) * 7919 + (j % cells))
            return rng.uniform(-1.0, 1.0)

        a = v(x0, y0) * (1 - tx) + v(x0 + 1, y0) * tx
        b = v(x0, y0 + 1) * (1 - tx) + v(x0 + 1, y0 + 1) * tx
        total += (a * (1 - ty) + b * ty) * amp
        amp *= 0.5
    return total / 1.75


im = Image.new("RGB", (SIZE, SIZE))
px = im.load()
for y in range(SIZE):
    for x in range(SIZE):
        # Distance to the nearer v edge (the image's top and bottom rows); the u edges are
        # the bar's ends and stay plain.
        d = min(y, SIZE - 1 - y)
        if d < BAND:
            shade = 1.0
        elif d < BAND + BLEED:
            t = (d - BAND) / BLEED
            shade = 1.0 - t * t * (3 - 2 * t)
        else:
            shade = 0.0
        # The paint's own unevenness: a few percent either way, never enough to read as dirt.
        m = 1.0 + 0.05 * mottle(x, y)
        c = tuple(
            int(max(0, min(255, (PAINT[i] * (1 - shade) + EDGE[i] * shade) * m)))
            for i in range(3)
        )
        px[x, y] = c

write_bmp32(OUT, im)
print(f"frame -> {OUT} {im.size}")
