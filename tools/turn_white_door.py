#!/usr/bin/env python3
"""EXT dev tool: bake a half-turn onto the white door of the PSX doors pack.

`src/ext/door.rs` hangs its leaf on the hinge and swings it about that edge, and the loader's
`Anchor::Hinge` puts a part's origin on its **low-x** edge (`src/ext/gltf_model.rs`). Every door
in Icevanilla's "Low-Poly PSX Style Essential Doors Pack" is modelled the other way round: the
knob sits at the low-x end, so the hinge is the high-x edge. Imported as it comes, the white door
would swing about its own doorknob.

Turning the two nodes the game uses a half-turn about Y moves the hinge edge to low x. It is a
rotation, not a mirror, so triangle winding is untouched and the loader carries the normals and
tangents through with it (`gltf_model.rs`, the `direction(&world, ..)` calls in `walk`) -- which
is the whole reason for doing it here, in a node transform the engine already applies, rather
than teaching the fit pass to reflect geometry it currently only scales and translates.

It is free to look at because this door is symmetric front to back: the same panels, and a knob
that goes through. All the turn changes is which side the knob is on, and a door has to pick one.

The pack is otherwise untouched -- the other six doors, the shared 1024x256 colour map and every
material are exactly as downloaded. Run it on a fresh download to reproduce the shipped file:

    python3 tools/turn_white_door.py ~/Downloads/low-poly_psx_style_essential_doors_pack.glb \
        Meshes/psx_essential_doors_pack.glb
"""

import json
import struct
import sys

# The white door: its leaf (the knob rides along as a child) and its frame. Both are turned, so
# the assembly keeps the relative offset `Anchor::Around("leaf")` reads out of the file.
TURNED = ["Bathroom Door_002.001", "Bathroom Doorframe_001.001"]

JSON_CHUNK = 0x4E4F534A
BIN_CHUNK = 0x004E4942


def read_glb(path):
    raw = open(path, "rb").read()
    magic, _version, _length = struct.unpack_from("<4sII", raw, 0)
    if magic != b"glTF":
        sys.exit(f"{path}: not a GLB")
    doc, blob, off = None, b"", 12
    while off < len(raw):
        (size, kind) = struct.unpack_from("<II", raw, off)
        off += 8
        if kind == JSON_CHUNK:
            doc = json.loads(raw[off : off + size])
        elif kind == BIN_CHUNK:
            blob = raw[off : off + size]
        off += size
    if doc is None:
        sys.exit(f"{path}: no JSON chunk")
    return doc, blob


def write_glb(path, doc, blob):
    js = json.dumps(doc, separators=(",", ":")).encode("utf-8")
    js += b" " * (-len(js) % 4)
    blob += b"\0" * (-len(blob) % 4)
    body = struct.pack("<II", len(js), JSON_CHUNK) + js
    if blob:
        body += struct.pack("<II", len(blob), BIN_CHUNK) + blob
    with open(path, "wb") as f:
        f.write(struct.pack("<4sII", b"glTF", 2, 12 + len(body)) + body)


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    src, dst = sys.argv[1], sys.argv[2]
    doc, blob = read_glb(src)

    by_name = {n.get("name"): n for n in doc["nodes"]}
    for name in TURNED:
        node = by_name.get(name)
        if node is None:
            sys.exit(f"{src}: no node named {name!r} -- is this the right pack?")
        if "matrix" not in node:
            # The pack stores these as matrices (a 100x scale and the exporter's Z-up swap).
            # A TRS node would need its own composition; refuse rather than guess.
            sys.exit(f"{name!r} is not a matrix node; composing TRS is not implemented")
        # PRE-multiply: R * M, which turns the node where it stands in its PARENT's space, so
        # both parts get the identical rigid transform and the offset between leaf and frame --
        # which `Anchor::Around("leaf")` reads straight out of the file -- survives exactly.
        # Post-multiplying would turn each about its own origin, and their origins differ.
        #
        # glTF matrices are column-major, m[4*col + row]. R negates x and z, so R*M is M with
        # rows 0 and 2 negated.
        m = list(node["matrix"])
        for col in range(4):
            m[4 * col + 0] = -m[4 * col + 0]
            m[4 * col + 2] = -m[4 * col + 2]
        node["matrix"] = m
        print(f"  turned {name}")

    write_glb(dst, doc, blob)
    print(f"wrote {dst}")


if __name__ == "__main__":
    main()
