"""Turn the converted Relativity model into a WALKABLE level.

The engine collides hit spheres against rectangle colliders (`c` lines) at 500 Hz, so the 660k
raw triangles can be neither the collision shape (wrong primitive) nor the collision budget
(~200x too slow). Instead this tool extracts a coarse collision shell from the visual mesh:

  1. keep only up-facing triangles (surfaces walkable under the player's gravity -- in a
     Relativity scene that is exactly one of the three stair systems; the other two read as
     walls and ceilings, which is the point of the room);
  2. rasterise them into a multi-layer heightfield (multi-layer because the building has
     overlapping storeys -- one height per cell is not enough);
  3. estimate the stair riser from adjacent-cell height steps, and scale the whole model so
     that risers land at a walkable ~0.17 world units;
  4. blur each layer so stair steps become RAMPS: the engine cannot climb steps (the foot
     sphere hits the riser and stops -- the original demo only ever uses slopes, never stairs),
     but it provably climbs slopes up to the ground-normal threshold (Player.cpp:112);
  5. emit one exactly-orthogonal rectangle collider per cell-layer, tilted to the local
     gradient, slightly oversized so neighbours overlap and leave no seams.

The output overwrites Meshes/escher_relativity.obj with the rescaled visual mesh plus the
collider shell appended as `c` lines, all in the engine's own dialect.
"""
import math, os, sys
from collections import defaultdict

SRC = "Meshes/escher_relativity.obj"
UP_DOT       = 0.82     # min normalised n.y for a face to count as walkable
TARGET_RISER = 0.17     # desired stair riser in world units after rescale
CELL_WORLD   = 0.42     # collider cell size in world units after rescale
LAYER_GAP    = 0.35     # pre-scale: cell samples further apart than this are separate storeys
BLUR_PASSES  = 2
MAX_COLLIDERS = 3200
SCALE_CLAMP  = (2.0, 8.0)

# ── Load ────────────────────────────────────────────────────────────────────────────────────
verts, faces = [], []
for line in open(SRC):
    if line.startswith("v "):
        p = line.split()
        verts.append((float(p[1]), float(p[2]), float(p[3])))
    elif line.startswith("f "):
        p = line.split()
        faces.append((int(p[1]) - 1, int(p[2]) - 1, int(p[3]) - 1))
print(f"loaded {len(verts)} verts, {len(faces)} tris")

def walkable_tris():
    for a, b, c in faces:
        va, vb, vc = verts[a], verts[b], verts[c]
        ux, uy, uz = vb[0]-va[0], vb[1]-va[1], vb[2]-va[2]
        wx, wy, wz = vc[0]-va[0], vc[1]-va[1], vc[2]-va[2]
        nx, ny, nz = uy*wz - uz*wy, uz*wx - ux*wz, ux*wy - uy*wx
        m = math.sqrt(nx*nx + ny*ny + nz*nz)
        if m > 1e-12 and ny / m > UP_DOT:
            yield va, vb, vc

# ── Rasterise into a multi-layer heightfield at a given cell size (pre-scale units) ────────
def build_field(cell):
    field = defaultdict(list)   # (ix, iz) -> [y, y, ...]
    for va, vb, vc in walkable_tris():
        xs = [va[0], vb[0], vc[0]]; zs = [va[2], vb[2], vc[2]]
        ix0, ix1 = int(min(xs)//cell), int(max(xs)//cell)
        iz0, iz1 = int(min(zs)//cell), int(max(zs)//cell)
        d = (vb[0]-va[0])*(vc[2]-va[2]) - (vc[0]-va[0])*(vb[2]-va[2])
        if abs(d) < 1e-12:
            continue
        for ix in range(ix0, ix1+1):
            for iz in range(iz0, iz1+1):
                px, pz = (ix+0.5)*cell, (iz+0.5)*cell
                w1 = ((vb[0]-px)*(vc[2]-pz) - (vc[0]-px)*(vb[2]-pz)) / d
                w2 = ((vc[0]-px)*(va[2]-pz) - (va[0]-px)*(vc[2]-pz)) / d
                w3 = 1.0 - w1 - w2
                if w1 < -0.05 or w2 < -0.05 or w3 < -0.05:
                    continue
                field[(ix, iz)].append(w1*va[1] + w2*vb[1] + w3*vc[1])
    # cluster each cell's samples into storey layers (topmost of each cluster wins)
    layers = {}
    for k, ys in field.items():
        ys.sort()
        out = [ys[0]]
        for y in ys[1:]:
            if y - out[-1] > LAYER_GAP:
                out.append(y)
            else:
                out[-1] = y   # keep the highest sample of the cluster
        layers[k] = out
    return layers

# ── Pass 1: estimate the stair riser on a fine grid ────────────────────────────────────────
fine = build_field(0.05)
diffs = []
for (ix, iz), ys in fine.items():
    for (jx, jz) in ((ix+1, iz), (ix, iz+1)):
        for y in ys:
            for y2 in fine.get((jx, jz), []):
                d = abs(y2 - y)
                if 0.015 < d < 0.12:
                    diffs.append(d)
diffs.sort()
riser = diffs[len(diffs)//2] if diffs else 0.035
scale = max(SCALE_CLAMP[0], min(SCALE_CLAMP[1], TARGET_RISER / riser))
print(f"estimated riser {riser:.3f} (pre-scale, {len(diffs)} samples) -> scale x{scale:.2f}"
      f" -> riser {riser*scale:.3f} world")

# ── Pass 2: the real field at collider resolution ──────────────────────────────────────────
cell = CELL_WORLD / scale
layers = build_field(cell)
while sum(len(v) for v in layers.values()) > MAX_COLLIDERS:
    cell *= 1.25
    layers = build_field(cell)
n_coll = sum(len(v) for v in layers.values())
print(f"heightfield: {len(layers)} cells, {n_coll} cell-layers at cell {cell*scale:.2f} world")

# ── Blur each layer into ramps (stairs are unclimbable; slopes are proven) ─────────────────
def neighbours(k, y):
    ix, iz = k
    for dx in (-1, 0, 1):
        for dz in (-1, 0, 1):
            for y2 in layers.get((ix+dx, iz+dz), []):
                if abs(y2 - y) < LAYER_GAP:
                    yield (dx, dz, y2)

for _ in range(BLUR_PASSES):
    new = {}
    for k, ys in layers.items():
        new[k] = [sum(y2 for _, _, y2 in neighbours(k, y)) /
                  max(1, sum(1 for _ in neighbours(k, y))) for y in ys]
    layers = new

# ── Emit: rescaled visual mesh + collider shell ────────────────────────────────────────────
lines = []
lines.append("# Escher Relativity by Benoit Gagnier -- CC-BY-4.0")
lines.append("# https://sketchfab.com/3d-models/escher-relativity-6dccd307edce45f19c032378b4d10933")
lines.append("# Rescaled to walkable architecture scale and fitted with a rectangle-collider")
lines.append("# walk shell by tools/gen_walkable.py. `c` lines are the collision; stairs are")
lines.append("# blurred into ramps because the engine climbs slopes, not steps.")
for v in verts:
    lines.append(f"v {v[0]*scale:.4f} {v[1]*scale:.4f} {v[2]*scale:.4f}")
for a, b, c in faces:
    lines.append(f"f {a+1} {b+1} {c+1}")

vbase = len(verts)
cverts, clines = [], []
grown = 1.18   # oversize so neighbouring rectangles overlap
for (ix, iz), ys in sorted(layers.items()):
    for y in ys:
        # local gradient from same-layer neighbours
        gx = gz = 0.0; nx = nz = 0
        for dxi, dzi, y2 in neighbours((ix, iz), y):
            if dxi and not dzi: gx += (y2 - y) / (dxi * cell); nx += 1
            if dzi and not dxi: gz += (y2 - y) / (dzi * cell); nz += 1
        gx = gx / nx if nx else 0.0
        gz = gz / nz if nz else 0.0
        cx, cz = (ix + 0.5) * cell, (iz + 0.5) * cell
        # exactly-orthogonal in-plane axes (Collider::CreateSorted debug-asserts orthogonality)
        U = (1.0, gx, 0.0)
        V = (0.0, gz, 1.0)
        uu = 1.0 + gx * gx
        d = (V[1] * U[1]) / uu             # Gram-Schmidt: V -= U * (U.V / U.U)
        V = (V[0] - U[0]*d, V[1] - U[1]*d, V[2] - U[2]*d)
        h = cell * 0.5 * grown
        U = (U[0]*h, U[1]*h, U[2]*h)
        vs = h / V[2] if abs(V[2]) > 1e-9 else h
        V = (V[0]*vs, V[1]*vs, V[2]*vs)
        C = (cx, y, cz)
        A = (C[0]-U[0]-V[0], C[1]-U[1]-V[1], C[2]-U[2]-V[2])
        B = (C[0]+U[0]-V[0], C[1]+U[1]-V[1], C[2]+U[2]-V[2])
        D = (C[0]-U[0]+V[0], C[1]-U[1]+V[1], C[2]-U[2]+V[2])
        i0 = vbase + len(cverts) + 1
        cverts += [A, B, D]
        clines.append((i0, i0+1, i0+2))
for v in cverts:
    lines.append(f"v {v[0]*scale:.4f} {v[1]*scale:.4f} {v[2]*scale:.4f}")
for a, b, c in clines:
    lines.append(f"c {a} {b} {c}")
open(SRC, "w").write("\n".join(lines) + "\n")
print(f"wrote {SRC}: {len(verts)+len(cverts)} verts, {len(faces)} tris, {len(clines)} colliders")

# ── Spawn suggestion: the flattest, lowest large region ────────────────────────────────────
best, best_score = None, -1
for (ix, iz), ys in layers.items():
    for y in ys:
        ns = list(neighbours((ix, iz), y))
        flat = [y2 for _, _, y2 in ns if abs(y2 - y) < 0.02]
        score = len(flat) - y * 2.0     # prefer big flat patches low in the building
        if score > best_score:
            best_score, best = score, ((ix+0.5)*cell*scale, y*scale, (iz+0.5)*cell*scale)
print(f"suggested spawn (world): ({best[0]:.2f}, {best[1]:.2f} + eye, {best[2]:.2f})")

# footprint after scale
xs = [v[0]*scale for v in verts]; zs = [v[2]*scale for v in verts]; ysv = [v[1]*scale for v in verts]
print(f"world bbox: x[{min(xs):.1f},{max(xs):.1f}] y[{min(ysv):.1f},{max(ysv):.1f}] z[{min(zs):.1f},{max(zs):.1f}]")

# max ramp steepness sanity: normal.y of steepest collider must clear the 0.7 ground threshold
steepest = 0.0
for (ix, iz), ys in layers.items():
    for y in ys:
        for dxi, dzi, y2 in neighbours((ix, iz), y):
            if abs(dxi) + abs(dzi) == 1:
                steepest = max(steepest, abs(y2 - y) / cell)
ny = 1.0 / math.sqrt(1.0 + steepest * steepest)
print(f"steepest ramp gradient {steepest:.2f} -> surface normal.y {ny:.2f} "
      f"({'CLIMBABLE' if ny > 0.7 else 'TOO STEEP in places -- those ramps act as walls'})")
