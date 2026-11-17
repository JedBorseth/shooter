#!/usr/bin/env python3
"""Recover Nuketown's destructible definitions and animated-prop clips from T6 data.

    python3 tools/extract_nuketown_behavior.py [--fastfile FF] [--out JSON]

Writes `builds/nuketown/behavior.json` (an ignored artifact) with:

- every `destructibledef` in `mp_nuketown_2020.ff`: the pieces, their five
  stages, show tags, hide-bone masks, health/damage scales, break FX/sounds,
  notifies, spawned piece models and physics assets, with tag names resolved
  through the zone's script-string table;
- the map's destructible and animated-prop entity placements, the way the
  `.d3dbsp` `entityString` stores them;
- the physics assets those doors/wheels/mirrors use (`physconstraints` and
  `physpreset`: constraint type, attach points, axes, damp/power, mass,
  frictions);
- the eight `xanim` clips OAT dumped under `builds/nuketown/unlink/xanim`,
  decoded from OAT's compiled v19 form into per-bone keyframes (quaternions
  xyzw + translations in inches) with fps, loop flag, notetracks and delta
  root motion, plus the prop models whose bones each clip drives.

Everything is read out of the retail zone/MAP data by walking it with the
open-fastfile T6 reader; no value here is guessed. The only derived values are
the decoded hide-bone bitmaps (see `notes` in the output) and float rounding.

Requirements (machine-local, nothing is vendored):

- a checkout of https://github.com/eithan1231/open-fastfile with `scripts/cod`
  (the T6 zone walker and the OAT compiled-xanim decoder); pass `--cod-repo`
  or set `COD_FASTFILE_REPO`;
- the T6 fastfile (`--fastfile`) and OAT's unlink dump (`--unlink`) with the
  `xanim/` files and `model_export/` glTF skeletons.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import struct
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DEFAULT_FASTFILE = Path(os.environ.get("BO2_GAME_PATH", "builds/bo2")) / "zone/all/mp_nuketown_2020.ff"
DEFAULT_COD_REPO = REPO / "builds/nuketown/tools/open-fastfile"
DEFAULT_UNLINK = REPO / "builds/nuketown/unlink"
DEFAULT_GSC = REPO / "builds/nuketown/decompiled/t6/mp_nuketown_2020.gsc"
DEFAULT_OUT = REPO / "builds/nuketown/behavior.json"

DIGITS = 6  # floats are rounded to this many decimals in the output


def r(value):
    """Round a float for readable, stable JSON (raw precision is meaningless past this)."""
    return round(float(value), DIGITS)


def git_rev(path: Path) -> str | None:
    try:
        return subprocess.run(
            ["git", "-C", str(path), "rev-parse", "HEAD"], capture_output=True, text=True, check=True
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_cod(repo: Path):
    """The open-fastfile T6 reader and OAT xanim decoder."""
    if not (repo / "scripts/cod/zonecode/walker.py").is_file():
        sys.exit(
            f"error: {repo} is not an open-fastfile checkout with scripts/cod; "
            "pass --cod-repo or set COD_FASTFILE_REPO"
        )
    sys.path.insert(0, str(repo))
    from scripts.cod.xanim import decode  # noqa: PLC0415
    from scripts.cod.zonecode.walker import walk_fastfile  # noqa: PLC0415

    return walk_fastfile, decode


def text(walk, index) -> str | None:
    """A ScriptString id -> its string."""
    if not index:
        return None
    return walk.script_string(int(index))


def asset_name(asset) -> str | None:
    return None if asset is None else asset.name


def vec3(record) -> list[float]:
    """A vec3_t read from the zone: its `v[3]` is a leaf, so it comes back as bytes."""
    value = record["v"]
    if isinstance(value, (bytes, bytearray)):
        return [r(x) for x in struct.unpack("<3f", bytes(value))]
    return [r(x) for x in value]


def bone_list(walk, xmodel) -> list[dict]:
    """A model's bones in skeleton order: index, script-string id, name."""
    model = xmodel.record
    count = int(model["numBones"])
    bones = []
    for i in range(count):
        bone_id = int(model["boneNames"][i])
        bones.append({"index": i, "id": bone_id, "name": text(walk, bone_id)})
    return bones


def model_info(walk, asset, model_export: Path) -> dict:
    """What a caller needs to skin a model: bone order and the dumped glTF levels."""
    bones = bone_list(walk, asset)
    lod_files = sorted(model_export.glob(f"{asset.name}_lod*.gltf")) if model_export.is_dir() else []
    return {
        "name": asset.name,
        "num_bones": int(asset.record["numBones"]),
        "num_root_bones": int(asset.record["numRootBones"]),
        "bones": bones,
        "gltf_lods": [str(p.relative_to(model_export.parent)) for p in lod_files],
    }


def hide_bones(piece, bone_names: list[str]) -> dict:
    """`int hideBones[5]`: a 160-bit bone mask, most significant bit first.

    Bit 31 of word 0 is bone 0. The order is inferred from the retail data and
    checked against pieces whose names say which bones they own (car doors,
    wheels, mirrors): with it, every mask in every definition lands inside its
    model's bone count, which normal little-endian bit order does not.
    """
    raw = piece["hideBones"]
    data = bytes(raw) if isinstance(raw, (bytes, bytearray)) else b"".join(struct.pack("<i", int(v)) for v in raw)
    words = list(struct.unpack("<5I", data))
    indices = [word * 32 + (31 - bit) for word, value in enumerate(words) for bit in range(32) if value >> bit & 1]
    return {
        "words": words,
        "bit_order": "msb_first: bit 31 of word 0 is bone 0",
        "indices": indices,
        "names": [bone_names[i] if i < len(bone_names) else None for i in indices],
        "out_of_range": [i for i in indices if i >= len(bone_names)],
    }


def stage_info(walk, stage, bone_ids: dict[int, tuple[int, str]]) -> dict:
    show = int(stage["showBone"])
    spawn = [stage[f"spawnModel[{i}]"] for i in range(3)]
    spawn_names = [asset_name(a) for a in spawn]
    effect = stage["breakEffect"]
    preset = stage["physPreset"]
    located = bone_ids.get(show) if show else None
    entry = {
        "index": None,
        "active": bool(
            show
            or stage["breakHealth"]
            or stage["maxTime"] != -1.0
            or stage["flags"]
            or effect
            or stage["breakSound"]
            or stage["breakNotify"]
            or stage["loopSound"]
            or any(spawn_names)
            or preset
        ),
        "show_bone": {
            "id": show or None,
            "name": text(walk, show) if show else None,
            "model_bone_index": located[0] if located else None,
            "found_in": located[1] if located else None,
        },
        "break_health_fraction": r(stage["breakHealth"]),
        "max_time_seconds": r(stage["maxTime"]),
        "flags": int(stage["flags"]),
        "flags_hex": f"0x{int(stage['flags']):x}",
        "break_effect": asset_name(effect),
        "break_sound": stage["breakSound"],
        "break_notify": stage["breakNotify"],
        "loop_sound": stage["loopSound"],
        "spawn_models": [n for n in spawn_names if n],
        "phys_preset": asset_name(preset),
    }
    return entry


def piece_info(walk, piece, bone_ids, bone_names) -> dict:
    constraints = piece["physConstraints"]
    stages = []
    for i in range(5):
        stage = stage_info(walk, piece[f"stages[{i}]"], bone_ids)
        stage["index"] = i
        stages.append(stage)
    hide = hide_bones(piece, bone_names)
    shown = {s["show_bone"]["model_bone_index"] for s in stages if s["show_bone"]["model_bone_index"] is not None}
    return {
        "parent_piece": int(piece["parentPiece"]),
        "parent_damage_fraction": r(piece["parentDamagePercent"]),
        "health": int(piece["health"]),
        "damage_scales": {
            "bullet": r(piece["bulletDamageScale"]),
            "explosive": r(piece["explosiveDamageScale"]),
            "melee": r(piece["meleeDamageScale"]),
            "impact": r(piece["impactDamageScale"]),
            "entity_transfer": r(piece["entityDamageTransfer"]),
        },
        "damage_sound": piece["damageSound"],
        "burn_effect": asset_name(piece["burnEffect"]),
        "burn_sound": piece["burnSound"],
        "enable_label": int(piece["enableLabel"]),
        "phys_constraints": asset_name(constraints),
        # The retail data lists every tag a piece drives, clean and destroyed; the
        # active stage's show tag is the one left visible. Every show tag is in the mask.
        "hide_bones": {**hide, "includes_all_show_bones": shown <= set(hide["indices"])},
        "stages": stages,
    }


def destructible_json(walk, asset, model_export: Path) -> dict:
    record = asset.record
    model_asset = record["model"]
    model = model_info(walk, model_asset, model_export) if model_asset else None
    bone_names = [b["name"] for b in model["bones"]] if model else []
    # showBone is a script-string id, not an index: find its position in the model.
    bone_ids = {b["id"]: (b["index"], model_asset.name) for b in model["bones"]} if model else {}
    # Spawned pieces are models of their own with a single tag; resolve against them too.
    pieces = record["pieces"]
    spawn_assets = {}
    for i in range(int(record["numPieces"])):
        for s in range(5):
            for slot in range(3):
                spawn = pieces[i][f"stages[{s}].spawnModel[{slot}]"]
                if spawn is not None:
                    spawn_assets[spawn.name] = spawn
    for name, spawn in spawn_assets.items():
        for bone in bone_list(walk, spawn):
            bone_ids.setdefault(bone["id"], (bone["index"], name))
    spawn_models = {n: model_info(walk, a, model_export) for n, a in sorted(spawn_assets.items())}
    return {
        "name": asset.name,
        "model": model["name"] if model else None,
        "pristine_model": asset_name(record["pristineModel"]),
        "client_only": int(record["clientOnly"]),
        "num_pieces": int(record["numPieces"]),
        "pieces": [piece_info(walk, pieces[i], bone_ids, bone_names) for i in range(int(record["numPieces"]))],
        "model_info": model,
        "spawn_models": spawn_models,
    }


def constraint_json(walk, asset, enums: dict[str, int]) -> dict:
    record = asset.record
    constraint_types = {v: k for k, v in enums.items() if k.startswith("CONSTRAINT_")}
    attach_points = {v: k for k, v in enums.items() if k.startswith("ATTACH_POINT_")}
    entries = []
    for i in range(int(record["count"])):
        c = record[f"data[{i}]"]
        entries.append(
            {
                "targetname": text(walk, c["targetname"]),
                "type": constraint_types.get(int(c["type"]), int(c["type"])),
                "attach_point_type1": attach_points.get(int(c["attach_point_type1"]), int(c["attach_point_type1"])),
                "target_index1": int(c["target_index1"]),
                "target_ent1": int(c["target_ent1"]),
                "target_bone1": c["target_bone1"],
                "attach_point_type2": attach_points.get(int(c["attach_point_type2"]), int(c["attach_point_type2"])),
                "target_index2": int(c["target_index2"]),
                "target_ent2": int(c["target_ent2"]),
                "target_bone2": c["target_bone2"],
                "offset": vec3(c["offset"]),
                "pos": vec3(c["pos"]),
                "pos2": vec3(c["pos2"]),
                "dir": vec3(c["dir"]),
                "flags": int(c["flags"]),
                "timeout": int(c["timeout"]),
                "min_health": int(c["min_health"]),
                "max_health": int(c["max_health"]),
                "distance": r(c["distance"]),
                "damp": r(c["damp"]),
                "power": r(c["power"]),
                "scale": vec3(c["scale"]),
                "spin_scale": r(c["spin_scale"]),
                "min_angle": r(c["minAngle"]),
                "max_angle": r(c["maxAngle"]),
                "material": asset_name(c["material"]),
            }
        )
    return {"name": asset.name, "count": int(record["count"]), "constraints": entries}


def phys_preset_json(asset) -> dict:
    record = asset.record
    return {
        "name": asset.name,
        "flags": int(record["flags"]),
        "mass": r(record["mass"]),
        "bounce": r(record["bounce"]),
        "friction": r(record["friction"]),
        "bullet_force_scale": r(record["bulletForceScale"]),
        "explosive_force_scale": r(record["explosiveForceScale"]),
        "pieces_spread_fraction": r(record["piecesSpreadFraction"]),
        "pieces_upward_velocity": r(record["piecesUpwardVelocity"]),
        "can_float": int(record["canFloat"]),
        "gravity_scale": r(record["gravityScale"]),
        "center_of_mass_offset": vec3(record["centerOfMassOffset"]),
    }


def parse_entities(blob: str) -> list[dict]:
    """The `.d3dbsp` entityString: one `"key" "value"` per line between braces."""
    entities, current = [], None
    for line in blob.splitlines():
        line = line.strip()
        if line == "{":
            current = {}
        elif line == "}":
            if current is not None:
                entities.append(current)
            current = None
        elif current is not None and line.startswith('"'):
            parts = line.split('"')
            if len(parts) >= 4:
                current[parts[1]] = parts[3]
    return entities


def vector(value: str | None) -> list[float] | None:
    if not value:
        return None
    try:
        parts = [float(v) for v in value.split()]
    except ValueError:
        return None
    return parts if len(parts) == 3 else None


def placement(entities: list[dict]) -> dict:
    return {
        "classname": entities.get("classname"),
        "targetname": entities.get("targetname"),
        "target": entities.get("target"),
        "destructibledef": entities.get("destructibledef"),
        "model": entities.get("model"),
        "origin": vector(entities.get("origin")),
        "angles": vector(entities.get("angles")),
        "spawnflags": entities.get("spawnflags"),
        "script_noteworthy": entities.get("script_noteworthy"),
        "script_linkname": entities.get("script_linkname"),
    }


def animation_json(asset_file: Path, decode, models: dict[str, dict]) -> dict:
    raw = asset_file.read_bytes()
    anim = decode(raw, asset_file.name)
    clip_bones = [bone.name for bone in anim.bones if bone.name != "tag_origin"]

    def track(t):
        if t is None:
            return None
        return {"indices": list(t.indices), "values": [[r(v) for v in key] for key in t.values]}

    def bone(json_bone):
        return {
            "name": json_bone.name,
            "rotation": track(json_bone.rotation),
            "translation": track(json_bone.translation),
        }

    candidates = []
    if clip_bones:
        wanted = set(clip_bones)
        for name, model in models.items():
            names = {b["name"] for b in model["bones"]} - {"tag_origin"}
            if wanted <= names:
                candidates.append(
                    {
                        "model": name,
                        "match": "exact" if wanted == names else "subset",
                        "extra_bones": sorted(names - wanted),
                        "gltf_lods": model["gltf_lods"],
                    }
                )
        candidates.sort(key=lambda c: (c["match"] != "exact", -len(c["extra_bones"]), c["model"]))
    tracks_used = [
        t
        for json_bone in anim.bones
        for t in (json_bone.rotation, json_bone.translation)
        if t is not None
    ]
    tracks_used += [t for t in (anim.delta_rotation, anim.delta_translation) if t is not None]
    bone_motion = any(len(t.indices) > 1 for t in tracks_used)
    if anim.has_root_motion and bone_motion:
        motion = "root+bones"
    elif anim.has_root_motion:
        motion = "root"
    else:
        motion = "bones"
    return {
        "name": asset_file.name,
        "source": f"OAT compiled xanim v19: unlink/xanim/{asset_file.name}",
        "bytes": len(raw),
        "frames": anim.frames,
        "fps": anim.fps,
        "duration_seconds": r(anim.duration),
        "last_key_frame": max((max(t.indices) for t in tracks_used if t.indices), default=None),
        "looped": anim.looped,
        "asset_type": anim.asset_type,
        "motion": motion,
        "has_root_motion": anim.has_root_motion,
        "delta_rotation": track(anim.delta_rotation),
        "delta_translation": track(anim.delta_translation),
        "bones": [bone(b) for b in anim.bones],
        "notetracks": [{"name": n, "frame": f, "time_seconds": r(f / anim.fps) if anim.fps else None} for n, f in anim.notetracks],
        "model_candidates": candidates,
        "model": candidates[0]["model"] if candidates else None,
        "placements": [],
    }


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--fastfile", type=Path, default=DEFAULT_FASTFILE, help=f"T6 fastfile (default {DEFAULT_FASTFILE})")
    parser.add_argument(
        "--cod-repo",
        type=Path,
        default=Path(os.environ.get("COD_FASTFILE_REPO", DEFAULT_COD_REPO)),
        help="open-fastfile checkout with scripts/cod (or set COD_FASTFILE_REPO)",
    )
    parser.add_argument("--unlink", type=Path, default=DEFAULT_UNLINK, help=f"OAT unlink dump (default {DEFAULT_UNLINK})")
    parser.add_argument("--gsc", type=Path, default=DEFAULT_GSC, help="decompiled map GSC, for script bindings")
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT, help=f"output JSON (default {DEFAULT_OUT})")
    args = parser.parse_args(argv)

    for needed in (args.fastfile, args.unlink / "xanim"):
        if not needed.exists():
            sys.exit(f"error: {needed} does not exist")

    walk_fastfile, decode = load_cod(args.cod_repo)
    walk = walk_fastfile(str(args.fastfile))
    problems = walk.check()
    if problems:
        sys.exit("error: the zone walk is not exact: " + "; ".join(problems))

    model_export = args.unlink / "model_export"
    models = {asset.name: model_info(walk, asset, model_export) for asset in walk.by_type("xmodel")}
    enums = {name: value for name, value in _load_enums(args.cod_repo).items()}

    destructibles = [destructible_json(walk, asset, model_export) for asset in walk.by_type("destructibledef")]
    by_name = {d["name"]: d for d in destructibles}

    # Physics assets reachable from the pieces.
    constraint_names = sorted({p["phys_constraints"] for d in destructibles for p in d["pieces"] if p["phys_constraints"]})
    preset_names = sorted(
        {
            stage["phys_preset"]
            for d in destructibles
            for p in d["pieces"]
            for stage in p["stages"]
            if stage["phys_preset"]
        }
    )
    constraints = {
        asset.name: constraint_json(walk, asset, enums)
        for asset in walk.by_type("physconstraints")
        if asset.name in constraint_names
    }
    presets = {asset.name: phys_preset_json(asset) for asset in walk.by_type("physpreset") if asset.name in preset_names}

    # Animations, and the models whose bones they drive (T6 stores no model->anim link).
    anim_dir = args.unlink / "xanim"
    animations = [animation_json(path, decode, models) for path in sorted(anim_dir.iterdir()) if path.is_file()]
    zone_anims = {asset.name: asset.record for asset in walk.by_type("xanim")}
    for anim in animations:
        record = zone_anims.get(anim["name"])
        anim["zone_asset"] = None
        if record is not None:
            frames, fps = int(record["numframes"]), float(record["framerate"])
            anim["zone_asset"] = {
                "numframes": frames,
                "framerate": fps,
                "matches_dump": frames == anim["frames"] and abs(fps - anim["fps"]) < 1e-3,
            }

    # Script bindings: `destructible_car_anims["car1"] = %fxanim_...`.
    bindings, callbacks = {}, {}
    if args.gsc.is_file():
        for line in args.gsc.read_text(errors="replace").splitlines():
            match = re.search(r'(\w+)\["([^"]+)"\]\s*=\s*%(\w+)', line)
            if match:
                bindings[match.group(3)] = {"table": match.group(1), "key": match.group(2)}
            callback = re.search(r'level\.destructible_callbacks\["([^"]+)"\]\s*=\s*(::\w+)', line)
            if callback:
                callbacks[callback.group(1)] = callback.group(2)
    for anim in animations:
        anim["script_binding"] = bindings.get(anim["name"])
        stem = anim["name"].removeprefix("fxanim_mp_nuked2025_").removesuffix("_anim")
        normalised = re.sub(r"[^a-z0-9]", "", stem.lower())
        anim["destructible"] = next((d["name"] for d in destructibles if normalised and normalised in re.sub(r"[^a-z0-9]", "", d["name"].lower())), None)
        if anim["destructible"]:
            target = by_name[anim["destructible"]]["model"]
            if target:
                anim["model_candidates"] = [
                    {
                        "model": target,
                        "match": "destructible model",
                        "extra_bones": [],
                        "gltf_lods": models.get(target, {}).get("gltf_lods", []),
                    }
                ]
                anim["model"] = target

    # Placements from the map's entity lump.
    mapents = walk.by_type("mapents")
    entities = []
    if mapents:
        entity_string = mapents[0].record["entityString"]
        blob = bytes(entity_string) if entity_string.items is None else bytes(entity_string.items)
        entities = parse_entities(blob.decode("latin-1"))
    destructible_placements = [placement(e) for e in entities if e.get("destructibledef")]
    animated_models = {name for name in models if name.startswith("fxanim_mp_nuked2025")}
    animated_placements = [placement(e) for e in entities if e.get("model") in animated_models]

    for anim in animations:
        driven = {c["model"] for c in anim["model_candidates"]}
        anim["placements"] = [p for p in animated_placements if p["model"] in driven]
        if not anim["placements"] and anim["destructible"]:
            anim["placements"] = [p for p in destructible_placements if p["destructibledef"] == anim["destructible"]]

    # Validation: the things that would mean a misread struct.
    hide_checks = [
        {
            "definition": d["name"],
            "piece": i,
            "indices": p["hide_bones"]["indices"],
            "out_of_range": p["hide_bones"]["out_of_range"],
            "includes_all_show_bones": p["hide_bones"]["includes_all_show_bones"],
        }
        for d in destructibles
        for i, p in enumerate(d["pieces"])
    ]
    show_unresolved = [
        {"definition": d["name"], "piece": i, "stage": s["index"], "show_bone": s["show_bone"]["id"]}
        for d in destructibles
        for i, p in enumerate(d["pieces"])
        for s in p["stages"]
        if s["show_bone"]["id"] and s["show_bone"]["model_bone_index"] is None
    ]

    output = {
        "schema": "nuketown-behavior/v1",
        "generated_utc": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "source": {
            "fastfile": str(args.fastfile),
            "fastfile_bytes": args.fastfile.stat().st_size,
            "fastfile_sha256": sha256(args.fastfile),
            "zone_walk": {
                "game": walk.game,
                "xassets": len(walk.entries),
                "assets_loaded": len(walk.assets),
                "problems": problems,
            },
            "open_fastfile": {"repo": str(args.cod_repo), "commit": git_rev(args.cod_repo)},
            "oat_unlink": {"dir": str(args.unlink), "xanim_files": len(animations)},
            "floats_rounded_to_decimals": DIGITS,
        },
        "destructibles": destructibles,
        "physics": {"physconstraints": constraints, "physpresets": presets},
        "animations": animations,
        "placements": {
            "destructibles": destructible_placements,
            "animated_props": animated_placements,
            "entity_count": len(entities),
        },
        "script": {
            "gsc": str(args.gsc) if args.gsc.is_file() else None,
            "anim_bindings": bindings,
            "destructible_callbacks": callbacks,
        },
        "validation": {
            "hide_bone_masks_all_in_range": all(not c["out_of_range"] for c in hide_checks),
            "every_piece_mask_includes_its_show_bones": all(c["includes_all_show_bones"] for c in hide_checks),
            "hide_bone_masks": hide_checks,
            "show_bones_unresolved": show_unresolved,
        },
        "notes": [
            "hideBones is int[5] = a 160-bit bone mask, most significant bit first (bit 31 of word 0 is bone 0). "
            "The bit order is inferred from retail data: with it every mask in all nine definitions lands inside its "
            "model's bone count, and each piece's mask holds exactly the tags that piece drives (its clean tag and its "
            "destroyed tag; validated in `validation.every_piece_mask_includes_its_show_bones`). The active stage's "
            "showBone is the visible one, so read hideBones as the piece's owned tag set, not as a second hide list. "
            "The engine's own implementation is not documented.",
            "Destructible stage flags (0x2, 0x4, 0x6, 0x8 observed) have no published T6 meaning; raw values only.",
            "break_health_fraction is a fraction of the piece's health per the APE conventions; a negative value "
            "(mannequin pieces: -0.01) reads as never breaking by damage. max_time_seconds -1 means unlimited.",
            "showBone is a script-string id, not a bone index: show_bone.model_bone_index is its position in the model.",
            "The six mannequin definitions have one 4-tag model (body/head/arm_left/arm_right). Head and arms are "
            "separate spawn models, so their meshes appear only after the stage breaks; the body tag stays.",
            "XAnim rotations are local bone rotations replacing the bind pose (quaternions xyzw, game units); "
            "translations are offsets added to the bind pose's local translation, in inches. car01/car02/"
            "displayGlass clips have no bone tracks: they are pure delta root motion (T6 compiled xanim v19), and "
            "T6's delta rotation is a yaw about local Z. Every clip's frame count/fps is cross-checked against the "
            "same xanim asset inside the zone (`zone_asset.matches_dump`).",
            "asset_type 7 (car01, car02, sprinkler) is root-motion only in this zone: every bone track is constant "
            "and the delta track carries the motion. car01/car02 translate the whole car hundreds of inches (the "
            "Nuketown nuke launch); the sprinkler only spins. asset_type 2 clips animate their bones. Delta "
            "translation is in x-forward, y-left, z-up inches relative to the prop's start pose.",
            "T6 clips key frames 0..frames inclusive: `last_key_frame` equals `frames` on all eight clips and the "
            "final key usually repeats the previous one. indices are stored as-is (frame numbers at `fps`), so "
            "sampling at `duration_seconds` lands on that final key.",
            "The display-glass clip holds frames 0-8 with constant, non-zero chunk offsets that assemble the "
            "pristine pane, then jumps at frame 9 (the shatter) and settles near the bind pose: the model's bind "
            "pose is the scattered layout. Play it from frame 0 when the display case is intact.",
            "Model->clip association is by bone-set match plus the decompiled GSC's destructible_car_anims bindings; "
            "T6 zones hold no direct xmodel->xanim reference. fxanim_mp_nuked2025_hose_anim only matches "
            "fxanim_mp_nuked2025_carwash_hoses_mod as a 2-of-3-joint subset; no distinct two-hose model is in this zone.",
            "Placements are BO2 entity coordinates (inches, Z up) copied verbatim from the .d3dbsp entity string; "
            "no axis conversion is applied. The entity's `model` is the Radiant/clean model; the definition's "
            "model_info.model is the all-states model the engine uses in game.",
            "Entity damage transfer, FX and sound references are assets/aliases by name; the FX graphs and sound "
            "banks themselves are not decoded here.",
        ],
    }

    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(output, indent=1) + "\n")
    print(f"wrote {args.out} ({args.out.stat().st_size:,} bytes)")
    print(
        f"  {len(destructibles)} destructibles, {sum(len(d['pieces']) for d in destructibles)} pieces, "
        f"{len(animations)} clips, {len(destructible_placements)} destructible placements, "
        f"{len(animated_placements)} animated prop placements"
    )
    print(
        f"  validation: hide masks in range={output['validation']['hide_bone_masks_all_in_range']}, "
        f"masks cover show tags={output['validation']['every_piece_mask_includes_its_show_bones']}, "
        f"unresolved show bones={len(show_unresolved)}"
    )
    if show_unresolved:
        print("  unresolved:", show_unresolved)
    return 0


def _load_enums(cod_repo: Path) -> dict[str, int]:
    data = json.loads((cod_repo / "scripts/cod/data/t6/zonecode.json").read_text())
    return {name: value for name, value in data["enums"].items() if isinstance(value, int)}


if __name__ == "__main__":
    sys.exit(main())
