# Domination

Two teams fight over three flags, A, B, and C (`--mode dom`,
`rust/sim/src/domination.rs`). The reference game is **Black Ops 2**
(Treyarch, in line with the project's graphics target), played the classic
way with no twists.

## Rules

| Rule | Here | Call of Duty |
| --- | --- | --- |
| Flags | Three: A by team 0's spawn, B in the middle, C by team 1's | Same in every game since Call of Duty 4 |
| Capture | Stand in a flag's circle for 10 s (`DominationRules::capture_seconds`) | 10 s in Call of Duty 4 and Black Ops 2 (`dom.gsc`) |
| More capturers | Each extra teammate adds one person's speed: 2 take 5 s, 3 take 3.3 s | The scripts' use rate is the number of claimants (CoD4 and Black Ops 2 `gameobjects.gsc`); later games cap the speed-up |
| Enemy flags | Take as long as neutral ones, in one step | Same in Black Ops 2; Modern Warfare (2019) onward first neutralises an enemy flag, which this project does not |
| Contested | Both teams in the circle: the capture stalls and keeps its progress | Same ("progress is stalled until only one team is left on the flag", Modern Warfare beta notes, 2019) |
| Leaving | The capturing team walking out, or the owners stepping on, starts the capture over | Same in Black Ops 2; Black Ops 3 onward lets progress decay slowly |
| Team score | Every 5 s, a point per owned flag (`point_seconds`) | Same: one point per flag every five seconds |
| Score limit | 200 | 200 in Black Ops 2 and Modern Warfare (2019) |
| Time limit | 10 minutes in two halves | Black Ops 2 plays two rounds of 5 minutes |
| Halftime | At half the clock the teams swap sides, every flag goes back to neutral, and everyone respawns at their new side; the score carries over (`halftime_due`) | Same: Black Ops 2's second round starts from neutral flags with the sides swapped |
| Draw | The team ahead when the clock runs out wins; level is a draw | Same in Black Ops 2 (overtime came later) |

Flags are circles 3.5 m in radius and 4 m tall
(`DEFAULT_RADIUS`; Black Ops 2's flag triggers are about 120 to 160 units).
Feet up to 0.5 m below the floor still count, for uneven ground.

Kills score nothing for the team.

## Player score

| Award | Points | When |
| --- | --- | --- |
| Flag Captured (medal) | 150 | Standing in the circle when the team takes the flag; every capturer gets it |
| Flag Defense | +50 on the kill | A kill with the killer or the victim on or within 5 m of a flag the killer's team owns |
| Flag Assault | +50 on the kill | The same near a flag the killer's team does not own (neutral or enemy) |

Captures and defences count on the scoreboard (`ScoreState::captures`,
`ScoreState::defends`). Black Ops 2's exact score values could not be
confirmed from a source; these follow this project's Hardpoint and Capture
the Flag scale (a kill is 100).

## Spawns

After the first spawn at each side, spawns follow the flags
(`DominationState::spawn_threats`): each flag a team owns counts as three
teammates at its circle, each enemy flag as an enemy there, and the side of
the map only breaks ties (`SpawnRules::for_mode`). A team that takes the
enemy's home flag and loses its own flips the spawns, as in Call of Duty.

## Network

The server decides every capture. It sends the map's flags on joining
(`GameEvent::DominationFlags`) and every flag's owner, claim, progress,
contest, and capturer count whenever any of those but the progress changes
(`GameEvent::Domination`). Clients run a capture on at its known speed
between events (`NetClient::domination_info`).

## Maps

A map marks its flags with `Node3D` children of a `DominationFlags` node,
each placed at the circle's floor centre, with a `label` metadata (A, B, C)
and an optional `radius` metadata. Maps without them get three on the line
between the teams' spawns, A at 22%, B at 50%, C at 78%
(`domination::fallback_points`).

## Still to come

The flag markers and HUD, the calls, a title screen choice, objective bots,
and flags placed by hand on Slamcity, Nuketown, and Warehouse: see the
Domination items in [TODO.md](../TODO.md).
