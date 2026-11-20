"""Build the first-person and world weapons from pinned CC0 source packs.

    tools/build_weapons.sh [key ...]          # the whole set, one command
    blender -b --python tools/build_weapons.py -- [key ...]   # this step only

The sources come from tools/fetch_weapon_sources.py (pinned by SHA-256 and
cached in ~/.cache/shooter-weapons/sources). For each weapon in WEAPONS this
writes, into godot/assets/weapons/<dir>/:

    <key>.glb            the weapon in metres, barrel along -Z, +X to the
                         right, origin on the sight line at the rear sight
                         (so the view scene's Model needs no transform). Its
                         nodes are `Body` and the moving parts (PARTS below),
                         each with its pivot as its origin.
    <key>_albedo.png     colour (sRGB)
    <key>_normal.png     tangent-space normal map, OpenGL (green up)
    <key>_orm.png        R occlusion, G roughness, B metallic
    <key>.tres           the StandardMaterial3D the glb's "Weapon" material
                         maps to (through its .import file)

and patches the marker positions (Muzzle, Ejection, OpticMount, GripRight,
GripLeft) in godot/scenes/weapons/<key>_view.tscn, so the scene follows the
model. Rounds use the shared cartridge material and tritium dots the shared
glow material (godot/assets/weapons/shared/, from tools/build_weapon_shared.py).

How a source becomes a weapon:

1. Import the rigged FBX in its rest pose and drop the rig. Each vertex keeps
   the bone that moves it, so every face goes to the part its bone belongs to
   (the magazine, the slide, the hammer, the rounds in the magazine).
2. Turn it to face forward, scale it to the real weapon's size, and move the
   sight line onto the origin.
3. Repack the source's PBR maps into the game's layout (OpenGL normals and
   an ORM map) at 2048 px. The source's own UVs stay, so the texel density
   the artist chose is kept.
4. Export one node per part, each with the source bone's head (or the part's
   centre, for parts that only slide) as its origin.

Needs Blender 4.2+ with numpy. Takes a few seconds per weapon.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
import weapon_finish
import weapon_m870
import weapon_shapes
import weapon_optics

REPO = Path(__file__).resolve().parent.parent
WEAPONS_DIR = REPO / "godot/assets/weapons"
SCENES_DIR = REPO / "godot/scenes/weapons"
SOURCES = Path.home() / ".cache/shooter-weapons/sources"
CLASSIC = SOURCES / "classic_weapons_pack_v1.1/WeaponsPack"
GUNS = SOURCES / "guns_explosives/Guns&Explosives"
SHARED = "res://assets/weapons/shared"
TEXTURE_SIZE = 2048

# Every weapon, keyed as the game names it (data/weapons/<key>.ron).
#
#   dir, scene     asset folder and view scene name (`<scene>_view.tscn`)
#   fbx, textures  source files; textures are colour, normal, and an RMAO map
#                  (R roughness, G metallic, B occlusion)
#   normal_dx      the normal map is DirectX style (green down)
#   scale          source units to metres, from the cartridge length (the
#                  rounds in the source's magazine) against the real round
#   sight          (y, z) in source space of the rear sight's aiming point
#                  (the top of a notch's shoulders, an aperture's centre)
#                  and of the front post's tip. The rear point becomes the
#                  view's origin, and the weapon pitches so the line through
#                  both is the view's -Z axis: aimed, the post tip sits level
#                  with the notch, exactly on the camera's axis. An
#                  optional `x` is the sight line's offset from the source's
#                  centre line, for packs whose sights sit off it.
#   sight_edits    reshapes the sights for a readable aimed picture (see
#                  `edit_sights`): `aperture` opens a rear peep, `widen`
#                  widens a notch or post
#   parts          source bone -> node path. Faces moved by any other bone
#                  are `Body`. `None` collects faces with no bone.
#   pivots         node -> bone whose head is the node's origin; other
#                  parts use the centre of their bounds
#   drop           nodes left out of the export
#   markers        view scene marker -> bone (its head, moved onto the
#                  centre line) or (y, z) on the centre line or (x, y, z), in
#                  source space
#   refinish       node -> (u, v): box-project that part's UVs, at the
#                  texture's own density, onto the atlas patch centred at
#                  (u, v), for a part the source paints in a finish that does
#                  not match the gun (the AK's near-white charging handle)
#   thin           node -> share: squeezes that part across the gun toward
#                  the receiver, keeping this share of its thickness (the
#                  source's AK selector lever stands 7.5 mm proud; the real
#                  stamping is about 3 mm)
#   spare          also export `MagazineSpare`, a second magazine with its
#                  rounds, for reloads that bring a fresh one up
#   forward        "+Y" for packs that face +Y with +X to the right (the
#                  default is -Y with +X to the left)
#   rounds         for a magazine modelled empty: adds the top rounds as
#                  bone "ROUNDS" (map it to `Magazine/Rounds`). `profile` is
#                  a CARTRIDGE_PROFILES key, `back` the (y, z) in source
#                  space of the top round's case head, `count` rounds
#                  stacked down and staggered left and right
# The AK pattern's sights (the AK-47 and the LSW-6). The post is 1.7 mm
# wide, 3 px at half a metre: thicken it to 3.4 mm, and open the rear notch
# from 2.8 to 4 mm to keep light either side of it.
AK_SIGHT_EDITS = [
    {"kind": "widen", "at": 0.0, "depth": 0.0015,
     "z": (-0.004, 0.0005), "half": (0.0014, 0.002), "keep": 0.0045},
    {"kind": "widen", "front": True, "at": 0.0, "depth": 0.004,
     "z": (-0.0038, 0.0005), "half": (0.00083, 0.0017), "keep": 0.0035},
]

# The middle of the AK atlas's dust cover (UV, v up): worn blued steel.
AK_COVER_FINISH = (0.27, 0.445)
# The source atlases' texel density, in UV per metre (about 4.5 texels/mm at
# 2048 px).
ATLAS_UV_PER_M = 4.5 * 1000.0 / TEXTURE_SIZE

WEAPONS = {
    "m4": {
        "dir": "m4",
        "scene": "m4",
        "fbx": GUNS / "M4A1/M4A1.fbx",
        "textures": {
            "color": GUNS / "M4A1/M4A1_Base_Color.png",
            "normal": GUNS / "M4A1/M4A1_Normal.png",
            "metallic": GUNS / "M4A1/M4A1_Metallic.png",
            "roughness": GUNS / "M4A1/M4A1_Roughness.png",
        },
        "forward": "+Y",
        # An M4A1 is 838 mm long with the stock out (the source's 859 mm).
        "scale": 0.976,
        "cartridge": 0,
        # The flip-up rear aperture's centre and the front post's tip. Both
        # sit 0.8 mm left of the source's centre line.
        "sight": {"rear": (-0.0693, 0.0774), "front": (0.3452, 0.0828), "x": -0.00079},
        # The leaf's peep is a 2.4 mm precision aperture: a pinhole in a
        # large ring at aimed distance. Open it to the 4.6 mm close-range
        # aperture, and thicken the post to 2.4 mm so it reads inside it.
        "sight_edits": [
            {"kind": "aperture", "group": "Sight_2", "at": 0.0014, "depth": 0.0025,
             "hole": (0.00118, 0.0023), "rim": 0.0046},
            {"kind": "widen", "front": True, "at": 0.0, "depth": 0.004,
             "z": (-0.006, 0.0005), "half": (0.00073, 0.0012), "keep": 0.0022},
        ],
        "rounds": {"profile": "5.56", "back": (0.024, 0.001), "count": 3},
        "parts": {
            "Magazine": "Magazine",
            "ROUNDS": "Magazine/Rounds",
            "Charging_Handle": "ChargingHandle",
            "Ejector_2": "BoltCarrier",
            "Ejector_Lid": "DustCover",
            "Firemode_Selector": "Selector",
            "Trigger": "Trigger",
            "Sight_2": "RearSight",
        },
        "pivots": {
            "DustCover": "Ejector_Lid",
            "Selector": "Firemode_Selector",
            "Trigger": "Trigger",
            # The leaf's hinge, at the bottom of the leaf.
            "RearSight": (-0.0693, 0.0565),
        },
        "markers": {
            "Muzzle": (0.5359, 0.0209),
            # The port's rear edge, on the right side.
            "Ejection": (0.016, 0.006, 0.026),
            "OpticMount": (0.02, 0.0512),
            "GripRight": (-0.0955, -0.071),
            "GripLeft": (0.2, 0.005),
        },
        "handle": "rear",
        "spare": True,
        # The source's maps are flat (one grey, an empty normal map), so the
        # finish is painted from the geometry (tools/weapon_finish.py).
        "finish": {
            "default": "aluminium",
            "unwrap": True,
            "bore": (-0.0008, 0.0209),
            "zones": [
                # The pack's own metallic map marks the grip and stock.
                {"zone": "polymer", "source_metal_below": 0.5},
                {"zone": "bolt", "parts": ["Ejector_2"]},
                # The rear sight and its leaf, and the front sight's tower.
                {"zone": "sight", "parts": ["Sight", "Sight_2"]},
                {"zone": "sight", "parts": ["Barrel"], "y_above": 0.31, "y_below": 0.37, "z_above": 0.04},
                {"zone": "magazine", "parts": ["Magazine"]},
                # The quad rail round the barrel is aluminium; the barrel,
                # the front sight, and the flash hider are steel.
                {"zone": "aluminium", "parts": ["Barrel"], "y_below": 0.305, "bore_radius_above": 0.014},
                {"zone": "steel", "parts": ["Barrel", "Ejector_Lid", "Firemode_Selector", "Trigger",
                                            "Switch1", "Switch2"]},
            ],
            # Left side of the lower receiver, the side the player sees:
            # the selector's positions round its pivot, and the rollmark on
            # the magazine well.
            # Letters are larger than a real rollmark's 3 mm, so they stay
            # legible at the texture's 2.6 texels per millimetre.
            "font": REPO / "godot/assets/fonts/BarlowCondensed-Bold.ttf",
            "markings": [
                {"text": "SAFE", "at": (-0.037, -0.006), "size": 0.0052},
                {"text": "SEMI", "at": (-0.060, 0.004), "size": 0.0052},
                {"text": "AUTO", "at": (-0.084, -0.008), "size": 0.0052},
                {"text": "M4A1 CARBINE\nCAL 5.56 MM", "at": (0.058, -0.016), "size": 0.0072},
            ],
        },
    },
    # Modelled from real dimensions by tools/weapon_m870.py (the pack's
    # shotgun was a light sporting pump with painted-on ports).
    "m870": {
        "dir": "m870",
        "scene": "m870",
        "model": "m870",
        "spare": False,
    },
    "ak47": {
        "dir": "ak47",
        "scene": "ak47",
        "fbx": CLASSIC / "AK47/SKM_AK47.fbx",
        "textures": [CLASSIC / f"AK47/T_AK47_{s}.png" for s in ("C", "N", "RMAO")],
        "normal_dx": True,
        # An AK-47 is 880 mm long with a 378 mm sight radius.
        "scale": 0.855,
        "sight": {"rear": (-0.218, 0.1472), "front": (-0.6723, 0.1441)},
        "sight_edits": AK_SIGHT_EDITS,
        "parts": {
            "MAGAZINE": "Magazine",
            "BULLET": "Magazine/Rounds",
            "SLIDE": "ChargingHandle",
            "SELECTOR": "Selector",
            "TRIGGER": "Trigger",
            "EJECTOR": "MagRelease",
            None: "OpticRail",
        },
        "pivots": {"Selector": "SELECTOR", "Trigger": "TRIGGER", "MagRelease": "EJECTOR"},
        "markers": {
            "Muzzle": "FX_FIRE",
            # The port's rear edge, on the right side.
            "Ejection": (-0.014, -0.15, 0.112),
            "OpticMount": "ATTACH_SCOPE",
            "GripRight": (-0.016, -0.004),
            "GripLeft": (-0.39, 0.075),
        },
        # The source paints the charging handle and selector lever near-white
        # and polished, and the lever thick: give both the dust cover's worn
        # blued steel, and thin the lever.
        "refinish": {"Selector": AK_COVER_FINISH, "ChargingHandle": AK_COVER_FINISH},
        "thin": {"Selector": 0.4},
        "handle": "+x",
        "spare": True,
    },
    "lsw6": {
        "dir": "lsw6",
        "scene": "lsw6",
        # An RPK pattern, derived from the AK-47: the barrel 175 mm longer,
        # a bipod under the front sight, and a 75-round drum.
        "fbx": CLASSIC / "AK47/SKM_AK47.fbx",
        "textures": [CLASSIC / f"AK47/T_AK47_{s}.png" for s in ("C", "N", "RMAO")],
        "normal_dx": True,
        "scale": 0.855,
        # Everything ahead of the gas block moves forward (source -Y): the
        # bare barrel between it and the front sight stretches.
        "stretch": {"from": -0.56, "by": -0.205},
        "sight": {"rear": (-0.218, 0.1472), "front": (-0.8773, 0.1441)},
        "sight_edits": AK_SIGHT_EDITS,
        "drum": {"tower": "MAGAZINE", "rounds": "7.62x39"},
        "bipod": {"at": -0.80},
        "parts": {
            "MAGAZINE": "DropMagazine",
            "BULLET": "DropMagazine",
            "DRUM": "Magazine",
            "ROUNDS": "Magazine/Rounds",
            "BIPOD": "Bipod",
            "SLIDE": "ChargingHandle",
            "SELECTOR": "Selector",
            "TRIGGER": "Trigger",
            "EJECTOR": "MagRelease",
            None: "OpticRail",
        },
        "pivots": {
            "Magazine": "MAGAZINE",
            "Selector": "SELECTOR",
            "Trigger": "TRIGGER",
            "MagRelease": "EJECTOR",
        },
        "drop": ["DropMagazine"],
        "markers": {
            "Muzzle": "FX_FIRE",
            "Ejection": (-0.014, -0.15, 0.112),
            "OpticMount": "ATTACH_SCOPE",
            "GripRight": (-0.023, -0.004),
            "GripLeft": (-0.39, 0.075),
        },
        # The source paints the charging handle and selector lever near-white
        # and polished, and the lever thick: give both the dust cover's worn
        # blued steel, and thin the lever.
        "refinish": {"Selector": AK_COVER_FINISH, "ChargingHandle": AK_COVER_FINISH},
        "thin": {"Selector": 0.4},
        "handle": "+x",
        "spare": True,
    },
    "kr12": {
        "dir": "kr12",
        "scene": "kr12",
        # A modernised AK in 7.62 mm, from the AK-47: the walnut stock,
        # grip, and handguard refinished as black polymer.
        "fbx": CLASSIC / "AK47/SKM_AK47.fbx",
        "textures": [CLASSIC / f"AK47/T_AK47_{s}.png" for s in ("C", "N", "RMAO")],
        "normal_dx": True,
        "wood_to": {"color": (0.075, 0.075, 0.072), "roughness": 0.64, "keep": 0.9},
        "scale": 0.855,
        "sight": {"rear": (-0.218, 0.1472), "front": (-0.6723, 0.1441)},
        "sight_edits": AK_SIGHT_EDITS,
        "parts": {
            "MAGAZINE": "Magazine",
            "BULLET": "Magazine/Rounds",
            "SLIDE": "ChargingHandle",
            "SELECTOR": "Selector",
            "TRIGGER": "Trigger",
            "EJECTOR": "MagRelease",
            None: "OpticRail",
        },
        "pivots": {"Selector": "SELECTOR", "Trigger": "TRIGGER", "MagRelease": "EJECTOR"},
        "markers": {
            "Muzzle": "FX_FIRE",
            "Ejection": (-0.014, -0.15, 0.112),
            "OpticMount": "ATTACH_SCOPE",
            "GripRight": (-0.016, -0.004),
            "GripLeft": (-0.39, 0.075),
        },
        "refinish": {"Selector": AK_COVER_FINISH, "ChargingHandle": AK_COVER_FINISH},
        "thin": {"Selector": 0.4},
        "handle": "+x",
        "spare": True,
    },
    "m1911": {
        "dir": "pistol",
        "scene": "pistol",
        "fbx": CLASSIC / "1911/SKM_1911.fbx",
        "textures": [CLASSIC / f"1911/{s}.png" for s in ("T_1911_C", "T_1991_N", "T_1911_RMAO")],
        "normal_dx": True,
        # An M1911 is 216 mm long with a 165 mm sight radius.
        "scale": 0.93,
        "sight": {"rear": (0.0106, 0.0895), "front": (-0.1667, 0.0878)},
        "cartridge": 1,
        "parts": {
            "SLIDE": "Slide",
            "MAGAZINE": "Magazine",
            "BULLET": "Magazine/Rounds",
            "HAMMER": "Hammer",
            "LEVER": "SlideStop",
            "SAFETY": "Safety",
            "TRIGGER": "Trigger",
            "RELEASE": "MagRelease",
            None: "OpticRail",
        },
        "pivots": {
            "Hammer": "HAMMER",
            "SlideStop": "LEVER",
            "Safety": "SAFETY",
            "Trigger": "TRIGGER",
            "MagRelease": "RELEASE",
        },
        "drop": ["OpticRail"],
        "markers": {
            "Muzzle": "FX_FIRE",
            "Ejection": (-0.011, -0.035, 0.075),
            "GripRight": (0.022, 0.0),
            "GripLeft": (0.035, -0.02),
        },
        "spare": True,
    },
    "pdw9": {
        "dir": "pdw9",
        "scene": "pdw9",
        "fbx": CLASSIC / "MP5/SKM_MP5.fbx",
        "textures": [CLASSIC / f"MP5/T_MP5_{s}.png" for s in ("C", "N", "RMAO")],
        "normal_dx": True,
        # An MP5A5 is 700 mm long with its stock out; 9x19 mm rounds.
        "scale": 0.98,
        "cartridge": 1,
        # It carries a reflex sight on the claw mount (the scene's IronSights
        # node): the sight line runs 18 mm over the mount's top, level.
        "sight": {"rear": (-0.1207, 0.2008), "front": (-0.37, 0.2008)},
        "parts": {
            "MAGASINE": "Magazine",
            "BULLET": "Magazine/Rounds",
            "BOLT": "ChargingHandle",
            "BOLT_02": "BoltCarrier",
            "SELECTOR": "Selector",
            "TRIGGER": "Trigger",
            "MAGASINE_RELEASE": "MagRelease",
            None: "OpticRail",
        },
        "pivots": {"Selector": "SELECTOR", "Trigger": "TRIGGER", "MagRelease": "MAGASINE_RELEASE"},
        "markers": {
            "Muzzle": "FX_FIRE",
            "Ejection": (-0.016, -0.128, 0.137),
            "OpticMount": "ATTACH_SCOPE",
            "GripRight": (0.0051, 0.0),
            "GripLeft": (-0.29, 0.09),
        },
        "handle": "-x",
        "spare": True,
    },
    "cq10": {
        "dir": "cq10",
        "scene": "cq10",
        # A MAC-11 pattern in a rail cage, with a wire stock and a 32-round
        # magazine; scaled so its rounds are 9x19 mm.
        "fbx": CLASSIC / "Mac10/SKM_Mac10.fbx",
        "textures": [CLASSIC / f"Mac10/T_Mac10_{s}" for s in ("C.png", "N.png", "RMAO.PNG")],
        "normal_dx": True,
        "scale": 1.08,
        "cartridge": 1,
        # A reflex sight on the cage's top rail (the scene's IronSights
        # node): the sight line runs 18 mm over the rail, level, from over
        # the optic mount.
        "sight": {"rear": (-0.0324, 0.1200), "front": (-0.25, 0.1200)},
        "parts": {
            "MAGAZINE": "Magazine",
            "BULLET": "Magazine/Rounds",
            "SLIDE": "ChargingHandle",
            "SELECTOR": "Selector",
            "MAG_RELEASE": "MagRelease",
            None: "OpticRail",
        },
        "pivots": {"Selector": "SELECTOR", "MagRelease": "MAG_RELEASE"},
        "markers": {
            "Muzzle": "FX_FIRE",
            # The port on the right side, over the magazine.
            "Ejection": (-0.013, -0.005, 0.0665),
            "OpticMount": "ATTACH_SCOPE",
            # The middle of the pistol grip (the magazine well).
            "GripRight": (0.004, -0.012),
            # The support hand under the cage's front rail.
            "GripLeft": (-0.118, 0.028),
            # The cocking knob on top, left of the rail's window.
            "ChargingHandle": (-0.013, 0.09),
        },
        "spare": True,
    },
    "bp9": {
        "dir": "bp9",
        "scene": "bp9",
        # A TEC-9 pattern: a ventilated barrel shroud, a 32-round magazine
        # ahead of the trigger guard, and a scope mount on the receiver.
        # The pack ships only its colour map; the rest is derived from it.
        "fbx": CLASSIC / "Tec9/SKM_Tec9.fbx",
        "textures": [CLASSIC / "Tec9/T_Tec9_C.png"],
        # A TEC-9 is 317 mm long with its 127 mm barrel; 9x19 mm rounds.
        "scale": 1.0,
        "cartridge": 1,
        # The pack's sights stand flush with the receiver's rear cap, which
        # hides the post: the scene's IronSights node builds a notched rear
        # blade over the back of the receiver and a post on the shroud's
        # front, their line 6.4 mm over the receiver.
        "sight": {"rear": (0.0, 0.1205), "front": (-0.2525, 0.1205)},
        "parts": {
            "MAGAZINE": "Magazine",
            "BULLET": "Magazine/Rounds",
            "SLIDE": "ChargingHandle",
            "TRIGGER": "Trigger",
            "MAG_RELEASE": "MagRelease",
            None: "OpticRail",
        },
        "pivots": {"Trigger": "TRIGGER", "MagRelease": "MAG_RELEASE"},
        # The scope mount's block would stand in the sight picture; a red
        # dot brings its own riser.
        "drop": ["OpticRail"],
        "markers": {
            "Muzzle": "FX_FIRE",
            "Ejection": (-0.012, -0.1159, 0.0928),
            "OpticMount": "ATTACH_SCOPE",
            # The middle of the pistol grip, and the support hand round the
            # magazine well.
            "GripRight": (0.040, -0.030),
            "GripLeft": (-0.123, -0.005),
        },
        "handle": "-x",
        "spare": True,
    },
    "br3": {
        "dir": "br3",
        "scene": "br3",
        "fbx": CLASSIC / "M14/SKM_M14.fbx",
        "textures": [CLASSIC / f"M14/T_M14_{s}.png" for s in ("C", "N", "RMAO")],
        "normal_dx": True,
        # An M14 is 1118 mm long; 7.62x51 mm rounds.
        "scale": 1.02,
        "cartridge": 0,
        # A reflex sight on the forward rail, high enough to clear the rear
        # aperture sight.
        "sight": {"rear": (-0.2, 0.105), "front": (-0.6, 0.105)},
        "parts": {
            "MAGAZINE": "Magazine",
            "BULLET_01": "Magazine/Rounds",
            "BULLET_02": "Magazine/Rounds",
            "BULLET_03": "Magazine/Rounds",
            "OPERATING_HANDLE": "ChargingHandle",
            "BOLT": "BoltCarrier",
            "BOLT_STOP": "BoltStop",
            "SAFETY": "Safety",
            "SELECTOR": "Selector",
            "TRIGGER": "Trigger",
            "MAGAZINE_RELEASE": "MagRelease",
            None: "OpticRail",
        },
        "pivots": {
            "Safety": "SAFETY",
            "Selector": "SELECTOR",
            "Trigger": "TRIGGER",
            "MagRelease": "MAGAZINE_RELEASE",
            "BoltStop": "BOLT_STOP",
        },
        "markers": {
            "Muzzle": "FX_FIRE",
            "Ejection": (-0.012, -0.17, 0.06),
            "OpticMount": (-0.2, 0.081),
            "GripRight": (0.0396, -0.02),
            "GripLeft": (-0.36, 0.02),
        },
        "handle": "+x",
        "spare": True,
    },
    "dm7": {
        "dir": "dm7",
        "scene": "dm7",
        # A marksman M14: the pack's rifle with a riflescope on its receiver
        # mount (`riflescope`, as the M40's) and its walnut refinished as
        # olive glass-fibre (`wood_to`).
        "fbx": CLASSIC / "M14/SKM_M14.fbx",
        "textures": [CLASSIC / f"M14/T_M14_{s}.png" for s in ("C", "N", "RMAO")],
        "normal_dx": True,
        "wood_to": {"color": (0.27, 0.285, 0.215), "roughness": 0.74, "keep": 0.7, "flatten": 0.85},
        "scale": 1.02,
        "cartridge": 0,
        # The scope's axis, 28 mm over the receiver mount (medium rings):
        # the sight line, level, with the eye 9 cm behind the rear ring.
        "sight": {"rear": (-0.065, 0.109), "front": (-0.4, 0.109)},
        "scope": {"rail": 0.0809, "rings": (-0.23, -0.155)},
        "parts": {
            "MAGAZINE": "Magazine",
            "BULLET_01": "Magazine/Rounds",
            "BULLET_02": "Magazine/Rounds",
            "BULLET_03": "Magazine/Rounds",
            "OPERATING_HANDLE": "ChargingHandle",
            "BOLT": "BoltCarrier",
            "BOLT_STOP": "BoltStop",
            "SAFETY": "Safety",
            "SELECTOR": "Selector",
            "TRIGGER": "Trigger",
            "MAGAZINE_RELEASE": "MagRelease",
            None: "Body",
        },
        "pivots": {
            "Safety": "SAFETY",
            "Selector": "SELECTOR",
            "Trigger": "TRIGGER",
            "MagRelease": "MAGAZINE_RELEASE",
            "BoltStop": "BOLT_STOP",
        },
        "markers": {
            "Muzzle": "FX_FIRE",
            "Ejection": (-0.012, -0.17, 0.06),
            "GripRight": (0.0396, -0.02),
            "GripLeft": (-0.36, 0.02),
        },
        "handle": "+x",
        "spare": True,
    },
    "m40": {
        "dir": "m40",
        "scene": "m40",
        "fbx": CLASSIC / "M700/SKM_M700.fbx",
        "textures": [CLASSIC / f"M700/T_M700_{s}.png" for s in ("C", "N", "RMAO")],
        "normal_dx": True,
        # A Remington 700 pattern: 1117 mm with a 24 inch barrel.
        "scale": 1.0,
        "cartridge": 0,
        # The scope's axis, 28 mm over the rail (medium rings): the sight
        # line, level. The scope is built by `riflescope`.
        "sight": {"rear": (0.0, 0.080), "front": (-0.3, 0.080)},
        "scope": {"rail": 0.052, "rings": (-0.19, -0.09)},
        "parts": {
            "MAGAZINE": "Magazine",
            "BULLET_01": "Magazine/Rounds",
            "BULLET_02": "Magazine/Rounds",
            "BOLT": "Bolt",
            "BOLT_STOP": "BoltStop",
            "SAFETY": "Safety",
            "TRIGGER": "Trigger",
            None: "Body",
        },
        "pivots": {"Safety": "SAFETY", "Trigger": "TRIGGER", "BoltStop": "BOLT_STOP"},
        "markers": {
            "Muzzle": "FX_FIRE",
            "Ejection": (-0.014, -0.125, 0.035),
            "GripRight": (0.05, -0.05),
            "GripLeft": (-0.36, 0.0),
        },
        "spare": False,
        # The pack's rifle is a sporting 700 (walnut, a pencil barrel). The
        # M40 is the military pattern (`militarize`): a heavy barrel, 24 mm
        # at the muzzle instead of 18; a wider, flat-sided beavertail
        # forend; a raised cheek piece on two posts; and the stock refinished
        # as olive glass fibre with a sprayed web.
        "military": {
            "barrel": {"ahead_of": -0.50, "axis": (0.0, 0.0304), "within": 0.0125, "scale": 1.33},
            "forend": {"y": (-0.53, -0.16), "widen": 1.32},
            "cheek": {"y": (0.035, 0.195), "bottom": (0.0185, 0.0075), "top": (0.0365, 0.0345),
                      "half": 0.0148, "posts": (0.070, 0.165)},
        },
        "finish": {
            "default": "steel",
            "unwrap": True,
            "zones": [
                {"zone": "bolt", "parts": ["BOLT"]},
                {"zone": "fibreglass", "parts": ["CHEEK"]},
                {"zone": "steel", "parts": ["CHEEKPOST"]},
                # The butt pad, behind the stock.
                {"zone": "rubber", "y_above": 0.29, "source_wood": False},
                # The pack's walnut; ahead of the forend's tip, warm texels
                # on the barrel are not wood.
                {"zone": "fibreglass", "source_wood": True, "y_above": -0.53},
            ],
        },
    },
}

# Source materials that are not the weapon's own texture set.
CARTRIDGE_MATERIALS = ("BULLETS",)
GLOW_MATERIALS = ("GlowSight",)


def log(*args):
    print("[build_weapons]", *args, flush=True)


def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)


class Source:
    """The imported source: one mesh object in source space, its bones'
    head positions, and the bone that moves each vertex.

    Packs that split the moving parts into separate objects instead of
    rigging them (no armature) work the same way: each object counts as a
    bone named after it, with its origin as the bone's head."""

    def __init__(self, spec: dict):
        bpy.ops.import_scene.fbx(filepath=str(spec["fbx"]))
        armature = next((o for o in bpy.data.objects if o.type == "ARMATURE"), None)
        meshes = [o for o in bpy.data.objects if o.type == "MESH"]
        if armature is not None:
            self.bones = {b.name: armature.matrix_world @ b.head_local for b in armature.data.bones}
        else:
            self.bones = {o.name: o.matrix_world.translation.copy() for o in meshes}
        for obj in meshes:
            for mod in list(obj.modifiers):
                obj.modifiers.remove(mod)
            if armature is not None:
                obj.parent = None
                obj.matrix_world = armature.matrix_world @ obj.matrix_parent_inverse @ obj.matrix_basis
            else:
                world = obj.matrix_world.copy()
                obj.parent = None
                obj.matrix_world = world
                obj.vertex_groups.clear()
                group = obj.vertex_groups.new(name=obj.name)
                group.add([v.index for v in obj.data.vertices], 1.0, "REPLACE")
            # Faces with no bone get a group of their own.
            if not obj.vertex_groups:
                group = obj.vertex_groups.new(name="<none>")
                group.add([v.index for v in obj.data.vertices], 1.0, "REPLACE")
            # Faces with no material take the weapon's.
            if not obj.material_slots or not any(slot.material for slot in obj.material_slots):
                obj.data.materials.clear()
                obj.data.materials.append(bpy.data.materials.get("Source") or bpy.data.materials.new("Source"))
        for obj in [o for o in bpy.data.objects if o.type not in ("MESH",)]:
            bpy.data.objects.remove(obj)
        bpy.ops.object.select_all(action="DESELECT")
        for obj in meshes:
            obj.select_set(True)
        bpy.context.view_layer.objects.active = meshes[0]
        if len(meshes) > 1:
            bpy.ops.object.join()
        self.obj = bpy.context.view_layer.objects.active
        bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
        triangulate_ngons(self.obj)


def triangulate_ngons(obj):
    """Splits faces of five or more corners (tangents need tris or quads),
    keeping the custom normals."""
    if not any(len(p.vertices) > 4 for p in obj.data.polygons):
        return
    mod = obj.modifiers.new("Triangulate", "TRIANGULATE")
    mod.min_vertices = 5
    mod.keep_custom_normals = True
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.modifier_apply(modifier=mod.name)


def to_view(spec: dict) -> Matrix:
    """Source space -> Blender space of the export: turned to face +Y (glTF
    -Z) with +X to the right, scaled to metres, sight origin at zero."""
    (ry, rz), (fy, fz) = spec["sight"]["rear"], spec["sight"]["front"]
    rx = spec["sight"].get("x", 0.0)
    # Most packs face -Y with +X to the left; turn those around.
    facing = 1.0 if spec.get("forward") == "+Y" else -1.0
    turn = Matrix.Identity(4) if facing > 0 else Matrix.Rotation(np.pi, 4, "Z")
    # Pitch about X so the front post's tip is level with the rear sight.
    pitch = Matrix.Rotation(np.arctan2(-facing * (fz - rz), facing * (fy - ry)), 4, "X")
    return turn @ Matrix.Scale(spec["scale"], 4) @ pitch @ Matrix.Translation((0.0 - rx, -ry, -rz))


def sight_radius(spec: dict) -> float:
    """Metres from the rear sight's aiming point to the front post's tip."""
    (ry, rz), (fy, fz) = spec["sight"]["rear"], spec["sight"]["front"]
    return float(np.hypot(fy - ry, fz - rz)) * spec["scale"]


def edit_sights(source: Source, spec: dict):
    """Reshapes the source's sights in view space (metres, the sight line
    along +Y from the rear sight's aiming point, +X right, +Z up), so the
    aimed picture reads at game scale. Each edit in `sight_edits` moves the
    vertices within `depth` of `at` along the line, measured from the rear
    sight, or from the front sight with `front`; `group` limits it to one
    source bone or object.

    aperture   opens a peep from radius `hole[0]` to `hole[1]`, squeezing
               the ring between it and `rim` so the outside keeps its shape.
    widen      moves the faces of a notch or post in a `z` range sideways,
               mapping `half[0]` to `half[1]` from the centre and squeezing
               what lies out to `keep`.
    """
    edits = spec.get("sight_edits", [])
    if not edits:
        return
    to_view_space = to_view(spec)
    back = to_view_space.inverted()
    front = sight_radius(spec)
    mesh = source.obj.data
    for edit in edits:
        kind = edit["kind"]
        at = edit["at"] + (front if edit.get("front") else 0.0)
        group = source.obj.vertex_groups.get(edit["group"]).index if "group" in edit else None
        moved = 0
        for v in mesh.vertices:
            if group is not None and not any(g.group == group and g.weight > 0.5 for g in v.groups):
                continue
            p = to_view_space @ v.co
            if abs(p.y - at) > edit["depth"]:
                continue
            if kind == "aperture":
                (r0, r1), rim = edit["hole"], edit["rim"]
                r = float(np.hypot(p.x, p.z))
                if r > rim or r < 1e-6:
                    continue
                scaled = r1 + (r - r0) * (rim - r1) / (rim - r0) if r >= r0 else r * r1 / r0
                p.x *= scaled / r
                p.z *= scaled / r
            elif kind == "widen":
                (h0, h1), keep = edit["half"], edit["keep"]
                z0, z1 = edit["z"]
                d = abs(p.x)
                if not z0 <= p.z <= z1 or d > keep:
                    continue
                w = d * h1 / h0 if d <= h0 else h1 + (d - h0) * (keep - h1) / (keep - h0)
                p.x = w if p.x >= 0 else -w
            v.co = back @ p
            moved += 1
        log(f"  sights: {kind} moved {moved} vertices")


def source_point(spec: dict, source: Source, where) -> Vector:
    if isinstance(where, str):
        p = source.bones[where].copy()
        p.x = spec["sight"].get("x", 0.0)
        return p
    if len(where) == 3:
        return Vector(where)
    return Vector((spec["sight"].get("x", 0.0), where[0], where[1]))


def godot(v: Vector) -> tuple[float, float, float]:
    """Blender (Z up, +Y forward) to the glTF / Godot axes (Y up, -Z forward)."""
    return (v.x, v.z, -v.y)


def face_parts(obj, spec: dict) -> list[str]:
    """The node each face belongs to, from the bone that moves most of it."""
    names = {g.index: g.name for g in obj.vertex_groups}
    mapping = spec["parts"]
    vertex_part = []
    for v in obj.data.vertices:
        if not v.groups:
            vertex_part.append("Body")
            continue
        bone = names[max(v.groups, key=lambda g: g.weight).group]
        key = None if bone == "<none>" else bone
        vertex_part.append(mapping.get(key, "Body"))
    parts = []
    for poly in obj.data.polygons:
        votes: dict[str, int] = {}
        for i in poly.vertices:
            votes[vertex_part[i]] = votes.get(vertex_part[i], 0) + 1
        parts.append(max(votes, key=votes.get))
    return parts


def material_kind(name: str) -> str:
    if name in ("Optics", "Lens"):
        return name
    if any(m in name for m in CARTRIDGE_MATERIALS):
        return "Cartridge"
    if any(m in name for m in GLOW_MATERIALS):
        return "Glow"
    return "Weapon"


def split(obj, spec: dict) -> dict[str, bpy.types.Object]:
    """Splits the joined mesh into one object per node path."""
    parts = face_parts(obj, spec)
    kinds = [material_kind(s.material.name if s.material else "") for s in obj.material_slots]
    # The tritium dots ride on the part they sit in, as their own node.
    for i, poly in enumerate(obj.data.polygons):
        if kinds[poly.material_index] == "Glow":
            parts[i] = parts[i] + "/SightGlow" if parts[i] != "Body" else "SightGlow"
    paths = sorted(set(parts))
    objects = {}
    for path in paths:
        dup = obj.copy()
        dup.data = obj.data.copy()
        bpy.context.scene.collection.objects.link(dup)
        bm = bmesh.new()
        bm.from_mesh(dup.data)
        bm.faces.ensure_lookup_table()
        doomed = [f for f in bm.faces if parts[f.index] != path]
        bmesh.ops.delete(bm, geom=doomed, context="FACES")
        bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
        bm.to_mesh(dup.data)
        bm.free()
        dup.name = path.split("/")[-1]
        dup.data.name = dup.name
        objects[path] = dup
        log(f"  {path}: {len(dup.data.polygons)} faces")
    bpy.data.objects.remove(obj)
    return objects


def refinish(obj, spec: dict, at: tuple[float, float]):
    """Box-projects a part's UVs onto the atlas patch centred at `at`: each
    face takes the two axes across its normal, in metres at the atlas's
    density, so the part wears that patch's finish."""
    scale = ATLAS_UV_PER_M * spec["scale"]
    mesh = obj.data
    centre = sum((v.co for v in mesh.vertices), Vector()) / max(len(mesh.vertices), 1)
    uv = mesh.uv_layers.active.data
    for poly in mesh.polygons:
        axis = max(range(3), key=lambda i: abs(poly.normal[i]))
        a, b = [i for i in range(3) if i != axis]
        for li in poly.loop_indices:
            co = mesh.vertices[mesh.loops[li].vertex_index].co - centre
            uv[li].uv = (at[0] + co[a] * scale, at[1] + co[b] * scale)


def thin(obj, spec: dict, share: float):
    """Squeezes a part across the gun (source X) toward its side nearest the
    centre line, keeping `share` of its thickness."""
    centre = spec["sight"].get("x", 0.0)
    xs = [v.co.x for v in obj.data.vertices]
    lo, hi = min(xs), max(xs)
    inner = lo if abs(lo - centre) < abs(hi - centre) else hi
    for v in obj.data.vertices:
        v.co.x = inner + (v.co.x - inner) * share
    obj.data.update()


def set_materials(objects: dict, key: str):
    """One material per kind: the weapon's own, cartridges, or glow."""
    shared = {}
    for name in ("Weapon", "Cartridge", "Glow"):
        shared[name] = bpy.data.materials.new(name)
    # Parts the tool models (a drum, a bipod) use the shared optics finish.
    for name in ("Optics", "Lens"):
        shared[name] = bpy.data.materials.get(name) or bpy.data.materials.new(name)
    for obj in objects.values():
        kinds = [material_kind(s.material.name if s.material else "") for s in obj.material_slots]
        used = sorted({kinds[p.material_index] for p in obj.data.polygons})
        remap = {k: i for i, k in enumerate(used)}
        indices = [remap[kinds[p.material_index]] for p in obj.data.polygons]
        obj.data.materials.clear()
        for k in used:
            obj.data.materials.append(shared[k])
        for poly, index in zip(obj.data.polygons, indices):
            poly.material_index = index


def cartridge_uvs(obj, row: int):
    """UVs on the shared cartridge texture: u runs from the case head (0) to
    the bullet tip (1) along each round (+Y once placed), v picks the
    cartridge's row (CARTRIDGES)."""
    mesh = obj.data
    bm = bmesh.new()
    bm.from_mesh(mesh)
    uv = bm.loops.layers.uv.active or bm.loops.layers.uv.new("UV_0")
    # Each loose piece is one round, lying along Y.
    seen = set()
    for start in bm.verts:
        if start in seen:
            continue
        stack, piece = [start], []
        seen.add(start)
        while stack:
            v = stack.pop()
            piece.append(v)
            for e in v.link_edges:
                o = e.other_vert(v)
                if o not in seen:
                    seen.add(o)
                    stack.append(o)
        ys = [v.co.y for v in piece]
        y0, y1 = min(ys), max(ys)
        span = max(y1 - y0, 1e-6)
        piece_set = set(piece)
        for face in {f for v in piece for f in v.link_faces}:
            for loop in face.loops:
                if loop.vert in piece_set:
                    t = (loop.vert.co.y - y0) / span
                    loop[uv].uv = (0.02 + 0.96 * t, 1.0 - (row + 0.5) / 4.0)
    bm.to_mesh(mesh)
    bm.free()


def place(objects: dict, spec: dict, source: Source) -> dict[str, Vector]:
    """Moves everything into view space and sets each node's origin.
    Returns each node's origin (Blender space)."""
    matrix = to_view(spec)
    origins = {}
    pivots = spec.get("pivots", {})
    for path, obj in objects.items():
        obj.data.transform(matrix)
        name = path.split("/")[-1]
        if name in pivots:
            where = pivots[name]
            point = source.bones[where] if isinstance(where, str) else source_point(spec, source, where)
            origin = matrix @ point
        else:
            co = [v.co for v in obj.data.vertices]
            lo = Vector([min(c[i] for c in co) for i in range(3)])
            hi = Vector([max(c[i] for c in co) for i in range(3)])
            origin = (lo + hi) * 0.5
        obj.data.transform(Matrix.Translation(-origin))
        origins[path] = origin
    # Parent children (Magazine/Rounds) and position every node.
    for path, obj in objects.items():
        if "/" in path:
            parent_path = path.rsplit("/", 1)[0]
            obj.parent = objects[parent_path]
            obj.location = origins[path] - origins[parent_path]
        else:
            obj.location = origins[path]
    return origins


def spare_magazine(objects: dict):
    mag = objects.get("Magazine")
    if mag is None:
        return
    spare = mag.copy()
    bpy.context.scene.collection.objects.link(spare)
    spare.name = "MagazineSpare"
    objects["MagazineSpare"] = spare
    for path, obj in list(objects.items()):
        if path.startswith("Magazine/"):
            child = obj.copy()
            bpy.context.scene.collection.objects.link(child)
            child.parent = spare
            child.name = obj.name + "Spare"
            objects["MagazineSpare/" + child.name] = child


def revolve(bm, profile, segments, uv, v0=0.0, cap_start=False, cap_end=False):
    """Adds a surface of revolution about +Y to `bm`: `profile` is (y, radius)
    from back to front. UVs wrap u around and run v along the length."""
    rings = []
    for y, r in profile:
        rings.append([bm.verts.new((r * np.sin(2 * np.pi * i / segments), y, r * np.cos(2 * np.pi * i / segments)))
                      for i in range(segments)])
    length = 0.0
    vs = [v0]
    for (y0, r0), (y1, r1) in zip(profile, profile[1:]):
        length += float(np.hypot(y1 - y0, r1 - r0))
        vs.append(v0 + length * 4.0)
    for k, (a, b) in enumerate(zip(rings, rings[1:])):
        for i in range(segments):
            j = (i + 1) % segments
            face = bm.faces.new((a[i], b[i], b[j], a[j]))
            for loop, (uu, vv) in zip(face.loops, ((i, vs[k]), (i, vs[k + 1]), (i + 1, vs[k + 1]), (i + 1, vs[k]))):
                loop[uv].uv = (uu / segments, vv)
    for cap, ring, flip in ((cap_start, rings[0], True), (cap_end, rings[-1], False)):
        if cap:
            face = bm.faces.new(list(reversed(ring)) if flip else ring)
            for loop in face.loops:
                loop[uv].uv = (0.5 + loop.vert.co.x * 4, 0.5 + loop.vert.co.z * 4)
    return rings

# Cartridge outlines, (distance from the case head, radius) in metres, head
# to tip: rim, extractor groove, body, shoulder, neck, and the bullet's ogive.
CARTRIDGE_PROFILES = {
    "7.62x39": [
        (0.0, 0.0035), (0.0, 0.0056), (0.0015, 0.0056), (0.0017, 0.0047), (0.0030, 0.0047),
        (0.0036, 0.0055), (0.0305, 0.0051), (0.0330, 0.0043), (0.0387, 0.0043), (0.0387, 0.0039),
        (0.0450, 0.0036), (0.0520, 0.0024), (0.0555, 0.0010), (0.0560, 0.0),
    ],
    "5.56": [
        (0.0, 0.0030), (0.0, 0.0048), (0.0011, 0.0048), (0.0013, 0.0040), (0.0024, 0.0040),
        (0.0030, 0.0048), (0.0365, 0.0045), (0.0395, 0.0032), (0.0447, 0.0032), (0.0447, 0.0029),
        (0.0500, 0.0025), (0.0540, 0.0016), (0.0565, 0.0007), (0.0574, 0.0),
    ],
}


def add_rounds(source: Source, spec: dict):
    """Adds the top rounds of a magazine the source models empty, as bone
    "ROUNDS" with the cartridge material: the top round at `back`, the next
    ones each a case width lower and staggered to the other side."""
    info = spec["rounds"]
    profile = CARTRIDGE_PROFILES[info["profile"]]
    facing = 1.0 if spec.get("forward") == "+Y" else -1.0
    scale = spec["scale"]
    by, bz = info["back"]
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new("UV_0")
    width = max(r for _, r in profile) * 2.0 / scale
    for k in range(info.get("count", 2)):
        prof = [(by + facing * d / scale, r / scale) for d, r in profile]
        rings = revolve(bm, prof, 12, uv)
        verts = [v for ring in rings for v in ring]
        # Stacked down, staggered left and right like a double-column magazine.
        x = (0.5 if k % 2 == 0 else -0.5) * width * 0.86
        z = bz - width * 0.5 - k * width * 0.5
        bmesh.ops.translate(bm, vec=(x, 0.0, z), verts=verts)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-7)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bmesh.ops.triangulate(bm, faces=[f for f in bm.faces if len(f.verts) > 4])
    mesh = bpy.data.meshes.new("Rounds")
    bm.to_mesh(mesh)
    bm.free()
    for poly in mesh.polygons:
        poly.use_smooth = True
    obj = bpy.data.objects.new("Rounds", mesh)
    bpy.context.scene.collection.objects.link(obj)
    obj.data.materials.append(bpy.data.materials.get("BULLETS") or bpy.data.materials.new("BULLETS"))
    group = obj.vertex_groups.new(name="ROUNDS")
    group.add([v.index for v in mesh.vertices], 1.0, "REPLACE")
    bpy.ops.object.select_all(action="DESELECT")
    source.obj.select_set(True)
    obj.select_set(True)
    bpy.context.view_layer.objects.active = source.obj
    bpy.ops.object.join()
    source.obj = bpy.context.view_layer.objects.active


def subdivide(source: Source, info: dict):
    """Rounds a low-poly source: one or more Catmull-Clark levels, with the
    edges sharper than `crease` degrees creased so boxy parts keep their
    corners while barrels and tubes come out round."""
    obj = source.obj
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    crease = bm.edges.layers.float.get("crease_edge") or bm.edges.layers.float.new("crease_edge")
    limit = np.radians(info.get("crease", 35.0))
    for edge in bm.edges:
        if len(edge.link_faces) != 2 or edge.calc_face_angle(0.0) > limit or edge.seam:
            edge[crease] = 1.0
    bm.to_mesh(obj.data)
    bm.free()
    mod = obj.modifiers.new("Subdivide", "SUBSURF")
    mod.levels = mod.render_levels = info.get("levels", 1)
    mod.uv_smooth = "PRESERVE_CORNERS"
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.modifier_apply(modifier=mod.name)
    for poly in obj.data.polygons:
        poly.use_smooth = True
    obj.data.set_sharp_from_angle(angle=limit)


def stretch(source: Source, info: dict):
    """Moves everything ahead of `from` (source y; the pack faces -Y) by
    `by`, bones included: the faces that cross it, the bare barrel,
    stretch."""
    cut, by = info["from"], info["by"]
    for v in source.obj.data.vertices:
        if v.co.y < cut:
            v.co.y += by
    for name, head in source.bones.items():
        if head.y < cut:
            head.y += by


def to_source(spec: dict, anchor: Vector):
    """A function from metres in a weapon-local frame (+X right, +Y forward,
    +Z up) to the source's space around `anchor`."""
    facing = 1.0 if spec.get("forward") == "+Y" else -1.0
    scale = spec["scale"]

    def convert(p):
        return Vector((facing * p[0] / scale, facing * p[1] / scale, p[2] / scale)) + anchor

    return convert


def add_part(source: Source, bm, group: str, material: str):
    """Joins the bmesh `bm` (already in source space) into the source as
    bone `group` with `material`."""
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-7)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bmesh.ops.triangulate(bm, faces=[f for f in bm.faces if len(f.verts) > 4])
    mesh = bpy.data.meshes.new(group)
    bm.to_mesh(mesh)
    bm.free()
    for poly in mesh.polygons:
        poly.use_smooth = True
    mesh.set_sharp_from_angle(angle=np.radians(35.0))
    obj = bpy.data.objects.new(group, mesh)
    bpy.context.scene.collection.objects.link(obj)
    obj.data.materials.append(bpy.data.materials.get(material) or bpy.data.materials.new(material))
    obj.vertex_groups.new(name=group).add([v.index for v in mesh.vertices], 1.0, "REPLACE")
    bpy.ops.object.select_all(action="DESELECT")
    source.obj.select_set(True)
    obj.select_set(True)
    bpy.context.view_layer.objects.active = source.obj
    bpy.ops.object.join()
    source.obj = bpy.context.view_layer.objects.active


def box(bm, uv, lo, hi, convert, bevel=0.0):
    """An axis-aligned box from `lo` to `hi` (weapon-local metres)."""
    verts = bmesh.ops.create_cube(bm, size=1.0)["verts"]
    size = [h - l for l, h in zip(lo, hi)]
    centre = [(l + h) / 2 for l, h in zip(lo, hi)]
    for v in verts:
        p = [centre[i] + v.co[i] * size[i] for i in range(3)]
        v.co = convert(p)
    if bevel > 0.0:
        edges = list({e for v in verts for e in v.link_edges})
        bmesh.ops.bevel(bm, geom=edges, offset=bevel, segments=2, affect="EDGES", profile=0.5)
    for f in bm.faces:
        for loop in f.loops:
            loop[uv].uv = (0.5 + loop.vert.co.x * 8, 0.5 + loop.vert.co.y * 8)


def add_drum(source: Source, spec: dict):
    """A 75-round drum in place of the box magazine (bone "DRUM"): a feed
    tower into the magazine well, the drum hanging below it with its axis
    across the gun, a winding key on the left face, ribs pressed into both
    faces, and the top rounds in the tower's lips (bone "ROUNDS")."""
    info = spec["drum"]
    # The tower goes where the box magazine's top was: its rear edge, on
    # the centre line, at the magazine well.
    obj = source.obj
    group = obj.vertex_groups[info["tower"]].index
    mag = [v.co for v in obj.data.vertices if any(g.group == group and g.weight > 0.5 for g in v.groups)]
    top = max(c.z for c in mag)
    lips = [c for c in mag if c.z > top - 0.02]
    facing = 1.0 if spec.get("forward") == "+Y" else -1.0
    rear = min(c.y * facing for c in lips) * facing
    anchor = Vector((0.0, rear, top))
    convert = to_source(spec, anchor)
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new("UV_0")
    # The tower: from the magazine catch down into the drum, slightly
    # forward-raked like the box magazine it replaces.
    box(bm, uv, (-0.0135, 0.0, -0.080), (0.0135, 0.062, 0.0), convert, bevel=0.0025)
    # The drum: 170 mm across, 76 mm thick, axis along X, its rim under
    # the tower.
    radius, half = 0.085, 0.038
    cy, cz = 0.040, -0.070 - radius
    seg = 40
    # Each face: a raised hub, a recessed band, a pressed rib, and the
    # rolled rim; the rim seam runs round the middle of the edge.
    face = [
        (0.0, 0.0), (0.0, 0.018), (0.0025, 0.021), (0.0025, 0.040), (0.0010, 0.042), (0.0010, 0.050),
        (0.0025, 0.052), (0.0025, 0.066), (0.0, 0.069), (0.0, radius - 0.007), (0.004, radius),
    ]
    rim = [(-half + d, r) for d, r in face]
    rim += [(-0.0012, radius), (-0.0006, radius + 0.0012), (0.0006, radius + 0.0012), (0.0012, radius)]
    rim += [(half - d, r) for d, r in reversed(face)]
    profile = rim
    rings = revolve(bm, profile, seg, uv)
    verts = [v for ring in rings for v in ring]
    turn = Matrix.Rotation(-np.pi / 2, 4, "Z")
    for v in verts:
        local = turn @ v.co
        v.co = convert((local.x, cy + local.y, cz + local.z))
    # The winding key: a hub and a folding lever on the left face.
    key = revolve(bm, [(0.0, 0.0), (0.0, 0.016), (0.010, 0.016), (0.012, 0.010), (0.012, 0.0)], 20, uv)
    verts = [v for ring in key for v in ring]
    for v in verts:
        local = Matrix.Rotation(np.pi / 2, 4, "Z") @ v.co
        v.co = convert((-half + local.x, cy + local.y, cz + local.z))
    box(bm, uv, (-half - 0.016, -0.004 + cy, cz - 0.004), (-half - 0.010, 0.034 + cy, cz + 0.004), convert, 0.0015)
    add_part(source, bm, "DRUM", "Optics")
    # Rounds in the tower's lips.
    profile = CARTRIDGE_PROFILES[info["rounds"]]
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new("UV_0")
    width = max(r for _, r in profile) * 2.0
    for k in range(2):
        rings = revolve(bm, profile, 12, uv)
        x = (0.5 if k % 2 == 0 else -0.5) * width * 0.86
        for ring in rings:
            for v in ring:
                p = (v.co.x + x, v.co.y + 0.004, v.co.z - width * 0.5 - k * width * 0.5)
                v.co = convert(p)
    add_part(source, bm, "ROUNDS", "BULLETS")


def add_bead(source: Source, spec: dict):
    """A brass bead front sight (bone "BEAD", on the cartridge texture's
    brass): a sphere sitting on the barrel at `at` (source y, z of the
    barrel's top)."""
    y, z = spec["bead"]["at"]
    r = spec["bead"]["radius"] / spec["scale"]
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new("UV_0")
    bmesh.ops.create_uvsphere(bm, u_segments=12, v_segments=8, radius=r)
    bmesh.ops.translate(bm, vec=(0.0, y, z), verts=bm.verts)
    for face in bm.faces:
        for loop in face.loops:
            loop[uv].uv = (0.3, 1.0 - 0.5 / 4.0)
    add_part(source, bm, "BEAD", "BULLETS")


def add_bipod(source: Source, spec: dict):
    """A folded bipod (bone "BIPOD"): a clamp around the barrel at `at`
    (source y) and two legs folded back under the barrel."""
    barrel = source.bones["FX_FIRE"]
    anchor = Vector((0.0, spec["bipod"]["at"], barrel.z))
    convert = to_source(spec, anchor)
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new("UV_0")
    # The clamp: a short collar around the barrel.
    collar = revolve(bm, [(-0.012, 0.0095), (-0.012, 0.0125), (0.012, 0.0125), (0.012, 0.0095)], 24, uv)
    for ring in collar:
        for v in ring:
            v.co = convert((v.co.x, v.co.y, v.co.z))
    # The yoke under the collar, and two legs folded back along the barrel,
    # splayed a little.
    box(bm, uv, (-0.018, -0.010, -0.024), (0.018, 0.010, -0.011), convert, 0.002)
    for side in (-1.0, 1.0):
        x = side * 0.013
        box(bm, uv, (x - 0.004, -0.250, -0.028), (x + 0.004, 0.0, -0.020), convert, 0.0015)
        # The feet: small spurs at the rear ends.
        box(bm, uv, (x - 0.007, -0.262, -0.034), (x + 0.007, -0.246, -0.018), convert, 0.002)
    add_part(source, bm, "BIPOD", "Optics")


def riflescope(spec: dict) -> bpy.types.Object:
    """A tactical riflescope in view space (the sight line is its axis, +Y
    forward): eyepiece with a rubber eye guard and power ring, 30 mm tube,
    a saddle with tall target turrets for elevation and windage, a 40 mm
    objective bell with its sunshade, two rings and their bases down to the
    rail, and coated lenses at both ends."""
    scale = spec["scale"]
    rail = (spec["scope"]["rail"] - spec["sight"]["rear"][1]) * scale
    ring_ys = [-(y - spec["sight"]["rear"][0]) * scale for y in spec["scope"]["rings"]]
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new("UV_0")
    seg = 48
    body = [
        (-0.012, 0.0172), (-0.011, 0.0196), (-0.004, 0.0205), (0.016, 0.0205), (0.018, 0.0192),
        (0.040, 0.0192), (0.041, 0.0198), (0.062, 0.0198), (0.063, 0.0185), (0.078, 0.0152),
        (0.125, 0.0152), (0.127, 0.0172), (0.163, 0.0172), (0.165, 0.0152), (0.226, 0.0152),
        (0.256, 0.0236), (0.300, 0.0236), (0.302, 0.0244), (0.322, 0.0244), (0.323, 0.0226),
    ]
    revolve(bm, body, seg, uv)
    # The bore of the eyepiece and the sunshade, so the lenses sit recessed.
    revolve(bm, [(-0.012, 0.0172), (-0.006, 0.0168)], seg, uv)
    revolve(bm, [(0.323, 0.0226), (0.314, 0.0222)], seg, uv)
    # Power ring grip and turret caps: ribs around the ring.
    for k in range(18):
        a = 2 * np.pi * k / 18
        c, sn = np.cos(a), np.sin(a)
        verts = []
        for (dy, dr) in ((0.042, 0.0198), (0.061, 0.0198), (0.061, 0.0206), (0.042, 0.0206)):
            for da in (-0.035, 0.035):
                verts.append((dr * np.sin(a + da), dy, dr * np.cos(a + da)))
        bmesh.ops.convex_hull(bm, input=[bm.verts.new(v) for v in verts])
    # Target turrets, elevation on top and windage on the right: a wide
    # knob on a collar, with grip grooves round it and a stepped cap.
    turret_y = 0.145
    for axis in ("z", "x"):
        prof = [(0.0165, 0.0120), (0.026, 0.0120), (0.027, 0.0168), (0.031, 0.0168), (0.0315, 0.0160),
                (0.0335, 0.0160), (0.034, 0.0168), (0.0375, 0.0168), (0.038, 0.0160), (0.040, 0.0160),
                (0.0405, 0.0168), (0.047, 0.0168), (0.048, 0.0150), (0.049, 0.0140)]
        rings = revolve(bm, prof, 32, uv, cap_end=True)
        verts = [v for ring in rings for v in ring]
        # Built along +Y about the origin; stand it up on the saddle.
        rot = Matrix.Rotation(-np.pi / 2, 4, "X") if axis == "z" else Matrix.Rotation(-np.pi / 2, 4, "Z")
        bmesh.ops.transform(bm, matrix=Matrix.Translation((0, turret_y, 0)) @ rot, verts=verts)
    # Rings and their bases.
    for y in ring_ys:
        rings = revolve(bm, [(y - 0.008, 0.0152), (y - 0.008, 0.0188), (y + 0.008, 0.0188), (y + 0.008, 0.0152)], seg, uv)
        base = bmesh.ops.create_cube(bm, size=1.0)["verts"]
        bmesh.ops.scale(bm, vec=(0.022, 0.014, abs(rail) - 0.012), verts=base)
        bmesh.ops.translate(bm, vec=(0.0, y, rail + (abs(rail) - 0.012) / 2), verts=base)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-6)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bmesh.ops.triangulate(bm, faces=[f for f in bm.faces if len(f.verts) > 4])
    lens_bm = bmesh.new()
    lens_uv = lens_bm.loops.layers.uv.new("UV_0")
    for y, r in ((-0.006, 0.0168), (0.314, 0.0222)):
        verts = [lens_bm.verts.new((r * np.sin(2 * np.pi * i / seg), y, r * np.cos(2 * np.pi * i / seg))) for i in range(seg)]
        face = lens_bm.faces.new(verts if y > 0 else list(reversed(verts)))
        for loop in face.loops:
            loop[lens_uv].uv = (0.5, 0.5)
    bmesh.ops.triangulate(lens_bm, faces=lens_bm.faces)
    mesh = bpy.data.meshes.new("Scope")
    bm.to_mesh(mesh)
    bm.free()
    lens = bpy.data.meshes.new("ScopeLens")
    lens_bm.to_mesh(lens)
    lens_bm.free()
    obj = bpy.data.objects.new("Scope", mesh)
    bpy.context.scene.collection.objects.link(obj)
    lens_obj = bpy.data.objects.new("ScopeLens", lens)
    bpy.context.scene.collection.objects.link(lens_obj)
    optics = bpy.data.materials.get("Optics") or bpy.data.materials.new("Optics")
    glass = bpy.data.materials.get("Lens") or bpy.data.materials.new("Lens")
    obj.data.materials.append(optics)
    lens_obj.data.materials.append(glass)
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    lens_obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.join()
    for poly in obj.data.polygons:
        poly.use_smooth = True
    obj.data.set_sharp_from_angle(angle=np.radians(40.0))
    return obj


# Parts left out of `World`: the spare magazine stays hidden, and the rail
# shows only under an optic. Everything else, the magazine included, is
# merged: one draw call for a gun that is not being reloaded.
WORLD_SEPARATE = ("MagazineSpare", "OpticRail")
# Seen from a few metres away, `World` keeps this share of the triangles
# (the normal map keeps the detail), and Godot's LODs take it further.
WORLD_DECIMATE = 0.2


def world_mesh(objects: dict, name: str = "World", leave_out: tuple = ()):
    """`World`: the weapon merged into one node, for other players' weapons
    and racks (one draw call per material instead of one per part). Hidden
    in first person. With `leave_out`, those parts are left out too."""
    copies = []
    for path, obj in objects.items():
        # Rounds and tritium dots are too small to see on another player.
        if path.split("/")[0] in WORLD_SEPARATE + leave_out or path.endswith(("Rounds", "SightGlow")):
            continue
        if path in ("World", "WorldBody", "WorldMagazine"):
            continue
        dup = obj.copy()
        dup.data = obj.data.copy()
        bpy.context.scene.collection.objects.link(dup)
        dup.parent = None
        dup.matrix_world = obj.matrix_world.copy()
        copies.append(dup)
    bpy.context.view_layer.update()
    bpy.ops.object.select_all(action="DESELECT")
    for dup in copies:
        dup.select_set(True)
    bpy.context.view_layer.objects.active = copies[0]
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    bpy.ops.object.join()
    world = bpy.context.view_layer.objects.active
    world.name = name
    world.data.name = name
    # One surface per material: lenses take the optics' finish at a distance.
    names = [slot.material.name if slot.material else "" for slot in world.material_slots]
    if "Lens" in names and "Optics" in names:
        lens, optics = names.index("Lens"), names.index("Optics")
        for poly in world.data.polygons:
            if poly.material_index == lens:
                poly.material_index = optics
        world.active_material_index = lens
        bpy.ops.object.material_slot_remove()
    decimate = world.modifiers.new("Decimate", "DECIMATE")
    decimate.ratio = WORLD_DECIMATE
    decimate.use_collapse_triangulate = True
    bpy.ops.object.modifier_apply(modifier=decimate.name)
    objects[name] = world


def world_magazine(objects: dict):
    """For other players' reloads (`WeaponView::pose_world_magazine`):
    `WorldBody`, `World` without its magazine, and `WorldMagazine`, the
    magazine alone with the same pivot as `Magazine`, both decimated as
    `World` is. They replace `World` only while a reload moves the
    magazine, so a gun costs one draw call the rest of the time."""
    if "Magazine" not in objects:
        return
    world_mesh(objects, "WorldBody", ("Magazine",))
    magazine = objects["Magazine"]
    dup = magazine.copy()
    dup.data = magazine.data.copy()
    bpy.context.scene.collection.objects.link(dup)
    dup.parent = None
    dup.matrix_world = magazine.matrix_world.copy()
    dup.name = "WorldMagazine"
    dup.data.name = "WorldMagazine"
    bpy.context.view_layer.update()
    bpy.ops.object.select_all(action="DESELECT")
    dup.select_set(True)
    bpy.context.view_layer.objects.active = dup
    decimate = dup.modifiers.new("Decimate", "DECIMATE")
    decimate.ratio = WORLD_DECIMATE
    decimate.use_collapse_triangulate = True
    bpy.ops.object.modifier_apply(modifier=decimate.name)
    objects["WorldMagazine"] = dup


def load_pixels(path: Path, color: bool) -> np.ndarray:
    image = bpy.data.images.load(str(path))
    image.colorspace_settings.name = "sRGB" if color else "Non-Color"
    w, h = image.size
    data = np.empty(w * h * 4, dtype=np.float32)
    image.pixels.foreach_get(data)
    bpy.data.images.remove(image)
    return data.reshape(h, w, 4)


def resize(pixels: np.ndarray, size: int) -> np.ndarray:
    h = pixels.shape[0]
    if h == size:
        return pixels
    factor = h // size
    if factor * size != h:
        raise SystemExit(f"texture {h} px does not divide to {size}")
    return pixels.reshape(size, factor, size, factor, -1).mean(axis=(1, 3))


def save_png(path: Path, rgb: np.ndarray, srgb: bool):
    size = rgb.shape[0]
    rgba = np.concatenate([np.clip(rgb, 0.0, 1.0), np.ones((size, size, 1), np.float32)], axis=2)
    image = bpy.data.images.new(path.stem + "_out", size, size, alpha=False)
    image.colorspace_settings.name = "sRGB" if srgb else "Non-Color"
    image.pixels.foreach_set(rgba.astype(np.float32).ravel())
    image.filepath_raw = str(path)
    image.file_format = "PNG"
    image.save()
    bpy.data.images.remove(image)

# Lowest roughness on metal, and the brightest bare steel (sRGB): see
# build_textures.
METAL_ROUGHNESS_MIN = 0.32
BARE_STEEL_MAX = 0.6
# Pixels each texture island's colour is grown into the empty space round it,
# so mipmaps and filtering never pull in the gaps' colour (white sparkle).
ISLAND_PADDING = 16


def uv_mask(objects: dict) -> np.ndarray:
    """Which texels of the weapon's own texture the UVs cover (rows from
    the bottom, as Blender stores pixels)."""
    size = TEXTURE_SIZE
    mask = np.zeros((size, size), bool)
    for obj in objects.values():
        names = [slot.material.name if slot.material else "" for slot in obj.material_slots]
        uv = obj.data.uv_layers.active
        if uv is None:
            continue
        for poly in obj.data.polygons:
            if material_kind(names[poly.material_index] if names else "") != "Weapon":
                continue
            pts = np.array([uv.data[i].uv[:] for i in poly.loop_indices], np.float64) * size
            for k in range(1, len(pts) - 1):
                tri = pts[[0, k, k + 1]]
                lo = np.floor(tri.min(axis=0)).astype(int).clip(0, size - 1)
                hi = np.ceil(tri.max(axis=0)).astype(int).clip(0, size - 1)
                xs, ys = np.meshgrid(np.arange(lo[0], hi[0] + 1) + 0.5, np.arange(lo[1], hi[1] + 1) + 0.5)
                (ax, ay), (bx, by), (cx, cy) = tri
                d = (by - cy) * (ax - cx) + (cx - bx) * (ay - cy)
                if abs(d) < 1e-12:
                    continue
                w0 = ((by - cy) * (xs - cx) + (cx - bx) * (ys - cy)) / d
                w1 = ((cy - ay) * (xs - cx) + (ax - cx) * (ys - cy)) / d
                inside = (w0 >= -0.02) & (w1 >= -0.02) & (w0 + w1 <= 1.02)
                mask[lo[1]:hi[1] + 1, lo[0]:hi[0] + 1] |= inside
    return mask


def pad_islands(pixels: np.ndarray, mask: np.ndarray, steps: int) -> np.ndarray:
    """Grows the colour of covered texels outwards `steps` texels into the
    uncovered ones, averaging neighbours, like a bake's margin."""
    out = pixels.copy()
    filled = mask.copy()
    for _ in range(steps):
        total = np.zeros_like(out)
        count = np.zeros(mask.shape, np.float32)
        for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            src = np.roll(np.roll(out, dy, 0), dx, 1)
            ok = np.roll(np.roll(filled, dy, 0), dx, 1)
            total += src * ok[..., None]
            count += ok
        grow = (~filled) & (count > 0)
        out[grow] = total[grow] / count[grow][:, None]
        filled |= grow
    return out


def derived_maps(color: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """A normal and an ORM map for a source that ships only its colour map
    (the Classic Weapons Pack's Tec-9). Its colour carries the wear: bright,
    grey texels are steel worn bare at the edges, the rest dark parkerised
    steel and polymer. Height is the colour's luminance with its broad
    shading taken out (a 33 px box blur), so stamped and cast detail stands
    up from the surface; the normal is its slope. Worn steel is metallic
    and smoother, the dark finish matte."""
    lum = color @ np.array([0.2126, 0.7152, 0.0722], np.float32)
    def box(img, r):
        c = np.cumsum(np.cumsum(np.pad(img, r + 1, mode="edge"), 0), 1)
        n = 2 * r + 1
        return (c[n:, n:] - c[:-n, n:] - c[n:, :-n] + c[:-n, :-n])[: img.shape[0], : img.shape[1]] / (n * n)
    height = lum - box(lum, 16)
    height = box(height, 1)
    strength = 6.0
    dx = (np.roll(height, -1, 1) - np.roll(height, 1, 1)) * strength
    dy = (np.roll(height, -1, 0) - np.roll(height, 1, 0)) * strength
    normal = np.stack([-dx, dy, np.ones_like(dx)], axis=2)
    normal /= np.linalg.norm(normal, axis=2, keepdims=True)
    normal = normal * 0.5 + 0.5
    grey = color.max(axis=2) - color.min(axis=2) < 0.08
    worn = np.clip((lum - 0.22) / 0.2, 0.0, 1.0) * grey
    metallic = 0.15 + 0.85 * worn
    roughness = 0.7 - 0.3 * worn
    occlusion = np.clip(0.75 + lum - box(lum, 16), 0.6, 1.0)
    return normal.astype(np.float32), np.stack([occlusion, roughness, metallic], axis=2).astype(np.float32)


def refinish_wood(
    color: np.ndarray, normal: np.ndarray, orm: np.ndarray, finish: dict
) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Recolours the source's wood texels (warm: red over blue, as
    `weapon_finish.is_wood`) as another finish, keeping the source's
    shading and wear as a little of its brightness, so the furniture keeps
    its moulding and scuffs: an AK's walnut as black polymer. The grain
    goes from the normal map too (moulded polymer has none), leaving a
    fine stipple from the source's noise."""
    r, g, b = color[..., 0], color[..., 1], color[..., 2]
    wood = ((r - b) > 0.06) & (r >= g)
    # Soften the mask's edge by a texel or two so seams do not ring.
    soft = wood.astype(np.float32)
    for axis in (0, 1):
        soft = (soft + np.roll(soft, 1, axis) + np.roll(soft, -1, axis)) / 3.0
    lum = color @ np.array([0.2126, 0.7152, 0.0722], np.float32)
    mean = float(lum[wood].mean()) if wood.any() else 0.3
    target = np.array(finish["color"], np.float32)
    shade = np.clip(1.0 + (lum - mean) * finish.get("keep", 0.8), 0.6, 1.5)
    new = target[None, None, :] * shade[..., None]
    color = color * (1.0 - soft[..., None]) + new * soft[..., None]
    orm = orm.copy()
    orm[..., 1] = orm[..., 1] * (1.0 - soft) + finish["roughness"] * soft
    orm[..., 2] = orm[..., 2] * (1.0 - soft)
    flat = np.array([0.5, 0.5, 1.0], np.float32)
    calm = soft[..., None] * finish.get("flatten", 0.8)
    normal = normal * (1.0 - calm) + flat * calm
    normal = (normal - 0.5) / np.maximum(np.linalg.norm(normal - 0.5, axis=2, keepdims=True), 1e-6) * 0.5 + 0.5
    log(f"  refinished {wood.mean():.1%} of the texels from wood")
    return color.astype(np.float32), normal.astype(np.float32), orm


def build_textures(key: str, spec: dict, out: Path, mask=None):
    textures = spec["textures"]
    if isinstance(textures, list) and len(textures) == 1:
        color = resize(load_pixels(textures[0], True), TEXTURE_SIZE)[..., :3]
        normal, orm = derived_maps(color)
        metal = orm[..., 2] > 0.5
        orm[..., 1] = np.where(metal, np.maximum(orm[..., 1], METAL_ROUGHNESS_MIN), orm[..., 1])
        color = np.where(metal[..., None], np.minimum(color, BARE_STEEL_MAX), color)
        if mask is not None:
            color, normal, orm = (pad_islands(m, mask, ISLAND_PADDING) for m in (color, normal, orm))
        save_png(out / f"{key}_albedo.png", color, srgb=True)
        save_png(out / f"{key}_normal.png", normal, srgb=False)
        save_png(out / f"{key}_orm.png", orm, srgb=False)
        return
    if isinstance(textures, dict):
        # Separate maps: colour, normal, metallic, roughness, and occlusion
        # if the pack has one.
        color_path, normal_path = textures["color"], textures["normal"]
    else:
        color_path, normal_path, rmao_path = textures
    # Blender reads sRGB PNG pixels as stored, so colour copies straight over.
    color = resize(load_pixels(color_path, True), TEXTURE_SIZE)[..., :3]
    normal = resize(load_pixels(normal_path, False), TEXTURE_SIZE)[..., :3].copy()
    if spec.get("normal_dx"):
        normal[..., 1] = 1.0 - normal[..., 1]
    if isinstance(textures, dict):
        def channel(key, default):
            if key not in textures:
                return np.full((TEXTURE_SIZE, TEXTURE_SIZE), default, np.float32)
            return resize(load_pixels(textures[key], False), TEXTURE_SIZE)[..., 0]

        orm = np.stack([channel("ao", 1.0), channel("roughness", 0.5), channel("metallic", 0.0)], axis=2)
    else:
        rmao = resize(load_pixels(rmao_path, False), TEXTURE_SIZE)
        orm = np.stack([rmao[..., 2], rmao[..., 0], rmao[..., 1]], axis=2)
    if "wood_to" in spec:
        color, normal, orm = refinish_wood(color, normal, orm, spec["wood_to"])
    # Weapon steel is blued, parkerised, or worn, never polished: below
    # this roughness bare edges read as chrome in first person.
    metal = orm[..., 2] > 0.5
    orm[..., 1] = np.where(metal, np.maximum(orm[..., 1], METAL_ROUGHNESS_MIN), orm[..., 1])
    color = np.where(metal[..., None], np.minimum(color, BARE_STEEL_MAX), color)
    if mask is not None:
        color, normal, orm = (pad_islands(m, mask, ISLAND_PADDING) for m in (color, normal, orm))
    log("  roughness mean", float(orm[..., 1].mean()), "metal mean", float(orm[..., 2].mean()),
        "ao mean", float(orm[..., 0].mean()))
    save_png(out / f"{key}_albedo.png", color, srgb=True)
    save_png(out / f"{key}_normal.png", normal, srgb=False)
    save_png(out / f"{key}_orm.png", orm, srgb=False)


def save_maps(key: str, out: Path, color: np.ndarray, normal: np.ndarray, orm: np.ndarray):
    """Writes painted maps (weapon_finish), with the same limits on bare
    metal as build_textures."""
    metal = orm[..., 2] > 0.5
    orm[..., 1] = np.where(metal, np.maximum(orm[..., 1], METAL_ROUGHNESS_MIN), orm[..., 1])
    color = np.where(metal[..., None], np.minimum(color, BARE_STEEL_MAX), color)
    log("  roughness mean", float(orm[..., 1].mean()), "metal mean", float(orm[..., 2].mean()),
        "ao mean", float(orm[..., 0].mean()))
    save_png(out / f"{key}_albedo.png", color, srgb=True)
    save_png(out / f"{key}_normal.png", normal, srgb=False)
    save_png(out / f"{key}_orm.png", orm, srgb=False)


def write_material(key: str, out: Path, folder: str):
    base = f"res://assets/weapons/{folder}/{key}"
    (out / f"{key}.tres").write_text(
        f"""[gd_resource type="StandardMaterial3D" format=3]

[ext_resource type="Texture2D" path="{base}_albedo.png" id="1_albedo"]
[ext_resource type="Texture2D" path="{base}_normal.png" id="2_normal"]
[ext_resource type="Texture2D" path="{base}_orm.png" id="3_orm"]

[resource]
albedo_texture = ExtResource("1_albedo")
metallic = 1.0
metallic_texture = ExtResource("3_orm")
metallic_texture_channel = 2
roughness_texture = ExtResource("3_orm")
roughness_texture_channel = 1
normal_enabled = true
normal_texture = ExtResource("2_normal")
ao_enabled = true
ao_texture = ExtResource("3_orm")
ao_light_affect = 0.3
"""
    )


IMPORT_SUBRESOURCES = """_subresources={{
"materials": {{
"Cartridge": {{
"use_external/enabled": true,
"use_external/path": "{shared}/cartridge.tres"
}},
"Glow": {{
"use_external/enabled": true,
"use_external/path": "{shared}/glow_tritium.tres"
}},
"Lens": {{
"use_external/enabled": true,
"use_external/path": "{shared}/lens.tres"
}},
"Optics": {{
"use_external/enabled": true,
"use_external/path": "{shared}/optics.tres"
}},
"Weapon": {{
"use_external/enabled": true,
"use_external/path": "{material}"
}}
}}
}}"""


def write_import(key: str, out: Path, folder: str):
    """Points the glb's materials at the .tres files. Godot fills in the rest
    of the file on import."""
    path = out / f"{key}.glb.import"
    material = f"res://assets/weapons/{folder}/{key}.tres"
    sub = IMPORT_SUBRESOURCES.format(shared=SHARED, material=material)
    if path.exists():
        text = path.read_text()
        start = text.find("_subresources=")
        end = text.find("\ngltf/", start)
        text = text[:start] + sub + text[end:]
        text = text.replace("meshes/light_baking=1", "meshes/light_baking=0")
    else:
        text = f"""[remap]

importer="scene"
importer_version=1
type="PackedScene"

[params]

nodes/root_type=""
nodes/root_name=""
meshes/ensure_tangents=true
meshes/generate_lods=true
meshes/create_shadow_meshes=true
meshes/light_baking=0
animation/import=false
{sub}
gltf/naming_version=2
gltf/embedded_image_handling=1
"""
    path.write_text(text)


def export(objects: dict, path: Path):
    bpy.ops.object.select_all(action="DESELECT")
    for obj in objects.values():
        obj.select_set(True)
        for layer in list(obj.data.uv_layers)[1:]:
            obj.data.uv_layers.remove(layer)
    # The source's custom normals carry its smoothing and go out as they are.
    bpy.context.view_layer.objects.active = next(iter(objects.values()))
    bpy.ops.export_scene.gltf(
        filepath=str(path),
        export_format="GLB",
        use_selection=True,
        export_tangents=True,
        export_materials="EXPORT",
        export_yup=True,
        export_apply=True,
        export_normals=True,
        export_animations=False,
        export_skins=False,
        export_morph=False,
    )


def patch_scene(spec: dict, markers: dict[str, tuple]):
    """Writes the marker positions into the view scene."""
    path = SCENES_DIR / f"{spec['scene']}_view.tscn"
    text = path.read_text()
    for name, (x, y, z) in markers.items():
        pattern = re.compile(
            r'(\[node name="' + name + r'" type="Marker3D" parent="\."[^\]]*\]\n)(position = Vector3\([^)]*\)\n)?'
        )
        line = f"position = Vector3({x:.4f}, {y:.4f}, {z:.4f})\n"
        text, count = pattern.subn(lambda m: m.group(1) + line, text)
        if count == 0:
            text = text.rstrip("\n") + f'\n\n[node name="{name}" type="Marker3D" parent="."]\n' + line
            log(f"  {path.name}: added marker {name}")
    path.write_text(text)


def handle_point(obj, origin: Vector, side: str) -> Vector:
    """The middle of the part's last few millimetres on `side`: "+x" (the
    right), "-x" (the left), or "rear" (toward the shooter, -Y in Blender
    view space)."""
    pts = [v.co + origin for v in obj.data.vertices]
    axis, sign = {"+x": (0, 1.0), "-x": (0, -1.0), "rear": (1, -1.0)}[side]
    far = max(sign * p[axis] for p in pts)
    tip = [p for p in pts if sign * p[axis] > far - 0.004]
    return sum(tip, Vector()) / len(tip)


# Weapons modelled by a tool instead of imported from a pack.
MODELLED = {"m870": weapon_m870}


def militarize(source: Source, spec: dict):
    """Turns a sporting rifle into its military pattern (`military`), in
    source space: thickens the free barrel about its axis, widens the
    forend, and adds a raised cheek piece on two posts (bones "CHEEK" and
    "CHEEKPOST", in Body). Wood is told apart by the source's colour map."""
    info = spec["military"]
    obj = source.obj
    mesh = obj.data
    color = load_pixels(spec["textures"][0], True)[..., :3]
    h, w = color.shape[:2]
    uv = mesh.uv_layers.active
    wood = set()
    for poly in mesh.polygons:
        mid = np.mean([uv.data[i].uv[:] for i in poly.loop_indices], axis=0)
        if weapon_finish.is_wood(color[min(int(mid[1] % 1.0 * h), h - 1), min(int(mid[0] % 1.0 * w), w - 1)]):
            wood.update(poly.vertices)
    barrel = info["barrel"]
    ax, az = barrel["axis"]
    thick = 0
    for v in mesh.vertices:
        dx, dz = v.co.x - ax, v.co.z - az
        if v.co.y < barrel["ahead_of"] and v.index not in wood and np.hypot(dx, dz) < barrel["within"]:
            v.co.x = ax + dx * barrel["scale"]
            v.co.z = az + dz * barrel["scale"]
            thick += 1
    y0, y1 = info["forend"]["y"]
    wide = 0
    for i in wood:
        v = mesh.vertices[i]
        if y0 < v.co.y < y1:
            v.co.x *= info["forend"]["widen"]
            wide += 1
    log(f"  military: barrel {thick} vertices thicker, forend {wide} wider")
    # The cheek piece: a riser following the comb's slope, on two posts.
    c = info["cheek"]
    (ya, yb), (ba, bb), (ta, tb) = c["y"], c["bottom"], c["top"]
    riser = weapon_shapes.side_profile("Cheek", [(ya, ba), (yb, bb), (yb, tb), (ya, ta)], -c["half"], c["half"])
    weapon_shapes.bevel(riser, 0.004, 3)
    weapon_shapes.group(riser, "CHEEK")
    parts = [riser]
    for y in c["posts"]:
        top = ba + (bb - ba) * (y - ya) / (yb - ya)
        post = weapon_shapes.lathe(f"Post{y}", [(-0.012, 0.0), (-0.012, 0.0032), (top + 0.002, 0.0032),
                                                (top + 0.002, 0.0)], 16, "Z", (0.0, y, 0.0))
        weapon_shapes.group(post, "CHEEKPOST")
        parts.append(post)
    material = obj.material_slots[0].material
    for part in parts:
        part.data.materials.append(material)
        part.data.uv_layers.new(name=mesh.uv_layers.active.name)
    bpy.ops.object.select_all(action="DESELECT")
    for part in parts:
        part.select_set(True)
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.join()
    source.obj = bpy.context.view_layer.objects.active


def build_modelled(key: str):
    """A weapon its own tool models (MODELLED): its nodes and markers come
    from the tool, painted (tools/weapon_finish.py) and exported like the
    others."""
    spec = WEAPONS[key]
    tool = MODELLED[spec["model"]]
    log(key)
    reset()
    out = WEAPONS_DIR / spec["dir"]
    out.mkdir(parents=True, exist_ok=True)
    objects, points = tool.build()
    finish = dict(tool.FINISH, font=REPO / "godot/assets/fonts/BarlowCondensed-Bold.ttf")
    for obj in objects.values():
        weapon_finish.assign_zones(obj, finish, None)
    bpy.context.view_layer.update()
    painted = weapon_finish.paint(objects, {"finish": finish}, tool.view_matrix(), TEXTURE_SIZE)
    for obj in objects.values():
        obj.data.attributes.remove(obj.data.attributes[weapon_finish.ZONE_ATTRIBUTE])
    world_mesh(objects)
    markers = {name: godot(point) for name, point in points.items()}

    def count(paths):
        return sum(sum(len(p.vertices) - 2 for p in objects[k].data.polygons) for k in paths)

    tris, world_tris = count([k for k in objects if k != "World"]), count(["World"])
    log(f"  {tris} triangles in first person, {world_tris} for other players, {len(objects)} nodes")
    stem = spec["dir"]
    export(objects, out / f"{stem}.glb")
    save_maps(stem, out, *painted)
    write_material(stem, out, spec["dir"])
    write_import(stem, out, spec["dir"])
    patch_scene(spec, markers)
    (out / f"{stem}.json").write_text(
        json.dumps(
            {
                "triangles": tris,
                "world_triangles": world_tris,
                "nodes": sorted(objects),
                "markers": {k: [round(c, 4) for c in v] for k, v in markers.items()},
            },
            indent=2,
        )
        + "\n"
    )


def build(key: str):
    spec = WEAPONS[key]
    if "model" in spec:
        build_modelled(key)
        return
    log(key)
    reset()
    out = WEAPONS_DIR / spec["dir"]
    out.mkdir(parents=True, exist_ok=True)
    source = Source(spec)
    if "subdivide" in spec:
        subdivide(source, spec["subdivide"])
    if "stretch" in spec:
        stretch(source, spec["stretch"])
    if "rounds" in spec:
        add_rounds(source, spec)
    if "drum" in spec:
        add_drum(source, spec)
    if "bipod" in spec:
        add_bipod(source, spec)
    if "bead" in spec:
        add_bead(source, spec)
    edit_sights(source, spec)
    if "military" in spec:
        militarize(source, spec)
    if "finish" in spec:
        textures = spec["textures"]
        if isinstance(textures, dict):
            metallic = load_pixels(textures["metallic"], False)[..., 0] if "metallic" in textures else None
            color = None
        else:
            metallic = load_pixels(textures[2], False)[..., 1]
            color = load_pixels(textures[0], True)[..., :3]
        weapon_finish.assign_zones(source.obj, spec["finish"], metallic, color)
    objects = split(source.obj, spec)
    for name, at in spec.get("refinish", {}).items():
        refinish(objects[name], spec, at)
    for name, share in spec.get("thin", {}).items():
        thin(objects[name], spec, share)
    for name in spec.get("drop", []):
        bpy.data.objects.remove(objects.pop(name))
    set_materials(objects, key)
    origins = place(objects, spec, source)
    for path, obj in objects.items():
        if path.endswith("Rounds"):
            cartridge_uvs(obj, row=spec.get("cartridge", 0))
    painted = None
    if "finish" in spec:
        bpy.context.view_layer.update()
        painted = weapon_finish.paint(objects, spec, to_view(spec), TEXTURE_SIZE)
        for obj in objects.values():
            attr = obj.data.attributes.get(weapon_finish.ZONE_ATTRIBUTE)
            if attr is not None:
                obj.data.attributes.remove(attr)
    if "scope" in spec:
        objects["Scope"] = riflescope(spec)
    bpy.context.view_layer.update()
    world_mesh(objects)
    world_magazine(objects)
    if spec.get("spare"):
        spare_magazine(objects)
    matrix = to_view(spec)
    markers = {}
    for name, where in spec["markers"].items():
        markers[name] = godot(matrix @ source_point(spec, source, where))
    # Where the hand works the charging handle: the end of the part on the
    # side it sticks out of (the reload clips reach for this marker).
    if "handle" in spec and "ChargingHandle" in objects:
        markers["ChargingHandle"] = godot(handle_point(objects["ChargingHandle"], origins["ChargingHandle"], spec["handle"]))
    def count(paths):
        return sum(sum(len(p.vertices) - 2 for p in objects[k].data.polygons) for k in paths)

    first = [k for k in objects if not k.startswith(("World", "MagazineSpare"))]
    world = ["World"] + [k for k in objects if k.split("/")[0] == "OpticRail"]
    tris, world_tris = count(first), count(world)
    log(f"  {tris} triangles in first person, {world_tris} for other players, {len(objects)} nodes")
    stem = spec["dir"] if spec["dir"] != key else key
    export(objects, out / f"{stem}.glb")
    if painted is not None:
        save_maps(stem, out, *painted)
    else:
        build_textures(stem, spec, out, uv_mask(objects))
    write_material(stem, out, spec["dir"])
    write_import(stem, out, spec["dir"])
    patch_scene(spec, markers)
    (out / f"{stem}.json").write_text(
        json.dumps(
            {
                "triangles": tris,
                "world_triangles": world_tris,
                "nodes": sorted(objects),
                "markers": {k: [round(c, 4) for c in v] for k, v in markers.items()},
            },
            indent=2,
        )
        + "\n"
    )


# Rows of the shared cartridge texture, top to bottom: (where the case ends
# and the bullet starts along the round, case colour, bullet colour,
# case roughness, case metallic). Shotgun shells: a brass head, then the hull.
CARTRIDGES = [
    ("rifle", 0.66, (0.58, 0.40, 0.17), (0.50, 0.25, 0.13), 0.32, 1.0),
    ("pistol", 0.74, (0.60, 0.42, 0.18), (0.52, 0.27, 0.14), 0.3, 1.0),
    ("shell", 0.22, (0.58, 0.40, 0.17), (0.32, 0.035, 0.03), 0.3, 1.0),
    ("steel", 0.66, (0.20, 0.21, 0.17), (0.50, 0.26, 0.14), 0.45, 0.6),
]


def build_shared():
    """The cartridge texture and material, and the tritium glow material,
    shared by every weapon."""
    out = WEAPONS_DIR / "shared"
    out.mkdir(exist_ok=True)
    size = 256
    u = np.linspace(0.0, 1.0, size, dtype=np.float32)[None, :]
    albedo = np.zeros((size, size, 3), np.float32)
    orm = np.ones((size, size, 3), np.float32)
    rng = np.random.default_rng(7)
    grain = rng.random((size, size)).astype(np.float32)
    for row, (name, split_at, case, bullet, rough, metal) in enumerate(CARTRIDGES):
        # Blender stores pixels bottom row first, and glTF flips V, so the
        # row the UVs pick (v = 1 - (row + 0.5) / 4) is this many rows from
        # the end of the array.
        rows = slice((3 - row) * size // 4, (4 - row) * size // 4)
        is_case = u < split_at
        if name == "shell":
            # Brass head, then the plastic hull: not metal, and the head's
            # rim and the crimp at the far end a little darker.
            col = np.where(is_case[..., None], np.array(case), np.array(bullet))
            r = np.where(is_case, rough, 0.55)
            m = np.where(is_case, metal, 0.0)
            col = col * np.where((u > 0.95) | (u < 0.03), 0.6, 1.0)[..., None]
        else:
            col = np.where(is_case[..., None], np.array(case), np.array(bullet))
            r = np.where(is_case, rough, 0.35)
            m = np.where(is_case, metal, 1.0)
            # The rim and extractor groove, and the case mouth.
            col = col * np.where((u > 0.03) & (u < 0.06), 0.55, 1.0)[..., None]
            col = col * np.where(np.abs(u - split_at) < 0.012, 0.7, 1.0)[..., None]
        albedo[rows] = col * (0.92 + 0.12 * grain[rows, :, None])
        orm[rows, :, 1] = np.clip(r + (grain[rows] - 0.5) * 0.08, 0.05, 1.0)
        orm[rows, :, 2] = m
    save_png(out / "cartridge_albedo.png", albedo, srgb=True)
    save_png(out / "cartridge_orm.png", orm, srgb=False)
    flat = np.zeros((16, 16, 3), np.float32) + np.array([0.5, 0.5, 1.0], np.float32)
    save_png(out / "flat_normal.png", flat, srgb=False)
    (out / "cartridge.tres").write_text(
        f"""[gd_resource type="StandardMaterial3D" format=3]

[ext_resource type="Texture2D" path="{SHARED}/cartridge_albedo.png" id="1_albedo"]
[ext_resource type="Texture2D" path="{SHARED}/cartridge_orm.png" id="2_orm"]
[ext_resource type="Texture2D" path="{SHARED}/flat_normal.png" id="3_normal"]

[resource]
albedo_texture = ExtResource("1_albedo")
metallic = 1.0
metallic_texture = ExtResource("2_orm")
metallic_texture_channel = 2
roughness_texture = ExtResource("2_orm")
roughness_texture_channel = 1
normal_enabled = true
normal_texture = ExtResource("3_normal")
"""
    )
    # Optics: black hard-anodised aluminium with a fine matte grain.
    grain = rng.random((size, size)).astype(np.float32)
    soft = resize(np.repeat(np.repeat(rng.random((size // 8, size // 8, 1)).astype(np.float32), 8, 0), 8, 1), size)[..., 0]
    optic_albedo = np.repeat((0.028 + 0.01 * grain + 0.008 * soft)[..., None], 3, axis=2)
    optic_orm = np.stack([np.ones_like(grain), 0.42 + 0.08 * grain + 0.06 * soft, np.full_like(grain, 0.35)], axis=2)
    save_png(out / "optics_albedo.png", optic_albedo, srgb=True)
    save_png(out / "optics_orm.png", optic_orm, srgb=False)
    (out / "optics.tres").write_text(
        f"""[gd_resource type="StandardMaterial3D" format=3]

[ext_resource type="Texture2D" path="{SHARED}/optics_albedo.png" id="1_albedo"]
[ext_resource type="Texture2D" path="{SHARED}/optics_orm.png" id="2_orm"]
[ext_resource type="Texture2D" path="{SHARED}/flat_normal.png" id="3_normal"]

[resource]
albedo_texture = ExtResource("1_albedo")
metallic = 1.0
metallic_texture = ExtResource("2_orm")
metallic_texture_channel = 2
roughness_texture = ExtResource("2_orm")
roughness_texture_channel = 1
normal_enabled = true
normal_texture = ExtResource("3_normal")
"""
    )
    # Coated glass: dark, glossy, a little green-blue in the reflection.
    (out / "lens.tres").write_text(
        """[gd_resource type="StandardMaterial3D" format=3]

[resource]
albedo_color = Color(0.02, 0.035, 0.04, 1)
metallic = 0.9
metallic_specular = 0.9
roughness = 0.04
"""
    )
    (out / "glow_tritium.tres").write_text(
        """[gd_resource type="StandardMaterial3D" format=3]

[resource]
albedo_color = Color(0.45, 1, 0.5, 1)
roughness = 0.3
emission_enabled = true
emission = Color(0.3, 1, 0.35, 1)
emission_energy_multiplier = 2.0
"""
    )


# Attachments the tool models, in metres. Each exports one node, `Body`, in
# the shared optics finish (black hard-anodised), with its origin where it
# mounts: the suppressor's rear face on the bore axis (it extends along -Z),
# the foregrip's clamp top on the rail (it hangs along -Y).
ATTACHMENTS = ("suppressor", "foregrip")


def build_attachment(key: str):
    reset()
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new("UV_0")
    ident = lambda p: Vector(p)  # noqa: E731 - already in Blender view space
    if key == "suppressor":
        # A 172 mm can: a threaded collar with wrench flats, a chamfer up to
        # the 38 mm tube, three grooves for grip near the rear, a vent ring,
        # and a stepped front cap round a 9 mm bore.
        r = 0.019
        profile = [
            (0.0, 0.0045), (0.0, 0.0125), (0.010, 0.0125), (0.0115, 0.0135), (0.014, 0.0135),
            (0.018, r), (0.024, r),
        ]
        y = 0.024
        for _ in range(3):
            profile += [(y + 0.0006, r - 0.0009), (y + 0.0036, r - 0.0009), (y + 0.0042, r)]
            y += 0.0085
        profile += [(0.150, r), (0.1506, r - 0.0012), (0.1534, r - 0.0012), (0.154, r), (0.165, r),
                    (0.168, r - 0.0012), (0.172, 0.0145), (0.172, 0.0072), (0.1705, 0.0058),
                    (0.1705, 0.0045), (0.0, 0.0045)]
        # Built along +Y (forward in Blender view space).
        revolve(bm, profile, 48, uv)
        # Wrench flats on the collar.
        for side in (-1.0, 1.0):
            box(bm, uv, (side * 0.0125 - 0.0012, 0.001, -0.007), (side * 0.0125 + 0.0012, 0.009, 0.007), ident)
    elif key == "foregrip":
        # A rail clamp (picatinny width, 50 mm), a tapered 32 mm grip 95 mm
        # long with three finger grooves, and a flared end cap.
        box(bm, uv, (-0.0115, -0.025, -0.012), (0.0115, 0.025, 0.0), ident, 0.0015)
        box(bm, uv, (-0.0135, -0.010, -0.010), (0.0135, 0.010, -0.002), ident, 0.001)
        grip = [(0.0, 0.0), (0.0, 0.0145), (0.012, 0.0150)]
        y = 0.022
        for _ in range(3):
            grip += [(y, 0.0158), (y + 0.006, 0.0146), (y + 0.012, 0.0158)]
            y += 0.016
        grip += [(0.084, 0.0162), (0.088, 0.0172), (0.095, 0.0172), (0.0975, 0.014), (0.0985, 0.0)]
        rings = revolve(bm, grip, 36, uv)
        # Stand it under the clamp: +Y along the grip becomes -Z (down).
        verts = [v for ring in rings for v in ring]
        bmesh.ops.transform(bm, matrix=Matrix.Translation((0, 0, -0.010)) @ Matrix.Rotation(-np.pi / 2, 4, "X"), verts=verts)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-7)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bmesh.ops.triangulate(bm, faces=[f for f in bm.faces if len(f.verts) > 4])
    mesh = bpy.data.meshes.new("Body")
    bm.to_mesh(mesh)
    bm.free()
    for poly in mesh.polygons:
        poly.use_smooth = True
    mesh.set_sharp_from_angle(angle=np.radians(35.0))
    obj = bpy.data.objects.new("Body", mesh)
    bpy.context.scene.collection.objects.link(obj)
    obj.data.materials.append(bpy.data.materials.new("Optics"))
    out = WEAPONS_DIR / key
    out.mkdir(parents=True, exist_ok=True)
    tris = sum(len(p.vertices) - 2 for p in mesh.polygons)
    log(f"{key}: {tris} triangles")
    export({"Body": obj}, out / f"{key}.glb")
    write_import(key, out, key)


# Fired cases, (distance from the head, radius) in metres, head to mouth:
# rim, extractor groove, body, and for bottlenecked rounds the shoulder and
# neck. `row` is the case's row on the shared cartridge texture
# (CARTRIDGES); `inner` the mouth's inside radius.
CASES = {
    "Case556": {"row": 0, "inner": 0.0028, "profile": [
        (0.0, 0.0048), (0.0011, 0.0048), (0.0013, 0.0040), (0.0024, 0.0040), (0.0030, 0.0048),
        (0.0365, 0.0045), (0.0395, 0.0032), (0.0447, 0.0032)]},
    "Case762x39": {"row": 3, "inner": 0.0039, "profile": [
        (0.0, 0.0056), (0.0015, 0.0056), (0.0017, 0.0047), (0.0030, 0.0047), (0.0036, 0.0055),
        (0.0305, 0.0051), (0.0330, 0.0043), (0.0387, 0.0043)]},
    "Case762x51": {"row": 0, "inner": 0.0039, "profile": [
        (0.0, 0.0060), (0.0014, 0.0060), (0.0016, 0.0050), (0.0028, 0.0050), (0.0034, 0.0059),
        (0.0390, 0.0054), (0.0435, 0.0043), (0.0512, 0.0043)]},
    "Case9mm": {"row": 1, "inner": 0.0045, "profile": [
        (0.0, 0.00495), (0.0013, 0.00495), (0.0014, 0.00425), (0.0023, 0.00425), (0.0028, 0.00495),
        (0.01915, 0.00483)]},
    "Case45": {"row": 1, "inner": 0.0057, "profile": [
        (0.0, 0.00605), (0.0012, 0.00605), (0.0013, 0.0052), (0.0023, 0.0052), (0.0029, 0.00605),
        (0.0228, 0.0060)]},
    # A fired 12-gauge shell: a brass head, then the plastic hull with its
    # crimp opened out.
    "Shell12": {"row": 2, "inner": 0.0096, "profile": [
        (0.0, 0.0112), (0.0015, 0.0112), (0.0017, 0.0104), (0.0160, 0.0104), (0.0163, 0.0102),
        (0.0650, 0.0101), (0.0680, 0.0104)]},
}


def build_casings():
    """`shared/casings.glb`: one node per fired case (CASES), standing on
    +Y (Godot) and centred on its middle, on the shared cartridge texture."""
    reset()
    objects = {}
    cartridge = bpy.data.materials.new("Cartridge")
    for name, info in CASES.items():
        bm = bmesh.new()
        uv = bm.loops.layers.uv.new("UV_0")
        outline = info["profile"]
        length = outline[-1][0]
        # Closed at the head, open at the mouth with the wall's thickness.
        profile = [(0.0, 0.0)] + outline + [(length, info["inner"]), (length - 0.002, info["inner"])]
        rings = revolve(bm, profile, 16, uv)
        row = info["row"]
        split = CARTRIDGES[row][1]
        # The whole case lies on the case colour; a shell's hull runs on past
        # its brass head.
        reach = 0.96 if CARTRIDGES[row][0] == "shell" else split - 0.03
        for face in bm.faces:
            for loop in face.loops:
                d = min(max(loop.vert.co.y, 0.0), length) / length
                loop[uv].uv = (0.02 + reach * d, 1.0 - (row + 0.5) / 4.0)
        verts = [v for ring in rings for v in ring]
        # Stand it up: +Y (Blender) along the case becomes +Z (up), centred.
        bmesh.ops.transform(bm, matrix=Matrix.Translation((0, 0, -length / 2)) @ Matrix.Rotation(np.pi / 2, 4, "X"), verts=verts)
        bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-7)
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
        bmesh.ops.triangulate(bm, faces=[f for f in bm.faces if len(f.verts) > 4])
        mesh = bpy.data.meshes.new(name)
        bm.to_mesh(mesh)
        bm.free()
        for poly in mesh.polygons:
            poly.use_smooth = True
        mesh.set_sharp_from_angle(angle=np.radians(50.0))
        mesh.materials.append(cartridge)
        obj = bpy.data.objects.new(name, mesh)
        bpy.context.scene.collection.objects.link(obj)
        objects[name] = obj
    out = WEAPONS_DIR / "shared"
    export(objects, out / "casings.glb")
    write_import("casings", out, "shared")
    log("casings:", ", ".join(objects))


# The optic housings' texture: an optic is a sixth of a rifle's size.
OPTIC_TEXTURE_SIZE = 1024


def build_optic(key: str):
    """The red dot or ACOG housing (tools/weapon_optics.py), painted like
    the M4A1 (tools/weapon_finish.py), into godot/assets/weapons/optics/."""
    reset()
    log(key)
    objects = weapon_optics.BUILDERS[key]()
    finish = dict(weapon_optics.FINISH, markings=weapon_optics.MARKINGS[key],
                  font=REPO / "godot/assets/fonts/BarlowCondensed-Bold.ttf")
    for obj in objects.values():
        weapon_finish.assign_zones(obj, finish, None)
    bpy.context.view_layer.update()
    painted = weapon_finish.paint(objects, {"finish": finish}, Matrix.Identity(4), OPTIC_TEXTURE_SIZE)
    for obj in objects.values():
        obj.data.attributes.remove(obj.data.attributes[weapon_finish.ZONE_ATTRIBUTE])
    objects["World"] = weapon_optics.world(objects)
    out = WEAPONS_DIR / "optics"
    out.mkdir(parents=True, exist_ok=True)
    tris = {name: sum(len(p.vertices) - 2 for p in obj.data.polygons) for name, obj in objects.items()}
    log(f"  {tris} triangles")
    export(objects, out / f"{key}.glb")
    save_maps(key, out, *painted)
    write_material(key, out, "optics")
    write_import(key, out, "optics")
    (out / f"{key}.json").write_text(json.dumps({"triangles": tris, "nodes": sorted(objects)}, indent=2) + "\n")


def main():
    args = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    build_shared()
    for key in args or list(WEAPONS) + list(ATTACHMENTS) + list(weapon_optics.BUILDERS) + ["casings"]:
        if key == "casings":
            build_casings()
        elif key in weapon_optics.BUILDERS:
            build_optic(key)
        elif key in ATTACHMENTS:
            build_attachment(key)
        else:
            build(key)
    log("done")


main()
