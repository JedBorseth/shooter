#!/usr/bin/env python3
# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy>=2", "soundfile>=0.12"]
# ///
"""Import Nuketown's prop audio from the original Black Ops II sound banks.

The map's props are silent without this step.  ``behavior.json`` names sound
aliases per destructible piece and stage (``damage_sound``, ``break_sound``,
``loop_sound``, ``burn_sound``); the map's entity file names its placed
``script_sound`` emitters (the swinging sign and the water sprinkler); and the
decompiled map script names the countdown explosion.  This tool resolves those
names through the game's own alias tables, reads only the selected recordings
out of the ``.sabl``/``.sabs`` banks, and writes the game-ready clips beside the
props.

Inputs (all relative by default, so nothing machine-specific is committed):

* ``builds/nuketown/behavior.json`` -- prop stages and their sound aliases.
* ``builds/nuketown/decompiled/t6/mp_nuketown_2020*.gsc`` -- map script sounds.
* ``builds/nuketown/source/exported_maps/black_ops_2/mp_nuketown_2020/`` --
  the entity text with the placed ``script_sound`` names.
* The game install named by ``behavior.json`` (``source.fastfile``) or by
  ``--game-dir``.  Its ``sound/*.sabl``/``*.sabs`` files hold the recordings.
* ``open-fastfile`` (``--open-fastfile`` or ``$OPEN_FASTFILE``), whose T6
  walker dumps the alias tables from the map and its companion zones.  Alias
  JSON already dumped under ``--aliases-dir`` can be reused instead.

Only the aliases this tool selects are exported.  The bank's whole table is
never written to the repository: the work dump is disposable, and the output
directory receives a few dozen short cues (well under 10 MB).

Outputs
-------
``godot/assets/maps/nuketown/props/audio/<clip>.ogg``
    One file per selected source recording, downmixed to mono at the bank's
    48 kHz rate.  Shared recordings (a car wheel and a metal part, for
    example) are written once and referenced by every alias that plays them.
    If libsndfile has no Vorbis writer, ``--format wav`` writes PCM WAV
    instead; the repository accepts both.

``godot/assets/maps/nuketown/props/sounds.json``
    The runtime table.  Its stable part is ``aliases``::

        "aliases": {"<source alias>": ["res://assets/maps/nuketown/props/audio/<clip>.ogg", ...]}

    Every value is an ``Array[String]`` of interchangeable variants; a runtime
    picks one at random or steps through them.  Empty lists are never emitted.
    ``details`` carries the playback facts a helper needs (``loop``, gain,
    pitch, distance, source recording); ``report`` records provenance and every
    alias that could not be resolved exactly.  The complete schema is::

        {
          "schema": "nuketown-prop-audio/v1",
          "generated_utc": str,
          "source": {"map": str, "behavior": str, "fastfiles": [str],
                     "alias_zones": [str], "game_sound_dir": str,
                     "fastfile_sha256": str | null,
                     "open_fastfile_commit": str | null},
          "format": {"container": "ogg" | "wav", "codec": str,
                     "sample_rate": int, "channels": 1},
          "aliases": {str: [str]},
          "details": {str: {
              "kind": "exact" | "fallback",
              "source_alias": str,
              "loop": bool,
              "gain_db": [float, float],
              "pitch": [float, float],
              "distance_m": [float, float],
              "channels": int,
              "source_channels": int,
              "seconds": float,
              "layers": [str],
              "variants": [{"path": str, "source_clip": str,
                            "bank": str, "offset": int, "format": str,
                            "seconds": float, "channels": int,
                            "sample_rate": int, "peak_db": float}]
          }},
          "report": {
              "counts": {...},
              "fallbacks": [{"alias": str, "used": str, "reason": str}],
              "unresolved": [{"alias": str, "reason": str,
                              "fallback": str | null, "playable": bool}],
              "skipped": [{"alias": str, "reason": str, "source": str}],
              "script_sounds": {"decompiled": [str], "entities": [str]},
              "notes": [str]
          }
        }

Sound aliases absent from the shared MP bank and the map bank are reported in
``report.unresolved``, each with the fallback that plays in its place (and
``playable: true``) or ``playable: false`` when nothing sensible exists.  A
playable substitution is also repeated in ``report.fallbacks``; the runtime
alias still gets that audio, so a prop is never silent merely because the
shipped MP bank dropped one of its authored stage names.  The CLI exits nonzero
when an alias has no playable fallback unless ``--allow-unresolved`` is passed,
so silence is a visible failure rather than a mystery.

Run with::

    uv run tools/import_nuketown_audio.py \\
        --open-fastfile /path/to/open-fastfile \\
        [builds/nuketown] [godot/assets/maps/nuketown/props]

Then import the new files in Godot (``godot --headless --path godot --import``)
or open the editor.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import math
import os
import re
import shutil
import struct
import subprocess
import sys
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path

import numpy as np
import soundfile as sf

REPO = Path(__file__).resolve().parent.parent
DEFAULT_SOURCE = Path("builds/nuketown")
DEFAULT_PROPS = Path("godot/assets/maps/nuketown/props")
GODOT_ROOT = REPO / "godot"
DEFAULT_MAP = "mp_nuketown_2020"
# Output budget for the committed cues. Keep headroom under the 10 MB target.
DEFAULT_MAX_BYTES = 9_500_000
OUTPUT_RATE = 48_000
INCHES_PER_METRE = 39.3701

# Aliases the runtime needs, with the number of interchangeable variants worth
# keeping. One variant per alias would repeat the same hit every time; more
# than two or three buys little for a prop cue and grows the commit.
SELECTED_CUES: tuple[tuple[str, int, str], ...] = (
    # Car destruction and the countdown explosion.
    ("exp_veh_small", 2, "car explosion body (behavior exp_veh_small)"),
    ("wpn_grenade_explode", 1, "grenade/vehicle explosion alternative"),
    ("dst_car_bullet_impact", 3, "bullet hitting the car body"),
    ("dst_car_bullet_rico", 2, "bullet ricochet off the car"),
    ("dst_car_fire", 2, "burning car loop (behavior loop_sound)"),
    ("dst_car_door_break", 2, "car door breaking"),
    ("dst_car_glass_piece", 1, "car glass shattering"),
    ("dst_car_light_crack", 2, "car light cracking"),
    ("dst_car_parts_small", 2, "car debris"),
    ("dst_car_parts_tiny", 2, "small car debris"),
    ("dst_car_tire", 1, "car tire impact"),
    ("dst_car_wheel", 2, "car wheel coming loose"),
    # Mannequins. The authored dst_dest_nt_nuked_* aliases are not present in
    # the shipped MP bank; these are the actual mannequin/body cues used as
    # documented fallbacks below.
    ("fly_bump_mannequin", 3, "mannequin hit/detach"),
    ("fly_bodyfall_generic_npc", 2, "mannequin body falling"),
    # Glass and other single props.
    ("dst_generic_glass_large", 2, "large glass shatter"),
    ("dst_generic_glass_med", 1, "medium glass shatter"),
    ("dst_generic_glass_small", 2, "small glass shatter"),
    ("dst_dest_electronic_outlet01_d0", 2, "electrical outlet damage"),
    ("dst_dest_electronic_outlet01_dp_d0", 2, "electrical outlet break"),
    # Map script/entity sounds.
    ("amb_sign_squeak", 2, "swinging sign squeak (entity script_sound)"),
    ("amb_sprinkler_sml", 1, "water sprinkler loop (entity script_sound)"),
    # Loose vehicle props placed by the map's scripts.
    ("fly_bump_car", 3, "car body bump"),
    ("fly_bump_truck", 2, "truck body bump"),
)

# Zones whose SndAlias tables are searched, lowest priority first. The map is
# appended last so its own aliases win. common_mp is the shared MP root bank;
# en_common_mp adds the localised companion; zm_nuked supplies a handful of
# destructible aliases the MP bank shares but does not define (the outlet).
ALIAS_ZONES: tuple[str, ...] = (
    "zm_nuked",
    "code_post_gfx_mp",
    "patch_mp",
    "common_patch_mp",
    "en_common_mp",
    "common_mp",
)

# A behavior alias with no exact bank table gets the closest actual source
# alias. These are substitutions, not guessed files: every target below is
# itself resolved and checksum-validated in a shipped bank.
FALLBACKS: tuple[tuple[str, str, str], ...] = (
    (
        r"^dst_dest_nt_nuked_.*_d[1-3]$",
        "fly_bodyfall_generic_npc",
        "authored mannequin stage alias absent from the MP bank; bodyfall is the closest source cue",
    ),
    (
        r"^dst_dest_nt_nuked_.*_(arm_left|arm_right|head)_dp_d0$",
        "fly_bump_mannequin",
        "authored mannequin limb-detach alias absent from the MP bank; mannequin bump is the closest source cue",
    ),
    (
        r"^dst_dest_nt_nuked_.*_dp_d0$",
        "fly_bodyfall_generic_npc",
        "authored mannequin detach alias absent from the MP bank; bodyfall is the closest source cue",
    ),
    (
        r"^dst_dest_nt_nuked_",
        "fly_bump_mannequin",
        "authored mannequin damage alias absent from the MP bank; mannequin bump is the closest source cue",
    ),
    (
        r"^dst_veh_fx_",
        "exp_veh_small",
        "authored car FX stage alias absent from the MP bank; vehicle explosion is the closest source cue",
    ),
    (
        r"^dst_veh_car_glass",
        "dst_car_glass_piece",
        "authored car glass stage alias absent from the MP bank; car glass piece is the closest source cue",
    ),
    (
        r"^dst_veh_",
        "dst_car_parts_small",
        "authored car stage alias absent from the MP bank; car debris is the closest source cue",
    ),
    (
        r"^dst_dest_electronic_outlet01_dp_d1$",
        "dst_dest_electronic_outlet01_dp_d0",
        "only the first outlet break stage is defined; reuse stage 0",
    ),
)

# The countdown is a map-wide cinematic, not a prop cue. It is reported, not
# exported, so a 30-second explosion does not ride along with the props.
SKIP_SCRIPT_ALIASES = {
    "amb_end_nuke": "map-wide countdown cinematic",
    "amb_end_nuke_2d": "map-wide countdown cinematic",
}

# T6 sound-bank layout, checked rather than trusted (open-fastfile's t6/sab.py
# and OAT's SoundBankTypes.h describe the same file).
BANK_MAGIC = b"2UX#"
BANK_VERSION = 14
BANK_HEADER = struct.Struct("<4sIIIIIII3q16s")
BANK_HEADER_SIZE = 0x248
BANK_ENTRY = struct.Struct("<IIIIBBBB")
BANK_CHECKSUM_SIZE = 16
BANK_DEPENDENCY_SIZE = 64
BANK_DATA_START = 0x800
BANK_SECTOR = 2048
BANK_FRAME_RATES = (8000, 12000, 16000, 24000, 32000, 44100, 48000, 96000, 192000)
BANK_FORMATS = {0: "pcm16", 1: "pcm24", 2: "pcm32", 3: "ieee", 4: "xma4", 5: "mp3", 6: "msadpcm", 7: "wma", 8: "flac"}

# ---------------------------------------------------------------------------
# T6 sound bank reading


@dataclass(frozen=True)
class BankEntry:
    id: int
    size: int
    offset: int
    frames: int
    rate_index: int
    channels: int
    looping: int
    format: int

    @property
    def rate(self) -> int:
        if self.rate_index >= len(BANK_FRAME_RATES):
            raise ValueError(f"frame-rate index {self.rate_index} is outside the T6 table")
        return BANK_FRAME_RATES[self.rate_index]

    @property
    def format_name(self) -> str:
        return BANK_FORMATS.get(self.format, str(self.format))


class SoundBank:
    """One ``.sabl``/``.sabs`` file, validated on open and per clip."""

    def __init__(self, path: Path):
        self.path = path
        self.size = path.stat().st_size
        with path.open("rb") as fh:
            head = fh.read(BANK_HEADER_SIZE)
            if len(head) != BANK_HEADER_SIZE:
                raise ValueError(f"{path}: file is shorter than the bank header")
            (
                magic,
                version,
                entry_size,
                checksum_size,
                dependency_size,
                count,
                dependency_count,
                _pad,
                file_size,
                entry_offset,
                checksum_offset,
                self.checksum,
            ) = BANK_HEADER.unpack_from(head)
            if magic != BANK_MAGIC or version != BANK_VERSION:
                raise ValueError(f"{path}: not a version-{BANK_VERSION} sound bank ({magic!r}, {version})")
            if (entry_size, checksum_size, dependency_size) != (
                BANK_ENTRY.size,
                BANK_CHECKSUM_SIZE,
                BANK_DEPENDENCY_SIZE,
            ):
                raise ValueError(
                    f"{path}: entry/checksum/dependency sizes "
                    f"{entry_size}/{checksum_size}/{dependency_size}"
                )
            if file_size != self.size:
                raise ValueError(f"{path}: header says {file_size} bytes, file is {self.size}")
            self.dependencies = [
                head[BANK_HEADER.size + i * dependency_size : BANK_HEADER.size + (i + 1) * dependency_size]
                .split(b"\0")[0]
                .decode(errors="replace")
                for i in range(dependency_count)
            ]
            fh.seek(entry_offset)
            table = fh.read(count * BANK_ENTRY.size)
            fh.seek(checksum_offset)
            sums = fh.read(count * BANK_CHECKSUM_SIZE)
        if len(table) != count * BANK_ENTRY.size or len(sums) != count * BANK_CHECKSUM_SIZE:
            raise ValueError(f"{path}: truncated entry or checksum table")
        self.entries = [BankEntry(*BANK_ENTRY.unpack_from(table, i * BANK_ENTRY.size)) for i in range(count)]
        self.checksums = [sums[i * BANK_CHECKSUM_SIZE : (i + 1) * BANK_CHECKSUM_SIZE] for i in range(count)]
        self.by_offset = {entry.offset: (entry, index) for index, entry in enumerate(self.entries)}
        self._check_layout(entry_offset, checksum_offset)

    def _check_layout(self, entry_offset: int, checksum_offset: int) -> None:
        cursor = BANK_DATA_START
        for entry in sorted(self.entries, key=lambda item: item.offset):
            if entry.offset != cursor:
                raise ValueError(f"{self.path}: entry {entry.id:#x} at {entry.offset}, expected {cursor}")
            cursor += entry.size
        if _align(cursor) != entry_offset:
            raise ValueError(f"{self.path}: audio ends at {cursor}, entry table starts at {entry_offset}")
        if entry_offset + _align(len(self.entries) * BANK_ENTRY.size) != checksum_offset:
            raise ValueError(f"{self.path}: checksum table is not where the entry table ends")
        if checksum_offset + _align(len(self.entries) * BANK_CHECKSUM_SIZE) != self.size:
            raise ValueError(f"{self.path}: {self.size - checksum_offset} bytes after the checksum table")

    def read(self, offset: int) -> tuple[BankEntry, bytes]:
        found = self.by_offset.get(offset)
        if found is None:
            raise ValueError(f"{self.path}: no entry at offset {offset}")
        entry, index = found
        with self.path.open("rb") as fh:
            fh.seek(entry.offset)
            blob = fh.read(entry.size)
        if len(blob) != entry.size:
            raise ValueError(f"{self.path}: entry {entry.id:#x} is truncated")
        if hashlib.md5(blob).digest() != self.checksums[index]:
            raise ValueError(f"{self.path}: entry {entry.id:#x} does not match its checksum")
        return entry, blob

    def decode(self, offset: int) -> tuple[np.ndarray, int, BankEntry]:
        """One recording as int16 samples shaped ``(frames, channels)``."""
        entry, blob = self.read(offset)
        if entry.format == 0:
            expected = entry.frames * entry.channels * 2
            if len(blob) != expected:
                raise ValueError(f"{self.path}: {len(blob)} PCM bytes is not {entry.frames} frames")
            samples = np.frombuffer(blob, dtype="<i2").reshape(entry.frames, entry.channels)
            return samples, entry.rate, entry
        if entry.format == 8:
            samples, rate = sf.read(io.BytesIO(blob), dtype="int16", always_2d=True)
            if (rate, samples.shape[1], samples.shape[0]) != (entry.rate, entry.channels, entry.frames):
                raise ValueError(
                    f"{self.path}: FLAC is {rate} Hz x{samples.shape[1]} x{samples.shape[0]}, "
                    f"entry says {entry.rate} Hz x{entry.channels} x{entry.frames}"
                )
            return samples, rate, entry
        raise ValueError(f"{self.path}: entry {entry.id:#x} is {entry.format_name}, not decoded")


def _align(value: int) -> int:
    return -(-value // BANK_SECTOR) * BANK_SECTOR


def _bank_for(game_sound_dir: Path, bank_file: str) -> SoundBank:
    path = game_sound_dir / bank_file
    if not path.is_file():
        raise FileNotFoundError(f"sound bank is missing: {path}")
    return SoundBank(path)


# ---------------------------------------------------------------------------
# Alias dumping and loading


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _find_fastfile(game_dir: Path, zone: str) -> Path | None:
    for folder in ("all", "english"):
        path = game_dir / "zone" / folder / f"{zone}.ff"
        if path.is_file():
            return path
    return None


def _run_open_fastfile_dump(
    open_fastfile: Path,
    game_dir: Path,
    work: Path,
    map_zone: str,
    zones: tuple[str, ...],
) -> None:
    """Dump only the alias metadata for the requested zones, lowest priority first."""
    package = open_fastfile / "scripts" / "cod" / "t6" / "backend.py"
    if not package.is_file():
        raise FileNotFoundError(
            f"{open_fastfile} does not look like the open-fastfile checkout (missing {package})"
        )
    sys.path.insert(0, str(open_fastfile))
    try:
        from scripts.cod.t6.backend import Backend  # type: ignore[import-not-found]

        backend = Backend()
        backend.check()
        alias_root = work / "snd"
        if alias_root.exists():
            shutil.rmtree(alias_root)
        alias_root.mkdir(parents=True, exist_ok=True)
        dumped: list[str] = []
        for zone in (*zones, map_zone):
            fastfile = _find_fastfile(game_dir, zone)
            if fastfile is None:
                print(f"  alias zone {zone}: no fastfile, skipped", file=sys.stderr)
                continue
            backend.release()
            backend.dump(str(fastfile), str(alias_root), "sound")
            dumped.append(zone)
        backend.release()
    finally:
        # The checkout is an input, not a dependency of the rest of this tool.
        if str(open_fastfile) in sys.path:
            sys.path.remove(str(open_fastfile))
    (work / "snd-zones.json").write_text(
        json.dumps(
            {
                "map": map_zone,
                "zones": dumped,
                "open_fastfile_commit": _git_commit(open_fastfile),
                "fastfiles": {
                    zone: _relative_for_report(_find_fastfile(game_dir, zone), REPO)
                    for zone in dumped
                },
            },
            indent=2,
        )
        + "\n"
    )


def ensure_alias_dump(
    source: Path,
    game_dir: Path,
    open_fastfile: Path | None,
    map_zone: str,
    zones: tuple[str, ...],
    redump: bool,
) -> Path:
    """Return a directory of ``sound_aliases/*.json``, dumping it if needed."""
    alias_root = source / "audio-work" / "snd" / "sound_aliases"
    if alias_root.is_dir() and not redump and any(alias_root.glob("*.json")):
        print(f"reusing alias dump {alias_root}")
        return alias_root
    if open_fastfile is None:
        raise SystemExit(
            "no alias dump found and no open-fastfile checkout given; pass "
            "--open-fastfile PATH (or $OPEN_FASTFILE) once, or --aliases-dir PATH"
        )
    print(f"dumping alias tables with {open_fastfile}")
    _run_open_fastfile_dump(open_fastfile, game_dir, source / "audio-work", map_zone, zones)
    if not alias_root.is_dir():
        raise SystemExit(f"open-fastfile produced no alias dump at {alias_root}")
    return alias_root


def load_alias_json(alias_root: Path, name: str) -> dict | None:
    path = alias_root / f"{name}.json"
    if not path.is_file():
        return None
    data = json.loads(path.read_text())
    if data.get("_type") != "sound_alias":
        return None
    return data


def banked_entries(alias: dict) -> list[dict]:
    return [entry for entry in alias.get("entries", []) if (entry.get("soundFile") or {}).get("bank")]


# ---------------------------------------------------------------------------
# Behavior and map-script inputs


def load_behavior(source: Path) -> dict:
    path = source / "behavior.json"
    if not path.is_file():
        raise FileNotFoundError(f"missing behavior file: {path}")
    return json.loads(path.read_text())


def map_name_from_behavior(behavior: dict) -> str:
    fastfile = ((behavior.get("source") or {}).get("fastfile") if isinstance(behavior, dict) else None)
    if fastfile:
        return Path(str(fastfile)).stem
    return DEFAULT_MAP


def game_dir_from_behavior(behavior: dict) -> Path | None:
    fastfile = ((behavior.get("source") or {}).get("fastfile") if isinstance(behavior, dict) else None)
    if not fastfile:
        return None
    path = Path(str(fastfile))
    # <game>/zone/<language>/<map>.ff
    if path.parent.parent.name == "zone":
        return path.parent.parent.parent
    return None


def behavior_sound_aliases(behavior: dict) -> dict[str, list[dict]]:
    """Every non-null sound alias a destructible piece or stage names, with owners."""
    found: dict[str, list[dict]] = {}

    def add(alias: object, owner: dict, field: str) -> None:
        if not isinstance(alias, str) or alias == "null":
            return
        found.setdefault(alias, []).append({**owner, "field": field})

    for definition in behavior.get("destructibles", []):
        for piece_index, piece in enumerate(definition.get("pieces", [])):
            base = {
                "destructible": definition.get("name"),
                "piece": piece_index,
                "damage_sound": piece.get("damage_sound"),
            }
            add(piece.get("damage_sound"), {**base, "stage": None}, "damage_sound")
            add(piece.get("burn_sound"), {**base, "stage": None}, "burn_sound")
            for stage in piece.get("stages", []):
                stage_owner = {
                    "destructible": definition.get("name"),
                    "piece": piece_index,
                    "stage": stage.get("index"),
                    "damage_sound": piece.get("damage_sound"),
                }
                add(stage.get("break_sound"), stage_owner, "break_sound")
                add(stage.get("loop_sound"), stage_owner, "loop_sound")
    return found


def script_sounds(source: Path, map_zone: str) -> dict[str, list[str]]:
    """Sound names the map's decompiled scripts and placed entities actually ask for."""
    decompiled: list[str] = []
    entities: list[str] = []
    for path in sorted((source / "decompiled" / "t6").glob(f"{map_zone}*.gsc")):
        text = path.read_text(errors="replace")
        decompiled.extend(re.findall(r'play\s*sound\s*\(\s*"([^"]+)"', text, flags=re.IGNORECASE))
    for path in sorted((source / "source" / "exported_maps").rglob(f"{map_zone}_entities.txt")):
        text = path.read_text(errors="replace")
        entities.extend(re.findall(r'"script_sound"\s+"([^"]+)"', text))
    return {"decompiled": sorted(set(decompiled)), "entities": sorted(set(entities))}


# ---------------------------------------------------------------------------
# Export


def _slug(value: str) -> str:
    return re.sub(r"[^a-z0-9]+", "_", value.lower()).strip("_")


def clip_slug(source_clip: str) -> str:
    """A readable, stable filename for a recording's game path."""
    text = source_clip.lower()
    for suffix in (".ln65", ".ll65", ".pn65", ".sn65", ".pc.snd"):
        text = text.removesuffix(suffix)
    return _slug(text)


def gain_db(value: int) -> float:
    linear = max(0.0, value / 65535.0)
    return round(20.0 * math.log10(linear), 2) if linear > 0 else -120.0


def pitch_ratio(value: int) -> float:
    return round(value / 32767.0, 4)


def distance_m(value: int) -> float:
    return round(value / INCHES_PER_METRE, 2)


class AudioExport:
    def __init__(
        self,
        source: Path,
        output: Path,
        fmt: str,
        max_bytes: int,
        game_dir: Path | None = None,
    ):
        self.source = source
        self.output = output
        self.format = fmt
        self.max_bytes = max_bytes
        self.behavior = load_behavior(source)
        self.map_zone = map_name_from_behavior(self.behavior)
        env_game = os.environ.get("SHOOTER_T6_GAME")
        self.game_dir = game_dir or (Path(env_game) if env_game else game_dir_from_behavior(self.behavior))
        self.game_sound_dir: Path | None = None
        self.alias_root: Path | None = None
        self._banks: dict[str, SoundBank] = {}
        self._clips: dict[tuple[str, int], dict] = {}
        self._alias_cache: dict[str, dict | None] = {}
        self._alias_exports: dict[str, dict] = {}
        self._used_names: set[str] = set()
        self.total_bytes = 0
        self.report_fallbacks: list[dict] = []
        self.report_unresolved: list[dict] = []
        self.report_skipped: list[dict] = []
        self.behavior_aliases = behavior_sound_aliases(self.behavior)
        self.script_aliases = script_sounds(source, self.map_zone)

    # -- alias resolution -------------------------------------------------

    def alias(self, name: str) -> dict | None:
        key = name.lower()
        if key not in self._alias_cache:
            assert self.alias_root is not None
            self._alias_cache[key] = load_alias_json(self.alias_root, key)
        return self._alias_cache[key]

    def alias_clips(self, name: str) -> list[dict]:
        alias = self.alias(name)
        return banked_entries(alias) if alias else []

    def fallback_for(self, alias: str) -> tuple[str, str] | None:
        for pattern, target, reason in FALLBACKS:
            if re.search(pattern, alias):
                return target, reason
        return None

    # -- clip decoding ----------------------------------------------------

    def bank(self, bank_file: str) -> SoundBank:
        if bank_file not in self._banks:
            assert self.game_sound_dir is not None
            self._banks[bank_file] = _bank_for(self.game_sound_dir, bank_file)
        return self._banks[bank_file]

    def export_alias(self, source_alias: str, variants: int, note: str) -> dict | None:
        """Decode and write the first ``variants`` recordings behind ``source_alias``."""
        if source_alias in self._alias_exports:
            existing = self._alias_exports[source_alias]
            if existing["variants"] >= variants:
                return existing
        entries = self.alias_clips(source_alias)
        if not entries:
            return None
        alias = self.alias(source_alias) or {}
        selected = entries[: max(1, variants)]
        clip_records: list[dict] = []
        paths: list[str] = []
        for entry in selected:
            sound_file = entry["soundFile"]
            bank_info = sound_file["bank"]
            bank_file = bank_info["file"]
            offset = int(bank_info["offset"])
            record = self.export_clip(bank_file, offset, sound_file.get("path") or source_alias)
            clip_records.append(record)
            paths.append(record["path"])
        first = selected[0]
        seconds = sum(item["seconds"] for item in clip_records)
        export = {
            "source_alias": source_alias,
            "note": note,
            "paths": paths,
            "variants": len(clip_records),
            "clips": clip_records,
            "loop": any(bool((entry["soundFile"]["bank"] or {}).get("looping")) for entry in selected),
            "gain_db": [
                gain_db(min(entry["volMin"] for entry in selected)),
                gain_db(max(entry["volMax"] for entry in selected)),
            ],
            "pitch": [
                pitch_ratio(min(entry["pitchMin"] for entry in selected)),
                pitch_ratio(max(entry["pitchMax"] for entry in selected)),
            ],
            "distance_m": [
                distance_m(min(entry["distMin"] for entry in selected)),
                distance_m(max(entry["distMax"] for entry in selected)),
            ],
            "channels": 1,
            "source_channels": clip_records[0]["channels"] if clip_records else 1,
            "seconds": round(seconds, 3),
            "layers": sorted(
                {
                    entry.get("secondaryName")
                    for entry in alias.get("entries", [])
                    if entry.get("secondaryName")
                }
            ),
            "bank_alias": alias.get("bank"),
            "first_clip": first["soundFile"].get("path"),
        }
        self._alias_exports[source_alias] = export
        return export

    def export_clip(self, bank_file: str, offset: int, source_clip: str) -> dict:
        key = (bank_file, offset)
        if key in self._clips:
            return self._clips[key]
        samples, rate, entry = self.bank(bank_file).decode(offset)
        if rate != OUTPUT_RATE:
            raise ValueError(f"{source_clip}: {rate} Hz recording is not the expected {OUTPUT_RATE} Hz")
        mono = samples.astype(np.float32)
        if mono.ndim > 1 and mono.shape[1] > 1:
            mono = mono.mean(axis=1)
        else:
            mono = mono.reshape(-1)
        # Keep the source level; only guard against an accidental digital
        # overshoot in the mono sum. A prop cue should not be normalised away
        # from the bank's mix.
        peak = float(np.max(np.abs(mono))) / 32768.0 if mono.size else 0.0
        # Leave headroom for Vorbis' transient overshoot, which can exceed the
        # source peak by a decibel or two on glass and gunfire. Quiet source
        # material (the sprinkler is deliberately subtle) is not lifted, so the
        # bank's mix is preserved.
        ceiling = 10 ** (-4.0 / 20.0)
        if peak > ceiling:
            mono = mono * (ceiling / peak)
            peak = ceiling
        # libsndfile's float API expects [-1, 1]; passing int16-scale floats
        # would clip or distort the Vorbis encoder.
        mono = mono / 32768.0
        if self.format == "ogg":
            extension = ".ogg"
            write_format = "OGG"
            subtype = "VORBIS"
        else:
            extension = ".wav"
            write_format = "WAV"
            subtype = "PCM_16"
        stem = clip_slug(source_clip)
        name = stem + extension
        if name in self._used_names:
            name = f"{stem}_{hashlib.sha1(f'{bank_file}:{offset}'.encode()).hexdigest()[:8]}{extension}"
        self._used_names.add(name)
        self.output.mkdir(parents=True, exist_ok=True)
        destination = self.output / name
        sf.write(destination, mono, rate, format=write_format, subtype=subtype)
        info = sf.info(destination)
        if info.frames <= 0 or info.samplerate != OUTPUT_RATE or info.channels != 1:
            raise ValueError(f"{destination}: wrote an invalid clip ({info})")
        check, check_rate = sf.read(destination, dtype="float32")
        if check_rate != OUTPUT_RATE or check.size == 0 or not np.isfinite(check).all():
            raise ValueError(f"{destination}: clip is not readable at {OUTPUT_RATE} Hz")
        if float(np.max(np.abs(check))) < 0.001:
            raise ValueError(f"{destination}: clip is silent")
        self.total_bytes += destination.stat().st_size
        record = {
            "path": _res_path(destination),
            "source_clip": source_clip,
            "bank": bank_file,
            "offset": offset,
            "format": entry.format_name,
            "seconds": round(entry.frames / entry.rate, 3),
            "channels": entry.channels,
            "sample_rate": entry.rate,
            "peak_db": round(20.0 * math.log10(peak), 2) if peak > 0 else -120.0,
        }
        self._clips[key] = record
        return record

    # -- report helpers ---------------------------------------------------

    def resolve_behavior(self) -> None:
        """Resolve every behavior alias to exact audio or a recorded fallback."""
        for alias, owners in sorted(self.behavior_aliases.items()):
            exact = self.export_alias(alias, VARIANT_LIMIT.get(alias, 2), "behavior alias")
            if exact is not None:
                continue
            fallback = self.fallback_for(alias)
            if fallback is None:
                self.report_unresolved.append(
                    {
                        "alias": alias,
                        "reason": "no SndAlias table entry in the map bank, shared MP bank, or selected companion zones",
                        "fallback": None,
                        "playable": False,
                        "owners": owners[:4],
                    }
                )
                continue
            target, reason = fallback
            resolved = self.export_alias(target, VARIANT_LIMIT.get(target, 2), f"fallback for {alias}")
            if resolved is None:
                self.report_unresolved.append(
                    {
                        "alias": alias,
                        "reason": f"exact alias absent; fallback {target} also has no banked recording",
                        "fallback": None,
                        "playable": False,
                        "owners": owners[:4],
                    }
                )
                continue
            self.report_unresolved.append(
                {
                    "alias": alias,
                    "reason": reason,
                    "fallback": target,
                    "playable": True,
                    "owners": owners[:4],
                }
            )
            self.report_fallbacks.append(
                {
                    "alias": alias,
                    "used": target,
                    "reason": reason,
                    "owners": owners[:4],
                }
            )

    def runtime_alias(self, alias: str) -> list[str] | None:
        exact = self._alias_exports.get(alias)
        if exact is not None:
            return exact["paths"]
        fallback = self.fallback_for(alias)
        if fallback is not None and fallback[0] in self._alias_exports:
            return self._alias_exports[fallback[0]]["paths"]
        return None

    def detail(self, source_alias: str, kind: str) -> dict:
        export = self._alias_exports[source_alias]
        return {
            "kind": kind,
            "source_alias": source_alias,
            "loop": bool(export["loop"]),
            "gain_db": export["gain_db"],
            "pitch": export["pitch"],
            "distance_m": export["distance_m"],
            "channels": export["channels"],
            "source_channels": export["source_channels"],
            "seconds": export["seconds"],
            "layers": export["layers"],
            "variants": [
                {
                    "path": clip["path"],
                    "source_clip": clip["source_clip"],
                    "bank": clip["bank"],
                    "offset": clip["offset"],
                    "format": clip["format"],
                    "seconds": clip["seconds"],
                    "channels": clip["channels"],
                    "sample_rate": clip["sample_rate"],
                    "peak_db": clip["peak_db"],
                }
                for clip in export["clips"]
            ],
        }

    def details(self) -> dict:
        details: dict = {}
        for alias in sorted(self.behavior_aliases):
            paths = self.runtime_alias(alias)
            if paths is None:
                continue
            exact = alias in self._alias_exports
            source_alias = alias if exact else (self.fallback_for(alias) or ("", ""))[0]
            if source_alias in self._alias_exports:
                details[alias] = self.detail(source_alias, "exact" if exact else "fallback")
        return details


def _res_path(path: Path) -> str:
    # Godot's project root is godot/, not the repository root.
    relative = path.resolve().relative_to(GODOT_ROOT.resolve())
    return "res://" + relative.as_posix()


def _relative_for_report(path: Path | None, root: Path) -> str | None:
    if path is None:
        return None
    # Prefer the path as the caller sees it (builds/nuketown may be a symlink to
    # an HDD work area); fall back to a resolved path, then to the bare name.
    for candidate, base in ((path.absolute(), root.absolute()), (path.resolve(), root.resolve())):
        try:
            return candidate.relative_to(base).as_posix()
        except ValueError:
            continue
    # Never write a machine path into the manifest. A source outside the
    # repository is identified by name only.
    return path.name


VARIANT_LIMIT: dict[str, int] = {alias: variants for alias, variants, _note in SELECTED_CUES}


def run(
    source: Path,
    manifest_path: Path,
    audio_dir: Path,
    game_dir: Path | None,
    open_fastfile: Path | None,
    aliases_dir: Path | None,
    redump: bool,
    fmt: str,
    max_bytes: int,
) -> tuple[dict, int]:
    exporter = AudioExport(source, audio_dir, fmt, max_bytes, game_dir)
    if exporter.game_dir is None:
        raise SystemExit(
            "cannot find the game install; behavior.json has no source.fastfile. "
            "Pass --game-dir /path/to/pluto_t6_full_game"
        )
    exporter.game_sound_dir = exporter.game_dir / "sound"
    if not exporter.game_sound_dir.is_dir():
        raise SystemExit(f"game sound directory is missing: {exporter.game_sound_dir}")

    if aliases_dir is not None:
        exporter.alias_root = aliases_dir
    else:
        exporter.alias_root = ensure_alias_dump(
            source,
            exporter.game_dir,
            open_fastfile,
            exporter.map_zone,
            ALIAS_ZONES,
            redump,
        )

    # Export the selected source cues once. The fallback targets are already in
    # this list; resolve_behavior() only adds a target if a configuration edit
    # introduces one that is not.
    for alias, variants, note in SELECTED_CUES:
        exported = exporter.export_alias(alias, variants, note)
        if exported is None:
            exporter.report_skipped.append(
                {
                    "alias": alias,
                    "reason": "selected cue has no banked recording in the dumped alias zones",
                    "source": "selection",
                }
            )

    exporter.resolve_behavior()

    # Map script sounds. The sign and sprinkler are selected above; anything
    # else is reported so the omission is a decision, not an oversight.
    for alias in exporter.script_aliases["decompiled"]:
        if alias in SKIP_SCRIPT_ALIASES:
            exporter.report_skipped.append(
                {"alias": alias, "reason": SKIP_SCRIPT_ALIASES[alias], "source": "decompiled"}
            )
        elif alias not in exporter.behavior_aliases and alias not in VARIANT_LIMIT:
            exporter.report_skipped.append(
                {"alias": alias, "reason": "outside the selected prop cue set", "source": "decompiled"}
            )
    for alias in exporter.script_aliases["entities"]:
        if alias in SKIP_SCRIPT_ALIASES:
            exporter.report_skipped.append(
                {"alias": alias, "reason": SKIP_SCRIPT_ALIASES[alias], "source": "entities"}
            )
        elif alias not in exporter.behavior_aliases and alias not in VARIANT_LIMIT:
            exporter.report_skipped.append(
                {"alias": alias, "reason": "outside the selected prop cue set", "source": "entities"}
            )

    aliases: dict[str, list[str]] = {}
    for alias in sorted(exporter.behavior_aliases):
        paths = exporter.runtime_alias(alias)
        if paths:
            aliases[alias] = paths
    for alias, variants, _note in SELECTED_CUES:
        exported = exporter._alias_exports.get(alias)
        if exported is not None and alias not in aliases:
            aliases[alias] = exported["paths"]

    details = exporter.details()
    for alias, export in exporter._alias_exports.items():
        # Every source cue that was exported is directly playable too (the sign,
        # the sprinkler, the car bump), even when no behavior alias names it.
        aliases.setdefault(alias, export["paths"])
        details.setdefault(alias, exporter.detail(alias, "exact"))

    # Validate the runtime table before writing it.
    missing_files = [
        path
        for paths in aliases.values()
        for path in paths
        if not (GODOT_ROOT / path.removeprefix("res://")).is_file()
    ]
    if missing_files:
        raise SystemExit(f"runtime table names missing files: {missing_files[:4]}")

    if exporter.total_bytes > max_bytes:
        raise SystemExit(
            f"selected cues are {exporter.total_bytes / 1e6:.2f} MB, over the "
            f"{max_bytes / 1e6:.2f} MB budget; reduce variants in SELECTED_CUES"
        )

    open_fastfile_commit = _git_commit(open_fastfile)
    marker = source / "audio-work" / "snd-zones.json"
    if open_fastfile_commit is None and marker.is_file():
        try:
            open_fastfile_commit = json.loads(marker.read_text()).get("open_fastfile_commit")
        except (OSError, ValueError):
            open_fastfile_commit = None

    manifest = {
        "schema": "nuketown-prop-audio/v1",
        "generated_utc": datetime.now(UTC).isoformat(timespec="seconds"),
        "source": {
            "map": exporter.map_zone,
            "behavior": _relative_for_report(source / "behavior.json", REPO),
            "fastfiles": sorted(
                {
                    _relative_for_report(_find_fastfile(exporter.game_dir, zone), REPO)
                    for zone in (exporter.map_zone, *ALIAS_ZONES)
                    if _find_fastfile(exporter.game_dir, zone) is not None
                }
            ),
            "alias_zones": [exporter.map_zone, *ALIAS_ZONES],
            "game_sound_dir": "sound/",
            "fastfile_sha256": (exporter.behavior.get("source") or {}).get("fastfile_sha256"),
            "open_fastfile_commit": open_fastfile_commit,
        },
        "format": {
            "container": "ogg" if fmt == "ogg" else "wav",
            "codec": "vorbis" if fmt == "ogg" else "pcm_s16le",
            "sample_rate": OUTPUT_RATE,
            "channels": 1,
        },
        "aliases": aliases,
        "details": details,
        "report": {
            "counts": {
                "behavior_aliases": len(exporter.behavior_aliases),
                "behavior_aliases_covered": sum(
                    1 for alias in exporter.behavior_aliases if exporter.runtime_alias(alias)
                ),
                "runtime_aliases": len(aliases),
                "exact_aliases": sum(1 for item in details.values() if item["kind"] == "exact"),
                "fallback_aliases": sum(1 for item in details.values() if item["kind"] == "fallback"),
                "unresolved_aliases": len(exporter.report_unresolved),
                "silent_aliases": sum(1 for item in exporter.report_unresolved if not item["playable"]),
                "source_aliases": len(exporter._alias_exports),
                "clips": len(exporter._clips),
                "bytes": exporter.total_bytes,
                "megabytes": round(exporter.total_bytes / 1e6, 2),
            },
            "fallbacks": exporter.report_fallbacks,
            "unresolved": exporter.report_unresolved,
            "skipped": exporter.report_skipped,
            "script_sounds": exporter.script_aliases,
            "notes": [
                "Runtime aliases map to interchangeable OGG variants; pick one or step through them.",
                "details[alias].loop says whether the source bank entry loops (car fire and sprinkler).",
                "details[alias].gain_db/pitch/distance_m are the alias' authored envelopes, not a normalisation target.",
                "Audio is downmixed to mono for 3D playback; source levels are preserved.",
                "Hot source clips are limited to -4 dBFS before Vorbis so codec overshoot does not clip.",
                (
                    "Authored mannequin and late car-stage aliases are absent from the shipped MP bank; "
                    "report.fallbacks names every substitution."
                ),
            ],
        },
    }
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
    return manifest, exporter.total_bytes


def _git_commit(tree: Path | None) -> str | None:
    if tree is None:
        return None
    try:
        return subprocess.run(
            ["git", "-C", str(tree), "rev-parse", "HEAD"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("source", nargs="?", type=Path, default=DEFAULT_SOURCE,
                        help="Nuketown build root (default: builds/nuketown)")
    parser.add_argument("props", nargs="?", type=Path, default=None,
                        help="props directory for sounds.json (default: godot/assets/maps/nuketown/props)")
    parser.add_argument("--game-dir", type=Path, default=None,
                        help="Black Ops II game root with sound/ and zone/ "
                             "(default: derived from behavior.json source.fastfile)")
    parser.add_argument("--open-fastfile", type=Path, default=None,
                        help="open-fastfile checkout used to dump SndAlias tables "
                             "(default: $OPEN_FASTFILE; only needed without a cached dump)")
    parser.add_argument("--aliases-dir", type=Path, default=None,
                        help="use an existing sound_aliases/ dump instead of running open-fastfile")
    parser.add_argument("--redump", action="store_true",
                        help="discard the cached alias dump and walk the zones again")
    parser.add_argument("--format", choices=("ogg", "wav"), default=None,
                        help="output container (default: ogg when libsndfile supports Vorbis, else wav)")
    parser.add_argument("--max-bytes", type=int, default=DEFAULT_MAX_BYTES,
                        help=f"fail above this committed-audio budget (default: {DEFAULT_MAX_BYTES})")
    parser.add_argument("--allow-unresolved", action="store_true",
                        help="exit zero even when an alias has no playable fallback")
    args = parser.parse_args(argv)

    if args.format is None:
        args.format = "ogg" if sf.available_formats().get("OGG") else "wav"
    if args.format == "ogg" and not sf.available_formats().get("OGG"):
        print("libsndfile has no OGG writer; falling back to WAV", file=sys.stderr)
        args.format = "wav"
    props = args.props or DEFAULT_PROPS
    project_manifest = props / "sounds.json"
    audio_dir = props / "audio"
    open_fastfile = args.open_fastfile
    if open_fastfile is None:
        env = os.environ.get("OPEN_FASTFILE")
        open_fastfile = Path(env) if env else None

    behavior_path = args.source / "behavior.json"
    if not behavior_path.is_file():
        print(f"missing behavior file: {behavior_path}", file=sys.stderr)
        return 1
    behavior = json.loads(behavior_path.read_text())
    if args.game_dir is None:
        derived = game_dir_from_behavior(behavior)
        if derived is None:
            print("cannot derive the game root from behavior.json; pass --game-dir", file=sys.stderr)
            return 1
        args.game_dir = derived

    manifest, total_bytes = run(
        args.source,
        project_manifest,
        audio_dir,
        args.game_dir,
        open_fastfile,
        args.aliases_dir,
        args.redump,
        args.format,
        args.max_bytes,
    )
    counts = manifest["report"]["counts"]
    print(
        f"wrote {counts['runtime_aliases']} runtime aliases, {counts['clips']} clips, "
        f"{total_bytes / 1e6:.2f} MB to {audio_dir}"
    )
    if manifest["report"]["unresolved"]:
        unresolved = manifest["report"]["unresolved"]
        silent = [item for item in unresolved if not item["playable"]]
        print(
            f"{len(unresolved)} alias(es) lack an exact bank entry; "
            f"{len(silent)} have no playable fallback (see report.unresolved)",
            file=sys.stderr,
        )
        for item in silent:
            print(f"  SILENT {item['alias']}: {item['reason']}", file=sys.stderr)
        if silent and not args.allow_unresolved:
            print(
                "Pass --allow-unresolved only after reviewing the report.",
                file=sys.stderr,
            )
            return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
