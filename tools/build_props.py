#!/usr/bin/env blender --background --python
"""Convert Elbolillo's two object packs into the game's two prop GLBs.

Reads the packs the owner downloaded (Sketchfab zip layout, FBX inside the
source rar -- extract it first so the .fbx files exist):

  ~/Downloads/exploration-objects*/         (or the scratchpad copy)
      source/Exploration objects/Exploration_objects.fbx     8 handheld tools
  ~/Downloads/objects-interiorvillage-alpha/
      source/Objects_Interior(Village)_Alpha/Models/*.fbx    198 objects:
          a complete demo house (House_Demo_*) plus a furniture showroom

and writes:

  Meshes/exploration_tools.glb   the 8 tools, names kept, CC0
  Meshes/village_objects.glb     the house + furniture, names kept, CC-BY-4.0

# What conversion changes, and nothing else

1. Names: `Cone.001` (the pack's unnamed spirit box) becomes `Spirit_Box`;
   the fridge shelf that shipped named bare `House_Demo` becomes
   `Fridge_Shelf` so `House_Demo_*` stays an unambiguous shell prefix.
   Empty stub meshes (0 vertices) are dropped.
2. Materials: metallic 0 / roughness 1 (PSX diffuse textures; glTF's default
   metallic 1 would render them as dark metal), and every `<Material>_Emissor*`
   texture that exists beside the pack is wired as the material's EMISSIVE map
   -- the loader packs emissive into its light texture and gltfpbr.frag adds
   it unlit, so screens and lampshades glow in the night scene for free.
3. One added mesh, `Lot_Ground`: a grass quad sized to the lot the game carves
   out of Liminal Neighborhood's stage (the baked lawn does not continue under
   the baked house -- measured, 47 of 60 rays through the house footprint hit
   nothing). Its texture is the stage's own `Pasto` grass, pulled byte-for-byte
   out of Meshes/abandoned_house.glb so the patch matches the lawn around it.
   Same author, same licence file.

Everything else -- geometry, UVs, the pack's own layout positions -- ships as
authored. The engine re-anchors parts by their bounds at spawn.

Run:  /Applications/Blender.app/Contents/MacOS/Blender --background \\
          --python tools/build_props.py
"""

import bpy
import json
import os
import struct
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCRATCH = os.environ.get(
    "PROPS_SRC",
    "/private/tmp/claude-501/-Users-jeanrojas-backrooms/"
    "7e4dc15f-e0bf-4b98-938f-e47c450e054b/scratchpad/objpacks",
)
EXPLORE_FBX = os.path.join(
    SCRATCH, "explore/source/Exploration objects/Exploration_objects.fbx")
EXPLORE_TEX = os.path.dirname(EXPLORE_FBX)
VILLAGE_FBX = os.path.join(
    SCRATCH,
    "village/source/Objects_Interior(Village)_Alpha/Models/"
    "Objects_Interior(Village)_Demo.fbx")
VILLAGE_TEX = os.path.join(
    SCRATCH, "village/source/Objects_Interior(Village)_Alpha/Textures")
STAGE_GLB = os.path.join(ROOT, "Meshes", "abandoned_house.glb")

# The carved lot is model x[-58.4,-20.2] x gz[-141.5,-115.2] (38.2 x 26.3 m);
# the quad runs a little past the hole so its edge always hides under lawn.
LOT_W = 38.6
LOT_D = 26.7
# One grass tile every this many metres of lot -- eyeballed against the
# stage's own lawn texel density, then checked on a screenshot.
LOT_TILE = 2.0


def grass_from_stage():
    """Pull the Pasto (grass) texture bytes out of abandoned_house.glb."""
    with open(STAGE_GLB, "rb") as f:
        f.read(12)
        clen, _ = struct.unpack("<II", f.read(8))
        doc = json.loads(f.read(clen))
        blen, _ = struct.unpack("<II", f.read(8))
        blob = f.read(blen)
    mat = next(m for m in doc["materials"] if "pasto" in m.get("name", "").lower())
    tex = doc["textures"][mat["pbrMetallicRoughness"]["baseColorTexture"]["index"]]
    img = doc["images"][tex["source"]]
    view = doc["bufferViews"][img["bufferView"]]
    off = view.get("byteOffset", 0)
    data = blob[off:off + view["byteLength"]]
    ext = ".png" if "png" in img.get("mimeType", "image/png") else ".jpg"
    path = os.path.join(SCRATCH, "pasto" + ext)
    with open(path, "wb") as f:
        f.write(data)
    return path


def fresh_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def find_tex(tex_dirs, stem):
    for d in tex_dirs:
        for ext in (".png", ".jpeg", ".jpg"):
            p = os.path.join(d, stem + ext)
            if os.path.exists(p):
                return p
    return None


def psx_materials(tex_dirs):
    """Metallic 0, roughness 1; base colour by material name where the FBX
    reference did not resolve; wire <name>_Emissor* as the emissive map."""
    wired = []
    bare = []
    for mat in bpy.data.materials:
        if not mat.use_nodes:
            continue
        bsdf = next((n for n in mat.node_tree.nodes
                     if n.type == "BSDF_PRINCIPLED"), None)
        if bsdf is None:
            continue
        bsdf.inputs["Metallic"].default_value = 0.0
        bsdf.inputs["Roughness"].default_value = 1.0
        # The village FBX references its textures at authoring-machine paths;
        # the files themselves sit in Textures/ named after their material
        # (the stove is the one plural: material "Stoves", file "Stove").
        has_base = any(
            link.to_socket.name == "Base Color"
            and link.from_node.type == "TEX_IMAGE"
            and link.from_node.image is not None
            and link.from_node.image.has_data
            for link in mat.node_tree.links)
        if not has_base:
            p = find_tex(tex_dirs, mat.name) or find_tex(
                tex_dirs, mat.name.rstrip("s"))
            if p:
                img = bpy.data.images.load(p, check_existing=True)
                node = mat.node_tree.nodes.new("ShaderNodeTexImage")
                node.image = img
                mat.node_tree.links.new(node.outputs["Color"],
                                        bsdf.inputs["Base Color"])
            else:
                bare.append(mat.name)
        # Prefer the exact _Emissor, else the mid variant (the EMF detector
        # ships five display states _00.._04; a fixed mid reading is honest
        # for a prop that cannot swap maps at runtime).
        emis = (find_tex(tex_dirs, f"{mat.name}_Emissor")
                or find_tex(tex_dirs, f"{mat.name}_Emissor_02"))
        if emis:
            img = bpy.data.images.load(emis, check_existing=True)
            node = mat.node_tree.nodes.new("ShaderNodeTexImage")
            node.image = img
            mat.node_tree.links.new(node.outputs["Color"],
                                    bsdf.inputs["Emission Color"])
            bsdf.inputs["Emission Strength"].default_value = 1.0
            wired.append(mat.name)
    if bare:
        print("NO-TEXTURE materials (ship as flat colour):", sorted(bare))
    return wired


def export(path):
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.export_scene.gltf(
        filepath=path,
        export_format="GLB",
        export_yup=True,
        export_apply=False,
        export_animations=False,
        export_skins=False,
        export_image_format="AUTO",
        export_materials="EXPORT",
    )
    print(f"EXPORTED {path} {os.path.getsize(path) / 1e6:.2f} MB")


def manifest(tag):
    """Bounds per mesh object, in glTF space (x, y=up, z=-blender_y)."""
    from mathutils import Vector
    out = {}
    for o in bpy.context.scene.objects:
        if o.type != "MESH" or len(o.data.vertices) == 0:
            continue
        pts = [o.matrix_world @ Vector(c) for c in o.bound_box]
        bx = [min(p.x for p in pts), max(p.x for p in pts)]
        by = [min(p.z for p in pts), max(p.z for p in pts)]
        bz = [min(-p.y for p in pts), max(-p.y for p in pts)]
        out[o.name] = [round(v, 3) for v in bx + by + bz]
    print(f"MANIFEST {tag} {json.dumps(out)}")


def build_tools():
    fresh_scene()
    bpy.ops.import_scene.fbx(filepath=EXPLORE_FBX)
    for o in bpy.context.scene.objects:
        if o.name.startswith("Cone"):
            o.name = "Spirit_Box"
    wired = psx_materials([EXPLORE_TEX])
    print("TOOLS emissive on:", sorted(wired))
    manifest("tools")
    export(os.path.join(ROOT, "Meshes", "exploration_tools.glb"))


def build_village():
    fresh_scene()
    bpy.ops.import_scene.fbx(filepath=VILLAGE_FBX)
    dropped = 0
    for o in list(bpy.context.scene.objects):
        if o.type == "MESH" and len(o.data.vertices) == 0:
            bpy.data.objects.remove(o, do_unlink=True)
            dropped += 1
    for o in bpy.context.scene.objects:
        if o.name == "House_Demo":
            o.name = "Fridge_Shelf"
    wired = psx_materials([VILLAGE_TEX])
    print(f"VILLAGE dropped {dropped} empty meshes; emissive on:", sorted(wired))

    # The lot patch: a grass quad in its own local frame, centred at origin,
    # face up, tiled so one repeat covers LOT_TILE metres.
    grass = bpy.data.images.load(grass_from_stage())
    mat = bpy.data.materials.new("Lot_Pasto")
    mat.use_nodes = True
    bsdf = next(n for n in mat.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
    bsdf.inputs["Metallic"].default_value = 0.0
    bsdf.inputs["Roughness"].default_value = 1.0
    tex = mat.node_tree.nodes.new("ShaderNodeTexImage")
    tex.image = grass
    mat.node_tree.links.new(tex.outputs["Color"], bsdf.inputs["Base Color"])
    mesh = bpy.data.meshes.new("Lot_Ground")
    hw, hd = LOT_W / 2.0, LOT_D / 2.0
    mesh.from_pydata(
        [(-hw, -hd, 0), (hw, -hd, 0), (hw, hd, 0), (-hw, hd, 0)], [],
        [(0, 1, 2, 3)])
    uv = mesh.uv_layers.new()
    ru, rv = LOT_W / LOT_TILE, LOT_D / LOT_TILE
    for li, (u, v) in zip(range(4), [(0, 0), (ru, 0), (ru, rv), (0, rv)]):
        uv.data[li].uv = (u, v)
    mesh.materials.append(mat)
    quad = bpy.data.objects.new("Lot_Ground", mesh)
    bpy.context.scene.collection.objects.link(quad)

    manifest("village")
    export(os.path.join(ROOT, "Meshes", "village_objects.glb"))


build_tools()
build_village()
