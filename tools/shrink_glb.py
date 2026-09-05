"""Shrink the embedded PNG maps of a GLB to a fixed square, in place.

    python3 tools/shrink_glb.py Meshes/some_model.glb 512

# Why

The intro door shipped as a 79 MB GLB: ten PNGs, most of them 4096 square, which the runtime
loader (src/ext/gltf_model.rs) decoded on three threads and downscaled to the door's MAP = 512
square on every intro load -- 0.22 s and ~400 MB of transient RGBA for a door a few hundred
pixels tall on screen. The loader already threw the extra resolution away, so it is thrown away
here, once, and the asset shrinks to a few megabytes that decode in a few milliseconds.

# What is preserved

Everything except the image bytes. The GLB's two chunks are rebuilt: each image's PNG is
re-encoded at the target size, every bufferView is re-laid out in its original order with the
same 4-byte alignment, offsets and lengths are fixed up, `buffers[0].byteLength` and the chunk
and file lengths follow. Geometry and every other bufferView are copied byte for byte; the JSON
is re-serialised with the same compact separators (float spellings may differ, values do not).

The filter is PIL's BILINEAR, which at a downscale is a triangle filter whose support scales with
the ratio -- the same filter the runtime used (`image`'s Triangle), so the result is the map the
game was already showing. An image already at or below the target size is left untouched.
"""

import io
import json
import struct
import sys

from PIL import Image

JSON_CHUNK = 0x4E4F534A
BIN_CHUNK = 0x004E4942


def pad4(n):
    return (4 - n % 4) % 4


def shrink(path, size):
    data = open(path, "rb").read()
    magic, version, total = struct.unpack_from("<III", data, 0)
    assert magic == 0x46546C67 and version == 2, "not a GLB 2.0 file"
    jlen, jtype = struct.unpack_from("<II", data, 12)
    assert jtype == JSON_CHUNK
    doc = json.loads(data[20 : 20 + jlen])
    blen, btype = struct.unpack_from("<II", data, 20 + jlen)
    assert btype == BIN_CHUNK
    bin_start = 28 + jlen
    blob = data[bin_start : bin_start + blen]
    assert len(doc["buffers"]) == 1, "expected one buffer (the BIN chunk)"

    # Which bufferViews are images, and their replacement bytes.
    replaced = {}
    for img in doc.get("images", []):
        bv = img["bufferView"]
        view = doc["bufferViews"][bv]
        off = view.get("byteOffset", 0)
        src = blob[off : off + view["byteLength"]]
        im = Image.open(io.BytesIO(src))
        if max(im.size) <= size:
            print(f"  {img.get('name', bv)}: {im.size[0]}x{im.size[1]}, kept")
            continue
        out = im.resize((size, size), Image.BILINEAR)
        buf = io.BytesIO()
        out.save(buf, format="PNG", optimize=True)
        replaced[bv] = buf.getvalue()
        img["mimeType"] = "image/png"
        print(f"  {img.get('name', bv)}: {im.size[0]}x{im.size[1]} {im.mode} -> {size}x{size}, "
              f"{len(src)} -> {len(replaced[bv])} bytes")

    # Re-lay the BIN chunk in the original bufferView order.
    new_blob = bytearray()
    for i, view in enumerate(doc["bufferViews"]):
        assert view.get("buffer", 0) == 0
        off = view.get("byteOffset", 0)
        payload = replaced.get(i, blob[off : off + view["byteLength"]])
        new_blob.extend(b"\0" * pad4(len(new_blob)))
        view["byteOffset"] = len(new_blob)
        view["byteLength"] = len(payload)
        new_blob.extend(payload)
    new_blob.extend(b"\0" * pad4(len(new_blob)))
    doc["buffers"][0]["byteLength"] = len(new_blob)

    js = json.dumps(doc, separators=(",", ":")).encode("utf-8")
    js += b" " * pad4(len(js))
    out = bytearray()
    out += struct.pack("<III", magic, 2, 12 + 8 + len(js) + 8 + len(new_blob))
    out += struct.pack("<II", len(js), JSON_CHUNK) + js
    out += struct.pack("<II", len(new_blob), BIN_CHUNK) + new_blob
    open(path, "wb").write(out)
    print(f"{path}: {total} -> {len(out)} bytes")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    shrink(sys.argv[1], int(sys.argv[2]))
