#!/usr/bin/env python3
"""Generate src/ext/village_catalog.rs from the village pack's object layout.

The furniture showroom in Meshes/village_objects.glb ships as 195 loose nodes
whose names do not follow their assemblies: the plain chest's drawers are
called `Chest_drawers_01.001`, a ventilator's blades are `blades_02`, the
fridge's shelf was named bare `House_Demo` (renamed `Fridge_Shelf` at
conversion). Hand-typing the groups would go stale the day the pack updates,
so this script re-derives them the only way that is reliable: SPATIALLY.
Two showroom objects belong to the same furniture piece exactly when their
bounding boxes overlap (grown 5 cm), because Elbolillo parked every display
piece clear of its neighbours.

Reads the MANIFEST line `tools/build_props.py` prints (pass the build log as
argv[1], default the scratchpad copy) and writes a generated Rust table:
one entry per placeable furniture piece -- part name, the glTF node names it
gathers, and a coarse category the house generator (`ext/prochouse.rs`)
plans rooms with. The demo house's own pieces (shell, door, fitted kitchen,
dining set, fridge) come out as fixed named groups instead, and `Lot_Ground`
is the stage patch.

Run:  python3 tools/gen_catalog.py [build_log]   # then cargo fmt
"""

import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LOG = sys.argv[1] if len(sys.argv) > 1 else (
    "/private/tmp/claude-501/-Users-jeanrojas-backrooms/"
    "7e4dc15f-e0bf-4b98-938f-e47c450e054b/scratchpad/build_out.txt")
OUT = os.path.join(ROOT, "src", "ext", "village_catalog.rs")

manifest = None
for line in open(LOG):
    if line.startswith("MANIFEST village "):
        manifest = json.loads(line[len("MANIFEST village "):])
if manifest is None:
    sys.exit("no village MANIFEST in " + LOG)

# ---- fixed groups: the demo house's own assembly, by name ----------------
HOUSE_RANGE = set(f"House_Demo_{i:03d}" for i in range(1, 43)) | {"House_Demo_040"}
DOOR = {"House_Demo_039"}
KITCHEN = {f"House_Demo_{i:03d}" for i in (28, 29, 30, 31, 38)}
FRIDGE = {"House_Demo_041", "House_Demo_042", "Fridge_Shelf"}
DINING = {f"House_Demo_{i:03d}" for i in (32, 33, 34, 35, 36, 37)}
SHELL = (HOUSE_RANGE - DOOR - KITCHEN - DINING -
         {f"House_Demo_{i:03d}" for i in (41, 42)})
LOT = {"Lot_Ground"}
fixed = SHELL | DOOR | KITCHEN | FRIDGE | DINING | LOT

free = {n: b for n, b in manifest.items() if n not in fixed}

# ---- spatial clustering over the showroom ---------------------------------
GROW = 0.05
names = sorted(free)
parent = {n: n for n in names}


def find(n):
    while parent[n] != n:
        parent[n] = parent[parent[n]]
        n = parent[n]
    return n


def touches(a, b):
    ax0, ax1, ay0, ay1, az0, az1 = a
    bx0, bx1, by0, by1, bz0, bz1 = b
    return (ax0 - GROW < bx1 and bx0 - GROW < ax1
            and ay0 - GROW < by1 and by0 - GROW < ay1
            and az0 - GROW < bz1 and bz0 - GROW < az1)


def fan_suffix(n):
    """Ceiling fans overlap each other's blade discs; pair them by numeric
    suffix instead of by space ("Ceiling_Fan_Blades_01" goes with
    "Ceiling_Fan_01", bare with bare)."""
    if not n.startswith("Ceiling_Fan"):
        return None
    m = re.search(r"_(\d+)$", n)
    return m.group(1) if m else ""


for i, a in enumerate(names):
    for b in names[i + 1:]:
        fa, fb = fan_suffix(a), fan_suffix(b)
        if fa is not None or fb is not None:
            if fa != fb:
                continue
            if find(a) != find(b):
                parent[find(a)] = find(b)
            continue
        if touches(free[a], free[b]) and find(a) != find(b):
            parent[find(a)] = find(b)

clusters = {}
for n in names:
    clusters.setdefault(find(n), []).append(n)


def primary(members):
    """The member whose name carries the piece's identity: shortest, ties by
    alphabet, generic sub-part names (blades, Plate...) never win over a
    named body."""
    def keyof(n):
        generic = n.split(".")[0].rstrip("_0123456789").lower() in (
            "blades", "plate", "tumbler", "cutlery")
        return (generic, len(n), n)
    return sorted(members, key=keyof)[0]


def categorize(prim, members, bounds):
    dx = max(b[1] for b in bounds) - min(b[0] for b in bounds)
    dz = max(b[5] for b in bounds) - min(b[4] for b in bounds)
    stem = prim.split(".")[0]
    base = re.sub(r"_?\d+$", "", stem)
    if base == "bed":
        return "Bed"
    if base == "Armchair":
        return "Sofa" if max(dx, dz) > 1.8 else "Armchair"
    if base == "Chair":
        return "Chair"
    if base == "Table":
        return "Table"
    if base in ("Chest_drawers", "Bookcase") and base == "Bookcase" and dx > 1.5 and max(
            b[3] for b in bounds) - min(b[2] for b in bounds) < 1.1:
        return "Sideboard"
    if base == "Chest_drawers":
        return "Chest"
    if base in ("Bookcase", "Fitment"):
        return "Bookcase"
    if base == "Wardrobe":
        return "Wardrobe"
    if base in ("Lamp", "Desk_lamp"):
        return "TableLamp"
    if base == "Ventilator":
        h = max(b[3] for b in bounds) - min(b[2] for b in bounds)
        return "FloorFan" if h > 1.2 else "TableFan"
    if base == "Ceiling_Fan":
        return "CeilingFan"
    if base == "Focus":
        return "CeilingLight"
    if base == "Painting":
        return "Painting"
    if base in ("Radio", "Phone", "Alarm_clock", "Gadgets", "Books"):
        return "Gizmo"
    if base in ("Trash_can", "Garbage_bags", "Box_Pizza"):
        return "Junk"
    if base == "Flower_Table":
        return "Plant"
    return "Food"


entries = []
for root, members in clusters.items():
    members = sorted(members)
    prim = primary(members)
    bounds = [free[m] for m in members]
    cat = categorize(prim, members, bounds)
    # A cluster that swallowed its table (a dressed table reads as one piece)
    # must be planned as floor furniture, whatever its primary was.
    if cat in ("Gizmo", "Food", "TableLamp", "TableFan"):
        dx = max(b[1] for b in bounds) - min(b[0] for b in bounds)
        dz = max(b[5] for b in bounds) - min(b[4] for b in bounds)
        has_table = any(m.split(".")[0].split("_")[0] == "Table" for m in members)
        if max(dx, dz) > 0.95 and has_table:
            cat = "Table"
    part = re.sub(r"[^A-Za-z0-9]+", "_", prim).strip("_").lower()
    entries.append((part, members, cat))
entries.sort()

# Distinct part names: two clusters may share a primary stem.
seen = {}
uniq = []
for part, members, cat in entries:
    if part in seen:
        seen[part] += 1
        part = f"{part}_v{seen[part]}"
    else:
        seen[part] = 0
    uniq.append((part, members, cat))

cats = sorted(set(c for _, _, c in uniq))


def rs_list(xs):
    return "&[" + ", ".join(f'"{x}"' for x in xs) + "]"


with open(OUT, "w") as f:
    w = f.write
    w("//! GENERATED by tools/gen_catalog.py -- DO NOT EDIT.\n")
    w("//!\n")
    w("//! The village pack's showroom, grouped into placeable furniture: every\n")
    w("//! cluster of overlapping nodes is one piece (a chest and its drawers, a\n")
    w("//! fan and its blades), named after its primary member and given the\n")
    w("//! coarse category `ext/prochouse.rs` plans rooms with. The demo house's\n")
    w("//! own assembly ships as the fixed groups at the end.\n\n")
    w("use crate::ext::gltf_model::{Anchor, Frame, PartSpec};\n\n")
    w("#[derive(Clone, Copy, PartialEq, Eq, Debug)]\n")
    w("pub enum Cat {\n")
    for c in cats:
        w(f"    {c},\n")
    w("}\n\n")
    w("pub struct CatEntry {\n")
    w("    pub part: &'static str,\n")
    w("    pub cat: Cat,\n")
    w("}\n\n")
    w(f"pub const CATALOG: [CatEntry; {len(uniq)}] = [\n")
    for part, members, cat in uniq:
        w(f'    CatEntry {{ part: "{part}", cat: Cat::{cat} }},\n')
    w("];\n\n")
    w("const fn spec(name: &'static str, roots: &'static [&'static str])")
    w(" -> PartSpec<'static> {\n")
    w("    PartSpec { name, roots, skip: &[], frame: Frame::Scene,")
    w(" anchor: Anchor::Hinge }\n}\n\n")
    total = len(uniq) + 6
    w(f"/// Every part of Meshes/village_objects.glb, one spec: the furniture\n")
    w(f"/// catalog plus the house's fixed groups.\n")
    w(f"pub const PARTS: [PartSpec<'static>; {total}] = [\n")
    w(f'    spec("house_shell", {rs_list(sorted(SHELL))}),\n')
    w(f'    spec("house_door", {rs_list(sorted(DOOR))}),\n')
    w(f'    spec("kitchen_fit", {rs_list(sorted(KITCHEN))}),\n')
    w(f'    spec("fridge_fit", {rs_list(sorted(FRIDGE))}),\n')
    w(f'    spec("dining_fit", {rs_list(sorted(DINING))}),\n')
    w(f'    spec("lot_ground", {rs_list(sorted(LOT))}),\n')
    for part, members, cat in uniq:
        w(f'    spec("{part}", {rs_list(members)}),\n')
    w("];\n")

print(f"wrote {OUT}: {len(uniq)} furniture parts, categories: "
      + ", ".join(f"{c}x{sum(1 for _, _, k in uniq if k == c)}" for c in cats))
