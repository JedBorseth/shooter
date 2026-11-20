# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy>=2", "pillow>=11"]
# ///
"""Authors the soldier's directional locomotion cycles: strafes, diagonals,
and backpedals for the walk, the jog, and the crouched walk, and the forward
diagonals of the sprint; and a vault over thin cover (``author_vault``).

Neither CC0 animation library the game uses has them, so they are made from
the forward cycles in ``godot/assets/characters/animations/ual1.glb``:

    uv run tools/build_gait_animations.py

writes ``godot/assets/characters/animations/gait.glb`` (the same skeleton, a
stub mesh, and the new clips). ``tools/build_character_assets.py`` runs it
after it rewrites ``ual1.glb``.

How a cycle for a direction is made (``author``):

- **The base cycle.** Directions from 45° left to 45° right of forward start
  from the forward cycle at the same stride phase. Directions further back
  start from the forward cycle played backward, so a backpedal reaches back
  toe first and rolls onto the heel. The backward cycle is timed so that each
  foot is planted over the same stride phases as in the forward one, so
  neighbouring directions blend without a foot sliding in one and swinging
  in the other.
- **Foot paths.** Each ankle's path through the cycle is turned about its
  mean position to the new direction, so the planted foot slides opposite to
  the travel (the body moving over it). Sideways the steps are shorter and
  quicker (``LATERAL_CADENCE``; the game's stride clock speeds them up to
  match, ``shooter_sim::hitbox::gait::cadence``), and the feet stand a
  little wider.
- **Hips.** The pelvis turns toward the travel (``HIP_YAW`` at a full
  strafe), the spine turns back so the chest still faces forward, and the
  trailing foot steps in front of the leading one, so a strafe crosses over
  instead of the legs passing through each other.
- **Legs.** Two-bone IK puts each ankle on its path: the knee bends to the
  hip-to-ankle distance about its own hinge, then the leg swings to the
  target. Feet keep the forward cycle's roll, turned with the hips and
  flattened a little sideways.

The stride phase of every clip is the game's shared one: phase 0 has the
left foot furthest along the travel, just planted (``gait::Loop::offset`` is
0 for each; the sprint's is the source clip's own offset, 0.86). Clip names
are ``<Base>_<Dir>``, Dir one of ``FR``, ``R``, ``BR``, ``B``, ``BL``, ``L``,
``FL`` (the sprint only ``FR`` and ``FL``).
"""

from __future__ import annotations

import argparse
import math
import sys
from dataclasses import dataclass
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from build_character_assets import (
    ROOT,
    Gltf,
    Writer,
    attach_mesh,
    copy_rig,
    write_stub_mesh,
)

ANIMATIONS = ROOT / "godot" / "assets" / "characters" / "animations"

# Sideways the stride is this much quicker (and each step this much
# shorter): cadence 1 + LATERAL_CADENCE * |sin(direction)|. Must match
# `gait::LATERAL_CADENCE`.
LATERAL_CADENCE = 0.2


@dataclass(frozen=True)
class Base:
    """A forward cycle and how its sideways versions are shaped."""

    clip: str
    name: str
    # Where the clip's stride phase 0 is (`gait::Loop::offset`).
    offset: float
    # How far the hips turn at a full strafe, and how far the line between
    # the feet turns, degrees.
    hip_yaw: float
    stance: float
    # How much wider each foot stands at a full strafe, metres.
    widen: float
    # The directions authored (`DIRECTIONS` labels); all of them if empty.
    only: tuple[str, ...] = ()


BASES = [
    Base("Walk_Loop", "Walk", 0.0, 20.0, 40.0, 0.03),
    Base("Jog_Fwd_Loop", "Jog", 0.0, 28.0, 45.0, 0.035),
    Base("Crouch_Fwd_Loop", "Crouch", 0.12, 22.0, 40.0, 0.02),
    # A sprint only drifts sideways (at most about 27°, the movement's
    # `sprint_strafe`), so it has the forward diagonals alone, and the hips
    # turn further into it than a jog's do.
    Base("Sprint_Loop", "Sprint", 0.86, 35.0, 45.0, 0.03, ("FR", "FL")),
]

# Direction name, angle from forward toward the right (degrees), and the share
# of `Base::hip_yaw` and `Base::stance` the hips and the feet turn toward the
# right (negative: the left). The line between the feet turns the same way all
# round (the trailing foot steps in front of the leading one), furthest at the
# back diagonals, so that the travel never runs along it: a foot's lane stays
# at least half the feet's spacing clear of the other's in every direction
# and in every blend between two neighbours. (Turning the back diagonals the
# other way would separate them better on their own, but the blend between
# a strafe and a back diagonal would then pass the legs through each other.)
# The left side mirrors the right.
DIRECTIONS = [
    ("FR", 45.0, 0.6, 0.45),
    ("R", 90.0, 1.0, 1.0),
    ("BR", 135.0, 0.8, 1.8),
    ("B", 180.0, 0.0, 0.0),
    ("BL", -135.0, -0.8, -1.8),
    ("L", -90.0, -1.0, -1.0),
    ("FL", -45.0, -0.6, -0.45),
]

# Share of the hips' turn the spine bones keep (the rest is given back, so the
# upper chest faces forward).
SPINE_KEEP = {"DEF-spine.001": 0.5, "DEF-spine.002": 0.2, "DEF-spine.003": 0.0}


# --- Small rotation helpers (quaternions are x, y, z, w as glTF stores them) -----


def quat_to_mat(q: np.ndarray) -> np.ndarray:
    x, y, z, w = q / np.linalg.norm(q)
    return np.array(
        [
            [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
            [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
            [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)],
        ]
    )


def mat_to_quat(m: np.ndarray) -> np.ndarray:
    t = np.trace(m)
    if t > 0:
        s = math.sqrt(t + 1.0) * 2
        q = [(m[2, 1] - m[1, 2]) / s, (m[0, 2] - m[2, 0]) / s, (m[1, 0] - m[0, 1]) / s, 0.25 * s]
    elif m[0, 0] > m[1, 1] and m[0, 0] > m[2, 2]:
        s = math.sqrt(1.0 + m[0, 0] - m[1, 1] - m[2, 2]) * 2
        q = [0.25 * s, (m[0, 1] + m[1, 0]) / s, (m[0, 2] + m[2, 0]) / s, (m[2, 1] - m[1, 2]) / s]
    elif m[1, 1] > m[2, 2]:
        s = math.sqrt(1.0 + m[1, 1] - m[0, 0] - m[2, 2]) * 2
        q = [(m[0, 1] + m[1, 0]) / s, 0.25 * s, (m[1, 2] + m[2, 1]) / s, (m[0, 2] - m[2, 0]) / s]
    else:
        s = math.sqrt(1.0 + m[2, 2] - m[0, 0] - m[1, 1]) * 2
        q = [(m[0, 2] + m[2, 0]) / s, (m[1, 2] + m[2, 1]) / s, 0.25 * s, (m[1, 0] - m[0, 1]) / s]
    q = np.array(q)
    return q / np.linalg.norm(q)


def slerp(a: np.ndarray, b: np.ndarray, t: float) -> np.ndarray:
    if np.dot(a, b) < 0:
        b = -b
    d = float(np.clip(np.dot(a, b), -1.0, 1.0))
    if d > 0.9995:
        q = a + (b - a) * t
        return q / np.linalg.norm(q)
    theta = math.acos(d)
    return (math.sin((1 - t) * theta) * a + math.sin(t * theta) * b) / math.sin(theta)


def axis_angle(axis: np.ndarray, angle: float) -> np.ndarray:
    """Rotation matrix about a unit axis (Rodrigues)."""
    x, y, z = axis
    c, s = math.cos(angle), math.sin(angle)
    k = np.array([[0, -z, y], [z, 0, -x], [-y, x, 0]])
    return np.eye(3) * c + s * k + (1 - c) * np.outer(axis, axis)


def yaw(angle_right: float) -> np.ndarray:
    """Turns forward (+Z, the glTF character's facing) toward its right (-X)
    by `angle_right` radians."""
    return axis_angle(np.array([0.0, 1.0, 0.0]), -angle_right)


def between(a: np.ndarray, b: np.ndarray) -> np.ndarray:
    """The smallest rotation taking direction `a` to direction `b`."""
    a = a / np.linalg.norm(a)
    b = b / np.linalg.norm(b)
    axis = np.cross(a, b)
    s = np.linalg.norm(axis)
    c = float(np.dot(a, b))
    if s < 1e-9:
        return np.eye(3)
    return axis_angle(axis / s, math.atan2(s, c))


# --- The skeleton and its clips ------------------------------------------------------


class Clip:
    """One animation's channels, sampled at any phase."""

    def __init__(self, src: Gltf, name: str):
        anim = next(a for a in src.json["animations"] if a["name"] == name)
        self.channels: dict[tuple[int, str], np.ndarray] = {}
        self.order: list[tuple[int, str, str]] = []
        frames = None
        for ch in anim["channels"]:
            sampler = anim["samplers"][ch["sampler"]]
            node, path = ch["target"]["node"], ch["target"]["path"]
            times = src.accessor(sampler["input"])[:, 0]
            frames = len(times) if frames is None else frames
            if len(times) != frames:
                raise SystemExit(f"{name}: channels have different key counts")
            self.channels[(node, path)] = src.accessor(sampler["output"]).astype(np.float64)
            self.order.append((node, path, sampler.get("interpolation", "LINEAR")))
        self.frames = frames
        self.length = float(times[-1])

    def sample(self, phase: float) -> dict[tuple[int, str], np.ndarray]:
        """Every channel at `phase` (0..1 of the clip, wrapping; the last key
        repeats the first)."""
        at = (phase % 1.0) * (self.frames - 1)
        i = min(math.floor(at), self.frames - 2)
        t = at - i
        out = {}
        for key, values in self.channels.items():
            a, b = values[i], values[i + 1]
            out[key] = slerp(a, b, t) if key[1] == "rotation" else a + (b - a) * t
        return out


class Skeleton:
    def __init__(self, src: Gltf):
        self.nodes = src.json["nodes"]
        self.index = {n["name"]: i for i, n in enumerate(self.nodes)}
        self.parent: dict[int, int] = {}
        for i, n in enumerate(self.nodes):
            for c in n.get("children", []):
                self.parent[c] = i

    def local(self, pose: dict, i: int) -> tuple[np.ndarray, np.ndarray]:
        n = self.nodes[i]
        t = pose.get((i, "translation"), n.get("translation", [0.0, 0.0, 0.0]))
        r = pose.get((i, "rotation"), n.get("rotation", [0.0, 0.0, 0.0, 1.0]))
        return quat_to_mat(np.asarray(r, dtype=np.float64)), np.asarray(t, dtype=np.float64)

    def globals(self, pose: dict) -> dict[int, tuple[np.ndarray, np.ndarray]]:
        """Global rotation and position of every node (scales are 1)."""
        out: dict[int, tuple[np.ndarray, np.ndarray]] = {}

        def get(i: int) -> tuple[np.ndarray, np.ndarray]:
            if i not in out:
                r, t = self.local(pose, i)
                if i in self.parent:
                    pr, pt = get(self.parent[i])
                    r, t = pr @ r, pr @ t + pt
                out[i] = (r, t)
            return out[i]

        for i in range(len(self.nodes)):
            get(i)
        return out


def contact(skel: Skeleton, clip: Clip, side: str, offset: float, samples: int = 120) -> tuple[float, float]:
    """The stride phases where foot `side` lands and lifts off: the ankle or
    the ball within 2 cm of its lowest."""
    ankle, ball = skel.index[f"DEF-foot.{side}"], skel.index[f"DEF-toe.{side}"]
    heights = []
    for k in range(samples):
        g = skel.globals(clip.sample(k / samples + offset))
        heights.append((g[ankle][1][1], g[ball][1][1]))
    h = np.array(heights)
    low = (h[:, 0] < h[:, 0].min() + 0.02) | (h[:, 1] < h[:, 1].min() + 0.02)
    # The first planted sample after a lifted one, and the last before one.
    land = next(k for k in range(samples) if low[k] and not low[k - 1])
    lift = next(k for k in range(samples) if low[k] and not low[(k + 1) % samples])
    return land / samples, lift / samples


def solve_leg(
    hip: np.ndarray,
    knee: np.ndarray,
    ankle: np.ndarray,
    hinge: np.ndarray,
    target: np.ndarray,
) -> tuple[np.ndarray, np.ndarray, float]:
    """Two-bone IK. From a leg (hip, knee, and ankle positions, the knee's
    hinge axis) returns the rotations to apply to the thigh and to the shin
    (world space, about their own origins) that put the ankle on `target`,
    and how far off it ends (m)."""
    upper, lower = knee - hip, ankle - knee
    l1, l2 = np.linalg.norm(upper), np.linalg.norm(lower)
    want = np.linalg.norm(target - hip)
    want = min(max(want, abs(l1 - l2) + 1e-4), l1 + l2 - 1e-4)
    # Bend the knee about its hinge until the hip-to-ankle distance is right:
    # (A - C) cos b + B sin b = D - C, with the shin turned by b.
    b = hinge / np.linalg.norm(hinge)
    d = (want * want - l1 * l1 - l2 * l2) / 2.0
    a_ = float(np.dot(upper, lower))
    b_ = float(np.dot(upper, np.cross(b, lower)))
    c_ = float(np.dot(upper, b) * np.dot(b, lower))
    r = math.hypot(a_ - c_, b_)
    phi = math.atan2(b_, a_ - c_)
    cos = max(-1.0, min(1.0, (d - c_) / r)) if r > 1e-9 else 1.0
    options = [phi + math.acos(cos), phi - math.acos(cos)]
    options = [((o + math.pi) % math.tau) - math.pi for o in options]
    bend = min(options, key=abs)
    turn_shin = axis_angle(b, bend)
    reach = upper + turn_shin @ lower
    swing = between(reach, target - hip)
    end = hip + swing @ reach
    return swing, swing @ turn_shin, float(np.linalg.norm(end - target))


def author(
    skel: Skeleton,
    base: Clip,
    spec: Base,
    direction: float,
    hip_share: float,
    stance_share: float,
    drop: float = 0.0,
) -> tuple[list[dict], float]:
    """The cycle for `direction` (degrees from forward, toward the right) from
    the forward cycle `base`, with the hips `drop` metres lower. Returns one
    pose per frame (channel -> value) and the worst IK miss (m)."""
    theta = math.radians(direction)
    side = math.sin(theta)
    backward = abs(direction) > 90.0
    # Each foot's landing and lift in the forward cycle; the backward cycle
    # is the forward one reversed about their sum, so a foot lands where it
    # lifted off going forward, at the same stride phase.
    land, lift = contact(skel, base, "L", spec.offset)
    pivot = land + lift
    base_dir = math.pi if backward else 0.0

    hips_i = skel.index["DEF-hips"]

    def base_pose(phase: float) -> dict:
        stride = (pivot - phase) if backward else phase
        pose = base.sample(stride + spec.offset)
        # The hips' parent is Z-up.
        pose[(hips_i, "translation")] = pose[(hips_i, "translation")] - np.array([0.0, 0.0, drop])
        return pose

    n = base.frames
    legs = {s: [skel.index[f"DEF-thigh.{s}"], skel.index[f"DEF-shin.{s}"], skel.index[f"DEF-foot.{s}"]] for s in "LR"}
    # Mean ankle positions over the cycle, and the hips' (the turn's pivot).
    mean_ankle = {s: np.zeros(3) for s in "LR"}
    mean_hips = np.zeros(3)
    flat_foot = {}
    for k in range(n - 1):
        g = skel.globals(base.sample(k / (n - 1) + spec.offset))
        for s in "LR":
            mean_ankle[s] += g[legs[s][2]][1] / (n - 1)
        mean_hips += g[hips_i][1] / (n - 1)
    for s, own_land, own_lift in (("L", land, lift), ("R", land + 0.5, lift + 0.5)):
        mid = (own_land + ((own_lift - own_land) % 1.0) / 2.0) % 1.0
        flat_foot[s] = skel.globals(base.sample(mid + spec.offset))[legs[s][2]][0]

    cadence = 1.0 + LATERAL_CADENCE * abs(side)
    hip_turn = math.radians(spec.hip_yaw) * hip_share
    turn_hips = yaw(hip_turn)
    turn_stance = yaw(math.radians(spec.stance) * stance_share)
    turn_path = yaw(theta - base_dir)
    centres = {}
    middle = (mean_ankle["L"] + mean_ankle["R"]) / 2.0
    middle[1] = 0.0
    for s, sign in (("L", 1.0), ("R", -1.0)):
        # The glTF character's left is +X, forward +Z.
        c = mean_ankle[s].copy()
        c[1] = 0.0
        c += np.array([sign * spec.widen * abs(side), 0.0, 0.0])
        centres[s] = middle + turn_stance @ (c - middle)

    frames = []
    worst = 0.0
    for k in range(n):
        phase = k / (n - 1)
        pose = base_pose(phase)
        g = skel.globals(pose)
        new_rot: dict[int, np.ndarray] = {}
        # The hips turn about their own origin; the spine turns back.
        new_rot[hips_i] = turn_hips @ g[hips_i][0]
        for name, keep in SPINE_KEEP.items():
            i = skel.index[name]
            new_rot[i] = yaw(hip_turn * keep) @ g[i][0]
        hips_r, hips_t = new_rot[hips_i], g[hips_i][1]
        for s in "LR":
            thigh, shin, foot = legs[s]
            ankle = g[foot][1]
            offset = ankle - mean_ankle[s]
            flat = turn_path @ np.array([offset[0], 0.0, offset[2]]) / cadence
            # The feet stay at their own heights however low the hips sit.
            target = centres[s] + flat + np.array([0.0, ankle[1] + drop, 0.0])
            # The leg as the turned hips carry it, then IK to the target.
            _, thigh_t = skel.local(pose, thigh)
            hip = hips_r @ thigh_t + hips_t
            carry = turn_hips
            knee = hip + carry @ (g[shin][1] - g[thigh][1])
            ankle_now = knee + carry @ (g[foot][1] - g[shin][1])
            hinge = carry @ g[thigh][0][:, 0]
            thigh_turn, shin_turn, miss = solve_leg(hip, knee, ankle_now, hinge, target)
            worst = max(worst, miss)
            new_rot[thigh] = thigh_turn @ carry @ g[thigh][0]
            new_rot[shin] = shin_turn @ carry @ g[shin][0]
            # The foot keeps its roll, turned with the hips, a little
            # flatter sideways.
            roll = mat_to_quat(carry @ g[foot][0])
            flat_q = mat_to_quat(carry @ flat_foot[s])
            new_rot[foot] = quat_to_mat(slerp(roll, flat_q, 0.4 * abs(side)))
        # Back to local rotations, parents first.
        out = dict(pose)
        for i in sorted(new_rot, key=lambda j: depth(skel, j)):
            parent_r = parent_rotation(skel, pose, new_rot, i)
            out[(i, "rotation")] = mat_to_quat(parent_r.T @ new_rot[i])
        frames.append(out)
    # The last frame repeats the first exactly, so the loop closes.
    frames[-1] = frames[0]
    return frames, worst


def depth(skel: Skeleton, i: int) -> int:
    d = 0
    while i in skel.parent:
        i = skel.parent[i]
        d += 1
    return d


def parent_rotation(skel: Skeleton, pose: dict, new_rot: dict, i: int) -> np.ndarray:
    """The new global rotation of `i`'s parent."""
    chain = []
    j = i
    while j in skel.parent:
        j = skel.parent[j]
        chain.append(j)
    # From the top: each node either has a new global rotation or keeps its
    # local one under its (possibly new) parent.
    rot = np.eye(3)
    for node in reversed(chain):
        if node in new_rot:
            rot = new_rot[node]
        else:
            rot = rot @ skel.local(pose, node)[0]
    return rot


def check(skel: Skeleton, frames: list[dict], spec: Base, direction: float) -> dict:
    """Measures an authored cycle: how far the planted feet slide (should be
    the travel), the ankles' spread, and that every bone kept its length."""
    theta = math.radians(direction)
    travel = np.array([-math.sin(theta), 0.0, math.cos(theta)])
    lowest = {}
    rows = []
    for pose in frames:
        g = skel.globals(pose)
        rows.append({s: g[skel.index[f"DEF-foot.{s}"]][1] for s in "LR"})
    for s in "LR":
        lowest[s] = min(r[s][1] for r in rows)
    along = {s: [float(np.dot(r[s], travel)) for r in rows] for s in "LR"}
    gap = min(float(np.linalg.norm((r["L"] - r["R"])[[0, 2]])) for r in rows)
    return {
        "range": {s: round(max(a) - min(a), 3) for s, a in along.items()},
        "closest_feet": round(gap, 3),
        "lowest": {s: round(v, 3) for s, v in lowest.items()},
    }


# --- The vault --------------------------------------------------------------------

# A speed vault over waist-high cover, keyed over a vault's rise (the game
# seeks it by `MoveState`'s climb progress, 0..1): the run's last stride
# drives the right knee up, the left hand plants on the top (posed by the
# game, `shooter_sim::hitbox::action::vault_plant`), the hips turn and roll
# toward the hand while both legs tuck and swing over on the right, and the
# feet reach down ahead for the landing. Positions are in the character's
# own space (+Z forward, +X its left, Y up from the ground the capsule
# stands on), which rises with the capsule: the body tucks its feet up
# instead of rising itself. Each key: progress, the hips' position, their
# turn to the right, forward pitch, and roll (left side down), degrees; the
# spine's extra forward bend and lean to the left; and each ankle's target.
VAULT_KEYS = [
    # progress, hips, (yaw, pitch, roll), (bend, lean), ankle L, ankle R
    (0.0, (0.005, 0.877, -0.086), (0.0, 0.0, 0.0), (0.0, 0.0), (0.195, 0.103, 0.127), (-0.162, 0.104, -0.267)),
    (0.25, (-0.02, 0.92, 0.05), (8.0, 20.0, 8.0), (10.0, 5.0), (0.08, 0.18, -0.15), (-0.12, 0.46, 0.18)),
    (0.55, (-0.08, 0.95, 0.10), (25.0, 25.0, 18.0), (15.0, 12.0), (-0.16, 0.50, 0.08), (-0.32, 0.56, 0.16)),
    (0.8, (-0.06, 0.92, 0.12), (15.0, 10.0, 12.0), (8.0, 8.0), (-0.08, 0.38, 0.36), (-0.25, 0.44, 0.44)),
    (1.0, (-0.02, 0.90, 0.05), (5.0, 0.0, 4.0), (2.0, 2.0), (0.08, 0.18, 0.16), (-0.12, 0.26, 0.30)),
]
# Frames in the vault clip (it is seeked, never played at its own rate).
VAULT_FRAMES = 31
# The foot points its toes down this far while tucked (degrees, at the
# middle key), so the boot does not stick out flat over the cover.
VAULT_TOE_DOWN = 35.0
# Turning a foot about its knee's hinge by a positive angle lifts the toes.
TOE_SIGN = -1.0


def keyed(t: float, values: list[tuple[float, np.ndarray]]) -> np.ndarray:
    """`values` (progress, value) at `t`, eased between keys (smoothstep)."""
    if t <= values[0][0]:
        return values[0][1]
    for (t0, v0), (t1, v1) in zip(values, values[1:]):
        if t <= t1:
            u = (t - t0) / max(t1 - t0, 1e-6)
            u = u * u * (3.0 - 2.0 * u)
            return v0 + (v1 - v0) * u
    return values[-1][1]


class Authored:
    """A clip made here: its channel order and length, as `write` needs."""

    def __init__(self, order: list[tuple[int, str, str]], length: float):
        self.order = order
        self.length = length


def author_vault(skel: Skeleton, base: Clip) -> tuple[list[dict], float]:
    """The vault clip, from the idle's first pose, where a mantle's climb
    starts too, so both blend in from the ground the same way (the arms are
    the game's: it holds the weapon and plants the hand itself). The first
    key is that pose. Returns one pose per frame and the worst IK miss (m)."""
    hips_i = skel.index["DEF-hips"]
    legs = {s: [skel.index[f"DEF-thigh.{s}"], skel.index[f"DEF-shin.{s}"], skel.index[f"DEF-foot.{s}"]] for s in "LR"}
    spine = [skel.index[n] for n in ("DEF-spine.001", "DEF-spine.002", "DEF-spine.003")]
    start = base.sample(0.0)
    g = skel.globals(start)
    parent_r, parent_t = g[skel.parent[hips_i]]

    def column(i: int) -> list[tuple[float, np.ndarray]]:
        return [(k[0], np.asarray(k[i], dtype=np.float64)) for k in VAULT_KEYS]

    hips_keys, turn_keys, spine_keys = column(1), column(2), column(3)
    ankle_keys = {"L": column(4), "R": column(5)}
    x, y, z = np.eye(3)
    frames = []
    worst = 0.0
    for k in range(VAULT_FRAMES):
        t = k / (VAULT_FRAMES - 1)
        pose = dict(start)
        turn_right, pitch, roll = np.radians(keyed(t, turn_keys))
        bend, lean = np.radians(keyed(t, spine_keys))
        # Forward pitch tips the top toward +Z (about +X); a roll with the
        # left side down tips it toward +X (about -Z).
        turn = yaw(turn_right) @ axis_angle(x, pitch) @ axis_angle(z, -roll)
        new_rot: dict[int, np.ndarray] = {hips_i: turn @ g[hips_i][0]}
        hips_t = keyed(t, hips_keys)
        pose[(hips_i, "translation")] = parent_r.T @ (hips_t - parent_t)
        # The spine bends on over the hips, shared up its three bones.
        bend_spine = axis_angle(x, bend / 3.0) @ axis_angle(z, -lean / 3.0)
        above = turn
        for i in spine:
            above = bend_spine @ above
            new_rot[i] = above @ g[i][0]
        toe = math.radians(VAULT_TOE_DOWN) * math.sin(math.pi * t)
        for s in "LR":
            thigh, shin, foot = legs[s]
            _, thigh_t = skel.local(pose, thigh)
            hip = new_rot[hips_i] @ thigh_t + hips_t
            knee = hip + turn @ (g[shin][1] - g[thigh][1])
            ankle = knee + turn @ (g[foot][1] - g[shin][1])
            hinge = turn @ g[thigh][0][:, 0]
            thigh_turn, shin_turn, miss = solve_leg(hip, knee, ankle, hinge, keyed(t, ankle_keys[s]))
            worst = max(worst, miss)
            new_rot[thigh] = thigh_turn @ turn @ g[thigh][0]
            new_rot[shin] = shin_turn @ turn @ g[shin][0]
            # The foot rides with the shin, toes pointed down while tucked
            # (about the knee's hinge as the leg now lies).
            across = thigh_turn @ hinge
            across = across / np.linalg.norm(across)
            new_rot[foot] = axis_angle(across, TOE_SIGN * toe) @ shin_turn @ turn @ g[foot][0]
        out = dict(pose)
        for i in sorted(new_rot, key=lambda j: depth(skel, j)):
            parent = parent_rotation(skel, pose, new_rot, i)
            out[(i, "rotation")] = mat_to_quat(parent.T @ new_rot[i])
        frames.append(out)
    return frames, worst


# --- Prone -----------------------------------------------------------------------

# Lying on the belly, chest propped up on the elbows, as Black Ops lies
# prone: the eyes sit over the player's origin (the first-person camera and
# where shots leave from) and the legs trail 1.4 m behind it, as the IW
# engine's prone body does (its collision box covers the upper body; the
# legs are checked behind it). The game's IK puts the arms on the weapon at
# the eyes. Positions are character space (+Z forward, +X its left).
#
# `Prone` holds the pose. `Prone_Crawl` is the low crawl, one stride per
# second of its timeline (left knee drawn up at phase 0): each knee in turn
# draws up and out to the side, digs in, and pushes the body forward while
# the other leg straightens behind, the hips rolling up on the drawn side.
#
# The other directions, as the IW engine's prone has a crawl for each
# (`prone_crawl_forward`, `_back`, `_left`, `_right`), on the same stride
# phase so they blend:
# - `Prone_Crawl_B` is the forward crawl run backward: each knee draws up
#   with its toes dug in and pulls the body back over them.
# - `Prone_Crawl_R` hitches the body to its right: the right knee draws up
#   and out to the side twice a stride and pushes the hips over it, while
#   the left leg drags straight behind and swings back in under the body,
#   the hips fishtailing toward the drawn knee. `Prone_Crawl_L` mirrors it.
PRONE_FRAMES = 33
# Torso: the hips' forward pitch (90 is flat), and how far the spine bends
# back up over its three bones and the neck and head lift to look ahead,
# degrees.
PRONE_HIPS_PITCH = 88.0
PRONE_SPINE_LIFT = 20.0
PRONE_NECK_LIFT = 25.0
PRONE_HEAD_LIFT = 38.0
# Hips position (metres): height off the ground; how far behind the origin
# is solved so the eyes sit over it.
PRONE_HIPS_HEIGHT = 0.17
# Where the eyes are in the head bone's frame (as `carry::EYE_IN_HEAD`).
EYE_IN_HEAD = np.array([0.0, 0.15, 0.09])
# Legs at rest: ankles this far out to each side and this far behind the
# hips, near the ground (toes dug in).
PRONE_ANKLE = np.array([0.2, -0.05, -0.77])
# The crawl: a stride of this length (m, one push per leg), the drawn-up
# ankle's place (out, up, back from the hips) and how far the thigh turns
# out to bring the knee to the side (degrees), the hips' roll up on the
# drawn side and yaw toward the pushing leg (degrees), and their sway (m).
CRAWL_STRIDE = 0.7
CRAWL_DRAWN = np.array([0.24, -0.02, -0.62])
CRAWL_TURN_OUT = 85.0
CRAWL_ROLL = 9.0
CRAWL_YAW = 6.0
CRAWL_SWAY = 0.035


# The side crawl: the leading knee drawn up this far out, up, and back from
# the hips (the right leg's, as x is +left), the trailing ankle's sweep
# across (out from its rest at the push's end, m), and the hips' fishtail
# yaw, roll up on the leading side (degrees), and sway along the travel (m).
SIDE_DRAWN = np.array([-0.4, -0.02, -0.52])
SIDE_TRAIL_SWEEP = 0.12
SIDE_YAW = 10.0
SIDE_ROLL = 7.0
SIDE_SWAY = 0.04
# Directions of the prone crawl: name suffix (forward has none), and the
# travel's angle from forward toward the character's right (degrees).
PRONE_DIRECTIONS = (("", 0.0), ("_R", 90.0), ("_B", 180.0), ("_L", -90.0))


def crawl_leg(phase: float, drawn: np.ndarray = CRAWL_DRAWN) -> tuple[np.ndarray, float]:
    """One leg's ankle target relative to the hips (left side; the right
    mirrors x) and its turn-out at `phase` of the stride: drawn up at 0,
    pushing back along the ground to straight by 0.5 (planted, so it moves
    back at the body's speed), then sliding up again, lifted a little."""
    phase %= 1.0
    if phase < 0.5:
        u = phase / 0.5
        ankle = drawn + (PRONE_ANKLE - drawn) * u
        out = CRAWL_TURN_OUT * (1.0 - u) ** 1.5
    else:
        u = (phase - 0.5) / 0.5
        e = u * u * (3.0 - 2.0 * u)
        ankle = PRONE_ANKLE + (drawn - PRONE_ANKLE) * e + np.array([0.0, 0.05 * math.sin(math.pi * u), 0.0])
        out = CRAWL_TURN_OUT * e
    return ankle, out


MIRROR = np.array([-1.0, 1.0, 1.0])


def crawl_targets(t: float, direction: float) -> tuple[dict, float, float, np.ndarray]:
    """The crawl at phase `t` travelling `direction` degrees right of
    forward (0, 90, 180, or -90): each leg's ankle target relative to the
    hips and turn-out, the hips' roll (left side up) and yaw (to the right),
    degrees, and their sway (m, character space)."""
    if direction in (0.0, 180.0):
        # Backward, the forward crawl runs in reverse.
        p = t if direction == 0.0 else -t
        left, left_out = crawl_leg(p)
        right, right_out = crawl_leg(p + 0.5)
        side = math.cos(math.tau * t)  # 1: left drawn up, -1: right
        sway = np.array([CRAWL_SWAY * side, 0.0, 0.0])
        return {"L": (left, left_out), "R": (right * MIRROR, right_out)}, CRAWL_ROLL * side, -CRAWL_YAW * side, sway
    # Sideways (worked out for the right; the left mirrors it): two hitches
    # a stride, the leading knee drawn up at each one's start.
    h = (2.0 * t) % 1.0
    lead, lead_out = crawl_leg(h, SIDE_DRAWN * MIRROR)
    lead = lead * MIRROR
    # The trailing leg drags out from under the body while the hips push
    # over, then swings back in, lifted a little.
    if h < 0.5:
        across = SIDE_TRAIL_SWEEP * (h / 0.5)
        lift = 0.0
    else:
        u = (h - 0.5) / 0.5
        across = SIDE_TRAIL_SWEEP * (1.0 - u * u * (3.0 - 2.0 * u))
        lift = 0.04 * math.sin(math.pi * u)
    trail = PRONE_ANKLE + np.array([across - 0.06, lift, 0.04])
    drawn = math.cos(math.tau * h)  # 1: the leading knee drawn up
    roll, yaw_r = -SIDE_ROLL * drawn, -SIDE_YAW * drawn
    sway = np.array([SIDE_SWAY * drawn, 0.0, 0.0])
    legs = {"R": (lead, lead_out), "L": (trail, 0.0)}
    if direction < 0.0:
        legs = {"L": (legs["R"][0] * MIRROR, legs["R"][1]), "R": (legs["L"][0] * MIRROR, legs["L"][1])}
        roll, yaw_r, sway = -roll, -yaw_r, sway * MIRROR
    return legs, roll, yaw_r, sway


def author_prone(
    skel: Skeleton, base: Clip, crawl: bool, direction: float = 0.0
) -> tuple[list[dict], float, np.ndarray]:
    """The prone pose (`crawl` False) or the low crawl travelling
    `direction` degrees right of forward, from the idle's first pose.
    Returns one pose per frame, the worst IK miss (m), and the eyes' mean
    position."""
    hips_i = skel.index["DEF-hips"]
    legs = {s: [skel.index[f"DEF-thigh.{s}"], skel.index[f"DEF-shin.{s}"], skel.index[f"DEF-foot.{s}"]] for s in "LR"}
    spine = [skel.index[n] for n in ("DEF-spine.001", "DEF-spine.002", "DEF-spine.003")]
    neck, head = skel.index["DEF-neck"], skel.index["DEF-head"]
    start = base.sample(0.0)
    g = skel.globals(start)
    parent_r, parent_t = g[skel.parent[hips_i]]
    x, y, z = np.eye(3)

    def pose_at(t: float, hips_z: float) -> tuple[dict, float, np.ndarray]:
        pose = dict(start)
        if crawl:
            ankles, roll, yaw_r, sway = crawl_targets(t, direction)
        else:
            ankles = {"L": (PRONE_ANKLE.copy(), 0.0), "R": (PRONE_ANKLE * MIRROR, 0.0)}
            roll = yaw_r = 0.0
            sway = np.zeros(3)
        # Roll the drawn side's hip up (left up tips the top toward -X).
        turn = yaw(math.radians(yaw_r)) @ axis_angle(x, math.radians(PRONE_HIPS_PITCH)) @ axis_angle(y, math.radians(-roll))
        new_rot: dict[int, np.ndarray] = {hips_i: turn @ g[hips_i][0]}
        hips_t = np.array([0.0, PRONE_HIPS_HEIGHT, hips_z]) + sway
        pose[(hips_i, "translation")] = parent_r.T @ (hips_t - parent_t)
        # The spine lifts the chest back up and turns back against the
        # hips' yaw, so the shoulders stay square to the aim.
        above = turn
        for i in spine:
            above = yaw(math.radians(-yaw_r / 3.0)) @ axis_angle(x, math.radians(-PRONE_SPINE_LIFT / 3.0)) @ above
            new_rot[i] = above @ g[i][0]
        above = axis_angle(x, math.radians(-PRONE_NECK_LIFT)) @ above
        new_rot[neck] = above @ g[neck][0]
        above = axis_angle(x, math.radians(-PRONE_HEAD_LIFT)) @ above
        new_rot[head] = above @ g[head][0]
        worst = 0.0
        for s in "LR":
            thigh, shin, foot = legs[s]
            _, thigh_t = skel.local(pose, thigh)
            hip = new_rot[hips_i] @ thigh_t + hips_t
            target, out = ankles[s]
            target = hips_t + target
            # The leg turns out about its own line (knee to the side) before
            # it bends, so the knee draws up along the ground.
            knee0 = hip + turn @ (g[shin][1] - g[thigh][1])
            ankle0 = knee0 + turn @ (g[foot][1] - g[shin][1])
            line = (ankle0 - hip) / np.linalg.norm(ankle0 - hip)
            sign = -1.0 if s == "L" else 1.0
            twist = axis_angle(line, sign * math.radians(out))
            knee = hip + twist @ (knee0 - hip)
            ankle = hip + twist @ (ankle0 - hip)
            hinge = twist @ turn @ g[thigh][0][:, 0]
            thigh_turn, shin_turn, miss = solve_leg(hip, knee, ankle, hinge, target)
            worst = max(worst, miss)
            new_rot[thigh] = thigh_turn @ twist @ turn @ g[thigh][0]
            new_rot[shin] = shin_turn @ twist @ turn @ g[shin][0]
            new_rot[foot] = shin_turn @ twist @ turn @ g[foot][0]
        out_pose = dict(pose)
        for i in sorted(new_rot, key=lambda j: depth(skel, j)):
            parent = parent_rotation(skel, pose, new_rot, i)
            out_pose[(i, "rotation")] = mat_to_quat(parent.T @ new_rot[i])
        hg = skel.globals(out_pose)[head]
        eyes = hg[1] + hg[0] @ EYE_IN_HEAD
        return out_pose, worst, eyes

    # Put the eyes over the origin: measure them with the hips at 0, then
    # move the hips back by that much.
    _, _, eyes = pose_at(0.0, 0.0)
    hips_z = -float(eyes[2])
    frames, worst, seen = [], 0.0, []
    count = PRONE_FRAMES if crawl else 2
    for k in range(count):
        t = k / (count - 1)
        pose, miss, eyes = pose_at(t, hips_z)
        frames.append(pose)
        worst = max(worst, miss)
        seen.append(eyes)
    return frames, worst, np.mean(seen, axis=0)


def write(src: Gltf, clips: list[tuple[str, list[dict], Clip]], path: Path) -> None:
    w = Writer()
    copy_rig(src, w)
    root = src.json["skins"][0]["joints"][0]
    mesh_node = next(n["name"] for n in src.json["nodes"] if "mesh" in n)
    attach_mesh(w, src, mesh_node, write_stub_mesh(w, src.json["skins"][0]["joints"].index(root)), "Rig")
    animations = []
    for name, frames, base in clips:
        times = np.linspace(0.0, base.length, len(frames), dtype=np.float32).reshape(-1, 1)
        time_acc = w.add(times, bounds=True)
        samplers, channels = [], []
        for node, path_, interp in base.order:
            values = np.array([f[(node, path_)] for f in frames], dtype=np.float32)
            samplers.append({"input": time_acc, "output": w.add(values), "interpolation": interp})
            channels.append({"sampler": len(samplers) - 1, "target": {"node": node, "path": path_}})
        animations.append({"name": name, "samplers": samplers, "channels": channels})
    w.doc["animations"] = animations
    w.save(path)


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--source", type=Path, default=ANIMATIONS / "ual1.glb")
    parser.add_argument("--out", type=Path, default=ANIMATIONS / "gait.glb")
    parser.add_argument("--report", action="store_true", help="print each cycle's foot measurements")
    args = parser.parse_args(argv)
    src = Gltf(args.source)
    skel = Skeleton(src)
    clips = []
    for spec in BASES:
        base = Clip(src, spec.clip)
        for label, direction, hip_share, stance_share in DIRECTIONS:
            if spec.only and label not in spec.only:
                continue
            frames, worst = author(skel, base, spec, direction, hip_share, stance_share)
            if worst > 0.002:
                # A foot out of reach: the hips sit that much lower through
                # the whole cycle, so the stride bobs as before.
                frames, worst = author(skel, base, spec, direction, hip_share, stance_share, worst + 0.003)
            if worst > 0.005 and not args.report:
                raise SystemExit(f"{spec.name}_{label}: IK misses its foot target by {worst * 100:.1f} cm")
            if args.report:
                print(f"{spec.name}_{label} miss {worst * 100:.1f} cm", check(skel, frames, spec, direction))
            clips.append((f"{spec.name}_{label}", frames, base))
    idle = Clip(src, "Idle_Loop")
    frames, worst = author_vault(skel, idle)
    if worst > 0.005 and not args.report:
        raise SystemExit(f"Vault: IK misses its foot target by {worst * 100:.1f} cm")
    if args.report:
        print(f"Vault miss {worst * 100:.1f} cm")
    clips.append(("Vault", frames, Authored(idle.order, 1.0)))
    prone = [("Prone", False, 0.0)] + [(f"Prone_Crawl{suffix}", True, d) for suffix, d in PRONE_DIRECTIONS]
    for name, crawl, direction in prone:
        frames, worst, eyes = author_prone(skel, idle, crawl, direction)
        if worst > 0.005 and not args.report:
            raise SystemExit(f"{name}: IK misses its foot target by {worst * 100:.1f} cm")
        if args.report:
            print(f"{name} miss {worst * 100:.1f} cm, eyes {np.round(eyes, 3)}")
        clips.append((name, frames, Authored(idle.order, 1.0)))
    write(src, clips, args.out)


if __name__ == "__main__":
    main()
