"""Generate the Escher gallery assets for scene `]`.

Emits:
  Meshes/penrose_stairs.obj   -- the impossible-staircase monument (visible)
  Meshes/gallery_bounds.obj   -- invisible collider ring (inner + outer octagon walls)
  Meshes/bounds.obj           -- reusable colliders-only unit box for other scenes' bounds
  scratchpad wireframe preview of the monument from the design eye

# The illusion

Four flights of stairs ascend around a diamond footprint and appear to close into a loop that
rises forever. Physically the loop cannot close: after four ascending flights the end sits H
above the start. The gap is hidden by PROJECTION: the loop's start point B is placed not at the
near corner N, but back along the design sight-line through N, lower and farther, such that B
projects exactly onto A (the loop's end, at N). From the design eye the two coincide and the
staircase reads as a closed ascending cycle.

Because projection maps straight lines to straight lines, every flight built between the true
3D endpoints projects onto the clean flight the image needs -- only the endpoints must be
constructed, the interiors follow.

The alignment equation: B = Eye + t*(A - Eye) with t chosen so B.y = y_low. It holds exactly
for one eye position; the game keeps it true by (a) yaw-rotating the monument to face the
player every frame and (b) confining the player to a ring walkway, so distance and eye height
stay near the design values.
"""
import math, os

# ── Design constants (mirrored in src/level13.rs) ────────────────────────────────────────────
EYE_H    = 1.5          # GH_PLAYER_HEIGHT
R_VIEW   = 7.0          # design viewing distance (centre of the ring walkway)
S        = 3.2          # footprint square side
C        = S * math.sqrt(2) / 2   # diamond corner radius = 2.263
Y_LOW    = 0.10         # walk height at the loop's start (B)
Y_HIGH   = 0.70         # walk height at the loop's end (A) -- gap H = 0.60
STEPS    = 5            # steps per flight
TREAD_W  = 0.55         # walkway width
STEP_TH  = 0.12         # step slab thickness
R_INNER  = 5.4          # inner collider ring (keeps player off the monument)
R_OUTER  = 8.5          # outer collider ring (keeps player near the design distance)
WALL_H   = 4.0

EYE = (0.0, EYE_H, R_VIEW)
N = (0.0, Y_HIGH, C)         # loop end A sits at the near corner
E = ( C, 0.0, 0.0)           # heights assigned below
F = (0.0, 0.0, -C)
W = (-C, 0.0, 0.0)

# B: on the eye->A ray, at walk height Y_LOW.
t = (Y_LOW - EYE[1]) / (N[1] - EYE[1])
B = (EYE[0] + t * (N[0] - EYE[0]), Y_LOW, EYE[2] + t * (N[2] - EYE[2]))
assert abs(B[0]) < 1e-9
assert -C < B[2] < C, f"B.z={B[2]:.2f} fell outside the footprint -- retune Y_LOW/Y_HIGH"

# Corner heights: four equal-rise flights B -> E -> F -> W -> A.
H = Y_HIGH - Y_LOW
rise = H / 4
corners = [B, (E[0], Y_LOW + rise, E[2]), (F[0], Y_LOW + 2*rise, F[2]),
           (W[0], Y_LOW + 3*rise, W[2]), (N[0], Y_HIGH, N[2])]

# ── Verify the seam closes from the design eye ───────────────────────────────────────────────
def project(p):
    # Perspective onto the z=const image plane through the monument centre; direction only.
    d = (p[0]-EYE[0], p[1]-EYE[1], p[2]-EYE[2])
    return (d[0]/-d[2], d[1]/-d[2])   # eye looks down -Z

pa, pb = project(corners[4]), project(corners[0])
err = math.hypot(pa[0]-pb[0], pa[1]-pb[1])
print(f"seam projection error from design eye: {err:.2e}  (A and B must coincide)")
assert err < 1e-9, "seam does not close -- alignment math is wrong"

# ── Emit the monument: one slab box per step ─────────────────────────────────────────────────
verts, faces = [], []
def add_box(centre, u_half, w_half, y_top, y_bot):
    """A slab: u_half/w_half are horizontal half-extent vectors, top at y_top, bottom y_bot."""
    cx, cz = centre
    base = len(verts)
    for (su, sw) in [(-1,-1),(1,-1),(1,1),(-1,1)]:
        x = cx + su*u_half[0] + sw*w_half[0]
        z = cz + su*u_half[1] + sw*w_half[1]
        verts.append((x, y_bot, z))
    for (su, sw) in [(-1,-1),(1,-1),(1,1),(-1,1)]:
        x = cx + su*u_half[0] + sw*w_half[0]
        z = cz + su*u_half[1] + sw*w_half[1]
        verts.append((x, y_top, z))
    b = base + 1  # OBJ is 1-based
    # bottom (0123 viewed from below CCW), top (4567 from above), 4 sides
    faces.append((b+0, b+1, b+2, b+3)[::-1])       # bottom faces down
    faces.append((b+4, b+5, b+6, b+7))             # top faces up
    for i in range(4):
        j = (i+1) % 4
        faces.append((b+i, b+j, b+4+j, b+4+i))

for k in range(4):
    p0, p1 = corners[k], corners[k+1]
    leg = (p1[0]-p0[0], p1[2]-p0[2])
    leg_len = math.hypot(*leg)
    u = (leg[0]/leg_len, leg[1]/leg_len)
    w = (-u[1], u[0])
    step_len = leg_len / STEPS
    for i in range(STEPS):
        s0, s1 = i*step_len, (i+1)*step_len
        cx = p0[0] + u[0]*(s0+s1)/2
        cz = p0[2] + u[1]*(s0+s1)/2
        y_top = p0[1] + (p1[1]-p0[1]) * (i+1)/STEPS
        add_box((cx, cz), (u[0]*step_len/2, u[1]*step_len/2),
                (w[0]*TREAD_W/2, w[1]*TREAD_W/2), y_top, y_top - STEP_TH)

with open("Meshes/penrose_stairs.obj", "w") as f:
    f.write("# EXT asset: procedurally generated Escher-style impossible staircase.\n")
    f.write("# Original geometry (tools/gen_escher.py), inspired by Penrose stairs / Escher's\n")
    f.write("# 'Ascending and Descending'. Not derived from any third-party model.\n")
    f.write("# No colliders: the monument is viewed from the gallery ring, never walked on.\n")
    for v in verts:
        f.write(f"v {v[0]:.4f} {v[1]:.4f} {v[2]:.4f}\n")
    f.write("vt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\n")
    for q in faces:
        f.write(f"f {q[0]}/1 {q[1]}/2 {q[2]}/3 {q[3]}/4\n")
print(f"penrose_stairs.obj: {len(verts)} verts, {len(faces)} quads ({4*STEPS} steps)")

# ── Gallery bounds: two octagons of collider-only walls ─────────────────────────────────────
bverts, bcolls = [], []
def add_wall(p0, p1):
    base = len(bverts)
    bverts.append((p0[0], 0.0,    p0[1]))
    bverts.append((p1[0], 0.0,    p1[1]))
    bverts.append((p1[0], WALL_H, p1[1]))
    bverts.append((p0[0], WALL_H, p0[1]))
    bcolls.append((base+1, base+2, base+3))   # right triangle -> Collider expands to the rect

for radius in (R_INNER, R_OUTER):
    # Octagon circumscribed so the flat sides sit AT the radius.
    r = radius / math.cos(math.pi/8)
    pts = [(r*math.sin(a), r*math.cos(a)) for a in
           [ (i + 0.5) * math.pi/4 for i in range(8) ]]
    for i in range(8):
        add_wall(pts[i], pts[(i+1) % 8])

with open("Meshes/gallery_bounds.obj", "w") as f:
    f.write("# EXT asset: colliders-only (no faces) -> invisible collision geometry.\n")
    f.write("# Two octagonal walls confining the player to the gallery ring: inner keeps them\n")
    f.write("# off the monument, outer keeps them near the illusion's design distance.\n")
    for v in bverts:
        f.write(f"v {v[0]:.4f} {v[1]:.4f} {v[2]:.4f}\n")
    for c in bcolls:
        f.write(f"c {c[0]} {c[1]} {c[2]}\n")
print(f"gallery_bounds.obj: {len(bcolls)} collider walls (invisible)")

# ── Reusable unit bounds box for the other scenes ───────────────────────────────────────────
with open("Meshes/bounds.obj", "w") as f:
    f.write("# EXT asset: colliders-only (no faces) -> invisible collision geometry.\n")
    f.write("# Unit box x[-1,1] y[0,1] z[-1,1]; scale per scene. Keeps players and thrown\n")
    f.write("# objects in bounds without adding any visible geometry.\n")
    V = [(-1,0,-1),(1,0,-1),(1,0,1),(-1,0,1),(-1,1,-1),(1,1,-1),(1,1,1),(-1,1,1)]
    for v in V:
        f.write(f"v {v[0]} {v[1]} {v[2]}\n")
    # four walls + ceiling (no floor: scenes provide their own ground)
    for c in [(1,5,8),(2,6,7),(1,2,6),(4,3,7),(5,6,7)]:
        f.write(f"c {c[0]} {c[1]} {c[2]}\n")
print("bounds.obj: 5 collider walls (invisible unit box)")

# ── Wireframe preview from the design eye ───────────────────────────────────────────────────
Wd = Hd = 512
img = bytearray(Wd*Hd)
def to_px(p):
    u, v = project(p)
    return (int(Wd/2 + u*Wd*1.1), int(Hd/2 - (v + 0.13)*Hd*1.6))
def line(a, b):
    x0,y0 = a; x1,y1 = b
    n = max(abs(x1-x0), abs(y1-y0), 1)
    for i in range(n+1):
        x = x0 + (x1-x0)*i//n; y = y0 + (y1-y0)*i//n
        if 0 <= x < Wd and 0 <= y < Hd: img[y*Wd+x] = 255
for q in faces:
    for i in range(4):
        line(to_px(verts[q[i]-1]), to_px(verts[q[(i+1)%4]-1]))
out = "/private/tmp/claude-501/-Users-jeanrojas-backrooms/f49bbcb3-72f9-407d-bb41-23a32828ec79/scratchpad/escher_preview.pgm"
with open(out, "wb") as f:
    f.write(f"P5 {Wd} {Hd} 255\n".encode()); f.write(bytes(img))
print(f"preview: {out}")
print("OK")
