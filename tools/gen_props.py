"""Generate the rigid-body props (src/ext/rigid.rs): three meshes and their textures.

  Meshes/apple.obj        a lathed apple, 9 cm across, with a stem cavity and a short stem;
                          centred on its origin (its collider is a ball about it)
  Meshes/chess_king.obj   a lathed king -- base, collar, body, crown -- with a small cross on
                          top, 14 cm tall, standing on its origin (a cylinder collider is
                          raised by half its height from there, ext/physics.rs `Shape`)
  Meshes/dice.obj         a 6 cm die: 24 vertices, four per face, each face mapped to a cell
                          of a 3 x 2 pip atlas; opposite faces sum to seven
  Textures/apple.bmp      red skin with a yellow blush on one side and lenticel speckles,
                          a brown band where the stem is
  Textures/chess_wood.bmp dark wood, grain along the piece
  Textures/dice.bmp       the pip atlas: six white cells, black pips

Mesh dialect is the engine's (Mesh.cpp): `v`, `vt`, `f a/at b/bt c/ct [d/dt]`, quads allowed,
counter-clockwise seen from outside (the engine culls back faces, engine.rs), flat normals
recomputed at load -- so the lathes carry enough segments that the facets do not show at
arm's length. No `c` lines: these collide through their rigid bodies, never through the
ported pass.

UV convention: the engine's BMP loader leaves GL t=0 at the image TOP (Texture.cpp:27-41
reads rows bottom-first into the top of the buffer), so `v = 0` is the top row of the PIL
image here, and the textures are painted top-origin. The BMPs are 24-bit, bottom-first rows,
a 54-byte header at data offset 54 -- what `decode_bmp` reads and what `gen_ui.py` writes.

Run from the repository root: `python3 tools/gen_props.py`. Needs numpy and Pillow.
"""
import math
import struct

import numpy as np
from PIL import Image, ImageDraw

rng = np.random.default_rng(11)

# ── Writers ──────────────────────────────────────────────────────────────────────────────────


def write_bmp24(path, im):
    """24-bit uncompressed BMP, BGR, bottom-first rows padded to four bytes, 54-byte header."""
    im = im.convert("RGB")
    w, h = im.size
    px = np.asarray(im, dtype=np.uint8)[:, :, ::-1]  # RGB -> BGR
    pad = (4 - (w * 3) % 4) % 4
    rows = []
    for y in range(h - 1, -1, -1):  # bottom-first
        rows.append(px[y].tobytes() + b"\0" * pad)
    data = b"".join(rows)
    hdr = struct.pack("<2sIHHI", b"BM", 54 + len(data), 0, 0, 54)
    hdr += struct.pack("<IiiHHIIiiII", 40, w, h, 1, 24, 0, len(data), 2835, 2835, 0, 0)
    with open(path, "wb") as f:
        f.write(hdr + data)


def write_obj(path, header, verts, uvs, faces):
    """`faces` are tuples of (vertex, uv) index pairs, 0-based, 3 or 4 long."""
    with open(path, "w") as f:
        for line in header:
            f.write("# " + line + "\n")
        for v in verts:
            f.write("v %.5f %.5f %.5f\n" % tuple(v))
        for t in uvs:
            f.write("vt %.5f %.5f\n" % tuple(t))
        for face in faces:
            f.write("f " + " ".join("%d/%d" % (a + 1, b + 1) for a, b in face) + "\n")


# ── Lathe ────────────────────────────────────────────────────────────────────────────────────


def lathe(profile, segments):
    """Revolve a (y, r) profile about +y. `v` runs 0 at the profile's first point to 1 at its
    last, by arc length; `u` runs once round. Returns verts, uvs, faces (quads, CCW from
    outside for a profile traced from the top down its outside), with the seam duplicated so
    the texture does not wrap backwards across it. A profile point with r == 0 is a pole:
    its ring collapses to a triangle fan."""
    profile = [(float(y), float(r)) for y, r in profile]
    arc = [0.0]
    for (y0, r0), (y1, r1) in zip(profile, profile[1:]):
        arc.append(arc[-1] + math.hypot(y1 - y0, r1 - r0))
    total = arc[-1]
    verts, uvs, faces = [], [], []
    ring = []
    for (y, r), a in zip(profile, arc):
        v = a / total
        row = []
        for i in range(segments + 1):
            u = i / segments
            th = 2.0 * math.pi * u
            verts.append((r * math.cos(th), y, r * math.sin(th)))
            uvs.append((u, v))
            row.append(len(verts) - 1)
        ring.append((row, r))
    for (row0, r0), (row1, r1) in zip(ring, ring[1:]):
        for i in range(segments):
            a, b = row0[i], row0[i + 1]
            c, d = row1[i + 1], row1[i]
            # Counter-clockwise seen from outside, for a profile traced with the solid on its
            # RIGHT in the (r, y) half-plane -- from the top down the outside, as both
            # profiles below are drawn. A pole collapses the quad to the triangle that is left.
            if r0 == 0.0:
                faces.append([(a, a), (c, c), (d, d)])
            elif r1 == 0.0:
                faces.append([(a, a), (b, b), (c, c)])
            else:
                faces.append([(a, a), (b, b), (c, c), (d, d)])
    return verts, uvs, faces, arc, total


def smooth_profile(points, n):
    """Resample a hand-drawn (y, r) polyline to `n` points by arc length, through a
    Catmull-Rom spline, so a lathe has evenly sized facets and no kinks."""
    pts = np.array(points, dtype=np.float64)
    seg = np.hypot(*np.diff(pts, axis=0).T)
    t = np.concatenate([[0.0], np.cumsum(seg)])
    t /= t[-1]
    out = []
    for s in np.linspace(0.0, 1.0, n):
        k = min(int(np.searchsorted(t, s, side="right")) - 1, len(pts) - 2)
        k = max(k, 0)
        p0 = pts[max(k - 1, 0)]
        p1 = pts[k]
        p2 = pts[k + 1]
        p3 = pts[min(k + 2, len(pts) - 1)]
        w = (s - t[k]) / (t[k + 1] - t[k]) if t[k + 1] > t[k] else 0.0
        a = 2 * p1
        b = p3 * 0 + (p2 - p0)
        c = 2 * p0 - 5 * p1 + 4 * p2 - p3
        d = -p0 + 3 * p1 - 3 * p2 + p3
        p = 0.5 * (a + b * w + c * w * w + d * w * w * w)
        out.append((p[0], max(p[1], 0.0)))
    out[0] = (points[0][0], points[0][1])
    out[-1] = (points[-1][0], points[-1][1])
    return out


# ── Apple ────────────────────────────────────────────────────────────────────────────────────

# (y, r) from the stem's tip down the cavity, over the shoulder, round the belly to the base
# dimple. 9 cm across, 8 cm tall plus the stem; the origin is the belly's centre, which is
# where the ball collider sits.
APPLE_PROFILE = [
    (0.052, 0.0000),  # stem tip
    (0.051, 0.0022),
    (0.038, 0.0026),
    (0.030, 0.0030),  # stem foot, in the cavity
    (0.029, 0.0060),
    (0.033, 0.0120),  # cavity wall climbing to the rim
    (0.038, 0.0210),  # rim
    (0.036, 0.0300),
    (0.028, 0.0385),
    (0.015, 0.0440),
    (0.000, 0.0450),  # widest, 9 cm
    (-0.015, 0.0435),
    (-0.028, 0.0370),
    (-0.036, 0.0240),
    (-0.040, 0.0110),
    (-0.038, 0.0040),  # base dimple
    (-0.037, 0.0000),
]
STEM_END = 3  # profile index where the stem meets the cavity

apple_prof = smooth_profile(APPLE_PROFILE, 36)
# The stem is the straight bit at the top: keep its first points as drawn so it stays thin.
apple_verts, apple_uvs, apple_faces, apple_arc, apple_len = lathe(apple_prof, 40)
# Where the stem ends along v, for the texture's brown band: the drawn profile's arc up to
# the stem's foot, over the resampled profile's total length.
stem_arc = sum(
    math.hypot(y1 - y0, r1 - r0)
    for (y0, r0), (y1, r1) in zip(APPLE_PROFILE[:STEM_END], APPLE_PROFILE[1 : STEM_END + 1])
)
STEM_V = stem_arc / apple_len
write_obj(
    "Meshes/apple.obj",
    ["EXT: generated by tools/gen_props.py -- a lathed apple, 9 cm across, centred on its origin.",
     "No colliders: it is a rigid body (src/ext/rigid.rs)."],
    apple_verts, apple_uvs, apple_faces,
)


def value_noise(shape, period, octaves=4, seed=0, stretch=1.0):
    """Tileable-in-u value noise, for the skin's mottle and the wood's grain. `stretch`
    divides the lattice period along v, so features run `stretch` times longer down the
    image than across it -- a grain."""
    h, w = shape
    out = np.zeros((h, w), np.float64)
    amp, total, p = 1.0, 0.0, period
    r = np.random.default_rng(seed)
    for _ in range(octaves):
        ph, pw = max(2, int(p / stretch)), max(2, int(p))
        lattice = r.random((ph + 1, pw))
        ys = np.linspace(0, ph, h, endpoint=False)
        xs = np.linspace(0, pw, w, endpoint=False)
        y0 = np.floor(ys).astype(int)
        x0 = np.floor(xs).astype(int)
        fy = (ys - y0)[:, None]
        fx = (xs - x0)[None, :]
        fy = fy * fy * (3 - 2 * fy)
        fx = fx * fx * (3 - 2 * fx)
        y1 = np.minimum(y0 + 1, ph)
        x1 = (x0 + 1) % pw
        a = lattice[y0][:, x0]
        b = lattice[y0][:, x1]
        c = lattice[y1][:, x0]
        d = lattice[y1][:, x1]
        out += ((a * (1 - fx) + b * fx) * (1 - fy) + (c * (1 - fx) + d * fx) * fy) * amp
        total += amp
        amp *= 0.5
        p *= 2
    return out / total


W = H = 256
u = np.linspace(0, 1, W, endpoint=False)[None, :]
v = np.linspace(0, 1, H, endpoint=False)[:, None]
# Red skin, a yellow-green blush on the side that faced the sun (one side in u), streaked
# along v as a skin is, and lenticels: small pale speckles.
blush = 0.5 + 0.5 * np.cos(2 * math.pi * (u - 0.3))
blush = np.clip(blush * 1.4 - 0.5, 0, 1) * (0.6 + 0.4 * value_noise((H, W), 6, seed=3))
streak = value_noise((H, W), 24, octaves=3, seed=4) * 0.12
red = np.array([0.72, 0.08, 0.06])
yellow = np.array([0.88, 0.72, 0.22])
skin = red[None, None, :] * (1 - blush[..., None]) + yellow[None, None, :] * blush[..., None]
skin = skin * (0.92 + streak[..., None])
# Lenticels: a few hundred pale dots, one to two pixels, soft-edged, scattered over the skin
# (not over the stem band).
speck = np.zeros((H, W))
yy, xx = np.mgrid[0:H, 0:W]
for _ in range(420):
    cx, cy = rng.random() * W, STEM_V * H + rng.random() * (1 - STEM_V - 0.02) * H
    r = 0.9 + rng.random() * 1.1
    d2 = ((xx - cx) ** 2 + (yy - cy) ** 2) / (r * r)
    speck = np.maximum(speck, np.clip(1.4 - d2, 0, 1) * (0.5 + 0.5 * rng.random()))
speck = speck[..., None]
skin = skin * (1 - speck) + np.array([0.96, 0.86, 0.62])[None, None, :] * speck
# Darker toward the cavity rim and the base, as apples are.
shade = 1.0 - 0.35 * np.clip((STEM_V + 0.08 - v) / 0.08, 0, 1) - 0.25 * np.clip((v - 0.9) / 0.1, 0, 1)
skin = skin * shade[..., None]
# The stem: a brown band over the first STEM_V of v.
stem = np.clip((STEM_V - v) / 0.01, 0, 1)[..., None]
brown = np.array([0.30, 0.20, 0.10])[None, None, :] * (0.8 + 0.4 * value_noise((H, W), 12, seed=6))[..., None]
skin = skin * (1 - stem) + brown * stem
write_bmp24("Textures/apple.bmp", Image.fromarray((np.clip(skin, 0, 1) * 255).astype(np.uint8)))

# ── Chess king ───────────────────────────────────────────────────────────────────────────────

# (y, r) from the base up -- reversed below, since the lathe wants the top first: base, a
# bead, the body tapering, a collar, the crown, closing at the top where the cross stands.
# 13.6 cm to the crown, 15 with the cross.
KING_PROFILE = [
    (0.000, 0.0000),
    (0.000, 0.0300),  # base, 6 cm across
    (0.010, 0.0300),
    (0.016, 0.0260),
    (0.022, 0.0240),  # bead
    (0.028, 0.0250),
    (0.034, 0.0215),
    (0.045, 0.0175),  # body
    (0.065, 0.0140),
    (0.085, 0.0120),
    (0.100, 0.0118),
    (0.105, 0.0160),  # collar
    (0.111, 0.0165),
    (0.116, 0.0120),
    (0.121, 0.0135),  # crown
    (0.128, 0.0130),
    (0.134, 0.0080),
    (0.136, 0.0040),
    (0.136, 0.0000),
]
king_prof = smooth_profile(KING_PROFILE[::-1], 44)
king_verts, king_uvs, king_faces, _, _ = lathe(king_prof, 36)


def box(verts, uvs, faces, centre, half, uv_cell):
    """An axis-aligned box of 24 vertices, each face its own quad, all faces on `uv_cell`
    ((u0, v0), (u1, v1)) of the texture. CCW seen from outside."""
    cx, cy, cz = centre
    hx, hy, hz = half
    (u0, v0), (u1, v1) = uv_cell
    # Each face: four corners in CCW order seen from outside, as (sx, sy, sz) signs.
    FACES = [
        [(-1, -1, 1), (1, -1, 1), (1, 1, 1), (-1, 1, 1)],      # +z
        [(1, -1, -1), (-1, -1, -1), (-1, 1, -1), (1, 1, -1)],  # -z
        [(1, -1, 1), (1, -1, -1), (1, 1, -1), (1, 1, 1)],      # +x
        [(-1, -1, -1), (-1, -1, 1), (-1, 1, 1), (-1, 1, -1)],  # -x
        [(-1, 1, 1), (1, 1, 1), (1, 1, -1), (-1, 1, -1)],      # +y
        [(-1, -1, -1), (1, -1, -1), (1, -1, 1), (-1, -1, 1)],  # -y
    ]
    corner_uv = [(u0, v1), (u1, v1), (u1, v0), (u0, v0)]
    for face in FACES:
        quad = []
        for (sx, sy, sz), t in zip(face, corner_uv):
            verts.append((cx + sx * hx, cy + sy * hy, cz + sz * hz))
            uvs.append(t)
            quad.append((len(verts) - 1, len(uvs) - 1))
        faces.append(quad)


# The cross: an upright and a bar, thin, from the crown's top.
box(king_verts, king_uvs, king_faces, (0, 0.143, 0), (0.0020, 0.0070, 0.0020), ((0.0, 0.0), (0.1, 0.1)))
box(king_verts, king_uvs, king_faces, (0, 0.145, 0), (0.0065, 0.0020, 0.0020), ((0.0, 0.0), (0.1, 0.1)))
write_obj(
    "Meshes/chess_king.obj",
    ["EXT: generated by tools/gen_props.py -- a lathed chess king, 14 cm, standing on its origin.",
     "No colliders: it is a rigid body (src/ext/rigid.rs)."],
    king_verts, king_uvs, king_faces,
)

# Dark wood: grain runs along v (the piece's height) -- noise stretched sixteen times
# longer down the image than across, in two scales, plus the slow colour wander of the
# rings across u.
rings = value_noise((H, W), 3, octaves=2, seed=7)
streaks = value_noise((H, W), 48, octaves=3, seed=8, stretch=16.0)
fine = value_noise((H, W), 128, octaves=2, seed=9, stretch=16.0)
grain = 0.35 * rings + 0.45 * streaks + 0.20 * fine
dark = np.array([0.16, 0.09, 0.05])
light = np.array([0.34, 0.20, 0.10])
wood = dark[None, None, :] * (1 - grain[..., None]) + light[None, None, :] * grain[..., None]
# A varnish sheen band so the lathe's vertical facets catch a little variation.
wood = wood * (0.9 + 0.2 * np.cos(2 * math.pi * u * 2.0 + 1.0))[..., None]
write_bmp24("Textures/chess_wood.bmp", Image.fromarray((np.clip(wood, 0, 1) * 255).astype(np.uint8)))

# ── Dice ─────────────────────────────────────────────────────────────────────────────────────

HALF = 0.03
CELL = 128
# Face -> pip count. Opposite faces sum to seven: +y/-y = 1/6, +x/-x = 2/5, +z/-z = 3/4.
PIPS = {"+y": 1, "-y": 6, "+x": 2, "-x": 5, "+z": 3, "-z": 4}
# Atlas cell of each pip count: 1 2 3 on the top row, 4 5 6 below.
def cell(n):
    col, row = (n - 1) % 3, (n - 1) // 3
    return ((col / 3.0, row / 2.0), ((col + 1) / 3.0, (row + 1) / 2.0))


dice_verts, dice_uvs, dice_faces = [], [], []
DICE_FACES = {
    "+z": [(-1, -1, 1), (1, -1, 1), (1, 1, 1), (-1, 1, 1)],
    "-z": [(1, -1, -1), (-1, -1, -1), (-1, 1, -1), (1, 1, -1)],
    "+x": [(1, -1, 1), (1, -1, -1), (1, 1, -1), (1, 1, 1)],
    "-x": [(-1, -1, -1), (-1, -1, 1), (-1, 1, 1), (-1, 1, -1)],
    "+y": [(-1, 1, 1), (1, 1, 1), (1, 1, -1), (-1, 1, -1)],
    "-y": [(-1, -1, -1), (1, -1, -1), (1, -1, 1), (-1, -1, 1)],
}
for name, corners in DICE_FACES.items():
    (u0, v0), (u1, v1) = cell(PIPS[name])
    corner_uv = [(u0, v1), (u1, v1), (u1, v0), (u0, v0)]
    quad = []
    for (sx, sy, sz), t in zip(corners, corner_uv):
        dice_verts.append((sx * HALF, sy * HALF, sz * HALF))
        dice_uvs.append(t)
        quad.append((len(dice_verts) - 1, len(dice_uvs) - 1))
    dice_faces.append(quad)
assert len(dice_verts) == 24
write_obj(
    "Meshes/dice.obj",
    ["EXT: generated by tools/gen_props.py -- a 6 cm die, 24 vertices, each face on a cell of",
     "Textures/dice.bmp (3 x 2, pips 1-6, opposite faces summing to seven). No colliders: it is",
     "a rigid body (src/ext/rigid.rs)."],
    dice_verts, dice_uvs, dice_faces,
)

# The atlas: six cells, white with a faint warm tint and a darker rim, black pips laid out
# as on a die.
PIP_AT = {
    1: [(0.5, 0.5)],
    2: [(0.25, 0.25), (0.75, 0.75)],
    3: [(0.25, 0.25), (0.5, 0.5), (0.75, 0.75)],
    4: [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)],
    5: [(0.25, 0.25), (0.75, 0.25), (0.5, 0.5), (0.25, 0.75), (0.75, 0.75)],
    6: [(0.25, 0.25), (0.75, 0.25), (0.25, 0.5), (0.75, 0.5), (0.25, 0.75), (0.75, 0.75)],
}
atlas = Image.new("RGB", (3 * CELL, 2 * CELL), (246, 242, 232))
draw = ImageDraw.Draw(atlas)
for n in range(1, 7):
    col, row = (n - 1) % 3, (n - 1) // 3
    x0, y0 = col * CELL, row * CELL
    # A rim a shade darker, so the edges of the cube read as edges.
    draw.rectangle([x0, y0, x0 + CELL - 1, y0 + CELL - 1], outline=(200, 194, 180), width=3)
    for fx, fy in PIP_AT[n]:
        cx, cy = x0 + fx * CELL, y0 + fy * CELL
        r = CELL * 0.085
        draw.ellipse([cx - r, cy - r, cx + r, cy + r], fill=(18, 16, 16))
        # A highlight: pips are concave, and the lit edge is the far one.
        draw.ellipse([cx - r * 0.5, cy - r * 0.75, cx + r * 0.1, cy - r * 0.3], fill=(60, 58, 58))
write_bmp24("Textures/dice.bmp", atlas)

print("apple: %d verts, %d faces (stem ends at v = %.3f)" % (len(apple_verts), len(apple_faces), STEM_V))
print("king:  %d verts, %d faces" % (len(king_verts), len(king_faces)))
print("dice:  %d verts, %d faces" % (len(dice_verts), len(dice_faces)))
