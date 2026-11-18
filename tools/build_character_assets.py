# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy>=2", "pillow>=11"]
# ///
"""Builds the game's characters and their animation sets.

Run it with no arguments:

    uv run tools/build_character_assets.py

It downloads its pinned CC0 sources into a cache (``--cache``, by default
``~/.cache/shooter-characters``), checks their hashes, installs MPFB 2
(MakeHuman for Blender) into a private Blender profile there, and then:

1. Runs ``tools/blender_characters.py`` in Blender for each kit (the soldier,
   the first-person arms, the zombies). It builds MakeHuman humans, dresses
   them, models the gear, fits everything onto the soldier skeleton with its
   weights, packs every part into one texture atlas, and bakes albedo, normal,
   roughness, occlusion, and palette-role maps.
2. Writes each kit as glTF on the original skeleton (copied node for node
   from Quaternius's Universal Animation Library, so the game's bones, rest
   pose, and proportions never change), one mesh with one material
   (``character_<kit>``), and its atlas as ``<kit>_{albedo,normal,orm}.png``.
3. Writes ``animations/ual1.glb`` and ``animations/ual2.glb``: the animations
   the game uses, on the libraries' own skeletons, with everything else
   stripped.

The animation libraries are not downloaded: get the free Standard zips of
Universal Animation Library (https://opengameart.org/content/universal-animation-library)
and Universal Animation Library 2
(https://opengameart.org/content/universal-animation-library-2), extract them,
and pass ``--ual1 .../Godot/AnimationLibrary_Godot_Standard.glb`` and
``--ual2 .../Unreal-Godot/UAL2_Standard.glb`` (defaults: under ``~/.cache/ual-src``).

Godot retargets every file onto its humanoid skeleton profile when it imports
them (see the ``.import`` files and ``bone_map_*.tres``). Open the editor or run
``godot --headless --import`` afterwards. ``--kits soldier`` rebuilds one kit;
``--reuse`` skips Blender and reassembles from the last bake.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
import shutil
import struct
import subprocess
import urllib.request
import zipfile
from dataclasses import dataclass, field
from pathlib import Path

import numpy as np
from PIL import Image

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

# Palette roles, in the order the game numbers them (rust/game/src/character/palette.rs).
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
    "eyes",
    "metal",
]
# Roles whose albedo is shading detail only; the game colours them per team.
TINTED = {"uniform", "gloves", "pants", "boots", "mask", "helmet", "vest", "webbing", "accent"}

MH = "https://files.makehumancommunity.org/asset_packs"
# name: (url, sha256). Every MakeHuman pack here is CC0 (its packs/*.json
# lists each item's licence); MPFB's code is GPL-3.0 but only runs the build.
SOURCES = {
    "mpfb": (
        "https://extensions.blender.org/download/sha256:c075b3e9b94b8e5556a5f03f4653602d02ea306de08126e8f55ed6dee28a2dad/add-on-mpfb-v2.0.18.zip",
        "c075b3e9b94b8e5556a5f03f4653602d02ea306de08126e8f55ed6dee28a2dad",
    ),
    "makehuman_system_assets": (
        f"{MH}/makehuman_system_assets/makehuman_system_assets_cc0.zip",
        "b542127a8e25547c7c29c19f2d1d2adb9a664c80396ecd694095dbc8028a0107",
    ),
    "skins02": (f"{MH}/skins02/skins02_cc0.zip", "1613f1ef3afca53094511d26620ed7cf1d2dedc29ed3d384d60bdebe250698ae"),
    "pants01": (f"{MH}/pants01/pants01_cc0.zip", "e4e0ec60db34f279be291a83cfd7b342a7c5cf09bb7676682a5f39f4f6ac4ad9"),
    "shirts01": (f"{MH}/shirts01/shirts01_cc0.zip", "a5a723b0e84a109bb190fcfeac7f1de4138d875da3e30fe5b3340eac9f38bcd3"),
    "shoes01": (f"{MH}/shoes01/shoes01_cc0.zip", "ded3f70428505eabbf1f6d7b5f61196a7366ef20757103d276ad0ed336c35ada"),
    "gloves01": (f"{MH}/gloves01/gloves01_cc0.zip", "ecdaee1d02749d17352791d415cb622a883350cc8a4b90eda3725aef35d9afb2"),
    "hands01": (f"{MH}/hands01/hands01_cc0.zip", "b874210dc3a9a179b73a3dd04976bc27521f47f985d106ce46fb45a480ee1701"),
}
# The MakeHuman file server answers a bare "Mozilla/5.0" with HTTP 466.
USER_AGENT = "curl/8.0"


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


def write_character_mesh(w: Writer, name: str, parts: list[dict], joint_names: list[str]) -> tuple[int, int]:
    """Every part as one primitive with the kit's one material, so a character
    draws in one call. TEXCOORD_0 is the atlas; TEXCOORD_1 is the palette role
    and the part group ((index + 0.5) / 16 each), which shaders/character.gdshader
    reads. Returns the mesh index and the triangle count."""
    positions, normals, tangents, uvs, roles, joints, weights, indices = [], [], [], [], [], [], [], []
    base = 0
    for part in parts:
        n = len(part["positions"])
        positions.append(part["positions"])
        normals.append(part["normals"])
        tangents.append(part["tangents"])
        uvs.append(part["uvs"])
        role = (ROLES.index(part["role"]) + 0.5) / 16.0
        group = (part["group"] + 0.5) / 16.0
        roles.append(np.tile([role, group], (n, 1)))
        j, wt = top_influences(part["weights"], part["bones"], joint_names)
        joints.append(j)
        weights.append(wt)
        indices.append(part["indices"] + base)
        base += n
    pos = np.concatenate(positions).astype(np.float32)
    nrm = np.concatenate(normals)
    nrm /= np.linalg.norm(nrm, axis=1, keepdims=True)
    tan = np.concatenate(tangents)
    tan[:, :3] /= np.maximum(np.linalg.norm(tan[:, :3], axis=1, keepdims=True), 1e-9)
    index_type = np.uint16 if len(pos) < 65536 else np.uint32
    tris = np.concatenate(indices)
    primitive = {
        "attributes": {
            "POSITION": w.add(pos, 34962, bounds=True),
            "NORMAL": w.add(nrm.astype(np.float32), 34962),
            "TANGENT": w.add(tan.astype(np.float32), 34962),
            "TEXCOORD_0": w.add(np.concatenate(uvs).astype(np.float32), 34962),
            "TEXCOORD_1": w.add(np.concatenate(roles).astype(np.float32), 34962),
            "JOINTS_0": w.add(np.concatenate(joints).astype(np.uint8), 34962),
            "WEIGHTS_0": w.add(np.concatenate(weights).astype(np.float32), 34962),
        },
        "indices": w.add(tris.reshape(-1).astype(index_type), 34963),
        "material": 0,
    }
    w.doc.setdefault("meshes", []).append({"name": name, "primitives": [primitive]})
    return len(w.doc["meshes"]) - 1, len(tris)


def top_influences(weights: np.ndarray, bones: list[str], joint_names: list[str]) -> tuple[np.ndarray, np.ndarray]:
    """The four strongest bones per vertex, as skin joint indices, normalised."""
    index = np.array([joint_names.index(b) for b in bones])
    order = np.argsort(-weights, axis=1)[:, :4]
    if order.shape[1] < 4:
        order = np.pad(order, ((0, 0), (0, 4 - order.shape[1])))
    top = np.take_along_axis(weights, order, axis=1)
    top = np.where(top > 1e-3, top, 0.0)
    total = top.sum(axis=1, keepdims=True)
    if np.any(total <= 0):
        raise SystemExit("a vertex has no bone weights")
    joints = np.where(top > 0, index[order], 0)
    return joints, top / total


def write_stub_mesh(w: Writer, joint: int) -> int:
    """A one-triangle mesh on one bone, so the importer builds the skeleton."""
    prim = {
        "attributes": {
            "POSITION": w.add(np.array([[0, 0, 0], [0.001, 0, 0], [0, 0.001, 0]], np.float32), 34962, bounds=True),
            "JOINTS_0": w.add(np.array([[joint, 0, 0, 0]] * 3, np.uint8), 34962),
            "WEIGHTS_0": w.add(np.array([[1.0, 0, 0, 0]] * 3, np.float32), 34962),
        },
        "indices": w.add(np.array([0, 1, 2], np.uint16), 34963),
        "material": 0,
    }
    w.doc.setdefault("meshes", []).append({"name": "Stub", "primitives": [prim]})
    return len(w.doc["meshes"]) - 1


def copy_rig(src: Gltf, w: Writer, material: str = "character") -> None:
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
    # The game swaps in a team's shaders/character.gdshader material by name.
    w.doc["materials"] = [
        {"name": material, "pbrMetallicRoughness": {"baseColorFactor": [0.5, 0.5, 0.5, 1.0], "metallicFactor": 0.0}}
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


# --- Sources ----------------------------------------------------------------------


def fetch(cache: Path) -> dict[str, Path]:
    """Downloads and extracts every pinned source; returns name -> directory."""
    out = {}
    for name, (url, digest) in SOURCES.items():
        archive = cache / "downloads" / f"{name}.zip"
        if not archive.exists() or sha256(archive) != digest:
            archive.parent.mkdir(parents=True, exist_ok=True)
            print(f"downloading {name}")
            request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
            with urllib.request.urlopen(request) as response, open(archive, "wb") as f:
                shutil.copyfileobj(response, f)
            if sha256(archive) != digest:
                raise SystemExit(f"{name}: unexpected sha256 {sha256(archive)}")
        target = cache / "src" / name
        if not target.exists():
            with zipfile.ZipFile(archive) as z:
                z.extractall(target)
        out[name] = target
    return out


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def blender_env(cache: Path) -> dict[str, str]:
    env = dict(os.environ)
    env["BLENDER_USER_RESOURCES"] = str(cache / "blender-user")
    return env


def install_mpfb(cache: Path, src: dict[str, Path]) -> None:
    if (cache / "blender-user" / "extensions" / "user_default" / "mpfb").exists():
        return
    subprocess.run(
        ["blender", "-b", "--factory-startup", "--command", "extension", "install-file",
         "-r", "user_default", "-e", str(cache / "downloads" / "mpfb.zip")],
        env=blender_env(cache),
        check=True,
    )


def asset_paths(src: dict[str, Path]) -> dict[str, str]:
    """Short names for the MakeHuman assets the kits use."""
    sys_ = src["makehuman_system_assets"]
    files = {
        "cargo": src["pants01"] / "clothes/cortu_cargo_pants/cortu_cargo_pants.mhclo",
        "shirt": src["shirts01"] / "clothes/toigo_fisherman_sweater/toigo_fisherman_sweater.mhclo",
        "boots": sys_ / "clothes/shoes03/shoes03.mhclo",
        "gloves": src["gloves01"] / "clothes/toigo_gloves_short/toigo_gloves_short.mhclo",
        "eyes": sys_ / "eyes/low-poly/low-poly.mhclo",
        "skin_young_caucasian_male": sys_ / "skins/young_caucasian_male/young_caucasian_male.mhmat",
        # Civilian clothes and bodies for the zombies.
        "overalls": sys_ / "clothes/male_worksuit01/male_worksuit01.mhclo",
        "jacket": sys_ / "clothes/male_casualsuit05/male_casualsuit05.mhclo",
        "tshirt": sys_ / "clothes/male_casualsuit06/male_casualsuit06.mhclo",
        "skin_middleage_african_male": sys_ / "skins/middleage_african_male/middleage_african_male.mhmat",
        "skin_old_caucasian_male": sys_ / "skins/old_caucasian_male/old_caucasian_male.mhmat",
        "skin_young_asian_male": sys_ / "skins/young_asian_male/young_asian_male.mhmat",
        "blouse_skirt": sys_ / "clothes/female_elegantsuit01/female_elegantsuit01.mhclo",
        "ankle_boots": src["shoes01"] / "clothes/toigo_ankle_boots_female/toigo_ankle_boots_female.mhclo",
        "skin_middleage_caucasian_female": sys_ / "skins/middleage_caucasian_female/middleage_caucasian_female.mhmat",
    }
    for name, path in files.items():
        if not path.exists():
            raise SystemExit(f"missing asset {name}: {path}")
    return {k: str(v) for k, v in files.items()}


# --- Kits -------------------------------------------------------------------------

# How each material is baked (tools/blender_characters.py): procedural relief
# ("detail"), roughness, metallic, and for untinted roles their colour. Tinted
# roles bake in grey; the game colours them per team.
GREY = (0.5, 0.5, 0.5)
MATERIALS = {
    # The first-person sleeves' own bake (their albedo, normal, roughness, and
    # occlusion come from their high-detail source; see tools/blender_hilo.py).
    "sleeve": {"colour": GREY, "detail": {"kind": "ripstop", "strength": 0.3, "distance": 0.0012}, "roughness": 0.88},
    "uniform": {"use_diffuse": False, "use_bump": False, "colour": GREY,
                "detail": {"kind": "ripstop", "strength": 0.3, "distance": 0.0012}, "roughness": 0.9},
    "pants": {"use_diffuse": False, "colour": GREY, "detail": {"kind": "ripstop", "strength": 0.15, "distance": 0.0012}, "roughness": 0.9},
    "gloves": {"detail": {"kind": "grain", "scale": 700, "strength": 0.25, "distance": 0.001}, "roughness": 0.6},
    "boots": {"use_diffuse": False, "colour": GREY, "detail": {"kind": "grain", "scale": 500, "strength": 0.3, "distance": 0.001}, "roughness": 0.72},
    "skin": {"roughness": 0.5},
    "eyes": {"roughness": 0.08},
    "helmet": {"colour": GREY, "detail": {"kind": "cordura", "strength": 0.3, "distance": 0.001, "fade": 0.35}, "roughness": 0.72},
    "molle": {"colour": GREY, "detail": {"kind": "molle", "strength": 0.6, "distance": 0.0025}, "roughness": 0.85},
    "cordura": {"colour": GREY, "detail": {"kind": "cordura", "strength": 0.35, "distance": 0.0012}, "roughness": 0.76},
    "strap": {"colour": GREY, "detail": {"kind": "strap", "strength": 0.4, "distance": 0.001}, "roughness": 0.66},
    "velcro": {"colour": (0.45, 0.45, 0.45), "detail": {"kind": "velcro", "strength": 0.4, "distance": 0.001}, "roughness": 0.95},
    "band": {"colour": GREY, "detail": {"kind": "strap", "strength": 0.3, "distance": 0.0008, "fade": 0.1}, "roughness": 0.6},
    "knit": {"colour": GREY, "detail": {"kind": "knit", "strength": 0.2, "distance": 0.0008, "cavity": 0.12, "fade": 0.15}, "roughness": 0.95},
    "plastic": {"colour": GREY, "detail": {"kind": "grit", "scale": 300, "strength": 0.15, "distance": 0.0006}, "roughness": 0.55},
    # Boot soles: near-black rubber with a lugged tread.
    "sole": {"colour": (0.04, 0.038, 0.036), "detail": {"kind": "lugs", "strength": 0.7, "distance": 0.0025, "cavity": 0.3, "fade": 0.1},
             "roughness": 0.82},
    "rubber": {"colour": GREY, "detail": {"kind": "grain", "scale": 900, "strength": 0.2, "distance": 0.0006}, "roughness": 0.7},
    "polymer": {"colour": (0.05, 0.05, 0.048), "detail": {"kind": "grit", "scale": 500, "strength": 0.15, "distance": 0.0005, "fade": 0.15}, "roughness": 0.5},
    "metal": {"colour": (0.06, 0.06, 0.062), "detail": {"kind": "grit", "scale": 800, "strength": 0.1, "distance": 0.0003, "fade": 0.2}, "roughness": 0.42, "metallic": 0.6},
    "lens": {"colour": (0.045, 0.05, 0.055), "roughness": 0.06},
    # Zombies' clothes: the garment's own texture, worn matte.
    "civilian": {"detail": {"kind": "grain", "scale": 400, "strength": 0.15, "distance": 0.001}, "roughness": 0.92},
}

KITS = {
    "soldier": {
        "spec": {
            "macro": {
                "gender": 1.0,
                "age": 0.5,
                "muscle": 0.7,
                "weight": 0.55,
                "height": 0.6,
                "race": {"caucasian": 0.8, "african": 0.1, "asian": 0.1},
            },
            "skin": "skin_young_caucasian_male",
            "parts": [
                {"asset": "cargo", "role": "pants", "subdiv": 1, "tighten": 0.016, "split_legs": 0.82},
                {"asset": "shirt", "role": "uniform", "tighten": 0.012, "decimate": 0.75},
                {"asset": "boots", "role": "boots", "decimate": 0.4},
                {"asset": "gloves", "role": "gloves", "decimate": 0.35},
                {"asset": "eyes", "role": "eyes", "type": "Eyes"},
            ],
            "keep_body": {"bones": ["DEF-head", "DEF-neck"], "threshold": 0.35},
            "body_decimate": 0.32,
            # A close crop under the helmet and stubble on bare faces (paint_hair).
            "hair": {"colour": (0.12, 0.085, 0.06), "front": 0.05, "nape": 0.055, "density": 0.85, "stubble": 0.65},
            "gear": {
                "helmet": 0,
                "glasses": 0,
                "balaclava": 0,
                "gaiter": 0,
                "cover_decimate": 0.4,
                "plate_carrier": [1.1, 1.4],
                "belt": 0.99,
                "knee_pads": 0,
                "boot_shafts": 0,
                "boot_soles": 0,
                "arm_bands": 0,
                "knuckles": 0,
            },
        },
        "density": {"skin": 3.0, "eyes": 2.0},
        # Face coverings and the skin they replace never occlude each other.
        "ao_rounds": [[[0, 3, 4], [0, 3, 4]], [[1], [0, 1]], [[2], [0, 2]]],
        "size": 2048,
        "poses": [["Crouch_Idle_Loop", 10], ["Sprint_Loop", 6]],
        # BodyFar, past FAR_DISTANCE in rig.rs: about a fifth of the triangles.
        "far_ratio": 0.2,
        "meshes": {"Body": None},
    },
    # The first-person arms: modelled gloves and cloth-simulated sleeves with
    # high-detail sources baked onto them (tools/blender_hilo.py), and a
    # watch, on an atlas of their own.
    "arms": {
        "spec": {
            "macro": {
                "gender": 1.0,
                "age": 0.5,
                "muscle": 0.7,
                "weight": 0.55,
                "height": 0.6,
                "race": {"caucasian": 0.8, "african": 0.1, "asian": 0.1},
            },
            "skin": "skin_young_caucasian_male",
            "parts": [
                {"asset": "eyes", "role": "eyes", "type": "Eyes"},
            ],
            "keep_body": {"bones": ["DEF-head", "DEF-neck"], "threshold": 0.35},
            "hilo": {"gloves": True, "sleeves": True, "glove_ratio": 0.62, "sleeve_ratio": 0.42},
            "gear": {"watch": 0},
            "keep_only": {"bones": ["DEF-upper_arm.", "DEF-forearm.", "DEF-hand.", "DEF-f_", "DEF-thumb."], "threshold": 0.5},
        },
        # The gloves fill the screen; they get most of the atlas.
        "density": {"gloves": 4.0},
        "size": 2048,
        "meshes": {"ArmsFP": None},
    },
    # The undead: the same body and clothes with bare hands and no kit, so
    # nothing under a helmet or plates is cut away.
    "zombie": {
        "spec": {
            "macro": {
                "gender": 1.0,
                "age": 0.5,
                "muscle": 0.45,
                "weight": 0.5,
                "height": 0.6,
                "race": {"caucasian": 0.8, "african": 0.1, "asian": 0.1},
            },
            "skin": "skin_young_caucasian_male",
            "parts": [
                {"asset": "cargo", "role": "pants", "material": "civilian", "subdiv": 1},
                {"asset": "shirt", "role": "uniform", "material": "civilian", "tighten": 0.018, "decimate": 0.75},
                {"asset": "boots", "role": "boots", "material": "civilian", "decimate": 0.4},
                {"asset": "eyes", "role": "eyes", "type": "Eyes"},
            ],
            "keep_body": {"bones": ["DEF-head", "DEF-neck", "DEF-hand.", "DEF-f_", "DEF-thumb."], "threshold": 0.35},
            "body_decimate": 0.32,
            # Short hair and stubble, painted on (paint_hair in blender_characters.py).
            "hair": {"colour": (0.2, 0.14, 0.09), "front": 0.055, "nape": 0.08, "density": 0.9, "stubble": 0.55},
        },
        "density": {"skin": 3.0, "eyes": 2.0},
        "size": 2048,
        "far_ratio": 0.3,
        "tinted": False,
        "meshes": {"Body": None},
    },
    # More of the undead, each a different person in their own clothes. Zombie
    # kits keep their clothes' colours (no team tint); the shader adds grime
    # and blood per zombie. SoldierRig picks one per zombie.
    "zombie_b": {
        "spec": {
            "macro": {"gender": 1.0, "age": 0.75, "muscle": 0.5, "weight": 0.75, "height": 0.55,
                      "race": {"caucasian": 0.0, "african": 1.0, "asian": 0.0}},
            "skin": "skin_middleage_african_male",
            "parts": [
                {"asset": "overalls", "role": "uniform", "material": "civilian"},
                {"asset": "eyes", "role": "eyes", "type": "Eyes"},
            ],
            "keep_body": {"bones": ["DEF-head", "DEF-neck", "DEF-hand.", "DEF-f_", "DEF-thumb.", "DEF-forearm.", "DEF-upper_arm."],
                          "threshold": 0.35},
            "body_decimate": 0.32,
            # Short hair and stubble, painted on (paint_hair in blender_characters.py).
            "hair": {"colour": (0.035, 0.03, 0.028), "front": 0.065, "nape": 0.07, "density": 0.95, "stubble": 0.75},
        },
        "tinted": False,
        "density": {"skin": 2.5, "eyes": 2.0},
        "size": 1024,
        "far_ratio": 0.3,
        "meshes": {"Body": None},
    },
    "zombie_c": {
        "spec": {
            "macro": {"gender": 1.0, "age": 0.95, "muscle": 0.3, "weight": 0.45, "height": 0.5,
                      "race": {"caucasian": 1.0, "african": 0.0, "asian": 0.0}},
            "skin": "skin_old_caucasian_male",
            "parts": [
                {"asset": "jacket", "role": "uniform", "material": "civilian"},
                {"asset": "eyes", "role": "eyes", "type": "Eyes"},
            ],
            "keep_body": {"bones": ["DEF-head", "DEF-neck", "DEF-hand.", "DEF-f_", "DEF-thumb."], "threshold": 0.35},
            "body_decimate": 0.32,
            # Short hair and stubble, painted on (paint_hair in blender_characters.py).
            "hair": {"colour": (0.55, 0.53, 0.5), "front": 0.09, "nape": 0.06, "density": 0.6, "stubble": 0.45},
        },
        "tinted": False,
        "density": {"skin": 2.5, "eyes": 2.0},
        "size": 1024,
        "far_ratio": 0.3,
        "meshes": {"Body": None},
    },
    "zombie_d": {
        "spec": {
            "macro": {"gender": 1.0, "age": 0.4, "muscle": 0.55, "weight": 0.4, "height": 0.5,
                      "race": {"caucasian": 0.0, "african": 0.0, "asian": 1.0}},
            "skin": "skin_young_asian_male",
            "parts": [
                {"asset": "tshirt", "role": "uniform", "material": "civilian"},
                {"asset": "eyes", "role": "eyes", "type": "Eyes"},
            ],
            "keep_body": {"bones": ["DEF-head", "DEF-neck", "DEF-hand.", "DEF-f_", "DEF-thumb.", "DEF-forearm.", "DEF-upper_arm."],
                          "threshold": 0.35},
            "body_decimate": 0.32,
            # Short hair and stubble, painted on (paint_hair in blender_characters.py).
            "hair": {"colour": (0.03, 0.028, 0.03), "front": 0.05, "nape": 0.085, "density": 1.0, "stubble": 0.15},
        },
        "tinted": False,
        "density": {"skin": 2.5, "eyes": 2.0},
        "size": 1024,
        "far_ratio": 0.3,
        "meshes": {"Body": None},
    },
    # A woman in office clothes: a blouse, a pencil skirt, and ankle boots,
    # with her calves bare.
    "zombie_e": {
        "spec": {
            "macro": {"gender": 0.0, "age": 0.6, "muscle": 0.4, "weight": 0.5, "height": 0.45,
                      "race": {"caucasian": 1.0, "african": 0.0, "asian": 0.0}},
            "skin": "skin_middleage_caucasian_female",
            "parts": [
                {"asset": "blouse_skirt", "role": "uniform", "material": "civilian"},
                {"asset": "ankle_boots", "role": "boots", "material": "civilian", "decimate": 0.35},
                {"asset": "eyes", "role": "eyes", "type": "Eyes"},
            ],
            "keep_body": {"bones": ["DEF-head", "DEF-neck", "DEF-hand.", "DEF-f_", "DEF-thumb.", "DEF-shin."],
                          "threshold": 0.35},
            "body_decimate": 0.32,
            # Short hair and stubble, painted on (paint_hair in blender_characters.py).
            "hair": {"colour": (0.26, 0.13, 0.07), "front": 0.05, "nape": 0.12, "density": 1.0, "stubble": 0.0},
        },
        "tinted": False,
        "density": {"skin": 2.5, "eyes": 2.0},
        "size": 1024,
        "far_ratio": 0.3,
        "meshes": {"Body": None},
    },
}

def bake_kit(name: str, kit: dict, cache: Path, ual1: Path, assets: dict[str, str]) -> Path:
    work = cache / "work" / name
    work.mkdir(parents=True, exist_ok=True)
    config = {
        "ual1": str(ual1),
        "out": str(work),
        "assets": assets,
        "spec": kit["spec"],
        "roles": ROLES,
        "materials": MATERIALS,
        "density": kit.get("density", {}),
        "size": kit["size"],
        "ao_samples": 48,
        "ao_rounds": kit.get("ao_rounds"),
        "poses": kit.get("poses", []),
        "far_ratio": kit.get("far_ratio"),
    }
    (work / "config.json").write_text(json.dumps(config, indent=1))
    subprocess.run(
        ["blender", "-b", "--python-exit-code", "1", "--python", str(ROOT / "tools" / "blender_characters.py"),
         "--", str(work / "config.json")],
        env=blender_env(cache),
        check=True,
    )
    return work


def load_parts(work: Path, name: str = "mesh.npz") -> list[dict]:
    data = np.load(work / name)
    parts = []
    for k in range(int(data["count"])):
        role, group, obj = (str(x) for x in data[f"p{k}_meta"])
        parts.append(
            {
                "name": obj,
                "role": role,
                "group": int(group),
                "positions": data[f"p{k}_positions"],
                "normals": data[f"p{k}_normals"],
                "tangents": data[f"p{k}_tangents"],
                "uvs": data[f"p{k}_uvs"],
                "weights": data[f"p{k}_weights"],
                "bones": [str(b) for b in data[f"p{k}_bones"]],
                "indices": data[f"p{k}_indices"],
            }
        )
    return parts


def write_kit_model(name: str, ual: Gltf, meshes: dict[str, list[dict]], path: Path) -> None:
    w = Writer()
    copy_rig(ual, w, material=f"character_{name}")
    joint_names = ual.joint_names()
    for mesh_name, parts in meshes.items():
        index, tris = write_character_mesh(w, mesh_name, parts, joint_names)
        if mesh_name == "Body" and "Mannequin" in [n["name"] for n in ual.json["nodes"]]:
            attach_mesh(w, ual, "Mannequin", index, "Rig")
            w.doc["nodes"][ual.node_index("Mannequin")]["name"] = "Body"
        else:
            attach_mesh(w, ual, mesh_name, index, "Rig")
        print(f"{name}/{mesh_name}: {tris} triangles, {sum(len(p['positions']) for p in parts)} vertices")
    # Unused nodes of the source (its mannequin) stay meshless.
    w.save(path)


def write_textures(name: str, work: Path, out_dir: Path, tinted: bool = True) -> None:
    """albedo (tinted roles reduced to detail centred on 0.5, unless the kit
    keeps its colours), normal, and ORM."""
    albedo = np.asarray(Image.open(work / "albedo.png").convert("RGB"), dtype=np.float32) / 255.0
    role_map = np.asarray(Image.open(work / "role.png").convert("L"), dtype=np.float32) / 255.0
    roles = np.clip((role_map * 16.0).astype(int), 0, 15)
    linear = srgb_to_linear(albedo)
    lum = linear @ np.array([0.2126, 0.7152, 0.0722], np.float32)
    for i, role in enumerate(ROLES):
        if not tinted or role not in TINTED:
            continue
        mask = roles == i
        if not mask.any():
            continue
        mean = float(np.median(lum[mask]))
        detail = np.clip(lum[mask] / max(mean, 1e-4) * 0.5, 0.0, 1.0)
        linear[mask] = detail[:, None]
    out_dir.mkdir(parents=True, exist_ok=True)
    save_rgb(linear_to_srgb(linear), out_dir / f"{name}_albedo.png")
    shutil.copy(work / "normal.png", out_dir / f"{name}_normal.png")
    ao = np.asarray(Image.open(work / "ao.png").convert("L"), dtype=np.float32) / 255.0
    rough = np.asarray(Image.open(work / "rough.png").convert("L"), dtype=np.float32) / 255.0
    metal = np.asarray(Image.open(work / "metal.png").convert("L"), dtype=np.float32) / 255.0
    save_rgb(np.dstack([ao, rough, metal]), out_dir / f"{name}_orm.png")


def srgb_to_linear(c: np.ndarray) -> np.ndarray:
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def linear_to_srgb(c: np.ndarray) -> np.ndarray:
    return np.where(c <= 0.0031308, c * 12.92, 1.055 * np.power(np.maximum(c, 0), 1 / 2.4) - 0.055)


def save_rgb(a: np.ndarray, path: Path) -> None:
    Image.fromarray((np.clip(a, 0, 1) * 255 + 0.5).astype(np.uint8), "RGB").save(path, optimize=True)
    print(f"wrote {path.relative_to(ROOT)}")




def build_animations(src: Gltf, names: list[str], path: Path) -> None:
    """The skeleton, a one-triangle stand-in mesh (so the importer builds the
    skeleton), and the listed animations."""
    w = Writer()
    copy_rig(src, w)
    root = src.json["skins"][0]["joints"][0]
    mesh_node = next(n["name"] for n in src.json["nodes"] if "mesh" in n)
    attach_mesh(w, src, mesh_node, write_stub_mesh(w, src.json["skins"][0]["joints"].index(root)), "Rig")
    copy_animations(src, w, names)
    w.save(path)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ual_src = Path.home() / ".cache" / "ual-src"
    parser.add_argument(
        "--ual1",
        type=Path,
        default=ual_src / "x" / "Animation Library[Standard]" / "Godot" / "AnimationLibrary_Godot_Standard.glb",
    )
    parser.add_argument(
        "--ual2",
        type=Path,
        default=ual_src / "y" / "Universal Animation Library 2 [Standard]" / "Unreal-Godot" / "UAL2_Standard.glb",
    )
    parser.add_argument("--cache", type=Path, default=Path.home() / ".cache" / "shooter-characters")
    parser.add_argument("--kits", default=",".join(KITS), help="comma-separated kits to build")
    parser.add_argument("--reuse", action="store_true", help="reassemble from the last bake, skipping Blender")
    parser.add_argument("--no-animations", action="store_true")
    args = parser.parse_args()
    ual1 = Gltf(args.ual1)
    src = fetch(args.cache)
    install_mpfb(args.cache, src)
    assets = asset_paths(src)
    for name in args.kits.split(","):
        kit = KITS[name]
        work = args.cache / "work" / name
        if not args.reuse:
            work = bake_kit(name, kit, args.cache, args.ual1, assets)
        parts = load_parts(work)
        out_dir = OUT / name
        meshes = {}
        for mesh_name, select in kit["meshes"].items():
            meshes[mesh_name] = parts if select is None else select(parts)
        if kit.get("far_ratio"):
            meshes["BodyFar"] = load_parts(work, "mesh_far.npz")
        write_kit_model(name, ual1, meshes, out_dir / f"{name}.glb")
        write_textures(name, work, out_dir, kit.get("tinted", True))
    if not args.no_animations:
        build_animations(ual1, UAL1_ANIMATIONS, OUT / "animations" / "ual1.glb")
        build_animations(Gltf(args.ual2), UAL2_ANIMATIONS, OUT / "animations" / "ual2.glb")


if __name__ == "__main__":
    main()
