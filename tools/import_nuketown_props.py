#!/usr/bin/env python3
# /// script
# requires-python = ">=3.11"
# dependencies = ["pillow>=11", "numpy>=2"]
# ///
"""Import Nuketown's interactive models from the C2M/OAT exports.

The map importer (``tools/import_nuketown.py``) bakes the C2M static and
dynamic placements into the map scene.  This importer handles the separate
``script_model`` props that the map's behavior scripts own at runtime, plus the
intact/damaged debris models spawned when those props are destroyed.

Inputs live under ``builds/nuketown`` (raw game data, never committed):

* ``source/exported_maps/black_ops_2/mp_nuketown_2020/`` entity text and
  static/dynamic placement JSON from the original C2M export.
* ``unlink/model_export/<name>_lod0.gltf`` OAT model dumps, with embedded
  geometry in OAT's y-up inch space.
* ``unlink/images/*.dds`` OAT texture dumps.
* ``unlink/materials/**/*.json`` OAT material metadata.

Outputs under ``<output>`` (``godot/assets/maps/nuketown/props`` by default):

* ``models/<source_name>.gltf`` -- one flattened, skinless model.
* ``models/<source_name>.bin``  -- its geometry.
* ``models/textures/*.png``     -- DDS textures shared by every model.
* ``manifest.json``            -- placements and model map for the runtime.

Geometry rules
--------------
OAT already writes positions in ``(x, z, -y)`` y-up inches, matching the OBJ
importer's axis conversion.  Normals are kept verbatim and every POSITION value
is multiplied by 0.0254.  Skins, skeleton nodes, JOINTS_0 and WEIGHTS_0 are
removed.  Skinned surfaces are split into identity mesh nodes named after the
dominant source joint/bone for each triangle.  A bone with several surfaces (or
materials) emits ``bone`` then ``bone_1``, ``bone_2``, ... so a runtime can
detach all pieces that belong to a bone by prefix.  Models without a skin keep
all their triangle surfaces in a single mesh node named ``tag_origin``.

The OAT dumps contain intact and damaged geometry together (for example a car's
clean body plus ``*_d`` parts).  Nothing is filtered by material name here: the
runtime's destruction behavior decides which pieces to show.

Manifest schema (stable; extra top-level report fields are informational)
-------------------------------------------------------------------------
``entities``
    One item per authored ``script_model`` with exported geometry, in source
    order::

        {
          "id": int,                  # 0..N-1, then loose continues the ids
          "model": str,               # OAT source model name
          "position": [x, y, z],      # meters, (x, z, -y) * 0.0254
          "rotation_degrees": [pitch, yaw, roll],  # original CoD angles
          "targetname": str,          # "" when the entity has no targetname
          "destructibledef": str,     # "" when not destructible
          "source_index": int,        # 0-based block index in entities.txt
          "source_keys": {str: str}   # every authored key, including gametype
        }

``loose``
    One item per C2M dynamic placement (131 in the current export), with ids
    continuing after ``entities``::

        {
          "id": int,
          "model": str,
          "position": [x, y, z],      # meters
          "rotation_degrees": [x, y, z],  # original RotationDegrees X/Y/Z
          "scale": [x, z, y],         # ModelScale reordered for Godot
          "source_node": str          # <Name>_<2992 + dynamic index>
        }

``models``
    Map from source model name to its Godot resource path::

        {"<name>": "res://assets/maps/nuketown/props/models/<name>.gltf"}

Every output mesh node also carries its original OAT bone name in
``extras.source_bone``.  For skinned models the node name is the exact source
bone (plus a numeric suffix for a second surface); for an unskinned model the
node and ``extras.source_bone`` are both ``tag_origin``.

``static_duplicates``
    C2M static node names (``<Name>_<static index>``) whose model and position
    duplicate an exported entity.  The parent hides these baked copies.

``skipped``
    Entity records deliberately not emitted, with ``model``, ``targetname``,
    ``source_index``, ``reason`` and the original ``source_keys``.  Reasons are
    ``hidden_outro_copy`` (the cinematic car/glass copies), ``placeholder_model``
    (``tag_origin`` authoring helpers), ``missing_gltf_geometry``, and
    ``unsupported_gametype_dm_tdm`` (entities whose gametype keys are only the
    unimplemented deathmatch/team-deathmatch modes).

``unsupported_gametypes`` / ``notes``
    The exporter keeps every authored gametype key in ``source_keys`` for the
    runtime to filter.  ``dm`` and ``tdm``-only entities are not emitted and the
    unsupported gametypes are reported explicitly.  Objective-mode bomb/tablet
    source assets currently absent from the OAT dump remain in ``unresolved``;
    the CLI exits nonzero unless ``--allow-unresolved`` is passed deliberately.

``unresolved``
    Missing source files detected while converting.  The importer never writes
    a placeholder model or texture: unresolved resources are reported here and
    cause a nonzero exit by default.

Run with::

    python3 tools/import_nuketown_props.py [builds/nuketown] \\
        [godot/assets/maps/nuketown/props] [--allow-unresolved]
"""

from __future__ import annotations

import argparse
import base64
import fnmatch
import hashlib
import json
import math
from pathlib import Path
import re
import sys

import numpy as np
from PIL import Image, ImageOps

UNIT = 0.0254
MAX_TEXTURE_SIZE = 1024
DEFAULT_SOURCE = Path('builds/nuketown')
DEFAULT_OUTPUT = Path('godot/assets/maps/nuketown/props')
MAP_NAME = 'mp_nuketown_2020'
STATIC_MODEL_COUNT = 2992  # dynamic placements start after the static list

# Script-model copies reserved for the outro cinematic.  The models themselves
# are still exported when they match a debris pattern; only these placed copies
# are omitted from `entities`.
HIDDEN_OUTRO_TARGETS = (
    'nuke_animated_car01',
    'nuke_animated_car02',
    'nuke_display_glass_client',
)

# Authoring helpers that are not renderable prop geometry.
PLACEHOLDER_MODELS = ('tag_origin',)

# The runtime currently implements team deathmatch; dm/tdm-only entity copies
# are documented and skipped instead of producing an untracked extra prop.
UNSUPPORTED_GAMETYPES = ('dm', 'tdm')

# Debris/intact models that must exist even when no entity places them.
REQUIRED_MODEL_PATTERNS = (
    'dest_nt_nuked_*',
    'veh_t6_nuketown_2020_*',
    'veh_t6_engine*',
    'dest_electronic_outlet*',
    'fxanim_mp_nuked2025_*',
    'nt_nuketown_sign*',
    'nt_sign_population*',
    'nt_2020_sign_numbers_small',
    'nt_2020_wall_clock_*',
)

MANNEQUIN_VARIANTS = (
    'female_01',
    'female_02',
    'female_03',
    'female_05',
    'male_01',
    'male_03',
)

COMPONENT_DTYPES = {
    5120: np.dtype('i1'),
    5121: np.dtype('u1'),
    5122: np.dtype('<i2'),
    5123: np.dtype('<u2'),
    5125: np.dtype('<u4'),
    5126: np.dtype('<f4'),
}

TYPE_COMPONENTS = {
    'SCALAR': 1,
    'VEC2': 2,
    'VEC3': 3,
    'VEC4': 4,
    'MAT2': 4,
    'MAT3': 9,
    'MAT4': 16,
}

COMPONENT_FOR_KIND = {
    'SCALAR': 5126,
    'VEC2': 5126,
    'VEC3': 5126,
    'VEC4': 5126,
}

SKIPPED_ATTRIBUTES = frozenset(
    name for name in (
        'JOINTS_0', 'JOINTS_1', 'JOINTS_2',
        'WEIGHTS_0', 'WEIGHTS_1', 'WEIGHTS_2',
    )
)


def _clean(value):
    """Drop negative zero and tiny float noise from manifest output."""
    value = round(float(value), 6)
    return 0.0 if value == 0 else value


def convert_position(x, y, z):
    """Convert a C2M/OAT z-up inch position to Godot y-up meters."""
    return [_clean(x * UNIT), _clean(z * UNIT), _clean(-y * UNIT)]


def c2m_node_name(name, index):
    """Node name produced by tools/import_nuketown.py for placement `index`."""
    return f'{name}_{index}'


def parse_cod_angles(value):
    """Return original CoD angles as [pitch, yaw, roll] degrees."""
    if isinstance(value, dict):
        return [_clean(value.get('X', 0)), _clean(value.get('Y', 0)), _clean(value.get('Z', 0))]
    if isinstance(value, (list, tuple)):
        if len(value) != 3:
            raise ValueError(f'Expected three angle components: {value!r}')
        return [_clean(part) for part in value]
    parts = str(value).split()
    if len(parts) != 3:
        raise ValueError(f'Expected three CoD angles: {value!r}')
    return [_clean(part) for part in parts]


def parse_source_vector(value, label='vector'):
    parts = str(value).split()
    if len(parts) != 3:
        raise ValueError(f'Expected three {label} components: {value!r}')
    return [float(part) for part in parts]


def parse_entities(path):
    """Parse the C2M entity text into one dict per ``{...}`` block."""
    text = Path(path).read_text()
    return [dict(re.findall(r'"([^"]*)"\s+"([^"]*)"', block))
            for block in re.findall(r'\{(.*?)\}', text, re.S)]


def dominant_joint_for_triangle(triangle, joints, weights, joint_names):
    """Pick the bone with the largest summed weight over a triangle.

    ``joints`` and ``weights`` are per-vertex (vertices, 4) arrays; joint ids
    index ``joint_names``.  Ties resolve to the lowest joint id so output is
    deterministic.  A triangle with no positive weights falls back to joint 0.
    """
    scores = {}
    for vertex in triangle:
        for joint, weight in zip(joints[vertex], weights[vertex]):
            weight = float(weight)
            if weight <= 0.0:
                continue
            joint = int(joint)
            scores[joint] = scores.get(joint, 0.0) + weight
    if not scores:
        joint = 0
    else:
        joint = max(scores, key=lambda item: (scores[item], -item))
    if not joint_names:
        raise ValueError('A skinned mesh has no joint names')
    if joint < 0 or joint >= len(joint_names):
        raise ValueError(f'Triangle references joint {joint}, but the skin has {len(joint_names)} joints')
    return joint_names[joint]


def group_triangles_by_bone(indices, joints, weights, joint_names):
    """Return ``{bone: [triangle indices]}`` for one source surface."""
    triangles = np.asarray(indices, dtype=np.int64).reshape(-1, 3)
    groups = {}
    for triangle_index, triangle in enumerate(triangles):
        bone = dominant_joint_for_triangle(triangle, joints, weights, joint_names)
        groups.setdefault(bone, []).append(triangle_index)
    return {bone: np.asarray(items, dtype=np.int64) for bone, items in groups.items()}


def load_gltf_buffer(document, gltf_path):
    """Return the bytes for a glTF buffer, supporting data URIs and sidecars."""
    buffer = document['buffers'][0]
    uri = buffer.get('uri', '')
    if uri.startswith('data:'):
        header, payload = uri.split(',', 1)
        if ';base64' in header:
            return base64.b64decode(payload)
        from urllib.parse import unquote_to_bytes
        return unquote_to_bytes(payload)
    if not uri:
        raise ValueError(f'{gltf_path}: buffer has no URI')
    return (Path(gltf_path).parent / uri).read_bytes()


def decode_accessor(document, data, index):
    """Decode an accessor into a ``(count, components)`` NumPy array."""
    accessor = document['accessors'][index]
    components = TYPE_COMPONENTS[accessor['type']]
    dtype = COMPONENT_DTYPES[accessor['componentType']]
    count = int(accessor['count'])
    if 'bufferView' not in accessor:
        return np.zeros((count, components), dtype=dtype)
    view = document['bufferViews'][accessor['bufferView']]
    item_size = dtype.itemsize
    stride = view.get('byteStride') or item_size * components
    offset = int(view.get('byteOffset', 0)) + int(accessor.get('byteOffset', 0))
    if stride == item_size * components:
        values = np.frombuffer(data, dtype=dtype, count=count * components, offset=offset)
        return values.reshape(count, components).copy()
    values = np.ndarray(
        shape=(count, components),
        dtype=dtype,
        buffer=data,
        offset=offset,
        strides=(stride, item_size),
    )
    return values.copy()


def material_alpha_mode(info, name=''):
    """Map an OAT technique set to a glTF alpha mode.

    Technique sets encode the blend function in their ``b0``/``t0`` suffix:
    ``b0`` is alpha blend, ``t0`` is alpha test.  The name fallback only covers
    materials whose metadata was not emitted.
    """
    technique = str((info or {}).get('techniqueSet') or '').lower()
    if 'shadowcaster' in technique:
        return 'invisible'
    if '_t0' in technique or 'flag_t' in technique or 'treecanopy' in technique:
        return 'MASK'
    if '_b0' in technique or 'unlit_add' in technique:
        return 'BLEND'
    if not technique and (
            'glass' in name.lower() or name.lower().endswith('_alpha')):
        return 'BLEND'
    return 'OPAQUE'


def is_unlit_material(info, name=''):
    """Screens and other unlit 3D materials ignore lighting in the original."""
    technique = str((info or {}).get('techniqueSet') or '').lower()
    if 'unlit' in technique or technique.startswith('mc_sw4_3d_'):
        return True
    return 'nuketown_tv' in technique or 'nuketown_tv' in name.lower()


def find_static_duplicates(static_models, entity_records, tolerance_inches=1.0):
    """C2M static node names duplicating an entity model at the same position."""
    duplicates = []
    tolerance = float(tolerance_inches) * UNIT
    for static_index, placement in enumerate(static_models):
        name = placement['Name']
        candidates = [record for record in entity_records if record['model'] == name]
        if not candidates:
            continue
        position = placement['Position']
        converted = convert_position(position['X'], position['Y'], position['Z'])
        if any(math.dist(converted, record['position']) <= tolerance for record in candidates):
            duplicates.append(c2m_node_name(name, static_index))
    return duplicates


def entity_gametypes(entity):
    """Return the authored gametype tokens from ``script_gameobjectname``."""
    value = str(entity.get('script_gameobjectname', '') or '')
    return tuple(token for token in value.split() if token)


def is_unsupported_gametype(entity):
    """True when every authored gametype token is an unimplemented dm/tdm one."""
    gametypes = set(entity_gametypes(entity))
    return bool(gametypes) and gametypes <= set(UNSUPPORTED_GAMETYPES)


def build_entity_records(entities, available_models):
    """Build manifest entity records, preserving source order and keys."""
    records = []
    skipped = []
    for source_index, entity in enumerate(entities):
        if entity.get('classname') != 'script_model':
            continue
        model = entity.get('model', '')
        targetname = entity.get('targetname', '')
        reason = None
        if model in PLACEHOLDER_MODELS:
            reason = 'placeholder_model'
        elif targetname in HIDDEN_OUTRO_TARGETS:
            reason = 'hidden_outro_copy'
        elif is_unsupported_gametype(entity):
            reason = 'unsupported_gametype_dm_tdm'
        elif model not in available_models:
            reason = 'missing_gltf_geometry'
        if reason:
            skipped.append({
                'model': model,
                'targetname': targetname,
                'source_index': source_index,
                'reason': reason,
                'source_keys': dict(entity),
            })
            continue
        origin = parse_source_vector(entity.get('origin', '0 0 0'), 'origin')
        records.append({
            'id': 0,
            'model': model,
            'position': convert_position(*origin),
            'rotation_degrees': parse_cod_angles(entity.get('angles', '0 0 0')),
            'targetname': targetname,
            'destructibledef': entity.get('destructibledef', ''),
            'source_index': source_index,
            'source_keys': dict(entity),
        })
    for index, record in enumerate(records):
        record['id'] = index
    return records, skipped


def build_loose_records(placements, start_id, static_count=STATIC_MODEL_COUNT):
    """Build manifest entries for the C2M dynamic placement list."""
    records = []
    for index, placement in enumerate(placements):
        position = placement['Position']
        rotation = placement['RotationDegrees']
        scale = placement['ModelScale']
        records.append({
            'id': start_id + index,
            'model': placement['Name'],
            'position': convert_position(position['X'], position['Y'], position['Z']),
            'rotation_degrees': [
                _clean(rotation['X']),
                _clean(rotation['Y']),
                _clean(rotation['Z']),
            ],
            'scale': [
                _clean(scale['X']),
                _clean(scale['Z']),
                _clean(scale['Y']),
            ],
            'source_node': c2m_node_name(placement['Name'], static_count + index),
        })
    return records


class UnresolvedCollector:
    """Collect missing source resources without inventing placeholders."""

    def __init__(self):
        self.items = []
        self._seen = set()

    def add(self, kind, name, detail=''):
        key = (kind, name, detail)
        if key in self._seen:
            return
        self._seen.add(key)
        self.items.append({'kind': kind, 'name': name, 'detail': detail})


class TextureStore:
    """Convert shared DDS textures to at-most-1024 px PNGs."""

    def __init__(self, model_export_dir, output_models_dir, unresolved,
                 max_size=MAX_TEXTURE_SIZE):
        self.model_export_dir = Path(model_export_dir)
        self.texture_dir = Path(output_models_dir) / 'textures'
        self.unresolved = unresolved
        self.max_size = int(max_size)
        self.cache = {}
        self.paths = set()

    def convert(self, uri, normal=False):
        """Return a path relative to the glTF (``textures/...png``), or None."""
        if not uri:
            return None
        key = (uri, bool(normal))
        if key in self.cache:
            return self.cache[key]
        source = (self.model_export_dir / uri).resolve()
        if not source.is_file():
            self.unresolved.add('texture', uri, 'DDS source not found')
            self.cache[key] = None
            return None
        filename = hashlib.sha256(f'{uri}|normal={bool(normal)}'.encode()).hexdigest()[:24] + '.png'
        relative = f'textures/{filename}'
        destination = self.texture_dir / filename
        destination.parent.mkdir(parents=True, exist_ok=True)
        if not destination.exists():
            try:
                with Image.open(source) as image:
                    image.load()
                    rgba = image.convert('RGBA')
                if max(rgba.size) > self.max_size:
                    scale = self.max_size / max(rgba.size)
                    size = (max(1, round(rgba.width * scale)), max(1, round(rgba.height * scale)))
                    rgba = rgba.resize(size, Image.Resampling.LANCZOS)
                if normal:
                    red, green, blue, alpha = rgba.split()
                    rgba = Image.merge('RGBA', (red, ImageOps.invert(green), blue, alpha))
                rgba.save(destination, compress_level=3)
            except Exception as error:  # noqa: BLE001 - report the exact source
                self.unresolved.add('texture', uri, str(error))
                self.cache[key] = None
                return None
        self.cache[key] = relative
        self.paths.add(relative)
        return relative


class ModelBuilder:
    """Flatten one OAT glTF into a skinless y-up meter glTF."""

    def __init__(self, name, gltf_path, material_index, texture_store, unresolved):
        self.name = name
        self.gltf_path = Path(gltf_path)
        self.material_index = material_index
        self.texture_store = texture_store
        self.unresolved = unresolved
        self.document = None
        self.data = None
        self.output = {
            'asset': {'version': '2.0', 'generator': 'Shooter Nuketown props importer'},
            'scene': 0,
            'scenes': [{'nodes': []}],
            'nodes': [],
            'meshes': [],
            'materials': [],
            'textures': [],
            'images': [],
            'samplers': [{
                'magFilter': 9729,
                'minFilter': 9987,
                'wrapS': 10497,
                'wrapT': 10497,
            }],
            'buffers': [],
            'bufferViews': [],
            'accessors': [],
        }
        self.binary = bytearray()
        self.material_cache = {}
        self.texture_cache = {}
        self.node_names = set()
        self.triangles = 0
        self.texture_paths = set()

    def metadata(self, material_name):
        base = str(material_name).rsplit('/', 1)[-1]
        info = self.material_index.get(base)
        if info is None:
            self.unresolved.add('material', material_name, 'OAT material metadata not found')
            return {}
        return info

    def model_material(self, index):
        if index in self.material_cache:
            return self.material_cache[index]
        materials = self.document.get('materials', [])
        source = materials[index] if 0 <= index < len(materials) else {}
        name = source.get('name', 'default')
        info = self.metadata(name)
        source_pbr = source.get('pbrMetallicRoughness', {})
        pbr = {
            'baseColorFactor': [float(value) for value in source_pbr.get(
                'baseColorFactor', [1.0, 1.0, 1.0, 1.0])],
            'metallicFactor': 0.0,
            'roughnessFactor': 0.8,
        }
        color = source_pbr.get('baseColorTexture')
        if color is not None:
            uri = self.image_uri(color.get('index'))
            converted = self.texture_store.convert(uri, normal=False)
            if converted is not None:
                pbr['baseColorTexture'] = {'index': self.texture(converted)}
        material = {
            'name': name,
            'pbrMetallicRoughness': pbr,
            'doubleSided': bool(source.get('doubleSided', True)),
        }
        normal = source.get('normalTexture')
        if normal is not None:
            uri = self.image_uri(normal.get('index'))
            converted = self.texture_store.convert(uri, normal=True)
            if converted is not None:
                material['normalTexture'] = {'index': self.texture(converted)}
        mode = material_alpha_mode(info, name)
        if mode == 'MASK':
            material['alphaMode'] = 'MASK'
            material['alphaCutoff'] = 0.35
        elif mode == 'BLEND':
            material['alphaMode'] = 'BLEND'
        elif mode == 'invisible':
            material['alphaMode'] = 'MASK'
            material['alphaCutoff'] = 1.0
            material['doubleSided'] = False
            pbr['baseColorFactor'] = [0, 0, 0, 0]
        if is_unlit_material(info, name):
            material['extensions'] = {'KHR_materials_unlit': {}}
            if 'extensionsUsed' not in self.output:
                self.output['extensionsUsed'] = ['KHR_materials_unlit']
        result = len(self.output['materials'])
        self.output['materials'].append(material)
        self.material_cache[index] = result
        return result

    def image_uri(self, texture_index):
        if texture_index is None:
            return None
        textures = self.document.get('textures', [])
        if not (0 <= texture_index < len(textures)):
            raise ValueError(f'{self.gltf_path}: texture index {texture_index} is out of range')
        source = textures[texture_index].get('source')
        images = self.document.get('images', [])
        if source is None or not (0 <= source < len(images)):
            return None
        return images[source].get('uri')

    def texture(self, relative):
        if relative in self.texture_cache:
            return self.texture_cache[relative]
        source = len(self.output['images'])
        self.output['images'].append({'uri': relative})
        self.output['textures'].append({'sampler': 0, 'source': source})
        self.texture_cache[relative] = source
        self.texture_paths.add(relative)
        return source

    def accessor(self, values, kind, component=5126, bounds=False):
        values = np.ascontiguousarray(values)
        _, components = values.shape
        expected = TYPE_COMPONENTS[kind]
        if components != expected:
            raise ValueError(f'Accessor {kind} needs {expected} components, got {components}')
        while len(self.binary) % 4:
            self.binary.append(0)
        offset = len(self.binary)
        payload = values.astype(COMPONENT_DTYPES[component], copy=False).tobytes()
        self.binary.extend(payload)
        view = len(self.output['bufferViews'])
        self.output['bufferViews'].append({
            'buffer': 0,
            'byteOffset': offset,
            'byteLength': len(payload),
        })
        accessor = {
            'bufferView': view,
            'componentType': component,
            'count': int(values.shape[0]),
            'type': kind,
        }
        if bounds:
            accessor['min'] = [float(value) for value in values.min(axis=0)]
            accessor['max'] = [float(value) for value in values.max(axis=0)]
        result = len(self.output['accessors'])
        self.output['accessors'].append(accessor)
        return result

    def build_primitive(self, source_primitive, vertex_indices, local_indices):
        attributes = {}
        arrays = {}
        for attribute, accessor_index in source_primitive['attributes'].items():
            if attribute in SKIPPED_ATTRIBUTES:
                continue
            source_accessor = self.document['accessors'][accessor_index]
            values = decode_accessor(self.document, self.data, accessor_index)[vertex_indices]
            if source_accessor.get('normalized') and np.issubdtype(values.dtype, np.integer):
                maximum = np.iinfo(values.dtype).max
                values = values.astype(np.float32) / float(maximum)
            if attribute == 'POSITION':
                values = values.astype(np.float32) * UNIT
            else:
                values = values.astype(np.float32)
            arrays[attribute] = values
            kind = {1: 'SCALAR', 2: 'VEC2', 3: 'VEC3', 4: 'VEC4'}.get(values.shape[1])
            if kind is None:
                raise ValueError(f'{self.gltf_path}: unsupported attribute {attribute} shape {values.shape}')
            attributes[attribute] = self.accessor(values, kind, bounds=attribute == 'POSITION')
        if 'POSITION' not in attributes:
            raise ValueError(f'{self.gltf_path}: primitive has no POSITION attribute')
        material = source_primitive.get('material', -1)
        self.triangles += int(local_indices.size // 3)
        return {
            'attributes': attributes,
            'indices': self.accessor(np.asarray(local_indices, dtype=np.uint32).reshape(-1, 1),
                                     'SCALAR', component=5125),
            'material': self.model_material(material),
            'mode': 4,
        }

    def source_indices(self, primitive):
        if 'indices' in primitive:
            indices = decode_accessor(self.document, self.data, primitive['indices']).reshape(-1)
            return indices.astype(np.int64)
        position_accessor = primitive['attributes']['POSITION']
        count = int(self.document['accessors'][position_accessor]['count'])
        return np.arange(count, dtype=np.int64)

    def add_static_mesh(self, primitives):
        if not primitives:
            return
        output_primitives = []
        for primitive in primitives:
            if primitive.get('mode', 4) != 4:
                raise ValueError(f'{self.gltf_path}: only triangle primitives are supported')
            indices = self.source_indices(primitive)
            if indices.size % 3:
                raise ValueError(f'{self.gltf_path}: index count is not divisible by three')
            used, inverse = np.unique(indices, return_inverse=True)
            local = inverse.reshape(-1).astype(np.uint32)
            output_primitives.append(self.build_primitive(primitive, used, local))
        mesh_index = len(self.output['meshes'])
        self.output['meshes'].append({'name': 'tag_origin', 'primitives': output_primitives})
        node_name = self.unique_node('tag_origin')
        self.add_node(node_name, mesh_index, 'tag_origin')

    def add_skinned_primitive(self, primitive, joint_names):
        if 'JOINTS_0' not in primitive['attributes'] or 'WEIGHTS_0' not in primitive['attributes']:
            return False
        if primitive.get('mode', 4) != 4:
            raise ValueError(f'{self.gltf_path}: only triangle primitives are supported')
        indices = self.source_indices(primitive)
        if indices.size % 3:
            raise ValueError(f'{self.gltf_path}: index count is not divisible by three')
        triangles = indices.reshape(-1, 3)
        joints = decode_accessor(self.document, self.data, primitive['attributes']['JOINTS_0'])
        weights = decode_accessor(self.document, self.data, primitive['attributes']['WEIGHTS_0'])
        groups = group_triangles_by_bone(triangles, joints, weights, joint_names)
        for bone, triangle_indices in groups.items():
            selected = triangles[triangle_indices].reshape(-1)
            used, inverse = np.unique(selected, return_inverse=True)
            local = inverse.reshape(-1).astype(np.uint32)
            output_primitive = self.build_primitive(primitive, used, local)
            mesh_index = len(self.output['meshes'])
            self.output['meshes'].append({'name': bone, 'primitives': [output_primitive]})
            node_name = self.unique_node(bone)
            self.add_node(node_name, mesh_index, bone)
        return True

    def unique_node(self, bone):
        if bone not in self.node_names:
            self.node_names.add(bone)
            return bone
        suffix = 1
        while f'{bone}_{suffix}' in self.node_names:
            suffix += 1
        name = f'{bone}_{suffix}'
        self.node_names.add(name)
        return name

    def add_node(self, name, mesh_index, source_bone):
        node = len(self.output['nodes'])
        self.output['nodes'].append({
            'name': name,
            'mesh': mesh_index,
            'extras': {'source_bone': source_bone},
        })
        self.output['scenes'][0]['nodes'].append(node)

    def build(self):
        self.document = json.loads(self.gltf_path.read_text())
        self.data = load_gltf_buffer(self.document, self.gltf_path)
        nodes = self.document.get('nodes', [])
        meshes = self.document.get('meshes', [])
        if not meshes:
            raise ValueError(f'{self.gltf_path}: no meshes')
        mesh_skin = {}
        for node in nodes:
            if 'mesh' not in node:
                continue
            if node.get('skin') is not None or node['mesh'] not in mesh_skin:
                mesh_skin[node['mesh']] = node.get('skin')
        static_primitives = []
        for mesh_index, mesh in enumerate(meshes):
            skin_index = mesh_skin.get(mesh_index)
            skin = None
            joint_names = None
            if skin_index is not None:
                skins = self.document.get('skins', [])
                if not (0 <= skin_index < len(skins)):
                    raise ValueError(f'{self.gltf_path}: mesh {mesh_index} references missing skin {skin_index}')
                skin = skins[skin_index]
                joint_names = [nodes[joint].get('name', f'joint_{joint}')
                               for joint in skin.get('joints', [])]
            for primitive in mesh.get('primitives', []):
                if skin is not None and self.add_skinned_primitive(primitive, joint_names):
                    continue
                static_primitives.append(primitive)
        self.add_static_mesh(static_primitives)
        if not self.output['meshes']:
            raise ValueError(f'{self.gltf_path}: no renderable triangles')
        self.output['buffers'] = [{
            'uri': f'{self.name}.bin',
            'byteLength': len(self.binary),
        }]
        return self.output, bytes(self.binary), self.triangles, self.texture_paths


class PropsExport:
    """Run the Nuketown prop import end to end."""

    def __init__(self, source, output, max_texture=MAX_TEXTURE_SIZE):
        self.source = Path(source)
        self.output = Path(output)
        self.map_dir = self.source / 'source/exported_maps/black_ops_2' / MAP_NAME
        self.model_export_dir = self.source / 'unlink/model_export'
        self.images_dir = self.source / 'unlink/images'
        self.materials_dir = self.source / 'unlink/materials'
        self.models_dir = self.output / 'models'
        self.max_texture = int(max_texture)
        self.available_models = self.find_available_models()
        self.material_index = self.load_material_index()
        self.unresolved = UnresolvedCollector()
        self.texture_store = TextureStore(
            self.model_export_dir, self.models_dir, self.unresolved, self.max_texture)
        self.model_builders = []
        self.stats = {
            'models': 0,
            'nodes': 0,
            'triangles': 0,
            'textures': 0,
        }

    def find_available_models(self):
        if not self.model_export_dir.is_dir():
            raise FileNotFoundError(f'Missing OAT model export directory: {self.model_export_dir}')
        return {
            path.name[:-len('_lod0.gltf')]
            for path in self.model_export_dir.glob('*_lod0.gltf')
        }

    def load_material_index(self):
        index = {}
        if not self.materials_dir.is_dir():
            return index
        for path in sorted(self.materials_dir.rglob('*.json')):
            try:
                info = json.loads(path.read_text())
            except (OSError, ValueError):
                continue
            if info.get('_type') != 'material':
                continue
            # Prefer the mc/ material namespace used by these models.
            if path.parent.name == 'mc' or path.stem not in index:
                index[path.stem] = info
        return index

    def targeted_models(self):
        targeted = set()
        for pattern in REQUIRED_MODEL_PATTERNS:
            matches = {name for name in self.available_models if fnmatch.fnmatch(name, pattern)}
            if not matches:
                raise FileNotFoundError(f'No OAT LOD0 models match required pattern {pattern!r}')
            targeted |= matches
        for variant in MANNEQUIN_VARIANTS:
            name = f'dest_nt_nuked_{variant}_d0'
            if name not in self.available_models:
                raise FileNotFoundError(f'Missing mannequin variant model {name}')
        return targeted

    def export_model(self, name):
        path = self.model_export_dir / f'{name}_lod0.gltf'
        if not path.is_file():
            raise FileNotFoundError(f'Missing OAT model {path}')
        builder = ModelBuilder(
            name, path, self.material_index, self.texture_store, self.unresolved)
        document, binary, triangles, _ = builder.build()
        self.models_dir.mkdir(parents=True, exist_ok=True)
        (self.models_dir / f'{name}.bin').write_bytes(binary)
        (self.models_dir / f'{name}.gltf').write_text(
            json.dumps(document, separators=(',', ':')))
        self.model_builders.append(builder)
        self.stats['models'] += 1
        self.stats['nodes'] += len(document['nodes'])
        self.stats['triangles'] += triangles
        self.stats['textures'] = len(self.texture_store.paths)

    def run(self):
        if not self.map_dir.is_dir():
            raise FileNotFoundError(f'Missing C2M map export: {self.map_dir}')
        entities = parse_entities(self.map_dir / f'{MAP_NAME}_entities.txt')
        static_models = json.loads((self.map_dir / f'{MAP_NAME}_static_models.json').read_text())
        dynamic_models = json.loads((self.map_dir / f'{MAP_NAME}_dynamic_models.json').read_text())
        entity_records, skipped = build_entity_records(entities, self.available_models)
        loose_records = build_loose_records(
            dynamic_models, len(entity_records), static_count=len(static_models))
        missing_loose = sorted({record['model'] for record in loose_records
                                if record['model'] not in self.available_models})
        if missing_loose:
            raise FileNotFoundError(
                f'Dynamic placement models have no OAT LOD0 geometry: {missing_loose}')
        selected = {record['model'] for record in entity_records}
        selected |= {record['model'] for record in loose_records}
        selected |= self.targeted_models()
        models_manifest = {}
        for name in sorted(selected):
            self.export_model(name)
            models_manifest[name] = (
                f'res://assets/maps/nuketown/props/models/{name}.gltf')
        static_duplicates = find_static_duplicates(static_models, entity_records)
        manifest = {
            'entities': entity_records,
            'loose': loose_records,
            'models': models_manifest,
            'static_duplicates': static_duplicates,
            'skipped': skipped,
            'unsupported_gametypes': list(UNSUPPORTED_GAMETYPES),
            'notes': [
                'Entities retain every authored source key; the runtime filters gametypes.',
                'Entities whose gametype keys are only dm/tdm are skipped as unsupported.',
                'Bomb/tablet objective-mode assets missing from the OAT dump are listed in '
                'unresolved; the CLI exits nonzero unless --allow-unresolved is passed.',
            ],
            'unresolved': self.unresolved.items,
            'source': {
                'map': MAP_NAME,
                'entity_file': f'{MAP_NAME}_entities.txt',
                'static_placements': len(static_models),
                'dynamic_placements': len(dynamic_models),
            },
            'units': 'meters',
            'coordinates': 'x,z,-y',
            'source_units_to_meters': UNIT,
            'counts': {
                'entities': len(entity_records),
                'loose': len(loose_records),
                'models': len(models_manifest),
                'static_duplicates': len(static_duplicates),
                'skipped': len(skipped),
                'unresolved': len(self.unresolved.items),
                'nodes': self.stats['nodes'],
                'triangles': self.stats['triangles'],
                'textures': self.stats['textures'],
            },
        }
        self.output.mkdir(parents=True, exist_ok=True)
        (self.output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
        return manifest


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', nargs='?', type=Path, default=DEFAULT_SOURCE,
                        help='Nuketown build root (default: builds/nuketown)')
    parser.add_argument('output', nargs='?', type=Path, default=DEFAULT_OUTPUT,
                        help='Godot props directory (default: godot/assets/maps/nuketown/props)')
    parser.add_argument('--max-texture', type=int, default=MAX_TEXTURE_SIZE,
                        help='maximum converted texture dimension (default: 1024)')
    parser.add_argument('--allow-unresolved', action='store_true',
                        help='exit zero after reporting known unresolved source assets')
    args = parser.parse_args(argv)
    manifest = PropsExport(args.source, args.output, args.max_texture).run()
    print(json.dumps(manifest['counts'], indent=2))
    if manifest['unresolved']:
        print('Unresolved source resources:', file=sys.stderr)
        for item in manifest['unresolved']:
            print(f"  {item['kind']}: {item['name']} ({item['detail']})", file=sys.stderr)
        if not args.allow_unresolved:
            print('Refusing to report success with unresolved assets; '
                  'pass --allow-unresolved only for known gametype-only omissions.',
                  file=sys.stderr)
            return 1
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
