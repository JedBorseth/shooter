# Spawns

The dedicated server picks every spawn, for players and bots alike
(`tdm::start_spawn`, `tdm::choose_spawn` in `rust/sim/src/tdm.rs`;
`NetServer::respawn` and `safest_spawn` in `rust/game/src/net/server.rs`).
It is a Call of Duty inspired system, not a reproduction of Black Ops 2's
spawn algorithm, which Treyarch never published.

## Start spawns

At the start of a match, a Search and Destroy round, and a half, everyone
comes back at their side's start points in join order, spread over the
points marked for their team (`start_spawn`). The start also clears
grenades, smokes, Scavenger packs, and the record of recent deaths. In a
free-for-all every point is anyone's, so each player gets the safest one
instead.

## Respawns

After a death, and when joining a match in progress, the server scores every
spawn point against the living players, using raycasts from each enemy's
eyes to a point 1.5 m above the spawn. Safety comes first, in tiers:

1. Unoccupied points with no enemy within 8 m and no enemy sightline.
2. Unoccupied points an enemy can see, but with no enemy within 8 m.
3. Unoccupied points with an enemy within 8 m.
4. Occupied points (a living player within 1.5 m), as a last resort.

Within a tier, the best score wins:

| Factor | Score |
| --- | --- |
| Each enemy within 25 m | minus (25 − distance) |
| Each enemy with a sightline | −60 |
| Each death within 8 m in the last 5 s | −25 |
| Each teammate within 25 m | +4 |
| A point on the team's own side | +10 |
| A point on the enemy's side | −30 |
| Seeded noise, to spread players out | 0 to +2 |

Side preference never overrides a safer tier, so a team pushed out of its
yard can respawn on the far side. There is always a least bad point when
every point is dangerous. The sightline test checks geometry, not which way
an enemy faces, and safety is judged at the respawn tick: an enemy can still
walk into view afterwards.

## Per mode

| Mode | Change (`SpawnRules::for_mode` and `safest_spawn`) |
| --- | --- |
| Capture the Flag | Never on the enemy's side while any other point exists: each team stays at its own base |
| Hardpoint | The live zone counts as two enemies at its centre, so nobody spawns on the hill |
| Domination | Each flag a team owns counts as three teammates at its circle and each enemy flag as one enemy; the side bonus falls to +4 and the enemy-side penalty to −8, so spawns follow the flags and flip when a team loses its home flag ([domination.md](domination.md)) |
| Free-for-all, Gun Game | No sides: every point is anyone's, and every other player is an enemy |
| Search and Destroy | Start spawns each round; nobody respawns mid-round |

At halftime (Capture the Flag, Domination) and Search and Destroy's side
swap, the teams take each other's start points.

## Maps

A map lists its points as children of a `SpawnPoints` node, each facing the
way players should, with `team` metadata 0 or 1 for a side's start point
(none or -1 for a neutral point either team may use). Respawns use every
point, starts only the team's own.

| Map | Points |
| --- | --- |
| Breakwater | 40: 20 on each side, in the scene |
| Warehouse | 40: six starts on the grass of the Field for team 0, six on the lowered concrete Docks for team 1, and 28 neutral points over both yards and the production and high-bay aisles, facing open aisles |
| Testing Area | None: six per team in two rows 14 m apart through the player start (`fallback_spawns`) |

## Tests

- `rust/sim/src/tdm.rs`: safety tiers, side flips, own-side-only spawns,
  start spreading, and maps where every point is dangerous.
- `godot/tests/warehouse_test.gd`: server preparation, floor, and body
  clearance at every point.
