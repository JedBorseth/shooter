"""Checks on Slamcity's layout (tools/slamcity_layout.py), run when the scene is
written (tools/build_slamcity.py) and in CI (tools/test_slamcity_layout.py):

- collision is the same on both halves, turned half round the centre;
- no spawn point is in sight of the enemy's spawns, upper floor, platform,
  train, or roof terrace;
- nothing at eye height on the ground runs clear for more than 50 m, nor for
  more than 50 m between the two teams' high ground, nor for more than 35 m from
  a platform down onto its own side's streets;
- every Hardpoint move goes at least 16 m to a zone that does not overlap the
  last one seen from above;
- if the enemy gets onto our upper floor, roof terrace, or platform, or into
  our half's streets and rooms, some spawn points are still out of their
  sight, so the server always has somewhere safe to put us.

Every check raises SystemExit with what failed. No dependencies.
"""

from __future__ import annotations

import hashlib
import json
import math
from pathlib import Path

import slamcity_layout as L

EYE = L.EYE


def turned(p):
    """A point's twin on the other team's half."""
    return (-p[0], p[1], -p[2])


def check_twin():
    """Collision must be the same on both halves: every shape has a twin
    turned half round the centre (shapes on the centre are their own twin),
    except furniture inside the rooms each half dresses as a different place
    (L.DRESSED), and surfaces, which may differ."""
    def key(shape):
        kind, surface, geo = shape
        if kind == "box":
            return kind, surface, tuple(round(v, 3) + 0.0 for v in geo)
        return kind, surface, tuple(sorted(tuple(round(v, 3) + 0.0 for v in p) for p in geo))

    def twin(shape):
        kind, surface, geo = shape
        if kind == "box":
            x0, x1, y0, y1, z0, z1 = geo
            return kind, surface, (-x1, -x0, y0, y1, -z1, -z0)
        return kind, surface, [(-x, y, -z) for x, y, z in geo]

    def dressed(shape):
        """Inside a room dressed differently on each half, or the floor under one."""
        kind, _, geo = shape
        pts = [(geo[0], geo[4]), (geo[1], geo[5])] if kind == "box" else [(p[0], p[2]) for p in geo]
        xs, zs = [p[0] for p in pts], [p[1] for p in pts]
        for x0, x1, z0, z1 in L.DRESSED + [(-r[1], -r[0], -r[3], -r[2]) for r in L.DRESSED]:
            if min(xs) >= x0 - 1e-6 and max(xs) <= x1 + 1e-6 and min(zs) >= z0 - 1e-6 and max(zs) <= z1 + 1e-6:
                return True
        return False

    # Surfaces may differ (a wooden floor on one half, tiles on the other).
    def bare(k):
        return (k[0], k[2])

    def wreck(shape):
        """Part of the derailed car: one car across the centre, its torn nose
        on Red's end and its open gangway on Blue's (doors on both sides)."""
        kind, _, geo = shape
        return kind == "convex" and all(math.hypot(p[0], p[2]) < 10.2 and p[1] < 3.0 for p in geo)

    shapes = {bare(key(c)) for c in L.COLLIDERS}
    lonely = [c for c in L.COLLIDERS if not dressed(c) and not wreck(c) and bare(key(twin(c))) not in shapes]
    if lonely:
        raise SystemExit(f"{len(lonely)} collision shapes have no twin, e.g. {lonely[:3]}")


# --- Sight, quickly: only the volumes near a line are tested --------------------------
_BOUNDS: list = []


def _bounds():
    if len(_BOUNDS) != len(L.SIGHT):
        _BOUNDS.clear()
        for b in L.SIGHT:
            if b[0] == "turned":
                _, cx, cz, hx, hz, _yaw, y0, y1 = b
                r = math.hypot(hx, hz)
                _BOUNDS.append(((cx - r, cx + r, y0, y1, cz - r, cz + r), b))
            elif b[0] == "tilted":
                _BOUNDS.append(((-1e9, 1e9, -1e9, 1e9, -1e9, 1e9), b))
            else:
                _BOUNDS.append((b, b))
    return _BOUNDS


def clear(p, q) -> bool:
    """L.line_clear, skipping volumes whose bounds miss the line's own."""
    lo = [min(p[i], q[i]) for i in range(3)]
    hi = [max(p[i], q[i]) for i in range(3)]
    for (x0, x1, y0, y1, z0, z1), b in _bounds():
        if x1 < lo[0] or x0 > hi[0] or y1 < lo[1] or y0 > hi[1] or z1 < lo[2] or z0 > hi[2]:
            continue
        if L.segment_hits(p, q, b):
            return False
    return True


def free(p) -> bool:
    """Whether someone could stand with their eye at p (not inside cover)."""
    return clear(p, (p[0] + 1e-3, p[1], p[2] + 1e-3))


# --- Where each team stands --------------------------------------------------------------
def high_ground(team=0):
    """Eye points on a team's high ground: the upper floor of its tenement (Red:
    the hotel), its roof terrace, and its platform. team 0 is Blue (authored),
    1 is Red (the twins)."""
    pts = [(x, L.UPPER + EYE, z) for x in (-31.5, -28.5, -25.5, -23.0)
           for z in (-25.0, -20.0, -15.0, -10.0, -5.0, -3.2)]
    pts += [(x, L.DINER_ROOF + EYE, z) for x in (-32.0, -29.0, -26.0, -23.5) for z in (8.0, 11.0, 14.0, 20.0, 23.0)]
    pts += [(x, L.PLATFORM + EYE, z) for x in (-6.6, -4.5, -2.2) for z in range(-11, 12, 2)]
    pts = [p for p in pts if free(p)]
    return pts if team == 0 else [turned(p) for p in pts]


def streets(team=0, step=3.0):
    """Eye points over a team's half of the streets, plaza, and aprons."""
    pts = []
    for rect in (L.YARD, L.DEPOT_LANE, L.MARKET_LANE, L.NORTH_ST, L.PAWN_ALLEY, L.CORNER, L.AVENUE, L.MARKET,
                 (-13.0, 0.0, -13.0, 13.0), L.APRON):
        x0, x1, z0, z1 = rect
        x = x0 + 0.8
        while x < x1 - 0.5:
            z = z0 + 0.8
            while z < z1 - 0.5:
                kerb = rect == L.AVENUE and (z < z0 + 1.4 or z > z1 - 1.4)
                pts.append((x, (L.WALK if kerb else L.STREET) + EYE, z))
                z += step
            x += step
    pts = [p for p in pts if free(p)]
    return pts if team == 0 else [turned(p) for p in pts]


def rooms(team=0):
    """Eye points inside a team's ground floors."""
    pts = [(x, L.WALK + EYE, z) for x, z in ((-27.0, -20.0), (-25.0, -7.0), (-15.5, -5.0), (-26.0, 9.0),
                                            (-26.0, 18.0), (-17.0, 7.5), (-15.0, 19.0), (-3.0, -31.0), (5.0, -24.0))]
    pts = [p for p in pts if free(p)]
    return pts if team == 0 else [turned(p) for p in pts]


def spawn_eyes(team=0):
    pts = [(x, y + EYE, z) for x, y, z in L.spawn_points()]
    return pts if team == 0 else [turned(p) for p in pts]


# --- The checks ------------------------------------------------------------------------
def check_spawn_sightlines():
    """No Blue spawn point may be seen, at eye height, from a Red spawn or Red's
    high ground (the map is balanced by a turn, so this covers both teams)."""
    watch = spawn_eyes(1) + high_ground(1) + [(0.0, L.PLATFORM + EYE, z) for z in range(-9, 10, 3)]
    seen = [(p, q) for p in spawn_eyes(0) for q in watch if clear(p, q)]
    if seen:
        raise SystemExit(f"spawns in sight of the enemy: {seen[:5]}")


def long_lines(limit=50.0):
    pts = streets(0, step=4.0) + streets(1, step=4.0)
    out = []
    for i, p in enumerate(pts):
        for q in pts[i + 1:]:
            if math.dist(p, q) > limit and clear(p, q):
                out.append((p, q))
    return out


def check_long_sightlines(limit=50.0):
    """No clear line at eye height longer than `limit` metres across the
    streets and the plaza: something must break every long line."""
    long = long_lines(limit)
    if long:
        raise SystemExit(f"{len(long)} clear lines longer than {limit:g} m, e.g. {long[:4]}")


def check_platform_avenue(limit=35.0):
    """No clear line longer than `limit` metres from a platform down onto its
    own team's streets: the platforms command the plaza at mid range, not the
    streets out to the spawn yards."""
    platform = [p for p in high_ground(0) if abs(p[1] - (L.PLATFORM + EYE)) < 0.01]
    down = [p for p in streets(0) if math.dist((p[0], p[2]), (0.0, 0.0)) > 14.0]
    long = [(p, q) for p in platform for q in down if math.dist(p, q) > limit and clear(p, q)]
    if long:
        raise SystemExit(f"{len(long)} clear lines over {limit:g} m from a platform down a street, e.g. {long[:3]}")


def check_hardpoint_hops(minimum=16.0):
    """Each Hardpoint move, the last back to the first included, goes at least
    `minimum` metres across the map (height does not count), to a zone whose
    footprint seen from above does not touch the last one's: the fight moves,
    not a room over or a floor up."""
    zones = L.HARDPOINTS
    for (name_a, _, ca, sa), (name_b, _, cb, sb) in zip(zones, zones[1:] + zones[:1]):
        hop = math.dist((ca[0], ca[2]), (cb[0], cb[2]))
        overlap = all(abs(ca[i] - cb[i]) < (sa[i] + sb[i]) / 2 for i in (0, 2))
        if hop < minimum or overlap:
            raise SystemExit(f"Hardpoint {name_a} -> {name_b} moves {hop:.1f} m"
                             + (" onto an overlapping footprint" if overlap else ""))


def through_train_windows(p, q) -> bool:
    """Whether the line p-q crosses x = 0 inside the stalled train, at window
    height: it threads both rows of windows."""
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
    long = [(p, q) for p in high_ground(0) for q in high_ground(1)
            if math.dist(p, q) > limit and clear(p, q) and not through_train_windows(p, q)]
    if long:
        raise SystemExit(f"{len(long)} clear lines over {limit:g} m between the teams' high ground, e.g. {long[:3]}")


def check_home_perches(minimum=10):
    """If the enemy takes our own high ground, at least `minimum` of our spawn
    points must still be out of their sight, so the server always has somewhere
    safe to spawn us (tdm::choose_spawn picks the safest)."""
    perches = high_ground(0)
    hidden = [p for p in spawn_eyes(0) if all(not clear(p, q) for q in perches)]
    if len(hidden) < minimum:
        raise SystemExit(f"only {len(hidden)} spawn points hide from our own high ground")
    return len(hidden)


def enemy_incursions():
    """Points in the middle of our own half the enemy can reach once they push
    in: our high ground, our streets east of the depot and the avenue's mouth,
    and the rooms we do not spawn in."""
    deep = [p for p in streets(0) if p[0] > -30.0]
    rooms_ = [p for p in rooms(0) if not (L.TENEMENT[0] < p[0] < L.TENEMENT[1] and L.TENEMENT[2] < p[2] < L.TENEMENT[3])
              and not (L.DINER[0] < p[0] < L.DINER[1] and L.DINER[2] < p[2] < L.DINER[3])]
    return high_ground(0) + deep + rooms_


def check_incursions(minimum=6):
    """At least `minimum` spawn points stay hidden even with the enemy pushed
    into the middle of our half (enemy_incursions): somewhere safe remains."""
    watch = enemy_incursions()
    hidden = [p for p in spawn_eyes(0) if all(not clear(p, q) for q in watch)]
    if len(hidden) < minimum:
        raise SystemExit(f"only {len(hidden)} spawn points hide from an enemy inside our half")
    return len(hidden)


def layout_hash() -> str:
    """A hash of everything the scene takes from the layout but the meshes:
    collision, spawns, flags, lanes, zones, charge sites, targets, and effects."""
    data = [L.COLLIDERS, L.spawn_points(), L.FLAG, L.BOT_LANES, L.HARDPOINTS, L.TARGETS, L.EFFECTS,
            L.CHARGE_SITES, L.CHARGE_CRATE]
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
    check_twin()
    check_spawn_sightlines()
    check_long_sightlines()
    check_perch_duels()
    check_platform_avenue()
    check_hardpoint_hops()
    return check_home_perches(), check_incursions()
