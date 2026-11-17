#!/usr/bin/env python3
"""Build the compact runtime behavior data for Nuketown's animated props.

    python3 tools/prepare_nuketown_behavior.py [builds/nuketown] \
        [godot/assets/maps/nuketown/props/behavior.json]

Inputs
------
* ``builds/nuketown/behavior.json`` -- the retail-zone extraction written by
  ``tools/extract_nuketown_behavior.py``.  It holds the nine destructible
  definitions and the eight decoded OAT compiled xanims, including each clip's
  candidate models.
* ``builds/nuketown/unlink/model_export/<model>_lod0.gltf`` -- OAT's model
  dumps.  Their skins supply the bone hierarchy, each joint's bind local
  transform and its inverse-bind matrix.  OAT writes positions and joint
  translations in y-up inches and puts the source Z-up -> Y-up axis change in
  the root joint's rotation; the glTF's inverse-bind matrices are the exact
  inverse of the resulting bind globals.

Output
------
``godot/assets/maps/nuketown/props/behavior.json`` with:

* ``destructibles`` -- the extraction's nine definitions, preserved verbatim.
* ``animations`` -- one entry per clip: frames/fps, authored loop flag, bone
  tracks, root motion, note tracks, candidate models, and the display-glass
  visibility hand-off.
* ``models`` -- one skeletal bind table per candidate model: bone names and
  parents, bind local translation/rotation, and the inverse-bind transform,
  all converted to y-up metres for direct use by a Godot ``Node3D``.
* ``clips_by_model`` -- the reverse index the runtime uses to answer
  "which clips can this model play?".

Conversions
-----------
Translations are multiplied by 0.0254 (inches -> metres).  Root-motion
translations are source ``(x, y, z)`` = forward/left/up inches, so they become
``(x, z, -y) * 0.0254`` in OAT's y-up/model space.  A source rotation is
conjugated by the same axis change: ``(x, y, z, w) -> (x, z, -y, w)``.
Every relative transform is normalized and rounded for stable output.

Source of the animation conventions
-----------------------------------
The replacement/additive rules implemented by the Godot helper follow the
documented Black Ops II xanim behaviour in the ``open-fastfile`` reference
(``scripts/cod/animations.py`` and ``docs/cod/reference.md``, checkout recorded
under ``source.open_fastfile`` in the extraction).  The Python and GDScript
here are written for this project; no source code from that repository is
copied.  The referenced checkout has no licence file in the local copy, so it
is used only as a format reference and is not redistributed.
"""

from __future__ import annotations

import argparse
import base64
import json
import math
import struct
from pathlib import Path
from typing import Any

UNIT = 0.0254
DEFAULT_SOURCE = Path('builds/nuketown')
DEFAULT_OUTPUT = Path('godot/assets/maps/nuketown/props/behavior.json')
SCHEMA = 'nuketown-prop-behavior/v1'

# The display-glass clip's chunk offsets hold the assembled pane from frame 0
# through frame 8, then jump at frame 9 when the glass shatters.  The tool finds
# that edge from the decoded tracks rather than hard-coding it; this is the
# fallback if a future clip has no obviously changing chunk key.
DISPLAY_SHATTER_FALLBACK = 9


class Gltf:
    """The small part of glTF needed here: nodes, skins and accessor reads."""

    def __init__(self, path: Path) -> None:
        self.path = path
        self.document = json.loads(path.read_text())
        self.buffer = self._read_buffer()
        self.nodes = self.document.get('nodes', [])
        self.parent = {}
        for index, node in enumerate(self.nodes):
            for child in node.get('children', []):
                self.parent[child] = index

    def _read_buffer(self) -> bytes:
        buffers = self.document.get('buffers', [])
        if not buffers:
            return b''
        uri = buffers[0].get('uri', '')
        if uri.startswith('data:'):
            return base64.b64decode(uri.split(',', 1)[1])
        if uri:
            return (self.path.parent / uri).read_bytes()
        return b''

    def accessor(self, index: int) -> list[tuple[float, ...]]:
        accessor = self.document['accessors'][index]
        view = self.document['bufferViews'][accessor['bufferView']]
        offset = int(view.get('byteOffset', 0)) + int(accessor.get('byteOffset', 0))
        components = {'SCALAR': 1, 'VEC2': 2, 'VEC3': 3, 'VEC4': 4, 'MAT4': 16}[accessor['type']]
        code = {5126: 'f', 5123: 'u2', 5125: 'u4', 5121: 'u1', 5122: 'i2', 5120: 'i1'}[
            accessor['componentType']
        ]
        size = struct.calcsize('<' + code)
        stride = int(view.get('byteStride', components * size))
        rows = []
        for key in range(int(accessor['count'])):
            values = struct.unpack_from('<' + code * components, self.buffer, offset + key * stride)
            rows.append(values)
        return rows


def quat_matrix(rotation: list[float]) -> list[list[float]]:
    x, y, z, w = (float(value) for value in rotation)
    return [
        [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w)],
        [2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w)],
        [2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y)],
    ]


def node_local(node: dict[str, Any]) -> tuple[list[list[float]], list[float]]:
    """A node's local basis (columns) and translation in source inches/y-up."""
    if 'matrix' in node:
        matrix = [float(value) for value in node['matrix']]
        columns = [[matrix[0], matrix[1], matrix[2]], [matrix[4], matrix[5], matrix[6]], [matrix[8], matrix[9], matrix[10]]]
        return columns, [matrix[12], matrix[13], matrix[14]]
    return quat_matrix(node.get('rotation', [0.0, 0.0, 0.0, 1.0])), [
        float(value) for value in node.get('translation', [0.0, 0.0, 0.0])
    ]


def matmul(a: list[list[float]], b: list[list[float]]) -> list[list[float]]:
    return [[sum(a[row][k] * b[k][col] for k in range(3)) for col in range(3)] for row in range(3)]


def matvec(a: list[list[float]], v: list[float]) -> list[float]:
    return [sum(a[row][k] * v[k] for k in range(3)) for row in range(3)]


def r(value: float, digits: int = 7) -> float:
    value = round(float(value), digits)
    return 0.0 if value == 0.0 else value


def round_vec(values: list[float], digits: int = 7) -> list[float]:
    return [r(value, digits) for value in values]


def normalize_quat(values: list[float]) -> list[float]:
    length = math.sqrt(sum(float(value) ** 2 for value in values))
    if length <= 0.0:
        return [0.0, 0.0, 0.0, 1.0]
    return [float(value) / length for value in values]


def inverse_bind_rows(rows: list[tuple[float, ...]]) -> list[list[float]]:
    """glTF MAT4 column-major -> Godot Transform3D column vectors + origin, metres."""
    matrix = [float(value) for value in rows[0]]
    x = [matrix[0], matrix[1], matrix[2]]
    y = [matrix[4], matrix[5], matrix[6]]
    z = [matrix[8], matrix[9], matrix[10]]
    origin = [matrix[12] * UNIT, matrix[13] * UNIT, matrix[14] * UNIT]
    return [round_vec(x), round_vec(y), round_vec(z), round_vec(origin, 6)]


def model_bind_table(model: str, unlink: Path) -> dict[str, Any]:
    path = unlink / 'model_export' / f'{model}_lod0.gltf'
    if not path.is_file():
        raise FileNotFoundError(f'{path} is missing (needed by an animation candidate)')
    gltf = Gltf(path)
    skins = gltf.document.get('skins', [])
    if not skins:
        raise ValueError(f'{path} has no skin; cannot export a bind hierarchy')
    skin = skins[0]
    joints = [int(joint) for joint in skin.get('joints', [])]
    joint_set = set(joints)
    index_of = {joint: index for index, joint in enumerate(joints)}
    inverse_bind = gltf.accessor(skin['inverseBindMatrices'])
    if len(inverse_bind) != len(joints):
        raise ValueError(f'{path}: inverse bind count does not match joint count')

    bones = []
    for key, joint in enumerate(joints):
        node = gltf.nodes[joint]
        basis, translation = node_local(node)
        parent = gltf.parent.get(joint)
        parent_index = index_of.get(parent, -1)
        if parent_index > key:
            raise ValueError(f'{path}: joints are not stored parent-first')
        bones.append(
            {
                'name': node.get('name', f'joint_{key}'),
                'parent': parent_index,
                'translation': round_vec([component * UNIT for component in translation], 6),
                'rotation': round_vec(normalize_quat(node.get('rotation', [0.0, 0.0, 0.0, 1.0])), 7),
                'inverse_bind': inverse_bind_rows([inverse_bind[key]]),
            }
        )

    # A cheap self-check: the exported bind locals and inverse binds must meet
    # at identity in both rotation and translation.  It catches a stale OAT
    # dump or an accessor misread.
    bind_bases: list[list[list[float]]] = []
    bind_origins: list[list[float]] = []
    for key, bone in enumerate(bones):
        local_basis = quat_matrix(bone['rotation'])
        parent = bone['parent']
        if parent < 0:
            bind_bases.append(local_basis)
            bind_origins.append(list(bone['translation']))
        else:
            bind_bases.append(matmul(bind_bases[parent], local_basis))
            bind_origins.append(
                [a + b for a, b in zip(bind_origins[parent], matvec(bind_bases[parent], bone['translation']))]
            )
    worst = 0.0
    for key, bone in enumerate(bones):
        ib = bone['inverse_bind']
        inverse_basis = [[ib[0][0], ib[1][0], ib[2][0]], [ib[0][1], ib[1][1], ib[2][1]], [ib[0][2], ib[1][2], ib[2][2]]]
        identity = matmul(bind_bases[key], inverse_basis)
        worst = max(worst, max(abs(identity[row][col] - (1.0 if row == col else 0.0)) for row in range(3) for col in range(3)))
        offset = [a + b for a, b in zip(matvec(inverse_basis, bind_origins[key]), ib[3])]
        worst = max(worst, max(abs(value) for value in offset))
    if worst > 1e-4:
        raise ValueError(f'{path}: exported inverse binds disagree with the bind hierarchy (max {worst:.2e})')

    roots = [bone['name'] for index, bone in enumerate(bones) if bone['parent'] < 0]
    return {
        'source': f'unlink/model_export/{model}_lod0.gltf',
        'root': roots[0] if roots else '',
        'bones': bones,
    }


def track(track_data: dict[str, Any] | None, *, quat: bool, convert_translation: bool) -> dict[str, Any] | None:
    if not track_data:
        return None
    values = []
    for value in track_data.get('values', []):
        if quat:
            values.append(round_vec(normalize_quat([float(component) for component in value]), 7))
        elif convert_translation:
            values.append(round_vec([float(component) * UNIT for component in value], 6))
        else:
            values.append(round_vec([float(component) for component in value], 6))
    if not values:
        return None
    return {'indices': [int(index) for index in track_data.get('indices', [])], 'values': values}


def root_rotation_track(track_data: dict[str, Any] | None) -> dict[str, Any] | None:
    """Convert the clip delta rotation from source Z-up to OAT's y-up space."""
    if not track_data:
        return None
    values = []
    for x, y, z, w in track_data.get('values', []):
        values.append(round_vec(normalize_quat([float(x), float(z), -float(y), float(w)]), 7))
    return {'indices': [int(index) for index in track_data.get('indices', [])], 'values': values}


def root_translation_track(track_data: dict[str, Any] | None) -> dict[str, Any] | None:
    if not track_data:
        return None
    values = []
    for x, y, z in track_data.get('values', []):
        values.append(round_vec([float(x) * UNIT, float(z) * UNIT, -float(y) * UNIT], 6))
    return {'indices': [int(index) for index in track_data.get('indices', [])], 'values': values}


def first_changed_frame(anim: dict[str, Any], names: list[str]) -> int | None:
    """First keyed frame after 0 where any named bone track leaves its frame-0 value."""
    by_name = {bone.get('name'): bone for bone in anim.get('bones', [])}
    best = None
    for name in names:
        bone = by_name.get(name)
        if not bone:
            continue
        for path in ('rotation', 'translation'):
            track_data = bone.get(path)
            if not track_data or len(track_data.get('values', [])) < 2:
                continue
            first = track_data['values'][0]
            for frame, value in zip(track_data['indices'][1:], track_data['values'][1:]):
                if any(abs(float(a) - float(b)) > 1e-6 for a, b in zip(value, first)):
                    best = int(frame) if best is None else min(best, int(frame))
                    break
    return best


def display_visibility(anim: dict[str, Any]) -> dict[str, Any] | None:
    names = [str(bone.get('name', '')) for bone in anim.get('bones', [])]
    pristine = [name for name in names if 'prestine' in name or 'pristine' in name]
    chunks = [name for name in names if name.startswith('glass_chunk')]
    if not pristine or not chunks:
        return None
    shatter = first_changed_frame(anim, chunks)
    if shatter is None:
        shatter = DISPLAY_SHATTER_FALLBACK
    return {
        'shatter_frame': shatter,
        'shatter_seconds': r(shatter / float(anim['fps']), 6) if anim.get('fps') else 0.0,
        'before': {'show': pristine, 'hide': chunks},
        'after': {'show': chunks, 'hide': pristine},
    }


def clips_by_model(animations: list[dict[str, Any]]) -> dict[str, list[str]]:
    result: dict[str, list[str]] = {}
    for anim in animations:
        for model in anim.get('models', []):
            result.setdefault(model, [])
            if anim['name'] not in result[model]:
                result[model].append(anim['name'])
    return result


def prepare(source: Path, output: Path) -> dict[str, Any]:
    raw_path = source / 'behavior.json'
    raw = json.loads(raw_path.read_text())
    unlink = source / 'unlink'
    models: dict[str, Any] = {}
    animations = []

    for anim in raw.get('animations', []):
        candidates = [candidate['model'] for candidate in anim.get('model_candidates', [])]
        for model in candidates:
            if model not in models:
                models[model] = model_bind_table(model, unlink)
        tracks = {}
        for bone in anim.get('bones', []):
            name = bone.get('name')
            if not name:
                continue
            rotation = track(bone.get('rotation'), quat=True, convert_translation=False)
            translation = track(bone.get('translation'), quat=False, convert_translation=True)
            if rotation or translation:
                entry = {}
                if rotation:
                    entry['rotation'] = rotation
                if translation:
                    entry['translation'] = translation
                tracks[name] = entry

        root_rotation = root_rotation_track(anim.get('delta_rotation'))
        root_translation = root_translation_track(anim.get('delta_translation'))
        root_motion = None
        if root_rotation or root_translation:
            root_motion = {}
            if root_rotation:
                root_motion['rotation'] = root_rotation
            if root_translation:
                root_motion['translation'] = root_translation

        prepared = {
            'name': anim['name'],
            'frames': int(anim['frames']),
            'fps': float(anim['fps']),
            'duration_seconds': r(float(anim['duration_seconds']), 6),
            'loop': bool(anim.get('looped', False)),
            'asset_type': int(anim.get('asset_type', 0)),
            'motion': anim.get('motion', 'bones'),
            'models': candidates,
            'tracks': tracks,
            'notetracks': [
                {
                    'name': note.get('name'),
                    'frame': int(note.get('frame', 0)),
                    'time_seconds': r(float(note.get('time_seconds', 0.0)), 6),
                }
                for note in anim.get('notetracks', [])
            ],
        }
        if root_motion:
            prepared['root_motion'] = root_motion
        if anim.get('destructible'):
            prepared['destructible'] = anim['destructible']
        if anim.get('script_binding'):
            prepared['script_binding'] = anim['script_binding']
        visibility = display_visibility(anim)
        if visibility:
            prepared['visibility'] = visibility
        animations.append(prepared)

    open_fastfile = raw.get('source', {}).get('open_fastfile', {})
    result = {
        'schema': SCHEMA,
        'source': {
            'behavior_schema': raw.get('schema'),
            'fastfile_sha256': raw.get('source', {}).get('fastfile_sha256'),
            'open_fastfile_commit': open_fastfile.get('commit'),
            'floats_decimals': 6,
        },
        'units': {
            'position': 'metres',
            'translation': 'metres',
            'rotation': 'quaternion_xyzw',
            'space': 'model_y_up',
        },
        'destructibles': raw.get('destructibles', []),
        'animations': animations,
        'models': models,
        'clips_by_model': clips_by_model(animations),
    }

    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=1, ensure_ascii=False) + '\n')
    return result


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('source', type=Path, nargs='?', default=DEFAULT_SOURCE, help=f'raw build directory (default {DEFAULT_SOURCE})')
    parser.add_argument('output', type=Path, nargs='?', default=DEFAULT_OUTPUT, help=f'output JSON (default {DEFAULT_OUTPUT})')
    args = parser.parse_args(argv)
    result = prepare(args.source, args.output)
    bones = sum(len(model['bones']) for model in result['models'].values())
    print(f'wrote {args.output} ({args.output.stat().st_size:,} bytes)')
    print(
        f'  {len(result["destructibles"])} destructibles, {len(result["animations"])} clips, '
        f'{len(result["models"])} models, {bones} bone binds'
    )
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
