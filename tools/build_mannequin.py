#!/usr/bin/env blender --background --python
"""Pack Mixamo's Mannequin and its clips into one GLB the game can load.

Reads `~/Downloads/mixamo_mannequin/` -- `Mannequin.fbx` (Mixamo's character
literally named "Mannequin", internal id Ch36_nonPBR) plus one
animation-only FBX per clip, including `zombie_pack/` -- and writes
`Meshes/mannequin.glb`: one mesh, one skeleton, every clip, named after its
file.

Nothing is retargeted here and nothing needs to be. Every file was
downloaded from Mixamo with the Mannequin selected, so the character and all
31 clips carry the same 65-bone `mixamorig1:` skeleton; merging is a straight
action copy. That is the whole reason this asset exists -- two earlier
attempts to rig a cast by hand were thrown out, and this path has no rigging
step to get wrong.

Run:  /Applications/Blender.app/Contents/MacOS/Blender --background \\
          --python tools/build_mannequin.py

# Two Blender traps this file is built around

1. **Slotted actions.** Blender 4.4+ binds an action to an ID through a SLOT.
   `animation_data.action = act` alone leaves the rig in its rest pose --
   which looks exactly like a clip that imported fine and does nothing.
2. **`animation_data_clear()` before export** orphans every imported action
   and the exporter then writes a file with a perfect skin and ZERO
   animations, silently. Set `action = None` instead and keep the block.
"""

import bpy
import os
import re
import sys

SRC = os.path.expanduser("~/Downloads/mixamo_mannequin")
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "Meshes", "mannequin.glb")

# Mixamo ships the Mannequin with four 4K maps; the game samples base colour
# only (`ext/skinned.rs` feeds 1x1 stand-ins to the other units), and this is
# a PS1-looking game. 1024 keeps the file a few megabytes.
MAX_MAP = 1024


def action_slots_bind(rig, action):
    rig.animation_data_create()
    rig.animation_data.action = action
    slots = getattr(action, "slots", None)
    if slots:
        try:
            rig.animation_data.action_slot = slots[0]
        except TypeError:
            pass


def slug(path):
    name = os.path.splitext(os.path.basename(path))[0].lower()
    return re.sub(r"[^a-z0-9]+", "_", name).strip("_")


def anim_files():
    out = []
    for root, _dirs, files in os.walk(SRC):
        for f in sorted(files):
            if not f.endswith(".fbx") or f == "Mannequin.fbx":
                continue
            out.append(os.path.join(root, f))
    return out


def main():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.fbx(filepath=os.path.join(SRC, "Mannequin.fbx"))
    rig = next(o for o in bpy.context.scene.objects if o.type == "ARMATURE")
    meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    for m in meshes:
        m.name = "Mannequin"
    print(f"CHARACTER bones={len(rig.data.bones)} meshes={len(meshes)} "
          f"verts={sum(len(m.data.vertices) for m in meshes)}")
    # The character's own FBX carries a one-frame T-pose action; drop it so it
    # cannot be mistaken for a clip.
    for a in list(bpy.data.actions):
        bpy.data.actions.remove(a)

    kept = []
    for path in anim_files():
        name = slug(path)
        before_a = set(bpy.data.actions)
        before_o = set(bpy.context.scene.objects)
        try:
            bpy.ops.import_scene.fbx(filepath=path)
        except Exception as e:
            print(f"  FAIL {name}: {repr(e)[:70]}")
            continue
        fresh_a = [a for a in bpy.data.actions if a not in before_a]
        for o in [o for o in bpy.context.scene.objects if o not in before_o]:
            bpy.data.objects.remove(o, do_unlink=True)
        if not fresh_a:
            print(f"  EMPTY {name}")
            continue
        act = fresh_a[0]
        act.name = name
        act.use_fake_user = True
        for extra in fresh_a[1:]:
            bpy.data.actions.remove(extra)
        kept.append(name)
    print(f"CLIPS {len(kept)}: {', '.join(kept[:8])}{' ...' if len(kept) > 8 else ''}")

    for img in bpy.data.images:
        if max(img.size) > MAX_MAP:
            img.scale(min(img.size[0], MAX_MAP), min(img.size[1], MAX_MAP))

    # Rest pose, animation_data KEPT (see the module docs, trap 2).
    rig.animation_data_create()
    rig.animation_data.action = None
    for pb in rig.pose.bones:
        pb.matrix_basis.identity()

    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.export_scene.gltf(
        filepath=OUT,
        export_format="GLB",
        export_yup=True,
        export_apply=False,
        export_animations=True,
        export_animation_mode="ACTIONS",
        export_force_sampling=True,
        export_optimize_animation_size=False,
        export_skins=True,
        export_image_format="AUTO",
        export_materials="EXPORT",
    )
    print(f"EXPORTED {OUT} {os.path.getsize(OUT) / 1e6:.1f} MB")


if __name__ == "__main__":
    main()
