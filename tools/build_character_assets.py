# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy>=2"]
# ///
"""Builds the soldier character and its animation sets from Quaternius's
Universal Animation Library (CC0).

Sources (download the free Standard zips and extract them):

- Universal Animation Library: https://opengameart.org/content/universal-animation-library
  (``Godot/AnimationLibrary_Godot_Standard.glb``)
- Universal Animation Library 2: https://opengameart.org/content/universal-animation-library-2
  (``Unreal-Godot/UAL2_Standard.glb``)

Then run:

    uv run tools/build_character_assets.py <path to AnimationLibrary_Godot_Standard.glb> \\
        <path to UAL2_Standard.glb>

It writes three files to ``godot/assets/characters/``:

- ``soldier/soldier.glb``: the mannequin as a soldier. The body is split into
  surfaces by the bone each triangle follows (skin, uniform, gloves, pants,
  boots), and low-poly gear is added on the same skin: helmet with a team band,
  goggles, plate carrier with pouches, belt, knee pads, and team armbands. Gear is
  lofted around the body itself, so it fits and deforms with it. A third mesh,
  ``ArmsFP``, holds only the arms (sleeves, gloves, armbands) for the first-person
  view. Material names are palette roles that the game recolours per team.
- ``animations/ual1.glb`` and ``animations/ual2.glb``: the animations the game
  uses, on the libraries' own skeletons, with everything else stripped.

Godot retargets all three onto its humanoid skeleton profile when it imports
them (see the ``.import`` files and ``bone_map_*.tres``), so animations from either
library play on the soldier. Open the editor or run ``godot --headless --import``
after this script.
"""

from __future__ import annotations

import argparse
import copy
import json
import struct
from dataclasses import dataclass, field
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "godot" / "assets" / "characters"

UAL1_ANIMATIONS = [
    "Idle_Loop",
    "Walk_Loop",
    "Jog_Fwd_Loop",
    "Sprint_Loop",
    "Crouch_Idle_Loop",
    "Crouch_Fwd_Loop",
    "Jump_Start",
    "Jump_Loop",
    "Jump_Land",
    "Death01",
    "Hit_Chest",
    "Hit_Head",
    "Pistol_Idle_Loop",
    "Pistol_Aim_Neutral",
    "Pistol_Aim_Up",
    "Pistol_Aim_Down",
    "Pistol_Reload",
    "Pistol_Shoot",
]
UAL2_ANIMATIONS = [
    "Slide_Start",
    "Slide_Loop",
    "Slide_Exit",
    "Hit_Knockback",
    "Zombie_Idle_Loop",
    "Zombie_Walk_Fwd_Loop",
    "Zombie_Scratch",
    "ClimbUp_1m_RM",
]

# Body regions by the bone a vertex mostly follows (UAL1 bone names).
REGION_OF_BONE = {
    "DEF-head": "skin",
    "DEF-neck": "skin",
    "DEF-spine.001": "uniform",
    "DEF-spine.002": "uniform",
    "DEF-spine.003": "uniform",
    "DEF-shoulder.L": "uniform",
    "DEF-shoulder.R": "uniform",
    "DEF-upper_arm.L": "uniform",
    "DEF-upper_arm.R": "uniform",
    "DEF-forearm.L": "uniform",
    "DEF-forearm.R": "uniform",
    "DEF-hips": "pants",
    "DEF-thigh.L": "pants",
    "DEF-thigh.R": "pants",
    "DEF-shin.L": "pants",
    "DEF-shin.R": "pants",
    "DEF-foot.L": "boots",
    "DEF-foot.R": "boots",
    "DEF-toe.L": "boots",
    "DEF-toe.R": "boots",
}
ARM_BONES = ("DEF-upper_arm.", "DEF-forearm.", "DEF-hand.", "DEF-f_", "DEF-thumb.")

# Palette roles, in material order. The game recolours them per team.
ROLES = [
    "skin",
    "uniform",
    "gloves",
    "pants",
    "boots",
    "mask",
    "helmet",
    "visor",
    "vest",
    "webbing",
    "accent",
]
# Colours the file ships with (the default team), so it also reads in other tools.
DEFAULT_COLOURS = {
    "skin": (0.62, 0.45, 0.34),
    "uniform": (0.36, 0.38, 0.27),
    "gloves": (0.12, 0.12, 0.11),
    "pants": (0.33, 0.34, 0.25),
    "boots": (0.16, 0.13, 0.1),
    "mask": (0.2, 0.21, 0.17),
    "helmet": (0.3, 0.32, 0.23),
    "visor": (0.05, 0.06, 0.07),
    "vest": (0.27, 0.29, 0.2),
    "webbing": (0.2, 0.21, 0.15),
    "accent": (0.15, 0.45, 0.85),
}

COMPONENT = {
    np.dtype(np.int8): 5120,
    np.dtype(np.uint8): 5121,
    np.dtype(np.int16): 5122,
    np.dtype(np.uint16): 5123,
    np.dtype(np.uint32): 5125,
    np.dtype(np.float32): 5126,
}
DTYPE = {v: k for k, v in COMPONENT.items()}
TYPE_OF_WIDTH = {1: "SCALAR", 2: "VEC2", 3: "VEC3", 4: "VEC4", 16: "MAT4"}
WIDTH_OF_TYPE = {v: k for k, v in TYPE_OF_WIDTH.items()}


# --- glTF reading and writing ---------------------------------------------------


class Gltf:
    """A binary glTF file held as its JSON plus the binary chunk."""

    def __init__(self, path: Path):
        data = path.read_bytes()
        json_len = struct.unpack_from("<I", data, 12)[0]
        self.json = json.loads(data[20 : 20 + json_len])
        offset = 20 + json_len
        bin_len = struct.unpack_from("<I", data, offset)[0]
        self.bin = data[offset + 8 : offset + 8 + bin_len]

    def accessor(self, index: int) -> np.ndarray:
        acc = self.json["accessors"][index]
        view = self.json["bufferViews"][acc["bufferView"]]
        dtype = DTYPE[acc["componentType"]]
        width = WIDTH_OF_TYPE[acc["type"]]
        start = view.get("byteOffset", 0) + acc.get("byteOffset", 0)
        stride = view.get("byteStride", dtype.itemsize * width)
        rows = [
            np.frombuffer(self.bin, dtype=dtype, count=width, offset=start + i * stride)
            for i in range(acc["count"])
        ]
        out = np.array(rows, dtype=dtype).reshape(acc["count"], width)
        if acc.get("normalized") and dtype.kind == "u":
            out = out.astype(np.float32) / np.iinfo(dtype).max
        return out

    def node_index(self, name: str) -> int:
        return next(i for i, n in enumerate(self.json["nodes"]) if n["name"] == name)

    def joint_names(self) -> list[str]:
        nodes = self.json["nodes"]
        return [nodes[j]["name"] for j in self.json["skins"][0]["joints"]]


@dataclass
class Writer:
    """Builds a new binary glTF from numpy arrays."""

    doc: dict = field(
        default_factory=lambda: {
            "asset": {"version": "2.0", "generator": "tools/build_character_assets.py"},
            "accessors": [],
            "bufferViews": [],
            "buffers": [],
        }
    )
    blob: bytearray = field(default_factory=bytearray)

    def add(self, array: np.ndarray, target: int | None = None, bounds: bool = False) -> int:
        array = np.ascontiguousarray(array)
        if array.ndim == 1:
            array = array.reshape(-1, 1)
        while len(self.blob) % 4:
            self.blob.append(0)
        view = {"buffer": 0, "byteOffset": len(self.blob), "byteLength": array.nbytes}
        if target is not None:
            view["target"] = target
        self.blob.extend(array.tobytes())
        self.doc["bufferViews"].append(view)
        accessor = {
            "bufferView": len(self.doc["bufferViews"]) - 1,
            "componentType": COMPONENT[array.dtype],
            "count": int(array.shape[0]),
            "type": TYPE_OF_WIDTH[array.shape[1]],
        }
        if bounds:
            accessor["min"] = array.min(axis=0).tolist()
            accessor["max"] = array.max(axis=0).tolist()
        self.doc["accessors"].append(accessor)
        return len(self.doc["accessors"]) - 1

    def save(self, path: Path) -> None:
        while len(self.blob) % 4:
            self.blob.append(0)
        self.doc["buffers"] = [{"byteLength": len(self.blob)}]
        text = json.dumps(self.doc, separators=(",", ":")).encode()
        text += b" " * (-len(text) % 4)
        body = (
            struct.pack("<II", len(text), 0x4E4F534A)
            + text
            + struct.pack("<II", len(self.blob), 0x004E4942)
            + bytes(self.blob)
        )
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(struct.pack("<III", 0x46546C67, 2, 12 + len(body)) + body)
        print(f"wrote {path.relative_to(ROOT)} ({path.stat().st_size / 1e6:.2f} MB)")


@dataclass
class Surface:
    """One primitive: vertices with skinning, triangle indices, and a material role."""

    role: str
    positions: np.ndarray
    normals: np.ndarray
    joints: np.ndarray
    weights: np.ndarray
    indices: np.ndarray

    def compacted(self) -> Surface:
        used, remap = np.unique(self.indices, return_inverse=True)
        return Surface(
            self.role,
            self.positions[used],
            self.normals[used],
            self.joints[used],
            self.weights[used],
            remap.reshape(-1, 3).astype(np.uint32),
        )


def write_merged_mesh(w: Writer, name: str, surfaces: list[Surface]) -> int:
    """All surfaces as one primitive with the shared `character` material, so a
    character draws in one call per mesh. Each vertex carries its rest-pose
    position (UV = x, y; UV2.x = z), for detail that sticks to the body while it
    animates, and its palette role (UV2.y = (index + 0.5) / 16), which
    shaders/character.gdshader colours per team."""
    positions, normals, joints, weights, indices, uv0, uv1 = [], [], [], [], [], [], []
    base = 0
    for s in surfaces:
        s = s.compacted()
        if len(s.indices) == 0:
            continue
        n = len(s.positions)
        positions.append(s.positions)
        normals.append(s.normals)
        joints.append(s.joints)
        weights.append(s.weights)
        indices.append(s.indices + base)
        uv0.append(s.positions[:, :2])
        role = (ROLES.index(s.role) + 0.5) / 16.0
        uv1.append(np.column_stack([s.positions[:, 2], np.full(n, role)]))
        base += n
    pos = np.concatenate(positions).astype(np.float32)
    index_type = np.uint16 if len(pos) < 65536 else np.uint32
    primitive = {
        "attributes": {
            "POSITION": w.add(pos, 34962, bounds=True),
            "NORMAL": w.add(np.concatenate(normals).astype(np.float32), 34962),
            "TEXCOORD_0": w.add(np.concatenate(uv0).astype(np.float32), 34962),
            "TEXCOORD_1": w.add(np.concatenate(uv1).astype(np.float32), 34962),
            "JOINTS_0": w.add(np.concatenate(joints).astype(np.uint8), 34962),
            "WEIGHTS_0": w.add(np.concatenate(weights).astype(np.float32), 34962),
        },
        "indices": w.add(np.concatenate(indices).reshape(-1).astype(index_type), 34963),
        "material": len(ROLES),
    }
    w.doc.setdefault("meshes", []).append({"name": name, "primitives": [primitive]})
    return len(w.doc["meshes"]) - 1


def write_mesh(w: Writer, name: str, surfaces: list[Surface]) -> int:
    primitives = []
    for s in surfaces:
        s = s.compacted()
        if len(s.indices) == 0:
            continue
        index_type = np.uint16 if len(s.positions) < 65536 else np.uint32
        primitives.append(
            {
                "attributes": {
                    "POSITION": w.add(s.positions.astype(np.float32), 34962, bounds=True),
                    "NORMAL": w.add(s.normals.astype(np.float32), 34962),
                    "JOINTS_0": w.add(s.joints.astype(np.uint8), 34962),
                    "WEIGHTS_0": w.add(s.weights.astype(np.float32), 34962),
                },
                "indices": w.add(s.indices.reshape(-1).astype(index_type), 34963),
                "material": ROLES.index(s.role),
            }
        )
    w.doc.setdefault("meshes", []).append({"name": name, "primitives": primitives})
    return len(w.doc["meshes"]) - 1


def copy_rig(src: Gltf, w: Writer) -> None:
    """Copies every node (same indices), the skin, and the scene. Meshes are
    attached by the caller."""
    nodes = copy.deepcopy(src.json["nodes"])
    for node in nodes:
        node.pop("mesh", None)
        node.pop("skin", None)
    w.doc["nodes"] = nodes
    skin = src.json["skins"][0]
    w.doc["skins"] = [
        {
            "name": skin.get("name", "Skin"),
            "joints": list(skin["joints"]),
            "inverseBindMatrices": w.add(src.accessor(skin["inverseBindMatrices"]).astype(np.float32)),
        }
    ]
    w.doc["scenes"] = copy.deepcopy(src.json["scenes"])
    w.doc["scene"] = src.json.get("scene", 0)
    w.doc["materials"] = [
        {
            "name": role,
            "pbrMetallicRoughness": {
                "baseColorFactor": [*DEFAULT_COLOURS[role], 1.0],
                "metallicFactor": 0.0,
                "roughnessFactor": 0.4 if role == "visor" else 0.85,
            },
        }
        for role in ROLES
    ] + [
        {
            # Every merged mesh (write_merged_mesh). The game swaps in a team's
            # shaders/character.gdshader material.
            "name": "character",
            "pbrMetallicRoughness": {"baseColorFactor": [0.4, 0.4, 0.4, 1.0], "metallicFactor": 0.0},
        }
    ]


def attach_mesh(w: Writer, src: Gltf, node_name: str, mesh: int, parent: str) -> None:
    nodes = w.doc["nodes"]
    existing = [i for i, n in enumerate(nodes) if n["name"] == node_name]
    if existing:
        index = existing[0]
    else:
        nodes.append({"name": node_name})
        index = len(nodes) - 1
        nodes[src.node_index(parent)].setdefault("children", []).append(index)
    nodes[index]["mesh"] = mesh
    nodes[index]["skin"] = 0


def copy_animations(src: Gltf, w: Writer, names: list[str]) -> None:
    """Copies the named animations. Root-motion clips (`_RM`) keep their root
    bone where it starts: the game moves the body itself."""
    by_name = {a["name"]: a for a in src.json["animations"]}
    root = src.json["skins"][0]["joints"][0]
    missing = [n for n in names if n not in by_name]
    if missing:
        raise SystemExit(f"animations not found: {missing}")
    out = []
    for name in names:
        anim = by_name[name]
        samplers = []
        frozen = {
            ch["sampler"]
            for ch in anim["channels"]
            if name.endswith("_RM") and ch["target"]["node"] == root and ch["target"]["path"] == "translation"
        }
        for i, s in enumerate(anim["samplers"]):
            times = src.accessor(s["input"]).astype(np.float32)
            values = src.accessor(s["output"]).astype(np.float32)
            if i in frozen:
                values = np.repeat(values[:1], len(values), axis=0)
            samplers.append(
                {
                    "input": w.add(times, bounds=True),
                    "output": w.add(values),
                    "interpolation": s.get("interpolation", "LINEAR"),
                }
            )
        out.append({"name": name, "samplers": samplers, "channels": copy.deepcopy(anim["channels"])})
    w.doc["animations"] = out


# --- Body ---------------------------------------------------------------------


@dataclass
class Body:
    """The source mannequin's vertices merged from its primitives, in bind pose."""

    positions: np.ndarray
    normals: np.ndarray
    joints: np.ndarray
    weights: np.ndarray
    triangles: np.ndarray
    dominant: np.ndarray
    joint_names: list[str]
    bone_heads: dict[str, np.ndarray]

    @classmethod
    def load(cls, src: Gltf) -> Body:
        positions, normals, joints, weights, triangles = [], [], [], [], []
        base = 0
        for prim in src.json["meshes"][0]["primitives"]:
            attrs = prim["attributes"]
            pos = src.accessor(attrs["POSITION"]).astype(np.float64)
            positions.append(pos)
            normals.append(src.accessor(attrs["NORMAL"]).astype(np.float64))
            joints.append(src.accessor(attrs["JOINTS_0"]).astype(np.int64))
            weights.append(src.accessor(attrs["WEIGHTS_0"]).astype(np.float64))
            triangles.append(src.accessor(prim["indices"]).reshape(-1, 3).astype(np.int64) + base)
            base += len(pos)
        joints_a = np.concatenate(joints)
        weights_a = np.concatenate(weights)
        dominant = joints_a[np.arange(len(joints_a)), weights_a.argmax(axis=1)]
        names = src.joint_names()
        heads = {name: world_matrix(src, src.node_index(name))[:3, 3] for name in names}
        return cls(
            np.concatenate(positions),
            np.concatenate(normals),
            joints_a,
            weights_a,
            np.concatenate(triangles),
            dominant,
            names,
            heads,
        )

    def bone(self, name: str) -> int:
        return self.joint_names.index(name)

    def vertices_of(self, *prefixes: str) -> np.ndarray:
        """Indices of vertices that mostly follow a bone whose name starts with a prefix."""
        names = np.array(self.joint_names)[self.dominant]
        mask = np.zeros(len(names), dtype=bool)
        for p in prefixes:
            mask |= np.char.startswith(names, p)
        return np.nonzero(mask)[0]

    def triangle_bones(self) -> list[str]:
        """The bone each triangle follows: the majority of its corners' bones."""
        out = []
        for tri in self.dominant[self.triangles]:
            values, counts = np.unique(tri, return_counts=True)
            out.append(self.joint_names[values[counts.argmax()]])
        return out

    def surface(self, role: str, triangles: np.ndarray) -> Surface:
        return Surface(role, self.positions, self.normals, self.joints, self.weights, triangles)


def world_matrix(src: Gltf, index: int) -> np.ndarray:
    nodes = src.json["nodes"]
    parent = {c: i for i, n in enumerate(nodes) for c in n.get("children", [])}
    m = local_matrix(nodes[index])
    while index in parent:
        index = parent[index]
        m = local_matrix(nodes[index]) @ m
    return m


def local_matrix(node: dict) -> np.ndarray:
    if "matrix" in node:
        return np.array(node["matrix"], dtype=np.float64).reshape(4, 4).T
    x, y, z, w = node.get("rotation", [0, 0, 0, 1])
    r = np.array(
        [
            [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
            [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
            [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)],
        ]
    )
    m = np.eye(4)
    m[:3, :3] = r * np.array(node.get("scale", [1, 1, 1]))
    m[:3, 3] = node.get("translation", [0, 0, 0])
    return m


# --- Gear -----------------------------------------------------------------------


@dataclass
class Shape:
    """Flat-shaded gear geometry before skinning."""

    role: str
    positions: list[np.ndarray] = field(default_factory=list)

    def tri(self, a, b, c) -> None:
        self.positions.extend([np.asarray(a, float), np.asarray(b, float), np.asarray(c, float)])

    def quad(self, a, b, c, d) -> None:
        self.tri(a, b, c)
        self.tri(a, c, d)


def ring_samples(points: np.ndarray, centre: np.ndarray, u: np.ndarray, v: np.ndarray, segs: int):
    """Largest distance from `centre` in the (u, v) plane per angle bin, filled in
    across empty bins."""
    rel = points - centre
    pu, pv = rel @ u, rel @ v
    angles = np.arctan2(pv, pu)
    radii = np.hypot(pu, pv)
    bins = ((angles + np.pi) / (2 * np.pi) * segs).astype(int) % segs
    out = np.full(segs, np.nan)
    for b in range(segs):
        sel = radii[bins == b]
        if len(sel):
            out[b] = sel.max()
    if np.all(np.isnan(out)):
        raise SystemExit("no vertices near a gear ring")
    good = np.nonzero(~np.isnan(out))[0]
    for b in np.nonzero(np.isnan(out))[0]:
        dist = np.minimum(np.abs(good - b), segs - np.abs(good - b))
        out[b] = out[good[dist.argmin()]]
    return out


def loft(
    points: np.ndarray,
    origin: np.ndarray,
    axis: np.ndarray,
    stations: list[float],
    segs: int,
    offset: float | list[float],
    band: float,
    arc: tuple[float, float] | None = None,
    up_hint: np.ndarray | None = None,
) -> np.ndarray:
    """Rings around `axis` that wrap the given points, pushed out by `offset`.

    Returns rings[station][segment] positions. Angle 0 points along `u`, which is
    the up hint (default +Z, the character's front) made perpendicular to the axis.
    With `arc`, only angles in that range (radians) are kept, for open shapes.
    """
    axis = axis / np.linalg.norm(axis)
    hint = np.array([0.0, 0.0, 1.0]) if up_hint is None else up_hint
    u = hint - axis * (hint @ axis)
    u /= np.linalg.norm(u)
    v = np.cross(axis, u)
    offsets = offset if isinstance(offset, list) else [offset] * len(stations)
    rings = []
    for t, off in zip(stations, offsets):
        centre_on_axis = origin + axis * t
        rel = points - centre_on_axis
        near = points[np.abs(rel @ axis) < band]
        # Centre the ring on the points' middle in the plane.
        mid = near.mean(axis=0)
        centre = centre_on_axis + u * ((mid - centre_on_axis) @ u) + v * ((mid - centre_on_axis) @ v)
        radii = ring_samples(near, centre, u, v, 72)
        ring = []
        count = segs if arc is None else segs + 1
        for i in range(count):
            if arc is None:
                a = -np.pi + 2 * np.pi * i / segs
            else:
                a = arc[0] + (arc[1] - arc[0]) * i / segs
            k = int((a + np.pi) / (2 * np.pi) * 72) % 72
            # Widest of the neighbouring bins, so thin gaps don't dent the shell.
            r = max(radii[(k + d) % 72] for d in (-2, -1, 0, 1, 2)) + off
            ring.append(centre + (u * np.cos(a) + v * np.sin(a)) * r)
        rings.append(np.array(ring))
    return np.array(rings)


def skin_rings(shape: Shape, rings: np.ndarray, closed: bool) -> None:
    stations, count = rings.shape[:2]
    for s in range(stations - 1):
        for i in range(count if closed else count - 1):
            j = (i + 1) % count
            a, b, c, d = rings[s][i], rings[s][j], rings[s + 1][j], rings[s + 1][i]
            shape.quad(a, b, c, d)


def lip(shape: Shape, outer: np.ndarray, inset: float, centre: np.ndarray, closed: bool, flip: bool) -> None:
    """A rim from a ring inward toward `centre`, giving the shell a visible thickness."""
    inner = outer + (centre - outer) * inset
    count = len(outer)
    for i in range(count if closed else count - 1):
        j = (i + 1) % count
        if flip:
            shape.quad(outer[i], inner[i], inner[j], outer[j])
        else:
            shape.quad(outer[j], inner[j], inner[i], outer[i])


def cap(shape: Shape, ring: np.ndarray, apex: np.ndarray) -> None:
    for i in range(len(ring)):
        j = (i + 1) % len(ring)
        shape.tri(ring[i], ring[j], apex)


def box(shape: Shape, centre, size, frame: np.ndarray | None = None) -> None:
    """An axis-aligned box (or one in `frame`'s columns)."""
    frame = np.eye(3) if frame is None else frame
    h = np.asarray(size, float) / 2
    corners = {}
    for sx in (-1, 1):
        for sy in (-1, 1):
            for sz in (-1, 1):
                corners[(sx, sy, sz)] = np.asarray(centre) + frame @ (h * np.array([sx, sy, sz]))
    c = corners
    shape.quad(c[(1, -1, -1)], c[(1, 1, -1)], c[(1, 1, 1)], c[(1, -1, 1)])
    shape.quad(c[(-1, -1, 1)], c[(-1, 1, 1)], c[(-1, 1, -1)], c[(-1, -1, -1)])
    shape.quad(c[(-1, 1, -1)], c[(-1, 1, 1)], c[(1, 1, 1)], c[(1, 1, -1)])
    shape.quad(c[(-1, -1, 1)], c[(-1, -1, -1)], c[(1, -1, -1)], c[(1, -1, 1)])
    shape.quad(c[(-1, -1, 1)], c[(1, -1, 1)], c[(1, 1, 1)], c[(-1, 1, 1)])
    shape.quad(c[(1, -1, -1)], c[(-1, -1, -1)], c[(-1, 1, -1)], c[(1, 1, -1)])


def shape_surface(body: Body, shape: Shape, bones: list[str], outward_from: np.ndarray | None) -> Surface:
    """Flat-shaded triangles, each corner skinned like the nearest body vertex that
    follows one of `bones`. Faces are wound to point away from `outward_from` (a
    point, or None to keep the winding as built)."""
    pos = np.array(shape.positions)
    tris = pos.reshape(-1, 3, 3)
    normals = np.cross(tris[:, 1] - tris[:, 0], tris[:, 2] - tris[:, 0])
    if outward_from is not None:
        centres = tris.mean(axis=1)
        flip = np.einsum("ij,ij->i", normals, centres - outward_from) < 0
        tris[flip] = tris[flip][:, ::-1]
        normals[flip] *= -1
    lengths = np.linalg.norm(normals, axis=1, keepdims=True)
    keep = lengths[:, 0] > 1e-10
    tris, normals = tris[keep], normals[keep] / lengths[keep]
    pos = tris.reshape(-1, 3)
    norm = np.repeat(normals, 3, axis=0)
    candidates = body.vertices_of(*bones)
    cand_pos = body.positions[candidates]
    nearest = np.array([candidates[np.argmin(((cand_pos - p) ** 2).sum(axis=1))] for p in pos])
    joints = body.joints[nearest]
    weights = body.weights[nearest]
    # Drop influences from bones outside the list, then renormalise.
    allowed = {body.bone(n) for n in body.joint_names if n.startswith(tuple(bones))}
    mask = np.vectorize(lambda j: j in allowed)(joints)
    weights = np.where(mask, weights, 0.0)
    totals = weights.sum(axis=1, keepdims=True)
    weights = np.where(totals > 0, weights / np.maximum(totals, 1e-9), 0.0)
    # A corner with no allowed influence follows the first listed bone.
    lone = totals[:, 0] <= 0
    joints[lone] = 0
    joints[lone, 0] = body.bone(next(n for n in body.joint_names if n.startswith(bones[0])))
    weights[lone] = [1.0, 0.0, 0.0, 0.0]
    indices = np.arange(len(pos), dtype=np.uint32).reshape(-1, 3)
    return Surface(shape.role, pos, norm, joints, weights, indices)


def build_gear(body: Body) -> list[Surface]:
    up = np.array([0.0, 1.0, 0.0])
    surfaces = []

    # Helmet: a low-poly dome over the head, cut higher at the brow than at the
    # back and sides, with a rim and a thin band in the team colour.
    head = body.positions[body.vertices_of("DEF-head")]
    upper = head[head[:, 1] > 1.66]
    centre = np.array([0.0, 1.705, (upper[:, 2].min() + upper[:, 2].max()) / 2])
    radii = np.array(
        [
            np.abs(upper[:, 0]).max() + 0.034,
            head[:, 1].max() - centre[1] + 0.03,
            (upper[:, 2].max() - upper[:, 2].min()) / 2 + 0.034,
        ]
    )
    segs = 14

    def dome_ring(elevation: np.ndarray) -> np.ndarray:
        """Points on the dome at an elevation (radians) per segment."""
        out = []
        for i in range(segs):
            a = 2 * np.pi * i / segs  # 0 faces +Z, the character's front
            e = elevation[i]
            out.append(
                centre
                + np.array(
                    [
                        np.sin(a) * np.cos(e) * radii[0],
                        np.sin(e) * radii[1],
                        np.cos(a) * np.cos(e) * radii[2],
                    ]
                )
            )
        return np.array(out)

    angles = 2 * np.pi * np.arange(segs) / segs
    front = np.maximum(0.0, np.cos(angles)) ** 2
    # The bottom edge: over the ears and the back of the neck, up at the brow.
    bottom = np.radians(-14.0) + front * np.radians(30.0)
    levels = [bottom]
    for t in (0.18, 0.3, 0.55, 0.78):
        levels.append(bottom + (np.radians(84.0) - bottom) * t)
    rings = [dome_ring(e) for e in levels]
    helmet = Shape("helmet")
    accent = Shape("accent")
    for k in range(len(rings) - 1):
        target = accent if k == 1 else helmet
        for i in range(segs):
            j = (i + 1) % segs
            target.quad(rings[k][i], rings[k][j], rings[k + 1][j], rings[k + 1][i])
    cap(helmet, rings[-1], centre + np.array([0.0, radii[1], 0.0]))
    # Rim: a short flare out and down, then a lip back in, for thickness.
    flare = centre + (rings[0] - centre) * np.array([1.07, 1.0, 1.07]) - np.array([0, 0.008, 0])
    for i in range(segs):
        j = (i + 1) % segs
        helmet.quad(flare[i], flare[j], rings[0][j], rings[0][i])
    inner = centre + (flare - centre) * np.array([0.9, 1.0, 0.9])
    for i in range(segs):
        j = (i + 1) % segs
        helmet.quad(inner[i], inner[j], flare[j], flare[i])
    surfaces.append(shape_surface(body, helmet, ["DEF-head"], centre - np.array([0, 0.05, 0])))
    surfaces.append(shape_surface(body, accent, ["DEF-head"], centre))

    # Goggles over the eyes, and a strap around the back of the head.
    origin = np.array([0.0, 0.0, 0.0])
    goggles = Shape("visor")
    g = loft(head, origin, up, [1.664, 1.704], 8, [0.011, 0.01], 0.014, arc=(-0.95, 0.95))
    for i in range(8):
        goggles.quad(g[0][i], g[0][i + 1], g[1][i + 1], g[1][i])
    # Frame: a lip around the lenses, so they read from the side.
    for k, d in ((0, -1.0), (1, 1.0)):
        rim = g[k] + np.array([0.0, 0.005 * d, 0.0])
        for i in range(8):
            goggles.quad(g[k][i], g[k][i + 1], rim[i + 1], rim[i])
    strap = loft(head, origin, up, [1.672, 1.69], 10, 0.008, 0.014, arc=(0.95, 2 * np.pi - 0.95))
    for i in range(10):
        goggles.quad(strap[0][i], strap[0][i + 1], strap[1][i + 1], strap[1][i])
    surfaces.append(shape_surface(body, goggles, ["DEF-head"], centre - np.array([0, 0.03, 0])))

    # Face mask: covers the lower face and neck.
    mask = Shape("mask")
    neck_head = body.positions[body.vertices_of("DEF-head", "DEF-neck")]
    m = loft(neck_head, origin, up, [1.49, 1.545, 1.59, 1.628], 12, [0.008, 0.007, 0.006, 0.005], 0.012)
    skin_rings(mask, m, True)
    surfaces.append(shape_surface(body, mask, ["DEF-head", "DEF-neck"], np.array([0.0, 1.57, centre[2]])))

    # Plate carrier: a shell around the chest and belly with a rim top and bottom.
    torso = body.positions[body.vertices_of("DEF-spine")]
    stations = [1.05, 1.12, 1.2, 1.28, 1.35, 1.41]
    vest_rings = loft(torso, origin, up, stations, 14, 0.03, 0.03)
    # Armholes: pull the top ring's sides down to the second-highest ring.
    for i in range(14):
        a = -np.pi + 2 * np.pi * i / 14
        side = abs(np.sin(a))  # 1 at the sides (+-X), 0 front and back
        if side > 0.8:
            vest_rings[-1][i] = vest_rings[-2][i] + up * 0.02
    vest = Shape("vest")
    skin_rings(vest, vest_rings, True)
    torso_centre = np.array([0.0, 1.22, torso[:, 2].mean()])
    lip(vest, vest_rings[-1], 0.18, np.array([0.0, vest_rings[-1][0][1], torso_centre[2]]), True, False)
    lip(vest, vest_rings[0], 0.15, np.array([0.0, vest_rings[0][0][1], torso_centre[2]]), True, True)
    surfaces.append(shape_surface(body, vest, ["DEF-spine"], torso_centre))

    # Pouches on the front of the carrier and the belt.
    webbing = Shape("webbing")
    front_z = vest_rings[1][:, 2].max()
    for x in (-0.09, 0.0, 0.09):
        box(webbing, (x, 1.135, front_z + 0.02), (0.07, 0.11, 0.05))
        box(webbing, (x, 1.195, front_z + 0.022), (0.075, 0.018, 0.056))  # flap
    radio = Shape("webbing")
    box(radio, (0.1, 1.33, front_z + 0.012), (0.05, 0.09, 0.03))
    surfaces.append(shape_surface(body, webbing, ["DEF-spine.001", "DEF-spine.002"], torso_centre))
    surfaces.append(shape_surface(body, radio, ["DEF-spine.003"], torso_centre))

    # Belt around the hips, with two side pouches.
    hips = body.positions[body.vertices_of("DEF-hips", "DEF-spine.001")]
    belt_rings = loft(hips, origin, up, [0.94, 0.99], 14, 0.018, 0.025)
    belt = Shape("webbing")
    skin_rings(belt, belt_rings, True)
    hip_centre = np.array([0.0, 0.965, hips[:, 2].mean()])
    lip(belt, belt_rings[1], 0.12, np.array([0.0, belt_rings[1][0][1], hip_centre[2]]), True, False)
    lip(belt, belt_rings[0], 0.12, np.array([0.0, belt_rings[0][0][1], hip_centre[2]]), True, True)
    for x in (-0.17, 0.17):
        box(belt, (x, 0.93, hip_centre[2] - 0.02), (0.05, 0.1, 0.09))
    surfaces.append(shape_surface(body, belt, ["DEF-hips"], hip_centre))

    # Knee pads: half shells over the front of each knee.
    for side, sign in (("L", 1.0), ("R", -1.0)):
        knee = body.bone_heads[f"DEF-shin.{side}"]
        leg = body.positions[body.vertices_of(f"DEF-shin.{side}", f"DEF-thigh.{side}")]
        pad_rings = loft(leg, knee, up, [-0.06, -0.01, 0.05], 5, 0.016, 0.03, arc=(-1.0, 1.0))
        pad = Shape("webbing")
        for s in range(2):
            for i in range(5):
                pad.quad(pad_rings[s][i], pad_rings[s][i + 1], pad_rings[s + 1][i + 1], pad_rings[s + 1][i])
        surfaces.append(shape_surface(body, pad, [f"DEF-shin.{side}"], knee - np.array([0, 0, 0.05])))

    surfaces.extend(armbands(body))
    return merge_by_role(surfaces)


def armbands(body: Body) -> list[Surface]:
    out = []
    for side in ("L", "R"):
        shoulder = body.bone_heads[f"DEF-upper_arm.{side}"]
        elbow = body.bone_heads[f"DEF-forearm.{side}"]
        axis = elbow - shoulder
        length = np.linalg.norm(axis)
        arm = body.positions[body.vertices_of(f"DEF-upper_arm.{side}")]
        rings = loft(arm, shoulder, axis, [length * 0.36, length * 0.52], 10, 0.012, 0.03, up_hint=np.array([0.0, 1.0, 0.0]))
        band = Shape("accent")
        skin_rings(band, rings, True)
        mid = shoulder + axis / length * length * 0.44
        out.append(shape_surface(body, band, [f"DEF-upper_arm.{side}"], mid))
    return out


def merge_by_role(surfaces: list[Surface]) -> list[Surface]:
    merged: dict[str, list[Surface]] = {}
    for s in surfaces:
        merged.setdefault(s.role, []).append(s)
    out = []
    for role, parts in merged.items():
        base = 0
        indices = []
        for p in parts:
            indices.append(p.indices + base)
            base += len(p.positions)
        out.append(
            Surface(
                role,
                np.concatenate([p.positions for p in parts]),
                np.concatenate([p.normals for p in parts]),
                np.concatenate([p.joints for p in parts]),
                np.concatenate([p.weights for p in parts]),
                np.concatenate(indices),
            )
        )
    return out


# --- Outputs --------------------------------------------------------------------


def body_surfaces(body: Body) -> tuple[list[Surface], list[Surface]]:
    """Third-person body surfaces by region, and the first-person arm surfaces."""
    bones = body.triangle_bones()
    regions: dict[str, list[int]] = {}
    arms: dict[str, list[int]] = {}
    for t, bone in enumerate(bones):
        role = "gloves" if bone.startswith(("DEF-hand.", "DEF-f_", "DEF-thumb.")) else REGION_OF_BONE.get(bone)
        if role is None:
            raise SystemExit(f"no region for bone {bone}")
        regions.setdefault(role, []).append(t)
        corners = np.array(body.joint_names)[body.dominant[body.triangles[t]]]
        if all(str(c).startswith(ARM_BONES) for c in corners):
            arms.setdefault(role, []).append(t)
    third = [body.surface(r, body.triangles[np.array(ts)]) for r, ts in regions.items()]
    first = [body.surface(r, body.triangles[np.array(ts)]) for r, ts in arms.items()]
    return third, first


def build_soldier(src: Gltf) -> None:
    body = Body.load(src)
    w = Writer()
    copy_rig(src, w)
    third, first = body_surfaces(body)
    gear = build_gear(body)
    attach_mesh(w, src, "Mannequin", write_merged_mesh(w, "Body", third), "Rig")
    w.doc["nodes"][src.node_index("Mannequin")]["name"] = "Body"
    attach_mesh(w, src, "Gear", write_merged_mesh(w, "Gear", gear), "Rig")
    fp = first + [s for s in armbands(body)]
    attach_mesh(w, src, "ArmsFP", write_merged_mesh(w, "ArmsFP", merge_by_role(fp)), "Rig")
    tris = sum(len(s.compacted().indices) for s in third + gear)
    print(f"soldier: {tris} triangles third-person, {len(body.joint_names)} bones")
    w.save(OUT / "soldier" / "soldier.glb")


def build_animations(src: Gltf, names: list[str], path: Path) -> None:
    """The skeleton, a one-triangle stand-in mesh (so the importer builds the
    skeleton), and the listed animations."""
    w = Writer()
    copy_rig(src, w)
    w.doc["materials"] = w.doc["materials"][:1]
    root = src.json["skins"][0]["joints"][0]
    tri = Surface(
        "skin",
        np.array([[0, 0, 0], [0.001, 0, 0], [0, 0.001, 0]], float),
        np.array([[0, 0, 1]] * 3, float),
        np.zeros((3, 4), int),
        np.array([[1.0, 0, 0, 0]] * 3),
        np.array([[0, 1, 2]]),
    )
    tri.joints[:, 0] = src.json["skins"][0]["joints"].index(root)
    mesh_node = next(
        n["name"] for n in src.json["nodes"] if "mesh" in n
    )
    attach_mesh(w, src, mesh_node, write_mesh(w, "Stub", [tri]), "Rig")
    copy_animations(src, w, names)
    w.save(path)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("ual1", type=Path, help="AnimationLibrary_Godot_Standard.glb")
    parser.add_argument("ual2", type=Path, help="UAL2_Standard.glb")
    args = parser.parse_args()
    ual1 = Gltf(args.ual1)
    ual2 = Gltf(args.ual2)
    build_soldier(ual1)
    build_animations(ual1, UAL1_ANIMATIONS, OUT / "animations" / "ual1.glb")
    build_animations(ual2, UAL2_ANIMATIONS, OUT / "animations" / "ual2.glb")


if __name__ == "__main__":
    main()
