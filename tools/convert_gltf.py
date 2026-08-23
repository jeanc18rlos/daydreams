"""Convert a glTF 2.0 model into the engine's OBJ dialect.

Written for Benoit Gagnier's "Escher Relativity" (CC-BY-4.0, via Sketchfab) but generic over any
single-material, position+index glTF: walks the node hierarchy, applies world transforms, merges
every primitive into one vertex/face soup, and emits `v` + bare `f a b c` lines.

Why so little is emitted:
  - No `vt`: the model carries no TEXCOORD_0. The engine's parser pushes (0,0) for missing UVs
    (Mesh.cpp:207-209), so a 1x1 texture gives a flat base colour.
  - No `vn`: the engine discards vn lines and recomputes flat per-face normals (Mesh.cpp:187).
    Flat shading suits the sculpture look.
  - No `c` lines: the monument is viewed from a gallery ring, never walked on.

Coordinate systems agree (both right-handed, +Y up, front faces CCW), so vertices pass through
with only the node transforms and a fit-to-target rescale applied.
"""
import json, math, os, struct, sys

SRC = "/private/tmp/claude-501/-Users-jeanrojas-backrooms/f49bbcb3-72f9-407d-bb41-23a32828ec79/scratchpad/escher/scene.gltf"
OUT = "Meshes/escher_relativity.obj"
TARGET_HEIGHT = 4.6   # world units the monument should stand tall in the gallery

g = json.load(open(SRC))
blob = open(os.path.join(os.path.dirname(SRC), g["buffers"][0]["uri"]), "rb").read() \
    if not g["buffers"][0]["uri"].startswith("data:") else None
assert blob is not None, "embedded buffers not handled"

COMP = {5120:("b",1),5121:("B",1),5122:("h",2),5123:("H",2),5125:("I",4),5126:("f",4)}
NCOMP = {"SCALAR":1,"VEC2":2,"VEC3":3,"VEC4":4,"MAT4":16}

def read_accessor(idx):
    a = g["accessors"][idx]
    bv = g["bufferViews"][a["bufferView"]]
    fmt, size = COMP[a["componentType"]]
    n = NCOMP[a["type"]]
    stride = bv.get("byteStride", size * n)
    base = bv.get("byteOffset", 0) + a.get("byteOffset", 0)
    out = []
    for i in range(a["count"]):
        off = base + i * stride
        out.append(struct.unpack_from("<" + fmt * n, blob, off))
    return out

# ── Node transforms (glTF matrices are column-major) ─────────────────────────────────────────
def mat_identity(): return [[1,0,0,0],[0,1,0,0],[0,0,1,0],[0,0,0,1]]
def mat_mul(a, b):
    return [[sum(a[i][k]*b[k][j] for k in range(4)) for j in range(4)] for i in range(4)]
def node_matrix(node):
    if "matrix" in node:
        m = node["matrix"]
        return [[m[c*4+r] for c in range(4)] for r in range(4)]   # column-major -> rows
    T = node.get("translation", [0,0,0])
    R = node.get("rotation", [0,0,0,1])   # xyzw quaternion
    S = node.get("scale", [1,1,1])
    x,y,z,w = R
    rot = [[1-2*(y*y+z*z), 2*(x*y-z*w),   2*(x*z+y*w),   0],
           [2*(x*y+z*w),   1-2*(x*x+z*z), 2*(y*z-x*w),   0],
           [2*(x*z-y*w),   2*(y*z+x*w),   1-2*(x*x+y*y), 0],
           [0,0,0,1]]
    m = [[rot[r][c]*(S[c] if c < 3 else 1) for c in range(4)] for r in range(3)] + [[0,0,0,1]]
    for r in range(3): m[r][3] = T[r]
    return m

verts, faces = [], []
def emit(mesh_idx, world):
    for prim in g["meshes"][mesh_idx]["primitives"]:
        if prim.get("mode", 4) != 4:
            continue
        pos = read_accessor(prim["attributes"]["POSITION"])
        idx = [i[0] for i in read_accessor(prim["indices"])] if "indices" in prim \
              else list(range(len(pos)))
        base = len(verts)
        for p in pos:
            x = world[0][0]*p[0] + world[0][1]*p[1] + world[0][2]*p[2] + world[0][3]
            y = world[1][0]*p[0] + world[1][1]*p[1] + world[1][2]*p[2] + world[1][3]
            z = world[2][0]*p[0] + world[2][1]*p[1] + world[2][2]*p[2] + world[2][3]
            verts.append((x, y, z))
        for k in range(0, len(idx), 3):
            faces.append((base+idx[k]+1, base+idx[k+1]+1, base+idx[k+2]+1))

def walk(node_idx, parent):
    node = g["nodes"][node_idx]
    world = mat_mul(parent, node_matrix(node))
    if "mesh" in node:
        emit(node["mesh"], world)
    for child in node.get("children", []):
        walk(child, world)

scene = g["scenes"][g.get("scene", 0)]
for root in scene["nodes"]:
    walk(root, mat_identity())

# ── Fit: recentre on the footprint, floor at y=0, scale to TARGET_HEIGHT ────────────────────
lo = [min(v[i] for v in verts) for i in range(3)]
hi = [max(v[i] for v in verts) for i in range(3)]
print(f"raw bbox: x[{lo[0]:.1f},{hi[0]:.1f}] y[{lo[1]:.1f},{hi[1]:.1f}] z[{lo[2]:.1f},{hi[2]:.1f}]")
height = hi[1] - lo[1]
s = TARGET_HEIGHT / height
cx, cz = (lo[0]+hi[0])/2, (lo[2]+hi[2])/2
verts = [((v[0]-cx)*s, (v[1]-lo[1])*s, (v[2]-cz)*s) for v in verts]
hw = max(hi[0]-lo[0], hi[2]-lo[2]) / 2 * s
print(f"scaled: height {TARGET_HEIGHT}, footprint half-width {hw:.2f}")

with open(OUT, "w") as f:
    f.write("# Escher Relativity by Benoit Gagnier -- CC-BY-4.0\n")
    f.write("# https://sketchfab.com/3d-models/escher-relativity-6dccd307edce45f19c032378b4d10933\n")
    f.write("# Converted to this engine's OBJ dialect by tools/convert_gltf.py; geometry only\n")
    f.write("# (the source carries no UVs), no colliders (viewed from the gallery ring).\n")
    for v in verts:
        f.write(f"v {v[0]:.4f} {v[1]:.4f} {v[2]:.4f}\n")
    for a, b, c in faces:
        f.write(f"f {a} {b} {c}\n")
sz = os.path.getsize(OUT) / 1e6
print(f"wrote {OUT}: {len(verts)} verts, {len(faces)} tris, {sz:.1f} MB")
