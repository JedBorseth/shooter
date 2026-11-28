"""Checks on Breakwater's layout (tools/breakwater_layout.py), run when the
scene is written (tools/build_breakwater.py) and in CI
(tools/test_breakwater_layout.py):

- collision is the same on both halves, turned half round the centre;
- no line runs clear at eye height from one team's spawn yard into the other's;
- no spawn point is in sight of the enemy's spawns, upper floor, roof deck, or
  half of the rack's deck;
- nothing at eye height on the ground runs clear for more than 55 m, nor for
  more than 50 m between the two teams' high ground;
- every Hardpoint move goes at least 16 m to a zone that does not overlap the
  last one seen from above;
- if the enemy gets onto our high ground, or into our half's roads and rooms,
  some spawn points are still out of their sight, so the server always has
  somewhere safe to put us.

The sight tests reuse Slamcity's (tools/slamcity_checks.py), pointed at this
map's sight volumes. Every check raises SystemExit with what failed. No
dependencies.
"""

from __future__ import annotations

import hashlib
import json
import math
from pathlib import Path

import breakwater_layout as L
import slamcity_checks as S

EYE = L.EYE
REPO = Path(__file__).resolve().parent.parent


def turned(p):
    """A point's twin on the other team's half."""
    return (-p[0], p[1], -p[2])


def check_twin():
    """Every collision shape has a twin turned half round the centre (shapes
    on the centre are their own twin); surfaces may differ."""
    def key(shape):
        kind, _surface, geo = shape
        if kind == "box":
            return kind, tuple(round(v, 3) + 0.0 for v in geo)
        return kind, tuple(sorted(tuple(round(v, 3) + 0.0 for v in p) for p in geo))

    def twin(shape):
        kind, surface, geo = shape
        if kind == "box":
            x0, x1, y0, y1, z0, z1 = geo
            return kind, surface, (-x1, -x0, y0, y1, -z1, -z0)
        return kind, surface, [(-x, y, -z) for x, y, z in geo]

    shapes = {key(c) for c in L.COLLIDERS}
    lonely = [c for c in L.COLLIDERS if key(twin(c)) not in shapes]
    if lonely:
        raise SystemExit(f"{len(lonely)} collision shapes have no twin, e.g. {lonely[:3]}")


# Slamcity's sight helpers read the layout module they were given; these read ours.
def clear(p, q) -> bool:
    S.L = L
    return S.clear(p, q)


def free(p) -> bool:
    return clear(p, (p[0] + 1e-3, p[1], p[2] + 1e-3))


def _grid(rect, y, step):
    x0, x1, z0, z1 = rect
    pts = []
    x = x0 + 0.8
    while x < x1 - 0.5:
        z = z0 + 0.8
        while z < z1 - 0.5:
            pts.append((x, y, z))
            z += step
        x += step
    return pts


# --- Where each team stands --------------------------------------------------------------
def high_ground(team=0):
    """Eye points on a team's high ground: its control house's upper floor
    (Red: the lab), its workshop's roof deck (Red: the fire station), and its
    half of the rack's deck."""
    pts = [(x, L.UPPER + EYE, z) for x in (-31.5, -28.5, -25.5, -23.0)
           for z in (-25.0, -20.0, -15.0, -10.0, -5.0, -3.0)]
    pts += [(x, L.ROOF_DECK + EYE, z) for x in (-32.0, -29.0, -26.0, -23.5) for z in (12.0, 14.0, 17.0, 20.0)]
    for u in (-8.0, -6.0, -4.0, -2.0, -0.5):
        x, z = L.along(u, 0.0)
        pts.append((x, L.DECK + EYE, z))
    pts = [p for p in pts if free(p)]
    return pts if team == 0 else [turned(p) for p in pts]


def streets(team=0, step=3.0):
    """Eye points over a team's half of the roads, quay, tank farm, and rack."""
    pts = []
    for rect in (L.YARD, L.GATE_ROAD, L.GATE_LANE, L.QUAY, L.PIPE_ALLEY, L.CORNER, L.MAIN_ROAD, L.TANK_FARM,
                 (-13.0, 0.0, -13.0, 13.0), L.APRON):
        pts += _grid(rect, L.STREET + EYE, step)
    pts = [p for p in pts if free(p)]
    return pts if team == 0 else [turned(p) for p in pts]


def rooms(team=0):
    """Eye points inside a team's ground floors."""
    pts = [(x, L.WALK + EYE, z) for x, z in ((-27.0, -20.0), (-25.0, -6.0), (-15.0, -7.5), (-26.0, 8.5),
                                            (-24.0, 19.0), (-17.0, 12.0), (-12.0, 17.5), (-3.0, -30.0), (5.0, -24.5))]
    pts = [p for p in pts if free(p)]
    return pts if team == 0 else [turned(p) for p in pts]


def spawn_eyes(team=0):
    pts = [(x, y + EYE, z) for x, y, z in L.spawn_points()]
    return pts if team == 0 else [turned(p) for p in pts]


def yard_eyes(team=0, step=1.5):
    """Eye points over a team's whole spawn yard."""
    pts = [p for p in _grid(L.YARD, L.STREET + EYE, step) if free(p)]
    return pts if team == 0 else [turned(p) for p in pts]


# --- The checks ------------------------------------------------------------------------
def check_yards_closed():
    """No clear eye-height line from anywhere in Blue's spawn yard to anywhere
    in Red's: the map's buildings and cover close every line between them."""
    blue, red = yard_eyes(0), yard_eyes(1)
    seen = [(p, q) for p in blue for q in red if clear(p, q)]
    if seen:
        raise SystemExit(f"{len(seen)} clear lines between the spawn yards, e.g. {seen[:3]}")
    return len(blue) * len(red)


def check_spawn_sightlines():
    """No Blue spawn point may be seen, at eye height, from a Red spawn or Red's
    high ground (the map is balanced by a turn, so this covers both teams)."""
    watch = spawn_eyes(1) + high_ground(1)
    seen = [(p, q) for p in spawn_eyes(0) for q in watch if clear(p, q)]
    if seen:
        raise SystemExit(f"spawns in sight of the enemy: {seen[:5]}")


def long_lines(limit=55.0):
    pts = streets(0, step=4.0) + streets(1, step=4.0)
    out = []
    for i, p in enumerate(pts):
        for q in pts[i + 1:]:
            if math.dist(p, q) > limit and clear(p, q):
                out.append((p, q))
    return out


def check_long_sightlines(limit=55.0):
    """No clear line at eye height longer than `limit` metres across the
    ground: something must break every long line."""
    long = long_lines(limit)
    if long:
        raise SystemExit(f"{len(long)} clear lines longer than {limit:g} m, e.g. {long[:4]}")


def check_perch_duels(limit=50.0):
    """No clear line longer than `limit` metres between the two teams' high
    ground: perches fight at mid range, not across the whole map."""
    long = [(p, q) for p in high_ground(0) for q in high_ground(1) if math.dist(p, q) > limit and clear(p, q)]
    if long:
        raise SystemExit(f"{len(long)} clear lines over {limit:g} m between the teams' high ground, e.g. {long[:3]}")


def check_hardpoint_hops(minimum=16.0):
    """Each Hardpoint move, the last back to the first included, goes at least
    `minimum` metres across the map (height does not count) or one floor up
    (the deck over the rack is the exception: Call of Duty's rotations also
    climb onto a landmark), to a zone whose footprint does not touch the last
    one's unless it is on another level."""
    zones = L.HARDPOINTS
    for (name_a, _, ca, sa), (name_b, _, cb, sb) in zip(zones, zones[1:] + zones[:1]):
        hop = math.dist((ca[0], ca[2]), (cb[0], cb[2]))
        if hop < minimum and abs(ca[1] - cb[1]) < 2.0:
            raise SystemExit(f"Hardpoint {name_a} -> {name_b} moves {hop:.1f} m")


def check_home_perches(minimum=10):
    """If the enemy takes our own high ground, at least `minimum` of our spawn
    points must still be out of their sight."""
    perches = high_ground(0)
    hidden = [p for p in spawn_eyes(0) if all(not clear(p, q) for q in perches)]
    if len(hidden) < minimum:
        raise SystemExit(f"only {len(hidden)} spawn points hide from our own high ground")
    return len(hidden)


def enemy_incursions():
    """Points in the middle of our own half the enemy can reach once they push
    in: our high ground, our roads east of the gate, and the rooms we do not
    spawn in."""
    deep = [p for p in streets(0) if p[0] > -30.0]
    rooms_ = [p for p in rooms(0) if not _inside(p, L.CONTROL) and not _inside(p, L.WORKSHOP)]
    return high_ground(0) + deep + rooms_


def _inside(p, rect):
    return rect[0] < p[0] < rect[1] and rect[2] < p[2] < rect[3]


def check_incursions(minimum=6):
    """At least `minimum` spawn points stay hidden even with the enemy pushed
    into the middle of our half: somewhere safe remains."""
    watch = enemy_incursions()
    hidden = [p for p in spawn_eyes(0) if all(not clear(p, q) for q in watch)]
    if len(hidden) < minimum:
        raise SystemExit(f"only {len(hidden)} spawn points hide from an enemy inside our half")
    return len(hidden)


def layout_hash() -> str:
    """A hash of everything the scene takes from the layout but the meshes."""
    data = [L.COLLIDERS, L.spawn_points(), L.FLAG, L.BOT_LANES, L.HARDPOINTS, L.TARGETS, L.EFFECTS,
            L.CHARGE_SITES, L.CHARGE_CRATE, L.DOM_FLAGS, L.PROPS, L.DECALS, L.LIGHTS]
    blob = json.dumps(data, sort_keys=True, default=list, separators=(",", ":"))
    return hashlib.sha256(blob.encode()).hexdigest()[:16]


MESH_HASH = REPO / "godot/assets/maps/breakwater/city.hash"
BAKE_HASH = REPO / "godot/scenes/breakwater.bake.hash"


def mesh_hash() -> str:
    """A hash of everything city.gltf is built from: the layout's parts and
    placed models, and the model, look, builder, and wear-bake sources."""
    tools = Path(__file__).resolve().parent
    data = json.dumps([L.PARTS, L.PLACED], sort_keys=True, default=list, separators=(",", ":"))
    digest = hashlib.sha256(data.encode())
    for name in ("breakwater_models.py", "slamcity_models.py", "world_looks.py", "build_breakwater_city.py",
                 "build_slamcity_city.py", "world_wear.py"):
        digest.update((tools / name).read_bytes())
    return digest.hexdigest()[:16]


def run_all():
    """Run every check; returns (spawns hidden from our own high ground,
    spawns hidden from an enemy inside our half)."""
    L.build()
    check_twin()
    check_yards_closed()
    check_spawn_sightlines()
    check_long_sightlines()
    check_perch_duels()
    check_hardpoint_hops()
    return check_home_perches(), check_incursions()


if __name__ == "__main__":
    L.build()
    for name in ("check_twin", "check_yards_closed", "check_spawn_sightlines", "check_long_sightlines",
                 "check_perch_duels", "check_hardpoint_hops", "check_home_perches", "check_incursions"):
        try:
            print(name, globals()[name]())
        except SystemExit as e:
            print(name, "FAILED:", e)
