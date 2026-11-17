"""Checks on Slamcity's layout (tools/slamcity_layout.py), run when the scene is
written (tools/build_slamcity.py) and in CI (tools/test_slamcity_layout.py):

- collision is the same on both halves;
- no spawn point is in sight of the enemy's spawns, roof, platform, train, or
  diner roof;
- nothing runs clear for more than 60 m at eye height down the avenue or the
  alley, for more than 50 m between the two teams' high ground, or for more
  than 35 m from a platform down onto the avenue;
- every Hardpoint move goes at least 16 m to a zone that does not overlap the
  last one seen from above;
- if the enemy gets onto our own roof, platform, or diner roof, or into our
  half of the street, alley, diner, or laundromat, some spawn points are still
  out of their sight, so the server always has somewhere safe to put us.

Every check raises SystemExit with what failed. No dependencies.
"""

from __future__ import annotations

import hashlib
import json
import math
from pathlib import Path

import slamcity_layout as L

EYE = L.EYE


def check_mirror():
    """Collision must be the same on both halves: every shape has a twin
    mirrored across x = 0 (shapes on the mirror line are their own twin)."""
    def key(shape):
        kind, surface, geo = shape
        if kind == "box":
            return kind, surface, tuple(round(v, 3) for v in geo)
        return kind, surface, tuple(sorted(tuple(round(v, 3) + 0.0 for v in p) for p in geo))

    def mirrored(shape):
        kind, surface, geo = shape
        if kind == "box":
            x0, x1, y0, y1, z0, z1 = geo
            return kind, surface, (-x1, -x0, y0, y1, z0, z1)
        return kind, surface, [(-x, y, z) for x, y, z in geo]

    shapes = {key(c) for c in L.COLLIDERS}
    lonely = [c for c in L.COLLIDERS if key(mirrored(c)) not in shapes]
    if lonely:
        raise SystemExit(f"{len(lonely)} collision shapes have no mirror twin, e.g. {lonely[:3]}")


def check_spawn_sightlines():
    """No west spawn point may be seen, at eye height, from an east spawn, the
    east tenement's roof, the east platform, the train, or the east diner's
    roof (the map is mirrored, so this covers both teams)."""
    eye = EYE
    spawns = [(x, y + eye, z) for x, y, z in L.spawn_points()]
    watch = [(-x, y + eye, z) for x, y, z in L.spawn_points()]
    watch += [(x, L.TENEMENT_ROOF + eye, z) for x in range(23, 48, 3) for z in (-12.0, -16.5, -21.0)]
    watch += [(x, L.PLATFORM + eye, z) for x in (2.2, 4.5, 6.8) for z in range(-11, 12, 2)]
    watch += [(0.0, L.PLATFORM + eye, z) for z in range(-9, 10, 3)]
    watch += [(x, L.DINER_ROOF + eye, z) for x in (21.0, 25.0, 30.0, 35.0) for z in (12.6, 17.0, 22.0)]
    clear = [(p, q) for p in spawns for q in watch if L.line_clear(p, q)]
    if clear:
        raise SystemExit(f"spawns in sight of the enemy: {clear[:5]}")


def check_long_sightlines(limit=60.0):
    """No clear line at eye height longer than `limit` metres down the avenue
    (kerb to kerb) or the alley: something must break every long line."""
    eye = EYE
    clear = []
    for z_lo, z_hi, y in ((-11.0, 11.0, eye + L.WALK), (-31.8, -22.2, eye)):
        zs = [z_lo + i * 0.25 for i in range(int((z_hi - z_lo) / 0.25) + 1)]
        for x0 in (-47.0, -40.0, -33.0):
            for za in zs:
                for zb in zs:
                    if abs(za - zb) > 4.0:
                        continue
                    for x1 in (-x0, -x0 - 7.0, -x0 + 7.0):
                        if abs(x1 - x0) < limit:
                            continue
                        p, q = (x0, y, za), (x1, y, zb)
                        if L.line_clear(p, q):
                            clear.append((p, q))
    if clear:
        raise SystemExit(f"{len(clear)} clear lines longer than {limit:g} m, e.g. {clear[:4]}")


def check_platform_avenue(limit=35.0):
    """No clear line longer than `limit` metres from a platform down onto its
    own side of the avenue: the platforms command the street at mid range, not
    its whole length out to the spawn yards. Points inside cover (the kiosk, a
    shelter) are skipped: nobody stands there."""
    eye = EYE

    def free(p):
        return not any(L.segment_hits(p, p, b) for b in L.SIGHT)

    platform = [(x, L.PLATFORM + eye, z) for x in (-6.6, -4.5, -2.2) for z in range(-11, 12, 1)]
    avenue = [(x, L.WALK + eye, z) for x in range(-50, -9, 3) for z in range(-9, 10, 2)]
    clear = [(p, q) for p in platform if free(p) for q in avenue
             if math.dist(p, q) > limit and L.line_clear(p, q)]
    if clear:
        raise SystemExit(f"{len(clear)} clear lines over {limit:g} m from a platform down the avenue, e.g. {clear[:3]}")


def check_hardpoint_hops(minimum=16.0):
    """Each Hardpoint move, the last back to the first included, goes at least
    `minimum` metres across the map (height does not count), to a zone whose footprint seen from above does not touch
    the last one's: the fight moves, not a room over or a floor up."""
    zones = L.HARDPOINTS
    for (name_a, _, ca, sa), (name_b, _, cb, sb) in zip(zones, zones[1:] + zones[:1]):
        hop = math.dist((ca[0], ca[2]), (cb[0], cb[2]))  # across the map, not up a floor
        overlap = all(abs(ca[i] - cb[i]) < (sa[i] + sb[i]) / 2 for i in (0, 2))
        if hop < minimum or overlap:
            raise SystemExit(f"Hardpoint {name_a} -> {name_b} moves {hop:.1f} m"
                             + (" onto an overlapping footprint" if overlap else ""))


def check_home_perches(minimum=15):
    """If the enemy takes our own roof, platform, or diner roof, at least `minimum` of our
    spawn points must still be out of their sight, so the server always has
    somewhere safe to spawn us (tdm::choose_spawn picks the safest)."""
    eye = EYE
    spawns = [(x, y + eye, z) for x, y, z in L.spawn_points()]
    perches = [(x, L.TENEMENT_ROOF + eye, z) for x in range(-46, -22, 3) for z in (-12.0, -16.5, -21.0)]
    perches += [(x, L.PLATFORM + eye, z) for x in (-6.8, -4.5, -2.2) for z in range(-11, 12, 2)]
    perches += [(x, L.DINER_ROOF + eye, z) for x in (-35.0, -30.0, -25.0, -21.0) for z in (12.6, 17.0, 22.0)]
    hidden = [p for p in spawns if all(not L.line_clear(p, q) for q in perches)]
    if len(hidden) < minimum:
        raise SystemExit(f"only {len(hidden)} spawn points hide from our own roof and platform")
    return len(hidden)


def high_ground(sign=1):
    """Eye points on one team's high ground: the tenement roof, the platform,
    and the diner roof (sign=1 west, -1 east)."""
    eye = EYE
    pts = [(sign * x, L.TENEMENT_ROOF + eye, z) for x in range(-46, -22, 3) for z in (-12.0, -16.5, -21.0)]
    pts += [(sign * x, L.PLATFORM + eye, z) for x in (-6.8, -4.5, -2.2) for z in range(-11, 12, 2)]
    pts += [(sign * x, L.DINER_ROOF + eye, z) for x in (-35.0, -30.0, -25.0, -21.0) for z in (12.6, 17.0, 22.0)]
    return pts


def through_train_windows(p, q) -> bool:
    """Whether the line p-q crosses the mirror line inside the stalled train,
    at window height: it threads both rows of windows."""
    if (p[0] > 0) == (q[0] > 0):
        return False
    t = p[0] / (p[0] - q[0])
    y = p[1] + t * (q[1] - p[1])
    z = p[2] + t * (q[2] - p[2])
    low, high = L.PLATFORM + 0.88, L.PLATFORM + 1.92
    return abs(z) < L.TRAIN_HALF and low <= y <= high


def check_perch_duels(limit=50.0):
    """No clear line longer than `limit` metres between the two teams' high
    ground: perches fight at mid range, not across the whole map. Lines that
    thread both rows of the stalled train's windows are allowed: keyholes a
    sniper has to find, not open lanes."""
    long = [(p, q) for p in high_ground(1) for q in high_ground(-1)
            if math.dist(p, q) > limit and L.line_clear(p, q) and not through_train_windows(p, q)]
    if long:
        raise SystemExit(f"{len(long)} clear lines over {limit:g} m between the teams' high ground, e.g. {long[:3]}")


def enemy_incursions():
    """Points in our own half that the enemy can reach: our roof, platform, and
    diner roof, and our half of the avenue, the alley, the diner, and the
    laundromat."""
    eye = EYE
    points = [(x, L.TENEMENT_ROOF + eye, z) for x in range(-46, -22, 3) for z in (-12.0, -16.5, -21.0)]
    points += [(x, L.PLATFORM + eye, z) for x in (-6.8, -4.5, -2.2) for z in range(-11, 12, 2)]
    points += [(x, L.DINER_ROOF + eye, z) for x in (-35.0, -30.0, -25.0, -21.0) for z in (12.6, 17.0, 22.0)]
    points += [(x, eye, z) for x in (-40.0, -30.0, -20.0) for z in (-6.0, 0.0, 6.0)]
    points += [(x, eye, z) for x in (-40.0, -30.0, -20.0) for z in (-30.0, -26.0, -23.0)]
    points += [(x, L.WALK + eye, z) for x in (-34.0, -28.0, -22.0) for z in (14.0, 20.0, 22.5)]
    points += [(x, L.WALK + eye, z) for x in (-18.0, -12.0) for z in (13.0, 20.0)]
    return points


def check_incursions(minimum=8):
    """At least `minimum` spawn points stay hidden even with the enemy in our
    half (enemy_incursions): somewhere safe always remains."""
    spawns = [(x, y + EYE, z) for x, y, z in L.spawn_points()]
    watch = enemy_incursions()
    hidden = [p for p in spawns if all(not L.line_clear(p, q) for q in watch)]
    if len(hidden) < minimum:
        raise SystemExit(f"only {len(hidden)} spawn points hide from an enemy inside our half")
    return len(hidden)


def layout_hash() -> str:
    """A hash of everything the scene takes from the layout but the meshes:
    collision, spawns, flags, lanes, zones, targets, and effects."""
    data = [L.COLLIDERS, L.spawn_points(), L.FLAG, L.BOT_LANES, L.HARDPOINTS, L.TARGETS, L.EFFECTS]
    blob = json.dumps(data, sort_keys=True, default=list, separators=(",", ":"))
    return hashlib.sha256(blob.encode()).hexdigest()[:16]


MESH_HASH = Path(__file__).resolve().parent.parent / "godot/assets/maps/slamcity/city.hash"
BAKE_HASH = Path(__file__).resolve().parent.parent / "godot/scenes/slamcity.bake.hash"


def mesh_hash() -> str:
    """A hash of everything city.gltf is built from: the layout's parts and
    placed models, and the model, look, builder, and sign atlas sources.
    tools/build_slamcity_city.py writes it beside the mesh (MESH_HASH)."""
    tools = Path(__file__).resolve().parent
    data = json.dumps([L.PARTS, L.PLACED], sort_keys=True, default=list, separators=(",", ":"))
    digest = hashlib.sha256(data.encode())
    for name in ("slamcity_models.py", "world_looks.py", "build_slamcity_city.py"):
        digest.update((tools / name).read_bytes())
    digest.update((tools.parent / "godot/assets/textures/slamcity/signs.json").read_bytes())
    return digest.hexdigest()[:16]


def run_all():
    """Run every check; returns (spawns hidden from our own high ground,
    spawns hidden from an enemy inside our half)."""
    L.build()
    check_mirror()
    check_spawn_sightlines()
    check_long_sightlines()
    check_perch_duels()
    check_platform_avenue()
    check_hardpoint_hops()
    return check_home_perches(), check_incursions()
