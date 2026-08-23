"""SUPERSEDED -- kept as documentation of the original scatter. Do not run.

The blade patch is now generated in-process by src/ext/grassgen.rs, which is the source of
truth: same PATCH / DENSE_R / BLADES / SEGMENTS / BLADE_H / BLADE_W, same arch and taper, and
the same (t, phase, lean) vertex channel -- but indexed, one winding, and bucketed into cull
cells, none of which the engine's OBJ dialect can express. Meshes/grass_patch.obj (124 MB, the
file this wrote) has been removed from the tree; the generator builds the patch in ~20 ms at
startup instead of the parser spending 0.37 s re-reading it on every intro load.

This script stays because it documents where the numbers came from and what the Houdini
session looked like; nothing in the build or the runtime depends on it.

Original docstring follows.

Build a tileable patch of grass BLADES in Houdini and export it in the engine's OBJ dialect.

Run with hython:
    /Applications/Houdini/Houdini22.0.368/Frameworks/Houdini.framework/Versions/22.0/\
Resources/bin/hython tools/gen_grass_houdini.py

# Why geometry rather than the Shadertoy shader

The reference (MonterMan, "grass field with blades", shadertoy.com/view/dd2cWh) raymarches an
SDF blade field with 512 steps per pixel and a three-buffer TAA chain. This engine is a
rasteriser whose defining feature is re-rendering the whole scene into each portal's 2048x2048
framebuffer, up to four levels deep -- per-pixel raymarching there is not a frame budget, and
TAA needs history buffers and motion reprojection the engine has no concept of. So the LOOK is
reproduced with real blades and the reference's shading ideas (see Shaders/grassblade.frag),
rather than by porting code.

# The patch

One mesh of blades covering PATCH x PATCH units, which `ext/grassfield.rs` keeps centred on the
player by snapping its position to a SNAP grid -- so the blades never slide underfoot, and only
this small neighbourhood is ever drawn. Beyond it the ground's grass *texture* takes over, and
distance haze hides the handover.

Density falls off toward the patch edge: dense where the player is, sparse where the texture
takes over, which is where the triangles are worth spending.

Per-vertex data rides in the 3-component `vt` channel the engine already supports:
    vt.x = t, height along the blade 0..1  (drives bend, colour, and AO)
    vt.y = per-blade random 0..1           (phase, so blades do not sway in lockstep)
    vt.z = blade lean angle in radians     (the direction this blade bends when wind hits)
The vertex shader reads it as `in_uv` and does the bending; nothing is animated on the CPU.
"""

import hou
import math
import os
import random

OUT = "Meshes/grass_patch.obj"

PATCH = 26.0          # patch side in world units -- smaller, so the budget buys DENSITY
DENSE_R = 8.0         # full density within this radius of the centre
BLADES = 130000       # a lawn, not bristles: real grass is dense enough to hide the soil
SEGMENTS = 4          # quads per blade; 4 lets a blade actually arch over
BLADE_H = (0.22, 0.55)
BLADE_W = (0.020, 0.038)   # wider: thin blades alias into wire at distance
SEED = 11


def blade_positions():
    """Scatter blades with a radial density falloff, in a disc that covers the patch."""
    rng = random.Random(SEED)
    half = PATCH / 2.0
    out = []
    tries = 0
    while len(out) < BLADES and tries < BLADES * 40:
        tries += 1
        x = rng.uniform(-half, half)
        z = rng.uniform(-half, half)
        r = math.hypot(x, z)
        if r > half:
            continue
        # Keep everything close in; thin out with distance so the far half costs little.
        keep = 1.0 if r <= DENSE_R else max(0.06, 1.0 - (r - DENSE_R) / (half - DENSE_R))
        if rng.random() <= keep:
            out.append((x, z, rng.random(), rng.random(), rng.random()))
    return out


def build(geo):
    rng = random.Random(SEED + 1)
    verts = []   # (pos, uvw)
    quads = []

    for (x, z, ra, rb, rc) in blade_positions():
        h = BLADE_H[0] + (BLADE_H[1] - BLADE_H[0]) * ra
        w = BLADE_W[0] + (BLADE_W[1] - BLADE_W[0]) * rb
        lean = rc * math.tau                     # which way this blade leans
        phase = rng.random()                     # so neighbours sway out of step
        # Real grass ARCHES -- blades fall away from vertical under their own weight. Straight
        # blades are what made the first version read as a bed of nails.
        curve = 0.45 + 0.85 * rng.random()
        dx, dz = math.cos(lean), math.sin(lean)

        ring = []
        for s in range(SEGMENTS + 1):
            t = s / float(SEGMENTS)
            y = h * t
            # Rest curve: quadratic in t, so the tip arches over and the base stays planted.
            off = curve * h * t * t
            cx = x + dx * off
            cz = z + dz * off
            # Blades taper to a point.
            # Taper toward the tip but keep some width most of the way, like a real blade.
            half_w = w * (1.0 - 0.85 * t * t) * 0.5
            # Width axis is perpendicular to the lean, so blades present a face when bent.
            px, pz = -dz, dx
            a = (cx - px * half_w, y, cz - pz * half_w)
            b = (cx + px * half_w, y, cz + pz * half_w)
            ring.append((a, b, t))

        base_idx = len(verts)
        for (a, b, t) in ring:
            verts.append((a, (t, phase, lean)))
            verts.append((b, (t, phase, lean)))
        for s in range(SEGMENTS):
            i0 = base_idx + s * 2
            # Two windings so a blade is visible from both sides: the engine culls back faces
            # and there is no per-object way to disable that from a scene.
            quads.append((i0 + 0, i0 + 1, i0 + 3, i0 + 2))
            quads.append((i0 + 2, i0 + 3, i0 + 1, i0 + 0))
    return verts, quads


def main():
    os.chdir(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    geo = hou.Geometry()
    verts, quads = build(geo)

    lines = [
        "# EXT asset: tileable grass blade patch, built by tools/gen_grass_houdini.py.",
        "# vt carries (t along blade, per-blade random, lean angle) -- Shaders/grassblade.vert",
        "# reads it as in_uv and does all bending on the GPU.",
        "# Shading model follows MonterMan's 'grass field with blades'",
        "# (shadertoy.com/view/dd2cWh, CC BY-NC-SA 3.0) -- ideas, not code; see Shaders/grassblade.frag.",
        "# No colliders: grass does not block the player.",
    ]
    for (pos, _uv) in verts:
        lines.append(f"v {pos[0]:.4f} {pos[1]:.4f} {pos[2]:.4f}")
    for (_pos, uv) in verts:
        lines.append(f"vt {uv[0]:.4f} {uv[1]:.4f} {uv[2]:.4f}")
    for (a, b, c, d) in quads:
        lines.append(f"f {a+1}/{a+1} {b+1}/{b+1} {c+1}/{c+1} {d+1}/{d+1}")

    with open(OUT, "w") as f:
        f.write("\n".join(lines) + "\n")

    tris = len(quads) * 2
    print(f"{OUT}: {len(verts)} verts, {len(quads)} quads -> {tris} tris")
    print(f"  patch {PATCH}x{PATCH} units, {len(verts)//((SEGMENTS+1)*2)} blades")
    size_mb = os.path.getsize(OUT) / 1e6
    print(f"  file {size_mb:.1f} MB")
    assert tris < 2600000, f"too many triangles for a per-frame draw: {tris}"


main()
