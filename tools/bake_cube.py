"""Bake a cube 'sticker' for scene `[`.

The panel is a flat decal on a flat wall -- an undistorted picture of a cube, not an anamorphic
smear. What makes it an illusion is not distortion but SIZE: the decal subtends exactly the angle
a real cube would at the spot it depicts, so from the station point it reads as an object hanging
in the room rather than a picture hanging on the wall.

Method: for each texel of the wall panel, take its world position, cast a ray from the station
point through it, and intersect the cube. That yields whatever mild keystone the geometry implies
(here under 1%, since the sight-line is only ~7 degrees off perpendicular) without ever having to
reason about it.

Also renders a verification view from the station point.
"""
import math, struct

TEX = 512

# ── MUST stay in step with the constants at the top of src/level12.rs ────────────────────────
STATION     = (0.0, 1.5, 6.0)         # player eye at the marked spot
CUBE        = (0.0, 2.3, 0.0)         # where the cube appears to hang
CUBE_HALF   = 0.7
CUBE_RY     = math.pi / 4             # 45 deg, so two side faces show
CUBE_RX     = 0.61547971              # atan(1/sqrt2) -- isometric tilt, shows the top
WALL_Z      = -4.98                   # decal sits a hair proud of the z = -5 wall
STICKER_Y   = 2.964                   # where the eye->cube sight-line meets the wall
STICKER_HALF = 2.5
CHECKER     = 4
LIGHT       = (0.36, 0.80, 0.48)      # LIGHT in Shaders/texture.frag

def rot_y(a, v):
    c, s = math.cos(a), math.sin(a)
    return (c*v[0] + s*v[2], v[1], -s*v[0] + c*v[2])
def rot_x(a, v):
    c, s = math.cos(a), math.sin(a)
    return (v[0], c*v[1] - s*v[2], s*v[1] + c*v[2])
def to_local(v): return rot_x(-CUBE_RX, rot_y(-CUBE_RY, v))
def to_world(v): return rot_y(CUBE_RY, rot_x(CUBE_RX, v))
def sub(a, b): return tuple(a[i]-b[i] for i in range(3))
def norm(v):
    m = math.sqrt(sum(c*c for c in v));  return tuple(c/m for c in v)

def trace(direction):
    """Cast from STATION; return (b,g,r) of the cube surface hit, or None."""
    o_l = tuple(c/CUBE_HALF for c in to_local(sub(STATION, CUBE)))
    d_l = tuple(c/CUBE_HALF for c in to_local(direction))
    tmin, tmax, axis, sign = -1e30, 1e30, 0, 1.0
    for k in range(3):
        if abs(d_l[k]) < 1e-9:
            if o_l[k] < -1.0 or o_l[k] > 1.0: return None
            continue
        t1 = (-1.0 - o_l[k]) / d_l[k]
        t2 = ( 1.0 - o_l[k]) / d_l[k]
        s = -1.0
        if t1 > t2: t1, t2, s = t2, t1, 1.0
        if t1 > tmin: tmin, axis, sign = t1, k, s
        tmax = min(tmax, t2)
        if tmin > tmax: return None
    if tmax < max(tmin, 0.0): return None
    p = tuple(o_l[k] + tmin*d_l[k] for k in range(3))
    u_ax, v_ax = [(1,2),(0,2),(0,1)][axis]
    cu = int((p[u_ax]*0.5 + 0.5) * CHECKER)
    cv = int((p[v_ax]*0.5 + 0.5) * CHECKER)
    n_local = [0.0,0.0,0.0]; n_local[axis] = sign
    n = norm(to_world(tuple(n_local)))
    shade = sum(n[k]*LIGHT[k] for k in range(3))*0.5 + 0.5
    base = 0.95 if (cu+cv) % 2 == 0 else 0.30
    val = max(int(255 * min(base*shade, 1.0)), 16)
    return (val, val, val)

def sticker_point(u, v):
    """World position of decal texel (u,v), u,v in [-1,1].
    The quad is turned to face +Z (euler.y = pi), which maps its local X to world -X."""
    return (-u*STICKER_HALF, STICKER_Y + v*STICKER_HALF, WALL_Z)

def write_bmp(path, rows, w, h):
    data = b"".join(rows)
    hdr  = struct.pack("<2sIHHI", b"BM", 54+len(data), 0, 0, 54)
    hdr += struct.pack("<IiiHHIIiiII", 40, w, h, 1, 24, 0, len(data), 2835, 2835, 0, 0)
    open(path, "wb").write(hdr + data)

rows, hit = [], 0
for j in range(TEX):                      # j = 0 is the BMP bottom row
    v = 2.0*(j + 0.5)/TEX - 1.0
    row = bytearray()
    for i in range(TEX):
        u = 2.0*(i + 0.5)/TEX - 1.0
        c = trace(norm(sub(sticker_point(u, v), STATION)))
        if c is None:
            row += bytes((0,0,0))         # key colour -> discarded by Shaders/cutout.frag
        else:
            row += bytes(c); hit += 1
    while len(row) % 4: row += b"\x00"
    rows.append(bytes(row))
write_bmp('Textures/cube_sticker.bmp', rows, TEX, TEX)
cov = hit/(TEX*TEX)
print(f"wrote Textures/cube_sticker.bmp: 512x512, cube covers {cov*100:.1f}%")

# ── Sanity: the projection must sit clear of the floor, or its lower half is hidden ─────────
ang = math.atan(CUBE_HALF*math.sqrt(3) / math.dist(STATION, CUBE))
reach = math.dist(STATION, (0.0, STICKER_Y, WALL_Z))
proj_r = math.tan(ang) * reach
low = STICKER_Y - proj_r
print(f"projection radius on the wall {proj_r:.2f}; lowest painted point y = {low:.2f}")
assert low > 0.05, "the cube image would sink below the floor and be clipped"
assert proj_r < STICKER_HALF, "the cube image would overrun the decal"
assert 0.15 < cov < 0.75, f"coverage {cov:.2f} looks wrong"
print("OK")
