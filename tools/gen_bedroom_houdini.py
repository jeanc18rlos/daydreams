"""Build the graybox BEDROOM KIT in Houdini and export it in the engine's OBJ dialect.

Run with hython, from the repository root:

    /Applications/Houdini/Houdini22.0.368/Frameworks/Houdini.framework/Versions/22.0/\
Resources/bin/hython tools/gen_bedroom_houdini.py

Writes Meshes/bedroom/*.obj -- one file per prop, no assembly. Placing them is the level's
job (that is the point: the kit is a set of parts on a shared grid, not a room).

# The grid

Everything is metres, Y up, and sized to one module:

    MODULE  2.00   wall panel width and floor/ceiling tile side
    HEIGHT  2.50   floor surface to ceiling surface
    THICK   0.12   wall thickness

Floor tiles put their TOP surface at y = 0 and hang below it, so the walkable plane is y = 0
and a room's props sit at y = 0 with no bookkeeping. Ceiling tiles put their BOTTOM at
y = HEIGHT. Wall panels stand on y = 0, run along X, and are THICK deep in Z with the room
side toward -Z -- so a wall is placed by rotating it about Y in 90-degree steps and stepping
its centre along the room's edge.

# Pivots

Each prop's origin is the point you would grab it by, because the assembler positions parts by
their origin and the engine rotates them about it:

    doors (room + closet)   the HINGE: bottom of the hinge edge, the leaf extending away
                            from it along X (+X, except closet_door_r, which hangs on the
                            carcass's right edge and so extends -X). Swinging a door is one
                            rotation about the origin's Y axis, either sign. Left and right
                            closet leaves ship as separate files rather than one mirrored
                            file -- mirroring flips the winding and the engine culls back
                            faces. Each file's own header gives where to place it.
    walls                   bottom centre of the panel.
    floor / ceiling tiles   tile centre (on the surface, see above).
    frames                  bottom centre of the OPENING they line, which is the same point
                            as the panel's own origin for a door and the sill centre for a
                            window -- frame and panel share coordinates.
    furniture               centre of the footprint, on the floor.

# Mesh dialect

The engine's (Mesh.cpp, and see tools/gen_props.py): `v`, `vt`, `f a/at b/bt c/ct [d/dt]`,
quads allowed, counter-clockwise seen from OUTSIDE -- the engine culls back faces. Houdini
winds the other way round, so `write_obj` orders each polygon against the primitive's own
normal instead of trusting either convention. Normals are recomputed flat at load; a graybox
wants the facets, so nothing here is smoothed.

No `c` lines: these are visual parts. Collision is the level's business -- the boxes are
axis-aligned and their dimensions are all in the KIT table below, so a collider per part is a
rectangle the level can write down without measuring the mesh.

UVs are a box projection at 1 unit = 1 metre (v = 0 at the top of the image, the BMP loader's
orientation), so a checker tiles per metre on every part without anyone unwrapping anything.
"""

import os
import sys

import hou

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT_DIR = os.path.join(ROOT, "Meshes", "bedroom")

# ── The grid ─────────────────────────────────────────────────────────────────────────────────

MODULE = 2.00      # wall panel width, floor tile side
HEIGHT = 2.50      # floor to ceiling
THICK = 0.12       # wall thickness
SLAB = 0.10        # floor / ceiling tile thickness

DOOR_W = 0.90      # door opening
DOOR_H = 2.05
LEAF_W = 0.86      # the leaf is smaller than its opening by the frame lining
LEAF_H = 2.00
LEAF_T = 0.045
LINING = 0.02      # frame lining thickness (into the opening)
CASING = 0.06      # frame casing width (onto the wall face)

WIN_W = 1.20       # window opening
WIN_H = 1.10
WIN_SILL = 0.95    # opening bottom above the floor


# ── Houdini helpers ──────────────────────────────────────────────────────────────────────────


def box(net, name, size, center=(0.0, 0.0, 0.0)):
    """A Box SOP of `size` (x, y, z) centred at `center`. Six quads, eight points."""
    n = net.createNode("box", name)
    n.parmTuple("size").set(size)
    n.parmTuple("t").set(center)
    return n


def sitting_box(net, name, size, center_xz=(0.0, 0.0), base=0.0):
    """A box standing ON `base`: `center_xz` in plan, bottom face at y = base."""
    sx, sy, sz = size
    return box(net, name, size, (center_xz[0], base + sy / 2.0, center_xz[1]))


def merged(net, name, parts):
    """Merge `parts` into one output node."""
    m = net.createNode("merge", name)
    for i, p in enumerate(parts):
        m.setInput(i, p)
    return m


# ── OBJ writer ───────────────────────────────────────────────────────────────────────────────


def _uv(pos, normal):
    """Box projection at 1 unit = 1 metre, v = 0 at the top of the image."""
    x, y, z = pos
    ax, ay, az = abs(normal[0]), abs(normal[1]), abs(normal[2])
    if ay >= ax and ay >= az:      # floor / ceiling face: plan projection
        return (x, z)
    if ax >= az:                   # face looking along X: depth across, height up
        return (z, -y)
    return (x, -y)                 # face looking along Z


def write_obj(path, geo, title, note=""):
    """Write `geo` in the engine's OBJ dialect. Returns (points, faces)."""
    lines = [
        "# %s -- graybox bedroom kit, generated by tools/gen_bedroom_houdini.py." % title,
        "# Metres, Y up. Do not hand-edit: re-run the generator.",
    ]
    if note:
        lines += ["# " + l for l in note.strip().splitlines()]
    lines.append("")

    pts = geo.points()
    index = {}
    for i, p in enumerate(pts):
        x, y, z = p.position()
        index[p.number()] = i + 1
        lines.append("v %.5g %.5g %.5g" % (x, y, z))

    uvs, uv_index, faces = [], {}, []
    for prim in geo.prims():
        verts = [v.point() for v in prim.vertices()]
        n = prim.normal()
        # Order the ring so that its right-hand-rule normal agrees with the primitive's own
        # outward normal -- counter-clockwise seen from outside, whichever way Houdini wound it.
        if _newell(verts).dot(n) < 0:
            verts = list(reversed(verts))
        face = []
        for v in verts:
            uv = _uv(v.position(), n)
            key = (round(uv[0], 5), round(uv[1], 5))
            if key not in uv_index:
                uv_index[key] = len(uvs) + 1
                uvs.append(key)
            face.append("%d/%d" % (index[v.number()], uv_index[key]))
        faces.append("f " + " ".join(face))

    lines.append("")
    lines += ["vt %.5g %.5g" % uv for uv in uvs]
    lines.append("")
    lines += faces
    lines.append("")

    with open(path, "w") as f:
        f.write("\n".join(lines))
    return len(pts), len(faces)


def _newell(verts):
    """Newell normal of a vertex ring, as a hou.Vector3."""
    n = hou.Vector3(0.0, 0.0, 0.0)
    for i, a in enumerate(verts):
        b = verts[(i + 1) % len(verts)]
        pa, pb = a.position(), b.position()
        n += hou.Vector3(
            (pa[1] - pb[1]) * (pa[2] + pb[2]),
            (pa[2] - pb[2]) * (pa[0] + pb[0]),
            (pa[0] - pb[0]) * (pa[1] + pb[1]),
        )
    return n


# ── The kit ──────────────────────────────────────────────────────────────────────────────────
#
# Each builder takes the geo network and returns (output node, note for the file header).


def floor_tile(net):
    """One MODULE square of floor. Top surface at y = 0, slab hanging below it."""
    b = box(net, "floor_tile", (MODULE, SLAB, MODULE), (0, -SLAB / 2.0, 0))
    return b, "Top surface at y = 0. Tile at (i * %g, 0, j * %g)." % (MODULE, MODULE)


def ceiling_tile(net):
    """One MODULE square of ceiling. Bottom surface at y = HEIGHT."""
    b = box(net, "ceiling_tile", (MODULE, SLAB, MODULE), (0, HEIGHT + SLAB / 2.0, 0))
    return b, "Bottom surface at y = %g." % HEIGHT


def wall_panel(net):
    """A blank wall module: MODULE wide along X, HEIGHT tall, THICK deep, room side toward -Z."""
    b = sitting_box(net, "wall_panel", (MODULE, HEIGHT, THICK))
    return b, "Origin at bottom centre. Runs along X, room side toward -Z."


def wall_door(net):
    """A wall module with the door opening at its centre: two jambs and a header."""
    side = (MODULE - DOOR_W) / 2.0
    off = (DOOR_W + side) / 2.0
    parts = [
        sitting_box(net, "wall_door_left", (side, HEIGHT, THICK), (-off, 0)),
        sitting_box(net, "wall_door_right", (side, HEIGHT, THICK), (off, 0)),
        sitting_box(net, "wall_door_head", (DOOR_W, HEIGHT - DOOR_H, THICK), (0, 0), DOOR_H),
    ]
    return merged(net, "wall_door", parts), (
        "Opening %g x %g centred on the origin, which is also the door frame's origin." %
        (DOOR_W, DOOR_H))


def wall_window(net):
    """A wall module with the window opening: two jambs, a sill course and a header."""
    side = (MODULE - WIN_W) / 2.0
    off = (WIN_W + side) / 2.0
    top = WIN_SILL + WIN_H
    parts = [
        sitting_box(net, "wall_window_left", (side, HEIGHT, THICK), (-off, 0)),
        sitting_box(net, "wall_window_right", (side, HEIGHT, THICK), (off, 0)),
        sitting_box(net, "wall_window_sill", (WIN_W, WIN_SILL, THICK), (0, 0)),
        sitting_box(net, "wall_window_head", (WIN_W, HEIGHT - top, THICK), (0, 0), top),
    ]
    return merged(net, "wall_window", parts), (
        "Opening %g x %g, sill at y = %g. The window frame's origin is the sill centre, "
        "%g above this panel's origin." % (WIN_W, WIN_H, WIN_SILL, WIN_SILL))


def door_frame(net):
    """Lining and casing for the door opening. Origin = the wall panel's origin."""
    lin_x = DOOR_W / 2.0 - LINING / 2.0
    parts = [
        # Lining: the three faces inside the opening.
        sitting_box(net, "door_frame_lin_l", (LINING, DOOR_H, THICK), (-lin_x, 0)),
        sitting_box(net, "door_frame_lin_r", (LINING, DOOR_H, THICK), (lin_x, 0)),
        sitting_box(net, "door_frame_lin_t", (DOOR_W, LINING, THICK), (0, 0), DOOR_H - LINING),
    ]
    # Casing: a flat band on both wall faces, standing slightly proud of them.
    for tag, z in (("front", -THICK / 2.0 - 0.01), ("back", THICK / 2.0 + 0.01)):
        cas_x = DOOR_W / 2.0 + CASING / 2.0
        parts += [
            sitting_box(net, "door_frame_cas_l_" + tag,
                        (CASING, DOOR_H + CASING, 0.02), (-cas_x, z)),
            sitting_box(net, "door_frame_cas_r_" + tag,
                        (CASING, DOOR_H + CASING, 0.02), (cas_x, z)),
            sitting_box(net, "door_frame_cas_t_" + tag,
                        (DOOR_W + 2 * CASING, CASING, 0.02), (0, z), DOOR_H),
        ]
    return merged(net, "door_frame", parts), (
        "Lines the %g x %g opening. Shares the wall_door panel's origin: place both at the "
        "same point." % (DOOR_W, DOOR_H))


def door(net):
    """The leaf, HINGED ON ITS ORIGIN: bottom of the hinge edge, leaf extending toward +X."""
    parts = [
        box(net, "door_leaf", (LEAF_W, LEAF_H, LEAF_T), (LEAF_W / 2.0, LEAF_H / 2.0, 0)),
        # Handle on the free edge, both faces.
        box(net, "door_handle_f", (0.11, 0.03, 0.05),
            (LEAF_W - 0.09, 1.02, -LEAF_T / 2.0 - 0.025)),
        box(net, "door_handle_b", (0.11, 0.03, 0.05),
            (LEAF_W - 0.09, 1.02, LEAF_T / 2.0 + 0.025)),
    ]
    return merged(net, "door", parts), (
        "HINGE PIVOT at the origin; leaf spans x = 0..%g. Rotate about Y to swing it -- the "
        "hinge sits %g inside the opening's left edge, so place it at the wall_door origin "
        "offset by (-%g, 0, 0)." % (LEAF_W, (DOOR_W - LEAF_W) / 2.0, LEAF_W / 2.0))


def window_frame(net):
    """Lining, sill nose and a centre mullion. Origin = sill centre of the opening."""
    lin_x = WIN_W / 2.0 - LINING / 2.0
    parts = [
        box(net, "win_lin_l", (LINING, WIN_H, THICK), (-lin_x, WIN_H / 2.0, 0)),
        box(net, "win_lin_r", (LINING, WIN_H, THICK), (lin_x, WIN_H / 2.0, 0)),
        box(net, "win_lin_t", (WIN_W, LINING, THICK), (0, WIN_H - LINING / 2.0, 0)),
        box(net, "win_lin_b", (WIN_W, LINING, THICK), (0, LINING / 2.0, 0)),
        # A sill nose, proud of the room face (-Z).
        box(net, "win_sill", (WIN_W + 2 * CASING, 0.04, THICK + 0.10),
            (0, -0.02, -0.05)),
        # Mullion: two casements.
        box(net, "win_mullion", (0.05, WIN_H, THICK), (0, WIN_H / 2.0, 0)),
    ]
    return merged(net, "window_frame", parts), (
        "Fits the %g x %g opening; origin is the sill centre, i.e. the wall_window origin "
        "raised by %g. Room side is -Z, matching the panel." % (WIN_W, WIN_H, WIN_SILL))


def window_glass(net):
    """Two panes, thin boxes so they have a front and a back like everything else."""
    pane_w = (WIN_W - 0.05) / 2.0 - LINING
    off = 0.025 + pane_w / 2.0
    parts = [
        box(net, "pane_l", (pane_w, WIN_H - 2 * LINING, 0.01), (-off, WIN_H / 2.0, 0)),
        box(net, "pane_r", (pane_w, WIN_H - 2 * LINING, 0.01), (off, WIN_H / 2.0, 0)),
    ]
    return merged(net, "window_glass", parts), (
        "Same origin as window_frame. Separate file so the level can draw it translucent, "
        "skip it, or break it.")


def bed(net):
    """Single bed frame with a headboard. 0.90 x 2.00 footprint, headboard at -Z."""
    w, l = 0.90, 2.00
    parts = [
        sitting_box(net, "bed_deck", (w, 0.10, l), (0, 0), 0.22),          # slats
        sitting_box(net, "bed_rail_l", (0.06, 0.32, l), (-w / 2.0 + 0.03, 0)),
        sitting_box(net, "bed_rail_r", (0.06, 0.32, l), (w / 2.0 - 0.03, 0)),
        sitting_box(net, "bed_foot", (w, 0.32, 0.06), (0, l / 2.0 - 0.03)),
        sitting_box(net, "bed_head", (w, 0.90, 0.06), (0, -l / 2.0 + 0.03)),  # headboard
    ]
    return merged(net, "bed", parts), (
        "Origin at the centre of the %g x %g footprint, on the floor. Headboard at -Z: face "
        "it at the wall. Deck top at y = 0.32." % (w, l))


def mattress(net):
    """Sits on the bed deck. Separate so a level can strip the bed or stack bedding."""
    b = sitting_box(net, "mattress", (0.86, 0.20, 1.94), (0, 0))
    return b, "Origin on its underside, centred. Place at the bed's origin + (0, 0.32, 0)."


def pillow(net):
    b = sitting_box(net, "pillow", (0.50, 0.12, 0.32), (0, 0))
    return b, "Origin on its underside, centred."


def nightstand(net):
    """0.45 square, 0.55 tall, with a drawer face and four feet."""
    w, d, h = 0.45, 0.40, 0.55
    parts = [sitting_box(net, "ns_body", (w, h - 0.08, d), (0, 0), 0.08),
             sitting_box(net, "ns_drawer", (w - 0.08, 0.16, 0.03),
                         (0, -d / 2.0 - 0.015), 0.30),
             sitting_box(net, "ns_pull", (0.12, 0.03, 0.03), (0, -d / 2.0 - 0.04), 0.365)]
    for sx in (-1, 1):
        for sz in (-1, 1):
            parts.append(sitting_box(
                net, "ns_foot_%d%d" % (sx + 1, sz + 1), (0.05, 0.08, 0.05),
                (sx * (w / 2.0 - 0.04), sz * (d / 2.0 - 0.04))))
    return merged(net, "nightstand", parts), (
        "Origin at the centre of the %g x %g footprint, on the floor. Top at y = %g -- where "
        "the alarm clock goes. Drawer face at -Z." % (w, d, h))


def alarm_clock(net):
    """A bedside clock: wedge-ish body, a recessed display, two bells and feet."""
    w, h, d = 0.14, 0.09, 0.07
    parts = [
        sitting_box(net, "clk_body", (w, h, d), (0, 0), 0.015),
        # Display panel, proud of the front face (-Z) so it can take its own material.
        sitting_box(net, "clk_face", (w - 0.03, h - 0.035, 0.008),
                    (0, -d / 2.0 - 0.004), 0.035),
        sitting_box(net, "clk_foot_l", (0.03, 0.015, d - 0.02), (-w / 2.0 + 0.02, 0)),
        sitting_box(net, "clk_foot_r", (0.03, 0.015, d - 0.02), (w / 2.0 - 0.02, 0)),
        # Bells.
        sitting_box(net, "clk_bell_l", (0.045, 0.035, 0.045), (-w / 2.0 + 0.02, 0), 0.015 + h),
        sitting_box(net, "clk_bell_r", (0.045, 0.035, 0.045), (w / 2.0 - 0.02, 0), 0.015 + h),
    ]
    return merged(net, "alarm_clock", parts), (
        "Origin at the centre of its footprint, on the surface it stands on. Display faces "
        "-Z, same room-facing direction as the walls, so an unrotated clock faces the room.")


def closet(net):
    """Carcass 1.40 wide, 0.60 deep, 2.10 tall: back, sides, top, plinth, shelf and a rail."""
    w, d, h = 1.40, 0.60, 2.10
    t = 0.04
    parts = [
        sitting_box(net, "cl_back", (w, h, t), (0, d / 2.0 - t / 2.0)),
        sitting_box(net, "cl_side_l", (t, h, d), (-w / 2.0 + t / 2.0, 0)),
        sitting_box(net, "cl_side_r", (t, h, d), (w / 2.0 - t / 2.0, 0)),
        sitting_box(net, "cl_top", (w, t, d), (0, 0), h - t),
        sitting_box(net, "cl_plinth", (w, 0.08, d), (0, 0)),
        sitting_box(net, "cl_shelf", (w - 2 * t, t, d - t), (0, 0), 1.45),
        sitting_box(net, "cl_rail", (w - 2 * t, 0.03, 0.03), (0, 0), 1.35),
    ]
    return merged(net, "closet", parts), (
        "Origin at the centre of the %g x %g footprint, on the floor. Open front at -Z. "
        "Leaves hinge on the front edge at z = %g." % (w, d, -d / 2.0))


def _closet_leaf(net, name, hand):
    """One closet leaf, hinged on its origin. `hand` +1 opens toward +X, -1 toward -X."""
    lw, lh, lt = 0.68, 2.00, 0.04
    parts = [
        box(net, name + "_leaf", (lw, lh, lt), (hand * lw / 2.0, lh / 2.0, 0)),
        box(net, name + "_pull", (0.03, 0.14, 0.04),
            (hand * (lw - 0.05), 1.15, -lt / 2.0 - 0.02)),
    ]
    return merged(net, name, parts), (
        "HINGE PIVOT at the origin; leaf spans x = %s. Place at the closet's origin + "
        "(%+.2f, 0.08, %.2f) -- the carcass's outer edge, on top of its plinth, just proud "
        "of the open front -- and rotate about Y to open." %
        ("0..%g" % lw if hand > 0 else "-%g..0" % lw,
         -hand * 0.70, -0.60 / 2.0 - lt / 2.0))


def closet_door_l(net):
    return _closet_leaf(net, "closet_door_l", 1)


def closet_door_r(net):
    return _closet_leaf(net, "closet_door_r", -1)


KIT = [
    ("floor_tile", floor_tile),
    ("ceiling_tile", ceiling_tile),
    ("wall_panel", wall_panel),
    ("wall_door", wall_door),
    ("wall_window", wall_window),
    ("door_frame", door_frame),
    ("door", door),
    ("window_frame", window_frame),
    ("window_glass", window_glass),
    ("bed", bed),
    ("mattress", mattress),
    ("pillow", pillow),
    ("nightstand", nightstand),
    ("alarm_clock", alarm_clock),
    ("closet", closet),
    ("closet_door_l", closet_door_l),
    ("closet_door_r", closet_door_r),
]


def main():
    if not os.path.isdir(os.path.join(ROOT, "Meshes")):
        sys.exit("run from the repository root: Meshes/ not found")
    if not os.path.isdir(OUT_DIR):
        os.makedirs(OUT_DIR)

    obj = hou.node("/obj")
    net = obj.createNode("geo", "bedroom_kit")
    for child in net.children():
        child.destroy()

    print("%-16s %6s %6s  %s" % ("part", "pts", "faces", "bounds (m)"))
    for name, build in KIT:
        node, note = build(net)
        geo = node.geometry()
        pts, faces = write_obj(os.path.join(OUT_DIR, name + ".obj"), geo, name, note)
        b = geo.boundingBox()
        print("%-16s %6d %6d  x %.2f..%.2f  y %.2f..%.2f  z %.2f..%.2f" % (
            name, pts, faces,
            b.minvec()[0], b.maxvec()[0],
            b.minvec()[1], b.maxvec()[1],
            b.minvec()[2], b.maxvec()[2]))

    hip = os.path.join(OUT_DIR, "bedroom_kit.hip")
    hou.hipFile.save(hip)
    print("\nwrote %d parts to Meshes/bedroom/ and the scene to %s" % (len(KIT), hip))


main()
