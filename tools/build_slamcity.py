#!/usr/bin/env python3
"""Generate godot/scenes/slamcity.tscn from tools/slamcity_layout.py.

The layout describes the map once (see its docstring for the lanes); this
writes the scene: collision boxes and hulls grouped by surface, the city mesh
that tools/build_slamcity_city.py builds in Blender, the Poly Haven props,
decals, baked-only lights, spawns, flags, bot lanes, practice targets, reverb
zones, and the dusk sky. It refuses a layout that fails the checks in
tools/slamcity_checks.py (which CI also runs), and stamps the layout's hash
into the scene so CI can tell a stale scene from a current one.

Edit the layout, not the scene, then rebuild everything and bake (the bake
needs a GPU, or Xvfb on a machine with one):

    uv run tools/build_slamcity_signs.py
    blender -b --python tools/build_slamcity_city.py
    python3 tools/build_slamcity.py
    tools/bake_lightmaps.sh res://scenes/slamcity.tscn
"""

from __future__ import annotations

import math
import sys
from pathlib import Path

repo = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(repo / "tools"))

import slamcity_checks as checks  # noqa: E402
import slamcity_layout as L  # noqa: E402

L.build()

out = repo / "godot/scenes/slamcity.tscn"
PI = math.pi
# The sky turns so its sunset glow sits due south, behind the Sun node.
SKY_YAW = 0.71
SUN_PITCH = -0.21


def f(v: float) -> str:
    v = round(v, 4)
    if v == int(v):
        return str(int(v))
    return repr(v)


def vec(x, y, z) -> str:
    return f"Vector3({f(x)}, {f(y)}, {f(z)})"


nodes: list[str] = []
subs: list[str] = []


def node(name, type_, parent, props=None, instance=None):
    head = f'[node name="{name}"'
    if type_:
        head += f' type="{type_}"'
    head += f' parent="{parent}"'
    if instance:
        head += f' instance=ExtResource("{instance}")'
    head += "]"
    lines = [head] + [f"{k} = {v}" for k, v in (props or {}).items()]
    nodes.append("\n".join(lines))


# --- Collision ---------------------------------------------------------------------
# One StaticBody3D per surface (its `surface` metadata picks impact effects and
# bullet penetration), with a shape per box or hull. Boxes of one size share a
# shape resource.
box_shapes: dict[tuple, str] = {}


def box_shape(size) -> str:
    key = tuple(round(s, 4) for s in size)
    if key not in box_shapes:
        sid = f"Box{len(box_shapes)}"
        box_shapes[key] = sid
        subs.append(f'[sub_resource type="BoxShape3D" id="{sid}"]\nsize = {vec(*key)}')
    return box_shapes[key]


node("Collision", "Node3D", ".")
by_surface: dict[str, list] = {}
for kind, surface, geo in L.COLLIDERS:
    by_surface.setdefault(surface, []).append((kind, geo))
hull_count = 0
for surface, items in sorted(by_surface.items()):
    body = surface.capitalize()
    node(body, "StaticBody3D", "Collision", {"metadata/surface": f'"{surface}"'})
    for i, (kind, geo) in enumerate(items):
        if kind == "box":
            x0, x1, y0, y1, z0, z1 = geo
            sid = box_shape((x1 - x0, y1 - y0, z1 - z0))
            node(f"S{i}", "CollisionShape3D", f"Collision/{body}", {
                "position": vec((x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2), "shape": f'SubResource("{sid}")'})
        else:
            sid = f"Hull{hull_count}"
            hull_count += 1
            pts = ", ".join(f"{f(x)}, {f(y)}, {f(z)}" for x, y, z in geo)
            subs.append(f'[sub_resource type="ConvexPolygonShape3D" id="{sid}"]\npoints = PackedVector3Array({pts})')
            node(f"S{i}", "CollisionShape3D", f"Collision/{body}", {"shape": f'SubResource("{sid}")'})

# --- Occluders --------------------------------------------------------------------
# The big solid pieces of the buildings' walls, as one occluder mesh: a quad
# down the middle of each, so the renderer can skip what is behind them.
def occluder_quads():
    quads = []
    for kind, surface, geo in L.COLLIDERS:
        if kind != "box" or surface != "concrete":
            continue
        x0, x1, y0, y1, z0, z1 = geo
        dx, dz, h = x1 - x0, z1 - z0, y1 - y0
        if h < 3.0 or min(dx, dz) > 0.5 or max(dx, dz) * h < 8.0:
            continue
        # Pull the quad in a little from the box's edges, so it stays hidden
        # inside the wall it stands for.
        y0, y1 = y0 + 0.05, y1 - 0.05
        if dx >= dz:
            zc = (z0 + z1) / 2
            quads.append([(x0 + 0.05, y0, zc), (x1 - 0.05, y0, zc), (x1 - 0.05, y1, zc), (x0 + 0.05, y1, zc)])
        else:
            xc = (x0 + x1) / 2
            quads.append([(xc, y0, z0 + 0.05), (xc, y0, z1 - 0.05), (xc, y1, z1 - 0.05), (xc, y1, z0 + 0.05)])
    return quads


quads = occluder_quads()
verts = [v for q in quads for v in q]
indices = [i for k in range(len(quads)) for i in (4 * k, 4 * k + 1, 4 * k + 2, 4 * k, 4 * k + 2, 4 * k + 3)]
subs.append('[sub_resource type="ArrayOccluder3D" id="Occluders"]\n'
            f'vertices = PackedVector3Array({", ".join(f"{f(x)}, {f(y)}, {f(z)}" for x, y, z in verts)})\n'
            f'indices = PackedInt32Array({", ".join(str(i) for i in indices)})')
node("Occluders", "OccluderInstance3D", ".", {"occluder": 'SubResource("Occluders")'})

# --- The city mesh, props, decals ---------------------------------------------------------
node("City", None, ".", instance="2_city")
node("Props", "Node3D", ".")
props_used = set()
for i, (model, (x, y, z), yaw, scale) in enumerate(L.PROPS):
    props_used.add(model)
    p = {"position": vec(x, y, z)}
    if yaw:
        p["rotation"] = vec(0, yaw, 0)
    if scale != 1.0:
        p["scale"] = vec(scale, scale, scale)
    node(f"{model}{i}", None, "Props", p, instance=f"p_{model}")

node("Decals", "Node3D", ".")
decals_used = set()
for i, d in enumerate(L.DECALS):
    decals_used.add(d["texture"])
    p = {"position": vec(*d["pos"])}
    if d["yaw"] or d["pitch"]:
        p["rotation"] = vec(d["pitch"], d["yaw"], 0)
    p["size"] = vec(*d["size"])
    p["texture_albedo"] = f'ExtResource("d_{d["texture"]}_albedo")'
    p["texture_normal"] = f'ExtResource("d_{d["texture"]}_normal")'
    if (repo / f"godot/assets/textures/decals/{d['texture']}_orm.png").exists():
        p["texture_orm"] = f'ExtResource("d_{d["texture"]}_orm")'
    p["modulate"] = "Color(%s)" % ", ".join(f(c) for c in d["modulate"])
    p["normal_fade"] = "0.3"
    p["cull_mask"] = "1"
    node(f"{d['texture']}{i}", "Decal", "Decals", p)

# Particle effects: steam in the pit, sparks at the viaduct's break, smoke
# from the derailed car.
node("Effects", "Node3D", ".")
effects_used = set()
for i, (scene, pos) in enumerate(L.EFFECTS):
    effects_used.add(scene)
    node(f"{scene}{i}", None, "Effects", {"position": vec(*pos)}, instance=f"fx_{scene}")

# --- Lights ---------------------------------------------------------------------
# Static and baked only: the node's script removes them in the game, so their
# light costs nothing at runtime (the lightmap and its probes keep it).
node("BakedLights", "Node3D", ".", {"script": 'ExtResource("22_baked")'})
for i, light in enumerate(L.LIGHTS):
    color = "Color(%s, 1)" % ", ".join(f(c) for c in light["color"])
    p = {"position": vec(*light["pos"])}
    if light["kind"] == "spot":
        p["rotation"] = vec(light["pitch"], light["yaw"], 0)
        p.update({"light_color": color, "light_energy": f(light["energy"]), "light_bake_mode": "1",
                  "shadow_enabled": "true" if light["shadow"] else "false", "spot_range": f(light["range"]),
                  "spot_angle": f(light["angle"]), "light_size": f(light["size"])})
        node(f"Spot{i}", "SpotLight3D", "BakedLights", p)
    else:
        p.update({"light_color": color, "light_energy": f(light["energy"]), "light_bake_mode": "1",
                  "shadow_enabled": "true" if light["shadow"] else "false", "omni_range": f(light["range"]),
                  "light_size": f(light["size"])})
        node(f"Omni{i}", "OmniLight3D", "BakedLights", p)

# --- Markers -------------------------------------------------------------------
# Spawns: team 0 in the west yard, team 1 in the east. The server picks among
# them by safety (tdm::choose_spawn); the first few are each team's start.
node("SpawnPoints", "Node3D", ".")
for team, m, yaw in ((0, False, -PI / 2), (1, True, PI / 2)):
    for i, (x, y, z) in enumerate(L.spawn_points()):
        node(f"Team{team}_{i:02d}", "Marker3D", "SpawnPoints", {
            "position": vec(L.mx(x, m), y + 0.05, z), "rotation": vec(0, yaw, 0), "metadata/team": str(team)})

# Capture the Flag: each team's flag stands in its own yard, by the dispatcher's
# booth at the mouth of the avenue. Team Deathmatch ignores these.
node("Flags", "Node3D", ".")
for team, m in ((0, False), (1, True)):
    node(f"Team{team}", "Marker3D", "Flags", {
        "position": vec(L.mx(L.FLAG[0], m), 0, L.FLAG[1]), "metadata/team": str(team)})

# Hardpoint rotates through these zones (server: map_zones).
node("Hardpoints", "Node3D", ".")
for name, label, (x, y, z), size in L.HARDPOINTS:
    node(name, "Marker3D", "Hardpoints", {
        "position": vec(x, y, z), "metadata/size": vec(*size), "metadata/label": f'"{label}"'})

node("Spawns", "Node3D", ".")
x0, _, z0 = L.spawn_points()[0]
node("West", "Marker3D", "Spawns", {"position": vec(x0, 0.05, z0), "rotation": vec(0, -PI / 2, 0)})
node("East", "Marker3D", "Spawns", {"position": vec(-x0, 0.05, z0), "rotation": vec(0, PI / 2, 0)})

# Lanes for bots to follow (rust/game/src/net/bots.rs), each from west to east.
node("BotLanes", "Node3D", ".")
for lane, points in L.BOT_LANES:
    node(lane, "Node3D", "BotLanes")
    for i, (x, z) in enumerate(points):
        node(f"P{i}", "Marker3D", f"BotLanes/{lane}", {"position": vec(x, 0.05, z)})

# Practice targets for offline play, on the east half (the server removes them).
node("Targets", "Node3D", ".")
for name, pos, ry, weapon, extra in L.TARGETS:
    p = {"position": vec(*pos)}
    if ry:
        p["rotation"] = vec(0, ry, 0)
    p["weapon"] = f'"{weapon}"'
    p.update(extra)
    node(name, None, "Targets", p, instance="4_dummy")

# Reverb zones: the rooms' acoustics (rust/game/src/acoustics.rs).
node("ReverbZones", "Node3D", ".")


def reverb_zone(name, pos, size, room, damping, wet, predelay):
    node(name, "ReverbZone", "ReverbZones", {
        "position": vec(*pos), "size": vec(*size), "room_size": f(room),
        "damping": f(damping), "wet": f(wet), "predelay_ms": f(predelay)})


for tag, m in (("W", False), ("E", True)):
    for name, (x, y, z), size, *settings in L.REVERB_ROOMS:
        reverb_zone(name + tag, (L.mx(x, m), y, z), size, *settings)
for name, pos, size, *settings in L.CENTRE_ROOMS:
    reverb_zone(name, pos, size, *settings)

# Ambience (rust/game/src/ambience.rs): city traffic and wind outside, room tone
# in the reverb zones; the air conditioners hum and the lamps buzz on their own.
node("Soundscape", "Soundscape", ".", {"outdoor": '"city"', "outdoor_db": f(-15.0), "indoor_db": f(-14.0)})

# The player, HUD, and pause menu: the same scenes as the testing arena.
node("Player", None, ".", {"position": vec(x0, 0.05, z0), "rotation": vec(0, -PI / 2, 0)}, instance="1_player")
node("HUD", None, ".", instance="19_hud")
node("PauseMenu", None, ".", {"player_path": 'NodePath("../Player")'}, instance="5_pause")


# --- Checks ----------------------------------------------------------------------
HIDDEN = checks.run_all()

# --- Resources ----------------------------------------------------------------------
ext = [
    '[ext_resource type="PackedScene" path="res://scenes/player.tscn" id="1_player"]',
    '[ext_resource type="PackedScene" path="res://assets/maps/slamcity/city.gltf" id="2_city"]',
    '[ext_resource type="PackedScene" path="res://scenes/props/target_dummy.tscn" id="4_dummy"]',
    '[ext_resource type="PackedScene" path="res://scenes/ui/pause_menu.tscn" id="5_pause"]',
    '[ext_resource type="PackedScene" path="res://scenes/ui/hud.tscn" id="19_hud"]',
    '[ext_resource type="Script" path="res://scripts/slamcity.gd" id="20_level"]',
    '[ext_resource type="Texture2D" path="res://assets/skies/slamcity_dusk.hdr" id="21_sky"]',
    '[ext_resource type="Script" path="res://scripts/baked_lights.gd" id="22_baked"]',
]
ext += [f'[ext_resource type="PackedScene" path="res://assets/props/{n}/{n}.gltf" id="p_{n}"]'
        for n in sorted(props_used)]
ext += [f'[ext_resource type="PackedScene" path="res://scenes/effects/{n}.tscn" id="fx_{n}"]'
        for n in sorted(effects_used)]
ext += [f'[ext_resource type="Texture2D" path="res://assets/textures/decals/{n}_{k}.png" id="d_{n}_{k}"]'
        for n in sorted(decals_used) for k in ("albedo", "normal", "orm")
        if (repo / f"godot/assets/textures/decals/{n}_{k}.png").exists()]

# Dusk: the sun low in the south behind the storefronts, a warm sky lighting the
# shade, haze that fades the skyline, and glow for the neon.
env = f"""[sub_resource type="PanoramaSkyMaterial" id="SkyMat"]
panorama = ExtResource("21_sky")
energy_multiplier = 0.85

[sub_resource type="Sky" id="Sky"]
sky_material = SubResource("SkyMat")
radiance_size = 2

[sub_resource type="Environment" id="Env"]
background_mode = 2
sky = SubResource("Sky")
sky_rotation = Vector3(0, {f(SKY_YAW)}, 0)
ambient_light_source = 3
ambient_light_sky_contribution = 1.0
ambient_light_energy = 0.55
reflected_light_source = 2
tonemap_mode = 4
tonemap_white = 6.0
tonemap_exposure = 0.88
ssao_enabled = true
ssao_radius = 1.4
ssao_intensity = 1.8
glow_enabled = true
glow_intensity = 0.7
glow_bloom = 0.05
glow_hdr_threshold = 1.1
glow_blend_mode = 1
fog_enabled = true
fog_light_color = Color(0.95, 0.6, 0.45, 1)
fog_light_energy = 0.75
fog_sun_scatter = 0.15
fog_density = 0.0034
fog_aerial_perspective = 0.6
fog_sky_affect = 0.15
adjustment_enabled = true
adjustment_contrast = 1.08
adjustment_saturation = 1.12"""

root = f"""[node name="Slamcity" type="Node3D"]
script = ExtResource("20_level")
metadata/layout_hash = "{checks.layout_hash()}"

[node name="WorldEnvironment" type="WorldEnvironment" parent="."]
environment = SubResource("Env")

[node name="LightmapGI" type="LightmapGI" parent="."]
quality = 1
bounces = 3
environment_mode = 1
max_texture_size = 4096

[node name="Sun" type="DirectionalLight3D" parent="."]
rotation = Vector3({f(SUN_PITCH)}, 0, 0)
light_color = Color(1, 0.6, 0.36, 1)
light_energy = 2.2
shadow_enabled = true
shadow_blur = 1.5
directional_shadow_max_distance = 120.0"""

text = "\n\n".join(["[gd_scene format=3]", "\n".join(ext), "\n\n".join(subs), env, root, *nodes]) + "\n"
out.write_text(text)
print(f"wrote {out} ({len(nodes)} nodes, {len(L.COLLIDERS)} collision shapes, {len(L.PROPS)} props; "
      f"of {len(L.SPAWN_ORDER)} spawns, {HIDDEN[0]} hide from our own high ground and {HIDDEN[1]} from an "
      f"enemy inside our half)")
