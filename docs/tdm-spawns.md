# Team Deathmatch spawns

The dedicated server chooses spawns for humans and bots. This is a Call of
Duty inspired system, not a reproduction of BO2's proprietary spawn algorithm.

At match start, each team is spread over its authored team-side markers.
On death (and when joining a match in progress), the server evaluates every
`SpawnPoints` child using the current living players and world collision raycasts.

Safety takes priority, in this order:

1. Unoccupied points with no enemy within 8 metres and no enemy sightline.
2. Unoccupied points with an enemy sightline, but no enemy within 8 metres.
3. Unoccupied points near an enemy.
4. Occupied points (a living player within 1.5 metres), as a last resort.

Within each tier, the score penalizes nearby enemies (within 25 metres),
enemy sightlines, and deaths within 8 metres in the last five seconds. Nearby
teammates and the team's original side receive small bonuses; the opposing side
receives a penalty. Seeded noise breaks ties. Side preference cannot override a
safer tier, so a team pushed out of its yard can respawn on the opposite side.
There is always a least-bad fallback when every point is dangerous.

Slamcity uses its scene's team markers. Nuketown uses 29 validated deathmatch
positions and facing directions derived from its imported `map.json`. The grounding tool projects them onto collision
and excludes three obstructed points. Rebuild `data/nuketown_spawns.json` with
`godot --headless --path godot --script res://tools/configure_nuketown_spawns.gd`
after changing map collision. Its six outermost ground-level points at either end of the X axis form the opposing starting
lineups; the remaining points are neutral respawn candidates, including the
upstairs points. Preparation happens before the dedicated server reads the map
and is safe to call repeatedly. Warehouse authors its markers in the scene: six
starts on the grass in the Field for team 0 and six starts on the lowered
concrete Docks for team 1. Twenty-eight neutral points cover both yards and
aisles on the production and high-bay sides. The markers face the hall's middle. All three maps appear in the
local TDM setup menu.

The sightline test checks geometry, regardless of which way an enemy is facing.
Safety is evaluated at the respawn tick; it cannot guarantee safety against an
enemy moving into view afterward. Offline exploration still respawns at its
original player start; dynamic selection belongs to server-run TDM.

Regression coverage is in `rust/sim/src/tdm.rs` (selection and flips),
`godot/tests/nuketown_test.gd` and `godot/tests/warehouse_test.gd` (server
preparation, floor and body clearance),
and `godot/tests/title_screen_test.gd` (menu selection).

Small vertical edges are handled in the shared body controller: a blocked grounded
move tries a capsule sweep up at most 30 cm, forward by one tick's requested
travel, and down onto a walkable surface. All three sweeps must clear collision,
so tall walls and low ceilings block the attempt. Airborne jumps do not step up.
The local camera eases the discrete rise over several ticks while the physical
body and authoritative server agree on the collision position. Nuketown physics
fixtures check thresholds at 5, 15, and 28 cm, camera motion, walls, and ceilings.
